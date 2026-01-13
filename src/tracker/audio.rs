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

/// Output gain multiplier - boosts overall volume since soundfont is quiet
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
        // Clamp pitch to valid range (avoid division by zero, cap at native)
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

/// PS1 SPU Gaussian interpolation table (512 entries)
/// The hardware uses bits 4-11 of the pitch counter as an index
/// Each value represents a multiplier as N/0x8000
/// Source: psx-spx documentation / nocash specs
static GAUSSIAN_TABLE: [i16; 512] = [
    -0x001, -0x001, -0x001, -0x001, -0x001, -0x001, -0x001, -0x001,
    -0x001, -0x001, -0x001, -0x001, -0x001, -0x001, -0x001, -0x001,
    0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0001,
    0x0001, 0x0001, 0x0001, 0x0002, 0x0002, 0x0002, 0x0003, 0x0003,
    0x0003, 0x0004, 0x0004, 0x0005, 0x0005, 0x0006, 0x0007, 0x0007,
    0x0008, 0x0009, 0x0009, 0x000A, 0x000B, 0x000C, 0x000D, 0x000E,
    0x000F, 0x0010, 0x0011, 0x0012, 0x0013, 0x0015, 0x0016, 0x0018,
    0x0019, 0x001B, 0x001C, 0x001E, 0x0020, 0x0021, 0x0023, 0x0025,
    0x0027, 0x0029, 0x002C, 0x002E, 0x0030, 0x0033, 0x0035, 0x0038,
    0x003A, 0x003D, 0x0040, 0x0043, 0x0046, 0x0049, 0x004D, 0x0050,
    0x0054, 0x0057, 0x005B, 0x005F, 0x0063, 0x0067, 0x006B, 0x006F,
    0x0074, 0x0078, 0x007D, 0x0082, 0x0087, 0x008C, 0x0091, 0x0096,
    0x009C, 0x00A1, 0x00A7, 0x00AD, 0x00B3, 0x00BA, 0x00C0, 0x00C7,
    0x00CD, 0x00D4, 0x00DB, 0x00E3, 0x00EA, 0x00F2, 0x00FA, 0x0101,
    0x010A, 0x0112, 0x011B, 0x0123, 0x012C, 0x0135, 0x013F, 0x0148,
    0x0152, 0x015C, 0x0166, 0x0171, 0x017B, 0x0186, 0x0191, 0x019C,
    0x01A8, 0x01B4, 0x01C0, 0x01CC, 0x01D9, 0x01E5, 0x01F2, 0x0200,
    0x020D, 0x021B, 0x0229, 0x0237, 0x0246, 0x0255, 0x0264, 0x0273,
    0x0283, 0x0293, 0x02A3, 0x02B4, 0x02C4, 0x02D6, 0x02E7, 0x02F9,
    0x030B, 0x031D, 0x0330, 0x0343, 0x0356, 0x036A, 0x037E, 0x0392,
    0x03A7, 0x03BC, 0x03D1, 0x03E7, 0x03FC, 0x0413, 0x042A, 0x0441,
    0x0458, 0x0470, 0x0488, 0x04A0, 0x04B9, 0x04D2, 0x04EC, 0x0506,
    0x0520, 0x053B, 0x0556, 0x0572, 0x058E, 0x05AA, 0x05C7, 0x05E4,
    0x0601, 0x061F, 0x063E, 0x065C, 0x067C, 0x069B, 0x06BB, 0x06DC,
    0x06FD, 0x071E, 0x0740, 0x0762, 0x0784, 0x07A7, 0x07CB, 0x07EF,
    0x0813, 0x0838, 0x085D, 0x0883, 0x08A9, 0x08D0, 0x08F7, 0x091E,
    0x0946, 0x096F, 0x0998, 0x09C1, 0x09EB, 0x0A16, 0x0A40, 0x0A6C,
    0x0A98, 0x0AC4, 0x0AF1, 0x0B1E, 0x0B4C, 0x0B7A, 0x0BA9, 0x0BD8,
    0x0C07, 0x0C38, 0x0C68, 0x0C99, 0x0CCB, 0x0CFD, 0x0D30, 0x0D63,
    0x0D97, 0x0DCB, 0x0E00, 0x0E35, 0x0E6B, 0x0EA1, 0x0ED7, 0x0F0F,
    0x0F46, 0x0F7F, 0x0FB7, 0x0FF1, 0x102A, 0x1065, 0x109F, 0x10DB,
    0x1116, 0x1153, 0x118F, 0x11CD, 0x120B, 0x1249, 0x1288, 0x12C7,
    0x1307, 0x1347, 0x1388, 0x13C9, 0x140B, 0x144D, 0x1490, 0x14D4,
    0x1517, 0x155C, 0x15A0, 0x15E6, 0x162C, 0x1672, 0x16B9, 0x1700,
    0x1747, 0x1790, 0x17D8, 0x1821, 0x186B, 0x18B5, 0x1900, 0x194B,
    0x1996, 0x19E2, 0x1A2E, 0x1A7B, 0x1AC8, 0x1B16, 0x1B64, 0x1BB3,
    0x1C02, 0x1C51, 0x1CA1, 0x1CF1, 0x1D42, 0x1D93, 0x1DE5, 0x1E37,
    0x1E89, 0x1EDC, 0x1F2F, 0x1F82, 0x1FD6, 0x202A, 0x207F, 0x20D4,
    0x2129, 0x217F, 0x21D5, 0x222C, 0x2282, 0x22DA, 0x2331, 0x2389,
    0x23E1, 0x2439, 0x2492, 0x24EB, 0x2545, 0x259E, 0x25F8, 0x2653,
    0x26AD, 0x2708, 0x2763, 0x27BE, 0x281A, 0x2876, 0x28D2, 0x292E,
    0x298B, 0x29E7, 0x2A44, 0x2AA1, 0x2AFF, 0x2B5C, 0x2BBA, 0x2C18,
    0x2C76, 0x2CD4, 0x2D33, 0x2D91, 0x2DF0, 0x2E4F, 0x2EAE, 0x2F0D,
    0x2F6C, 0x2FCC, 0x302B, 0x308B, 0x30EA, 0x314A, 0x31AA, 0x3209,
    0x3269, 0x32C9, 0x3329, 0x3389, 0x33E9, 0x3449, 0x34A9, 0x3509,
    0x3569, 0x35C9, 0x3629, 0x3689, 0x36E8, 0x3748, 0x37A8, 0x3807,
    0x3867, 0x38C6, 0x3926, 0x3985, 0x39E4, 0x3A43, 0x3AA2, 0x3B00,
    0x3B5F, 0x3BBD, 0x3C1B, 0x3C79, 0x3CD7, 0x3D35, 0x3D92, 0x3DEF,
    0x3E4C, 0x3EA9, 0x3F05, 0x3F62, 0x3FBD, 0x4019, 0x4074, 0x40D0,
    0x412A, 0x4185, 0x41DF, 0x4239, 0x4292, 0x42EB, 0x4344, 0x439C,
    0x43F4, 0x444C, 0x44A3, 0x44FA, 0x4550, 0x45A6, 0x45FC, 0x4651,
    0x46A6, 0x46FA, 0x474E, 0x47A1, 0x47F4, 0x4846, 0x4898, 0x48E9,
    0x493A, 0x498A, 0x49D9, 0x4A29, 0x4A77, 0x4AC5, 0x4B13, 0x4B5F,
    0x4BAC, 0x4BF7, 0x4C42, 0x4C8D, 0x4CD7, 0x4D20, 0x4D68, 0x4DB0,
    0x4DF7, 0x4E3E, 0x4E84, 0x4EC9, 0x4F0E, 0x4F52, 0x4F95, 0x4FD7,
    0x5019, 0x505A, 0x509A, 0x50DA, 0x5118, 0x5156, 0x5194, 0x51D0,
    0x520C, 0x5247, 0x5281, 0x52BA, 0x52F3, 0x532A, 0x5361, 0x5397,
    0x53CC, 0x5401, 0x5434, 0x5467, 0x5499, 0x54CA, 0x54FA, 0x5529,
    0x5558, 0x5585, 0x55B2, 0x55DE, 0x5609, 0x5632, 0x565B, 0x5684,
    0x56AB, 0x56D1, 0x56F6, 0x571B, 0x573E, 0x5761, 0x5782, 0x57A3,
    0x57C3, 0x57E2, 0x57FF, 0x581C, 0x5838, 0x5853, 0x586D, 0x5886,
    0x589E, 0x58B5, 0x58CB, 0x58E0, 0x58F4, 0x5907, 0x5919, 0x592A,
    0x593A, 0x5949, 0x5958, 0x5965, 0x5971, 0x597C, 0x5986, 0x598F,
    0x5997, 0x599E, 0x59A4, 0x59A9, 0x59AD, 0x59B0, 0x59B2, 0x59B3,
];

/// PS1 SPU Gaussian Resampler
///
/// Implements authentic PS1 SPU sample rate conversion by:
/// 1. Downsampling 44.1kHz audio to the target rate (averaging)
/// 2. Upsampling back to 44.1kHz using the real PS1 Gaussian interpolation
///
/// This creates the characteristic "warm/muffled" sound of lower sample rates
/// on the PS1, as opposed to harsh aliasing or simple filtering.
pub struct SpuResampler {
    /// Sample history for Gaussian interpolation (need 4 samples)
    history_l: [f32; 4],
    history_r: [f32; 4],
    /// Pitch counter (fractional position between samples)
    /// Format: bits 12+ = integer part, bits 4-11 = Gaussian index, bits 0-3 = sub-index
    pitch_counter: u32,
    /// Current pitch value (0x1000 = 44.1kHz, 0x0800 = 22kHz, etc.)
    pitch: u16,
    /// Accumulator for downsampling (left)
    accum_l: f32,
    /// Accumulator for downsampling (right)
    accum_r: f32,
    /// Sample count for averaging during downsample
    accum_count: u32,
    /// Whether SPU emulation is enabled
    enabled: bool,
}

impl Default for SpuResampler {
    fn default() -> Self {
        Self::new()
    }
}

impl SpuResampler {
    pub fn new() -> Self {
        Self {
            history_l: [0.0; 4],
            history_r: [0.0; 4],
            pitch_counter: 0,
            pitch: 0x1000, // Native rate
            accum_l: 0.0,
            accum_r: 0.0,
            accum_count: 0,
            enabled: true,
        }
    }

    /// Set the target sample rate via pitch value
    /// 0x1000 = 44.1kHz (native), 0x0800 = 22kHz, 0x0400 = 11kHz, 0x0200 = 5.5kHz
    pub fn set_pitch(&mut self, pitch: SpuPitch) {
        if self.pitch != pitch.0 {
            self.pitch = pitch.0;
            // Reset state when pitch changes to avoid artifacts
            self.reset_state();
        }
    }

    /// Reset all internal state (call when audio restarts or settings change)
    pub fn reset_state(&mut self) {
        self.history_l = [0.0; 4];
        self.history_r = [0.0; 4];
        self.pitch_counter = 0;
        self.accum_l = 0.0;
        self.accum_r = 0.0;
        self.accum_count = 0;
    }

    /// Enable or disable SPU emulation
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        // Always reset state when toggling to avoid artifacts
        self.reset_state();
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Perform Gaussian interpolation on 4 samples
    /// gauss_idx is bits 4-11 of the pitch counter (0-255)
    #[inline]
    fn gaussian_interpolate(samples: &[f32; 4], gauss_idx: usize) -> f32 {
        // Get the 4 Gaussian coefficients from the table
        // Table layout matches PS1 hardware:
        // gauss[0xFF-i], gauss[0x1FF-i], gauss[0x100+i], gauss[i]
        let g0 = GAUSSIAN_TABLE[0xFF - gauss_idx] as f32;
        let g1 = GAUSSIAN_TABLE[0x1FF - gauss_idx] as f32;
        let g2 = GAUSSIAN_TABLE[0x100 + gauss_idx] as f32;
        let g3 = GAUSSIAN_TABLE[gauss_idx] as f32;

        // Apply weighted sum (coefficients are in Q15 format, so divide by 32768)
        let result = (g0 * samples[0] + g1 * samples[1] + g2 * samples[2] + g3 * samples[3])
            / 32768.0;

        result
    }

    /// Push a new sample into the history buffer (shifts old samples out)
    #[inline]
    fn push_sample(history: &mut [f32; 4], sample: f32) {
        history[0] = history[1];
        history[1] = history[2];
        history[2] = history[3];
        history[3] = sample;
    }

    /// Process stereo audio through the SPU resampler
    ///
    /// This implements authentic PS1 sample rate conversion:
    /// 1. Accumulate input samples for downsampling
    /// 2. When enough samples accumulated, push averaged sample to history
    /// 3. For each output sample, perform Gaussian interpolation
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        if !self.enabled || self.pitch >= 0x1000 {
            // Bypass - no processing needed at native rate or when disabled
            return;
        }

        let len = left.len().min(right.len());
        if len == 0 {
            return;
        }

        // Calculate how many input samples per output sample
        // pitch 0x1000 = 1:1, 0x0800 = 2:1, 0x0400 = 4:1, etc.
        let downsample_ratio = 0x1000u32 / self.pitch.max(1) as u32;

        for i in 0..len {
            let sample_l = left[i];
            let sample_r = right[i];

            // Accumulate input samples (with denormal protection)
            self.accum_l += sample_l;
            self.accum_r += sample_r;
            self.accum_count += 1;

            // When we've accumulated enough samples, push to history
            if self.accum_count >= downsample_ratio {
                let count = self.accum_count as f32;
                let avg_l = self.accum_l / count;
                let avg_r = self.accum_r / count;

                // Clamp to valid range to prevent drift
                let avg_l = avg_l.clamp(-1.5, 1.5);
                let avg_r = avg_r.clamp(-1.5, 1.5);

                Self::push_sample(&mut self.history_l, avg_l);
                Self::push_sample(&mut self.history_r, avg_r);

                self.accum_l = 0.0;
                self.accum_r = 0.0;
                self.accum_count = 0;
            }

            // Advance pitch counter by the pitch value
            self.pitch_counter = self.pitch_counter.wrapping_add(self.pitch as u32);

            // Extract Gaussian interpolation index (bits 4-11)
            let gauss_idx = ((self.pitch_counter >> 4) & 0xFF) as usize;

            // Apply Gaussian interpolation
            let out_l = Self::gaussian_interpolate(&self.history_l, gauss_idx);
            let out_r = Self::gaussian_interpolate(&self.history_r, gauss_idx);

            // Clamp output to prevent any runaway values
            left[i] = out_l.clamp(-1.5, 1.5);
            right[i] = out_r.clamp(-1.5, 1.5);

            // Wrap pitch counter (keep only fractional part relevant to interpolation)
            // Reset when we've advanced past 0x1000 (one full sample)
            if self.pitch_counter >= 0x1000 {
                self.pitch_counter &= 0xFFF; // Keep only lower 12 bits
            }
        }

        // Kill denormals in accumulators to prevent CPU spikes and drift
        if self.accum_l.abs() < 1e-20 {
            self.accum_l = 0.0;
        }
        if self.accum_r.abs() < 1e-20 {
            self.accum_r = 0.0;
        }
    }
}

/// Legacy function for backward compatibility
/// Now delegates to a simple low-pass filter as fallback
fn apply_ps1_degradation(samples: &mut [f32], pitch: SpuPitch) {
    if pitch.0 >= 0x1000 {
        return;
    }

    let len = samples.len();
    if len < 2 {
        return;
    }

    // Simple low-pass filter fallback
    let window = (0x1000 / pitch.0.max(1)) as usize;
    if window <= 1 {
        return;
    }

    let mut prev = samples[0];
    let alpha = 1.0 / window as f32;
    let one_minus_alpha = 1.0 - alpha;

    for sample in samples.iter_mut() {
        *sample = alpha * *sample + one_minus_alpha * prev;
        prev = *sample;
    }
}

/// Audio engine state shared between main thread and audio callback
struct AudioState {
    /// The synthesizer
    synth: Option<Synthesizer>,
    /// Whether audio is playing
    playing: bool,
    /// PS1 SPU reverb processor
    reverb: PsxReverb,
    /// Output sample rate mode
    output_sample_rate: OutputSampleRate,
    /// PS1 SPU Gaussian resampler
    resampler: SpuResampler,
    /// Master volume (0.0 to 2.0, default 1.0)
    master_volume: f32,
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

                    // Apply PS1 SPU Gaussian resampling (authentic sample rate conversion)
                    state.resampler.process(&mut left_buffer[..samples_needed], &mut right_buffer[..samples_needed]);

                    // Apply master volume and output gain
                    let gain = state.master_volume * OUTPUT_GAIN;
                    for i in 0..samples_needed {
                        data[i * 2] = left_buffer[i] * gain;
                        data[i * 2 + 1] = right_buffer[i] * gain;
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
            output_sample_rate: OutputSampleRate::default(),
            resampler: SpuResampler::new(),
            master_volume: 1.0,
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

    /// Set output sample rate mode
    pub fn set_output_sample_rate(&self, rate: OutputSampleRate) {
        let mut state = self.state.lock().unwrap();
        state.output_sample_rate = rate;
        state.resampler.set_pitch(rate);
    }

    /// Get current output sample rate mode
    pub fn output_sample_rate(&self) -> OutputSampleRate {
        self.state.lock().unwrap().output_sample_rate
    }

    /// Set master volume (0.0 to 2.0)
    pub fn set_master_volume(&self, volume: f32) {
        let mut state = self.state.lock().unwrap();
        state.master_volume = volume.clamp(0.0, 2.0);
    }

    /// Get master volume
    pub fn master_volume(&self) -> f32 {
        self.state.lock().unwrap().master_volume
    }

    /// Enable or disable SPU resampling emulation
    pub fn set_spu_resampling_enabled(&self, enabled: bool) {
        let mut state = self.state.lock().unwrap();
        state.resampler.set_enabled(enabled);
    }

    /// Check if SPU resampling is enabled
    pub fn is_spu_resampling_enabled(&self) -> bool {
        self.state.lock().unwrap().resampler.is_enabled()
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

            // Apply PS1 SPU Gaussian resampling (authentic sample rate conversion)
            state.resampler.process(&mut self.left_buffer[..samples], &mut self.right_buffer[..samples]);

            // Apply master volume and output gain
            let gain = state.master_volume * OUTPUT_GAIN;
            for i in 0..samples {
                self.left_buffer[i] *= gain;
                self.right_buffer[i] *= gain;
            }

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
    /// Returns (bank, program, name) tuples for all 128 GM melodic instruments
    /// Note: Drums require MIDI channel 10 and bank select - not yet supported
    pub fn get_preset_names(&self) -> Vec<(u8, u8, String)> {
        // Standard GM melodic instruments (bank 0, programs 0-127)
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
