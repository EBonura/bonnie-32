//! Game Runtime
//!
//! The game tool state that renders and simulates the game.
//! Reads level data from ProjectData for rendering, uses ECS World for entities.

use crate::rasterizer::{Camera, Vec3, RasterSettings};
use crate::world::Level;
use super::{World, Events, Entity};

/// State for the Game tool (play mode)
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

    /// Rasterizer settings
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
            raster_settings: RasterSettings::default(),
            playing: false,
            player_entity: None,
            viewport_last_mouse: (0.0, 0.0),
            viewport_mouse_captured: false,
            camera_initialized: false,
        }
    }

    /// Initialize camera from level's player start (if any)
    /// Call this once when entering the game tab or when level changes
    pub fn init_from_level(&mut self, level: &Level) {
        if self.camera_initialized {
            return;
        }

        // Try to get player start position from tile-based objects
        if let Some(spawn) = level.get_player_start() {
            // Get the room to calculate world position
            if let Some(room) = level.rooms.get(spawn.room) {
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

    /// Toggle play/pause state
    pub fn toggle_playing(&mut self) {
        self.playing = !self.playing;
    }

    /// Reset the game state (clear entities, respawn player)
    pub fn reset(&mut self) {
        self.world = World::new();
        self.events = Events::new();
        self.player_entity = None;
        self.playing = false;
    }

    /// Spawn the player entity at a position
    pub fn spawn_player(&mut self, position: Vec3) {
        let player = self.world.spawn_player(position, 100);
        self.player_entity = Some(player);
    }

    /// Run one frame of game simulation
    pub fn tick(&mut self, _delta_time: f32) {
        if !self.playing {
            return;
        }

        // TODO: Run game systems
        // - Physics/movement
        // - Collision detection
        // - AI
        // - Combat
        // - etc.

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
