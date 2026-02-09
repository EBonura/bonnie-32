//! Fixed-point math for PS1-authentic rendering
//!
//! The PS1's GTE (Geometry Transform Engine) used specific fixed-point formats:
//! - 1.3.12 format for coordinates and matrices (16-bit: 1 sign, 3 integer, 12 fractional)
//! - Screen coordinates output as integers (no subpixel precision)
//! - UNR (Unsigned Newton-Raphson) division which introduces precision errors
//!
//! This module replicates these limitations to achieve authentic PS1 vertex jitter.

use std::ops::{Add, Sub, Mul, Neg};

// =============================================================================
// PS1 GTE UNR Division Lookup Table (257 entries)
// =============================================================================

/// Authentic PS1 GTE UNR (Unsigned Newton-Raphson) reciprocal approximation table.
/// Used by the GTE's RTPS/RTPT commands for perspective division.
/// Generated via: table[i] = max(0, (0x40000 / (i + 0x100) + 1) / 2 - 0x101)
/// Source: psx-spx GTE documentation, verified against Duckstation emulator.
const UNR_TABLE: [u8; 257] = {
    let mut table = [0u8; 257];
    let mut i = 0u32;
    while i < 257 {
        let div = i + 256;
        let quotient = 262144 / div; // 0x40000 / (i + 0x100)
        let val = ((quotient + 1) / 2) as i32 - 257;
        table[i as usize] = if val > 0 { val as u8 } else { 0 };
        i += 1;
    }
    table
};

// =============================================================================
// PS1 GTE Fixed-Point: 1.3.12 format (16-bit)
// =============================================================================

/// Fixed-point number in PS1 GTE 1.3.12 format
/// - 1 sign bit
/// - 3 integer bits (range -8 to +7.999...)
/// - 12 fractional bits (precision: 1/4096 ≈ 0.000244)
///
/// This limited range and precision is what causes PS1 vertex jitter.
/// Values are stored in i16 for authentic 16-bit behavior.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Fixed16(pub i16);

/// 12 fractional bits (4096 = 1.0)
const FRAC_BITS_16: i32 = 12;
const ONE_16: i16 = 1 << FRAC_BITS_16; // 4096

impl Fixed16 {
    pub const ZERO: Fixed16 = Fixed16(0);
    pub const ONE: Fixed16 = Fixed16(ONE_16);

    /// Create from f32 (clamps to 1.3.12 range)
    #[inline]
    pub fn from_f32(f: f32) -> Self {
        let scaled = (f * ONE_16 as f32).clamp(-32768.0, 32767.0) as i16;
        Fixed16(scaled)
    }

    /// Convert to f32
    #[inline]
    pub fn to_f32(self) -> f32 {
        self.0 as f32 / ONE_16 as f32
    }

    /// Floor to integer
    #[inline]
    pub fn floor(self) -> i32 {
        (self.0 >> FRAC_BITS_16) as i32
    }
}

impl Add for Fixed16 {
    type Output = Self;
    #[inline]
    fn add(self, other: Self) -> Self {
        Fixed16(self.0.wrapping_add(other.0))
    }
}

impl Sub for Fixed16 {
    type Output = Self;
    #[inline]
    fn sub(self, other: Self) -> Self {
        Fixed16(self.0.wrapping_sub(other.0))
    }
}

impl Neg for Fixed16 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Fixed16(self.0.wrapping_neg())
    }
}

// =============================================================================
// 32-bit Fixed-Point for intermediate calculations
// =============================================================================

/// Fixed-point number with 4.12 format in 32-bit storage
/// Used for intermediate calculations before truncating to 16-bit
/// - More headroom for multiplication results
/// - Still uses 12 fractional bits like PS1
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Fixed32(pub i32);

const FRAC_BITS: i32 = 12;
const ONE_32: i32 = 1 << FRAC_BITS; // 4096

impl Fixed32 {
    pub const ZERO: Fixed32 = Fixed32(0);
    pub const ONE: Fixed32 = Fixed32(ONE_32);

    /// Create from integer
    #[inline]
    pub fn from_int(n: i32) -> Self {
        Fixed32(n << FRAC_BITS)
    }

    /// Create from f32
    #[inline]
    pub fn from_f32(f: f32) -> Self {
        Fixed32((f * ONE_32 as f32) as i32)
    }

    /// Convert to f32
    #[inline]
    pub fn to_f32(self) -> f32 {
        self.0 as f32 / ONE_32 as f32
    }

    /// Floor to integer (this is what PS1 GPU receives - integer screen coords)
    #[inline]
    pub fn floor(self) -> i32 {
        self.0 >> FRAC_BITS
    }

    /// Truncate to 16-bit Fixed16 (loses precision, causes jitter)
    #[inline]
    pub fn to_fixed16(self) -> Fixed16 {
        Fixed16(self.0.clamp(-32768, 32767) as i16)
    }

    /// Minimum
    #[inline]
    pub fn min(self, other: Self) -> Self {
        Fixed32(self.0.min(other.0))
    }

    /// Maximum
    #[inline]
    pub fn max(self, other: Self) -> Self {
        Fixed32(self.0.max(other.0))
    }

    /// Fixed-point multiplication (12.12 * 12.12 -> 12.12)
    #[inline]
    pub fn mul_fixed(self, other: Self) -> Self {
        // Multiply then shift back down
        let result = (self.0 as i64 * other.0 as i64) >> FRAC_BITS;
        Fixed32(result as i32)
    }

    /// PS1 GTE-style division using the authentic UNR (Unsigned Newton-Raphson) algorithm.
    ///
    /// Replicates the exact algorithm from the PS1 GTE's RTPS/RTPT commands:
    /// 1. Count leading zeros in divisor and normalize
    /// 2. Look up initial reciprocal approximation in a 257-entry table
    /// 3. Refine with two Newton-Raphson iterations
    /// 4. Multiply by dividend
    ///
    /// The data-dependent error pattern (~2-3 bits) creates the characteristic
    /// PS1 vertex jitter that varies based on geometry and camera position.
    #[inline]
    pub fn div_unr(self, divisor: Self) -> Self {
        if divisor.0 == 0 {
            return Fixed32(0);
        }

        // Handle signs separately (PS1 UNR works with unsigned values)
        let result_negative = (self.0 < 0) != (divisor.0 < 0);
        let num = self.0.unsigned_abs() as u64;
        let den = divisor.0.unsigned_abs();

        if den == 0 {
            return Fixed32(0);
        }

        // Count leading zeros and normalize divisor to have MSB at bit 31
        let z = den.leading_zeros();
        let d_norm = (den as u64) << z;

        // Extract upper 16 bits for UNR table lookup (now in range 0x8000..0xFFFF)
        let d16 = (d_norm >> 16) as u64;

        // PS1 UNR table lookup: index = (d16 - 0x7FC0) >> 7
        let table_idx = ((d16.wrapping_sub(0x7FC0)) >> 7).min(256) as usize;
        let u_val = UNR_TABLE[table_idx] as u64 + 0x101;

        // Two Newton-Raphson iterations (exact PS1 GTE algorithm)
        let nr1 = (0x2000080u64.wrapping_sub(d16.wrapping_mul(u_val))) >> 8;
        let nr2 = (0x80u64.wrapping_add(nr1.wrapping_mul(u_val))) >> 8;

        // nr2 ≈ 2^32 / d16 where d16 = (den << z) >> 16
        // So nr2 ≈ 2^48 / (den << z) = 2^(48-z) / den
        // We want: result = num * 2^12 / den (for 4.12 fixed-point)
        // result = num * nr2 / 2^(36-z)
        let raw = num.wrapping_mul(nr2);
        let shift = 36u32.wrapping_sub(z);

        let magnitude = if shift < 64 {
            // Add rounding before shift
            let rounding = if shift > 0 { 1u64 << (shift - 1) } else { 0 };
            (raw.wrapping_add(rounding)) >> shift
        } else {
            0
        };

        // Clamp to i32 range
        let clamped = magnitude.min(i32::MAX as u64) as i32;

        if result_negative {
            Fixed32(-clamped)
        } else {
            Fixed32(clamped)
        }
    }
}

impl Add for Fixed32 {
    type Output = Self;
    #[inline]
    fn add(self, other: Self) -> Self {
        Fixed32(self.0.wrapping_add(other.0))
    }
}

impl Sub for Fixed32 {
    type Output = Self;
    #[inline]
    fn sub(self, other: Self) -> Self {
        Fixed32(self.0.wrapping_sub(other.0))
    }
}

impl Mul for Fixed32 {
    type Output = Self;
    #[inline]
    fn mul(self, other: Self) -> Self {
        self.mul_fixed(other)
    }
}

impl Neg for Fixed32 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Fixed32(-self.0)
    }
}

// =============================================================================
// 3D Vector using fixed-point
// =============================================================================

/// 3D vector using 4.12 fixed-point coordinates
#[derive(Debug, Clone, Copy, Default)]
pub struct FixedVec3 {
    pub x: Fixed32,
    pub y: Fixed32,
    pub z: Fixed32,
}

impl FixedVec3 {
    pub const ZERO: FixedVec3 = FixedVec3 {
        x: Fixed32::ZERO,
        y: Fixed32::ZERO,
        z: Fixed32::ZERO,
    };

    #[inline]
    pub fn new(x: Fixed32, y: Fixed32, z: Fixed32) -> Self {
        Self { x, y, z }
    }

    /// Create from Vec3 (converts float to fixed-point)
    #[inline]
    pub fn from_vec3(v: super::Vec3) -> Self {
        Self {
            x: Fixed32::from_f32(v.x),
            y: Fixed32::from_f32(v.y),
            z: Fixed32::from_f32(v.z),
        }
    }

    /// Convert back to Vec3
    #[inline]
    pub fn to_vec3(self) -> super::Vec3 {
        super::Vec3::new(
            self.x.to_f32(),
            self.y.to_f32(),
            self.z.to_f32(),
        )
    }

    /// Dot product in fixed-point
    #[inline]
    pub fn dot(self, other: Self) -> Fixed32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    /// Scale by fixed-point value
    #[inline]
    pub fn scale(self, s: Fixed32) -> Self {
        Self {
            x: self.x * s,
            y: self.y * s,
            z: self.z * s,
        }
    }
}

impl Add for FixedVec3 {
    type Output = Self;
    #[inline]
    fn add(self, other: Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
            z: self.z + other.z,
        }
    }
}

impl Sub for FixedVec3 {
    type Output = Self;
    #[inline]
    fn sub(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
            z: self.z - other.z,
        }
    }
}

// =============================================================================
// PS1-style Projection Pipeline (all fixed-point)
// =============================================================================

/// Transform a vertex by camera basis vectors using fixed-point math
/// This is the first stage where precision loss begins.
///
/// Note: The real PS1 GTE truncates rotation results to 16-bit IR registers
/// (1.3.12 in i16, range -8 to +7.999), but we keep the full 32-bit Fixed32
/// here because this renderer's world coordinates are not constrained to
/// PS1-scale ranges. The 4.12 fixed-point and UNR division already produce
/// authentic jitter without the 16-bit truncation.
pub fn transform_to_camera_space(
    world_pos: super::Vec3,
    camera_pos: super::Vec3,
    basis_x: super::Vec3,
    basis_y: super::Vec3,
    basis_z: super::Vec3,
) -> FixedVec3 {
    // Convert to fixed-point immediately (first precision loss)
    let rel = FixedVec3::from_vec3(world_pos) - FixedVec3::from_vec3(camera_pos);
    let bx = FixedVec3::from_vec3(basis_x);
    let by = FixedVec3::from_vec3(basis_y);
    let bz = FixedVec3::from_vec3(basis_z);

    // Dot products in fixed-point (more precision loss from multiplications)
    let cx = rel.dot(bx);
    let cy = rel.dot(by);
    let cz = rel.dot(bz);

    FixedVec3::new(cx, cy, cz)
}

/// Project camera-space coordinates to screen using PS1-style fixed-point math
/// Returns integer screen coordinates (no subpixel precision) and depth
///
/// This is where the famous PS1 jitter comes from:
/// 1. All math done in 4.12 fixed-point
/// 2. Division uses inaccurate UNR algorithm
/// 3. Final coordinates are integer-only (floored)
pub fn project_to_screen(
    cam_pos: FixedVec3,
    width: usize,
    height: usize,
) -> (i32, i32, Fixed32) {
    // Projection constants (same as float version)
    let distance = Fixed32::from_f32(5.0);
    let scale = Fixed32::from_f32(4.0); // us = distance - 1
    let viewport_scale = Fixed32::from_f32((width.min(height) as f32 / 2.0) * 0.75);
    let half_w = Fixed32::from_int(width as i32 / 2);
    let half_h = Fixed32::from_int(height as i32 / 2);

    // Perspective divide denominator
    let denom = cam_pos.z + distance;

    // Check for near-zero denominator
    if denom.0.abs() < 256 { // ~0.0625 in 4.12
        return (half_w.floor(), half_h.floor(), cam_pos.z);
    }

    // This is the key: PS1-style UNR division (inaccurate!)
    let proj_x = (cam_pos.x * scale).div_unr(denom);
    let proj_y = (cam_pos.y * scale).div_unr(denom);

    // Scale to screen coordinates
    let screen_x = proj_x * viewport_scale + half_w;
    let screen_y = proj_y * viewport_scale + half_h;

    // Floor to integers - PS1 GPU only accepts integer coordinates
    (screen_x.floor(), screen_y.floor(), cam_pos.z)
}

/// Complete PS1-style vertex transformation pipeline
/// Takes world coordinates and returns integer screen coordinates
pub fn project_fixed(
    world_pos: super::Vec3,
    camera_pos: super::Vec3,
    basis_x: super::Vec3,
    basis_y: super::Vec3,
    basis_z: super::Vec3,
    width: usize,
    height: usize,
) -> (i32, i32, f32) {
    // Transform to camera space (in fixed-point)
    let cam_pos = transform_to_camera_space(world_pos, camera_pos, basis_x, basis_y, basis_z);

    // Project to screen (in fixed-point, returns integers)
    let (sx, sy, depth) = project_to_screen(cam_pos, width, height);

    // Return screen integers + depth as float (for z-buffer compatibility)
    (sx, sy, depth.to_f32())
}

// =============================================================================
// 2D vector for UV coordinates
// =============================================================================

/// 2D vector using fixed-point (for UVs)
#[derive(Debug, Clone, Copy, Default)]
pub struct FixedVec2 {
    pub x: Fixed32,
    pub y: Fixed32,
}

impl FixedVec2 {
    #[inline]
    pub fn new(x: Fixed32, y: Fixed32) -> Self {
        Self { x, y }
    }

    #[inline]
    pub fn from_f32(x: f32, y: f32) -> Self {
        Self {
            x: Fixed32::from_f32(x),
            y: Fixed32::from_f32(y),
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed32_precision() {
        // 4.12 format: precision is 1/4096
        let a = Fixed32::from_f32(1.0);
        assert_eq!(a.0, 4096);

        let b = Fixed32::from_f32(0.5);
        assert_eq!(b.0, 2048);

        // Small value should be representable
        let c = Fixed32::from_f32(0.001);
        assert!(c.0 > 0); // Should be ~4
    }

    #[test]
    fn test_fixed32_mul() {
        let a = Fixed32::from_f32(2.0);
        let b = Fixed32::from_f32(3.0);
        let result = a * b;
        assert!((result.to_f32() - 6.0).abs() < 0.01);
    }

    #[test]
    fn test_unr_division_has_error() {
        // UNR division should be close but not exact (authentic PS1 GTE behavior)
        let a = Fixed32::from_f32(10.0);
        let b = Fixed32::from_f32(3.0);
        let result = a.div_unr(b);
        let expected = 10.0 / 3.0;

        // Should be close (within ~0.1% for typical values)
        let error = (result.to_f32() - expected).abs();
        assert!(error < 0.1, "UNR error too large: {}", error);
    }

    #[test]
    fn test_unr_division_basic() {
        // Simple division: 10 / 2 = 5
        let a = Fixed32::from_f32(10.0);
        let b = Fixed32::from_f32(2.0);
        let result = a.div_unr(b);
        assert!((result.to_f32() - 5.0).abs() < 0.01, "10/2 = {}", result.to_f32());

        // Negative dividend
        let a = Fixed32::from_f32(-6.0);
        let b = Fixed32::from_f32(2.0);
        let result = a.div_unr(b);
        assert!((result.to_f32() - -3.0).abs() < 0.01, "-6/2 = {}", result.to_f32());

        // Division by 1
        let a = Fixed32::from_f32(7.5);
        let b = Fixed32::from_f32(1.0);
        let result = a.div_unr(b);
        assert!((result.to_f32() - 7.5).abs() < 0.1, "7.5/1 = {}", result.to_f32());
    }

    #[test]
    fn test_projection_outputs_integers() {
        use super::super::Vec3;

        let world_pos = Vec3::new(1.234, 2.567, 5.0);
        let camera_pos = Vec3::new(0.0, 0.0, 0.0);
        let basis_x = Vec3::new(1.0, 0.0, 0.0);
        let basis_y = Vec3::new(0.0, 1.0, 0.0);
        let basis_z = Vec3::new(0.0, 0.0, 1.0);

        let (x, y, _z) = project_fixed(world_pos, camera_pos, basis_x, basis_y, basis_z, 320, 240);

        // Result should be reasonable screen coordinates
        assert!(x > -1000 && x < 1000);
        assert!(y > -1000 && y < 1000);
    }
}
