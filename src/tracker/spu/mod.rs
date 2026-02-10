//! PS1 SPU DSP Core
//!
//! Hardware-accurate PS1 Sound Processing Unit emulation.
//! Provides 24 voices with ADPCM decode, Gaussian interpolation,
//! ADSR envelopes, stereo volume, and reverb processing.
//!
//! The main entry point is `SpuCore::tick()` which produces one
//! stereo sample pair at 44100Hz per call.

pub mod adpcm;
pub mod reverb;
pub mod tables;
pub mod types;
pub mod voice;
pub mod convert;

use reverb::{SpuReverb, ReverbType};
use tables::MAX_VOICES;
use types::SampleLibrary;
use voice::Voice;

/// PS1 SPU Core — 24-voice mixer with reverb
pub struct SpuCore {
    /// 24 hardware voices
    voices: [Voice; MAX_VOICES],
    /// Hardware-accurate reverb processor
    reverb: SpuReverb,
    /// Loaded sample library (SF2 → ADPCM)
    sample_library: Option<SampleLibrary>,

    /// Master volume (0.0 to 2.0)
    master_volume: f32,
    /// Per-channel program (GM instrument number, 0-127)
    channel_programs: [u8; MAX_VOICES],
    /// Per-channel reverb enable flags
    channel_reverb: [bool; MAX_VOICES],
}

impl SpuCore {
    pub fn new() -> Self {
        Self {
            voices: std::array::from_fn(|_| Voice::new()),
            reverb: SpuReverb::new(),
            sample_library: None,
            master_volume: 1.0,
            channel_programs: [0u8; MAX_VOICES],
            channel_reverb: [false; MAX_VOICES],
        }
    }

    /// Load a sample library (created by convert::convert_sf2_to_spu)
    pub fn load_sample_library(&mut self, library: SampleLibrary) {
        self.all_notes_off();
        self.sample_library = Some(library);
    }

    /// Check if a sample library is loaded
    pub fn is_loaded(&self) -> bool {
        self.sample_library.is_some()
    }

    /// Get the source soundfont name
    pub fn source_name(&self) -> Option<&str> {
        self.sample_library.as_ref().map(|l| l.source_name.as_str())
    }

    // =========================================================================
    // Main tick — called 44100 times per second
    // =========================================================================

    /// Process one stereo sample pair at 44100Hz
    ///
    /// This is the heart of the SPU. Called from the audio callback.
    /// Returns (left, right) as f32 in the range -1.0 to 1.0.
    ///
    /// Pipeline:
    /// 1. Tick all 24 voices → per-voice (left, right) in i32
    /// 2. Sum voice outputs (dry mix) in i32, clamp to i16
    /// 3. Sum reverb-enabled voice outputs (reverb send)
    /// 4. Feed reverb send through SpuReverb
    /// 5. Mix dry + wet reverb output
    /// 6. Apply master volume
    /// 7. Convert i16 → f32
    pub fn tick(&mut self) -> (f32, f32) {
        let spu_ram = match &self.sample_library {
            Some(lib) => &lib.spu_ram,
            None => return (0.0, 0.0),
        };

        let mut dry_left: i32 = 0;
        let mut dry_right: i32 = 0;
        let mut reverb_in_left: i32 = 0;
        let mut reverb_in_right: i32 = 0;

        // Process all voices
        for i in 0..MAX_VOICES {
            let (left, right) = self.voices[i].tick(spu_ram);

            dry_left += left;
            dry_right += right;

            if self.channel_reverb[i] {
                reverb_in_left += left;
                reverb_in_right += right;
            }
        }

        // Clamp dry mix to i16
        let dry_left = clamp16(dry_left);
        let dry_right = clamp16(dry_right);

        // Process reverb
        let reverb_in_left = clamp16(reverb_in_left);
        let reverb_in_right = clamp16(reverb_in_right);
        let (reverb_left, reverb_right) = self.reverb.process_sample(reverb_in_left, reverb_in_right);

        // Mix dry + reverb
        let wet = self.reverb.wet_level();
        let dry_mix = 1.0 - wet;

        let left = dry_left as f32 * dry_mix + reverb_left as f32 * wet;
        let right = dry_right as f32 * dry_mix + reverb_right as f32 * wet;

        // Apply master volume and convert to f32 (-1.0 to 1.0)
        let scale = self.master_volume / 32768.0;
        (left * scale, right * scale)
    }

    // =========================================================================
    // Note control — called from tracker
    // =========================================================================

    /// Trigger a note on a voice
    ///
    /// Looks up the appropriate sample region for the given program + note,
    /// and starts playback on the specified voice.
    pub fn note_on(&mut self, voice_idx: usize, program: u8, note: u8, velocity: u8) {
        if voice_idx >= MAX_VOICES {
            return;
        }

        let library = match &self.sample_library {
            Some(lib) => lib,
            None => return,
        };

        // Find the instrument bank for this program
        let bank = match library.instrument(program) {
            Some(b) => b,
            None => return,
        };

        // Find the region that covers this note
        let region = match bank.region_for_note(note) {
            Some(r) => r,
            None => return,
        };

        // Store program for this channel
        self.channel_programs[voice_idx] = program;

        // Trigger the voice
        self.voices[voice_idx].key_on(region, note, velocity);
        self.voices[voice_idx].reverb_enabled = self.channel_reverb[voice_idx];
    }

    /// Release a note on a voice
    pub fn note_off(&mut self, voice_idx: usize) {
        if voice_idx >= MAX_VOICES {
            return;
        }
        self.voices[voice_idx].key_off();
    }

    /// Stop all voices immediately
    pub fn all_notes_off(&mut self) {
        for voice in &mut self.voices {
            voice.active = false;
        }
    }

    // =========================================================================
    // Per-voice parameter control
    // =========================================================================

    /// Set the GM program (instrument) for a channel
    pub fn set_program(&mut self, voice_idx: usize, program: u8) {
        if voice_idx < MAX_VOICES {
            self.channel_programs[voice_idx] = program;
        }
    }

    /// Get current program for a channel
    pub fn program(&self, voice_idx: usize) -> u8 {
        if voice_idx < MAX_VOICES {
            self.channel_programs[voice_idx]
        } else {
            0
        }
    }

    /// Set voice pan (0=left, 64=center, 127=right) and volume (0-127)
    pub fn set_voice_pan(&mut self, voice_idx: usize, pan: u8, volume: u8) {
        if voice_idx < MAX_VOICES {
            self.voices[voice_idx].set_volume_from_pan(pan, volume);
        }
    }

    /// Set voice pitch directly (for pitch bend effects)
    pub fn set_voice_pitch(&mut self, voice_idx: usize, pitch: u16) {
        if voice_idx < MAX_VOICES {
            self.voices[voice_idx].set_pitch(pitch);
        }
    }

    /// Enable/disable reverb send for a voice
    pub fn set_voice_reverb(&mut self, voice_idx: usize, enabled: bool) {
        if voice_idx < MAX_VOICES {
            self.channel_reverb[voice_idx] = enabled;
            self.voices[voice_idx].reverb_enabled = enabled;
        }
    }

    /// Check if a voice is active
    pub fn is_voice_active(&self, voice_idx: usize) -> bool {
        voice_idx < MAX_VOICES && self.voices[voice_idx].active
    }

    /// Get the current pitch register value of a voice
    pub fn voice_pitch(&self, voice_idx: usize) -> u16 {
        if voice_idx < MAX_VOICES {
            self.voices[voice_idx].current_pitch()
        } else {
            0
        }
    }

    // =========================================================================
    // Global controls
    // =========================================================================

    /// Set reverb preset
    pub fn set_reverb_preset(&mut self, reverb_type: ReverbType) {
        self.reverb.set_preset(reverb_type);
    }

    /// Get current reverb type
    pub fn reverb_type(&self) -> ReverbType {
        self.reverb.reverb_type()
    }

    /// Set reverb wet level (0.0 = dry, 1.0 = wet)
    pub fn set_reverb_wet_level(&mut self, level: f32) {
        self.reverb.set_wet_level(level);
    }

    /// Get reverb wet level
    pub fn reverb_wet_level(&self) -> f32 {
        self.reverb.wet_level()
    }

    /// Clear reverb buffers (call when stopping playback)
    pub fn clear_reverb(&mut self) {
        self.reverb.clear();
    }

    /// Set master volume (0.0 to 2.0)
    pub fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = volume.clamp(0.0, 2.0);
    }

    /// Get master volume
    pub fn master_volume(&self) -> f32 {
        self.master_volume
    }

    /// Get instrument names from the loaded sample library
    pub fn get_instrument_names(&self) -> Vec<(u8, String)> {
        match &self.sample_library {
            Some(lib) => lib.instrument_names(),
            None => Vec::new(),
        }
    }
}

impl Default for SpuCore {
    fn default() -> Self {
        Self::new()
    }
}

/// Clamp i32 to i16 range
#[inline]
fn clamp16(value: i32) -> i16 {
    value.clamp(-32768, 32767) as i16
}
