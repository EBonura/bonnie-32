//! Scale Tool
//!
//! Scale selection using:
//! - Gizmo drag (click on axis handle)
//! - S key for modal scaling (Blender-style)
//! - X/Y/Z to constrain to axis (or uniform if no constraint)

use crate::ui::{Tool, ToolController, InputState, DragAcceptResult, Axis};

/// Scale tool state
#[derive(Debug, Clone, Default)]
pub struct ScaleTool {
    /// Whether this tool is active
    active: bool,
    /// Currently hovered gizmo axis (None = uniform scale center)
    pub hovered_axis: Option<Axis>,
    /// Whether currently dragging
    dragging: bool,
    /// Axis constraint during drag (None = uniform)
    pub drag_axis: Option<Axis>,
}

impl ScaleTool {
    /// Create a new ScaleTool
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

    /// Set axis constraint during drag (X/Y/Z key pressed, None = uniform)
    pub fn set_axis_constraint(&mut self, axis: Option<Axis>) {
        if self.dragging {
            self.drag_axis = axis;
        }
    }
}

impl Tool for ScaleTool {
    fn id(&self) -> &'static str { "scale" }
    fn label(&self) -> &'static str { "Scale (T)" }
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

impl ToolController for ScaleTool {
    fn accept_mouse_drag(&mut self, input: &InputState) -> DragAcceptResult {
        // Scale can start with hovered axis (constrained) or without (uniform)
        if input.left_pressed {
            self.start_drag(self.hovered_axis);
            DragAcceptResult::Started
        } else {
            DragAcceptResult::None
        }
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
