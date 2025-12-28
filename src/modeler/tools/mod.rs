//! Modeler Tool System
//!
//! TrenchBroom-inspired tool management for the 3D modeler.
//!
//! # Tools
//!
//! - **Select**: Click to select vertices/edges/faces
//! - **Move**: Translate selection with gizmo or G key
//! - **Rotate**: Rotate selection with gizmo or R key
//! - **Scale**: Scale selection with gizmo or S key
//! - **Extrude**: Extrude faces with E key
//!
//! # Tool Groups
//!
//! Move, Rotate, Scale are mutually exclusive (only one gizmo at a time).
//! Select can coexist with transform tools.

mod select_tool;
mod move_tool;
mod rotate_tool;
mod scale_tool;
mod extrude_tool;

pub use select_tool::SelectTool;
pub use move_tool::MoveTool;
pub use rotate_tool::RotateTool;
pub use scale_tool::ScaleTool;
pub use extrude_tool::ExtrudeTool;

use crate::ui::{Tool, ToolBox, ToolRegistry};

/// Tool identifiers for the modeler
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelerToolId {
    Select,
    Move,
    Rotate,
    Scale,
    Extrude,
}

impl ModelerToolId {
    /// Get the string ID for this tool
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Select => "select",
            Self::Move => "move",
            Self::Rotate => "rotate",
            Self::Scale => "scale",
            Self::Extrude => "extrude",
        }
    }

    /// Get all tool IDs
    pub fn all() -> &'static [ModelerToolId] {
        &[
            Self::Select,
            Self::Move,
            Self::Rotate,
            Self::Scale,
            Self::Extrude,
        ]
    }
}

/// Registry containing all modeler tools (implements ToolRegistry)
pub struct ModelerTools {
    /// Selection tool
    pub select: SelectTool,
    /// Move/translate tool
    pub move_tool: MoveTool,
    /// Rotation tool
    pub rotate: RotateTool,
    /// Scale tool
    pub scale: ScaleTool,
    /// Extrusion tool
    pub extrude: ExtrudeTool,
}

impl ModelerTools {
    /// Create a new ModelerTools with all tools
    pub fn new() -> Self {
        Self {
            select: SelectTool::new(),
            move_tool: MoveTool::new(),
            rotate: RotateTool::new(),
            scale: ScaleTool::new(),
            extrude: ExtrudeTool::new(),
        }
    }

    /// Get the currently active transform tool (Move/Rotate/Scale), if any
    pub fn active_transform_tool(&self) -> Option<ModelerToolId> {
        if self.move_tool.active() {
            Some(ModelerToolId::Move)
        } else if self.rotate.active() {
            Some(ModelerToolId::Rotate)
        } else if self.scale.active() {
            Some(ModelerToolId::Scale)
        } else {
            None
        }
    }
}

impl Default for ModelerTools {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry for ModelerTools {
    fn get_tool_mut(&mut self, id: &str) -> Option<&mut dyn Tool> {
        match id {
            "select" => Some(&mut self.select),
            "move" => Some(&mut self.move_tool),
            "rotate" => Some(&mut self.rotate),
            "scale" => Some(&mut self.scale),
            "extrude" => Some(&mut self.extrude),
            _ => None,
        }
    }

    fn get_tool(&self, id: &str) -> Option<&dyn Tool> {
        match id {
            "select" => Some(&self.select),
            "move" => Some(&self.move_tool),
            "rotate" => Some(&self.rotate),
            "scale" => Some(&self.scale),
            "extrude" => Some(&self.extrude),
            _ => None,
        }
    }

    fn tool_ids(&self) -> Vec<&'static str> {
        vec!["select", "move", "rotate", "scale", "extrude"]
    }
}

/// Container combining tools with ToolBox management
///
/// This struct owns both the tool registry and the ToolBox.
/// Methods delegate to ToolBox, passing the registry as needed.
pub struct ModelerToolBox {
    /// Tool lifecycle manager
    pub tool_box: ToolBox,
    /// All modeler tools
    pub tools: ModelerTools,
}

impl ModelerToolBox {
    /// Create a new ModelerToolBox with default configuration
    pub fn new() -> Self {
        let mut tool_box = ToolBox::new();

        // Move, Rotate, Scale are mutually exclusive (only one gizmo at a time)
        tool_box.add_exclusive_group(&["move", "rotate", "scale"]);

        // Extrude suppresses transform tools while active
        tool_box.suppress_while_active("extrude", &["move", "rotate", "scale"]);

        Self {
            tool_box,
            tools: ModelerTools::new(),
        }
    }

    /// Get the currently active transform tool (Move/Rotate/Scale), if any
    pub fn active_transform_tool(&self) -> Option<ModelerToolId> {
        self.tools.active_transform_tool()
    }

    /// Activate a tool by ID
    pub fn activate(&mut self, tool_id: ModelerToolId) {
        self.tool_box.activate_tool(tool_id.as_str(), &mut self.tools);
    }

    /// Deactivate a tool by ID
    pub fn deactivate(&mut self, tool_id: ModelerToolId) {
        self.tool_box.deactivate_tool(tool_id.as_str(), &mut self.tools);
    }

    /// Toggle a tool by ID
    pub fn toggle(&mut self, tool_id: ModelerToolId) {
        self.tool_box.toggle_tool(tool_id.as_str(), &mut self.tools);
    }

    /// Check if a tool is active
    pub fn is_active(&self, tool_id: ModelerToolId) -> bool {
        self.tool_box.is_tool_active(tool_id.as_str())
    }

    /// Deactivate all tools
    pub fn deactivate_all(&mut self) {
        self.tool_box.deactivate_all(&mut self.tools);
    }
}

impl Default for ModelerToolBox {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exclusive_groups() {
        let mut mtb = ModelerToolBox::new();

        // Activate move
        mtb.activate(ModelerToolId::Move);
        assert!(mtb.tools.move_tool.active());
        assert!(!mtb.tools.rotate.active());
        assert!(!mtb.tools.scale.active());

        // Activate rotate - should deactivate move
        mtb.activate(ModelerToolId::Rotate);
        assert!(!mtb.tools.move_tool.active());
        assert!(mtb.tools.rotate.active());
        assert!(!mtb.tools.scale.active());

        // Activate scale - should deactivate rotate
        mtb.activate(ModelerToolId::Scale);
        assert!(!mtb.tools.move_tool.active());
        assert!(!mtb.tools.rotate.active());
        assert!(mtb.tools.scale.active());
    }

    #[test]
    fn test_extrude_suppression() {
        let mut mtb = ModelerToolBox::new();

        // Activate move first
        mtb.activate(ModelerToolId::Move);
        assert!(mtb.tools.move_tool.active());

        // Activate extrude - should suppress move
        mtb.activate(ModelerToolId::Extrude);
        assert!(mtb.tools.extrude.active());
        assert!(!mtb.tools.move_tool.active());

        // Deactivate extrude - should restore move
        mtb.deactivate(ModelerToolId::Extrude);
        assert!(!mtb.tools.extrude.active());
        assert!(mtb.tools.move_tool.active());
    }

    #[test]
    fn test_active_transform_tool() {
        let mut mtb = ModelerToolBox::new();

        assert_eq!(mtb.active_transform_tool(), None);

        mtb.activate(ModelerToolId::Move);
        assert_eq!(mtb.active_transform_tool(), Some(ModelerToolId::Move));

        mtb.activate(ModelerToolId::Rotate);
        assert_eq!(mtb.active_transform_tool(), Some(ModelerToolId::Rotate));

        mtb.activate(ModelerToolId::Scale);
        assert_eq!(mtb.active_transform_tool(), Some(ModelerToolId::Scale));
    }

    #[test]
    fn test_deactivate_all() {
        let mut mtb = ModelerToolBox::new();

        mtb.activate(ModelerToolId::Move);
        mtb.activate(ModelerToolId::Select);

        mtb.deactivate_all();

        assert!(!mtb.tools.move_tool.active());
        assert!(!mtb.tools.select.active());
    }
}
