//! Extrude Tool
//!
//! Extrude selected faces using:
//! - E key to start extrusion
//! - Mouse movement to set extrusion distance
//! - Click to confirm, Escape to cancel

use crate::ui::{Tool, ToolController, InputState, DragAcceptResult};

/// Extrusion tool state
#[derive(Debug, Clone, Default)]
pub struct ExtrudeTool {
    /// Whether this tool is active
    active: bool,
    /// Whether currently extruding (dragging after E key)
    extruding: bool,
}

impl ExtrudeTool {
    /// Create a new ExtrudeTool
    pub fn new() -> Self {
        Self {
            active: false,
            extruding: false,
        }
    }

    /// Check if currently extruding
    pub fn is_extruding(&self) -> bool {
        self.extruding
    }

    /// Start extrusion (called when E key is pressed)
    pub fn start_extrude(&mut self) {
        self.extruding = true;
    }

    /// End extrusion
    pub fn end_extrude(&mut self) {
        self.extruding = false;
    }
}

impl Tool for ExtrudeTool {
    fn id(&self) -> &'static str { "extrude" }
    fn label(&self) -> &'static str { "Extrude (E)" }
    fn active(&self) -> bool { self.active }

    fn do_activate(&mut self) -> bool {
        self.active = true;
        true
    }

    fn do_deactivate(&mut self) -> bool {
        self.active = false;
        self.extruding = false;
        true
    }
}

impl ToolController for ExtrudeTool {
    fn accept_mouse_drag(&mut self, input: &InputState) -> DragAcceptResult {
        // Extrusion is typically started by E key, not mouse drag
        // But we can support drag-to-extrude if tool is active
        if input.left_pressed && self.active && !self.extruding {
            self.start_extrude();
            DragAcceptResult::Started
        } else {
            DragAcceptResult::None
        }
    }

    fn cancel(&mut self) -> bool {
        if self.extruding {
            self.end_extrude();
            true
        } else {
            false
        }
    }
}
