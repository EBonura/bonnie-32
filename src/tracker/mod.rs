//! Tracker/Music Editor
//!
//! A pattern-based music tracker with SF2 soundfont support.
//! Inspired by Picotron's tracker design.
//!
//! Features authentic PS1 SPU reverb emulation based on the nocash specifications.

mod state;
mod audio;
mod pattern;
mod layout;
mod psx_reverb;

pub use state::TrackerState;
pub use audio::AudioEngine;
pub use pattern::*;
pub use layout::draw_tracker;
pub use psx_reverb::{PsxReverb, ReverbType};
