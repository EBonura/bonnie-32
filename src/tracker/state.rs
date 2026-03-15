//! Tracker editor state

use super::audio::AudioEngine;
use super::pattern::{Song, Note, Effect, MAX_CHANNELS};
use super::spu::reverb::ReverbType;
use std::path::PathBuf;
use std::time::Instant;

/// Tracker view mode
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrackerView {
    Pattern,
    Arrangement,
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
    pub current_pattern_idx: usize,
    pub current_row: usize,
    pub current_channel: usize,
    /// Current column within channel (0=note, 1=inst, 2=vol, 3=fx, 4=fx_param)
    pub current_column: usize,

    // Edit state
    pub octave: u8,
    pub default_volume: u8,
    pub edit_mode: bool,

    // Playback state
    pub playing: bool,
    pub playback_row: usize,
    pub playback_pattern_idx: usize,
    pub playback_time: f64,

    // View state
    pub scroll_row: usize,
    pub visible_rows: usize,

    // Selection
    pub selection_start: Option<(usize, usize, usize)>,
    pub selection_end: Option<(usize, usize, usize)>,

    pub dirty: bool,
    pub status_message: Option<(String, Instant)>,
    last_played_notes: [Option<u8>; MAX_CHANNELS],

    // Effect preview values
    pub preview_pan: [u8; MAX_CHANNELS],
    pub preview_modulation: [u8; MAX_CHANNELS],
    pub preview_expression: [u8; MAX_CHANNELS],
    pub instrument_scroll: usize,

    /// Clipboard for copy/paste
    pub clipboard: Option<Vec<Vec<Note>>>,

    /// Tap tempo timestamps
    tap_times: Vec<Instant>,
}

/// Soundfont filename
const SOUNDFONT_NAME: &str = "TimGM6mb.sf2";

#[cfg(not(target_arch = "wasm32"))]
fn find_soundfont() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from(format!("assets/runtime/soundfonts/{}", SOUNDFONT_NAME)),
        std::env::current_exe().ok()
            .and_then(|p| p.parent().map(|d| d.join("assets/runtime/soundfonts").join(SOUNDFONT_NAME)))
            .unwrap_or_default(),
        PathBuf::from(SOUNDFONT_NAME),
    ];

    for path in candidates {
        if path.exists() && !path.as_os_str().is_empty() {
            return Some(path);
        }
    }
    None
}

impl TrackerState {
    pub fn new() -> Self {
        let mut audio = AudioEngine::new();

        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(sf_path) = find_soundfont() {
                match audio.load_soundfont(&sf_path) {
                    Ok(()) => log::info!("Loaded soundfont: {:?}", sf_path),
                    Err(e) => log::error!("Failed to load soundfont {:?}: {}", sf_path, e),
                }
            } else {
                log::warn!("Soundfont {} not found", SOUNDFONT_NAME);
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

            preview_pan: [64; MAX_CHANNELS],
            preview_modulation: [0; MAX_CHANNELS],
            preview_expression: [127; MAX_CHANNELS],
            instrument_scroll: 0,
            clipboard: None,
            tap_times: Vec::new(),
        }
    }

    pub fn tap_tempo(&mut self) -> Option<u16> {
        let now = Instant::now();

        if let Some(last) = self.tap_times.last() {
            if now.duration_since(*last).as_secs_f64() > 2.0 {
                self.tap_times.clear();
            }
        }

        self.tap_times.push(now);

        if self.tap_times.len() > 8 {
            self.tap_times.remove(0);
        }

        if self.tap_times.len() < 2 {
            return None;
        }

        let mut total_interval = 0.0;
        for i in 1..self.tap_times.len() {
            total_interval += self.tap_times[i].duration_since(self.tap_times[i - 1]).as_secs_f64();
        }
        let avg_interval = total_interval / (self.tap_times.len() - 1) as f64;
        let bpm = (60.0 / avg_interval).round() as u16;
        Some(bpm.clamp(40, 300))
    }

    pub fn set_status(&mut self, message: &str, duration_secs: f64) {
        let expiry = Instant::now() + std::time::Duration::from_secs_f64(duration_secs);
        self.status_message = Some((message.to_string(), expiry));
    }

    pub fn get_status(&self) -> Option<&str> {
        if let Some((msg, expiry)) = &self.status_message {
            if Instant::now() < *expiry {
                return Some(msg);
            }
        }
        None
    }

    pub fn current_pattern(&self) -> Option<&super::pattern::Pattern> {
        let pattern_num = self.song.arrangement.get(self.current_pattern_idx)?;
        self.song.patterns.get(*pattern_num)
    }

    pub fn current_pattern_mut(&mut self) -> Option<&mut super::pattern::Pattern> {
        let pattern_num = *self.song.arrangement.get(self.current_pattern_idx)?;
        self.song.patterns.get_mut(pattern_num)
    }

    pub fn current_instrument(&self) -> u8 {
        self.song.get_channel_instrument(self.current_channel)
    }

    pub fn set_current_instrument(&mut self, instrument: u8) {
        self.song.set_channel_instrument(self.current_channel, instrument);
        self.audio.set_program(self.current_channel as i32, instrument as i32);
    }

    pub fn set_preview_pan(&mut self, value: u8) {
        self.preview_pan[self.current_channel] = value;
        self.audio.set_pan(self.current_channel as i32, value as i32);
    }

    pub fn set_preview_modulation(&mut self, value: u8) {
        self.preview_modulation[self.current_channel] = value;
        self.audio.set_modulation(self.current_channel as i32, value as i32);
    }

    pub fn set_preview_expression(&mut self, value: u8) {
        self.preview_expression[self.current_channel] = value;
        self.audio.set_expression(self.current_channel as i32, value as i32);
    }

    pub fn reset_preview_effects(&mut self) {
        let ch = self.current_channel;
        self.preview_pan[ch] = 64;
        self.preview_modulation[ch] = 0;
        self.preview_expression[ch] = 127;
        self.audio.reset_controllers(ch as i32);
    }

    pub fn num_channels(&self) -> usize {
        self.song.num_channels()
    }

    pub fn add_channel(&mut self) {
        self.song.add_channel();
    }

    pub fn remove_channel(&mut self) {
        self.song.remove_channel();
        if self.current_channel >= self.song.num_channels() {
            self.current_channel = self.song.num_channels() - 1;
        }
    }

    pub fn pattern_length(&self) -> usize {
        self.current_pattern().map(|p| p.length).unwrap_or(64)
    }

    pub fn increase_pattern_length(&mut self) {
        let new_len = (self.pattern_length() + 16).min(256);
        if let Some(pattern) = self.current_pattern_mut() {
            pattern.set_length(new_len);
        }
        self.dirty = true;
    }

    pub fn decrease_pattern_length(&mut self) {
        let new_len = self.pattern_length().saturating_sub(16).max(16);
        if let Some(pattern) = self.current_pattern_mut() {
            pattern.set_length(new_len);
        }
        if self.current_row >= new_len {
            self.current_row = new_len - 1;
        }
        self.dirty = true;
    }

    // ========================================================================
    // Playback
    // ========================================================================

    pub fn play(&mut self) {
        if !self.playing {
            self.playing = true;
            self.playback_row = self.current_row;
            self.playback_pattern_idx = self.current_pattern_idx;
            self.playback_time = 0.0;
            self.apply_channel_settings();
        }
    }

    pub fn stop(&mut self) {
        self.playing = false;
        self.audio.all_notes_off();
        self.audio.clear_reverb();
        self.last_played_notes = [None; MAX_CHANNELS];
    }

    pub fn toggle_play(&mut self) {
        if self.playing {
            self.stop();
        } else {
            self.play();
        }
    }

    /// Apply all channel settings to the audio engine (instruments, volumes, etc.)
    fn apply_channel_settings(&self) {
        for ch in 0..self.song.num_channels() {
            let instrument = self.song.get_channel_instrument(ch);
            self.audio.set_program(ch as i32, instrument as i32);

            let settings = self.song.get_channel_settings(ch);
            self.audio.set_pan(ch as i32, settings.pan as i32);
            self.audio.set_expression(ch as i32, settings.expression as i32);
            self.audio.set_modulation(ch as i32, settings.modulation as i32);
            self.audio.set_reverb_send(ch as i32, settings.effect_amount as i32);
        }

        // Apply global reverb
        let reverb_type = ReverbType::from_index(self.song.reverb.preset);
        self.audio.set_reverb_preset(reverb_type);
        self.audio.set_reverb_wet_level(self.song.reverb.wet as f32 / 127.0);

        // Apply master volume
        self.audio.set_master_volume(self.song.master_volume as f32 / 100.0);
    }

    /// Advance playback by dt seconds. Returns true if a row was advanced.
    pub fn tick_playback(&mut self, dt: f64) -> bool {
        if !self.playing {
            return false;
        }

        self.playback_time += dt;
        let tick_duration = self.song.tick_duration();

        if self.playback_time < tick_duration {
            return false;
        }
        self.playback_time -= tick_duration;

        // Play notes on current row
        self.play_row(self.playback_pattern_idx, self.playback_row);

        // Advance row
        self.playback_row += 1;
        let pattern_len = self.song.arrangement.get(self.playback_pattern_idx)
            .and_then(|&idx| self.song.patterns.get(idx))
            .map(|p| p.length)
            .unwrap_or(64);

        if self.playback_row >= pattern_len {
            self.playback_row = 0;
            self.playback_pattern_idx += 1;
            if self.playback_pattern_idx >= self.song.arrangement.len() {
                self.playback_pattern_idx = 0; // loop
            }
        }

        // Follow cursor
        self.current_row = self.playback_row;
        self.current_pattern_idx = self.playback_pattern_idx;

        true
    }

    fn play_row(&mut self, pattern_idx: usize, row: usize) {
        let pattern_num = match self.song.arrangement.get(pattern_idx) {
            Some(&n) => n,
            None => return,
        };
        let pattern = match self.song.patterns.get(pattern_num) {
            Some(p) => p,
            None => return,
        };

        // Apply global reverb change if present
        if let Some(reverb_preset) = pattern.get_reverb(row) {
            let reverb_type = ReverbType::from_index(reverb_preset);
            self.audio.set_reverb_preset(reverb_type);
        }

        for ch in 0..pattern.num_channels().min(MAX_CHANNELS) {
            if let Some(note) = pattern.get(ch, row) {
                if note.is_empty() {
                    continue;
                }

                // Handle note-off
                if note.is_off() {
                    self.audio.channel_notes_off(ch as i32);
                    self.last_played_notes[ch] = None;
                    continue;
                }

                // Apply volume
                if let Some(vol) = note.volume {
                    self.audio.set_volume(ch as i32, vol as i32);
                }

                // Apply effects
                if let Some(fx_char) = note.effect {
                    let param = note.effect_param.unwrap_or(0);
                    let effect = Effect::from_char(fx_char, param);
                    self.apply_effect(ch, effect);
                }

                // Play note
                if let Some(pitch) = note.pitch {
                    if pitch != 0xFF {
                        // Set instrument if specified
                        if let Some(inst) = note.instrument {
                            self.audio.set_program(ch as i32, inst as i32);
                        }

                        let velocity = note.volume.unwrap_or(self.default_volume);
                        self.audio.note_on(ch as i32, pitch as i32, velocity as i32);
                        self.last_played_notes[ch] = Some(pitch);
                    }
                }
            }
        }
    }

    fn apply_effect(&self, channel: usize, effect: Effect) {
        let ch = channel as i32;
        match effect {
            Effect::SetVolume(v) => self.audio.set_volume(ch, v as i32),
            Effect::SetPan(p) => self.audio.set_pan(ch, p as i32),
            Effect::SetExpression(e) => self.audio.set_expression(ch, e as i32),
            Effect::SetModulation(m) => self.audio.set_modulation(ch, m as i32),
            Effect::SlideUp(p) => {
                let bend = 8192 + (p as i32) * 64;
                self.audio.set_pitch_bend(ch, bend);
            }
            Effect::SlideDown(p) => {
                let bend = 8192 - (p as i32) * 64;
                self.audio.set_pitch_bend(ch, bend);
            }
            _ => {}
        }
    }

    // ========================================================================
    // Song I/O
    // ========================================================================

    pub fn save_song(&mut self) -> Result<(), String> {
        let path = self.current_file.as_ref()
            .ok_or_else(|| "No file path set".to_string())?;

        let config = ron::ser::PrettyConfig::new()
            .depth_limit(4)
            .indentor("  ".to_string());
        let ron_string = ron::ser::to_string_pretty(&self.song, config)
            .map_err(|e| format!("Serialize error: {}", e))?;
        std::fs::write(path, ron_string)
            .map_err(|e| format!("Write error: {}", e))?;

        self.dirty = false;
        self.set_status("Song saved", 2.0);
        Ok(())
    }

    pub fn load_song(&mut self, path: &std::path::Path) -> Result<(), String> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| format!("Read error: {}", e))?;
        let song: Song = ron::from_str(&contents)
            .map_err(|e| format!("Parse error: {}", e))?;

        self.stop();
        self.song = song;
        self.current_file = Some(path.to_path_buf());
        self.current_row = 0;
        self.current_pattern_idx = 0;
        self.current_channel = 0;
        self.dirty = false;
        self.apply_channel_settings();
        self.set_status("Song loaded", 2.0);
        Ok(())
    }

    pub fn new_song(&mut self) {
        self.stop();
        self.song = Song::new();
        self.current_file = None;
        self.current_row = 0;
        self.current_pattern_idx = 0;
        self.current_channel = 0;
        self.dirty = false;
        self.apply_channel_settings();
    }
}

impl Default for TrackerState {
    fn default() -> Self {
        Self::new()
    }
}
