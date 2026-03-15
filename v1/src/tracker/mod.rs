//! Tracker/Music Editor
//!
//! A pattern-based music tracker with SF2 soundfont support.
//! Inspired by Picotron's tracker design.
//!
//! Features hardware-accurate PS1 SPU emulation: ADPCM decode,
//! Gaussian interpolation, ADSR envelopes, and reverb per voice.
//!
//! Note: Some pattern/state API not yet fully used by the editor UI.

#![allow(dead_code)]

mod state;
mod audio;
mod pattern;
mod layout;
mod spu;
mod io;
pub mod actions;
mod song_browser;

// Re-export public API
// Some of these aren't used externally yet but are part of the intended public API
pub use state::TrackerState;
#[allow(unused_imports)]
pub use audio::{AudioEngine, OutputSampleRate};
#[allow(unused_imports)]
pub use pattern::*;
pub use layout::{draw_tracker, draw_song_browser};
#[allow(unused_imports)]
pub use spu::reverb::ReverbType;
// WASM async loading functions for song browser
#[allow(unused_imports)]
pub use song_browser::{load_song_list, load_song_async};
// Song browser types and discovery for main.rs cloud integration
pub use song_browser::{SongCategory, SongInfo, SAMPLES_SONGS_DIR, USER_SONGS_DIR};
#[cfg(not(target_arch = "wasm32"))]
pub use song_browser::discover_songs_from_dir;
// IO functions for cloud loading in main.rs
pub use io::load_song_from_str;
