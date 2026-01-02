//! Tracker/Music Editor
//!
//! A pattern-based music tracker with SF2 soundfont support.
//! Inspired by Picotron's tracker design.
//!
//! Features authentic PS1 SPU reverb emulation based on the nocash specifications.
//!
//! Note: Some pattern/state API not yet fully used by the editor UI.

#![allow(dead_code)]

mod state;
mod audio;
mod pattern;
mod layout;
mod psx_reverb;
mod io;
pub mod actions;

// Re-export public API
// Some of these aren't used externally yet but are part of the intended public API
pub use state::TrackerState;
#[allow(unused_imports)]
pub use audio::{AudioEngine, OutputSampleRate};
#[allow(unused_imports)]
pub use pattern::*;
pub use layout::draw_tracker;
#[allow(unused_imports)]
pub use psx_reverb::{PsxReverb, ReverbType};
// Actions used internally by layout.rs and state.rs
