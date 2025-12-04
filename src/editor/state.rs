//! Editor state and data

use crate::world::Level;
use crate::rasterizer::{Camera, Vec3};

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

    /// 3D viewport mouse state (for camera control)
    pub viewport_last_mouse: (f32, f32),
    pub viewport_mouse_captured: bool,
}

impl EditorState {
    pub fn new(level: Level) -> Self {
        let mut camera_3d = Camera::new();
        camera_3d.position = Vec3::new(0.0, 5.0, -10.0);

        Self {
            level,
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
            viewport_last_mouse: (0.0, 0.0),
            viewport_mouse_captured: false,
        }
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
}
