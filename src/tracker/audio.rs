//! Audio engine using rustysynth for SF2 playback
//!
//! Platform-specific audio output:
//! - Native: cpal for direct audio device access
//! - WASM: Web Audio API via JavaScript FFI
//!
//! Features authentic PS1 SPU reverb emulation.

use std::sync::{Arc, Mutex};
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
#[cfg(not(target_arch = "wasm32"))]
use std::fs::File;
use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};
use super::psx_reverb::{PsxReverb, ReverbType};

/// Sample rate for audio output
pub const SAMPLE_RATE: u32 = 44100;

/// Audio engine state shared between main thread and audio callback
struct AudioState {
    /// The synthesizer
    synth: Option<Synthesizer>,
    /// Whether audio is playing
    playing: bool,
    /// PS1 SPU reverb processor
    reverb: PsxReverb,
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
            sample_rate: SampleRate(SAMPLE_RATE),
            buffer_size: cpal::BufferSize::Default,
        };

        let mut left_buffer = vec![0.0f32; 1024];
        let mut right_buffer = vec![0.0f32; 1024];

        let stream = device.build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let mut state = state.lock().unwrap();

                if let Some(ref mut synth) = state.synth {
                    let samples_needed = data.len() / 2;
                    if left_buffer.len() < samples_needed {
                        left_buffer.resize(samples_needed, 0.0);
                        right_buffer.resize(samples_needed, 0.0);
                    }

                    synth.render(&mut left_buffer[..samples_needed], &mut right_buffer[..samples_needed]);

                    // Apply PS1 reverb
                    state.reverb.process(&mut left_buffer[..samples_needed], &mut right_buffer[..samples_needed]);

                    for i in 0..samples_needed {
                        data[i * 2] = left_buffer[i];
                        data[i * 2 + 1] = right_buffer[i];
                    }
                } else {
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
        fn bonnie_is_soundfont_loaded() -> i32;
        fn bonnie_get_soundfont_size() -> usize;
        fn bonnie_copy_soundfont(dest_ptr: *mut u8, max_len: usize) -> usize;
        // Audio output
        fn bonnie_audio_init(sample_rate: u32);
        fn bonnie_audio_write(left_ptr: *const f32, right_ptr: *const f32, len: usize);
    }

    pub fn is_soundfont_cached() -> bool {
        unsafe { bonnie_is_soundfont_loaded() != 0 }
    }

    pub fn get_cached_soundfont() -> Option<Vec<u8>> {
        unsafe {
            let size = bonnie_get_soundfont_size();
            if size == 0 {
                return None;
            }

            let mut buffer = vec![0u8; size];
            let copied = bonnie_copy_soundfont(buffer.as_mut_ptr(), size);

            if copied != size {
                return None;
            }

            Some(buffer)
        }
    }

    pub fn init_audio() {
        unsafe { bonnie_audio_init(SAMPLE_RATE) }
    }

    pub fn write_audio(left: &[f32], right: &[f32]) {
        let len = left.len().min(right.len());
        unsafe { bonnie_audio_write(left.as_ptr(), right.as_ptr(), len) }
    }
}

// =============================================================================
// AudioEngine - cross-platform wrapper
// =============================================================================

/// The audio engine manages SF2 loading and note playback
pub struct AudioEngine {
    /// Shared state
    state: Arc<Mutex<AudioState>>,
    /// The audio stream (native only, kept alive)
    #[cfg(not(target_arch = "wasm32"))]
    _stream: Option<cpal::Stream>,
    /// Loaded soundfont info
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
        let state = Arc::new(Mutex::new(AudioState {
            synth: None,
            playing: false,
            reverb: PsxReverb::new(SAMPLE_RATE),
        }));

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

    /// Set the PS1 reverb preset
    pub fn set_reverb_preset(&self, reverb_type: ReverbType) {
        let mut state = self.state.lock().unwrap();
        state.reverb.set_preset(reverb_type);
    }

    /// Get current reverb type
    pub fn reverb_type(&self) -> ReverbType {
        self.state.lock().unwrap().reverb.reverb_type()
    }

    /// Set reverb wet/dry mix (0.0 = dry, 1.0 = wet)
    pub fn set_reverb_wet_level(&self, level: f32) {
        let mut state = self.state.lock().unwrap();
        state.reverb.set_wet_level(level);
    }

    /// Get reverb wet level
    pub fn reverb_wet_level(&self) -> f32 {
        self.state.lock().unwrap().reverb.wet_level()
    }

    /// Clear reverb buffers (call when stopping playback)
    pub fn clear_reverb(&self) {
        let mut state = self.state.lock().unwrap();
        state.reverb.clear();
    }

    /// Load a soundfont from file (native only)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_soundfont(&mut self, path: &Path) -> Result<(), String> {
        let file = File::open(path)
            .map_err(|e| format!("Failed to open soundfont: {}", e))?;

        let mut reader = std::io::BufReader::new(file);
        self.load_soundfont_from_reader(&mut reader, path.file_name()
            .map(|n| n.to_string_lossy().to_string()))
    }

    /// Load a soundfont from bytes (works on all platforms including WASM)
    pub fn load_soundfont_from_bytes(&mut self, bytes: &[u8], name: Option<String>) -> Result<(), String> {
        let mut cursor = std::io::Cursor::new(bytes);
        self.load_soundfont_from_reader(&mut cursor, name)
    }

    /// Internal: Load soundfont from any reader
    fn load_soundfont_from_reader<R: std::io::Read>(&mut self, reader: &mut R, name: Option<String>) -> Result<(), String> {
        let soundfont = SoundFont::new(reader)
            .map_err(|e| format!("Failed to parse soundfont: {:?}", e))?;

        let soundfont = Arc::new(soundfont);

        let settings = SynthesizerSettings::new(SAMPLE_RATE as i32);
        let synth = Synthesizer::new(&soundfont, &settings)
            .map_err(|e| format!("Failed to create synthesizer: {:?}", e))?;

        self.soundfont_name = name;

        let mut state = self.state.lock().unwrap();
        state.synth = Some(synth);
        state.playing = true;

        Ok(())
    }

    /// Check if a soundfont is loaded
    pub fn is_loaded(&self) -> bool {
        self.state.lock().unwrap().synth.is_some()
    }

    /// Get the loaded soundfont name
    pub fn soundfont_name(&self) -> Option<&str> {
        self.soundfont_name.as_deref()
    }

    /// Render and output audio (WASM only - must be called each frame with delta time)
    #[cfg(target_arch = "wasm32")]
    pub fn render_audio(&mut self, delta: f64) {
        let mut state = self.state.lock().unwrap();
        if let Some(ref mut synth) = state.synth {
            // Calculate exact samples needed based on actual elapsed time
            // delta is in seconds, sample_rate is 44100 samples/sec
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
            synth.render(&mut self.left_buffer[..samples], &mut self.right_buffer[..samples]);

            // Apply PS1 reverb
            state.reverb.process(&mut self.left_buffer[..samples], &mut self.right_buffer[..samples]);

            wasm::write_audio(&self.left_buffer[..samples], &self.right_buffer[..samples]);
        }
    }

    /// Play a note (note on)
    pub fn note_on(&self, channel: i32, key: i32, velocity: i32) {
        let mut state = self.state.lock().unwrap();
        if let Some(ref mut synth) = state.synth {
            synth.note_on(channel, key, velocity);
        }
    }

    /// Stop a note (note off)
    pub fn note_off(&self, channel: i32, key: i32) {
        let mut state = self.state.lock().unwrap();
        if let Some(ref mut synth) = state.synth {
            synth.note_off(channel, key);
        }
    }

    /// Stop all notes
    pub fn all_notes_off(&self) {
        let mut state = self.state.lock().unwrap();
        if let Some(ref mut synth) = state.synth {
            for channel in 0..16 {
                for key in 0..128 {
                    synth.note_off(channel, key);
                }
            }
        }
    }

    /// Set the instrument (program) for a channel
    pub fn set_program(&self, channel: i32, program: i32) {
        let mut state = self.state.lock().unwrap();
        if let Some(ref mut synth) = state.synth {
            synth.process_midi_message(channel, 0xC0, program, 0);
        }
    }

    /// Set channel volume (CC 7)
    pub fn set_volume(&self, channel: i32, volume: i32) {
        let mut state = self.state.lock().unwrap();
        if let Some(ref mut synth) = state.synth {
            synth.process_midi_message(channel, 0xB0, 7, volume);
        }
    }

    /// Set channel pan (CC 10)
    pub fn set_pan(&self, channel: i32, pan: i32) {
        let mut state = self.state.lock().unwrap();
        if let Some(ref mut synth) = state.synth {
            synth.process_midi_message(channel, 0xB0, 10, pan);
        }
    }

    /// Set pitch bend (0-16383, center = 8192)
    pub fn set_pitch_bend(&self, channel: i32, value: i32) {
        let mut state = self.state.lock().unwrap();
        if let Some(ref mut synth) = state.synth {
            // Pitch bend is 0xE0, with LSB and MSB as the two data bytes
            let lsb = value & 0x7F;
            let msb = (value >> 7) & 0x7F;
            synth.process_midi_message(channel, 0xE0, lsb, msb);
        }
    }

    /// Set modulation wheel (CC 1)
    pub fn set_modulation(&self, channel: i32, value: i32) {
        let mut state = self.state.lock().unwrap();
        if let Some(ref mut synth) = state.synth {
            synth.process_midi_message(channel, 0xB0, 1, value.clamp(0, 127));
        }
    }

    /// Set expression (CC 11)
    pub fn set_expression(&self, channel: i32, value: i32) {
        let mut state = self.state.lock().unwrap();
        if let Some(ref mut synth) = state.synth {
            synth.process_midi_message(channel, 0xB0, 11, value.clamp(0, 127));
        }
    }


    /// Reset all controllers on a channel
    pub fn reset_controllers(&self, channel: i32) {
        let mut state = self.state.lock().unwrap();
        if let Some(ref mut synth) = state.synth {
            synth.reset_all_controllers_channel(channel);
        }
    }

    /// Get list of preset names from the loaded soundfont
    pub fn get_preset_names(&self) -> Vec<(u8, u8, String)> {
        let gm_names = [
            "Acoustic Grand Piano", "Bright Acoustic Piano", "Electric Grand Piano",
            "Honky-tonk Piano", "Electric Piano 1", "Electric Piano 2", "Harpsichord",
            "Clavinet", "Celesta", "Glockenspiel", "Music Box", "Vibraphone",
            "Marimba", "Xylophone", "Tubular Bells", "Dulcimer", "Drawbar Organ",
            "Percussive Organ", "Rock Organ", "Church Organ", "Reed Organ",
            "Accordion", "Harmonica", "Tango Accordion", "Acoustic Guitar (nylon)",
            "Acoustic Guitar (steel)", "Electric Guitar (jazz)", "Electric Guitar (clean)",
            "Electric Guitar (muted)", "Overdriven Guitar", "Distortion Guitar",
            "Guitar Harmonics", "Acoustic Bass", "Electric Bass (finger)",
            "Electric Bass (pick)", "Fretless Bass", "Slap Bass 1", "Slap Bass 2",
            "Synth Bass 1", "Synth Bass 2", "Violin", "Viola", "Cello", "Contrabass",
            "Tremolo Strings", "Pizzicato Strings", "Orchestral Harp", "Timpani",
            "String Ensemble 1", "String Ensemble 2", "Synth Strings 1", "Synth Strings 2",
            "Choir Aahs", "Voice Oohs", "Synth Voice", "Orchestra Hit", "Trumpet",
            "Trombone", "Tuba", "Muted Trumpet", "French Horn", "Brass Section",
            "Synth Brass 1", "Synth Brass 2", "Soprano Sax", "Alto Sax", "Tenor Sax",
            "Baritone Sax", "Oboe", "English Horn", "Bassoon", "Clarinet", "Piccolo",
            "Flute", "Recorder", "Pan Flute", "Blown Bottle", "Shakuhachi", "Whistle",
            "Ocarina", "Lead 1 (square)", "Lead 2 (sawtooth)", "Lead 3 (calliope)",
            "Lead 4 (chiff)", "Lead 5 (charang)", "Lead 6 (voice)", "Lead 7 (fifths)",
            "Lead 8 (bass + lead)", "Pad 1 (new age)", "Pad 2 (warm)", "Pad 3 (polysynth)",
            "Pad 4 (choir)", "Pad 5 (bowed)", "Pad 6 (metallic)", "Pad 7 (halo)",
            "Pad 8 (sweep)", "FX 1 (rain)", "FX 2 (soundtrack)", "FX 3 (crystal)",
            "FX 4 (atmosphere)", "FX 5 (brightness)", "FX 6 (goblins)", "FX 7 (echoes)",
            "FX 8 (sci-fi)", "Sitar", "Banjo", "Shamisen", "Koto", "Kalimba",
            "Bagpipe", "Fiddle", "Shanai", "Tinkle Bell", "Agogo", "Steel Drums",
            "Woodblock", "Taiko Drum", "Melodic Tom", "Synth Drum", "Reverse Cymbal",
            "Guitar Fret Noise", "Breath Noise", "Seashore", "Bird Tweet",
            "Telephone Ring", "Helicopter", "Applause", "Gunshot",
        ];

        gm_names.iter().enumerate()
            .map(|(i, name)| (0, i as u8, name.to_string()))
            .collect()
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}
