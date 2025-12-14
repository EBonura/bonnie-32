//! Editor state and data

use std::path::PathBuf;
use crate::world::Level;
use crate::rasterizer::{Camera, Vec3, Texture, RasterSettings};
use super::texture_pack::TexturePack;

/// TRLE grid constraints
/// Sector size in world units (X-Z plane)
pub const SECTOR_SIZE: f32 = 1024.0;
/// Height subdivision ("click") in world units (Y axis)
pub const CLICK_HEIGHT: f32 = 256.0;
/// Default ceiling height (2x sector size)
pub const CEILING_HEIGHT: f32 = 2048.0;

/// Camera mode for 3D viewport
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraMode {
    Free,   // WASD + mouse look (FPS style)
    Orbit,  // Rotate around target point
}

/// Current editor tool
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EditorTool {
    Select,
    DrawFloor,
    DrawWall,
    DrawCeiling,
    PlacePortal,
    PlaceObject,
}

/// Which face within a sector is selected
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectorFace {
    Floor,
    Ceiling,
    WallNorth(usize),  // Index into walls array
    WallEast(usize),
    WallSouth(usize),
    WallWest(usize),
}

/// What is currently selected in the editor
#[derive(Debug, Clone, PartialEq)]
pub enum Selection {
    None,
    Room(usize),
    /// Entire sector selected (all faces)
    Sector { room: usize, x: usize, z: usize },
    /// Specific face within a sector
    SectorFace { room: usize, x: usize, z: usize, face: SectorFace },
    /// Edge of a face (two vertices)
    /// face_idx: 0=floor, 1=ceiling, 2=wall
    /// edge_idx: 0-3 for floor/ceiling (north, east, south, west)
    ///           0-3 for wall (bottom, right, top, left)
    /// wall_face: Some(SectorFace::WallXxx) when face_idx=2
    Edge { room: usize, x: usize, z: usize, face_idx: usize, edge_idx: usize, wall_face: Option<SectorFace> },
    Portal { room: usize, portal: usize },
}

impl Selection {
    /// Check if this selection includes a specific sector (either whole sector or face within it)
    pub fn includes_sector(&self, room_idx: usize, sx: usize, sz: usize) -> bool {
        match self {
            Selection::Sector { room, x, z } => *room == room_idx && *x == sx && *z == sz,
            Selection::SectorFace { room, x, z, .. } => *room == room_idx && *x == sx && *z == sz,
            _ => false,
        }
    }

    /// Get the sector coordinates if this is a sector or sector-face selection
    pub fn sector_coords(&self) -> Option<(usize, usize, usize)> {
        match self {
            Selection::Sector { room, x, z } => Some((*room, *x, *z)),
            Selection::SectorFace { room, x, z, .. } => Some((*room, *x, *z)),
            _ => None,
        }
    }

    /// Check if this selection includes a specific face
    pub fn includes_face(&self, room_idx: usize, sx: usize, sz: usize, face: SectorFace) -> bool {
        match self {
            // Whole sector selection includes all faces
            Selection::Sector { room, x, z } => *room == room_idx && *x == sx && *z == sz,
            // Face selection only matches exact face
            Selection::SectorFace { room, x, z, face: f } => {
                *room == room_idx && *x == sx && *z == sz && *f == face
            }
            _ => false,
        }
    }
}

/// Editor state
pub struct EditorState {
    /// The level being edited
    pub level: Level,

    /// Current file path (None = unsaved new file)
    pub current_file: Option<PathBuf>,

    /// Current tool
    pub tool: EditorTool,

    /// Current selection
    pub selection: Selection,

    /// Multi-selection (for selecting multiple faces/vertices/edges)
    pub multi_selection: Vec<Selection>,

    /// Selection rectangle state (for drag-to-select)
    pub selection_rect_start: Option<(f32, f32)>, // Start position in viewport coords
    pub selection_rect_end: Option<(f32, f32)>,   // End position in viewport coords

    /// Currently selected room index (for editing)
    pub current_room: usize,

    /// Selected texture reference (pack + name)
    pub selected_texture: crate::world::TextureRef,

    /// 3D viewport camera
    pub camera_3d: Camera,

    /// Camera mode (free or orbit)
    pub camera_mode: CameraMode,

    /// Orbit camera state
    pub orbit_target: Vec3,      // Point the camera orbits around
    pub orbit_distance: f32,     // Distance from target
    pub orbit_azimuth: f32,      // Horizontal angle (radians)
    pub orbit_elevation: f32,    // Vertical angle (radians)
    pub last_orbit_target: Vec3, // Last orbit target (for when nothing is selected)

    /// 2D grid view camera (pan and zoom)
    pub grid_offset_x: f32,
    pub grid_offset_y: f32,
    pub grid_zoom: f32,

    /// Grid settings
    pub grid_size: f32, // World units per grid cell
    pub show_grid: bool,

    /// 3D viewport settings
    pub show_room_bounds: bool, // Show room boundary wireframes

    /// Vertex editing mode
    pub link_coincident_vertices: bool, // When true, moving a vertex moves all vertices at same position

    /// Undo/redo (simple version - just level snapshots)
    pub undo_stack: Vec<Level>,
    pub redo_stack: Vec<Level>,

    /// Dirty flag (unsaved changes)
    pub dirty: bool,

    /// Status message (shown in status bar)
    pub status_message: Option<(String, f64)>, // (message, expiry_time)

    /// 3D viewport mouse state (for camera control)
    pub viewport_last_mouse: (f32, f32),
    pub viewport_mouse_captured: bool,

    /// 2D grid view mouse state
    pub grid_last_mouse: (f32, f32),
    pub grid_panning: bool,
    pub grid_dragging_vertex: Option<usize>, // Primary dragged vertex (for backward compat)
    pub grid_dragging_vertices: Vec<usize>,   // All vertices being dragged (for linking)
    pub grid_drag_started: bool, // True if we've started dragging (for undo)

    /// 3D viewport vertex dragging state (legacy - kept for compatibility)
    pub viewport_dragging_vertices: Vec<(usize, usize)>, // List of (room_idx, vertex_idx)
    pub viewport_drag_started: bool,
    pub viewport_drag_plane_y: f32, // Y height of the drag plane (reference point for delta)
    pub viewport_drag_initial_y: Vec<f32>, // Initial Y positions of each dragged vertex

    /// 3D viewport sector-based vertex dragging
    /// Each entry is (room_idx, gx, gz, face_type, corner_idx)
    /// corner_idx: 0=NW, 1=NE, 2=SE, 3=SW for horizontal faces
    /// For walls: 0=bottom-left, 1=bottom-right, 2=top-right, 3=top-left
    pub dragging_sector_vertices: Vec<(usize, usize, usize, SectorFace, usize)>,
    pub drag_initial_heights: Vec<f32>, // Initial Y/height values for each vertex

    /// Texture palette state
    pub texture_packs: Vec<TexturePack>,
    pub selected_pack: usize,
    pub texture_scroll: f32,

    /// Properties panel scroll offset
    pub properties_scroll: f32,

    /// UV editing drag state (for drag-value widgets)
    pub uv_drag_active: [bool; 5],      // [x_offset, y_offset, x_scale, y_scale, angle]
    pub uv_drag_start_value: [f32; 5],
    pub uv_drag_start_x: [f32; 5],

    /// UV editing link state (when true, dragging X also changes Y)
    pub uv_offset_linked: bool,
    pub uv_scale_linked: bool,

    /// UV editing text input state (for double-click manual entry)
    pub uv_editing_field: Option<usize>,  // Which field is being edited (0-4 = offset x/y, scale x/y, angle)
    pub uv_edit_buffer: String,           // Text input buffer

    /// Placement height adjustment (for DrawFloor/DrawCeiling/DrawWall modes)
    pub placement_target_y: f32,           // Current Y height for new placements
    pub height_adjust_mode: bool,          // True when Shift is held for height adjustment
    pub height_adjust_start_mouse_y: f32,  // Mouse Y when height adjust started
    pub height_adjust_start_y: f32,        // placement_target_y when height adjust started
    pub height_adjust_locked_pos: Option<(f32, f32)>, // Locked (x, z) position when adjusting

    /// Rasterizer settings (PS1 effects)
    pub raster_settings: RasterSettings,

    /// Selected vertex indices for color editing (0-3 for face corners)
    pub selected_vertex_indices: Vec<usize>,
}

impl EditorState {
    pub fn new(level: Level) -> Self {
        let mut camera_3d = Camera::new();
        // Position camera far away from origin to get good view of sector
        // Single 1024×1024 sector is at origin (0,0,0) to (1024,0,1024)
        camera_3d.position = Vec3::new(4096.0, 4096.0, 4096.0);
        // Set initial rotation for good viewing angle
        camera_3d.rotation_x = 0.46;
        camera_3d.rotation_y = 4.02;
        camera_3d.update_basis();

        // Discover all texture packs
        let texture_packs = TexturePack::discover_all();
        println!("Discovered {} texture packs", texture_packs.len());
        for pack in &texture_packs {
            println!("  - {} ({} textures)", pack.name, pack.textures.len());
        }

        // Auto-select first texture from first pack (if available)
        let selected_texture = texture_packs.first()
            .and_then(|pack| pack.textures.first().map(|tex| {
                crate::world::TextureRef::new(&pack.name, &tex.name)
            }))
            .unwrap_or_else(crate::world::TextureRef::none);

        // Default orbit target at center of first sector
        let orbit_target = Vec3::new(512.0, 512.0, 512.0);
        let orbit_distance = 4000.0;
        let orbit_azimuth = 0.8;     // ~45 degrees
        let orbit_elevation = 0.4;   // ~23 degrees up

        Self {
            level,
            current_file: None,
            tool: EditorTool::Select,
            selection: Selection::None,
            multi_selection: Vec::new(),
            selection_rect_start: None,
            selection_rect_end: None,
            current_room: 0,
            selected_texture,
            camera_3d,
            camera_mode: CameraMode::Free,
            orbit_target,
            orbit_distance,
            orbit_azimuth,
            orbit_elevation,
            last_orbit_target: orbit_target,
            grid_offset_x: 0.0,
            grid_offset_y: 0.0,
            grid_zoom: 0.1, // Pixels per world unit (very zoomed out for TRLE 1024-unit sectors)
            grid_size: SECTOR_SIZE, // TRLE sector size
            show_grid: true,
            show_room_bounds: true, // Show room boundaries by default
            link_coincident_vertices: true, // Default to linked mode
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            dirty: false,
            status_message: None,
            viewport_last_mouse: (0.0, 0.0),
            viewport_mouse_captured: false,
            grid_last_mouse: (0.0, 0.0),
            grid_panning: false,
            grid_dragging_vertex: None,
            grid_dragging_vertices: Vec::new(),
            grid_drag_started: false,
            viewport_dragging_vertices: Vec::new(),
            viewport_drag_started: false,
            viewport_drag_plane_y: 0.0,
            viewport_drag_initial_y: Vec::new(),
            dragging_sector_vertices: Vec::new(),
            drag_initial_heights: Vec::new(),
            texture_packs,
            selected_pack: 0,
            texture_scroll: 0.0,
            properties_scroll: 0.0,
            uv_drag_active: [false; 5],
            uv_drag_start_value: [0.0; 5],
            uv_drag_start_x: [0.0; 5],
            uv_offset_linked: true,  // Default to linked
            uv_scale_linked: true,   // Default to linked
            uv_editing_field: None,
            uv_edit_buffer: String::new(),
            placement_target_y: 0.0,
            height_adjust_mode: false,
            height_adjust_start_mouse_y: 0.0,
            height_adjust_start_y: 0.0,
            height_adjust_locked_pos: None,
            raster_settings: RasterSettings::default(), // backface_cull=true shows backfaces as wireframe
            selected_vertex_indices: Vec::new(),
        }
    }

    /// Create editor state with a file path
    pub fn with_file(level: Level, path: PathBuf) -> Self {
        let mut state = Self::new(level);
        state.current_file = Some(path);
        state
    }

    /// Load a new level, preserving view state (camera, zoom, etc.)
    pub fn load_level(&mut self, level: Level, path: PathBuf) {
        self.level = level;
        self.current_file = Some(path);
        self.dirty = false;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.selection = Selection::None;
        self.selected_vertex_indices.clear();
        // Clamp current_room to valid range
        if self.current_room >= self.level.rooms.len() {
            self.current_room = 0;
        }
    }

    /// Set the current selection and clear vertex color selection
    pub fn set_selection(&mut self, selection: Selection) {
        self.selection = selection;
        self.selected_vertex_indices.clear();
    }

    /// Set a status message that will be displayed for a duration
    pub fn set_status(&mut self, message: &str, duration_secs: f64) {
        let expiry = macroquad::time::get_time() + duration_secs;
        self.status_message = Some((message.to_string(), expiry));
    }

    /// Get current status message if not expired
    pub fn get_status(&self) -> Option<&str> {
        if let Some((msg, expiry)) = &self.status_message {
            if macroquad::time::get_time() < *expiry {
                return Some(msg);
            }
        }
        None
    }

    /// Save current state for undo
    pub fn save_undo(&mut self) {
        self.undo_stack.push(self.level.clone());
        self.redo_stack.clear();
        self.dirty = true;

        // Limit undo stack size
        if self.undo_stack.len() > 50 {
            self.undo_stack.remove(0);
        }
    }

    /// Undo last action
    pub fn undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(self.level.clone());
            self.level = prev;
        }
    }

    /// Redo last undone action
    pub fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(self.level.clone());
            self.level = next;
        }
    }

    /// Get current room being edited
    pub fn current_room(&self) -> Option<&crate::world::Room> {
        self.level.rooms.get(self.current_room)
    }

    /// Get current room mutably
    pub fn current_room_mut(&mut self) -> Option<&mut crate::world::Room> {
        self.level.rooms.get_mut(self.current_room)
    }

    /// Get textures from the currently selected pack
    pub fn current_textures(&self) -> &[Texture] {
        self.texture_packs
            .get(self.selected_pack)
            .map(|p| p.textures.as_slice())
            .unwrap_or(&[])
    }

    /// Get the name of the currently selected pack
    pub fn current_pack_name(&self) -> &str {
        self.texture_packs
            .get(self.selected_pack)
            .map(|p| p.name.as_str())
            .unwrap_or("(none)")
    }

    /// Check if a selection is in the multi-selection list
    pub fn is_multi_selected(&self, selection: &Selection) -> bool {
        self.multi_selection.iter().any(|s| s == selection)
    }

    /// Add a selection to the multi-selection list (if not already present)
    pub fn add_to_multi_selection(&mut self, selection: Selection) {
        if !matches!(selection, Selection::None) && !self.is_multi_selected(&selection) {
            self.multi_selection.push(selection);
        }
    }

    /// Clear multi-selection
    pub fn clear_multi_selection(&mut self) {
        self.multi_selection.clear();
    }

    /// Toggle a selection in the multi-selection list
    /// Also ensures the current primary selection is in multi_selection
    /// (so Shift+click after a regular click keeps the first item selected)
    pub fn toggle_multi_selection(&mut self, selection: Selection) {
        // First, ensure the current primary selection is in multi_selection
        // This handles the case where user clicks A, then Shift+clicks B
        if !matches!(self.selection, Selection::None) {
            if !self.multi_selection.iter().any(|s| s == &self.selection) {
                self.multi_selection.push(self.selection.clone());
            }
        }

        // Now toggle the new selection
        if let Some(pos) = self.multi_selection.iter().position(|s| s == &selection) {
            self.multi_selection.remove(pos);
        } else if !matches!(selection, Selection::None) {
            self.multi_selection.push(selection);
        }
    }

    /// Update camera position from orbit parameters
    pub fn sync_camera_from_orbit(&mut self) {
        let pitch = self.orbit_elevation;
        let yaw = self.orbit_azimuth;

        // Forward direction (what camera looks at)
        let forward = Vec3::new(
            pitch.cos() * yaw.sin(),
            -pitch.sin(),
            pitch.cos() * yaw.cos(),
        );

        // Camera sits behind the target along the forward direction
        self.camera_3d.position = self.orbit_target - forward * self.orbit_distance;
        self.camera_3d.rotation_x = pitch;
        self.camera_3d.rotation_y = yaw;
        self.camera_3d.update_basis();
    }

    /// Get the center point of the current selection (for orbit target)
    pub fn get_selection_center(&self) -> Option<Vec3> {
        match &self.selection {
            Selection::None => None,
            Selection::Room(room_idx) => {
                self.level.rooms.get(*room_idx).map(|room| {
                    let center_x = room.position.x + (room.width as f32 * SECTOR_SIZE) / 2.0;
                    let center_z = room.position.z + (room.depth as f32 * SECTOR_SIZE) / 2.0;
                    let center_y = room.position.y + 512.0; // Approximate middle height
                    Vec3::new(center_x, center_y, center_z)
                })
            }
            Selection::Sector { room, x, z } | Selection::SectorFace { room, x, z, .. } => {
                self.level.rooms.get(*room).and_then(|r| {
                    r.get_sector(*x, *z).map(|sector| {
                        let base_x = r.position.x + (*x as f32) * SECTOR_SIZE;
                        let base_z = r.position.z + (*z as f32) * SECTOR_SIZE;
                        let center_x = base_x + SECTOR_SIZE / 2.0;
                        let center_z = base_z + SECTOR_SIZE / 2.0;
                        // Calculate average height from floor/ceiling
                        let floor_y = sector.floor.as_ref().map(|f| f.avg_height()).unwrap_or(0.0);
                        let ceiling_y = sector.ceiling.as_ref().map(|c| c.avg_height()).unwrap_or(2048.0);
                        let center_y = (floor_y + ceiling_y) / 2.0;
                        Vec3::new(center_x, center_y, center_z)
                    })
                })
            }
            Selection::Edge { room, x, z, .. } => {
                // Same as sector for now
                self.level.rooms.get(*room).and_then(|r| {
                    r.get_sector(*x, *z).map(|sector| {
                        let base_x = r.position.x + (*x as f32) * SECTOR_SIZE;
                        let base_z = r.position.z + (*z as f32) * SECTOR_SIZE;
                        let center_x = base_x + SECTOR_SIZE / 2.0;
                        let center_z = base_z + SECTOR_SIZE / 2.0;
                        let floor_y = sector.floor.as_ref().map(|f| f.avg_height()).unwrap_or(0.0);
                        let ceiling_y = sector.ceiling.as_ref().map(|c| c.avg_height()).unwrap_or(2048.0);
                        let center_y = (floor_y + ceiling_y) / 2.0;
                        Vec3::new(center_x, center_y, center_z)
                    })
                })
            }
            Selection::Portal { room, portal } => {
                self.level.rooms.get(*room).and_then(|r| {
                    r.portals.get(*portal).map(|p| {
                        // Average of portal vertices
                        let sum = p.vertices.iter().fold(Vec3::ZERO, |acc, v| acc + *v);
                        let count = p.vertices.len() as f32;
                        Vec3::new(sum.x / count, sum.y / count, sum.z / count)
                    })
                })
            }
        }
    }

    /// Update orbit target based on current selection
    pub fn update_orbit_target(&mut self) {
        if let Some(center) = self.get_selection_center() {
            self.orbit_target = center;
            self.last_orbit_target = center;
        } else {
            // Use last known target if nothing selected
            self.orbit_target = self.last_orbit_target;
        }
    }
}
