//! PS1 SPU type definitions
//!
//! Core data structures for the SPU emulation engine:
//! - SpuRam: Virtual 512KB sample memory
//! - AdpcmBlock: 16-byte compressed audio block (28 samples)
//! - AdsrParams: Hardware envelope configuration
//! - SampleRegion: Instrument key-split region with sample mapping
//! - InstrumentBank: Collection of regions for one GM instrument
//! - SampleLibrary: Complete loaded sample set with SPU RAM

use super::tables::SPU_RAM_SIZE;

/// Virtual SPU RAM — 512KB of sample data
///
/// The PS1 SPU has 512KB of dedicated RAM for sample data.
/// ADPCM-encoded samples are loaded here, and voices reference
/// addresses within this memory for playback.
pub struct SpuRam {
    data: Vec<u8>,
    /// Next free byte offset for allocation
    next_free: usize,
}

impl SpuRam {
    pub fn new() -> Self {
        Self {
            data: vec![0u8; SPU_RAM_SIZE],
            next_free: 0,
        }
    }

    /// Read a byte from SPU RAM
    #[inline]
    pub fn read_byte(&self, addr: u32) -> u8 {
        self.data[(addr as usize) % SPU_RAM_SIZE]
    }

    /// Read a 16-bit sample from SPU RAM (little-endian)
    #[inline]
    pub fn read_i16(&self, addr: u32) -> i16 {
        let idx = (addr as usize) % SPU_RAM_SIZE;
        // Ensure aligned read (SPU RAM is byte-addressable but samples are 16-bit)
        let lo = self.data[idx] as u16;
        let hi = self.data[(idx + 1) % SPU_RAM_SIZE] as u16;
        (lo | (hi << 8)) as i16
    }

    /// Write a 16-bit sample to SPU RAM (little-endian)
    #[inline]
    pub fn write_i16(&mut self, addr: u32, value: i16) {
        let idx = (addr as usize) % SPU_RAM_SIZE;
        let bytes = (value as u16).to_le_bytes();
        self.data[idx] = bytes[0];
        self.data[(idx + 1) % SPU_RAM_SIZE] = bytes[1];
    }

    /// Copy a slice of bytes into SPU RAM at the given offset
    /// Returns the start address where data was written
    pub fn write_bytes(&mut self, offset: usize, data: &[u8]) {
        for (i, &byte) in data.iter().enumerate() {
            self.data[(offset + i) % SPU_RAM_SIZE] = byte;
        }
    }

    /// Allocate space in SPU RAM and copy data there
    /// Returns the byte offset where the data was placed, or None if full
    pub fn allocate(&mut self, data: &[u8]) -> Option<u32> {
        // Align to 16-byte boundary (ADPCM block size)
        let aligned = (self.next_free + 15) & !15;
        if aligned + data.len() > SPU_RAM_SIZE {
            return None;
        }
        let offset = aligned as u32;
        self.write_bytes(aligned, data);
        self.next_free = aligned + data.len();
        Some(offset)
    }

    /// Reset allocation pointer (call before loading a new sample library)
    pub fn reset(&mut self) {
        self.data.fill(0);
        self.next_free = 0;
    }

    /// Get total allocated bytes
    pub fn allocated_bytes(&self) -> usize {
        self.next_free
    }

    /// Get raw slice for direct access (used by reverb for buffer memory)
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Get mutable raw slice
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

impl Default for SpuRam {
    fn default() -> Self {
        Self::new()
    }
}

/// ADPCM block flags
pub mod adpcm_flags {
    /// End of sample data — voice stops or loops
    pub const LOOP_END: u8 = 0x01;
    /// Loop repeat — jump to loop address instead of stopping
    pub const LOOP_REPEAT: u8 = 0x02;
    /// Loop start — set the voice's loop address to this block
    pub const LOOP_START: u8 = 0x04;
}

/// A single 16-byte ADPCM block encoding 28 samples
///
/// Layout:
/// - Byte 0: shift (bits 0-3) | filter (bits 4-6)
/// - Byte 1: flags (bit 0: loop end, bit 1: loop repeat, bit 2: loop start)
/// - Bytes 2-15: 28 nibbles of compressed sample data (2 per byte, low nibble first)
#[derive(Clone, Copy)]
#[repr(C)]
pub struct AdpcmBlock {
    pub shift_filter: u8,
    pub flags: u8,
    pub data: [u8; 14],
}

impl AdpcmBlock {
    /// Get the shift value (0-12, values 13-15 are treated as 9 by hardware)
    #[inline]
    pub fn shift(&self) -> u8 {
        self.shift_filter & 0x0F
    }

    /// Get the filter index (0-4, values 5-7 behave like 0 on real hardware)
    #[inline]
    pub fn filter(&self) -> u8 {
        (self.shift_filter >> 4) & 0x07
    }

    /// Get the i-th nibble (0-27) as a sign-extended i16
    #[inline]
    pub fn get_nibble(&self, index: usize) -> i16 {
        let byte = self.data[index / 2];
        let nibble = if index & 1 == 0 {
            byte & 0x0F // Low nibble first
        } else {
            byte >> 4 // High nibble second
        };
        // Sign-extend 4-bit to 16-bit
        ((nibble as i16) << 12) >> 12
    }

    /// Check if this block has the loop end flag
    #[inline]
    pub fn is_loop_end(&self) -> bool {
        self.flags & adpcm_flags::LOOP_END != 0
    }

    /// Check if this block should loop (end + repeat)
    #[inline]
    pub fn is_loop_repeat(&self) -> bool {
        self.flags & adpcm_flags::LOOP_REPEAT != 0
    }

    /// Check if this block marks the loop start point
    #[inline]
    pub fn is_loop_start(&self) -> bool {
        self.flags & adpcm_flags::LOOP_START != 0
    }

    /// Create from raw 16 bytes
    pub fn from_bytes(bytes: &[u8; 16]) -> Self {
        let mut data = [0u8; 14];
        data.copy_from_slice(&bytes[2..16]);
        Self {
            shift_filter: bytes[0],
            flags: bytes[1],
            data,
        }
    }

    /// Serialize to 16 bytes
    pub fn to_bytes(&self) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        bytes[0] = self.shift_filter;
        bytes[1] = self.flags;
        bytes[2..16].copy_from_slice(&self.data);
        bytes
    }
}

impl Default for AdpcmBlock {
    fn default() -> Self {
        Self {
            shift_filter: 0,
            flags: 0,
            data: [0u8; 14],
        }
    }
}

/// ADSR envelope configuration matching PS1 hardware registers
///
/// The PS1 SPU ADSR uses two 16-bit registers (ADSR1 and ADSR2):
///
/// ADSR1 (low 16 bits):
///   bits 0-4:   sustain_level (0-15, maps to level * 0x800 + 0x800... wait,
///               actual mapping: (sustain_level + 1) * 0x800, so 0→0x800, 15→0x8000→clamped to 0x7FFF)
///   bits 5-8:   decay_rate (0-15, always exponential decrease)
///   bits 9-14:  attack_rate (0-127... actually only 7 bits: 0-127... wait)
///
/// Actually the PS1 register layout is:
///   ADSR1 = sustain_level(4) | decay_shift(4) | attack_step(2) | attack_shift(5) | attack_mode(1)
///   ADSR2 = release_mode(1) | release_shift(5) | sustain_dir(1) | sustain_step(2) | sustain_shift(5) | sustain_mode(1)
///
/// We store the logical parameters here and compute the hardware rate/step at use time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdsrParams {
    /// Attack mode: false = linear, true = exponential
    pub attack_exp: bool,
    /// Attack shift (0-31) — controls attack speed
    pub attack_shift: u8,
    /// Attack step (0-3) — fine-tunes attack speed
    pub attack_step: u8,
    /// Decay shift (0-15) — always exponential decrease
    pub decay_shift: u8,
    /// Sustain level (0-15) — target level after decay
    /// Maps to (sustain_level + 1) << 11, clamped to 0x7FFF
    pub sustain_level: u8,
    /// Sustain mode: false = linear, true = exponential
    pub sustain_exp: bool,
    /// Sustain direction: false = increase, true = decrease
    pub sustain_decrease: bool,
    /// Sustain shift (0-31)
    pub sustain_shift: u8,
    /// Sustain step (0-3)
    pub sustain_step: u8,
    /// Release mode: false = linear, true = exponential
    pub release_exp: bool,
    /// Release shift (0-31)
    pub release_shift: u8,
}

impl AdsrParams {
    /// Get the sustain level as a 16-bit value (0-0x7FFF)
    pub fn sustain_level_i16(&self) -> i16 {
        let level = ((self.sustain_level as i32 + 1) << 11).min(0x7FFF);
        level as i16
    }

    /// Encode as PS1 ADSR1 register (16 bits)
    pub fn to_adsr1(&self) -> u16 {
        let mut val: u16 = 0;
        val |= (self.sustain_level as u16 & 0xF) << 0;
        val |= (self.decay_shift as u16 & 0xF) << 4;
        val |= (self.attack_step as u16 & 0x3) << 8;
        val |= (self.attack_shift as u16 & 0x1F) << 10;
        val |= (self.attack_exp as u16) << 15;
        val
    }

    /// Encode as PS1 ADSR2 register (16 bits)
    pub fn to_adsr2(&self) -> u16 {
        let mut val: u16 = 0;
        val |= (self.sustain_exp as u16) << 0;
        val |= (self.sustain_shift as u16 & 0x1F) << 1;
        val |= (self.sustain_step as u16 & 0x3) << 6;
        val |= if self.sustain_decrease { 1u16 << 8 } else { 0 };
        val |= (self.release_shift as u16 & 0x1F) << 9;
        val |= (self.release_exp as u16) << 14;
        val
    }

    /// Quick preset: short percussive envelope
    pub fn percussive() -> Self {
        Self {
            attack_exp: false,
            attack_shift: 0,
            attack_step: 3,
            decay_shift: 2,
            sustain_level: 10,
            sustain_exp: true,
            sustain_decrease: true,
            sustain_shift: 8,
            sustain_step: 0,
            release_exp: true,
            release_shift: 5,
        }
    }

    /// Quick preset: sustained tone (organ, strings, etc.)
    pub fn sustained() -> Self {
        Self {
            attack_exp: false,
            attack_shift: 2,
            attack_step: 3,
            decay_shift: 6,
            sustain_level: 12,
            sustain_exp: false,
            sustain_decrease: false,
            sustain_shift: 31, // No change during sustain
            sustain_step: 0,
            release_exp: true,
            release_shift: 10,
        }
    }
}

impl Default for AdsrParams {
    fn default() -> Self {
        // Default: fast attack, moderate sustain, moderate release
        Self {
            attack_exp: false,
            attack_shift: 0,
            attack_step: 3, // fastest
            decay_shift: 4,
            sustain_level: 12,
            sustain_exp: false,
            sustain_decrease: false,
            sustain_shift: 31, // effectively no sustain change
            sustain_step: 0,
            release_exp: true,
            release_shift: 8,
        }
    }
}

/// ADSR envelope phase
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdsrPhase {
    Attack,
    Decay,
    Sustain,
    Release,
    Off,
}

/// A single instrument sample region
///
/// Maps a range of MIDI keys to a sample stored in SPU RAM.
/// Multiple regions can cover the full keyboard for one instrument (key splits).
#[derive(Debug, Clone)]
pub struct SampleRegion {
    /// Start address of ADPCM data in SPU RAM (byte offset)
    pub spu_ram_offset: u32,
    /// Loop start address in SPU RAM (byte offset to the ADPCM block)
    pub loop_offset: u32,
    /// Whether this sample loops
    pub has_loop: bool,
    /// Length of ADPCM data in bytes
    pub adpcm_length: u32,
    /// MIDI note this sample was recorded at (root key)
    pub base_note: u8,
    /// SPU pitch register value for playback at the root key's frequency
    /// Accounts for the sample's original sample rate vs 44100Hz
    pub base_pitch: u16,
    /// Lowest MIDI note this region covers
    pub key_lo: u8,
    /// Highest MIDI note this region covers
    pub key_hi: u8,
    /// ADSR envelope parameters for this sample
    pub adsr: AdsrParams,
    /// Base volume for this sample (0-0x3FFF)
    pub default_volume: i16,
    /// Fine tuning in cents (-100 to +100)
    pub fine_tune: i16,
}

impl SampleRegion {
    /// Calculate the SPU pitch register value for a given MIDI note
    /// Uses equal temperament: pitch = base_pitch * 2^((note - base_note + fine_tune/100) / 12)
    pub fn pitch_for_note(&self, note: u8) -> u16 {
        let semitone_diff = note as f64 - self.base_note as f64 + self.fine_tune as f64 / 100.0;
        let ratio = (semitone_diff / 12.0).exp2();
        let pitch = (self.base_pitch as f64 * ratio) as u32;
        // Clamp to PS1 max pitch (0x3FFF)
        pitch.min(0x3FFF) as u16
    }
}

/// Instrument bank — holds all sample regions for one GM instrument
#[derive(Debug, Clone)]
pub struct InstrumentBank {
    /// Instrument name (from SF2 or GM standard)
    pub name: String,
    /// GM program number (0-127)
    pub program: u8,
    /// Key-split regions for this instrument, sorted by key_lo
    pub regions: Vec<SampleRegion>,
}

impl InstrumentBank {
    /// Find the region that covers a given MIDI note
    pub fn region_for_note(&self, note: u8) -> Option<&SampleRegion> {
        self.regions.iter().find(|r| note >= r.key_lo && note <= r.key_hi)
    }
}

/// Complete sample library loaded and converted from SF2
///
/// Contains all ADPCM-encoded samples in SPU RAM and
/// instrument mappings for all 128 GM programs.
pub struct SampleLibrary {
    /// Virtual SPU RAM with all encoded samples
    pub spu_ram: SpuRam,
    /// Instrument banks indexed by GM program number (0-127)
    /// Empty vec for unused programs
    pub instruments: Vec<InstrumentBank>,
    /// Total number of samples encoded
    pub sample_count: usize,
    /// Source soundfont name
    pub source_name: String,
}

impl SampleLibrary {
    pub fn new(source_name: String) -> Self {
        Self {
            spu_ram: SpuRam::new(),
            instruments: Vec::with_capacity(128),
            sample_count: 0,
            source_name,
        }
    }

    /// Get instrument bank for a GM program number
    pub fn instrument(&self, program: u8) -> Option<&InstrumentBank> {
        self.instruments.iter().find(|b| b.program == program)
    }

    /// Get human-readable names for all loaded instruments
    pub fn instrument_names(&self) -> Vec<(u8, String)> {
        self.instruments.iter()
            .map(|b| (b.program, b.name.clone()))
            .collect()
    }

    /// Reset the library — clear SPU RAM and all instruments
    /// Used when SPU RAM is full and we need to reload a different set of programs
    pub fn reset(&mut self) {
        self.spu_ram.reset();
        self.instruments.clear();
        self.sample_count = 0;
    }
}
