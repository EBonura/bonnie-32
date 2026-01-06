//! Editor state and data

use std::path::PathBuf;
use crate::world::{Level, ObjectType, SpawnPointType, LevelObject, TextureRef, FaceNormalMode, UvProjection, SplitDirection, HorizontalFace, VerticalFace};
use crate::rasterizer::{Camera, Vec3, Vec2, Texture, RasterSettings, Color, BlendMode};
use super::texture_pack::TexturePack;

/// Frame timing breakdown for editor performance debugging
#[derive(Debug, Clone, Default)]
pub struct EditorFrameTimings {
    /// Total frame time (ms)
    pub total_ms: f32,
    /// Toolbar drawing time (ms)
    pub toolbar_ms: f32,
    /// Left panel (skybox, 2D grid, rooms, debug) time (ms)
    pub left_panel_ms: f32,
    /// 3D viewport rendering time (ms) - total
    pub viewport_3d_ms: f32,
    /// Right panel (textures, properties) time (ms)
    pub right_panel_ms: f32,
    /// Status bar time (ms)
    pub status_ms: f32,

    // === 3D Viewport sub-timings ===
    /// Input handling (camera, keyboard shortcuts)
    pub vp_input_ms: f32,
    /// Clear/skybox rendering
    pub vp_clear_ms: f32,
    /// Grid line drawing
    pub vp_grid_ms: f32,
    /// Light collection
    pub vp_lights_ms: f32,
    /// Texture conversion (RGB888 to RGB555)
    pub vp_texconv_ms: f32,
    /// Mesh data generation (to_render_data)
    pub vp_meshgen_ms: f32,
    /// Rasterization (render_mesh calls)
    pub vp_raster_ms: f32,
    /// Preview rendering (walls, floors, objects, clipboard)
    pub vp_preview_ms: f32,
    /// Selection/highlighting overlays
    pub vp_selection_ms: f32,
    /// Texture upload to GPU
    pub vp_upload_ms: f32,
}

impl EditorFrameTimings {
    /// Start timing (returns time in seconds from macroquad)
    pub fn start() -> f64 {
        macroquad::prelude::get_time()
    }

    /// Get elapsed time in ms since start
    pub fn elapsed_ms(start: f64) -> f32 {
        ((macroquad::prelude::get_time() - start) * 1000.0) as f32
    }
}

/// TRLE grid constraints
/// Sector size in world units (X-Z plane)
pub const SECTOR_SIZE: f32 = 1024.0;
/// Height subdivision ("click") in world units (Y axis)
pub const CLICK_HEIGHT: f32 = 256.0;
/// Default ceiling height (3x sector size = 3 meters)
pub const CEILING_HEIGHT: f32 = 3072.0;

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
    DrawWall,      // Handles all 6 directions (N, E, S, W, NW-SE, NE-SW)
    DrawCeiling,
    PlaceObject,
}

/// 2D Grid View projection mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GridViewMode {
    #[default]
    Top,    // X-Z plane (looking down Y axis)
    Front,  // X-Y plane (looking along -Z)
    Side,   // Y-Z plane (looking along -X)
}

/// Which triangle within a horizontal face is selected for editing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TriangleSelection {
    #[default]
    Both,   // Edit both triangles (linked behavior)
    Tri1,   // Edit only triangle 1
    Tri2,   // Edit only triangle 2
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
    WallNwSe(usize),   // Diagonal wall NW to SE corner
    WallNeSw(usize),   // Diagonal wall NE to SW corner
}

impl SectorFace {
    /// Returns true if this is a wall face (not floor or ceiling)
    pub fn is_wall(&self) -> bool {
        !matches!(self, SectorFace::Floor | SectorFace::Ceiling)
    }

    /// Returns the Direction for wall faces, None for floor/ceiling
    pub fn direction(&self) -> Option<crate::world::Direction> {
        use crate::world::Direction;
        match self {
            SectorFace::WallNorth(_) => Some(Direction::North),
            SectorFace::WallEast(_) => Some(Direction::East),
            SectorFace::WallSouth(_) => Some(Direction::South),
            SectorFace::WallWest(_) => Some(Direction::West),
            SectorFace::WallNwSe(_) => Some(Direction::NwSe),
            SectorFace::WallNeSw(_) => Some(Direction::NeSw),
            SectorFace::Floor | SectorFace::Ceiling => None,
        }
    }
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
    /// Single vertex of a face
    /// corner_idx: 0=NW, 1=NE, 2=SE, 3=SW for horizontal faces
    /// For walls: 0=bottom-left, 1=bottom-right, 2=top-right, 3=top-left
    Vertex { room: usize, x: usize, z: usize, face: SectorFace, corner_idx: usize },
    /// Edge of a face (two vertices)
    /// face_idx: 0=floor, 1=ceiling, 2=wall
    /// edge_idx: 0-3 for floor/ceiling (north, east, south, west)
    ///           0-3 for wall (bottom, right, top, left)
    /// wall_face: Some(SectorFace::WallXxx) when face_idx=2
    Edge { room: usize, x: usize, z: usize, face_idx: usize, edge_idx: usize, wall_face: Option<SectorFace> },
    Portal { room: usize, portal: usize },
    /// Level object selected (spawn, light, prop, etc.)
    /// room: which room the object belongs to
    /// index: index within that room's objects array
    Object { room: usize, index: usize },
}

/// Snapshot of selection state for undo/redo
#[derive(Debug, Clone)]
pub struct SelectionSnapshot {
    pub selection: Selection,
    pub multi_selection: Vec<Selection>,
}

/// Face properties for clipboard copy/paste (excludes heights)
#[derive(Debug, Clone)]
pub enum FaceClipboard {
    /// Horizontal face properties (floor/ceiling)
    Horizontal {
        split_direction: SplitDirection,
        texture: TextureRef,
        uv: Option<[Vec2; 4]>,
        colors: [Color; 4],
        texture_2: Option<TextureRef>,
        uv_2: Option<[Vec2; 4]>,
        colors_2: Option<[Color; 4]>,
        walkable: bool,
        blend_mode: BlendMode,
        normal_mode: FaceNormalMode,
        black_transparent: bool,
    },
    /// Vertical face properties (wall)
    Vertical {
        texture: TextureRef,
        uv: Option<[Vec2; 4]>,
        solid: bool,
        blend_mode: BlendMode,
        colors: [Color; 4],
        normal_mode: FaceNormalMode,
        black_transparent: bool,
        uv_projection: UvProjection,
    },
}

/// A copied face with its position relative to anchor
#[derive(Debug, Clone)]
pub struct CopiedFace {
    /// Relative sector position (from anchor)
    pub rel_x: i32,
    pub rel_z: i32,
    /// The face type and data
    pub face: CopiedFaceData,
}

/// The actual face data (floor, ceiling, or wall)
#[derive(Debug, Clone)]
pub enum CopiedFaceData {
    Floor(HorizontalFace),
    Ceiling(HorizontalFace),
    WallNorth(usize, VerticalFace),  // wall index + face data
    WallEast(usize, VerticalFace),
    WallSouth(usize, VerticalFace),
    WallWest(usize, VerticalFace),
    WallNwSe(usize, VerticalFace),
    WallNeSw(usize, VerticalFace),
}

/// Geometry clipboard for copying/pasting entire face selections
#[derive(Debug, Clone)]
pub struct GeometryClipboard {
    /// All copied faces with their relative positions
    pub faces: Vec<CopiedFace>,
    /// Horizontal flip state (toggled with H key)
    pub flip_h: bool,
    /// Vertical flip state (toggled with V key)
    pub flip_v: bool,
}

impl GeometryClipboard {
    pub fn new() -> Self {
        Self {
            faces: Vec::new(),
            flip_h: false,
            flip_v: false,
        }
    }

    /// Get bounding box of copied geometry (min_x, max_x, min_z, max_z)
    pub fn bounds(&self) -> (i32, i32, i32, i32) {
        if self.faces.is_empty() {
            return (0, 0, 0, 0);
        }
        let mut min_x = i32::MAX;
        let mut max_x = i32::MIN;
        let mut min_z = i32::MAX;
        let mut max_z = i32::MIN;
        for face in &self.faces {
            min_x = min_x.min(face.rel_x);
            max_x = max_x.max(face.rel_x);
            min_z = min_z.min(face.rel_z);
            max_z = max_z.max(face.rel_z);
        }
        (min_x, max_x, min_z, max_z)
    }
}

/// Unified undo event - either a level change or a selection change
#[derive(Debug, Clone)]
pub enum UndoEvent {
    Level(Level),
    Selection(SelectionSnapshot),
}

impl Selection {
    /// Check if this selection includes a specific sector (either whole sector or face within it)
    pub fn includes_sector(&self, room_idx: usize, sx: usize, sz: usize) -> bool {
        match self {
            Selection::Sector { room, x, z } => *room == room_idx && *x == sx && *z == sz,
            Selection::SectorFace { room, x, z, .. } => *room == room_idx && *x == sx && *z == sz,
            Selection::Vertex { room, x, z, .. } => *room == room_idx && *x == sx && *z == sz,
            _ => false,
        }
    }

    /// Get the sector coordinates if this is a sector or sector-face selection
    pub fn sector_coords(&self) -> Option<(usize, usize, usize)> {
        match self {
            Selection::Sector { room, x, z } => Some((*room, *x, *z)),
            Selection::SectorFace { room, x, z, .. } => Some((*room, *x, *z)),
            Selection::Vertex { room, x, z, .. } => Some((*room, *x, *z)),
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
            // Vertex selection includes its parent face
            Selection::Vertex { room, x, z, face: f, .. } => {
                *room == room_idx && *x == sx && *z == sz && *f == face
            }
            _ => false,
        }
    }

    /// Check if this selection includes a specific vertex
    pub fn includes_vertex(&self, room_idx: usize, sx: usize, sz: usize, face: SectorFace, corner: usize) -> bool {
        match self {
            // Whole sector selection includes all vertices
            Selection::Sector { room, x, z } => *room == room_idx && *x == sx && *z == sz,
            // Face selection includes all its vertices
            Selection::SectorFace { room, x, z, face: f } => {
                *room == room_idx && *x == sx && *z == sz && *f == face
            }
            // Vertex selection matches exact vertex
            Selection::Vertex { room, x, z, face: f, corner_idx } => {
                *room == room_idx && *x == sx && *z == sz && *f == face && *corner_idx == corner
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

    /// Which triangle is selected for texture editing (for horizontal faces)
    pub selected_triangle: TriangleSelection,

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

    /// 2D grid view projection mode (Top/Front/Side)
    pub grid_view_mode: GridViewMode,

    /// Grid settings
    pub grid_size: f32, // World units per grid cell
    pub show_grid: bool,

    /// 3D viewport settings
    pub show_room_bounds: bool, // Show room boundary wireframes

    /// Vertex editing mode
    pub link_coincident_vertices: bool, // When true, moving a vertex moves all vertices at same position

    /// Unified undo/redo stack (level and selection changes in order)
    pub undo_stack: Vec<UndoEvent>,
    pub redo_stack: Vec<UndoEvent>,

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

    /// 2D grid view sector dragging (for moving sectors within room or moving entire room)
    /// List of (room_idx, grid_x, grid_z) for sectors being dragged
    pub grid_dragging_sectors: Vec<(usize, usize, usize)>,
    /// World-space offset being applied during drag
    pub grid_sector_drag_offset: (f32, f32),
    /// Starting world position when drag began (for calculating offset)
    pub grid_sector_drag_start: Option<(f32, f32)>,
    /// True if dragging the room origin marker (moves entire room position)
    pub grid_dragging_room_origin: bool,
    /// Object being dragged in 2D grid view (room_idx, object_idx)
    pub grid_dragging_object: Option<(usize, usize)>,

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

    /// 3D viewport object dragging state
    pub dragging_object: Option<(usize, usize)>, // (room_idx, object_idx)
    pub dragging_object_initial_y: f32,          // Initial Y when drag started
    pub dragging_object_plane_y: f32,            // Current accumulated drag plane Y

    /// Texture palette state
    pub texture_packs: Vec<TexturePack>,
    pub selected_pack: usize,
    pub texture_scroll: f32,
    pub texture_palette_width: f32, // Actual width for scroll calculations

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

    /// Drag-to-place state (for DrawFloor/DrawCeiling modes)
    /// Start grid position when drag began (gx, gz)
    pub placement_drag_start: Option<(i32, i32)>,
    /// Current grid position during drag (gx, gz)
    pub placement_drag_current: Option<(i32, i32)>,

    /// Wall drag-to-place state (for DrawWall mode)
    /// Start position: (grid_x, grid_z, direction)
    pub wall_drag_start: Option<(i32, i32, crate::world::Direction)>,
    /// Current position during drag: (grid_x, grid_z, direction)
    pub wall_drag_current: Option<(i32, i32, crate::world::Direction)>,
    /// Room-relative Y position when wall drag started (for consistent gap selection)
    pub wall_drag_mouse_y: Option<f32>,
    /// Current wall direction for DrawWall mode (rotated with R key)
    pub wall_direction: crate::world::Direction,
    /// Prefer high gap when placing walls (toggled with F key)
    pub wall_prefer_high: bool,

    /// Rasterizer settings (PS1 effects)
    pub raster_settings: RasterSettings,

    /// Selected vertex indices for color editing (0-3 for face corners)
    pub selected_vertex_indices: Vec<usize>,

    /// Color picker active slider (0=R, 1=G, 2=B) for light color editing
    pub light_color_slider: Option<usize>,

    /// Color picker active slider for vertex color editing
    pub vertex_color_slider: Option<usize>,

    /// Skybox panel: active slider ID
    pub skybox_active_slider: Option<usize>,

    /// Skybox panel: selected color target (for RGB sliders)
    /// 0-3 = gradient colors (zenith, horizon_sky, horizon_ground, nadir)
    /// 10 = horizontal tint, 20 = sun core, 21 = sun glow, 22 = moon core, 23 = moon glow
    /// 30 = cloud layer 1, 31 = cloud layer 2, 40 = mtn range 1 lit, 41 = mtn range 1 shadow
    /// 42 = mtn range 1 highlight, 50 = mtn range 2 lit, etc., 60 = stars, 70 = haze
    pub skybox_selected_color: Option<usize>,

    /// Skybox panel: section expansion states
    pub skybox_gradient_expanded: bool,
    pub skybox_celestial_expanded: bool,
    pub skybox_clouds_expanded: bool,
    pub skybox_mountains_expanded: bool,
    pub skybox_stars_expanded: bool,
    pub skybox_atmo_expanded: bool,

    /// Skybox panel: selected cloud layer (0 or 1)
    pub skybox_selected_cloud_layer: usize,
    /// Skybox panel: selected mountain range (0 or 1)
    pub skybox_selected_mountain_range: usize,

    /// Hidden rooms (room indices that should not be rendered in 2D/3D views)
    pub hidden_rooms: std::collections::HashSet<usize>,

    /// Portals need recalculation (set when geometry changes)
    pub portals_dirty: bool,

    /// Selected object type to place (when PlaceObject tool is active)
    pub selected_object_type: ObjectType,

    /// Player property editing state (for click-to-edit numeric fields)
    /// Field IDs: 0=radius, 1=height, 2=step, 3=walk, 4=run, 5=gravity, 6=camera_distance, 7=camera_height
    pub player_prop_editing: Option<usize>,
    pub player_prop_buffer: String,

    /// Clipboard for copy/paste operations (stores copied object)
    pub clipboard: Option<LevelObject>,

    /// Face clipboard for copy/paste face properties (texture, UV, colors, etc.)
    pub face_clipboard: Option<FaceClipboard>,

    /// Geometry clipboard for copy/paste entire face selections
    pub geometry_clipboard: Option<GeometryClipboard>,

    /// Frame timing breakdown for debug panel
    pub frame_timings: EditorFrameTimings,
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
            selected_triangle: TriangleSelection::Both,
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
            grid_view_mode: GridViewMode::Top,
            grid_size: SECTOR_SIZE, // TRLE sector size
            show_grid: true,
            show_room_bounds: true, // Room boundaries visible by default
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
            grid_dragging_sectors: Vec::new(),
            grid_sector_drag_offset: (0.0, 0.0),
            grid_sector_drag_start: None,
            grid_dragging_room_origin: false,
            grid_dragging_object: None,
            viewport_dragging_vertices: Vec::new(),
            viewport_drag_started: false,
            viewport_drag_plane_y: 0.0,
            viewport_drag_initial_y: Vec::new(),
            dragging_sector_vertices: Vec::new(),
            drag_initial_heights: Vec::new(),
            dragging_object: None,
            dragging_object_initial_y: 0.0,
            dragging_object_plane_y: 0.0,
            texture_packs,
            selected_pack: 0,
            texture_scroll: 0.0,
            texture_palette_width: 200.0, // Default, updated by draw_texture_palette
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
            placement_drag_start: None,
            placement_drag_current: None,
            wall_drag_start: None,
            wall_drag_current: None,
            wall_drag_mouse_y: None,
            wall_direction: crate::world::Direction::North,
            wall_prefer_high: false,
            raster_settings: RasterSettings::default(), // backface_cull=true shows backfaces as wireframe
            selected_vertex_indices: Vec::new(),
            light_color_slider: None,
            vertex_color_slider: None,
            skybox_active_slider: None,
            skybox_selected_color: None,
            skybox_gradient_expanded: true,  // Start expanded
            skybox_celestial_expanded: false,
            skybox_clouds_expanded: false,
            skybox_mountains_expanded: false,
            skybox_stars_expanded: false,
            skybox_atmo_expanded: false,
            skybox_selected_cloud_layer: 0,
            skybox_selected_mountain_range: 0,
            hidden_rooms: std::collections::HashSet::new(),
            portals_dirty: true, // Recalculate on first frame
            selected_object_type: ObjectType::Spawn(SpawnPointType::PlayerStart), // Default to player start
            player_prop_editing: None,
            player_prop_buffer: String::new(),
            clipboard: None,
            face_clipboard: None,
            geometry_clipboard: None,
            frame_timings: EditorFrameTimings::default(),
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
        self.multi_selection.clear();
        self.selected_vertex_indices.clear();
        self.portals_dirty = true; // Recalculate portals for loaded level
        // Clamp current_room to valid range
        if self.current_room >= self.level.rooms.len() {
            self.current_room = 0;
        }
    }

    /// Set the current selection and clear vertex color selection
    /// Does NOT auto-save undo - caller should call save_selection_undo() BEFORE
    /// modifying any selection state (including toggle/clear multi_selection)
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

    /// Save current level state for undo
    pub fn save_undo(&mut self) {
        self.undo_stack.push(UndoEvent::Level(self.level.clone()));
        self.redo_stack.clear();
        self.dirty = true;

        // Limit undo stack size
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
    }

    /// Save current selection state for undo
    pub fn save_selection_undo(&mut self) {
        // Don't save if selection hasn't changed from the last selection snapshot
        for event in self.undo_stack.iter().rev() {
            if let UndoEvent::Selection(last) = event {
                if last.selection == self.selection && last.multi_selection == self.multi_selection {
                    return; // No change from last selection snapshot
                }
                break; // Found a different selection snapshot
            }
        }

        self.undo_stack.push(UndoEvent::Selection(SelectionSnapshot {
            selection: self.selection.clone(),
            multi_selection: self.multi_selection.clone(),
        }));
        self.redo_stack.clear();

        // Limit stack size
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
    }

    /// Undo last action (level or selection)
    pub fn undo(&mut self) {
        if let Some(event) = self.undo_stack.pop() {
            match event {
                UndoEvent::Level(prev_level) => {
                    self.redo_stack.push(UndoEvent::Level(self.level.clone()));
                    self.level = prev_level;
                }
                UndoEvent::Selection(prev_sel) => {
                    self.redo_stack.push(UndoEvent::Selection(SelectionSnapshot {
                        selection: self.selection.clone(),
                        multi_selection: self.multi_selection.clone(),
                    }));
                    self.set_selection(prev_sel.selection);
                    self.multi_selection = prev_sel.multi_selection;
                }
            }
        }
    }

    /// Redo last undone action (level or selection)
    pub fn redo(&mut self) {
        if let Some(event) = self.redo_stack.pop() {
            match event {
                UndoEvent::Level(next_level) => {
                    self.undo_stack.push(UndoEvent::Level(self.level.clone()));
                    self.level = next_level;
                }
                UndoEvent::Selection(next_sel) => {
                    self.undo_stack.push(UndoEvent::Selection(SelectionSnapshot {
                        selection: self.selection.clone(),
                        multi_selection: self.multi_selection.clone(),
                    }));
                    self.set_selection(next_sel.selection);
                    self.multi_selection = next_sel.multi_selection;
                }
            }
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
    /// Note: Caller should call save_selection_undo() before a batch of selection changes
    pub fn add_to_multi_selection(&mut self, selection: Selection) {
        if !matches!(selection, Selection::None) && !self.is_multi_selected(&selection) {
            self.multi_selection.push(selection);
        }
    }

    /// Clear multi-selection
    /// Note: Caller should call save_selection_undo() before a batch of selection changes
    pub fn clear_multi_selection(&mut self) {
        self.multi_selection.clear();
    }

    /// Toggle a selection in the multi-selection list
    /// Also ensures the current primary selection is in multi_selection
    /// (so Shift+click after a regular click keeps the first item selected)
    /// Note: Does not auto-save undo - caller should use set_selection() after, which will save
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
            Selection::Sector { room, x, z } | Selection::SectorFace { room, x, z, .. } | Selection::Vertex { room, x, z, .. } => {
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
            Selection::Object { room: room_idx, index } => {
                self.level.rooms.get(*room_idx).and_then(|room| {
                    room.objects.get(*index).map(|obj| {
                        obj.world_position(room)
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

    /// Mark portals as needing recalculation
    pub fn mark_portals_dirty(&mut self) {
        self.portals_dirty = true;
    }

    /// Scroll texture palette to show and highlight a specific texture
    /// Switches to the correct pack, adjusts scroll position, and sets selection
    pub fn scroll_to_texture(&mut self, tex_ref: &crate::world::TextureRef) {
        if !tex_ref.is_valid() {
            return;
        }

        // Find the pack index
        let pack_idx = self.texture_packs.iter().position(|p| p.name == tex_ref.pack);
        if let Some(idx) = pack_idx {
            // Switch to that pack
            self.selected_pack = idx;

            // Highlight the texture in the palette
            self.selected_texture = tex_ref.clone();

            // Find the texture index within the pack
            if let Some(pack) = self.texture_packs.get(idx) {
                if let Some(tex_idx) = pack.textures.iter().position(|t| t.name == tex_ref.name) {
                    // Calculate scroll position to show the texture at the top of visible area
                    // Constants from texture_palette.rs
                    const THUMB_SIZE: f32 = 48.0;
                    const THUMB_PADDING: f32 = 4.0;

                    // Use actual palette width (updated by draw_texture_palette)
                    let palette_width = self.texture_palette_width;
                    let cols = ((palette_width - THUMB_PADDING) / (THUMB_SIZE + THUMB_PADDING)).floor() as usize;
                    let cols = cols.max(1);

                    let row = tex_idx / cols;
                    // Position this row at top of visible area
                    let row_y = row as f32 * (THUMB_SIZE + THUMB_PADDING);

                    // Set scroll to show this row at the top (texture_palette will clamp to valid range)
                    self.texture_scroll = row_y;
                }
            }
        }
    }

    /// Center the 2D grid view on the current room
    pub fn center_2d_on_current_room(&mut self) {
        use crate::world::SECTOR_SIZE;

        if let Some(room) = self.level.rooms.get(self.current_room) {
            // Calculate room center in world coordinates based on view mode
            let center_x = room.position.x + (room.width as f32 * SECTOR_SIZE) / 2.0;
            let center_z = room.position.z + (room.depth as f32 * SECTOR_SIZE) / 2.0;
            let center_y = room.position.y + (room.bounds.max.y + room.bounds.min.y) / 2.0;

            // The grid view uses: screen_x = center_x + world_a * scale
            // where center_x = rect.x + rect.w * 0.5 + grid_offset_x
            // To center on room, we need: screen_center = world_room_center * scale + (rect.center + offset)
            // So offset = -world_room_center * scale (to put room center at screen center)
            // But since scale is applied later, we just need offset = -world_room_center * scale
            // Actually simpler: offset makes the world origin appear at (rect.center + offset)
            // To center on room at (room_x, room_z), we need offset = -room_center * scale

            let (room_a, room_b) = match self.grid_view_mode {
                GridViewMode::Top => (center_x, center_z),
                GridViewMode::Front => (center_x, center_y),
                GridViewMode::Side => (center_z, center_y),
            };

            // Set offset to center the room (negative because we want room center at origin)
            self.grid_offset_x = -room_a * self.grid_zoom;
            self.grid_offset_y = room_b * self.grid_zoom; // Positive because screen Y is inverted
        }
    }

    /// Center the 3D camera on the current room
    pub fn center_3d_on_current_room(&mut self) {
        use crate::world::SECTOR_SIZE;
        use crate::rasterizer::Vec3;

        if let Some(room) = self.level.rooms.get(self.current_room) {
            // Calculate room center
            let center_x = room.position.x + (room.width as f32 * SECTOR_SIZE) / 2.0;
            let center_z = room.position.z + (room.depth as f32 * SECTOR_SIZE) / 2.0;
            let center_y = room.position.y + (room.bounds.max.y + room.bounds.min.y) / 2.0;

            // Calculate room size to determine camera distance
            let room_size_x = room.width as f32 * SECTOR_SIZE;
            let room_size_z = room.depth as f32 * SECTOR_SIZE;
            let room_size = room_size_x.max(room_size_z);

            // Position camera above and behind the room center, looking at it
            let distance = room_size * 1.5;
            self.camera_3d.position = Vec3::new(
                center_x,
                center_y + distance * 0.5,
                center_z - distance,
            );

            // Look at room center (calculate rotation angles)
            let dx = center_x - self.camera_3d.position.x;
            let dy = center_y - self.camera_3d.position.y;
            let dz = center_z - self.camera_3d.position.z;
            let horizontal_dist = (dx * dx + dz * dz).sqrt();

            self.camera_3d.rotation_y = dx.atan2(dz);
            self.camera_3d.rotation_x = (-dy).atan2(horizontal_dist);

            // Update orbit parameters if in orbit mode
            self.orbit_target = Vec3::new(center_x, center_y, center_z);
            self.orbit_distance = distance;
            self.orbit_azimuth = 0.0;
            self.orbit_elevation = 0.3; // Slight downward angle
            self.sync_camera_from_orbit();
        }
    }
}
