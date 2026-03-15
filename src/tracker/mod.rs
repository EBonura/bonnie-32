//! Tracker/Music Editor
//!
//! A pattern-based music tracker with SF2 soundfont support.
//! Features hardware-accurate PS1 SPU emulation: ADPCM decode,
//! Gaussian interpolation, ADSR envelopes, and reverb per voice.

#![allow(dead_code)]

pub mod spu;
pub mod pattern;
pub mod audio;
pub mod state;

pub use pattern::*;
pub use audio::{AudioEngine, SpuPitch};
pub use state::{TrackerState, TrackerView};
pub use spu::reverb::ReverbType;
