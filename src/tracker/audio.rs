//! Audio engine using PS1 SPU Core for playback
//!
//! Platform-specific audio output:
//! - Native: cpal for direct audio device access
//! - WASM: Web Audio API via JavaScript FFI
//!
//! Features hardware-accurate PS1 SPU DSP: per-voice ADPCM decode,
//! Gaussian interpolation, ADSR envelopes, and reverb processing.
//! SF2 soundfonts are converted to PS1 ADPCM at load time.

use std::sync::{Arc, Mutex, MutexGuard};
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use super::spu::SpuCore;
use super::spu::reverb::ReverbType;
use super::spu::convert::{parse_sf2, GM_NAMES};
use super::spu::tables::MAX_VOICES;

/// Lock a mutex, recovering gracefully from poisoning.
///
/// A mutex becomes poisoned when a thread panics while holding the lock.
/// This can happen if `spu.tick()` panics in the audio callback, or if
/// `ensure_program_loaded()` panics during ADPCM encoding. Rather than
/// crashing the entire application, we recover the inner data and continue.
fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| {
        #[cfg(not(target_arch = "wasm32"))]
        eprintln!("SPU: mutex was poisoned, recovering");
        poisoned.into_inner()
    })
}

/// Sample rate for audio output
pub const SAMPLE_RATE: u32 = 44100;

/// Output gain multiplier — boosts overall volume
const OUTPUT_GAIN: f32 = 2.0;

/// PS1 SPU Pitch Register emulation
///
/// The PS1 SPU uses a 16-bit pitch register where:
/// - 0x1000 (4096) = 44100 Hz (1:1 playback, native rate)
/// - Formula: effective_rate = (pitch / 0x1000) * 44100
/// - Range: 0x0000 (stopped) to 0x4000 (176.4kHz, clamped max)
///
/// Common pitch values used in PS1 games:
/// - 0x1000 = 44100 Hz
/// - 0x0800 = 22050 Hz
/// - 0x0400 = 11025 Hz
/// - 0x0200 = 5512 Hz
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpuPitch(pub u16);

impl SpuPitch {
    /// Native 44.1kHz - pitch 0x1000
    pub const NATIVE: SpuPitch = SpuPitch(0x1000);
    /// 22050 Hz - pitch 0x0800 (common PS1 sample rate)
    pub const PS1_22K: SpuPitch = SpuPitch(0x0800);
    /// 11025 Hz - pitch 0x0400 (lo-fi PS1)
    pub const PS1_11K: SpuPitch = SpuPitch(0x0400);
    /// 5512 Hz - pitch 0x0200 (very crunchy)
    pub const PS1_5K: SpuPitch = SpuPitch(0x0200);

    /// Preset pitch values for UI buttons
    pub const PRESETS: [SpuPitch; 4] = [
        Self::NATIVE,
        Self::PS1_22K,
        Self::PS1_11K,
        Self::PS1_5K,
    ];

    /// Get display name for this pitch setting
    pub fn name(&self) -> &'static str {
        match self.0 {
            0x1000 => "44kHz",
            0x0800 => "22kHz",
            0x0400 => "11kHz",
            0x0200 => "5kHz",
            _ => "Custom",
        }
    }

    /// Calculate effective sample rate in Hz
    /// Formula: (pitch / 0x1000) * 44100
    pub fn effective_rate(&self) -> u32 {
        ((self.0 as u32) * 44100) / 0x1000
    }

    /// Get the downsampling factor based on pitch
    /// pitch 0x1000 = factor 1 (no downsampling)
    /// pitch 0x0800 = factor 2 (half rate)
    /// pitch 0x0400 = factor 4 (quarter rate)
    /// pitch 0x0200 = factor 8 (eighth rate)
    pub fn factor(&self) -> usize {
        let pitch = self.0.clamp(1, 0x1000);
        (0x1000 / pitch as usize).max(1)
    }

    /// Backward compatibility: provide ALL as alias for PRESETS
    pub const ALL: [SpuPitch; 4] = Self::PRESETS;
}

impl Default for SpuPitch {
    fn default() -> Self {
        Self::NATIVE
    }
}

// Keep backward compatibility alias
pub type OutputSampleRate = SpuPitch;

/// Audio engine state shared between main thread and audio callback
struct AudioState {
    /// PS1 SPU core — 24 voices with ADPCM, Gaussian interp, ADSR, reverb
    spu: SpuCore,
    /// Whether audio is playing
    playing: bool,
    /// Per-channel volume (MIDI CC 7), 0-127
    channel_volume: [u8; MAX_VOICES],
    /// Per-channel expression (MIDI CC 11), 0-127
    channel_expression: [u8; MAX_VOICES],
    /// Per-channel pan (MIDI CC 10), 0-127 (64=center)
    channel_pan: [u8; MAX_VOICES],
    /// Per-channel modulation (MIDI CC 1), stored for UI display
    channel_modulation: [u8; MAX_VOICES],
    /// Per-channel base pitch (set during note_on, for pitch bend calculation)
    channel_base_pitch: [u16; MAX_VOICES],
    /// Per-channel pitch bend (0-16383, center=8192)
    channel_pitch_bend: [i32; MAX_VOICES],
    /// Global sample rate mode (backward compat for UI)
    output_sample_rate: SpuPitch,
    /// Whether SPU resampling mode is enabled (backward compat)
    spu_resampling_enabled: bool,
}

impl AudioState {
    fn new() -> Self {
        Self {
            spu: SpuCore::new(),
            playing: false,
            channel_volume: [100u8; MAX_VOICES],
            channel_expression: [127u8; MAX_VOICES],
            channel_pan: [64u8; MAX_VOICES],
            channel_modulation: [0u8; MAX_VOICES],
            channel_base_pitch: [0u16; MAX_VOICES],
            channel_pitch_bend: [8192i32; MAX_VOICES],
            output_sample_rate: SpuPitch::NATIVE,
            spu_resampling_enabled: true,
        }
    }

    /// Calculate effective volume from CC7 (volume) and CC11 (expression)
    fn effective_volume(&self, channel: usize) -> u8 {
        let v = self.channel_volume[channel] as u16;
        let e = self.channel_expression[channel] as u16;
        ((v * e) / 127).min(127) as u8
    }

    /// Sync pan + volume to the SPU voice for a channel
    fn sync_voice_volume(&mut self, channel: usize) {
        let vol = self.effective_volume(channel);
        let pan = self.channel_pan[channel];
        self.spu.set_voice_pan(channel, pan, vol);
    }

    /// Apply pitch bend to a voice using stored base_pitch
    fn apply_pitch_bend(&mut self, channel: usize) {
        let base = self.channel_base_pitch[channel] as f64;
        if base == 0.0 {
            return;
        }
        let bend = self.channel_pitch_bend[channel];
        // Pitch bend range: +/- 2 semitones
        let semitones = (bend - 8192) as f64 / 8192.0 * 2.0;
        let ratio = (semitones / 12.0).exp2();
        let new_pitch = (base * ratio) as u16;
        self.spu.set_voice_pitch(channel, new_pitch.min(0x3FFF));
    }
}

// =============================================================================
// Native audio output using cpal
// =============================================================================

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{Stream, SampleRate, StreamConfig};

    pub fn init_audio_stream(state: Arc<Mutex<AudioState>>) -> Option<Stream> {
        let host = cpal::default_host();
        let device = host.default_output_device()?;

        let config = StreamConfig {
            channels: 2,
            sample_rate: SampleRate(super::SAMPLE_RATE),
            buffer_size: cpal::BufferSize::Default,
        };

        let stream = device.build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                // Wrap in catch_unwind to prevent panics from aborting
                // (this callback is called from CoreAudio via extern "C",
                // so panics cannot unwind and would cause a hard abort)
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut state = lock_or_recover(&state);
                    let samples_needed = data.len() / 2;

                    for i in 0..samples_needed {
                        let (l, r) = state.spu.tick();
                        data[i * 2] = l * OUTPUT_GAIN;
                        data[i * 2 + 1] = r * OUTPUT_GAIN;
                    }
                }));

                if result.is_err() {
                    // Panic occurred inside tick — output silence
                    for sample in data.iter_mut() {
                        *sample = 0.0;
                    }
                }
            },
            |err| eprintln!("Audio stream error: {}", err),
            None,
        ).ok()?;

        stream.play().ok()?;
        Some(stream)
    }
}

// =============================================================================
// WASM audio output using Web Audio API via JavaScript
// =============================================================================

#[cfg(target_arch = "wasm32")]
pub mod wasm {
    use super::*;

    extern "C" {
        // Soundfont cache
        fn b32_is_soundfont_loaded() -> i32;
        fn b32_get_soundfont_size() -> usize;
        fn b32_copy_soundfont(dest_ptr: *mut u8, max_len: usize) -> usize;
        // Audio output
        fn b32_audio_init(sample_rate: u32);
        fn b32_audio_write(left_ptr: *const f32, right_ptr: *const f32, len: usize);
    }

    pub fn is_soundfont_cached() -> bool {
        unsafe { b32_is_soundfont_loaded() != 0 }
    }

    pub fn get_cached_soundfont() -> Option<Vec<u8>> {
        unsafe {
            let size = b32_get_soundfont_size();
            if size == 0 {
                return None;
            }

            let mut buffer = vec![0u8; size];
            let copied = b32_copy_soundfont(buffer.as_mut_ptr(), size);

            if copied != size {
                return None;
            }

            Some(buffer)
        }
    }

    pub fn init_audio() {
        unsafe { b32_audio_init(SAMPLE_RATE) }
    }

    pub fn write_audio(left: &[f32], right: &[f32]) {
        let len = left.len().min(right.len());
        unsafe { b32_audio_write(left.as_ptr(), right.as_ptr(), len) }
    }
}

// =============================================================================
// AudioEngine - cross-platform wrapper
// =============================================================================

/// The audio engine manages SF2 loading and note playback via the PS1 SPU core
pub struct AudioEngine {
    /// Shared state (accessed from audio callback on native)
    state: Arc<Mutex<AudioState>>,
    /// The audio stream (native only, kept alive)
    #[cfg(not(target_arch = "wasm32"))]
    _stream: Option<cpal::Stream>,
    /// Loaded soundfont info (cached for borrowing without lock)
    soundfont_name: Option<String>,
    /// Audio render buffers (WASM only - we render on demand)
    #[cfg(target_arch = "wasm32")]
    left_buffer: Vec<f32>,
    #[cfg(target_arch = "wasm32")]
    right_buffer: Vec<f32>,
    /// Accumulated fractional samples (WASM only - for timing accuracy)
    #[cfg(target_arch = "wasm32")]
    sample_accumulator: f64,
}

impl AudioEngine {
    /// Create a new audio engine (no soundfont loaded yet)
    pub fn new() -> Self {
        let state = Arc::new(Mutex::new(AudioState::new()));

        #[cfg(not(target_arch = "wasm32"))]
        {
            let stream = native::init_audio_stream(Arc::clone(&state));
            Self {
                state,
                _stream: stream,
                soundfont_name: None,
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            wasm::init_audio();
            Self {
                state,
                soundfont_name: None,
                left_buffer: vec![0.0; 2048],
                right_buffer: vec![0.0; 2048],
                sample_accumulator: 0.0,
            }
        }
    }

    // =========================================================================
    // Reverb controls
    // =========================================================================

    /// Set the PS1 reverb preset
    pub fn set_reverb_preset(&self, reverb_type: ReverbType) {
        let mut state = lock_or_recover(&self.state);
        state.spu.set_reverb_preset(reverb_type);
    }

    /// Get current reverb type
    pub fn reverb_type(&self) -> ReverbType {
        lock_or_recover(&self.state).spu.reverb_type()
    }

    /// Set reverb wet/dry mix (0.0 = dry, 1.0 = wet)
    pub fn set_reverb_wet_level(&self, level: f32) {
        let mut state = lock_or_recover(&self.state);
        state.spu.set_reverb_wet_level(level);
    }

    /// Get reverb wet level
    pub fn reverb_wet_level(&self) -> f32 {
        lock_or_recover(&self.state).spu.reverb_wet_level()
    }

    /// Clear reverb buffers (call when stopping playback)
    pub fn clear_reverb(&self) {
        let mut state = lock_or_recover(&self.state);
        state.spu.clear_reverb();
    }

    // =========================================================================
    // Sample rate / resampling controls (backward compat)
    // =========================================================================

    /// Set output sample rate mode
    ///
    /// With per-voice SPU Gaussian interpolation, sample rate degradation
    /// is inherent in the voice pitch. This setting is stored for UI display.
    pub fn set_output_sample_rate(&self, rate: OutputSampleRate) {
        let mut state = lock_or_recover(&self.state);
        state.output_sample_rate = rate;
    }

    /// Get current output sample rate mode
    pub fn output_sample_rate(&self) -> OutputSampleRate {
        lock_or_recover(&self.state).output_sample_rate
    }

    // =========================================================================
    // Master volume
    // =========================================================================

    /// Set master volume (0.0 to 2.0)
    pub fn set_master_volume(&self, volume: f32) {
        let mut state = lock_or_recover(&self.state);
        state.spu.set_master_volume(volume);
    }

    /// Get master volume
    pub fn master_volume(&self) -> f32 {
        lock_or_recover(&self.state).spu.master_volume()
    }

    /// Enable or disable SPU resampling emulation (backward compat)
    pub fn set_spu_resampling_enabled(&self, enabled: bool) {
        let mut state = lock_or_recover(&self.state);
        state.spu_resampling_enabled = enabled;
    }

    /// Check if SPU resampling is enabled
    pub fn is_spu_resampling_enabled(&self) -> bool {
        lock_or_recover(&self.state).spu_resampling_enabled
    }

    // =========================================================================
    // Soundfont loading
    // =========================================================================

    /// Load a soundfont from file (native only)
    ///
    /// Reads the SF2 file, extracts PCM samples, encodes them to PS1 ADPCM,
    /// and loads the resulting sample library into the SPU core.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_soundfont(&mut self, path: &Path) -> Result<(), String> {
        let bytes = std::fs::read(path)
            .map_err(|e| format!("Failed to read soundfont: {}", e))?;
        let name = path.file_name()
            .map(|n| n.to_string_lossy().to_string());
        self.load_soundfont_from_bytes(&bytes, name)
    }

    /// Load a soundfont from bytes (works on all platforms including WASM)
    ///
    /// Parses the SF2 and stores it for on-demand instrument conversion.
    /// Individual programs are converted to ADPCM lazily when needed.
    pub fn load_soundfont_from_bytes(&mut self, bytes: &[u8], name: Option<String>) -> Result<(), String> {
        let sf_name = name.as_deref().unwrap_or("unknown.sf2").to_string();
        let soundfont = parse_sf2(bytes)?;

        #[cfg(not(target_arch = "wasm32"))]
        eprintln!("SPU: parsed SF2 '{}', instruments will be loaded on demand", sf_name);

        let mut state = lock_or_recover(&self.state);
        state.spu.load_soundfont(soundfont, sf_name);
        state.playing = true;

        self.soundfont_name = name;
        Ok(())
    }

    /// Check if a soundfont is loaded
    pub fn is_loaded(&self) -> bool {
        lock_or_recover(&self.state).spu.is_loaded()
    }

    /// Get the loaded soundfont name
    pub fn soundfont_name(&self) -> Option<&str> {
        self.soundfont_name.as_deref()
    }

    // =========================================================================
    // WASM audio rendering
    // =========================================================================

    /// Render and output audio (WASM only - must be called each frame with delta time)
    #[cfg(target_arch = "wasm32")]
    pub fn render_audio(&mut self, delta: f64) {
        let mut state = lock_or_recover(&self.state);

        // Calculate exact samples needed based on actual elapsed time
        self.sample_accumulator += delta * SAMPLE_RATE as f64;

        // Only render whole samples
        let samples = self.sample_accumulator as usize;
        if samples == 0 {
            return;
        }

        // Keep fractional part for next frame
        self.sample_accumulator -= samples as f64;

        // Cap to reasonable max (prevents runaway if tab was backgrounded)
        let samples = samples.min(4096);

        if self.left_buffer.len() < samples {
            self.left_buffer.resize(samples, 0.0);
            self.right_buffer.resize(samples, 0.0);
        }

        // Generate audio sample-by-sample from SPU core
        for i in 0..samples {
            let (l, r) = state.spu.tick();
            self.left_buffer[i] = l * OUTPUT_GAIN;
            self.right_buffer[i] = r * OUTPUT_GAIN;
        }

        wasm::write_audio(&self.left_buffer[..samples], &self.right_buffer[..samples]);
    }

    // =========================================================================
    // Note control
    // =========================================================================

    /// Play a note (note on)
    ///
    /// Triggers the SPU voice for the given channel with the current program.
    pub fn note_on(&self, channel: i32, key: i32, velocity: i32) {
        let mut state = lock_or_recover(&self.state);
        let ch = channel as usize;
        #[cfg(not(target_arch = "wasm32"))]
        eprintln!("AudioEngine::note_on called: ch={} key={} vel={} loaded={}", channel, key, velocity, state.spu.is_loaded());
        if ch >= MAX_VOICES {
            return;
        }
        let program = state.spu.program(ch);
        let note = (key as u8).min(127);
        let vel = (velocity as u8).min(127);

        state.spu.note_on(ch, program, note, vel);

        // Store base pitch for pitch bend calculations
        state.channel_base_pitch[ch] = state.spu.voice_pitch(ch);
        // Reset pitch bend to center
        state.channel_pitch_bend[ch] = 8192;

        // Apply current pan/volume settings to the new voice
        state.sync_voice_volume(ch);
    }

    /// Stop a note (note off)
    pub fn note_off(&self, channel: i32, _key: i32) {
        let mut state = lock_or_recover(&self.state);
        let ch = channel as usize;
        if ch >= MAX_VOICES {
            return;
        }
        state.spu.note_off(ch);
    }

    /// Stop all notes
    pub fn all_notes_off(&self) {
        let mut state = lock_or_recover(&self.state);
        state.spu.all_notes_off();
    }

    // =========================================================================
    // Channel controls
    // =========================================================================

    /// Set the instrument (program) for a channel
    pub fn set_program(&self, channel: i32, program: i32) {
        let mut state = lock_or_recover(&self.state);
        let ch = channel as usize;
        if ch >= MAX_VOICES {
            return;
        }
        state.spu.set_program(ch, program as u8);
    }

    /// Set channel volume (CC 7)
    pub fn set_volume(&self, channel: i32, volume: i32) {
        let mut state = lock_or_recover(&self.state);
        let ch = channel as usize;
        if ch >= MAX_VOICES {
            return;
        }
        state.channel_volume[ch] = (volume as u8).min(127);
        state.sync_voice_volume(ch);
    }

    /// Set channel pan (CC 10)
    pub fn set_pan(&self, channel: i32, pan: i32) {
        let mut state = lock_or_recover(&self.state);
        let ch = channel as usize;
        if ch >= MAX_VOICES {
            return;
        }
        state.channel_pan[ch] = (pan as u8).min(127);
        state.sync_voice_volume(ch);
    }

    /// Set pitch bend (0-16383, center = 8192)
    pub fn set_pitch_bend(&self, channel: i32, value: i32) {
        let mut state = lock_or_recover(&self.state);
        let ch = channel as usize;
        if ch >= MAX_VOICES {
            return;
        }
        state.channel_pitch_bend[ch] = value.clamp(0, 16383);
        state.apply_pitch_bend(ch);
    }

    /// Set modulation wheel (CC 1)
    pub fn set_modulation(&self, channel: i32, value: i32) {
        let mut state = lock_or_recover(&self.state);
        let ch = channel as usize;
        if ch >= MAX_VOICES {
            return;
        }
        state.channel_modulation[ch] = (value as u8).min(127);
        // Modulation is stored for UI display but not directly applied to SPU
        // (PS1 SPU has no hardware modulation wheel support)
    }

    /// Set expression (CC 11)
    pub fn set_expression(&self, channel: i32, value: i32) {
        let mut state = lock_or_recover(&self.state);
        let ch = channel as usize;
        if ch >= MAX_VOICES {
            return;
        }
        state.channel_expression[ch] = (value as u8).min(127);
        state.sync_voice_volume(ch);
    }

    /// Reset all controllers on a channel
    pub fn reset_controllers(&self, channel: i32) {
        let mut state = lock_or_recover(&self.state);
        let ch = channel as usize;
        if ch >= MAX_VOICES {
            return;
        }
        state.channel_volume[ch] = 100;
        state.channel_expression[ch] = 127;
        state.channel_pan[ch] = 64;
        state.channel_modulation[ch] = 0;
        state.channel_pitch_bend[ch] = 8192;
        state.sync_voice_volume(ch);
    }

    // =========================================================================
    // Instrument info
    // =========================================================================

    /// Get list of preset names from the loaded soundfont
    /// Returns (bank, program, name) tuples for all 128 GM melodic instruments
    pub fn get_preset_names(&self) -> Vec<(u8, u8, String)> {
        GM_NAMES.iter().enumerate()
            .map(|(i, name)| (0, i as u8, name.to_string()))
            .collect()
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}
