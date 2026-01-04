//! Tracker editor state

use super::audio::AudioEngine;
use super::pattern::{Song, Note, Effect, MAX_CHANNELS};
use super::psx_reverb::ReverbType;
use super::actions::create_tracker_actions;
use super::song_browser::SongBrowser;
use crate::ui::ActionRegistry;
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
    /// Action registry for keyboard shortcuts
    pub actions: ActionRegistry,

    /// Clipboard for copy/paste (rows × channels of notes)
    /// Stored as [channel][row] to match pattern structure
    pub clipboard: Option<Vec<Vec<Note>>>,

    /// Song browser dialog
    pub song_browser: SongBrowser,

    /// Preview song for browser playback (uses this instead of main song when Some)
    preview_song: Option<Song>,
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
            actions: create_tracker_actions(),
            clipboard: None,
            song_browser: SongBrowser::new(),
            preview_song: None,
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

    /// Get the current pattern length
    pub fn pattern_length(&self) -> usize {
        self.current_pattern().map(|p| p.length).unwrap_or(64)
    }

    /// Increase pattern length by 16 rows (max 256)
    pub fn increase_pattern_length(&mut self) {
        let current_len = self.pattern_length();
        let new_len = (current_len + 16).min(256);
        if let Some(pattern) = self.current_pattern_mut() {
            pattern.set_length(new_len);
        }
        self.dirty = true;
    }

    /// Decrease pattern length by 16 rows (min 16)
    pub fn decrease_pattern_length(&mut self) {
        let current_len = self.pattern_length();
        let new_len = current_len.saturating_sub(16).max(16);
        if let Some(pattern) = self.current_pattern_mut() {
            pattern.set_length(new_len);
        }
        // Make sure cursor is still within bounds
        if self.current_row >= new_len {
            self.current_row = new_len - 1;
        }
        self.dirty = true;
    }

    // ========================================================================
    // Pattern Bank Management
    // ========================================================================

    /// Get the total number of patterns in the bank
    pub fn pattern_count(&self) -> usize {
        self.song.patterns.len()
    }

    /// Create a new empty pattern in the bank (doesn't add to arrangement)
    /// Returns the index of the new pattern
    pub fn create_pattern(&mut self) -> usize {
        let num_channels = self.song.num_channels();
        let new_pattern = super::pattern::Pattern::with_channels(64, num_channels);
        self.song.patterns.push(new_pattern);
        self.dirty = true;
        self.song.patterns.len() - 1
    }

    /// Duplicate a pattern in the bank
    /// Returns the index of the new pattern, or None if source doesn't exist
    pub fn duplicate_pattern(&mut self, pattern_idx: usize) -> Option<usize> {
        let pattern = self.song.patterns.get(pattern_idx)?.clone();
        self.song.patterns.push(pattern);
        self.dirty = true;
        Some(self.song.patterns.len() - 1)
    }

    /// Delete a pattern from the bank (also removes from arrangement)
    /// Returns false if pattern doesn't exist or is the last one
    pub fn delete_pattern(&mut self, pattern_idx: usize) -> bool {
        if self.song.patterns.len() <= 1 || pattern_idx >= self.song.patterns.len() {
            return false;
        }

        // Remove the pattern
        self.song.patterns.remove(pattern_idx);

        // Update arrangement: remove references to deleted pattern, adjust indices
        self.song.arrangement.retain(|&idx| idx != pattern_idx);
        for idx in &mut self.song.arrangement {
            if *idx > pattern_idx {
                *idx -= 1;
            }
        }

        // Make sure arrangement isn't empty
        if self.song.arrangement.is_empty() {
            self.song.arrangement.push(0);
        }

        // Adjust current pattern index if needed
        if self.current_pattern_idx >= self.song.arrangement.len() {
            self.current_pattern_idx = self.song.arrangement.len() - 1;
        }

        self.dirty = true;
        true
    }

    // ========================================================================
    // Arrangement Management
    // ========================================================================

    /// Insert a pattern into the arrangement at the given position
    pub fn arrangement_insert(&mut self, position: usize, pattern_idx: usize) {
        if pattern_idx < self.song.patterns.len() {
            let pos = position.min(self.song.arrangement.len());
            self.song.arrangement.insert(pos, pattern_idx);
            self.dirty = true;
        }
    }

    /// Remove an entry from the arrangement at the given position
    /// Won't remove if it's the last entry
    pub fn arrangement_remove(&mut self, position: usize) -> bool {
        if self.song.arrangement.len() > 1 && position < self.song.arrangement.len() {
            self.song.arrangement.remove(position);
            // Adjust current position if needed
            if self.current_pattern_idx >= self.song.arrangement.len() {
                self.current_pattern_idx = self.song.arrangement.len() - 1;
            }
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Move an arrangement entry up (earlier in sequence)
    pub fn arrangement_move_up(&mut self, position: usize) -> bool {
        if position > 0 && position < self.song.arrangement.len() {
            self.song.arrangement.swap(position, position - 1);
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Move an arrangement entry down (later in sequence)
    pub fn arrangement_move_down(&mut self, position: usize) -> bool {
        if position + 1 < self.song.arrangement.len() {
            self.song.arrangement.swap(position, position + 1);
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Set the pattern at a specific arrangement position
    pub fn arrangement_set_pattern(&mut self, position: usize, pattern_idx: usize) {
        if position < self.song.arrangement.len() && pattern_idx < self.song.patterns.len() {
            self.song.arrangement[position] = pattern_idx;
            self.dirty = true;
        }
    }

    /// Get arrangement length
    pub fn arrangement_len(&self) -> usize {
        self.song.arrangement.len()
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
    /// Columns within channel: 0=Note, 1=Volume, 2=Effect, 3=Effect param
    /// Special column 4 = Global Reverb (not within a channel)
    pub fn cursor_left(&mut self) {
        if self.current_column == 4 {
            // From global reverb, go to last channel's last column
            self.current_column = 3;
            self.current_channel = self.num_channels() - 1;
        } else if self.current_column > 0 {
            self.current_column -= 1;
        } else if self.current_channel > 0 {
            self.current_channel -= 1;
            self.current_column = 3; // fx param column (last column in channel)
        }
    }

    /// Move cursor right
    /// Columns within channel: 0=Note, 1=Volume, 2=Effect, 3=Effect param
    /// Special column 4 = Global Reverb (not within a channel)
    pub fn cursor_right(&mut self) {
        let num_ch = self.num_channels();
        if self.current_column == 4 {
            // Already at global reverb, can't go further right
            return;
        } else if self.current_column < 3 {
            self.current_column += 1;
        } else if self.current_channel < num_ch - 1 {
            self.current_channel += 1;
            self.current_column = 0;
        } else {
            // At last channel's last column, go to global reverb
            self.current_column = 4;
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

    /// Enter a note at cursor position (or fill selection if active)
    pub fn enter_note(&mut self, pitch: u8) {
        let instrument = self.current_instrument();
        let note = Note::new(pitch, instrument);

        // Check if we have a selection - if so, fill all selected cells
        if let Some((start_row, end_row, start_ch, end_ch)) = self.get_selection_bounds() {
            if let Some(pattern) = self.current_pattern_mut() {
                for ch in start_ch..=end_ch {
                    for row in start_row..=end_row {
                        pattern.set(ch, row, note);
                    }
                }
            }
        } else {
            // No selection - just set at cursor position
            let channel = self.current_channel;
            let row = self.current_row;
            if let Some(pattern) = self.current_pattern_mut() {
                pattern.set(channel, row, note);
            }
        }
        self.dirty = true;

        // Preview the note (make sure audio engine uses correct instrument for channel)
        let channel = self.current_channel;
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

    /// Set global reverb preset at cursor row (0-9)
    /// PS1 has a single global reverb processor, so this affects all channels
    pub fn set_reverb(&mut self, preset: u8) {
        let row = self.current_row;

        if let Some(pattern) = self.current_pattern_mut() {
            pattern.set_reverb(row, Some(preset.min(9)));
        }
        self.dirty = true;
    }

    /// Clear global reverb at cursor row
    pub fn clear_reverb(&mut self) {
        let row = self.current_row;

        if let Some(pattern) = self.current_pattern_mut() {
            pattern.set_reverb(row, None);
        }
        self.dirty = true;
    }

    /// Called after entering a note (no-op, cursor stays on current row)
    fn advance_cursor(&mut self) {
        // Do nothing - cursor stays on current row after note entry
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
        self.preview_song = None;
    }

    /// Start preview playback of a song from the browser
    pub fn start_preview_playback(&mut self, song: Song) {
        self.audio.all_notes_off();
        self.preview_song = Some(song);
        self.playback_row = 0;
        self.playback_pattern_idx = 0;
        self.playback_time = 0.0;
        self.playing = true;
        self.last_played_notes = [None; MAX_CHANNELS];
    }

    /// Stop preview playback
    pub fn stop_preview_playback(&mut self) {
        self.playing = false;
        self.playback_row = 0;
        self.playback_pattern_idx = 0;
        self.audio.all_notes_off();
        self.last_played_notes = [None; MAX_CHANNELS];
        self.preview_song = None;
    }

    /// Get the current song for playback (preview song if set, else main song)
    fn playback_song(&self) -> &Song {
        self.preview_song.as_ref().unwrap_or(&self.song)
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
        let tick_duration = self.playback_song().tick_duration();

        while self.playback_time >= tick_duration {
            self.playback_time -= tick_duration;
            self.play_current_row();
            self.advance_playback();
        }
    }

    /// Play notes at current playback row
    fn play_current_row(&mut self) {
        let song = self.playback_song();
        let pattern_num = match song.arrangement.get(self.playback_pattern_idx) {
            Some(&n) => n,
            None => return,
        };

        let pattern = match song.patterns.get(pattern_num) {
            Some(p) => p,
            None => return,
        };

        // Collect note data first to avoid borrow issues
        let num_channels = song.num_channels();
        let playback_row = self.playback_row;
        let mut notes_to_play: Vec<(usize, Option<u8>, Option<u8>, Option<u8>, Option<u8>)> = Vec::new();
        let mut effects_to_apply: Vec<(usize, Effect)> = Vec::new();

        // Global reverb for this row (PS1 has single global reverb processor)
        let reverb_change = pattern.get_reverb(playback_row);

        // Get channel instruments from the playback song
        let channel_instruments: Vec<u8> = (0..num_channels)
            .map(|ch| song.get_channel_instrument(ch))
            .collect();

        // Track channels with empty rows (to clear sustain state after loop)
        let mut empty_channels: Vec<usize> = Vec::new();

        for channel in 0..num_channels {
            if let Some(note) = pattern.get(channel, playback_row) {
                if note.pitch.is_some() {
                    // Has a note - collect note data
                    let inst = note.instrument.unwrap_or(channel_instruments[channel]);
                    notes_to_play.push((channel, note.pitch, Some(inst), note.volume, None));

                    // Collect effect
                    if let (Some(fx_char), Some(fx_param)) = (note.effect, note.effect_param) {
                        let effect = Effect::from_char(fx_char, fx_param);
                        effects_to_apply.push((channel, effect));
                    }
                } else {
                    // Empty row (pitch is None) - mark for clearing sustain state
                    empty_channels.push(channel);
                }
            } else {
                // No note data at all - mark for clearing sustain state
                empty_channels.push(channel);
            }
        }

        // Clear sustain state for empty rows (so next identical note re-triggers)
        for channel in empty_channels {
            self.last_played_notes[channel] = None;
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

        // Apply reverb change if any (PS1: global reverb shared by all voices)
        if let Some(r) = reverb_change {
            let reverb_type = match r {
                0 => ReverbType::Off,
                1 => ReverbType::Room,
                2 => ReverbType::StudioSmall,
                3 => ReverbType::StudioMedium,
                4 => ReverbType::StudioLarge,
                5 => ReverbType::Hall,
                6 => ReverbType::HalfEcho,
                7 => ReverbType::SpaceEcho,
                8 => ReverbType::ChaosEcho,
                9 => ReverbType::Delay,
                _ => ReverbType::Off, // Invalid values default to off
            };
            self.audio.set_reverb_preset(reverb_type);
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
            // Note: Reverb is now handled via the dedicated reverb column, not the Fx column
        }
    }

    /// Advance playback to next row
    fn advance_playback(&mut self) {
        let song = self.playback_song();
        let pattern_num = match song.arrangement.get(self.playback_pattern_idx) {
            Some(&n) => n,
            None => {
                self.stop_playback();
                return;
            }
        };

        let pattern_len = match song.patterns.get(pattern_num) {
            Some(p) => p.length,
            None => {
                self.stop_playback();
                return;
            }
        };

        let arrangement_len = song.arrangement.len();

        self.playback_row += 1;
        if self.playback_row >= pattern_len {
            self.playback_row = 0;
            self.playback_pattern_idx += 1;
            if self.playback_pattern_idx >= arrangement_len {
                // Loop or stop
                self.playback_pattern_idx = 0; // Loop for now
            }
        }

        // Update view cursor to follow playback (only for main song, not preview)
        if self.preview_song.is_none() {
            self.current_row = self.playback_row;
            self.current_pattern_idx = self.playback_pattern_idx;
            self.ensure_row_visible();
        }
    }

    /// Convert keyboard key to MIDI note
    pub fn key_to_note(key: macroquad::prelude::KeyCode, octave: u8) -> Option<u8> {
        use macroquad::prelude::KeyCode;

        // Piano keyboard layout (3 octaves, 37 keys):
        // Bottom row: Z S X D C V G B H N J M , L . ; / (semitones 0-16)
        // Top row: Q 2 W 3 E 4 R T 5 Y 6 U I 8 O 9 P 0 [ ] (semitones 17-36)
        let base_note = octave * 12;

        let note_offset = match key {
            // Bottom row (semitones 0-16)
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
            KeyCode::Comma => Some(12), // C+1
            KeyCode::L => Some(13), // C#+1
            KeyCode::Period => Some(14), // D+1
            KeyCode::Semicolon => Some(15), // D#+1
            KeyCode::Slash => Some(16), // E+1

            // Top row (semitones 17-36)
            KeyCode::Q => Some(17), // F+1
            KeyCode::Key2 => Some(18), // F#+1
            KeyCode::W => Some(19), // G+1
            KeyCode::Key3 => Some(20), // G#+1
            KeyCode::E => Some(21), // A+1
            KeyCode::Key4 => Some(22), // A#+1
            KeyCode::R => Some(23), // B+1
            KeyCode::T => Some(24), // C+2
            KeyCode::Key5 => Some(25), // C#+2
            KeyCode::Y => Some(26), // G+2
            KeyCode::Key6 => Some(27), // G#+2
            KeyCode::U => Some(28), // A+2
            KeyCode::I => Some(29), // A#+2
            KeyCode::Key8 => Some(30), // B+2
            KeyCode::O => Some(31), // C+3
            KeyCode::Key9 => Some(32), // C#+3
            KeyCode::P => Some(33), // D+3
            KeyCode::Key0 => Some(34), // D#+3
            KeyCode::LeftBracket => Some(35), // E+3
            KeyCode::RightBracket => Some(36), // F+3

            _ => None,
        };

        note_offset.map(|offset| (base_note + offset).min(127))
    }

    // ========================================================================
    // Selection Methods
    // ========================================================================

    /// Start a new selection at the current cursor position
    pub fn start_selection(&mut self) {
        self.selection_start = Some((self.current_pattern_idx, self.current_row, self.current_channel));
        self.selection_end = Some((self.current_pattern_idx, self.current_row, self.current_channel));
    }

    /// Update selection end to current cursor position
    pub fn update_selection(&mut self) {
        if self.selection_start.is_some() {
            self.selection_end = Some((self.current_pattern_idx, self.current_row, self.current_channel));
        }
    }

    /// Clear the current selection
    pub fn clear_selection(&mut self) {
        self.selection_start = None;
        self.selection_end = None;
    }

    /// Check if there's an active selection
    pub fn has_selection(&self) -> bool {
        self.selection_start.is_some() && self.selection_end.is_some()
    }

    /// Get normalized selection bounds (start_row, end_row, start_channel, end_channel)
    /// Returns None if no selection, or if selection spans multiple patterns
    pub fn get_selection_bounds(&self) -> Option<(usize, usize, usize, usize)> {
        let (pat1, row1, ch1) = self.selection_start?;
        let (pat2, row2, ch2) = self.selection_end?;

        // Only support selection within same pattern for now
        if pat1 != pat2 {
            return None;
        }

        let start_row = row1.min(row2);
        let end_row = row1.max(row2);
        let start_ch = ch1.min(ch2);
        let end_ch = ch1.max(ch2);

        Some((start_row, end_row, start_ch, end_ch))
    }

    /// Check if a cell is within the current selection
    pub fn is_in_selection(&self, row: usize, channel: usize) -> bool {
        if let Some((start_row, end_row, start_ch, end_ch)) = self.get_selection_bounds() {
            row >= start_row && row <= end_row && channel >= start_ch && channel <= end_ch
        } else {
            false
        }
    }

    // ========================================================================
    // Copy/Paste Methods
    // ========================================================================

    /// Copy the current selection to clipboard
    pub fn copy_selection(&mut self) {
        let bounds = match self.get_selection_bounds() {
            Some(b) => b,
            None => {
                // No selection - copy single cell
                if let Some(pattern) = self.current_pattern() {
                    if let Some(note) = pattern.get(self.current_channel, self.current_row) {
                        self.clipboard = Some(vec![vec![*note]]);
                        self.set_status("Copied 1 note", 1.0);
                    }
                }
                return;
            }
        };

        let (start_row, end_row, start_ch, end_ch) = bounds;
        let pattern = match self.current_pattern() {
            Some(p) => p,
            None => return,
        };

        let num_channels = end_ch - start_ch + 1;
        let num_rows = end_row - start_row + 1;
        let mut clipboard_data: Vec<Vec<Note>> = Vec::with_capacity(num_channels);

        for ch in start_ch..=end_ch {
            let mut channel_notes: Vec<Note> = Vec::with_capacity(num_rows);
            for row in start_row..=end_row {
                let note = pattern.get(ch, row).copied().unwrap_or(Note::EMPTY);
                channel_notes.push(note);
            }
            clipboard_data.push(channel_notes);
        }

        self.clipboard = Some(clipboard_data);
        self.set_status(&format!("Copied {} notes ({} rows × {} channels)", num_rows * num_channels, num_rows, num_channels), 1.0);
    }

    /// Cut the current selection (copy then delete)
    pub fn cut_selection(&mut self) {
        self.copy_selection();
        self.delete_selection();
    }

    /// Delete notes in the current selection
    pub fn delete_selection(&mut self) {
        let bounds = match self.get_selection_bounds() {
            Some(b) => b,
            None => {
                // No selection - delete single cell
                self.delete_note();
                return;
            }
        };

        let (start_row, end_row, start_ch, end_ch) = bounds;

        if let Some(pattern) = self.current_pattern_mut() {
            for ch in start_ch..=end_ch {
                for row in start_row..=end_row {
                    pattern.set(ch, row, Note::EMPTY);
                }
            }
        }

        let count = (end_row - start_row + 1) * (end_ch - start_ch + 1);
        self.dirty = true;
        self.set_status(&format!("Deleted {} notes", count), 1.0);
        self.clear_selection();
    }

    /// Paste clipboard at current cursor position
    pub fn paste(&mut self) {
        let clipboard = match &self.clipboard {
            Some(c) => c.clone(),
            None => {
                self.set_status("Nothing to paste", 1.0);
                return;
            }
        };

        let num_clipboard_channels = clipboard.len();
        if num_clipboard_channels == 0 {
            return;
        }

        // Capture cursor position before borrowing pattern
        let start_ch = self.current_channel;
        let start_row = self.current_row;

        let pattern = match self.current_pattern_mut() {
            Some(p) => p,
            None => return,
        };

        let pattern_len = pattern.length;
        let pattern_channels = pattern.num_channels();
        let mut pasted = 0;

        for (ch_offset, channel_notes) in clipboard.iter().enumerate() {
            let target_ch = start_ch + ch_offset;
            if target_ch >= pattern_channels {
                break;
            }

            for (row_offset, note) in channel_notes.iter().enumerate() {
                let target_row = start_row + row_offset;
                if target_row >= pattern_len {
                    break;
                }
                pattern.set(target_ch, target_row, *note);
                pasted += 1;
            }
        }

        self.dirty = true;
        self.set_status(&format!("Pasted {} notes", pasted), 1.0);
    }
}

impl Default for TrackerState {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// File I/O Methods
// ============================================================================

impl TrackerState {
    /// Save the current song to a file
    pub fn save_to_file(&mut self, path: &std::path::Path) -> Result<(), String> {
        super::io::save_song(&self.song, path)?;
        self.current_file = Some(path.to_path_buf());
        self.dirty = false;
        self.set_status(&format!("Saved: {}", path.file_name().unwrap_or_default().to_string_lossy()), 2.0);
        Ok(())
    }

    /// Load a song from a file
    pub fn load_from_file(&mut self, path: &std::path::Path) -> Result<(), String> {
        let song = super::io::load_song(path)?;
        self.song = song;
        self.current_file = Some(path.to_path_buf());
        self.dirty = false;

        // Reset playback state
        self.playing = false;
        self.playback_row = 0;
        self.playback_pattern_idx = 0;
        self.current_row = 0;
        self.current_pattern_idx = 0;
        self.current_channel = 0;
        self.scroll_row = 0;
        self.clear_selection();
        self.audio.all_notes_off();

        // Make sure channel instruments and settings are synced with audio engine
        for (ch, &inst) in self.song.channel_instruments.iter().enumerate() {
            self.audio.set_program(ch as i32, inst as i32);
        }
        self.sync_all_channel_settings();

        self.set_status(&format!("Loaded: {}", path.file_name().unwrap_or_default().to_string_lossy()), 2.0);
        Ok(())
    }

    /// Create a new empty song
    pub fn new_song(&mut self) {
        self.song = Song::new();
        self.current_file = None;
        self.dirty = false;

        // Reset all state
        self.playing = false;
        self.playback_row = 0;
        self.playback_pattern_idx = 0;
        self.current_row = 0;
        self.current_pattern_idx = 0;
        self.current_channel = 0;
        self.scroll_row = 0;
        self.clear_selection();
        self.audio.all_notes_off();
        self.sync_all_channel_settings();

        self.set_status("New song created", 2.0);
    }

    /// Check if there are unsaved changes
    pub fn has_unsaved_changes(&self) -> bool {
        self.dirty
    }

    /// Get the current file name (for display)
    pub fn current_file_name(&self) -> Option<String> {
        self.current_file.as_ref().and_then(|p| {
            p.file_name().map(|f| f.to_string_lossy().to_string())
        })
    }

    /// Sync a single channel's settings to the audio engine
    pub fn sync_channel_settings(&self, channel: usize) {
        let settings = self.song.get_channel_settings(channel);
        let ch = channel as i32;
        self.audio.set_pan(ch, settings.pan as i32);
        self.audio.set_modulation(ch, settings.modulation as i32);
        self.audio.set_expression(ch, settings.expression as i32);
        // Note: wet/reverb send would need per-channel support in the audio engine
        // For now, wet is stored but the global reverb wet level applies to all
    }

    /// Sync all channel settings to the audio engine
    pub fn sync_all_channel_settings(&self) {
        for ch in 0..self.song.num_channels() {
            self.sync_channel_settings(ch);
        }
    }

    /// Update a channel setting and sync to audio
    pub fn set_channel_pan(&mut self, channel: usize, value: u8) {
        if let Some(settings) = self.song.channel_settings.get_mut(channel) {
            settings.pan = value;
            self.audio.set_pan(channel as i32, value as i32);
            self.dirty = true;
        }
    }

    pub fn set_channel_modulation(&mut self, channel: usize, value: u8) {
        if let Some(settings) = self.song.channel_settings.get_mut(channel) {
            settings.modulation = value;
            self.audio.set_modulation(channel as i32, value as i32);
            self.dirty = true;
        }
    }

    pub fn set_channel_expression(&mut self, channel: usize, value: u8) {
        if let Some(settings) = self.song.channel_settings.get_mut(channel) {
            settings.expression = value;
            self.audio.set_expression(channel as i32, value as i32);
            self.dirty = true;
        }
    }

    /// Reset channel settings to defaults
    pub fn reset_channel_settings(&mut self, channel: usize) {
        self.song.reset_channel_settings(channel);
        self.sync_channel_settings(channel);
        self.dirty = true;
    }
}
