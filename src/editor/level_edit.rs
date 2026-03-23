use crate::scene::Level;
use super::undo::UndoStack;

/// Working state for the currently active level being edited.
/// Owns the mutable working copy, undo stack, and dirty flag.
pub struct LevelEditState {
    pub current_level: Option<Level>,
    /// Undo/redo for level geometry edits
    pub level_undo: UndoStack<Level>,
    /// True when the level has unsaved changes
    pub level_dirty: bool,
}

impl LevelEditState {
    pub fn new() -> Self {
        Self {
            current_level: None,
            level_undo: UndoStack::new(),
            level_dirty: false,
        }
    }

    /// Call before mutating the level to record a snapshot for undo.
    pub fn push_level_undo(&mut self) {
        if let Some(level) = &self.current_level {
            self.level_undo.push(level.clone());
            self.level_dirty = true;
        }
    }
}

impl Default for LevelEditState {
    fn default() -> Self {
        Self::new()
    }
}
