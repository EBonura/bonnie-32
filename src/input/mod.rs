//! Input handling with gamepad support
//!
//! Provides an action-based input system that works with both keyboard/mouse
//! and gamepad controllers. Uses Elden Ring-style button mapping.

mod actions;
mod state;
mod debug;

pub use actions::*;
pub use state::*;
pub use debug::draw_controller_debug;
