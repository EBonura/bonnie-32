//! PS1 SPU per-voice processing
//!
//! Each voice handles the complete DSP pipeline:
//! 1. ADPCM decode — decompress 4-bit samples from SPU RAM
//! 2. Gaussian interpolation — 4-point interpolation at pitch counter rate
//! 3. ADSR envelope — hardware-accurate attack/decay/sustain/release
//! 4. Volume — per-voice stereo volume with PS1 fixed-point math
//!
//! Implementation matches Duckstation spu.cpp for hardware accuracy.
//! Reference: Duckstation spu.cpp, psx-spx SPU documentation

use super::adpcm;
use super::tables::{
    GAUSSIAN_TABLE, MAX_PITCH, SAMPLES_PER_ADPCM_BLOCK, ADPCM_BLOCK_SIZE,
};
use super::types::{AdsrParams, AdsrPhase, SpuRam, SampleRegion};

// =============================================================================
// Volume Envelope — matches Duckstation VolumeEnvelope
// =============================================================================

/// PS1 hardware volume envelope (used for ADSR)
///
/// Pre-computes step and counter_increment from rate, then ticks each sample.
/// Matches Duckstation's VolumeEnvelope::Reset() and Tick() exactly.
struct VolumeEnvelope {
    /// Pre-computed step value (signed, applied when counter fires)
    step: i32,
    /// Counter increment per tick (controls timing)
    counter_increment: u16,
    /// 16-bit counter, fires when bit 15 set
    counter: u16,
    /// Rate value (0-127) stored for exponential increase threshold checks
    rate: u8,
    /// Direction flag
    decreasing: bool,
    /// Exponential mode flag
    exponential: bool,
}

impl VolumeEnvelope {
    fn new() -> Self {
        Self {
            step: 0,
            counter_increment: 0,
            counter: 0,
            rate: 0,
            decreasing: false,
            exponential: false,
        }
    }

    /// Initialize envelope for a phase transition
    ///
    /// Matches Duckstation VolumeEnvelope::Reset():
    /// - base_step = 7 - (rate & 3)
    /// - Decreasing uses ~base_step (bitwise NOT: +7→-8, +6→-7, +5→-6, +4→-5)
    /// - rate < 44: step <<= (11 - (rate >> 2))
    /// - rate >= 48: counter_increment >>= ((rate >> 2) - 11)
    /// - rate_mask handles "max rate = never tick" edge case
    fn reset(&mut self, rate: u8, rate_mask: u8, decreasing: bool, exponential: bool) {
        self.rate = rate;
        self.decreasing = decreasing;
        self.exponential = exponential;
        self.counter = 0;
        self.counter_increment = 0x8000;

        let base_step = 7_i32 - (rate & 3) as i32;
        // Duckstation: step = decreasing ? ~base_step : base_step
        // ~base_step for values 4-7 gives -5,-6,-7,-8 (one larger magnitude than negation)
        self.step = if decreasing { !base_step } else { base_step };

        let shift = (rate >> 2) as i32;
        if rate < 44 {
            self.step <<= 11 - shift;
        } else if rate >= 48 {
            let shift_amount = (shift - 11) as u32;
            // u16 can only be shifted by 0-15; larger shifts produce 0 (envelope never ticks)
            if shift_amount >= 16 {
                self.counter_increment = 0;
            } else {
                self.counter_increment >>= shift_amount;
                // Rate with all bits set in mask = never tick (counter_increment stays 0)
                if (rate & rate_mask) != rate_mask {
                    self.counter_increment = self.counter_increment.max(1);
                }
            }
        }
        // rates 44-47: step and counter_increment stay at base values
    }

    /// Tick the envelope, potentially modifying current_level
    ///
    /// Matches Duckstation VolumeEnvelope::Tick() exactly:
    /// - Exponential decrease: step scaled by current level
    /// - Exponential increase above 0x6000: rate-dependent slowdown
    /// - Counter fires when bit 15 set
    fn tick(&mut self, current_level: &mut i16) {
        if self.counter_increment == 0 {
            return;
        }

        let mut this_step = self.step;
        let mut this_increment = self.counter_increment as u32;

        if self.exponential {
            if self.decreasing {
                // Exponential decrease: scale step by current level
                // step is negative, level is positive → result is negative
                this_step = (this_step * *current_level as i32) >> 15;
            } else {
                // Exponential increase: slow down when level >= 0x6000
                if *current_level >= 0x6000 {
                    if self.rate < 40 {
                        this_step >>= 2;
                    } else if self.rate >= 44 {
                        this_increment >>= 2;
                    } else {
                        // rate 40-43: split the slowdown
                        this_step >>= 1;
                        this_increment >>= 1;
                    }
                }
            }
        }

        self.counter = self.counter.wrapping_add(this_increment as u16);

        // Check for counter overflow (bit 15 set)
        if self.counter & 0x8000 == 0 {
            return; // Not time to apply step yet
        }
        self.counter = 0;

        // Apply step to level
        let new_level = *current_level as i32 + this_step;
        if !self.decreasing {
            *current_level = new_level.clamp(-32768, 32767) as i16;
        } else {
            *current_level = new_level.max(0) as i16;
        }
    }
}

// =============================================================================
// Voice — per-voice SPU processing
// =============================================================================

/// Per-voice state for the SPU
pub struct Voice {
    // --- Active state ---
    pub active: bool,

    // --- Sample playback ---
    /// Current ADPCM block address in SPU RAM (byte offset)
    current_address: u32,
    /// Loop return address (set when LOOP_START flag encountered)
    loop_address: u32,
    /// Whether a loop address has been set
    has_loop: bool,
    /// ADPCM decode state: previous two samples for filter prediction
    adpcm_prev1: i16,
    adpcm_prev2: i16,
    /// Current decoded block (28 samples)
    decoded_samples: [i16; SAMPLES_PER_ADPCM_BLOCK],
    /// Position within decoded block (0-27)
    sample_index: usize,
    /// Whether we have decoded samples available
    has_samples: bool,

    // --- Pitch / Gaussian interpolation ---
    /// Pitch register (0x0000-0x3FFF), 0x1000 = 44100Hz
    pitch: u16,
    /// Fractional position counter for interpolation
    /// Bits 4-11 index into Gaussian table (0-255)
    /// Bits 12+ = integer part (sample index within block)
    pitch_counter: u32,
    /// Last 4 samples for Gaussian interpolation [0]=oldest, [3]=newest
    /// This sliding window matches Duckstation's current_block_samples[s-3..s]
    gauss_history: [i16; 4],

    // --- ADSR envelope ---
    adsr_params: AdsrParams,
    adsr_phase: AdsrPhase,
    /// Current envelope level (0 to 0x7FFF)
    adsr_level: i16,
    /// ADSR target level for current phase
    adsr_target: i16,
    /// Hardware envelope state
    adsr_envelope: VolumeEnvelope,

    // --- Volume ---
    /// Base volume from instrument region (0 to 0x7FFF, PS1 voice volume range)
    base_volume: i16,
    /// Left volume (0 to 0x7FFF), applied as >> 15
    volume_left: i16,
    /// Right volume (0 to 0x7FFF), applied as >> 15
    volume_right: i16,

    // --- Flags ---
    /// Whether this voice sends to the reverb unit
    pub reverb_enabled: bool,
    /// ENDX flag — set when sample reaches loop end
    pub end_flag: bool,

    // --- Tracker state ---
    /// GM program number for this voice
    pub instrument: u8,
    /// MIDI note currently playing
    pub note: u8,
    /// Velocity (used for volume scaling)
    pub velocity: u8,
}

impl Voice {
    pub fn new() -> Self {
        Self {
            active: false,
            current_address: 0,
            loop_address: 0,
            has_loop: false,
            adpcm_prev1: 0,
            adpcm_prev2: 0,
            decoded_samples: [0i16; SAMPLES_PER_ADPCM_BLOCK],
            sample_index: 0,
            has_samples: false,
            pitch: 0x1000,
            pitch_counter: 0,
            gauss_history: [0i16; 4],
            adsr_params: AdsrParams::default(),
            adsr_phase: AdsrPhase::Off,
            adsr_level: 0,
            adsr_target: 0,
            adsr_envelope: VolumeEnvelope::new(),
            base_volume: 0x3FFF,
            volume_left: 0x3FFF,
            volume_right: 0x3FFF,
            reverb_enabled: false,
            end_flag: false,
            instrument: 0,
            note: 0,
            velocity: 127,
        }
    }

    /// Trigger a note (key on)
    ///
    /// Matches Duckstation Voice::KeyOn(): reset counter, ADPCM state, start attack.
    pub fn key_on(&mut self, region: &SampleRegion, note: u8, velocity: u8) {
        self.active = true;
        self.note = note;
        self.velocity = velocity;

        // Set sample addresses
        self.current_address = region.spu_ram_offset;
        self.loop_address = region.loop_offset;
        self.has_loop = region.has_loop;

        // Reset ADPCM decode state (matches Duckstation: adpcm_last_samples.fill(0))
        self.adpcm_prev1 = 0;
        self.adpcm_prev2 = 0;
        self.has_samples = false;
        self.sample_index = 0;

        // Calculate pitch for this note
        self.pitch = region.pitch_for_note(note).min(MAX_PITCH);

        // Reset interpolation state (counter = 0, history = carryover = 0)
        self.pitch_counter = 0;
        self.gauss_history = [0i16; 4];

        // Set ADSR and start attack phase
        self.adsr_params = region.adsr;
        self.adsr_level = 0;
        self.adsr_phase = AdsrPhase::Attack;
        self.update_adsr_envelope();

        // Dump ADSR parameters for debugging
        #[cfg(not(target_arch = "wasm32"))]
        {
            let p = &self.adsr_params;
            let atk_rate = self.compute_attack_rate();
            let dec_rate = self.compute_decay_rate();
            let sus_rate = self.compute_sustain_rate();
            let rel_rate = self.compute_release_rate();
            eprintln!(
                "  ADSR: atk={}{} dec={} sus_lvl={} sus={}{}{} rel={}{}",
                atk_rate, if p.attack_exp { "e" } else { "l" },
                dec_rate,
                p.sustain_level,
                sus_rate, if p.sustain_exp { "e" } else { "l" },
                if p.sustain_decrease { "-" } else { "+" },
                rel_rate, if p.release_exp { "e" } else { "l" },
            );
            eprintln!(
                "        sustain_target=0x{:04X} pitch=0x{:04X} base_vol={}",
                p.sustain_level_i16(), self.pitch, region.default_volume,
            );
        }

        // Store base volume from instrument region for set_volume_from_pan
        // PS1 volume registers range 0-0x7FFF (unity gain with >> 15)
        self.base_volume = region.default_volume.min(0x7FFF);

        // Set initial volume (will be overridden by sync_voice_volume if called)
        // Velocity uses squared curve to match MIDI/GM/SF2 conventions:
        // gain = (vel/127)^2. This matches rustysynth's note_gain calculation
        // and produces natural velocity dynamics for SF2-sourced instruments.
        let vel_sq = (velocity as i32 * velocity as i32) / 127;
        let vol = (self.base_volume as i32 * vel_sq) / 127;
        let vol = vol.clamp(0, 0x7FFF) as i16;
        self.volume_left = vol;
        self.volume_right = vol;

        self.end_flag = false;
    }

    /// Release a note (key off → enter release phase)
    ///
    /// Matches Duckstation Voice::KeyOff()
    pub fn key_off(&mut self) {
        if self.adsr_phase == AdsrPhase::Off || self.adsr_phase == AdsrPhase::Release {
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        eprintln!("SPU voice key_off: note={} phase={:?} level={}", self.note, self.adsr_phase, self.adsr_level);
        self.adsr_phase = AdsrPhase::Release;
        self.update_adsr_envelope();
    }

    /// Set stereo volume from pan position (0=left, 64=center, 127=right)
    /// and MIDI volume (0-127, from CC7*CC11)
    ///
    /// Combines base_volume (0-0x7FFF, from instrument region) with MIDI
    /// volume and velocity to produce SPU-range L/R volumes for the >> 15
    /// shift in tick().
    pub fn set_volume_from_pan(&mut self, pan: u8, volume: u8) {
        // MIDI/GM/SF2 standard: both volume and velocity use squared curves.
        // Matches rustysynth: channel_gain = (vol * expr)^2, note_gain includes (vel/127)^2.
        let vel_sq = (self.velocity as i32 * self.velocity as i32) / 127; // max 127
        let vol_sq = (volume as i32 * volume as i32) / 127; // max 127
        let combined = (self.base_volume as i32 * vol_sq * vel_sq) / (127 * 127);
        let combined = combined.clamp(0, 0x7FFF) as f32;

        // Equal-power panning
        let pan_f = pan as f32 / 127.0;
        let angle = pan_f * std::f32::consts::FRAC_PI_2;
        self.volume_left = (combined * angle.cos()) as i16;
        self.volume_right = (combined * angle.sin()) as i16;
    }

    /// Set pitch directly (for pitch bend, portamento, etc.)
    pub fn set_pitch(&mut self, pitch: u16) {
        self.pitch = pitch.min(MAX_PITCH);
    }

    /// Get the current pitch register value
    pub fn current_pitch(&self) -> u16 {
        self.pitch
    }

    /// Get current ADSR level (for visualization)
    pub fn adsr_level(&self) -> i16 {
        self.adsr_level
    }

    /// Get current ADSR phase
    pub fn adsr_phase(&self) -> AdsrPhase {
        self.adsr_phase
    }

    /// Process one sample at 44100Hz
    ///
    /// Matches Duckstation SampleVoice() ordering:
    /// 1. Ensure decoded samples available
    /// 2. Interpolate at CURRENT counter position
    /// 3. Apply ADSR volume
    /// 4. Tick ADSR
    /// 5. Advance counter (advance sample position for next tick)
    /// 6. Apply L/R volume
    ///
    /// Returns (dry_left, dry_right) as i32 values ready for mixing.
    pub fn tick(&mut self, spu_ram: &SpuRam) -> (i32, i32) {
        if !self.active {
            return (0, 0);
        }

        // --- Step 1: Ensure we have decoded samples ---
        if !self.has_samples {
            self.decode_current_block(spu_ram);
            self.has_samples = true;

            // Check for loop start flag on this block
            let flags = spu_ram.read_byte(self.current_address + 1);
            if flags & 0x04 != 0 {
                self.loop_address = self.current_address;
                self.has_loop = true;
            }
        }

        // --- Step 2: Gaussian interpolation at current position ---
        let interpolated = self.gaussian_interpolate();

        // --- Step 3: Apply ADSR volume ---
        let volume = if self.adsr_level != 0 {
            (interpolated as i32 * self.adsr_level as i32) >> 15
        } else {
            0
        };

        // --- Step 4: Tick ADSR ---
        self.tick_adsr();

        if self.adsr_phase == AdsrPhase::Off {
            self.active = false;
            return (0, 0);
        }

        // --- Step 5: Advance pitch counter ---
        let step = self.pitch.min(0x3FFF) as u32;
        self.pitch_counter += step;

        // Check if we've crossed into the next sample (integer part >= 28)
        let sample_index = (self.pitch_counter >> 12) as usize;
        if sample_index >= SAMPLES_PER_ADPCM_BLOCK {
            // Wrap the sample index
            self.pitch_counter -= (SAMPLES_PER_ADPCM_BLOCK as u32) << 12;
            self.has_samples = false;

            // Check end flags on current block
            let flags = spu_ram.read_byte(self.current_address + 1);
            let is_loop_end = flags & 0x01 != 0;
            let is_loop_repeat = flags & 0x02 != 0;

            // Advance to next ADPCM block
            self.current_address += ADPCM_BLOCK_SIZE as u32;

            if is_loop_end {
                self.end_flag = true;
                if is_loop_repeat && self.has_loop {
                    self.current_address = self.loop_address;
                } else {
                    // Sample ended
                    self.active = false;
                    self.adsr_phase = AdsrPhase::Off;
                    self.adsr_level = 0;
                    return (0, 0);
                }
            }
        }

        // --- Step 6: Apply L/R volume ---
        let left = (volume * self.volume_left as i32) >> 15;
        let right = (volume * self.volume_right as i32) >> 15;

        (left, right)
    }

    /// Decode the ADPCM block at current_address and update Gaussian history
    ///
    /// Before decoding, saves the last 4 samples from the current (soon-to-be-previous)
    /// block into gauss_history for cross-block Gaussian interpolation.
    /// This matches Duckstation's carryover of NUM_SAMPLES_FROM_LAST_ADPCM_BLOCK samples.
    fn decode_current_block(&mut self, spu_ram: &SpuRam) {
        // Always carry over last 4 samples for cross-block Gaussian interpolation.
        // decoded_samples still contains the previous block's data (or zeros on first call).
        // The old code guarded this with `if self.has_samples`, but has_samples is already
        // false by the time we get here (set false when advancing blocks in tick()),
        // which caused stale history and audible scratching at every block boundary.
        self.gauss_history = [
            self.decoded_samples[SAMPLES_PER_ADPCM_BLOCK - 4],
            self.decoded_samples[SAMPLES_PER_ADPCM_BLOCK - 3],
            self.decoded_samples[SAMPLES_PER_ADPCM_BLOCK - 2],
            self.decoded_samples[SAMPLES_PER_ADPCM_BLOCK - 1],
        ];

        adpcm::decode_block_from_ram(
            spu_ram.data(),
            self.current_address as usize,
            &mut self.adpcm_prev1,
            &mut self.adpcm_prev2,
            &mut self.decoded_samples,
        );
    }

    /// Perform 4-point Gaussian interpolation
    ///
    /// Uses bits 4-11 of pitch_counter as index into the 512-entry Gaussian table.
    /// Reads from both the history buffer and the current decoded block.
    #[inline]
    fn gaussian_interpolate(&self) -> i16 {
        let interp_index = ((self.pitch_counter >> 4) & 0xFF) as usize;
        let sample_index = ((self.pitch_counter >> 12) as usize).min(SAMPLES_PER_ADPCM_BLOCK - 1);

        // Get the 4 samples needed for interpolation
        // We need samples at positions [s-3, s-2, s-1, s] relative to current position
        let s = [
            self.get_sample(sample_index as i32 - 3),
            self.get_sample(sample_index as i32 - 2),
            self.get_sample(sample_index as i32 - 1),
            self.get_sample(sample_index as i32),
        ];

        // Table lookups matching PS1 hardware layout
        let g0 = GAUSSIAN_TABLE[0xFF - interp_index] as i32;
        let g1 = GAUSSIAN_TABLE[0x1FF - interp_index] as i32;
        let g2 = GAUSSIAN_TABLE[0x100 + interp_index] as i32;
        let g3 = GAUSSIAN_TABLE[interp_index] as i32;

        let sum = g0 * s[0] as i32
                + g1 * s[1] as i32
                + g2 * s[2] as i32
                + g3 * s[3] as i32;

        (sum >> 15).clamp(-32768, 32767) as i16
    }

    /// Get a sample by index, using gauss_history for negative indices
    /// (samples from the previous block) and decoded_samples for positive indices
    #[inline]
    fn get_sample(&self, index: i32) -> i16 {
        if index < 0 {
            // Read from history (previous block carryover)
            // gauss_history[3] = sample at index -1
            // gauss_history[2] = sample at index -2
            // gauss_history[1] = sample at index -3
            // gauss_history[0] = sample at index -4 (rarely used)
            let hist_idx = (3 + index + 1) as usize;
            if hist_idx < 4 {
                self.gauss_history[hist_idx]
            } else {
                0
            }
        } else {
            let idx = index as usize;
            if idx < SAMPLES_PER_ADPCM_BLOCK {
                self.decoded_samples[idx]
            } else {
                0
            }
        }
    }

    // =========================================================================
    // ADSR Envelope — matches Duckstation Voice::TickADSR() + UpdateADSREnvelope()
    // =========================================================================

    /// Tick ADSR and handle phase transitions
    ///
    /// Matches Duckstation Voice::TickADSR():
    /// 1. Tick the envelope
    /// 2. Check if target reached (except sustain which never transitions)
    /// 3. Move to next phase if target reached
    fn tick_adsr(&mut self) {
        if self.adsr_phase == AdsrPhase::Off {
            return;
        }

        // Tick the hardware envelope
        self.adsr_envelope.tick(&mut self.adsr_level);

        // During release, if the level is very low, force it to Off.
        // With exponential release the step shrinks as level drops (step * level >> 15),
        // making the tail audibly silent long before mathematically reaching 0.
        if self.adsr_phase == AdsrPhase::Release && self.adsr_level < 0x10 {
            self.adsr_level = 0;
            self.adsr_phase = AdsrPhase::Off;
            return;
        }

        // Check for phase transition (sustain phase never transitions on its own)
        if self.adsr_phase != AdsrPhase::Sustain {
            let reached_target = if self.adsr_envelope.decreasing {
                self.adsr_level <= self.adsr_target
            } else {
                self.adsr_level >= self.adsr_target
            };

            if reached_target {
                // Clamp level to target on transition (matches Duckstation).
                // Attack→Decay: ensures level is exactly 0x7FFF, not some overshoot.
                // Decay→Sustain: ensures level is exactly sustain_target, preventing
                // fast decays from overshooting.
                self.adsr_level = self.adsr_target;

                self.adsr_phase = match self.adsr_phase {
                    AdsrPhase::Attack => AdsrPhase::Decay,
                    AdsrPhase::Decay => AdsrPhase::Sustain,
                    AdsrPhase::Release => AdsrPhase::Off,
                    other => other,
                };
                self.update_adsr_envelope();
            }
        }
    }

    /// Configure the envelope for the current ADSR phase
    ///
    /// Matches Duckstation Voice::UpdateADSREnvelope() exactly.
    fn update_adsr_envelope(&mut self) {
        match self.adsr_phase {
            AdsrPhase::Off => {
                self.adsr_target = 0;
                self.adsr_envelope.reset(0, 0, false, false);
            }
            AdsrPhase::Attack => {
                self.adsr_target = 0x7FFF; // Max volume
                let rate = self.compute_attack_rate();
                self.adsr_envelope.reset(
                    rate,
                    0x7F, // rate_mask for 7-bit rate
                    false, // increasing
                    self.adsr_params.attack_exp,
                );
            }
            AdsrPhase::Decay => {
                // Target = sustain level: (sustain_level + 1) * 0x800, clamped to 0x7FFF
                self.adsr_target = self.adsr_params.sustain_level_i16();
                let rate = self.compute_decay_rate();
                self.adsr_envelope.reset(
                    rate,
                    0x1F << 2, // rate_mask for 5-bit shift (decay has no step bits)
                    true,  // decreasing
                    true,  // always exponential
                );
            }
            AdsrPhase::Sustain => {
                self.adsr_target = 0; // Sustain doesn't use target (never transitions)
                let rate = self.compute_sustain_rate();
                self.adsr_envelope.reset(
                    rate,
                    0x7F, // rate_mask for 7-bit rate
                    self.adsr_params.sustain_decrease,
                    self.adsr_params.sustain_exp,
                );
            }
            AdsrPhase::Release => {
                self.adsr_target = 0;
                let rate = self.compute_release_rate();
                self.adsr_envelope.reset(
                    rate,
                    0x1F << 2, // rate_mask for 5-bit shift
                    true, // decreasing
                    self.adsr_params.release_exp,
                );
            }
        }
    }

    /// Attack rate: shift(5 bits) * 4 + step(2 bits) = 0-127
    fn compute_attack_rate(&self) -> u8 {
        let rate = (self.adsr_params.attack_shift as u32) * 4
            + self.adsr_params.attack_step as u32;
        rate.min(127) as u8
    }

    /// Decay rate: shift(4 bits) * 4 = 0-60 (no step component)
    fn compute_decay_rate(&self) -> u8 {
        let rate = (self.adsr_params.decay_shift as u32) * 4;
        rate.min(127) as u8
    }

    /// Sustain rate: shift(5 bits) * 4 + step(2 bits) = 0-127
    fn compute_sustain_rate(&self) -> u8 {
        let rate = (self.adsr_params.sustain_shift as u32) * 4
            + self.adsr_params.sustain_step as u32;
        rate.min(127) as u8
    }

    /// Release rate: shift(5 bits) * 4 = 0-124 (no step component)
    fn compute_release_rate(&self) -> u8 {
        let rate = (self.adsr_params.release_shift as u32) * 4;
        rate.min(127) as u8
    }
}

impl Default for Voice {
    fn default() -> Self {
        Self::new()
    }
}
