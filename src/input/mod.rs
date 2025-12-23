//! Input handling with gamepad support
//!
//! Provides an action-based input system that works with both keyboard/mouse
//! and gamepad controllers. Uses Elden Ring-style button mapping.
//!
//! Native: Uses gilrs crate for cross-platform gamepad input
//! WASM: Uses custom Web Gamepad API bindings (avoids RefCell conflicts)

mod actions;
mod gamepad;
mod state;
mod debug;

pub use actions::*;
pub use gamepad::{Gamepad, button};
pub use state::*;
pub use debug::draw_controller_debug;
