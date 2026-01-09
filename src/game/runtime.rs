//! Test Runtime
//!
//! The test tool state that renders and simulates the game.
//! Reads level data from ProjectData for rendering, uses ECS World for entities.
//! Player settings are stored in Level.player_settings and edited in the World Editor.

use crate::rasterizer::{Camera, Vec3, RasterSettings, Texture15};
use crate::world::Level;
use super::{World, Events, Entity};

/// Frame timing data for performance profiling
#[derive(Debug, Clone, Default)]
pub struct FrameTimings {
    /// Input handling time (ms)
    pub input_ms: f32,
    /// Physics/game logic time (ms)
    pub logic_ms: f32,
    /// Framebuffer clear time (ms)
    pub clear_ms: f32,
    /// Mesh rendering time (ms) - total
    pub render_ms: f32,
    /// UI/overlay drawing time (ms)
    pub ui_ms: f32,
    /// Total frame time (ms)
    pub total_ms: f32,

    // === Render sub-timings ===
    /// Light collection time (ms)
    pub render_lights_ms: f32,
    /// Texture conversion time (ms) - RGB888 to RGB555
    pub render_texconv_ms: f32,
    /// Mesh data generation time (ms) - to_render_data_with_textures
    pub render_meshgen_ms: f32,
    /// Actual rasterization time (ms) - render_mesh calls
    pub render_raster_ms: f32,
    /// Framebuffer to texture upload time (ms)
    pub render_upload_ms: f32,

    // === Raster sub-timings (breakdown of render_raster_ms) ===
    /// Vertex transform and projection time (ms)
    pub raster_transform_ms: f32,
    /// PS1-style fog/depth cueing time (ms)
    pub raster_fog_ms: f32,
    /// Surface building and backface culling time (ms)
    pub raster_cull_ms: f32,
    /// Depth sorting time (ms) - painter's algorithm
    pub raster_sort_ms: f32,
    /// Triangle fill/drawing time (ms)
    pub raster_draw_ms: f32,
    /// Wireframe rendering time (ms)
    pub raster_wireframe_ms: f32,
}

impl FrameTimings {
    /// Start timing a phase (returns time in seconds from macroquad)
    pub fn start() -> f64 {
        macroquad::prelude::get_time()
    }

    /// Get elapsed time in ms since start
    pub fn elapsed_ms(start: f64) -> f32 {
        ((macroquad::prelude::get_time() - start) * 1000.0) as f32
    }
}

/// Camera control mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CameraMode {
    /// Third-person camera following player (Elden Ring style)
    #[default]
    Character,
    /// Free-flying spectator camera (noclip)
    FreeFly,
}

/// FPS limit setting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FpsLimit {
    /// 30 FPS (authentic PS1 for many games)
    Fps30,
    /// 60 FPS (smooth gameplay)
    #[default]
    Fps60,
    /// Unlocked (as fast as possible)
    Unlocked,
}

impl FpsLimit {
    /// Get the target frame time in seconds (None = unlocked)
    pub fn frame_time(&self) -> Option<f64> {
        match self {
            FpsLimit::Fps30 => Some(1.0 / 30.0),
            FpsLimit::Fps60 => Some(1.0 / 60.0),
            FpsLimit::Unlocked => None,
        }
    }

    /// Cycle to next value
    pub fn next(self) -> Self {
        match self {
            FpsLimit::Fps30 => FpsLimit::Fps60,
            FpsLimit::Fps60 => FpsLimit::Unlocked,
            FpsLimit::Unlocked => FpsLimit::Fps30,
        }
    }

    /// Cycle to previous value
    pub fn prev(self) -> Self {
        match self {
            FpsLimit::Fps30 => FpsLimit::Unlocked,
            FpsLimit::Fps60 => FpsLimit::Fps30,
            FpsLimit::Unlocked => FpsLimit::Fps60,
        }
    }

    /// Display name
    pub fn label(&self) -> &'static str {
        match self {
            FpsLimit::Fps30 => "30",
            FpsLimit::Fps60 => "60",
            FpsLimit::Unlocked => "Unlocked",
        }
    }
}

/// State for the Test tool (play mode)
pub struct GameToolState {
    /// ECS world containing all dynamic entities
    pub world: World,

    /// Event queues for game systems
    pub events: Events,

    /// Game camera (separate from editor camera)
    pub camera: Camera,

    /// Orbit camera parameters
    pub orbit_target: Vec3,
    pub orbit_distance: f32,
    pub orbit_azimuth: f32,
    pub orbit_elevation: f32,

    /// Rasterizer settings (game mode: no editor debug features)
    pub raster_settings: RasterSettings,

    /// Is the game currently playing? (vs paused)
    pub playing: bool,

    /// The player entity (if spawned)
    pub player_entity: Option<Entity>,

    /// Viewport mouse state
    pub viewport_last_mouse: (f32, f32),
    pub viewport_mouse_captured: bool,

    /// Has the camera been initialized from the level?
    pub camera_initialized: bool,

    /// Camera control mode (character follow vs free-fly)
    pub camera_mode: CameraMode,

    /// Is the options menu currently open?
    pub options_menu_open: bool,

    /// Debug menu selected item index
    pub debug_menu_selection: usize,

    /// Show debug overlay (top-right HUD with player stats)
    pub show_debug_overlay: bool,

    /// Free-fly camera parameters (when in FreeFly mode)
    pub freefly_yaw: f32,
    pub freefly_pitch: f32,

    /// Character mode: camera orbit yaw (azimuth around player)
    pub char_cam_yaw: f32,
    /// Character mode: camera orbit pitch (elevation)
    pub char_cam_pitch: f32,

    /// FPS limit setting (30/60/Unlocked)
    pub fps_limit: FpsLimit,

    /// Frame timing data for performance profiling
    pub frame_timings: FrameTimings,

    /// Cached RGB555 textures (lazy-populated, invalidated when texture count changes)
    pub textures_15_cache: Vec<Texture15>,
}

impl GameToolState {
    pub fn new() -> Self {
        // Set up orbit camera
        let orbit_target = Vec3::new(512.0, 256.0, 512.0);
        let orbit_distance = 3000.0;
        let orbit_azimuth = 0.8;
        let orbit_elevation = 0.3;

        let mut camera = Camera::new();
        Self::update_camera_from_orbit(
            &mut camera,
            orbit_target,
            orbit_distance,
            orbit_azimuth,
            orbit_elevation,
        );

        Self {
            world: World::new(),
            events: Events::new(),
            camera,
            orbit_target,
            orbit_distance,
            orbit_azimuth,
            orbit_elevation,
            raster_settings: RasterSettings::game(), // No editor debug features
            playing: false,
            player_entity: None,
            viewport_last_mouse: (0.0, 0.0),
            viewport_mouse_captured: false,
            camera_initialized: false,
            camera_mode: CameraMode::default(),
            options_menu_open: false,
            debug_menu_selection: 0,
            show_debug_overlay: false,
            freefly_yaw: 0.0,
            freefly_pitch: 0.0,
            char_cam_yaw: 0.0,
            char_cam_pitch: 0.2, // Slight downward pitch by default
            fps_limit: FpsLimit::default(),
            frame_timings: FrameTimings::default(),
            textures_15_cache: Vec::new(),
        }
    }

    /// Initialize camera from level's player start (if any)
    /// Call this once when entering the game tab or when level changes
    pub fn init_from_level(&mut self, level: &Level) {
        if self.camera_initialized {
            return;
        }

        // Try to get player start position from tile-based objects
        if let Some((room_idx, spawn)) = level.get_player_start() {
            // Get the room to calculate world position
            if let Some(room) = level.rooms.get(room_idx) {
                let spawn_pos = spawn.world_position(room);
                // Position camera to look at spawn point from above and behind
                self.orbit_target = spawn_pos + Vec3::new(0.0, 200.0, 0.0); // Slightly above ground
                self.orbit_distance = 1500.0; // Closer than default
                self.orbit_azimuth = spawn.facing + std::f32::consts::PI; // Behind the spawn facing
                self.orbit_elevation = 0.4;
                self.sync_camera_from_orbit();
            }
        } else if !level.rooms.is_empty() {
            // Fall back to room center if no spawn point
            let room = &level.rooms[0];
            let center = room.bounds.center();
            self.orbit_target = Vec3::new(
                room.position.x + center.x,
                room.position.y + center.y + 200.0,
                room.position.z + center.z,
            );
            self.orbit_distance = 2000.0;
            self.sync_camera_from_orbit();
        }

        self.camera_initialized = true;
    }

    /// Reset camera initialization (call when level changes)
    pub fn reset_camera(&mut self) {
        self.camera_initialized = false;
    }

    /// Update camera position from orbit parameters
    fn update_camera_from_orbit(
        camera: &mut Camera,
        target: Vec3,
        distance: f32,
        azimuth: f32,
        elevation: f32,
    ) {
        let pitch = elevation;
        let yaw = azimuth;

        // Forward direction (what camera looks at)
        let forward = Vec3::new(
            pitch.cos() * yaw.sin(),
            -pitch.sin(),
            pitch.cos() * yaw.cos(),
        );

        // Camera sits behind the target along the forward direction
        camera.position = target - forward * distance;
        camera.rotation_x = pitch;
        camera.rotation_y = yaw;
        camera.update_basis();
    }

    /// Sync camera from current orbit parameters
    pub fn sync_camera_from_orbit(&mut self) {
        Self::update_camera_from_orbit(
            &mut self.camera,
            self.orbit_target,
            self.orbit_distance,
            self.orbit_azimuth,
            self.orbit_elevation,
        );
    }

    /// Update camera to follow player in Dark Souls-style orbit view.
    /// Camera orbits around player independently of player facing.
    /// Returns the player position if player exists.
    pub fn update_camera_follow_player(&mut self, level: &Level) -> Option<Vec3> {
        let player = self.player_entity?;
        let transform = self.world.transforms.get(player)?;
        let player_pos = transform.position;

        // Get camera settings from level
        let settings = &level.player_settings;

        // Target point: player position + vertical offset (shoulder/chest height)
        let look_at = player_pos + Vec3::new(0.0, settings.camera_vertical_offset, 0.0);

        // Calculate camera position using spherical coordinates around player
        // yaw = horizontal rotation, pitch = vertical angle
        let yaw = self.char_cam_yaw;
        let pitch = self.char_cam_pitch;

        // Spherical to cartesian: camera position relative to target
        // Pitch: 0 = level, positive = looking down (camera above), negative = looking up (camera below)
        let horizontal_dist = settings.camera_distance * pitch.cos();
        let vertical_offset = settings.camera_distance * pitch.sin();

        let cam_offset = Vec3::new(
            -yaw.sin() * horizontal_dist,
            vertical_offset,
            -yaw.cos() * horizontal_dist,
        );

        self.camera.position = look_at + cam_offset;

        // Point camera at target
        let to_target = (look_at - self.camera.position).normalize();
        self.camera.rotation_y = to_target.x.atan2(to_target.z);
        self.camera.rotation_x = (-to_target.y).asin();
        self.camera.update_basis();

        Some(player_pos)
    }

    /// Get the camera forward direction projected onto XZ plane (for movement)
    pub fn get_camera_forward_xz(&self) -> Vec3 {
        let yaw = self.char_cam_yaw;
        Vec3::new(yaw.sin(), 0.0, yaw.cos()).normalize()
    }

    /// Get the camera right direction on XZ plane (for strafing)
    pub fn get_camera_right_xz(&self) -> Vec3 {
        let yaw = self.char_cam_yaw;
        Vec3::new(yaw.cos(), 0.0, -yaw.sin()).normalize()
    }

    /// Get player position if playing and player exists
    pub fn get_player_position(&self) -> Option<Vec3> {
        let player = self.player_entity?;
        self.world.transforms.get(player).map(|t| t.position)
    }

    /// Toggle play/pause state
    pub fn toggle_playing(&mut self) {
        self.playing = !self.playing;
        if !self.playing {
            // Reset ECS world when stopping
            self.world = World::new();
            self.events = Events::new();
            self.player_entity = None;
        }
    }

    /// Reset the game state (clear entities, respawn player)
    pub fn reset(&mut self) {
        self.world = World::new();
        self.events = Events::new();
        self.player_entity = None;
        self.playing = false;
    }

    /// Full reset for loading a new level (resets entities, camera, and texture cache)
    pub fn reset_for_new_level(&mut self) {
        self.reset();
        self.reset_camera();
        self.textures_15_cache.clear();
    }

    /// Spawn the player entity at a position using level settings
    pub fn spawn_player(&mut self, position: Vec3, level: &Level) {
        let player = self.world.spawn_player(position, 100, &level.player_settings);
        self.player_entity = Some(player);
    }

    /// Run one frame of game simulation
    pub fn tick(&mut self, level: &Level, delta_time: f32) {
        if !self.playing {
            return;
        }

        // =====================================================================
        // Character Controller System: Apply gravity and collision
        // =====================================================================
        // Collect entities with controllers to avoid borrow issues
        let controller_entities: Vec<(u32, super::components::CharacterController)> = self.world.controllers
            .iter()
            .map(|(idx, ctrl)| (idx, *ctrl))
            .collect();

        for (idx, mut controller) in controller_entities {
            let entity = Entity::new(idx, 0);

            // Get current position and velocity
            let position = self.world.transforms.get(entity)
                .map(|t| t.position)
                .unwrap_or(Vec3::ZERO);
            let velocity = self.world.velocities.get(entity)
                .map(|v| v.0)
                .unwrap_or(Vec3::ZERO);

            // Apply collision and movement
            let new_pos = super::collision::move_and_slide(
                level,
                position,
                velocity,
                &mut controller,
                delta_time,
            );

            // Update transform
            if let Some(transform) = self.world.transforms.get_mut(entity) {
                transform.position = new_pos;
            }

            // Update controller state
            self.world.controllers.insert(entity, controller);
        }

        // =====================================================================
        // Simple Movement System: Apply velocity (for entities without controllers)
        // =====================================================================
        for (idx, velocity) in self.world.velocities.iter() {
            let entity = Entity::new(idx, 0);
            // Skip entities with controllers (already handled above)
            if self.world.controllers.contains(entity) {
                continue;
            }
            if let Some(transform) = self.world.transforms.get_mut(entity) {
                transform.position = transform.position + velocity.0 * delta_time;
            }
        }

        // =====================================================================
        // Update global transforms (for rendering)
        // =====================================================================
        for (idx, transform) in self.world.transforms.iter() {
            let entity = Entity::new(idx, 0);
            let global = super::GlobalTransform::from_transform(transform);
            self.world.global_transforms.insert(entity, global);
        }

        // =====================================================================
        // Health System: Tick invincibility frames
        // =====================================================================
        for (_idx, health) in self.world.health.iter_mut() {
            health.tick_invincibility();
        }

        // Process pending despawns
        self.world.flush_despawns();

        // Clear events for next frame
        self.events.clear_all();
    }
}

impl Default for GameToolState {
    fn default() -> Self {
        Self::new()
    }
}
