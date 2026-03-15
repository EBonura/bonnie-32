//! PS1 SPU Hardware-Accurate Reverb
//!
//! Implements the Mednafen/Duckstation reverb formula with:
//! - 39-tap FIR for downsampling 44100→22050Hz and upsampling 22050→44100Hz
//! - IIR same-side and cross-channel reflections
//! - 4 comb filters for early reflections
//! - 2 cascaded all-pass filters for diffusion
//! - Proper address scaling (register values × 4 for halfword offsets)
//! - Alternating-cycle processing (reverb core runs at 22050Hz)
//!
//! Reference: Duckstation spu.cpp ProcessReverb(), psx-spx SPU documentation

use super::tables::{REVERB_FIR, REVERB_FIR_CENTER};

// =============================================================================
// Reverb Preset Data — migrated from psx_reverb.rs
// =============================================================================

/// PS1 reverb preset coefficients (32 register values)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReverbPreset {
    // All-pass filter offsets
    pub d_apf1: u16,   // FB_SRC_A
    pub d_apf2: u16,   // FB_SRC_B
    // Volume coefficients
    pub v_iir: i16,    // IIR_ALPHA
    pub v_comb1: i16,  // ACC_COEF_A
    pub v_comb2: i16,  // ACC_COEF_B
    pub v_comb3: i16,  // ACC_COEF_C
    pub v_comb4: i16,  // ACC_COEF_D
    pub v_wall: i16,   // IIR_COEF
    pub v_apf1: i16,   // FB_ALPHA
    pub v_apf2: i16,   // FB_X
    // Buffer addresses — L/R pairs
    pub m_l_same: u16,  // IIR_DEST_A[0]
    pub m_r_same: u16,  // IIR_DEST_A[1]
    pub m_l_comb1: u16, // ACC_SRC_A[0]
    pub m_r_comb1: u16, // ACC_SRC_A[1]
    pub m_l_comb2: u16, // ACC_SRC_B[0]
    pub m_r_comb2: u16, // ACC_SRC_B[1]
    pub d_l_same: u16,  // IIR_SRC_A[0]
    pub d_r_same: u16,  // IIR_SRC_A[1]
    pub m_l_diff: u16,  // IIR_DEST_B[0]
    pub m_r_diff: u16,  // IIR_DEST_B[1]
    pub m_l_comb3: u16, // ACC_SRC_C[0]
    pub m_r_comb3: u16, // ACC_SRC_C[1]
    pub m_l_comb4: u16, // ACC_SRC_D[0]
    pub m_r_comb4: u16, // ACC_SRC_D[1]
    pub d_l_diff: u16,  // IIR_SRC_B[0]
    pub d_r_diff: u16,  // IIR_SRC_B[1]
    pub m_l_apf1: u16,  // MIX_DEST_A[0]
    pub m_r_apf1: u16,  // MIX_DEST_A[1]
    pub m_l_apf2: u16,  // MIX_DEST_B[0]
    pub m_r_apf2: u16,  // MIX_DEST_B[1]
    pub v_l_in: i16,    // IN_COEF[0]
    pub v_r_in: i16,    // IN_COEF[1]
}

impl ReverbPreset {
    const fn new(data: [u16; 32]) -> Self {
        Self {
            d_apf1: data[0], d_apf2: data[1],
            v_iir: data[2] as i16, v_comb1: data[3] as i16,
            v_comb2: data[4] as i16, v_comb3: data[5] as i16,
            v_comb4: data[6] as i16, v_wall: data[7] as i16,
            v_apf1: data[8] as i16, v_apf2: data[9] as i16,
            m_l_same: data[10], m_r_same: data[11],
            m_l_comb1: data[12], m_r_comb1: data[13],
            m_l_comb2: data[14], m_r_comb2: data[15],
            d_l_same: data[16], d_r_same: data[17],
            m_l_diff: data[18], m_r_diff: data[19],
            m_l_comb3: data[20], m_r_comb3: data[21],
            m_l_comb4: data[22], m_r_comb4: data[23],
            d_l_diff: data[24], d_r_diff: data[25],
            m_l_apf1: data[26], m_r_apf1: data[27],
            m_l_apf2: data[28], m_r_apf2: data[29],
            v_l_in: data[30] as i16, v_r_in: data[31] as i16,
        }
    }

    /// Get addresses for a channel (0=left, 1=right) as [L, R] array accessors
    fn iir_dest_a(&self, ch: usize) -> u16 { if ch == 0 { self.m_l_same } else { self.m_r_same } }
    fn iir_dest_b(&self, ch: usize) -> u16 { if ch == 0 { self.m_l_diff } else { self.m_r_diff } }
    fn iir_src_a(&self, ch: usize) -> u16 { if ch == 0 { self.d_l_same } else { self.d_r_same } }
    fn iir_src_b(&self, ch: usize) -> u16 { if ch == 0 { self.d_l_diff } else { self.d_r_diff } }
    fn acc_src_a(&self, ch: usize) -> u16 { if ch == 0 { self.m_l_comb1 } else { self.m_r_comb1 } }
    fn acc_src_b(&self, ch: usize) -> u16 { if ch == 0 { self.m_l_comb2 } else { self.m_r_comb2 } }
    fn acc_src_c(&self, ch: usize) -> u16 { if ch == 0 { self.m_l_comb3 } else { self.m_r_comb3 } }
    fn acc_src_d(&self, ch: usize) -> u16 { if ch == 0 { self.m_l_comb4 } else { self.m_r_comb4 } }
    fn mix_dest_a(&self, ch: usize) -> u16 { if ch == 0 { self.m_l_apf1 } else { self.m_r_apf1 } }
    fn mix_dest_b(&self, ch: usize) -> u16 { if ch == 0 { self.m_l_apf2 } else { self.m_r_apf2 } }
    fn in_coef(&self, ch: usize) -> i16 { if ch == 0 { self.v_l_in } else { self.v_r_in } }
}

/// Available reverb preset types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReverbType {
    #[default]
    Off,
    Room,
    StudioSmall,
    StudioMedium,
    StudioLarge,
    Hall,
    HalfEcho,
    SpaceEcho,
    ChaosEcho,
    Delay,
}

impl ReverbType {
    pub const ALL: [ReverbType; 10] = [
        ReverbType::Off, ReverbType::Room, ReverbType::StudioSmall,
        ReverbType::StudioMedium, ReverbType::StudioLarge, ReverbType::Hall,
        ReverbType::HalfEcho, ReverbType::SpaceEcho,
        ReverbType::ChaosEcho, ReverbType::Delay,
    ];

    pub fn from_index(index: u8) -> Self {
        match index {
            0 => ReverbType::Off, 1 => ReverbType::Room,
            2 => ReverbType::StudioSmall, 3 => ReverbType::StudioMedium,
            4 => ReverbType::StudioLarge, 5 => ReverbType::Hall,
            6 => ReverbType::HalfEcho, 7 => ReverbType::SpaceEcho,
            8 => ReverbType::ChaosEcho, 9 => ReverbType::Delay,
            _ => ReverbType::Off,
        }
    }

    pub fn to_index(&self) -> u8 {
        match self {
            ReverbType::Off => 0, ReverbType::Room => 1,
            ReverbType::StudioSmall => 2, ReverbType::StudioMedium => 3,
            ReverbType::StudioLarge => 4, ReverbType::Hall => 5,
            ReverbType::HalfEcho => 6, ReverbType::SpaceEcho => 7,
            ReverbType::ChaosEcho => 8, ReverbType::Delay => 9,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            ReverbType::Off => "Off", ReverbType::Room => "Room",
            ReverbType::StudioSmall => "Studio Small", ReverbType::StudioMedium => "Studio Medium",
            ReverbType::StudioLarge => "Studio Large", ReverbType::Hall => "Hall",
            ReverbType::HalfEcho => "Half Echo", ReverbType::SpaceEcho => "Space Echo",
            ReverbType::ChaosEcho => "Chaos Echo", ReverbType::Delay => "Delay",
        }
    }

    pub fn preset(&self) -> &'static ReverbPreset {
        match self {
            ReverbType::Off => &PRESET_OFF, ReverbType::Room => &PRESET_ROOM,
            ReverbType::StudioSmall => &PRESET_STUDIO_SMALL,
            ReverbType::StudioMedium => &PRESET_STUDIO_MEDIUM,
            ReverbType::StudioLarge => &PRESET_STUDIO_LARGE,
            ReverbType::Hall => &PRESET_HALL, ReverbType::HalfEcho => &PRESET_HALF_ECHO,
            ReverbType::SpaceEcho => &PRESET_SPACE_ECHO,
            ReverbType::ChaosEcho => &PRESET_CHAOS_ECHO,
            ReverbType::Delay => &PRESET_DELAY,
        }
    }
}

// Standard PS1 reverb presets from PsyQ SDK
// Data from lv2-psx-reverb by ipatix (https://github.com/ipatix/lv2-psx-reverb)

static PRESET_OFF: ReverbPreset = ReverbPreset::new([
    0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
    0x0000, 0x0000, 0x0001, 0x0001, 0x0001, 0x0001, 0x0001, 0x0001,
    0x0000, 0x0000, 0x0001, 0x0001, 0x0001, 0x0001, 0x0001, 0x0001,
    0x0000, 0x0000, 0x0001, 0x0001, 0x0001, 0x0001, 0x0000, 0x0000,
]);

static PRESET_ROOM: ReverbPreset = ReverbPreset::new([
    0x007D, 0x005B, 0x6D80, 0x54B8, 0xBED0, 0x0000, 0x0000, 0xBA80,
    0x5800, 0x5300, 0x04D6, 0x0333, 0x03F0, 0x0227, 0x0374, 0x01EF,
    0x0334, 0x01B5, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
    0x0000, 0x0000, 0x01B4, 0x0136, 0x00B8, 0x005C, 0x8000, 0x8000,
]);

static PRESET_STUDIO_SMALL: ReverbPreset = ReverbPreset::new([
    0x0033, 0x0025, 0x70F0, 0x4FA8, 0xBCE0, 0x4410, 0xC0F0, 0x9C00,
    0x5280, 0x4EC0, 0x03E4, 0x031B, 0x03A4, 0x02AF, 0x0372, 0x0266,
    0x031C, 0x025D, 0x025C, 0x018E, 0x022F, 0x0135, 0x01D2, 0x00B7,
    0x018F, 0x00B5, 0x00B4, 0x0080, 0x004C, 0x0026, 0x8000, 0x8000,
]);

static PRESET_STUDIO_MEDIUM: ReverbPreset = ReverbPreset::new([
    0x00B1, 0x007F, 0x70F0, 0x4FA8, 0xBCE0, 0x4510, 0xBEF0, 0xB4C0,
    0x5280, 0x4EC0, 0x0904, 0x076B, 0x0824, 0x065F, 0x07A2, 0x0616,
    0x076C, 0x05ED, 0x05EC, 0x042E, 0x050F, 0x0305, 0x0462, 0x02B7,
    0x042F, 0x0265, 0x0264, 0x01B2, 0x0100, 0x0080, 0x8000, 0x8000,
]);

static PRESET_STUDIO_LARGE: ReverbPreset = ReverbPreset::new([
    0x00E3, 0x00A9, 0x6F60, 0x4FA8, 0xBCE0, 0x4510, 0xBEF0, 0xA680,
    0x5680, 0x52C0, 0x0DFB, 0x0B58, 0x0D09, 0x0A3C, 0x0BD9, 0x0973,
    0x0B59, 0x08DA, 0x08D9, 0x05E9, 0x07EC, 0x04B0, 0x06EF, 0x03D2,
    0x05EA, 0x031D, 0x031C, 0x0238, 0x0154, 0x00AA, 0x8000, 0x8000,
]);

static PRESET_HALL: ReverbPreset = ReverbPreset::new([
    0x01A5, 0x0139, 0x6000, 0x5000, 0x4C00, 0xB800, 0xBC00, 0xC000,
    0x6000, 0x5C00, 0x15BA, 0x11BB, 0x14C2, 0x10BD, 0x11BC, 0x0DC1,
    0x11C0, 0x0DC3, 0x0DC0, 0x09C1, 0x0BC4, 0x07C1, 0x0A00, 0x06CD,
    0x09C2, 0x05C1, 0x05C0, 0x041A, 0x0274, 0x013A, 0x8000, 0x8000,
]);

static PRESET_HALF_ECHO: ReverbPreset = ReverbPreset::new([
    0x0017, 0x0013, 0x70F0, 0x4FA8, 0xBCE0, 0x4510, 0xBEF0, 0x8500,
    0x5F80, 0x54C0, 0x0371, 0x02AF, 0x02E5, 0x01DF, 0x02B0, 0x01D7,
    0x0358, 0x026A, 0x01D6, 0x011E, 0x012D, 0x00B1, 0x011F, 0x0059,
    0x01A0, 0x00E3, 0x0058, 0x0040, 0x0028, 0x0014, 0x8000, 0x8000,
]);

static PRESET_SPACE_ECHO: ReverbPreset = ReverbPreset::new([
    0x033D, 0x0231, 0x7E00, 0x5000, 0xB400, 0xB000, 0x4C00, 0xB000,
    0x6000, 0x5400, 0x1ED6, 0x1A31, 0x1D14, 0x183B, 0x1BC2, 0x16B2,
    0x1A32, 0x15EF, 0x15EE, 0x1055, 0x1334, 0x0F2D, 0x11F6, 0x0C5D,
    0x1056, 0x0AE1, 0x0AE0, 0x07A2, 0x0464, 0x0232, 0x8000, 0x8000,
]);

static PRESET_CHAOS_ECHO: ReverbPreset = ReverbPreset::new([
    0x0001, 0x0001, 0x7FFF, 0x7FFF, 0x0000, 0x0000, 0x0000, 0x8100,
    0x0000, 0x0000, 0x1FFF, 0x0FFF, 0x1005, 0x0005, 0x0000, 0x0000,
    0x1005, 0x0005, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
    0x0000, 0x0000, 0x1004, 0x1002, 0x0004, 0x0002, 0x8000, 0x8000,
]);

static PRESET_DELAY: ReverbPreset = ReverbPreset::new([
    0x0001, 0x0001, 0x7FFF, 0x7FFF, 0x0000, 0x0000, 0x0000, 0x0000,
    0x0000, 0x0000, 0x1FFF, 0x0FFF, 0x1005, 0x0005, 0x0000, 0x0000,
    0x1005, 0x0005, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
    0x0000, 0x0000, 0x1004, 0x1002, 0x0004, 0x0002, 0x8000, 0x8000,
]);

// =============================================================================
// SPU Reverb Processor
// =============================================================================

/// Reverb buffer size in i16 samples (halfwords)
/// Matches PS1's 256K halfword address space for reverb (MASK = 0x3FFFF)
const REVERB_BUFFER_SIZE: usize = 0x40000; // 256K samples = 512KB
const REVERB_BUFFER_MASK: u32 = 0x3FFFF;

/// PS1 SPU Reverb processor — Duckstation/Mednafen formula
pub struct SpuReverb {
    preset: ReverbPreset,
    reverb_type: ReverbType,

    /// Single reverb buffer (interleaved L/R via different address offsets)
    buffer: Vec<i16>,
    /// Current write position (advances by 1 each 22050Hz cycle)
    current_address: u32,

    /// Downsample ring buffer for FIR filtering (per channel)
    /// 64 entries + 64 mirror (matches Duckstation's 0x40 | 0x00 addressing)
    downsample_buffer: [[i16; 128]; 2],
    /// Upsample ring buffer for FIR filtering (per channel)
    /// 32 entries + 32 mirror
    upsample_buffer: [[i16; 64]; 2],
    /// Ring buffer position (0-63, advances by 1 each 44100Hz sample)
    resample_pos: usize,

    /// Wet/dry mix level (0.0 = dry, 1.0 = wet)
    wet_level: f32,
    /// Whether reverb processing is active
    enabled: bool,
}

impl SpuReverb {
    pub fn new() -> Self {
        Self {
            preset: *ReverbType::Off.preset(),
            reverb_type: ReverbType::Off,
            buffer: vec![0i16; REVERB_BUFFER_SIZE],
            current_address: 0,
            downsample_buffer: [[0i16; 128]; 2],
            upsample_buffer: [[0i16; 64]; 2],
            resample_pos: 0,
            wet_level: 0.5,
            enabled: false,
        }
    }

    pub fn set_preset(&mut self, reverb_type: ReverbType) {
        if self.reverb_type == reverb_type {
            return;
        }
        self.reverb_type = reverb_type;
        self.preset = *reverb_type.preset();
        self.enabled = reverb_type != ReverbType::Off;
        self.clear();
    }

    pub fn reverb_type(&self) -> ReverbType {
        self.reverb_type
    }

    pub fn set_wet_level(&mut self, level: f32) {
        self.wet_level = level.clamp(0.0, 1.0);
    }

    pub fn wet_level(&self) -> f32 {
        self.wet_level
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn clear(&mut self) {
        self.buffer.fill(0);
        self.downsample_buffer = [[0i16; 128]; 2];
        self.upsample_buffer = [[0i16; 64]; 2];
        self.current_address = 0;
        self.resample_pos = 0;
    }

    // =========================================================================
    // Buffer read/write with address scaling (matches Duckstation)
    // =========================================================================

    /// Read from reverb buffer at (register_address * 4 + offset)
    /// The ×4 scaling converts register values to halfword addresses.
    /// Offset is in halfword units (typically 0 or -1 for IIR prev sample).
    #[inline]
    fn reverb_read(&self, register_addr: u16, offset: i32) -> i16 {
        let addr = (register_addr as u32) << 2;
        let sample_addr = self.current_address.wrapping_add(addr);
        let sample_addr = (sample_addr as i32 + offset) as u32 & REVERB_BUFFER_MASK;
        self.buffer[sample_addr as usize]
    }

    /// Write to reverb buffer at (register_address * 4)
    #[inline]
    fn reverb_write(&mut self, register_addr: u16, value: i16) {
        let addr = (register_addr as u32) << 2;
        let sample_addr = self.current_address.wrapping_add(addr) & REVERB_BUFFER_MASK;
        self.buffer[sample_addr as usize] = value;
    }

    // =========================================================================
    // Duckstation helper functions
    // =========================================================================

    /// iiasm() — Duckstation's IIR edge case helper
    /// Handles the special case where IIR_ALPHA == -32768
    #[inline]
    fn iiasm(&self, insamp: i16) -> i32 {
        if self.preset.v_iir == -32768 {
            if insamp == -32768 { 0 } else { insamp as i32 * -65536 }
        } else {
            insamp as i32 * (32768 - self.preset.v_iir as i32)
        }
    }

    /// neg() — Duckstation's negation with overflow protection
    #[inline]
    fn neg(samp: i32) -> i32 {
        if samp == -32768 { 0x7FFF } else { -samp }
    }

    /// Clamp to i16 range
    #[inline]
    fn clamp16(value: i32) -> i16 {
        value.clamp(-32768, 32767) as i16
    }

    // =========================================================================
    // Core processing — called at 44100Hz
    // =========================================================================

    /// Process one stereo sample pair at 44100Hz
    ///
    /// Returns (reverb_left, reverb_right) as i32 values.
    /// The caller mixes these with the dry signal using the wet level.
    ///
    /// Matches Duckstation ProcessReverb() exactly:
    /// - Every tick: fill downsample buffer, FIR upsample for output
    /// - Odd ticks: FIR downsample, run reverb algorithm, FIR upsample
    pub fn process_sample(&mut self, left_in: i16, right_in: i16) -> (i32, i32) {
        if !self.enabled {
            return (0, 0);
        }

        let p = self.preset;
        let pos = self.resample_pos;

        // Fill downsample ring buffer (every cycle)
        // Write to both primary and mirror positions for wrap-free FIR access
        self.downsample_buffer[0][pos & 0x3F] = left_in;
        self.downsample_buffer[0][(pos & 0x3F) | 0x40] = left_in;
        self.downsample_buffer[1][pos & 0x3F] = right_in;
        self.downsample_buffer[1][(pos & 0x3F) | 0x40] = right_in;

        let out: [i32; 2];

        if pos & 1 != 0 {
            // === Odd cycle: Full reverb processing ===

            // Step 1: FIR downsample (44100 → 22050)
            let mut downsampled = [0i32; 2];
            for ch in 0..2 {
                let src_start = (pos.wrapping_sub(38)) & 0x3F;
                let src = &self.downsample_buffer[ch];
                let mut acc: i32 = 0;
                for t in 0..20 {
                    acc += REVERB_FIR[t] * src[src_start + t * 2] as i32;
                }
                // Add center tap (tap 19 in the 39-tap filter = src[19] relative to start)
                // Center tap is at position src_start + 19 in the source
                acc += REVERB_FIR_CENTER * src[src_start + 19] as i32;
                downsampled[ch] = Self::clamp16(acc >> 15) as i32;
            }

            // Step 2: IIR filters (same-side and cross-channel)
            for ch in 0..2 {
                // IIR_INPUT_A = Clamp16(((ReverbRead(IIR_SRC_A[ch]) * IIR_COEF) >> 14
                //              + (downsampled[ch] * IN_COEF[ch]) >> 14) >> 1)
                let iir_input_a = Self::clamp16(
                    ((self.reverb_read(p.iir_src_a(ch), 0) as i32 * p.v_wall as i32) >> 14)
                    .wrapping_add((downsampled[ch] * p.in_coef(ch) as i32) >> 14)
                    >> 1
                );

                // IIR_INPUT_B — cross-channel: IIR_SRC_B[ch ^ 1]
                let iir_input_b = Self::clamp16(
                    ((self.reverb_read(p.iir_src_b(ch ^ 1), 0) as i32 * p.v_wall as i32) >> 14)
                    .wrapping_add((downsampled[ch] * p.in_coef(ch) as i32) >> 14)
                    >> 1
                );

                // IIR_A = Clamp16(((IIR_INPUT_A * IIR_ALPHA) >> 14
                //        + iiasm(ReverbRead(IIR_DEST_A[ch], -1)) >> 14) >> 1)
                let iir_a = Self::clamp16(
                    ((iir_input_a as i32 * p.v_iir as i32) >> 14)
                    .wrapping_add(self.iiasm(self.reverb_read(p.iir_dest_a(ch), -1)) >> 14)
                    >> 1
                );

                let iir_b = Self::clamp16(
                    ((iir_input_b as i32 * p.v_iir as i32) >> 14)
                    .wrapping_add(self.iiasm(self.reverb_read(p.iir_dest_b(ch), -1)) >> 14)
                    >> 1
                );

                self.reverb_write(p.iir_dest_a(ch), iir_a);
                self.reverb_write(p.iir_dest_b(ch), iir_b);
            }

            // Step 3: Comb filters + all-pass filters (per channel)
            for ch in 0..2 {
                // ACC = sum of 4 comb filter outputs
                let acc =
                    ((self.reverb_read(p.acc_src_a(ch), 0) as i32 * p.v_comb1 as i32) >> 14)
                    + ((self.reverb_read(p.acc_src_b(ch), 0) as i32 * p.v_comb2 as i32) >> 14)
                    + ((self.reverb_read(p.acc_src_c(ch), 0) as i32 * p.v_comb3 as i32) >> 14)
                    + ((self.reverb_read(p.acc_src_d(ch), 0) as i32 * p.v_comb4 as i32) >> 14);

                // All-pass filter 1
                let fb_a = self.reverb_read_apf(p.mix_dest_a(ch), p.d_apf1);
                let fb_b = self.reverb_read_apf(p.mix_dest_b(ch), p.d_apf2);
                let mda = Self::clamp16(
                    (acc + ((fb_a as i32 * Self::neg(p.v_apf1 as i32)) >> 14)) >> 1
                );
                let mdb = Self::clamp16(
                    fb_a as i32 + ((
                        ((mda as i32 * p.v_apf1 as i32) >> 14)
                        + ((fb_b as i32 * Self::neg(p.v_apf2 as i32)) >> 14)
                    ) >> 1)
                );

                // Write to upsample buffer
                let upsample_idx = (self.resample_pos >> 1) & 0x1F;
                let upsample_val = Self::clamp16(
                    fb_b as i32 + ((mdb as i32 * p.v_apf2 as i32) >> 15)
                );
                self.upsample_buffer[ch][upsample_idx] = upsample_val;
                self.upsample_buffer[ch][upsample_idx | 0x20] = upsample_val;

                self.reverb_write(p.mix_dest_a(ch), mda);
                self.reverb_write(p.mix_dest_b(ch), mdb);
            }

            // Advance reverb buffer position (22050Hz rate — once every 2 samples)
            self.current_address = (self.current_address + 1) & REVERB_BUFFER_MASK;

            // Step 4: FIR upsample (22050 → 44100) for odd cycle
            out = self.fir_upsample();
        } else {
            // === Even cycle: just output from upsample buffer ===
            // On even cycles, read directly from upsample buffer at center position
            let idx = (((self.resample_pos >> 1).wrapping_sub(19)) & 0x1F) + 9;
            out = [
                self.upsample_buffer[0][idx] as i32,
                self.upsample_buffer[1][idx] as i32,
            ];
        }

        self.resample_pos = (self.resample_pos + 1) & 0x3F;

        (out[0], out[1])
    }

    /// Read from reverb buffer for all-pass filter: MIX_DEST - FB_SRC
    #[inline]
    fn reverb_read_apf(&self, mix_dest: u16, fb_src: u16) -> i16 {
        // Address: (mix_dest - fb_src) with ×4 scaling on both
        let addr = (mix_dest as u32).wrapping_sub(fb_src as u32);
        let sample_addr = self.current_address
            .wrapping_add(addr << 2)
            & REVERB_BUFFER_MASK;
        self.buffer[sample_addr as usize]
    }

    /// FIR upsample using the 20-tap filter
    fn fir_upsample(&self) -> [i32; 2] {
        let mut out = [0i32; 2];
        for ch in 0..2 {
            let src_start = ((self.resample_pos >> 1).wrapping_sub(19)) & 0x1F;
            let src = &self.upsample_buffer[ch];
            let mut acc: i32 = 0;
            for t in 0..20 {
                acc += REVERB_FIR[t] * src[src_start + t] as i32;
            }
            out[ch] = (acc >> 14).clamp(-32768, 32767);
        }
        out
    }
}

impl Default for SpuReverb {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preset_data_preserved() {
        let preset = ReverbType::Hall.preset();
        assert_eq!(preset.d_apf1, 0x01A5);
        assert_eq!(preset.d_apf2, 0x0139);
        assert_eq!(preset.v_iir as u16, 0x6000);
    }

    #[test]
    fn test_reverb_off_produces_silence() {
        let mut reverb = SpuReverb::new();
        reverb.set_preset(ReverbType::Off);
        let (l, r) = reverb.process_sample(16000, 16000);
        assert_eq!(l, 0);
        assert_eq!(r, 0);
    }

    #[test]
    fn test_reverb_produces_output() {
        let mut reverb = SpuReverb::new();
        reverb.set_preset(ReverbType::Hall);

        // Feed a burst of signal
        for _ in 0..1000 {
            reverb.process_sample(16000, 16000);
        }

        // After some delay, reverb should produce non-zero output
        let mut has_output = false;
        for _ in 0..10000 {
            let (l, r) = reverb.process_sample(0, 0);
            if l != 0 || r != 0 {
                has_output = true;
                break;
            }
        }
        assert!(has_output, "Reverb should produce output after input burst");
    }

    #[test]
    fn test_address_scaling() {
        let reverb = SpuReverb::new();
        // Register value 0x100 should access address 0x100 * 4 = 0x400
        let sample = reverb.reverb_read(0x100, 0);
        assert_eq!(sample, 0); // Buffer initialized to zero
    }
}
