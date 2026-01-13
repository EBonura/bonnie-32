//! Input handling with gamepad support
//!
//! Provides an action-based input system that works with both keyboard/mouse
//! and gamepad controllers. Uses Elden Ring-style button mapping.
//!
//! Native: Uses gilrs crate for cross-platform gamepad input
//! WASM: Uses custom Web Gamepad API bindings (avoids RefCell conflicts)

// Allow unused - input system scaffolding for future game runtime
#![allow(dead_code)]

mod actions;
mod controller_type;
mod gamepad;
mod midi;
mod state;
mod debug;

pub use actions::*;
pub use controller_type::{ControllerType, ButtonLabels};
// ButtonPosition is available in controller_type module if needed for advanced use
pub use gamepad::{Gamepad, button};
pub use midi::{MidiInput, MidiMessage};
pub use state::*;
pub use debug::draw_controller_debug;
