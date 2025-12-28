//! Rotate Tool
//!
//! Rotate selection using:
//! - Gizmo drag (click on axis ring)
//! - R key for modal rotation (Blender-style)
//! - X/Y/Z to constrain to axis

use crate::ui::{Tool, ToolController, InputState, DragAcceptResult, Axis};

/// Rotation tool state
#[derive(Debug, Clone, Default)]
pub struct RotateTool {
    /// Whether this tool is active
    active: bool,
    /// Currently hovered gizmo axis (for highlighting)
    pub hovered_axis: Option<Axis>,
    /// Whether currently dragging
    dragging: bool,
    /// Axis constraint during drag
    pub drag_axis: Option<Axis>,
    /// Initial angle when drag started (radians)
    pub initial_angle: f32,
}

impl RotateTool {
    /// Create a new RotateTool
    pub fn new() -> Self {
        Self {
            active: false,
            hovered_axis: None,
            dragging: false,
            drag_axis: None,
            initial_angle: 0.0,
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
    pub fn start_drag(&mut self, axis: Option<Axis>, initial_angle: f32) {
        self.dragging = true;
        self.drag_axis = axis;
        self.initial_angle = initial_angle;
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

impl Tool for RotateTool {
    fn id(&self) -> &'static str { "rotate" }
    fn label(&self) -> &'static str { "Rotate (R)" }
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

impl ToolController for RotateTool {
    fn accept_mouse_drag(&mut self, input: &InputState) -> DragAcceptResult {
        if input.left_pressed && self.hovered_axis.is_some() {
            // Initial angle would be computed from mouse position relative to center
            self.start_drag(self.hovered_axis, 0.0);
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
