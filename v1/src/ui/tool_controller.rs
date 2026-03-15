//! ToolController - Input handling for tools
//!
//! Inspired by TrenchBroom's ToolController. Defines how tools respond to
//! mouse, keyboard, and gesture input.
//!
//! Tools that handle input should implement both Tool and ToolController.

use super::tool::Tool;

/// Modifier key state
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModifierKeys {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

/// Mouse button state
#[derive(Debug, Clone, Copy, Default)]
pub struct MouseButtons {
    pub left: bool,
    pub right: bool,
    pub middle: bool,
}

/// Unified input state passed to tool controllers
///
/// Contains all input information needed to handle mouse, keyboard,
/// and modifier key events.
#[derive(Debug, Clone, Default)]
pub struct InputState {
    /// Current mouse X position (screen coordinates)
    pub mouse_x: f32,
    /// Current mouse Y position (screen coordinates)
    pub mouse_y: f32,
    /// Mouse X delta since last frame
    pub mouse_dx: f32,
    /// Mouse Y delta since last frame
    pub mouse_dy: f32,
    /// Current mouse button state
    pub buttons: MouseButtons,
    /// Whether left mouse was just pressed this frame
    pub left_pressed: bool,
    /// Whether left mouse was just released this frame
    pub left_released: bool,
    /// Whether right mouse was just pressed this frame
    pub right_pressed: bool,
    /// Scroll wheel delta
    pub scroll: f32,
    /// Current modifier key state
    pub modifiers: ModifierKeys,
    /// Whether this is a double-click
    pub double_click: bool,
}

impl InputState {
    /// Get mouse position as tuple
    pub fn mouse_pos(&self) -> (f32, f32) {
        (self.mouse_x, self.mouse_y)
    }

    /// Get mouse delta as tuple
    pub fn mouse_delta(&self) -> (f32, f32) {
        (self.mouse_dx, self.mouse_dy)
    }

    /// Check if any modifier is held
    pub fn has_modifier(&self) -> bool {
        self.modifiers.shift || self.modifiers.ctrl || self.modifiers.alt
    }
}

/// Result of accepting a drag operation
pub enum DragAcceptResult {
    /// No drag started
    None,
    /// Started a drag with the given tracker ID (tool manages the tracker)
    Started,
}

/// Input handling trait for tools
///
/// Tools that respond to user input should implement this trait.
/// The ToolBox routes input to active tool controllers.
///
/// # Input Routing
///
/// Input events are routed with different semantics:
///
/// - **First-wins**: `mouse_click`, `mouse_double_click`, `accept_mouse_drag`
///   The first active tool to return `true` / `Some` consumes the event.
///
/// - **Broadcast**: `mouse_move`, `mouse_scroll`, `modifier_key_change`
///   All active tools receive these events.
///
/// # Example
///
/// ```ignore
/// impl ToolController for MoveTool {
///     fn mouse_move(&mut self, input: &InputState) {
///         // Update hover state based on mouse position
///         self.update_gizmo_hover(input.mouse_x, input.mouse_y);
///     }
///
///     fn accept_mouse_drag(&mut self, input: &InputState) -> DragAcceptResult {
///         if input.left_pressed && self.hovered_axis.is_some() {
///             // Start drag
///             DragAcceptResult::Started
///         } else {
///             DragAcceptResult::None
///         }
///     }
/// }
/// ```
pub trait ToolController: Tool {
    /// Handle mouse click (left button pressed and released without drag)
    ///
    /// Return `true` if the click was handled and should not propagate.
    fn mouse_click(&mut self, _input: &InputState) -> bool {
        false
    }

    /// Handle mouse double-click
    ///
    /// Return `true` if handled.
    fn mouse_double_click(&mut self, _input: &InputState) -> bool {
        false
    }

    /// Handle mouse movement (always called, doesn't consume)
    ///
    /// Use this for hover detection, cursor updates, etc.
    fn mouse_move(&mut self, _input: &InputState) {}

    /// Handle mouse scroll
    fn mouse_scroll(&mut self, _input: &InputState) {}

    /// Accept a mouse drag operation
    ///
    /// Called when the user starts dragging (mouse down + move).
    /// Return `DragAcceptResult::Started` to claim the drag.
    ///
    /// The actual drag tracking is handled by the tool's internal state
    /// or by the DragManager system.
    fn accept_mouse_drag(&mut self, _input: &InputState) -> DragAcceptResult {
        DragAcceptResult::None
    }

    /// Handle modifier key change during a drag
    ///
    /// Called when Shift/Ctrl/Alt state changes while dragging.
    /// Useful for axis constraints (pressing X/Y/Z to lock axis).
    fn modifier_key_change(&mut self, _input: &InputState) {}

    /// Cancel the current operation
    ///
    /// Called when user presses Escape or right-clicks during operation.
    /// Return `true` if there was something to cancel.
    fn cancel(&mut self) -> bool {
        false
    }
}

/// Convenience macro for implementing ToolController with Tool
///
/// Many tools need both traits. This macro reduces boilerplate.
#[macro_export]
macro_rules! impl_tool_controller {
    ($type:ty, $id:literal, $label:literal) => {
        impl $crate::ui::Tool for $type {
            fn id(&self) -> &'static str { $id }
            fn label(&self) -> &'static str { $label }
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
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_state() {
        let input = InputState {
            mouse_x: 100.0,
            mouse_y: 200.0,
            mouse_dx: 5.0,
            mouse_dy: -3.0,
            modifiers: ModifierKeys { shift: true, ctrl: false, alt: false },
            ..Default::default()
        };

        assert_eq!(input.mouse_pos(), (100.0, 200.0));
        assert_eq!(input.mouse_delta(), (5.0, -3.0));
        assert!(input.has_modifier());
    }

    #[test]
    fn test_modifier_keys() {
        let no_mods = ModifierKeys::default();
        assert!(!no_mods.shift);
        assert!(!no_mods.ctrl);
        assert!(!no_mods.alt);

        let with_shift = ModifierKeys { shift: true, ..Default::default() };
        assert!(with_shift.shift);
    }
}
