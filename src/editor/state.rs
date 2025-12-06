//! Editor state and data

use std::path::PathBuf;
use crate::world::Level;
use crate::rasterizer::{Camera, Vec3, Texture};

/// TRLE grid constraints
/// Sector size in world units (X-Z plane)
pub const SECTOR_SIZE: f32 = 1024.0;
/// Height subdivision ("click") in world units (Y axis)
pub const CLICK_HEIGHT: f32 = 256.0;

/// A texture pack loaded from a folder
pub struct TexturePack {
    pub name: String,
    pub path: PathBuf,
    pub textures: Vec<Texture>,
}

impl TexturePack {
    /// Load a texture pack from a directory (native only)
    #[cfg(not(target_arch = "wasm32"))]
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

    /// Discover all texture packs in the assets/textures directory (native only)
    #[cfg(not(target_arch = "wasm32"))]
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

    /// Discover all texture packs from manifest (WASM)
    #[cfg(target_arch = "wasm32")]
    pub fn discover_all() -> Vec<Self> {
        // Return empty - will be loaded async
        Vec::new()
    }

    /// Load texture packs from manifest asynchronously (for WASM)
    pub async fn load_from_manifest() -> Vec<Self> {
        use macroquad::prelude::*;

        let manifest_path = "assets/textures/manifest.txt";
        let manifest = match load_string(manifest_path).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to load texture manifest: {}", e);
                return Vec::new();
            }
        };

        let mut packs = Vec::new();
        let mut current_pack: Option<(String, Vec<Texture>)> = None;

        for line in manifest.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                // Save previous pack if any
                if let Some((name, textures)) = current_pack.take() {
                    if !textures.is_empty() {
                        packs.push(TexturePack {
                            name: name.clone(),
                            path: PathBuf::from(format!("assets/textures/{}", name)),
                            textures,
                        });
                    }
                }
                // Start new pack
                let pack_name = line[1..line.len()-1].to_string();
                current_pack = Some((pack_name, Vec::new()));
            } else if let Some((ref pack_name, ref mut textures)) = current_pack {
                // Load texture file
                let tex_path = format!("assets/textures/{}/{}", pack_name, line);
                match load_file(&tex_path).await {
                    Ok(bytes) => {
                        let tex_name = line.strip_suffix(".png")
                            .or_else(|| line.strip_suffix(".PNG"))
                            .unwrap_or(line)
                            .to_string();
                        match Texture::from_bytes(&bytes, tex_name) {
                            Ok(tex) => textures.push(tex),
                            Err(e) => eprintln!("Failed to decode {}: {}", tex_path, e),
                        }
                    }
                    Err(e) => eprintln!("Failed to load {}: {}", tex_path, e),
                }
            }
        }

        // Don't forget the last pack
        if let Some((name, textures)) = current_pack {
            if !textures.is_empty() {
                packs.push(TexturePack {
                    name: name.clone(),
                    path: PathBuf::from(format!("assets/textures/{}", name)),
                    textures,
                });
            }
        }

        println!("Loaded {} texture packs from manifest", packs.len());
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
    Vertex { room: usize, vertex: usize },
    Edge { room: usize, v0: usize, v1: usize },  // Edge defined by two vertex indices
    Face { room: usize, face: usize },
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

    /// 2D grid view camera (pan and zoom)
    pub grid_offset_x: f32,
    pub grid_offset_y: f32,
    pub grid_zoom: f32,

    /// Grid settings
    pub grid_size: f32, // World units per grid cell
    pub show_grid: bool,

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

    /// 3D viewport vertex dragging state
    pub viewport_dragging_vertices: Vec<(usize, usize)>, // List of (room_idx, vertex_idx)
    pub viewport_drag_started: bool,
    pub viewport_drag_plane_y: f32, // Y height of the drag plane (reference point for delta)
    pub viewport_drag_initial_y: Vec<f32>, // Initial Y positions of each dragged vertex

    /// Texture palette state
    pub texture_packs: Vec<TexturePack>,
    pub selected_pack: usize,
    pub texture_scroll: f32,
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

        Self {
            level,
            current_file: None,
            tool: EditorTool::Select,
            selection: Selection::None,
            multi_selection: Vec::new(),
            selection_rect_start: None,
            selection_rect_end: None,
            current_room: 0,
            selected_texture: crate::world::TextureRef::none(),
            camera_3d,
            grid_offset_x: 0.0,
            grid_offset_y: 0.0,
            grid_zoom: 0.1, // Pixels per world unit (very zoomed out for TRLE 1024-unit sectors)
            grid_size: SECTOR_SIZE, // TRLE sector size
            show_grid: true,
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

    /// Check if a selection is in the multi-selection list
    pub fn is_multi_selected(&self, selection: &Selection) -> bool {
        self.multi_selection.iter().any(|s| match (s, selection) {
            (Selection::Face { room: r1, face: f1 }, Selection::Face { room: r2, face: f2 }) => {
                r1 == r2 && f1 == f2
            }
            (Selection::Vertex { room: r1, vertex: v1 }, Selection::Vertex { room: r2, vertex: v2 }) => {
                r1 == r2 && v1 == v2
            }
            (Selection::Edge { room: r1, v0: v0_1, v1: v1_1 }, Selection::Edge { room: r2, v0: v0_2, v1: v1_2 }) => {
                r1 == r2 && v0_1 == v0_2 && v1_1 == v1_2
            }
            _ => false,
        })
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
    pub fn toggle_multi_selection(&mut self, selection: Selection) {
        if let Some(pos) = self.multi_selection.iter().position(|s| match (s, &selection) {
            (Selection::Face { room: r1, face: f1 }, Selection::Face { room: r2, face: f2 }) => {
                r1 == r2 && f1 == f2
            }
            (Selection::Vertex { room: r1, vertex: v1 }, Selection::Vertex { room: r2, vertex: v2 }) => {
                r1 == r2 && v1 == v2
            }
            (Selection::Edge { room: r1, v0: v0_1, v1: v1_1 }, Selection::Edge { room: r2, v0: v0_2, v1: v1_2 }) => {
                r1 == r2 && v0_1 == v0_2 && v1_1 == v1_2
            }
            _ => false,
        }) {
            self.multi_selection.remove(pos);
        } else if !matches!(selection, Selection::None) {
            self.multi_selection.push(selection);
        }
    }
}
