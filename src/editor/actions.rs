//! World Editor Action Definitions
//!
//! Defines all actions available in the level/world editor, with their shortcuts,
//! icons, and enable conditions.

use macroquad::prelude::*;
use crate::ui::{Action, ActionRegistry, ActionContext, Shortcut, icon};

/// Custom flags for editor-specific conditions
pub mod flags {
    /// Has a room selected
    pub const ROOM_SELECTED: u32 = 1 << 0;
    /// Has a sector selected
    pub const SECTOR_SELECTED: u32 = 1 << 1;
    /// Has an object selected
    pub const OBJECT_SELECTED: u32 = 1 << 2;
    /// Has a portal selected
    pub const PORTAL_SELECTED: u32 = 1 << 3;
    /// In geometry editing mode
    pub const GEOMETRY_MODE: u32 = 1 << 4;
    /// In texture editing mode
    pub const TEXTURE_MODE: u32 = 1 << 5;
    /// In object placement mode
    pub const OBJECT_MODE: u32 = 1 << 6;
    /// Has level loaded
    pub const HAS_LEVEL: u32 = 1 << 7;
}

/// Create the complete action registry for the world editor
pub fn create_editor_actions() -> ActionRegistry {
    let mut registry = ActionRegistry::new();

    // ========================================================================
    // File Actions
    // ========================================================================
    registry.register(
        Action::new("file.new")
            .label("New Level")
            .shortcut(Shortcut::ctrl(KeyCode::N))
            .icon(icon::FILE_PLUS)
            .status_tip("Create a new level")
            .category("File"),
    );

    registry.register(
        Action::new("file.open")
            .label("Open Level")
            .shortcut(Shortcut::ctrl(KeyCode::O))
            .icon(icon::FOLDER_OPEN)
            .status_tip("Open an existing level")
            .category("File"),
    );

    registry.register(
        Action::new("file.save")
            .label("Save")
            .shortcut(Shortcut::ctrl(KeyCode::S))
            .icon(icon::SAVE)
            .status_tip("Save the current level")
            .category("File"),
    );

    registry.register(
        Action::new("file.save_as")
            .label("Save As...")
            .shortcut(Shortcut::ctrl_shift(KeyCode::S))
            .icon(icon::SAVE_AS)
            .status_tip("Save to a new file")
            .category("File"),
    );

    // ========================================================================
    // Edit Actions
    // ========================================================================
    registry.register(
        Action::new("edit.undo")
            .label("Undo")
            .shortcut(Shortcut::ctrl(KeyCode::Z))
            .icon(icon::UNDO)
            .status_tip("Undo last action")
            .category("Edit")
            .enabled_when(|ctx| ctx.can_undo),
    );

    registry.register(
        Action::new("edit.redo")
            .label("Redo")
            .shortcut(Shortcut::ctrl_shift(KeyCode::Z))
            .icon(icon::REDO)
            .status_tip("Redo last undone action")
            .category("Edit")
            .enabled_when(|ctx| ctx.can_redo),
    );

    registry.register(
        Action::new("edit.copy")
            .label("Copy")
            .shortcut(Shortcut::ctrl(KeyCode::C))
            .status_tip("Copy selected object")
            .category("Edit")
            .enabled_when(|ctx| ctx.has_flag(flags::OBJECT_SELECTED)),
    );

    registry.register(
        Action::new("edit.paste")
            .label("Paste")
            .shortcut(Shortcut::ctrl(KeyCode::V))
            .status_tip("Paste object at cursor")
            .category("Edit")
            .enabled_when(|ctx| ctx.has_clipboard),
    );

    registry.register(
        Action::new("edit.delete")
            .label("Delete")
            .shortcut(Shortcut::key(KeyCode::Delete))
            .status_tip("Delete selection")
            .category("Edit")
            .enabled_when(|ctx| ctx.has_selection),
    );

    // ========================================================================
    // Room Operations
    // ========================================================================
    registry.register(
        Action::new("room.add")
            .label("Add Room")
            .icon(icon::BOX)
            .status_tip("Add a new room to the level")
            .category("Room"),
    );

    registry.register(
        Action::new("room.delete")
            .label("Delete Room")
            .status_tip("Delete the selected room")
            .category("Room")
            .enabled_when(|ctx| ctx.has_flag(flags::ROOM_SELECTED)),
    );

    registry.register(
        Action::new("room.duplicate")
            .label("Duplicate Room")
            .status_tip("Duplicate the selected room")
            .category("Room")
            .enabled_when(|ctx| ctx.has_flag(flags::ROOM_SELECTED)),
    );

    // ========================================================================
    // Sector Operations
    // ========================================================================
    registry.register(
        Action::new("sector.raise_floor")
            .label("Raise Floor")
            .status_tip("Raise floor of selected sector")
            .category("Sector")
            .enabled_when(|ctx| ctx.has_flag(flags::SECTOR_SELECTED)),
    );

    registry.register(
        Action::new("sector.lower_floor")
            .label("Lower Floor")
            .status_tip("Lower floor of selected sector")
            .category("Sector")
            .enabled_when(|ctx| ctx.has_flag(flags::SECTOR_SELECTED)),
    );

    registry.register(
        Action::new("sector.raise_ceiling")
            .label("Raise Ceiling")
            .status_tip("Raise ceiling of selected sector")
            .category("Sector")
            .enabled_when(|ctx| ctx.has_flag(flags::SECTOR_SELECTED)),
    );

    registry.register(
        Action::new("sector.lower_ceiling")
            .label("Lower Ceiling")
            .status_tip("Lower ceiling of selected sector")
            .category("Sector")
            .enabled_when(|ctx| ctx.has_flag(flags::SECTOR_SELECTED)),
    );

    // ========================================================================
    // Portal Operations
    // ========================================================================
    registry.register(
        Action::new("portal.create")
            .label("Create Portal")
            .icon(icon::DOOR_CLOSED)
            .status_tip("Create a portal between rooms")
            .category("Portal")
            .enabled_when(|ctx| ctx.has_flag(flags::SECTOR_SELECTED)),
    );

    registry.register(
        Action::new("portal.delete")
            .label("Delete Portal")
            .status_tip("Remove the selected portal")
            .category("Portal")
            .enabled_when(|ctx| ctx.has_flag(flags::PORTAL_SELECTED)),
    );

    // ========================================================================
    // Object Operations
    // ========================================================================
    registry.register(
        Action::new("object.add")
            .label("Add Object")
            .icon(icon::MAP_PIN)
            .status_tip("Place a new object in the level")
            .category("Object"),
    );

    registry.register(
        Action::new("object.delete")
            .label("Delete Object")
            .status_tip("Remove the selected object")
            .category("Object")
            .enabled_when(|ctx| ctx.has_flag(flags::OBJECT_SELECTED)),
    );

    // ========================================================================
    // View Actions
    // ========================================================================
    registry.register(
        Action::new("view.toggle_grid")
            .label("Toggle Grid")
            .icon(icon::GRID)
            .status_tip("Show/hide the editing grid")
            .category("View"),
    );

    registry.register(
        Action::new("view.zoom_in")
            .label("Zoom In")
            .shortcut(Shortcut::key(KeyCode::Equal))
            .icon(icon::PLUS)
            .status_tip("Zoom in on the viewport")
            .category("View"),
    );

    registry.register(
        Action::new("view.zoom_out")
            .label("Zoom Out")
            .shortcut(Shortcut::key(KeyCode::Minus))
            .icon(icon::MINUS)
            .status_tip("Zoom out of the viewport")
            .category("View"),
    );

    registry
}

/// Build an ActionContext from the current editor state
pub fn build_context(
    can_undo: bool,
    can_redo: bool,
    has_selection: bool,
    has_clipboard: bool,
    selection_flags: u32, // ROOM_SELECTED, SECTOR_SELECTED, etc.
    text_editing: bool,
    is_dirty: bool,
) -> ActionContext {
    ActionContext {
        can_undo,
        can_redo,
        has_selection,
        has_clipboard,
        mode: "editor",
        text_editing,
        has_face_selection: false,
        has_vertex_selection: false,
        is_dirty,
        flags: selection_flags,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_editor_actions_registered() {
        let registry = create_editor_actions();

        assert!(registry.get("file.save").is_some());
        assert!(registry.get("edit.undo").is_some());
        assert!(registry.get("room.add").is_some());
        assert!(registry.get("portal.create").is_some());
    }

    #[test]
    fn test_portal_enable_conditions() {
        let registry = create_editor_actions();

        // Portal creation requires sector selected
        let ctx = build_context(false, false, true, false, 0, false, false);
        assert!(!registry.is_enabled("portal.create", &ctx));

        let ctx2 = build_context(false, false, true, false, flags::SECTOR_SELECTED, false, false);
        assert!(registry.is_enabled("portal.create", &ctx2));
    }
}
