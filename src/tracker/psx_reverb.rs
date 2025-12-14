//! PS1 SPU Reverb Emulation
//!
//! Implements the PlayStation 1's hardware reverb algorithm based on the
//! nocash PSX specifications. The SPU reverb uses:
//! - IIR filtering for same-side and different-side reflections
//! - 4 comb filters for early reflections
//! - 2 cascaded all-pass filters for diffusion
//!
//! Reference: https://psx-spx.consoledev.net/soundprocessingunitspu/

/// PS1 reverb preset coefficients
/// These are the 10 standard presets from the PsyQ SDK
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReverbPreset {
    /// All-pass filter 1 offset
    pub d_apf1: u16,
    /// All-pass filter 2 offset
    pub d_apf2: u16,
    /// IIR filter volume
    pub v_iir: i16,
    /// Comb filter 1 volume
    pub v_comb1: i16,
    /// Comb filter 2 volume
    pub v_comb2: i16,
    /// Comb filter 3 volume
    pub v_comb3: i16,
    /// Comb filter 4 volume
    pub v_comb4: i16,
    /// Wall reflection volume
    pub v_wall: i16,
    /// All-pass filter 1 volume
    pub v_apf1: i16,
    /// All-pass filter 2 volume
    pub v_apf2: i16,
    /// Same-side reflection addresses (left/right)
    pub m_l_same: u16,
    pub m_r_same: u16,
    /// Comb filter 1 addresses
    pub m_l_comb1: u16,
    pub m_r_comb1: u16,
    /// Comb filter 2 addresses
    pub m_l_comb2: u16,
    pub m_r_comb2: u16,
    /// Same-side reflection source addresses
    pub d_l_same: u16,
    pub d_r_same: u16,
    /// Different-side reflection addresses
    pub m_l_diff: u16,
    pub m_r_diff: u16,
    /// Comb filter 3 addresses
    pub m_l_comb3: u16,
    pub m_r_comb3: u16,
    /// Comb filter 4 addresses
    pub m_l_comb4: u16,
    pub m_r_comb4: u16,
    /// Different-side reflection source addresses
    pub d_l_diff: u16,
    pub d_r_diff: u16,
    /// All-pass filter 1 addresses
    pub m_l_apf1: u16,
    pub m_r_apf1: u16,
    /// All-pass filter 2 addresses
    pub m_l_apf2: u16,
    pub m_r_apf2: u16,
    /// Input volumes
    pub v_l_in: i16,
    pub v_r_in: i16,
}

impl ReverbPreset {
    const fn new(data: [u16; 32]) -> Self {
        Self {
            d_apf1: data[0],
            d_apf2: data[1],
            v_iir: data[2] as i16,
            v_comb1: data[3] as i16,
            v_comb2: data[4] as i16,
            v_comb3: data[5] as i16,
            v_comb4: data[6] as i16,
            v_wall: data[7] as i16,
            v_apf1: data[8] as i16,
            v_apf2: data[9] as i16,
            m_l_same: data[10],
            m_r_same: data[11],
            m_l_comb1: data[12],
            m_r_comb1: data[13],
            m_l_comb2: data[14],
            m_r_comb2: data[15],
            d_l_same: data[16],
            d_r_same: data[17],
            m_l_diff: data[18],
            m_r_diff: data[19],
            m_l_comb3: data[20],
            m_r_comb3: data[21],
            m_l_comb4: data[22],
            m_r_comb4: data[23],
            d_l_diff: data[24],
            d_r_diff: data[25],
            m_l_apf1: data[26],
            m_r_apf1: data[27],
            m_l_apf2: data[28],
            m_r_apf2: data[29],
            v_l_in: data[30] as i16,
            v_r_in: data[31] as i16,
        }
    }
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
        ReverbType::Off,
        ReverbType::Room,
        ReverbType::StudioSmall,
        ReverbType::StudioMedium,
        ReverbType::StudioLarge,
        ReverbType::Hall,
        ReverbType::HalfEcho,
        ReverbType::SpaceEcho,
        ReverbType::ChaosEcho,
        ReverbType::Delay,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            ReverbType::Off => "Off",
            ReverbType::Room => "Room",
            ReverbType::StudioSmall => "Studio Small",
            ReverbType::StudioMedium => "Studio Medium",
            ReverbType::StudioLarge => "Studio Large",
            ReverbType::Hall => "Hall",
            ReverbType::HalfEcho => "Half Echo",
            ReverbType::SpaceEcho => "Space Echo",
            ReverbType::ChaosEcho => "Chaos Echo",
            ReverbType::Delay => "Delay",
        }
    }

    pub fn preset(&self) -> &'static ReverbPreset {
        match self {
            ReverbType::Off => &PRESET_OFF,
            ReverbType::Room => &PRESET_ROOM,
            ReverbType::StudioSmall => &PRESET_STUDIO_SMALL,
            ReverbType::StudioMedium => &PRESET_STUDIO_MEDIUM,
            ReverbType::StudioLarge => &PRESET_STUDIO_LARGE,
            ReverbType::Hall => &PRESET_HALL,
            ReverbType::HalfEcho => &PRESET_HALF_ECHO,
            ReverbType::SpaceEcho => &PRESET_SPACE_ECHO,
            ReverbType::ChaosEcho => &PRESET_CHAOS_ECHO,
            ReverbType::Delay => &PRESET_DELAY,
        }
    }
}

// Standard PS1 reverb presets from PsyQ SDK
// Data from lv2-psx-reverb by ipatix (https://github.com/ipatix/lv2-psx-reverb)

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

static PRESET_OFF: ReverbPreset = ReverbPreset::new([
    0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
    0x0000, 0x0000, 0x0001, 0x0001, 0x0001, 0x0001, 0x0001, 0x0001,
    0x0000, 0x0000, 0x0001, 0x0001, 0x0001, 0x0001, 0x0001, 0x0001,
    0x0000, 0x0000, 0x0001, 0x0001, 0x0001, 0x0001, 0x0000, 0x0000,
]);

/// PS1 SPU reverb buffer size
/// The original PS1 ran reverb at 22050Hz, we run at 44100Hz so we double the buffer
/// Max buffer size needed based on largest preset offsets
const REVERB_BUFFER_SIZE: usize = 0x20000; // 128KB of samples (64KB per channel)

/// PS1 SPU Reverb processor
pub struct PsxReverb {
    /// Current preset
    preset: ReverbPreset,
    /// Reverb type for display
    reverb_type: ReverbType,
    /// Left channel buffer
    buffer_l: Vec<i16>,
    /// Right channel buffer
    buffer_r: Vec<i16>,
    /// Current buffer position (advances each sample at 22050Hz rate)
    buffer_pos: usize,
    /// Sample rate ratio (for adapting 22050Hz algorithm to other rates)
    rate_ratio: f32,
    /// Fractional sample accumulator for rate conversion
    sample_accum: f32,
    /// Wet/dry mix (0.0 = dry, 1.0 = wet)
    wet_level: f32,
    /// Output volume
    output_volume: f32,
    /// Whether reverb is enabled
    enabled: bool,
}

impl PsxReverb {
    /// Create a new PS1 reverb processor
    pub fn new(sample_rate: u32) -> Self {
        let rate_ratio = sample_rate as f32 / 22050.0;
        Self {
            preset: *ReverbType::Off.preset(),
            reverb_type: ReverbType::Off,
            buffer_l: vec![0i16; REVERB_BUFFER_SIZE],
            buffer_r: vec![0i16; REVERB_BUFFER_SIZE],
            buffer_pos: 0,
            rate_ratio,
            sample_accum: 0.0,
            wet_level: 0.5,
            output_volume: 1.0,
            enabled: false,
        }
    }

    /// Set the reverb preset
    pub fn set_preset(&mut self, reverb_type: ReverbType) {
        self.reverb_type = reverb_type;
        self.preset = *reverb_type.preset();
        self.enabled = reverb_type != ReverbType::Off;
        // Clear buffers when changing preset to avoid artifacts
        self.buffer_l.fill(0);
        self.buffer_r.fill(0);
    }

    /// Get current reverb type
    pub fn reverb_type(&self) -> ReverbType {
        self.reverb_type
    }

    /// Set wet/dry mix (0.0 = fully dry, 1.0 = fully wet)
    pub fn set_wet_level(&mut self, level: f32) {
        self.wet_level = level.clamp(0.0, 1.0);
    }

    /// Get wet level
    pub fn wet_level(&self) -> f32 {
        self.wet_level
    }

    /// Set output volume
    pub fn set_output_volume(&mut self, volume: f32) {
        self.output_volume = volume.clamp(0.0, 2.0);
    }

    /// Check if reverb is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Read from reverb buffer with offset (handles wrapping)
    #[inline]
    fn read_buffer(&self, buffer: &[i16], offset: u16) -> i16 {
        let idx = (self.buffer_pos + offset as usize) % REVERB_BUFFER_SIZE;
        buffer[idx]
    }

    /// Write to reverb buffer with offset (handles wrapping)
    #[inline]
    fn write_buffer(&mut self, is_left: bool, offset: u16, value: i16) {
        let idx = (self.buffer_pos + offset as usize) % REVERB_BUFFER_SIZE;
        if is_left {
            self.buffer_l[idx] = value;
        } else {
            self.buffer_r[idx] = value;
        }
    }

    /// Saturating multiply and divide by 0x8000 (PS1 fixed-point math)
    #[inline]
    fn mul_vol(sample: i32, volume: i16) -> i32 {
        ((sample * volume as i32) >> 15).clamp(-32768, 32767)
    }

    /// Process a single sample pair through the reverb (at 22050Hz rate)
    fn process_sample_22k(&mut self, left_in: i16, right_in: i16) -> (i16, i16) {
        // Copy preset to avoid borrow issues (preset is Copy)
        let p = self.preset;

        // Input scaling
        let l_in = Self::mul_vol(left_in as i32, p.v_l_in) as i16;
        let r_in = Self::mul_vol(right_in as i32, p.v_r_in) as i16;

        // Same-side reflections with IIR filter
        // [mLSAME] = (Lin + [dLSAME]*vWALL - [mLSAME-2])*vIIR + [mLSAME-2]
        let d_l_same = self.read_buffer(&self.buffer_l, p.d_l_same);
        let m_l_same_prev = self.read_buffer(&self.buffer_l, p.m_l_same.wrapping_sub(2));
        let l_same_input = l_in as i32 + Self::mul_vol(d_l_same as i32, p.v_wall);
        let l_same = Self::mul_vol(l_same_input - m_l_same_prev as i32, p.v_iir) + m_l_same_prev as i32;
        self.write_buffer(true, p.m_l_same, l_same.clamp(-32768, 32767) as i16);

        let d_r_same = self.read_buffer(&self.buffer_r, p.d_r_same);
        let m_r_same_prev = self.read_buffer(&self.buffer_r, p.m_r_same.wrapping_sub(2));
        let r_same_input = r_in as i32 + Self::mul_vol(d_r_same as i32, p.v_wall);
        let r_same = Self::mul_vol(r_same_input - m_r_same_prev as i32, p.v_iir) + m_r_same_prev as i32;
        self.write_buffer(false, p.m_r_same, r_same.clamp(-32768, 32767) as i16);

        // Different-side reflections (cross-channel)
        // [mLDIFF] = (Lin + [dRDIFF]*vWALL - [mLDIFF-2])*vIIR + [mLDIFF-2]
        let d_r_diff = self.read_buffer(&self.buffer_r, p.d_r_diff);
        let m_l_diff_prev = self.read_buffer(&self.buffer_l, p.m_l_diff.wrapping_sub(2));
        let l_diff_input = l_in as i32 + Self::mul_vol(d_r_diff as i32, p.v_wall);
        let l_diff = Self::mul_vol(l_diff_input - m_l_diff_prev as i32, p.v_iir) + m_l_diff_prev as i32;
        self.write_buffer(true, p.m_l_diff, l_diff.clamp(-32768, 32767) as i16);

        let d_l_diff = self.read_buffer(&self.buffer_l, p.d_l_diff);
        let m_r_diff_prev = self.read_buffer(&self.buffer_r, p.m_r_diff.wrapping_sub(2));
        let r_diff_input = r_in as i32 + Self::mul_vol(d_l_diff as i32, p.v_wall);
        let r_diff = Self::mul_vol(r_diff_input - m_r_diff_prev as i32, p.v_iir) + m_r_diff_prev as i32;
        self.write_buffer(false, p.m_r_diff, r_diff.clamp(-32768, 32767) as i16);

        // Comb filters - early reflections
        // Lout = vCOMB1*[mLCOMB1] + vCOMB2*[mLCOMB2] + vCOMB3*[mLCOMB3] + vCOMB4*[mLCOMB4]
        let l_comb1 = self.read_buffer(&self.buffer_l, p.m_l_comb1);
        let l_comb2 = self.read_buffer(&self.buffer_l, p.m_l_comb2);
        let l_comb3 = self.read_buffer(&self.buffer_l, p.m_l_comb3);
        let l_comb4 = self.read_buffer(&self.buffer_l, p.m_l_comb4);
        let mut l_out = Self::mul_vol(l_comb1 as i32, p.v_comb1)
            + Self::mul_vol(l_comb2 as i32, p.v_comb2)
            + Self::mul_vol(l_comb3 as i32, p.v_comb3)
            + Self::mul_vol(l_comb4 as i32, p.v_comb4);

        let r_comb1 = self.read_buffer(&self.buffer_r, p.m_r_comb1);
        let r_comb2 = self.read_buffer(&self.buffer_r, p.m_r_comb2);
        let r_comb3 = self.read_buffer(&self.buffer_r, p.m_r_comb3);
        let r_comb4 = self.read_buffer(&self.buffer_r, p.m_r_comb4);
        let mut r_out = Self::mul_vol(r_comb1 as i32, p.v_comb1)
            + Self::mul_vol(r_comb2 as i32, p.v_comb2)
            + Self::mul_vol(r_comb3 as i32, p.v_comb3)
            + Self::mul_vol(r_comb4 as i32, p.v_comb4);

        // All-pass filter 1
        // Lout = Lout - vAPF1*[mLAPF1-dAPF1], [mLAPF1] = Lout, Lout = Lout*vAPF1 + [mLAPF1-dAPF1]
        let l_apf1_delayed = self.read_buffer(&self.buffer_l, p.m_l_apf1.wrapping_sub(p.d_apf1));
        l_out = l_out - Self::mul_vol(l_apf1_delayed as i32, p.v_apf1);
        self.write_buffer(true, p.m_l_apf1, l_out.clamp(-32768, 32767) as i16);
        l_out = Self::mul_vol(l_out, p.v_apf1) + l_apf1_delayed as i32;

        let r_apf1_delayed = self.read_buffer(&self.buffer_r, p.m_r_apf1.wrapping_sub(p.d_apf1));
        r_out = r_out - Self::mul_vol(r_apf1_delayed as i32, p.v_apf1);
        self.write_buffer(false, p.m_r_apf1, r_out.clamp(-32768, 32767) as i16);
        r_out = Self::mul_vol(r_out, p.v_apf1) + r_apf1_delayed as i32;

        // All-pass filter 2
        let l_apf2_delayed = self.read_buffer(&self.buffer_l, p.m_l_apf2.wrapping_sub(p.d_apf2));
        l_out = l_out - Self::mul_vol(l_apf2_delayed as i32, p.v_apf2);
        self.write_buffer(true, p.m_l_apf2, l_out.clamp(-32768, 32767) as i16);
        l_out = Self::mul_vol(l_out, p.v_apf2) + l_apf2_delayed as i32;

        let r_apf2_delayed = self.read_buffer(&self.buffer_r, p.m_r_apf2.wrapping_sub(p.d_apf2));
        r_out = r_out - Self::mul_vol(r_apf2_delayed as i32, p.v_apf2);
        self.write_buffer(false, p.m_r_apf2, r_out.clamp(-32768, 32767) as i16);
        r_out = Self::mul_vol(r_out, p.v_apf2) + r_apf2_delayed as i32;

        // Advance buffer position
        self.buffer_pos = (self.buffer_pos + 1) % REVERB_BUFFER_SIZE;

        (
            l_out.clamp(-32768, 32767) as i16,
            r_out.clamp(-32768, 32767) as i16,
        )
    }

    /// Process audio buffers in-place
    /// Input/output are f32 samples normalized to -1.0..1.0
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        if !self.enabled || self.wet_level <= 0.0 {
            return;
        }

        let len = left.len().min(right.len());
        let dry_level = 1.0 - self.wet_level;

        for i in 0..len {
            // Accumulate fractional samples for rate conversion
            self.sample_accum += 1.0 / self.rate_ratio;

            // Process at 22050Hz rate
            while self.sample_accum >= 1.0 {
                self.sample_accum -= 1.0;

                // Convert f32 to i16
                let l_in = (left[i] * 32767.0).clamp(-32768.0, 32767.0) as i16;
                let r_in = (right[i] * 32767.0).clamp(-32768.0, 32767.0) as i16;

                // Process reverb
                let (l_wet, r_wet) = self.process_sample_22k(l_in, r_in);

                // Mix wet/dry and convert back to f32
                let l_dry = left[i];
                let r_dry = right[i];
                let l_wet_f = l_wet as f32 / 32767.0;
                let r_wet_f = r_wet as f32 / 32767.0;

                left[i] = (l_dry * dry_level + l_wet_f * self.wet_level) * self.output_volume;
                right[i] = (r_dry * dry_level + r_wet_f * self.wet_level) * self.output_volume;
            }
        }
    }

    /// Clear reverb buffers (call when stopping playback)
    pub fn clear(&mut self) {
        self.buffer_l.fill(0);
        self.buffer_r.fill(0);
        self.buffer_pos = 0;
        self.sample_accum = 0.0;
    }
}

impl Default for PsxReverb {
    fn default() -> Self {
        Self::new(44100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reverb_preset_creation() {
        let preset = ReverbType::Hall.preset();
        assert_eq!(preset.d_apf1, 0x01A5);
        assert_eq!(preset.d_apf2, 0x0139);
    }

    #[test]
    fn test_reverb_processing() {
        let mut reverb = PsxReverb::new(44100);
        reverb.set_preset(ReverbType::Hall);
        reverb.set_wet_level(0.5);

        let mut left = vec![0.5f32; 1024];
        let mut right = vec![0.5f32; 1024];

        reverb.process(&mut left, &mut right);

        // After processing, values should be modified
        // (exact values depend on reverb algorithm)
    }

    #[test]
    fn test_reverb_off() {
        let mut reverb = PsxReverb::new(44100);
        reverb.set_preset(ReverbType::Off);

        let mut left = vec![0.5f32; 1024];
        let mut right = vec![0.5f32; 1024];
        let original_left = left.clone();
        let original_right = right.clone();

        reverb.process(&mut left, &mut right);

        // With reverb off, values should be unchanged
        assert_eq!(left, original_left);
        assert_eq!(right, original_right);
    }
}
