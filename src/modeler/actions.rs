//! Modeler Action Definitions
//!
//! Defines all actions available in the model editor, with their shortcuts,
//! icons, and enable conditions.

use macroquad::prelude::*;
use crate::ui::{Action, ActionRegistry, ActionContext, Shortcut, icon};

/// Custom flags for modeler-specific conditions
pub mod flags {
    /// In face selection mode
    pub const FACE_MODE: u32 = 1 << 0;
    /// In vertex selection mode
    pub const VERTEX_MODE: u32 = 1 << 1;
    /// In edge selection mode
    pub const EDGE_MODE: u32 = 1 << 2;
    /// Has a mesh loaded
    pub const HAS_MESH: u32 = 1 << 5;
    /// Currently dragging/transforming
    pub const DRAGGING: u32 = 1 << 6;
    /// In paint mode
    pub const PAINT_MODE: u32 = 1 << 7;
    /// UV editor has focus (mouse inside canvas, UV mode active)
    pub const UV_EDITOR_FOCUSED: u32 = 1 << 8;
}

/// Create the complete action registry for the modeler
pub fn create_modeler_actions() -> ActionRegistry {
    let mut registry = ActionRegistry::new();

    // ========================================================================
    // File Actions
    // ========================================================================
    registry.register(
        Action::new("file.new")
            .label("New")
            .shortcut(Shortcut::ctrl(KeyCode::N))
            .icon(icon::FILE_PLUS)
            .status_tip("Create a new model")
            .category("File"),
    );

    registry.register(
        Action::new("file.open")
            .label("Open")
            .shortcut(Shortcut::ctrl(KeyCode::O))
            .icon(icon::FOLDER_OPEN)
            .status_tip("Open an existing model")
            .category("File"),
    );

    registry.register(
        Action::new("file.save")
            .label("Save")
            .shortcut(Shortcut::ctrl(KeyCode::S))
            .icon(icon::SAVE)
            .status_tip("Save the current model")
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

    registry.register(
        Action::new("file.browse_models")
            .label("Browse Models")
            .icon(icon::LAYERS)
            .status_tip("Open model browser")
            .category("File"),
    );

    registry.register(
        Action::new("file.browse_meshes")
            .label("Import OBJ")
            .icon(icon::FOLDER_OPEN)
            .status_tip("Import mesh from OBJ file")
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

    // Also register Ctrl+Y for redo (Windows convention)
    registry.register(
        Action::new("edit.redo_alt")
            .label("Redo")
            .shortcut(Shortcut::ctrl(KeyCode::Y))
            .category("Edit")
            .enabled_when(|ctx| ctx.can_redo),
    );

    registry.register(
        Action::new("edit.delete")
            .label("Delete")
            .shortcut(Shortcut::key(KeyCode::Delete))
            .status_tip("Delete selection")
            .category("Edit")
            .enabled_when(|ctx| ctx.has_selection),
    );

    // Also support backspace for delete
    registry.register(
        Action::new("edit.delete_alt")
            .label("Delete")
            .shortcut(Shortcut::key(KeyCode::Backspace))
            .category("Edit")
            .enabled_when(|ctx| ctx.has_selection),
    );

    // ========================================================================
    // Selection Mode Actions (1/2/3 keys)
    // ========================================================================
    registry.register(
        Action::new("select.vertex_mode")
            .label("Vertex Mode")
            .shortcut(Shortcut::key(KeyCode::Key1))
            .icon(icon::CIRCLE_DOT)
            .status_tip("Switch to vertex selection mode")
            .category("Selection")
            .checked_when(|ctx| ctx.has_flag(flags::VERTEX_MODE)),
    );

    registry.register(
        Action::new("select.edge_mode")
            .label("Edge Mode")
            .shortcut(Shortcut::key(KeyCode::Key2))
            .status_tip("Switch to edge selection mode")
            .category("Selection")
            .checked_when(|ctx| ctx.has_flag(flags::EDGE_MODE)),
    );

    registry.register(
        Action::new("select.face_mode")
            .label("Face Mode")
            .shortcut(Shortcut::key(KeyCode::Key3))
            .icon(icon::SCAN)
            .status_tip("Switch to face selection mode")
            .category("Selection")
            .checked_when(|ctx| ctx.has_flag(flags::FACE_MODE)),
    );

    registry.register(
        Action::new("select.all")
            .label("Select All")
            .shortcut(Shortcut::ctrl(KeyCode::A))
            .status_tip("Select all elements in current mode")
            .category("Selection")
            .enabled_when(|ctx| !ctx.has_flag(flags::UV_EDITOR_FOCUSED)),
    );

    registry.register(
        Action::new("select.loop")
            .label("Select Loop")
            .shortcut(Shortcut::alt(KeyCode::L))
            .status_tip("Select edge/face loop from selection (Alt+L)")
            .category("Selection")
            .enabled_when(|ctx| ctx.has_selection),
    );

    // ========================================================================
    // Transform Actions (G/R/T - similar to Blender but T for scale since S is camera strafe)
    // ========================================================================
    registry.register(
        Action::new("transform.grab")
            .label("Grab/Move")
            .shortcut(Shortcut::key(KeyCode::G))
            .icon(icon::MOVE)
            .status_tip("Move selection (G)")
            .category("Transform")
            .enabled_when(|ctx| ctx.has_selection),
    );

    registry.register(
        Action::new("transform.rotate")
            .label("Rotate")
            .shortcut(Shortcut::key(KeyCode::R))
            .icon(icon::ROTATE_3D)
            .status_tip("Rotate selection (R)")
            .category("Transform")
            .enabled_when(|ctx| ctx.has_selection),
    );

    registry.register(
        Action::new("transform.scale")
            .label("Scale")
            .shortcut(Shortcut::key(KeyCode::T))
            .icon(icon::SCALE_3D)
            .status_tip("Scale selection")
            .category("Transform")
            .enabled_when(|ctx| ctx.has_selection),
    );

    registry.register(
        Action::new("transform.extrude")
            .label("Extrude")
            .shortcut(Shortcut::key(KeyCode::E))
            .icon(icon::UNFOLD_VERTICAL)
            .status_tip("Extrude selected faces (E)")
            .category("Transform")
            .enabled_when(|ctx| ctx.has_face_selection),
    );

    // ========================================================================
    // View Actions
    // ========================================================================
    registry.register(
        Action::new("view.toggle_fullscreen")
            .label("Toggle Fullscreen Viewport")
            .shortcut(Shortcut::key(KeyCode::Space))
            .status_tip("Expand active viewport to full screen")
            .category("View"),
    );

    registry.register(
        Action::new("view.toggle_wireframe")
            .label("Toggle Wireframe")
            .shortcut(Shortcut::shift(KeyCode::Z))
            .status_tip("Toggle wireframe overlay (Shift+Z)")
            .category("View"),
    );

    registry.register(
        Action::new("view.toggle_xray")
            .label("Toggle X-Ray")
            .shortcut(Shortcut::alt(KeyCode::Z))
            .status_tip("See and select through geometry (Alt+Z)")
            .category("View"),
    );

    // ========================================================================
    // Mesh Cleanup Actions
    // ========================================================================
    registry.register(
        Action::new("mesh.merge_by_distance")
            .label("Merge by Distance")
            .shortcut(Shortcut::key(KeyCode::M))
            .status_tip("Merge overlapping vertices (M)")
            .category("Mesh"),
    );

    registry.register(
        Action::new("mesh.merge_to_center")
            .label("Merge to Center")
            .shortcut(Shortcut::alt(KeyCode::M))
            .status_tip("Merge selected vertices to center (Alt+M)")
            .category("Mesh")
            .enabled_when(|ctx| ctx.has_vertex_selection),
    );

    registry.register(
        Action::new("view.cycle_shading")
            .label("Cycle Shading")
            .shortcut(Shortcut::key(KeyCode::L))
            .icon(icon::SUN)
            .status_tip("Cycle through shading modes (None/Flat/Gouraud)")
            .category("View"),
    );

    // ========================================================================
    // UV/Texture Actions
    // ========================================================================
    registry.register(
        Action::new("uv.flip_horizontal")
            .label("Flip U")
            .shortcut(Shortcut::key(KeyCode::H))
            .icon(icon::FLIP_HORIZONTAL)
            .status_tip("Flip UVs horizontally")
            .category("UV")
            .enabled_when(|ctx| ctx.has_face_selection),
    );

    registry.register(
        Action::new("uv.flip_vertical")
            .label("Flip V")
            .shortcut(Shortcut::shift(KeyCode::H))
            .icon(icon::FLIP_VERTICAL)
            .status_tip("Flip UVs vertically")
            .category("UV")
            .enabled_when(|ctx| ctx.has_face_selection),
    );

    registry.register(
        Action::new("uv.rotate_cw")
            .label("Rotate UV CW")
            .icon(icon::ROTATE_CW)
            .status_tip("Rotate UVs clockwise 90°")
            .category("UV")
            .enabled_when(|ctx| ctx.has_face_selection),
    );

    registry.register(
        Action::new("uv.reset")
            .label("Reset UVs")
            .icon(icon::REFRESH_CW)
            .status_tip("Reset UVs to default")
            .category("UV")
            .enabled_when(|ctx| ctx.has_face_selection),
    );

    // ========================================================================
    // Context Menu Actions
    // ========================================================================
    registry.register(
        Action::new("context.open_menu")
            .label("Open Context Menu")
            .shortcut(Shortcut::key(KeyCode::Tab))
            .status_tip("Open context menu for adding primitives")
            .category("Context"),
    );

    registry.register(
        Action::new("context.close")
            .label("Close/Cancel")
            .shortcut(Shortcut::key(KeyCode::Escape))
            .status_tip("Close menu or cancel current operation")
            .category("Context"),
    );

    // ========================================================================
    // Axis Constraints (during transforms)
    // ========================================================================
    registry.register(
        Action::new("axis.constrain_x")
            .label("Constrain to X")
            .shortcut(Shortcut::key(KeyCode::X))
            .status_tip("Constrain transform to X axis")
            .category("Transform")
            .enabled_when(|ctx| ctx.has_flag(flags::DRAGGING)),
    );

    registry.register(
        Action::new("axis.constrain_y")
            .label("Constrain to Y")
            .shortcut(Shortcut::key(KeyCode::Y))
            .status_tip("Constrain transform to Y axis")
            .category("Transform")
            .enabled_when(|ctx| ctx.has_flag(flags::DRAGGING)),
    );

    // Note: Z key is also used for snap toggle, context determines which applies
    registry.register(
        Action::new("axis.constrain_z")
            .label("Constrain to Z")
            .shortcut(Shortcut::key(KeyCode::Z))
            .status_tip("Constrain transform to Z axis")
            .category("Transform")
            .enabled_when(|ctx| ctx.has_flag(flags::DRAGGING)),
    );

    // ========================================================================
    // Snap Settings
    // ========================================================================
    registry.register(
        Action::new("snap.toggle")
            .label("Disable Snap (Hold)")
            .shortcut(Shortcut::key(KeyCode::Z))
            .icon(icon::MAGNET)
            .status_tip("Hold Z to temporarily disable grid snapping")
            .category("Snap"),
    );

    // ========================================================================
    // Paint Mode Actions
    // ========================================================================
    registry.register(
        Action::new("brush.square")
            .label("Square Brush")
            .shortcut(Shortcut::key(KeyCode::B))
            .status_tip("Switch to square brush")
            .category("Paint")
            .enabled_when(|ctx| ctx.has_flag(flags::PAINT_MODE)),
    );

    registry.register(
        Action::new("brush.fill")
            .label("Fill Brush")
            .shortcut(Shortcut::key(KeyCode::F))
            .status_tip("Switch to fill brush")
            .category("Paint")
            .enabled_when(|ctx| ctx.has_flag(flags::PAINT_MODE)),
    );

    // ========================================================================
    // Mesh Operations
    // ========================================================================
    registry.register(
        Action::new("mesh.toggle_vertex_linking")
            .label("Toggle Vertex Linking")
            .icon(icon::LINK)
            .status_tip("Link coincident vertices when moving")
            .category("Mesh")
            .enabled_when(|ctx| ctx.has_flag(flags::VERTEX_MODE)),
    );

    // ========================================================================
    // Arrow Key Movement (PicoCAD-style)
    // ========================================================================
    registry.register(
        Action::new("move.left")
            .label("Move Left")
            .shortcut(Shortcut::key(KeyCode::Left))
            .status_tip("Move selection left by grid unit")
            .category("Move")
            .enabled_when(|ctx| ctx.has_selection),
    );

    registry.register(
        Action::new("move.right")
            .label("Move Right")
            .shortcut(Shortcut::key(KeyCode::Right))
            .status_tip("Move selection right by grid unit")
            .category("Move")
            .enabled_when(|ctx| ctx.has_selection),
    );

    registry.register(
        Action::new("move.up")
            .label("Move Up")
            .shortcut(Shortcut::key(KeyCode::Up))
            .status_tip("Move selection up by grid unit")
            .category("Move")
            .enabled_when(|ctx| ctx.has_selection),
    );

    registry.register(
        Action::new("move.down")
            .label("Move Down")
            .shortcut(Shortcut::key(KeyCode::Down))
            .status_tip("Move selection down by grid unit")
            .category("Move")
            .enabled_when(|ctx| ctx.has_selection),
    );

    // Shift variants for half-grid movement
    registry.register(
        Action::new("move.left_small")
            .label("Move Left (Small)")
            .shortcut(Shortcut::shift(KeyCode::Left))
            .status_tip("Move selection left by half grid unit")
            .category("Move")
            .enabled_when(|ctx| ctx.has_selection),
    );

    registry.register(
        Action::new("move.right_small")
            .label("Move Right (Small)")
            .shortcut(Shortcut::shift(KeyCode::Right))
            .status_tip("Move selection right by half grid unit")
            .category("Move")
            .enabled_when(|ctx| ctx.has_selection),
    );

    registry.register(
        Action::new("move.up_small")
            .label("Move Up (Small)")
            .shortcut(Shortcut::shift(KeyCode::Up))
            .status_tip("Move selection up by half grid unit")
            .category("Move")
            .enabled_when(|ctx| ctx.has_selection),
    );

    registry.register(
        Action::new("move.down_small")
            .label("Move Down (Small)")
            .shortcut(Shortcut::shift(KeyCode::Down))
            .status_tip("Move selection down by half grid unit")
            .category("Move")
            .enabled_when(|ctx| ctx.has_selection),
    );

    registry
}

/// Build an ActionContext from the current modeler state
/// This should be called each frame before processing actions
pub fn build_context(
    can_undo: bool,
    can_redo: bool,
    has_selection: bool,
    has_face_selection: bool,
    has_vertex_selection: bool,
    select_mode: &str, // "vertex", "edge", or "face"
    text_editing: bool,
    is_dirty: bool,
    is_dragging: bool,
    is_paint_mode: bool,
    uv_editor_focused: bool,
) -> ActionContext {
    let mut ctx = ActionContext {
        can_undo,
        can_redo,
        has_selection,
        has_clipboard: false, // Modeler doesn't use clipboard yet
        mode: "modeler",
        text_editing,
        has_face_selection,
        has_vertex_selection,
        is_dirty,
        flags: 0,
    };

    match select_mode {
        "vertex" => ctx.flags |= flags::VERTEX_MODE,
        "edge" => ctx.flags |= flags::EDGE_MODE,
        "face" => ctx.flags |= flags::FACE_MODE,
        _ => {}
    }

    if is_dragging {
        ctx.flags |= flags::DRAGGING;
    }

    if is_paint_mode {
        ctx.flags |= flags::PAINT_MODE;
    }

    if uv_editor_focused {
        ctx.flags |= flags::UV_EDITOR_FOCUSED;
    }

    ctx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modeler_actions_registered() {
        let registry = create_modeler_actions();

        // Check some key actions exist
        assert!(registry.get("file.save").is_some());
        assert!(registry.get("edit.undo").is_some());
        assert!(registry.get("transform.grab").is_some());
        assert!(registry.get("select.face_mode").is_some());
    }

    #[test]
    fn test_action_enable_conditions() {
        let registry = create_modeler_actions();

        // Undo should be disabled when can't undo
        let ctx = ActionContext {
            can_undo: false,
            ..Default::default()
        };
        assert!(!registry.is_enabled("edit.undo", &ctx));

        // Undo should be enabled when can undo
        let ctx2 = ActionContext {
            can_undo: true,
            ..Default::default()
        };
        assert!(registry.is_enabled("edit.undo", &ctx2));

        // Extrude requires face selection
        let ctx3 = ActionContext {
            has_selection: true,
            has_face_selection: false,
            ..Default::default()
        };
        assert!(!registry.is_enabled("transform.extrude", &ctx3));

        let ctx4 = ActionContext {
            has_selection: true,
            has_face_selection: true,
            ..Default::default()
        };
        assert!(registry.is_enabled("transform.extrude", &ctx4));
    }

    #[test]
    fn test_toggle_checked_state() {
        let registry = create_modeler_actions();

        // Face mode should show as checked when in face mode
        let ctx = build_context(
            false, false, false, false, false, "face", false, false, false, false, false
        );
        assert!(registry.is_checked("select.face_mode", &ctx));
        assert!(!registry.is_checked("select.vertex_mode", &ctx));

        // Texture mode toggle - use text_editing=true since that's what toggles the mode
        let ctx2 = build_context(
            false, false, false, false, false, "face", true, false, false, false, false
        );
        assert!(registry.is_checked("view.toggle_mode", &ctx2));
    }

    #[test]
    fn test_axis_constraint_conditions() {
        let registry = create_modeler_actions();

        // Axis constraints should only be enabled when dragging
        let ctx_not_dragging = build_context(
            false, false, true, false, false, "vertex", false, false, false, false, false
        );
        assert!(!registry.is_enabled("axis.constrain_x", &ctx_not_dragging));

        let ctx_dragging = build_context(
            false, false, true, false, false, "vertex", false, false, true, false, false
        );
        assert!(registry.is_enabled("axis.constrain_x", &ctx_dragging));
        assert!(registry.is_enabled("axis.constrain_y", &ctx_dragging));
        assert!(registry.is_enabled("axis.constrain_z", &ctx_dragging));
    }

    #[test]
    fn test_paint_mode_conditions() {
        let registry = create_modeler_actions();

        // Brush actions should only be enabled in paint mode
        let ctx_not_paint = build_context(
            false, false, false, false, false, "face", false, false, false, false, false
        );
        assert!(!registry.is_enabled("brush.square", &ctx_not_paint));

        let ctx_paint = build_context(
            false, false, false, false, false, "face", false, false, false, true, false
        );
        assert!(registry.is_enabled("brush.square", &ctx_paint));
        assert!(registry.is_enabled("brush.fill", &ctx_paint));
    }

    #[test]
    fn test_select_all_uv_editor_focused() {
        let registry = create_modeler_actions();

        // Select all should be disabled when UV editor is focused
        let ctx_no_uv = build_context(
            false, false, false, false, false, "face", false, false, false, false, false
        );
        assert!(registry.is_enabled("select.all", &ctx_no_uv));

        let ctx_uv_focused = build_context(
            false, false, false, false, false, "face", false, false, false, false, true
        );
        assert!(!registry.is_enabled("select.all", &ctx_uv_focused));
    }
}
