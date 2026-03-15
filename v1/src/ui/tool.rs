//! Tool trait for editor tools
//!
//! Inspired by TrenchBroom's Tool class. Tools have:
//! - Activation lifecycle (activate/deactivate with success/failure)
//! - Active state tracking
//! - Identity (id, label) for UI and serialization

/// Base trait for all tools
///
/// Tools are stateful objects that can be activated and deactivated.
/// Only active tools receive input events and render their visuals.
///
/// # Lifecycle
///
/// ```text
/// [Inactive] --activate()--> [Active] --deactivate()--> [Inactive]
///                ^                          |
///                |     (can fail)           |
///                +--------------------------+
/// ```
///
/// # Example
///
/// ```ignore
/// struct MoveTool {
///     active: bool,
///     hovered_axis: Option<Axis>,
/// }
///
/// impl Tool for MoveTool {
///     fn id(&self) -> &'static str { "move" }
///     fn label(&self) -> &'static str { "Move (G)" }
///     fn active(&self) -> bool { self.active }
///
///     fn do_activate(&mut self) -> bool {
///         self.active = true;
///         true
///     }
///
///     fn do_deactivate(&mut self) -> bool {
///         self.hovered_axis = None;
///         self.active = false;
///         true
///     }
/// }
/// ```
pub trait Tool {
    /// Unique identifier for this tool (e.g., "move", "rotate", "select")
    fn id(&self) -> &'static str;

    /// Human-readable label (e.g., "Move (G)", "Rotate (R)")
    fn label(&self) -> &'static str;

    /// Whether this tool is currently active
    fn active(&self) -> bool;

    /// Attempt to activate the tool.
    ///
    /// Called by ToolBox when this tool should become active.
    /// Return `true` if activation succeeded, `false` if preconditions not met.
    ///
    /// # Preconditions
    /// - Tool must not already be active (enforced by ToolBox)
    ///
    /// # Implementation
    /// Override `do_activate()` to add custom activation logic.
    fn activate(&mut self) -> bool {
        if self.active() {
            return false; // Already active
        }
        self.do_activate()
    }

    /// Attempt to deactivate the tool.
    ///
    /// Called by ToolBox when this tool should become inactive.
    /// Return `true` if deactivation succeeded, `false` if cleanup needed.
    ///
    /// # Preconditions
    /// - Tool must be active (enforced by ToolBox)
    ///
    /// # Implementation
    /// Override `do_deactivate()` to add custom deactivation logic.
    fn deactivate(&mut self) -> bool {
        if !self.active() {
            return false; // Already inactive
        }
        self.do_deactivate()
    }

    /// Internal activation logic - override this in implementations.
    ///
    /// Should set `active = true` and perform any setup.
    /// Return `false` to deny activation (e.g., preconditions not met).
    fn do_activate(&mut self) -> bool {
        true
    }

    /// Internal deactivation logic - override this in implementations.
    ///
    /// Should set `active = false` and perform any cleanup.
    /// Return `false` to deny deactivation (e.g., unsaved changes).
    fn do_deactivate(&mut self) -> bool {
        true
    }
}

/// Trait for accessing tools by ID
///
/// Implemented by tool containers (e.g., ModelerToolBox) to allow
/// ToolBox to activate/deactivate tools by ID.
pub trait ToolRegistry {
    /// Get a mutable reference to a tool by ID
    fn get_tool_mut(&mut self, id: &str) -> Option<&mut dyn Tool>;

    /// Get an immutable reference to a tool by ID
    fn get_tool(&self, id: &str) -> Option<&dyn Tool>;

    /// Get all tool IDs
    fn tool_ids(&self) -> Vec<&'static str>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestTool {
        name: &'static str,
        active: bool,
        activate_count: u32,
        deactivate_count: u32,
    }

    impl TestTool {
        fn new(name: &'static str) -> Self {
            Self {
                name,
                active: false,
                activate_count: 0,
                deactivate_count: 0,
            }
        }
    }

    impl Tool for TestTool {
        fn id(&self) -> &'static str { self.name }
        fn label(&self) -> &'static str { self.name }
        fn active(&self) -> bool { self.active }

        fn do_activate(&mut self) -> bool {
            self.active = true;
            self.activate_count += 1;
            true
        }

        fn do_deactivate(&mut self) -> bool {
            self.active = false;
            self.deactivate_count += 1;
            true
        }
    }

    #[test]
    fn test_activation_lifecycle() {
        let mut tool = TestTool::new("test");

        assert!(!tool.active());
        assert!(tool.activate());
        assert!(tool.active());
        assert_eq!(tool.activate_count, 1);

        // Can't activate twice
        assert!(!tool.activate());
        assert_eq!(tool.activate_count, 1);

        assert!(tool.deactivate());
        assert!(!tool.active());
        assert_eq!(tool.deactivate_count, 1);

        // Can't deactivate twice
        assert!(!tool.deactivate());
        assert_eq!(tool.deactivate_count, 1);
    }
}
