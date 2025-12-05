//! Editor state and data

use std::path::PathBuf;
use crate::world::Level;
use crate::rasterizer::{Camera, Vec3, Texture};

/// A texture pack loaded from a folder
pub struct TexturePack {
    pub name: String,
    pub path: PathBuf,
    pub textures: Vec<Texture>,
}

impl TexturePack {
    /// Load a texture pack from a directory
    pub fn from_directory(path: PathBuf) -> Option<Self> {
        let name = path.file_name()?.to_string_lossy().to_string();
        let textures = Texture::load_directory(&path);
        if textures.is_empty() {
            // Try loading from subdirectories (some packs have nested folders)
            let mut all_textures = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&path) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let entry_path = entry.path();
                    if entry_path.is_dir() {
                        all_textures.extend(Texture::load_directory(&entry_path));
                    }
                }
            }
            if all_textures.is_empty() {
                return None;
            }
            Some(Self { name, path, textures: all_textures })
        } else {
            Some(Self { name, path, textures })
        }
    }

    /// Discover all texture packs in the assets/textures directory
    pub fn discover_all() -> Vec<Self> {
        let mut packs = Vec::new();
        let textures_dir = PathBuf::from("assets/textures");

        if let Ok(entries) = std::fs::read_dir(&textures_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(pack) = Self::from_directory(path) {
                        packs.push(pack);
                    }
                }
            }
        }

        // Sort by name
        packs.sort_by(|a, b| a.name.cmp(&b.name));
        packs
    }
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

/// What is currently selected in the editor
#[derive(Debug, Clone)]
pub enum Selection {
    None,
    Room(usize),
    Face { room: usize, face: usize },
    Vertex { room: usize, vertex: usize },
    Portal { room: usize, portal: usize },
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

    /// Currently selected room index (for editing)
    pub current_room: usize,

    /// Selected texture index
    pub selected_texture: usize,

    /// 3D viewport camera
    pub camera_3d: Camera,

    /// 2D grid view camera (pan and zoom)
    pub grid_offset_x: f32,
    pub grid_offset_y: f32,
    pub grid_zoom: f32,

    /// Grid settings
    pub grid_size: f32, // World units per grid cell
    pub show_grid: bool,

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
    pub grid_dragging_vertex: Option<usize>,
    pub grid_drag_started: bool, // True if we've started dragging (for undo)

    /// 3D viewport vertex dragging state
    pub viewport_dragging_vertex: Option<(usize, usize)>, // (room_idx, vertex_idx)
    pub viewport_drag_started: bool,
    pub viewport_drag_plane_y: f32, // Y height of the drag plane

    /// Texture palette state
    pub texture_packs: Vec<TexturePack>,
    pub selected_pack: usize,
    pub texture_scroll: f32,
}

impl EditorState {
    pub fn new(level: Level) -> Self {
        let mut camera_3d = Camera::new();
        camera_3d.position = Vec3::new(0.0, 5.0, -10.0);

        // Discover all texture packs
        let texture_packs = TexturePack::discover_all();
        println!("Discovered {} texture packs", texture_packs.len());
        for pack in &texture_packs {
            println!("  - {} ({} textures)", pack.name, pack.textures.len());
        }

        Self {
            level,
            current_file: None,
            tool: EditorTool::Select,
            selection: Selection::None,
            current_room: 0,
            selected_texture: 0,
            camera_3d,
            grid_offset_x: 0.0,
            grid_offset_y: 0.0,
            grid_zoom: 20.0, // Pixels per world unit
            grid_size: 1.0,
            show_grid: true,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            dirty: false,
            status_message: None,
            viewport_last_mouse: (0.0, 0.0),
            viewport_mouse_captured: false,
            grid_last_mouse: (0.0, 0.0),
            grid_panning: false,
            grid_dragging_vertex: None,
            grid_drag_started: false,
            viewport_dragging_vertex: None,
            viewport_drag_started: false,
            viewport_drag_plane_y: 0.0,
            texture_packs,
            selected_pack: 0,
            texture_scroll: 0.0,
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
        // Clamp current_room to valid range
        if self.current_room >= self.level.rooms.len() {
            self.current_room = 0;
        }
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
}
