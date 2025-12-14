//! Tracker editor state

use super::audio::AudioEngine;
use super::pattern::{Song, Note, Effect, MAX_CHANNELS};
use std::path::PathBuf;

/// Tracker view mode
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrackerView {
    /// Pattern editor (main view)
    Pattern,
    /// Song arrangement
    Arrangement,
    /// Instrument selection
    Instruments,
}

/// Tracker editor state
pub struct TrackerState {
    /// The current song being edited
    pub song: Song,
    /// Current file path
    pub current_file: Option<PathBuf>,
    /// Audio engine for playback
    pub audio: AudioEngine,
    /// Current view mode
    pub view: TrackerView,

    // Cursor position
    /// Current pattern index in arrangement
    pub current_pattern_idx: usize,
    /// Current row in pattern
    pub current_row: usize,
    /// Current channel (0-7)
    pub current_channel: usize,
    /// Current column within channel (0=note, 1=inst, 2=vol, 3=fx, 4=fx_param)
    pub current_column: usize,

    // Edit state
    /// Current octave for note entry (0-9)
    pub octave: u8,
    /// Current default volume (0-127)
    pub default_volume: u8,
    /// Edit step (how many rows to advance after entering a note)
    pub edit_step: usize,
    /// Is editing mode active? (vs. navigation only)
    pub edit_mode: bool,

    // Playback state
    /// Is playback active?
    pub playing: bool,
    /// Current playback row
    pub playback_row: usize,
    /// Current playback pattern in arrangement
    pub playback_pattern_idx: usize,
    /// Time accumulator for playback timing
    pub playback_time: f64,

    // View state
    /// First visible row in pattern view
    pub scroll_row: usize,
    /// Number of visible rows
    pub visible_rows: usize,

    // Selection
    /// Selection start (pattern_idx, row, channel)
    pub selection_start: Option<(usize, usize, usize)>,
    /// Selection end
    pub selection_end: Option<(usize, usize, usize)>,

    /// Dirty flag
    pub dirty: bool,
    /// Status message
    pub status_message: Option<(String, f64)>,
    /// Last played note per channel (for sustain detection - same note = no re-trigger)
    last_played_notes: [Option<u8>; MAX_CHANNELS],

    // Effect preview values (per channel, for testing in instruments view)
    /// Pan value per channel (0=left, 64=center, 127=right)
    pub preview_pan: [u8; MAX_CHANNELS],
    /// Modulation value per channel (0-127)
    pub preview_modulation: [u8; MAX_CHANNELS],
    /// Expression value per channel (0-127)
    pub preview_expression: [u8; MAX_CHANNELS],

    /// Instrument list scroll offset
    pub instrument_scroll: usize,

    /// Which knob is being edited (for text input)
    /// None = not editing, Some(index) = editing knob at index
    pub editing_knob: Option<usize>,
    /// Text being edited for knob value
    pub knob_edit_text: String,
}

/// Soundfont filename
const SOUNDFONT_NAME: &str = "TimGM6mb.sf2";

/// Find the soundfont in various locations (development, deployed, macOS app bundle)
#[cfg(not(target_arch = "wasm32"))]
fn find_soundfont() -> Option<PathBuf> {
    let candidates = [
        // Development: relative to cwd
        PathBuf::from(format!("assets/soundfonts/{}", SOUNDFONT_NAME)),
        // Deployed: next to executable
        std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("assets/soundfonts").join(SOUNDFONT_NAME))).unwrap_or_default(),
        // macOS app bundle: Contents/Resources
        std::env::current_exe().ok().and_then(|p| p.parent().and_then(|d| d.parent()).map(|d| d.join("Resources/assets/soundfonts").join(SOUNDFONT_NAME))).unwrap_or_default(),
        // Fallback: just the filename in cwd
        PathBuf::from(SOUNDFONT_NAME),
    ];

    for path in candidates {
        if path.exists() && path.as_os_str().len() > 0 {
            return Some(path);
        }
    }
    None
}

impl TrackerState {
    pub fn new() -> Self {
        let mut audio = AudioEngine::new();

        // Load soundfont - different strategies for native vs WASM
        #[cfg(target_arch = "wasm32")]
        {
            // On WASM: get from JavaScript cache (prefetched before WASM loaded)
            if super::audio::wasm::is_soundfont_cached() {
                if let Some(bytes) = super::audio::wasm::get_cached_soundfont() {
                    match audio.load_soundfont_from_bytes(&bytes, Some(SOUNDFONT_NAME.to_string())) {
                        Ok(()) => println!("Loaded soundfont from WASM cache: {}", SOUNDFONT_NAME),
                        Err(e) => eprintln!("Failed to load soundfont from cache: {}", e),
                    }
                }
            } else {
                eprintln!("Soundfont not available in WASM cache");
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            // On native: load from filesystem
            if let Some(sf_path) = find_soundfont() {
                match audio.load_soundfont(&sf_path) {
                    Ok(()) => println!("Loaded soundfont: {:?}", sf_path),
                    Err(e) => eprintln!("Failed to load soundfont {:?}: {}", sf_path, e),
                }
            } else {
                eprintln!("Soundfont {} not found in any search path", SOUNDFONT_NAME);
                if let Ok(cwd) = std::env::current_dir() {
                    eprintln!("Current working directory: {:?}", cwd);
                }
                if let Ok(exe) = std::env::current_exe() {
                    eprintln!("Executable location: {:?}", exe);
                }
            }
        }

        Self {
            song: Song::new(),
            current_file: None,
            audio,
            view: TrackerView::Pattern,

            current_pattern_idx: 0,
            current_row: 0,
            current_channel: 0,
            current_column: 0,

            octave: 4,
            default_volume: 100,
            edit_step: 1,
            edit_mode: true,

            playing: false,
            playback_row: 0,
            playback_pattern_idx: 0,
            playback_time: 0.0,

            scroll_row: 0,
            visible_rows: 32,

            selection_start: None,
            selection_end: None,

            dirty: false,
            status_message: None,
            last_played_notes: [None; MAX_CHANNELS],

            // Effect previews - initialize to defaults
            preview_pan: [64; MAX_CHANNELS],        // Center
            preview_modulation: [0; MAX_CHANNELS],  // No modulation
            preview_expression: [127; MAX_CHANNELS], // Full expression
            instrument_scroll: 0,
            editing_knob: None,
            knob_edit_text: String::new(),
        }
    }

    /// Set status message
    pub fn set_status(&mut self, message: &str, duration: f64) {
        let expiry = macroquad::time::get_time() + duration;
        self.status_message = Some((message.to_string(), expiry));
    }

    /// Get current status message if not expired
    pub fn get_status(&self) -> Option<&str> {
        if let Some((msg, expiry)) = &self.status_message {
            if macroquad::time::get_time() < *expiry {
                return Some(msg);
            }
        }
        None
    }

    /// Get the current pattern being edited
    pub fn current_pattern(&self) -> Option<&super::pattern::Pattern> {
        let pattern_num = self.song.arrangement.get(self.current_pattern_idx)?;
        self.song.patterns.get(*pattern_num)
    }

    /// Get the current pattern mutably
    pub fn current_pattern_mut(&mut self) -> Option<&mut super::pattern::Pattern> {
        let pattern_num = *self.song.arrangement.get(self.current_pattern_idx)?;
        self.song.patterns.get_mut(pattern_num)
    }

    /// Get the instrument for the current channel
    pub fn current_instrument(&self) -> u8 {
        self.song.get_channel_instrument(self.current_channel)
    }

    /// Set the instrument for the current channel
    pub fn set_current_instrument(&mut self, instrument: u8) {
        self.song.set_channel_instrument(self.current_channel, instrument);
        self.audio.set_program(self.current_channel as i32, instrument as i32);
    }

    /// Set preview pan for current channel and apply to audio
    pub fn set_preview_pan(&mut self, value: u8) {
        self.preview_pan[self.current_channel] = value;
        self.audio.set_pan(self.current_channel as i32, value as i32);
    }

    /// Set preview modulation for current channel and apply to audio
    pub fn set_preview_modulation(&mut self, value: u8) {
        self.preview_modulation[self.current_channel] = value;
        self.audio.set_modulation(self.current_channel as i32, value as i32);
    }

    /// Set preview expression for current channel and apply to audio
    pub fn set_preview_expression(&mut self, value: u8) {
        self.preview_expression[self.current_channel] = value;
        self.audio.set_expression(self.current_channel as i32, value as i32);
    }

    /// Reset all effect previews to defaults for current channel
    pub fn reset_preview_effects(&mut self) {
        let ch = self.current_channel;
        self.preview_pan[ch] = 64;
        self.preview_modulation[ch] = 0;
        self.preview_expression[ch] = 127;
        self.audio.reset_controllers(ch as i32);
    }

    /// Get the number of channels
    pub fn num_channels(&self) -> usize {
        self.song.num_channels()
    }

    /// Add a channel
    pub fn add_channel(&mut self) {
        self.song.add_channel();
    }

    /// Remove a channel
    pub fn remove_channel(&mut self) {
        self.song.remove_channel();
        // Make sure current_channel is still valid
        if self.current_channel >= self.song.num_channels() {
            self.current_channel = self.song.num_channels() - 1;
        }
    }

    /// Move cursor up
    pub fn cursor_up(&mut self) {
        if self.current_row > 0 {
            self.current_row -= 1;
            self.ensure_row_visible();
        }
    }

    /// Move cursor down
    pub fn cursor_down(&mut self) {
        if let Some(pattern) = self.current_pattern() {
            if self.current_row < pattern.length - 1 {
                self.current_row += 1;
                self.ensure_row_visible();
            }
        }
    }

    /// Move cursor left
    pub fn cursor_left(&mut self) {
        if self.current_column > 0 {
            self.current_column -= 1;
        } else if self.current_channel > 0 {
            self.current_channel -= 1;
            self.current_column = 4; // fx_param column
        }
    }

    /// Move cursor right
    pub fn cursor_right(&mut self) {
        let num_ch = self.num_channels();
        if self.current_column < 4 {
            self.current_column += 1;
        } else if self.current_channel < num_ch - 1 {
            self.current_channel += 1;
            self.current_column = 0;
        }
    }

    /// Jump to next channel
    pub fn next_channel(&mut self) {
        let num_ch = self.num_channels();
        if self.current_channel < num_ch - 1 {
            self.current_channel += 1;
        }
    }

    /// Jump to previous channel
    pub fn prev_channel(&mut self) {
        if self.current_channel > 0 {
            self.current_channel -= 1;
        }
    }

    /// Ensure current row is visible
    fn ensure_row_visible(&mut self) {
        if self.current_row < self.scroll_row {
            self.scroll_row = self.current_row;
        } else if self.current_row >= self.scroll_row + self.visible_rows {
            self.scroll_row = self.current_row - self.visible_rows + 1;
        }
    }

    /// Enter a note at cursor position
    pub fn enter_note(&mut self, pitch: u8) {
        let channel = self.current_channel;
        let row = self.current_row;
        let instrument = self.current_instrument();

        if let Some(pattern) = self.current_pattern_mut() {
            let note = Note::new(pitch, instrument);
            pattern.set(channel, row, note);
        }
        self.dirty = true;

        // Preview the note (make sure audio engine uses correct instrument for channel)
        self.audio.set_program(channel as i32, instrument as i32);
        self.audio.note_on(channel as i32, pitch as i32, 100);

        // Advance cursor
        self.advance_cursor();
    }

    /// Enter a note-off at cursor position
    pub fn enter_note_off(&mut self) {
        let channel = self.current_channel;
        let row = self.current_row;

        if let Some(pattern) = self.current_pattern_mut() {
            pattern.set(channel, row, Note::off());
        }
        self.dirty = true;
        self.advance_cursor();
    }

    /// Delete note at cursor position
    pub fn delete_note(&mut self) {
        let channel = self.current_channel;
        let row = self.current_row;

        if let Some(pattern) = self.current_pattern_mut() {
            pattern.set(channel, row, Note::EMPTY);
        }
        self.dirty = true;
    }

    /// Set effect at cursor position
    pub fn set_effect(&mut self, effect_char: char, param: u8) {
        let channel = self.current_channel;
        let row = self.current_row;

        if let Some(pattern) = self.current_pattern_mut() {
            if let Some(note) = pattern.channels.get_mut(channel).and_then(|ch| ch.get_mut(row)) {
                note.effect = Some(effect_char);
                note.effect_param = Some(param);
            }
        }
        self.dirty = true;
    }

    /// Set only the effect character at cursor (keep existing param)
    pub fn set_effect_char(&mut self, effect_char: char) {
        let channel = self.current_channel;
        let row = self.current_row;

        if let Some(pattern) = self.current_pattern_mut() {
            if let Some(note) = pattern.channels.get_mut(channel).and_then(|ch| ch.get_mut(row)) {
                note.effect = Some(effect_char);
                // Initialize param if not set
                if note.effect_param.is_none() {
                    note.effect_param = Some(0);
                }
            }
        }
        self.dirty = true;
    }

    /// Set only the effect parameter at cursor (high nibble)
    pub fn set_effect_param_high(&mut self, nibble: u8) {
        let channel = self.current_channel;
        let row = self.current_row;

        if let Some(pattern) = self.current_pattern_mut() {
            if let Some(note) = pattern.channels.get_mut(channel).and_then(|ch| ch.get_mut(row)) {
                let low = note.effect_param.unwrap_or(0) & 0x0F;
                note.effect_param = Some((nibble << 4) | low);
            }
        }
        self.dirty = true;
    }

    /// Set only the effect parameter at cursor (low nibble)
    pub fn set_effect_param_low(&mut self, nibble: u8) {
        let channel = self.current_channel;
        let row = self.current_row;

        if let Some(pattern) = self.current_pattern_mut() {
            if let Some(note) = pattern.channels.get_mut(channel).and_then(|ch| ch.get_mut(row)) {
                let high = note.effect_param.unwrap_or(0) & 0xF0;
                note.effect_param = Some(high | (nibble & 0x0F));
            }
        }
        self.dirty = true;
    }

    /// Clear effect at cursor position
    pub fn clear_effect(&mut self) {
        let channel = self.current_channel;
        let row = self.current_row;

        if let Some(pattern) = self.current_pattern_mut() {
            if let Some(note) = pattern.channels.get_mut(channel).and_then(|ch| ch.get_mut(row)) {
                note.effect = None;
                note.effect_param = None;
            }
        }
        self.dirty = true;
    }

    /// Advance cursor by edit_step rows
    fn advance_cursor(&mut self) {
        if let Some(pattern) = self.current_pattern() {
            self.current_row = (self.current_row + self.edit_step).min(pattern.length - 1);
            self.ensure_row_visible();
        }
    }

    /// Toggle playback from current cursor position
    pub fn toggle_playback(&mut self) {
        self.playing = !self.playing;
        if self.playing {
            self.playback_row = self.current_row;
            self.playback_pattern_idx = self.current_pattern_idx;
            self.playback_time = 0.0;
            self.last_played_notes = [None; MAX_CHANNELS];
        } else {
            self.audio.all_notes_off();
            self.last_played_notes = [None; MAX_CHANNELS];
        }
    }

    /// Start playback from the beginning of the song
    pub fn play_from_start(&mut self) {
        self.audio.all_notes_off();
        self.playback_row = 0;
        self.playback_pattern_idx = 0;
        self.playback_time = 0.0;
        self.playing = true;
        self.last_played_notes = [None; MAX_CHANNELS];
    }

    /// Stop playback and return cursor to start
    pub fn stop_playback(&mut self) {
        self.playing = false;
        self.playback_row = 0;
        self.playback_pattern_idx = 0;
        self.current_row = 0;
        self.current_pattern_idx = 0;
        self.scroll_row = 0;
        self.audio.all_notes_off();
        self.last_played_notes = [None; MAX_CHANNELS];
    }

    /// Update playback (called each frame)
    pub fn update_playback(&mut self, delta: f64) {
        // On WASM, we need to render audio each frame to push samples to Web Audio
        #[cfg(target_arch = "wasm32")]
        {
            self.audio.render_audio(delta);
        }

        if !self.playing {
            return;
        }

        self.playback_time += delta;
        let tick_duration = self.song.tick_duration();

        while self.playback_time >= tick_duration {
            self.playback_time -= tick_duration;
            self.play_current_row();
            self.advance_playback();
        }
    }

    /// Play notes at current playback row
    fn play_current_row(&mut self) {
        let pattern_num = match self.song.arrangement.get(self.playback_pattern_idx) {
            Some(&n) => n,
            None => return,
        };

        let pattern = match self.song.patterns.get(pattern_num) {
            Some(p) => p,
            None => return,
        };

        // Collect note data first to avoid borrow issues
        let num_channels = self.song.num_channels();
        let playback_row = self.playback_row;
        let mut notes_to_play: Vec<(usize, Option<u8>, Option<u8>, Option<u8>, Option<u8>)> = Vec::new();
        let mut effects_to_apply: Vec<(usize, Effect)> = Vec::new();

        for channel in 0..num_channels {
            if let Some(note) = pattern.get(channel, playback_row) {
                // Collect note data
                let inst = note.instrument.unwrap_or_else(|| self.song.get_channel_instrument(channel));
                notes_to_play.push((channel, note.pitch, Some(inst), note.volume, None));

                // Collect effect
                if let (Some(fx_char), Some(fx_param)) = (note.effect, note.effect_param) {
                    let effect = Effect::from_char(fx_char, fx_param);
                    effects_to_apply.push((channel, effect));
                }
            }
        }

        // Now process notes (pattern borrow is released)
        for (channel, pitch, inst, volume, _) in notes_to_play {
            if let Some(p) = pitch {
                if p == 0xFF {
                    // Note off
                    self.audio.note_off(channel as i32, 0);
                    self.last_played_notes[channel] = None;
                } else {
                    // Check if same note is already playing (sustain behavior like Picotron)
                    let last_note = self.last_played_notes[channel];
                    if last_note != Some(p) {
                        // Different note or first note - trigger it
                        let velocity = volume.unwrap_or(100) as i32;
                        let instrument = inst.unwrap_or(0);
                        self.audio.set_program(channel as i32, instrument as i32);
                        self.audio.note_on(channel as i32, p as i32, velocity);
                        self.last_played_notes[channel] = Some(p);
                    }
                    // Same note = sustain, don't re-trigger
                }
            }
        }

        // Now apply effects
        for (channel, effect) in effects_to_apply {
            self.apply_effect(channel, effect);
        }
    }

    /// Apply an effect to a channel
    fn apply_effect(&mut self, channel: usize, effect: Effect) {
        let ch = channel as i32;
        match effect {
            Effect::None => {}
            Effect::SetVolume(v) => {
                self.audio.set_volume(ch, v as i32);
            }
            Effect::SetPan(p) => {
                self.audio.set_pan(ch, p as i32);
            }
            Effect::SetExpression(v) => {
                self.audio.set_expression(ch, v as i32);
            }
            Effect::SetModulation(v) => {
                self.audio.set_modulation(ch, v as i32);
            }
            Effect::SlideUp(amount) => {
                // Pitch bend up: center (8192) + amount * 64
                let bend = 8192 + (amount as i32 * 64);
                self.audio.set_pitch_bend(ch, bend.min(16383));
            }
            Effect::SlideDown(amount) => {
                // Pitch bend down: center (8192) - amount * 64
                let bend = 8192 - (amount as i32 * 64);
                self.audio.set_pitch_bend(ch, bend.max(0));
            }
            Effect::Vibrato(_, depth) => {
                // Use modulation wheel for vibrato
                self.audio.set_modulation(ch, (depth as i32 * 8).min(127));
            }
            Effect::SetSpeed(bpm) => {
                // Change song tempo
                if bpm > 0 {
                    self.song.bpm = bpm as u16;
                }
            }
            Effect::PatternBreak(row) => {
                // Jump to next pattern at specified row
                // This will be handled in advance_playback
                // For now, just set a flag or target row
                // TODO: Implement pattern break properly
                let _ = row;
            }
            // Effects that need per-tick processing (not implemented yet)
            Effect::Arpeggio(_, _) => {
                // Would need sub-row tick processing
            }
            Effect::Portamento(_) => {
                // Would need note memory and per-tick slide
            }
            Effect::VolumeSlide(_, _) => {
                // Would need per-tick processing
            }
        }
    }

    /// Advance playback to next row
    fn advance_playback(&mut self) {
        let pattern_num = match self.song.arrangement.get(self.playback_pattern_idx) {
            Some(&n) => n,
            None => {
                self.stop_playback();
                return;
            }
        };

        let pattern_len = match self.song.patterns.get(pattern_num) {
            Some(p) => p.length,
            None => {
                self.stop_playback();
                return;
            }
        };

        self.playback_row += 1;
        if self.playback_row >= pattern_len {
            self.playback_row = 0;
            self.playback_pattern_idx += 1;
            if self.playback_pattern_idx >= self.song.arrangement.len() {
                // Loop or stop
                self.playback_pattern_idx = 0; // Loop for now
            }
        }

        // Update view cursor to follow playback
        self.current_row = self.playback_row;
        self.current_pattern_idx = self.playback_pattern_idx;
        self.ensure_row_visible();
    }

    /// Convert keyboard key to MIDI note
    pub fn key_to_note(key: macroquad::prelude::KeyCode, octave: u8) -> Option<u8> {
        use macroquad::prelude::KeyCode;

        // Piano keyboard layout:
        // Bottom row: Z S X D C V G B H N J M (C to B)
        // Top row: Q 2 W 3 E R 5 T 6 Y 7 U (C+1 octave to B+1)
        let base_note = octave * 12;

        let note_offset = match key {
            // Bottom row - lower octave
            KeyCode::Z => Some(0),  // C
            KeyCode::S => Some(1),  // C#
            KeyCode::X => Some(2),  // D
            KeyCode::D => Some(3),  // D#
            KeyCode::C => Some(4),  // E
            KeyCode::V => Some(5),  // F
            KeyCode::G => Some(6),  // F#
            KeyCode::B => Some(7),  // G
            KeyCode::H => Some(8),  // G#
            KeyCode::N => Some(9),  // A
            KeyCode::J => Some(10), // A#
            KeyCode::M => Some(11), // B

            // Top row - upper octave
            KeyCode::Q => Some(12), // C
            KeyCode::Key2 => Some(13), // C#
            KeyCode::W => Some(14), // D
            KeyCode::Key3 => Some(15), // D#
            KeyCode::E => Some(16), // E
            KeyCode::R => Some(17), // F
            KeyCode::Key5 => Some(18), // F#
            KeyCode::T => Some(19), // G
            KeyCode::Key6 => Some(20), // G#
            KeyCode::Y => Some(21), // A
            KeyCode::Key7 => Some(22), // A#
            KeyCode::U => Some(23), // B

            _ => None,
        };

        note_offset.map(|offset| (base_note + offset).min(127))
    }
}

impl Default for TrackerState {
    fn default() -> Self {
        Self::new()
    }
}
