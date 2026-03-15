//! Move Tool
//!
//! Translate selection using:
//! - Gizmo drag (click on axis arrow)
//! - G key for modal grab (Blender-style)
//! - X/Y/Z to constrain to axis

use crate::ui::{Tool, ToolController, InputState, DragAcceptResult, Axis};

/// Move/translate tool state
#[derive(Debug, Clone, Default)]
pub struct MoveTool {
    /// Whether this tool is active
    active: bool,
    /// Currently hovered gizmo axis (for highlighting)
    pub hovered_axis: Option<Axis>,
    /// Whether currently dragging
    dragging: bool,
    /// Axis constraint during drag
    pub drag_axis: Option<Axis>,
}

impl MoveTool {
    /// Create a new MoveTool
    pub fn new() -> Self {
        Self {
            active: false,
            hovered_axis: None,
            dragging: false,
            drag_axis: None,
        }
    }

    /// Set the hovered axis (called during mouse move over gizmo)
    pub fn set_hovered_axis(&mut self, axis: Option<Axis>) {
        if !self.dragging {
            self.hovered_axis = axis;
        }
    }

    /// Check if currently dragging
    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    /// Start a drag operation
    pub fn start_drag(&mut self, axis: Option<Axis>) {
        self.dragging = true;
        self.drag_axis = axis;
    }

    /// End the drag operation
    pub fn end_drag(&mut self) {
        self.dragging = false;
        self.drag_axis = None;
    }

    /// Set axis constraint during drag (X/Y/Z key pressed)
    pub fn set_axis_constraint(&mut self, axis: Option<Axis>) {
        if self.dragging {
            self.drag_axis = axis;
        }
    }
}

impl Tool for MoveTool {
    fn id(&self) -> &'static str { "move" }
    fn label(&self) -> &'static str { "Move (G)" }
    fn active(&self) -> bool { self.active }

    fn do_activate(&mut self) -> bool {
        self.active = true;
        true
    }

    fn do_deactivate(&mut self) -> bool {
        self.active = false;
        self.hovered_axis = None;
        self.dragging = false;
        self.drag_axis = None;
        true
    }
}

impl ToolController for MoveTool {
    fn mouse_move(&mut self, _input: &InputState) {
        // Gizmo hover detection is done externally (needs 3D projection)
        // This is called to allow the tool to respond to movement
    }

    fn accept_mouse_drag(&mut self, input: &InputState) -> DragAcceptResult {
        if input.left_pressed && self.hovered_axis.is_some() {
            self.start_drag(self.hovered_axis);
            DragAcceptResult::Started
        } else {
            DragAcceptResult::None
        }
    }

    fn modifier_key_change(&mut self, _input: &InputState) {
        // Axis constraints are handled by the viewport based on X/Y/Z key presses
    }

    fn cancel(&mut self) -> bool {
        if self.dragging {
            self.end_drag();
            true
        } else {
            false
        }
    }
}
