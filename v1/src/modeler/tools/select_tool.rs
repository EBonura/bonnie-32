//! Select Tool
//!
//! Click to select vertices/edges/faces. Supports:
//! - Click to select single element
//! - Shift+click to add to selection
//! - Ctrl+click to remove from selection
//! - Click+drag for box selection

use crate::ui::{Tool, ToolController, InputState, DragAcceptResult};

/// Selection tool state
#[derive(Debug, Clone, Default)]
pub struct SelectTool {
    /// Whether this tool is active
    active: bool,
    /// Whether a box selection drag is in progress
    box_selecting: bool,
}

impl SelectTool {
    /// Create a new SelectTool
    pub fn new() -> Self {
        Self {
            active: false,
            box_selecting: false,
        }
    }

    /// Check if currently box selecting
    pub fn is_box_selecting(&self) -> bool {
        self.box_selecting
    }

    /// Start box selection
    pub fn start_box_select(&mut self) {
        self.box_selecting = true;
    }

    /// End box selection
    pub fn end_box_select(&mut self) {
        self.box_selecting = false;
    }
}

impl Tool for SelectTool {
    fn id(&self) -> &'static str { "select" }
    fn label(&self) -> &'static str { "Select" }
    fn active(&self) -> bool { self.active }

    fn do_activate(&mut self) -> bool {
        self.active = true;
        true
    }

    fn do_deactivate(&mut self) -> bool {
        self.active = false;
        self.box_selecting = false;
        true
    }
}

impl ToolController for SelectTool {
    fn accept_mouse_drag(&mut self, input: &InputState) -> DragAcceptResult {
        if input.left_pressed && !input.modifiers.alt {
            // Start box selection (Alt+drag is for camera orbit)
            self.start_box_select();
            DragAcceptResult::Started
        } else {
            DragAcceptResult::None
        }
    }

    fn cancel(&mut self) -> bool {
        if self.box_selecting {
            self.box_selecting = false;
            true
        } else {
            false
        }
    }
}
