//! ToolBox - Tool lifecycle manager
//!
//! Inspired by TrenchBroom's ToolBox. Manages:
//! - Modal tool stack (active tools in LIFO order)
//! - Exclusive groups (only one tool active per group)
//! - Tool suppression (tool A suppresses tools B, C while active)

use std::collections::{HashMap, HashSet};
use super::tool::ToolRegistry;

/// Manages tool activation, exclusive groups, and suppression
///
/// # Exclusive Groups
///
/// Tools in the same exclusive group cannot be active simultaneously.
/// When tool A is activated, all other tools in its groups are deactivated.
///
/// ```ignore
/// tool_box.add_exclusive_group(&["move", "rotate", "scale"]);
/// // Now only one of move/rotate/scale can be active at a time
/// ```
///
/// # Tool Suppression
///
/// A tool can suppress other tools while it's active. Suppressed tools
/// are deactivated when the primary tool activates, and reactivated
/// when the primary tool deactivates.
///
/// ```ignore
/// tool_box.suppress_while_active("vertex_mode", &["move", "extrude"]);
/// // When vertex_mode activates, move and extrude are suppressed
/// // When vertex_mode deactivates, they're restored
/// ```
#[derive(Debug, Clone, Default)]
pub struct ToolBox {
    /// Stack of active tools (most recently activated at back)
    pub(crate) modal_tool_stack: Vec<&'static str>,

    /// Groups where only one tool can be active at a time
    /// Each inner Vec contains tool IDs that are mutually exclusive
    exclusive_groups: Vec<Vec<&'static str>>,

    /// Map of tool ID -> tool IDs it suppresses while active
    suppressed_by: HashMap<&'static str, Vec<&'static str>>,

    /// Tools that were active before being suppressed (for restoration)
    suppressed_tools: HashSet<&'static str>,

    /// Whether the toolbox is enabled (disabled during drags, etc.)
    enabled: bool,
}

impl ToolBox {
    /// Create a new empty ToolBox
    pub fn new() -> Self {
        Self {
            modal_tool_stack: Vec::new(),
            exclusive_groups: Vec::new(),
            suppressed_by: HashMap::new(),
            suppressed_tools: HashSet::new(),
            enabled: true,
        }
    }

    /// Add an exclusive group - only one tool in the group can be active
    ///
    /// A tool can be in multiple exclusive groups. When activated, it will
    /// deactivate all tools in all groups it belongs to.
    pub fn add_exclusive_group(&mut self, tool_ids: &[&'static str]) {
        if tool_ids.len() > 1 {
            self.exclusive_groups.push(tool_ids.to_vec());
        }
    }

    /// Configure tool suppression - when `primary` is active, `suppressed` tools
    /// are temporarily deactivated
    ///
    /// Suppressed tools are automatically reactivated when the primary tool
    /// is deactivated.
    pub fn suppress_while_active(&mut self, primary: &'static str, suppressed: &[&'static str]) {
        self.suppressed_by
            .entry(primary)
            .or_default()
            .extend(suppressed.iter().copied());
    }

    /// Check if toolbox is enabled
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Enable the toolbox
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disable the toolbox (e.g., during drag operations)
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Get the currently active tool (top of modal stack)
    pub fn active_tool(&self) -> Option<&'static str> {
        self.modal_tool_stack.last().copied()
    }

    /// Check if a specific tool is active
    pub fn is_tool_active(&self, tool_id: &str) -> bool {
        self.modal_tool_stack.iter().any(|id| *id == tool_id)
    }

    /// Check if a tool is currently suppressed
    pub fn is_tool_suppressed(&self, tool_id: &str) -> bool {
        self.suppressed_tools.contains(tool_id)
    }

    /// Toggle a tool's active state
    ///
    /// If active, deactivates it. If inactive, activates it.
    pub fn toggle_tool(&mut self, tool_id: &'static str, registry: &mut dyn ToolRegistry) {
        if self.is_tool_active(tool_id) {
            self.deactivate_tool(tool_id, registry);
        } else {
            self.activate_tool(tool_id, registry);
        }
    }

    /// Activate a tool
    ///
    /// This will:
    /// 1. Deactivate all tools in exclusive groups with this tool
    /// 2. Suppress tools configured to be suppressed by this tool
    /// 3. Activate the tool and add it to the modal stack
    pub fn activate_tool(&mut self, tool_id: &'static str, registry: &mut dyn ToolRegistry) {
        if !self.enabled {
            return;
        }

        // Verify tool exists and isn't already active
        match registry.get_tool(tool_id) {
            Some(t) if !t.active() => {}
            _ => return, // Tool doesn't exist or is already active
        }

        // 1. Deactivate exclusive tools
        for excluded_id in self.excluded_tools(tool_id) {
            if let Some(excluded_tool) = registry.get_tool_mut(excluded_id) {
                if excluded_tool.active() {
                    self.deactivate_tool_internal(excluded_id, registry);
                }
            }
        }

        // 2. Compute and apply suppressions
        let previously_suppressed = self.currently_suppressed_tools();

        // 3. Activate the tool
        if let Some(tool) = registry.get_tool_mut(tool_id) {
            if tool.activate() {
                // Suppress newly suppressed tools
                let now_suppressed = self.tools_suppressed_by(tool_id);
                for suppressed_id in now_suppressed {
                    if !previously_suppressed.contains(suppressed_id) {
                        if let Some(suppressed_tool) = registry.get_tool_mut(suppressed_id) {
                            if suppressed_tool.active() {
                                suppressed_tool.deactivate();
                                self.suppressed_tools.insert(suppressed_id);
                                // Remove from modal stack but remember it was suppressed
                                self.modal_tool_stack.retain(|id| *id != suppressed_id);
                            }
                        }
                    }
                }

                self.modal_tool_stack.push(tool_id);
            }
        }
    }

    /// Deactivate a tool
    ///
    /// This will:
    /// 1. Deactivate the tool and remove it from the modal stack
    /// 2. Restore any tools that were suppressed by this tool
    pub fn deactivate_tool(&mut self, tool_id: &'static str, registry: &mut dyn ToolRegistry) {
        self.deactivate_tool_internal(tool_id, registry);
    }

    /// Internal deactivation (doesn't check enabled state)
    fn deactivate_tool_internal(&mut self, tool_id: &'static str, registry: &mut dyn ToolRegistry) {
        let previously_suppressed = self.currently_suppressed_tools();

        // Deactivate the tool
        if let Some(tool) = registry.get_tool_mut(tool_id) {
            if tool.active() {
                tool.deactivate();
            }
        }

        // Remove from modal stack
        self.modal_tool_stack.retain(|id| *id != tool_id);

        // Restore tools that are no longer suppressed
        let still_suppressed = self.currently_suppressed_tools();
        let to_restore: Vec<_> = previously_suppressed
            .difference(&still_suppressed)
            .copied()
            .collect();

        for restore_id in to_restore {
            if self.suppressed_tools.remove(restore_id) {
                if let Some(tool) = registry.get_tool_mut(restore_id) {
                    if tool.activate() {
                        self.modal_tool_stack.push(restore_id);
                    }
                }
            }
        }
    }

    /// Deactivate all tools
    pub fn deactivate_all(&mut self, registry: &mut dyn ToolRegistry) {
        let tool_ids: Vec<_> = self.modal_tool_stack.clone();
        for tool_id in tool_ids {
            self.deactivate_tool_internal(tool_id, registry);
        }
        self.suppressed_tools.clear();
    }

    /// Get all tools that should be deactivated when `tool_id` is activated
    fn excluded_tools(&self, tool_id: &str) -> HashSet<&'static str> {
        let mut result = HashSet::new();

        for group in &self.exclusive_groups {
            if group.iter().any(|id| *id == tool_id) {
                result.extend(group.iter().copied());
            }
        }

        // Don't exclude self
        result.remove(tool_id);
        result
    }

    /// Get tools suppressed by a specific tool
    fn tools_suppressed_by(&self, tool_id: &str) -> HashSet<&'static str> {
        self.suppressed_by
            .get(tool_id)
            .map(|v| v.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Compute currently suppressed tools based on active tools
    fn currently_suppressed_tools(&self) -> HashSet<&'static str> {
        let mut result = HashSet::new();

        for active_id in &self.modal_tool_stack {
            if let Some(suppressed) = self.suppressed_by.get(active_id) {
                result.extend(suppressed.iter().copied());
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::tool::Tool;

    struct TestTool {
        id: &'static str,
        active: bool,
    }

    impl TestTool {
        fn new(id: &'static str) -> Self {
            Self { id, active: false }
        }
    }

    impl crate::ui::tool::Tool for TestTool {
        fn id(&self) -> &'static str { self.id }
        fn label(&self) -> &'static str { self.id }
        fn active(&self) -> bool { self.active }

        fn do_activate(&mut self) -> bool {
            self.active = true;
            true
        }

        fn do_deactivate(&mut self) -> bool {
            self.active = false;
            true
        }
    }

    struct TestRegistry {
        select: TestTool,
        move_tool: TestTool,
        rotate: TestTool,
        scale: TestTool,
    }

    impl TestRegistry {
        fn new() -> Self {
            Self {
                select: TestTool::new("select"),
                move_tool: TestTool::new("move"),
                rotate: TestTool::new("rotate"),
                scale: TestTool::new("scale"),
            }
        }
    }

    impl ToolRegistry for TestRegistry {
        fn get_tool_mut(&mut self, id: &str) -> Option<&mut dyn crate::ui::tool::Tool> {
            match id {
                "select" => Some(&mut self.select),
                "move" => Some(&mut self.move_tool),
                "rotate" => Some(&mut self.rotate),
                "scale" => Some(&mut self.scale),
                _ => None,
            }
        }

        fn get_tool(&self, id: &str) -> Option<&dyn crate::ui::tool::Tool> {
            match id {
                "select" => Some(&self.select),
                "move" => Some(&self.move_tool),
                "rotate" => Some(&self.rotate),
                "scale" => Some(&self.scale),
                _ => None,
            }
        }

        fn tool_ids(&self) -> Vec<&'static str> {
            vec!["select", "move", "rotate", "scale"]
        }
    }

    #[test]
    fn test_exclusive_groups() {
        let mut tool_box = ToolBox::new();
        let mut registry = TestRegistry::new();

        // Move, rotate, scale are mutually exclusive
        tool_box.add_exclusive_group(&["move", "rotate", "scale"]);

        // Activate move
        tool_box.activate_tool("move", &mut registry);
        assert!(registry.move_tool.active());
        assert!(!registry.rotate.active());

        // Activate rotate - should deactivate move
        tool_box.activate_tool("rotate", &mut registry);
        assert!(!registry.move_tool.active());
        assert!(registry.rotate.active());

        // Activate scale - should deactivate rotate
        tool_box.activate_tool("scale", &mut registry);
        assert!(!registry.rotate.active());
        assert!(registry.scale.active());
    }

    #[test]
    fn test_suppression() {
        let mut tool_box = ToolBox::new();
        let mut registry = TestRegistry::new();

        // Select suppresses move while active
        tool_box.suppress_while_active("select", &["move"]);

        // Activate move first
        tool_box.activate_tool("move", &mut registry);
        assert!(registry.move_tool.active());

        // Activate select - should suppress move
        tool_box.activate_tool("select", &mut registry);
        assert!(registry.select.active());
        assert!(!registry.move_tool.active());
        assert!(tool_box.is_tool_suppressed("move"));

        // Deactivate select - move should be restored
        tool_box.deactivate_tool("select", &mut registry);
        assert!(!registry.select.active());
        assert!(registry.move_tool.active());
        assert!(!tool_box.is_tool_suppressed("move"));
    }

    #[test]
    fn test_toggle() {
        let mut tool_box = ToolBox::new();
        let mut registry = TestRegistry::new();

        tool_box.toggle_tool("move", &mut registry);
        assert!(registry.move_tool.active());

        tool_box.toggle_tool("move", &mut registry);
        assert!(!registry.move_tool.active());
    }

    #[test]
    fn test_deactivate_all() {
        let mut tool_box = ToolBox::new();
        let mut registry = TestRegistry::new();

        tool_box.activate_tool("move", &mut registry);
        tool_box.activate_tool("select", &mut registry);

        tool_box.deactivate_all(&mut registry);

        assert!(!registry.move_tool.active());
        assert!(!registry.select.active());
        assert!(tool_box.modal_tool_stack.is_empty());
    }
}
