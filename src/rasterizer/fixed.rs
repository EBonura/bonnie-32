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

    /// PS1 GTE-style division using UNR (Unsigned Newton-Raphson) approximation
    /// This is intentionally less accurate than proper division - that's the point!
    /// The inaccuracy causes the characteristic PS1 geometry wobble.
    #[inline]
    pub fn div_unr(self, divisor: Self) -> Self {
        if divisor.0 == 0 {
            return Fixed32(0);
        }

        // The PS1 GTE uses a lookup table + Newton-Raphson iteration
        // We simulate this by:
        // 1. Using integer division (loses precision)
        // 2. Truncating intermediate results (accumulates error)

        let dividend = (self.0 as i64) << FRAC_BITS;
        let result = dividend / divisor.0 as i64;

        // Truncate to 16-bit range then extend back (simulates GTE's 16-bit internal ops)
        let truncated = (result as i32).clamp(-32768 * ONE_32, 32767 * ONE_32);

        // Additional precision loss: mask off lowest bits (simulates UNR error)
        // The PS1's UNR has ~2-3 bits of error in the result
        Fixed32(truncated & !0x7) // Lose bottom 3 bits
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

    /// Create from IVec3 directly (no float conversion!)
    /// IVec3 uses INT_SCALE=4, Fixed32 uses 4.12 format (4096=1.0)
    /// Conversion: multiply by 1024 (= 4096/4)
    #[inline]
    pub fn from_ivec3(v: super::IVec3) -> Self {
        const IVEC_TO_FIXED: i32 = 1024; // 4096 / INT_SCALE
        Self {
            x: Fixed32(v.x * IVEC_TO_FIXED),
            y: Fixed32(v.y * IVEC_TO_FIXED),
            z: Fixed32(v.z * IVEC_TO_FIXED),
        }
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
/// This is the first stage where precision loss begins
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
    FixedVec3::new(
        rel.dot(bx),
        rel.dot(by),
        rel.dot(bz),
    )
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

/// Transform IVec3 to camera space directly (no float conversion!)
/// Camera position and basis vectors are still float (they come from the editor camera)
pub fn transform_to_camera_space_int(
    world_pos: super::IVec3,
    camera_pos: super::Vec3,
    basis_x: super::Vec3,
    basis_y: super::Vec3,
    basis_z: super::Vec3,
) -> FixedVec3 {
    // Convert world pos directly from integer to fixed-point (no float!)
    let world_fixed = FixedVec3::from_ivec3(world_pos);
    // Camera is still float (editor camera position)
    let cam_fixed = FixedVec3::from_vec3(camera_pos);
    let rel = world_fixed - cam_fixed;

    let bx = FixedVec3::from_vec3(basis_x);
    let by = FixedVec3::from_vec3(basis_y);
    let bz = FixedVec3::from_vec3(basis_z);

    FixedVec3::new(
        rel.dot(bx),
        rel.dot(by),
        rel.dot(bz),
    )
}

/// Complete PS1-style vertex transformation from integer coordinates
/// Takes IVec3 world position, returns integer screen coordinates
/// This is the optimal path for mesh editor: IVec3 → Fixed32 → screen integers
pub fn project_fixed_int(
    world_pos: super::IVec3,
    camera_pos: super::Vec3,
    basis_x: super::Vec3,
    basis_y: super::Vec3,
    basis_z: super::Vec3,
    width: usize,
    height: usize,
) -> (i32, i32, f32) {
    // Transform to camera space (integer → fixed-point, no float conversion!)
    let cam_pos = transform_to_camera_space_int(world_pos, camera_pos, basis_x, basis_y, basis_z);

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
// PS1-style Sin/Cos Lookup Tables for Integer Rotation
// =============================================================================

/// Number of entries in sin/cos tables (full rotation = 4096 entries)
pub const TRIG_TABLE_SIZE: usize = 4096;

/// Fixed-point scale for trig values (4096 = 1.0)
pub const TRIG_SCALE: i32 = 4096;

/// Pre-computed sin table (4096 entries, scaled by 4096)
/// Index 0 = 0°, Index 1024 = 90°, Index 2048 = 180°, Index 3072 = 270°
pub static SIN_TABLE: [i32; TRIG_TABLE_SIZE] = generate_sin_table();

/// Pre-computed cos table (4096 entries, scaled by 4096)
pub static COS_TABLE: [i32; TRIG_TABLE_SIZE] = generate_cos_table();

/// Generate sin lookup table at compile time
const fn generate_sin_table() -> [i32; TRIG_TABLE_SIZE] {
    let mut table = [0i32; TRIG_TABLE_SIZE];
    let mut i = 0;
    while i < TRIG_TABLE_SIZE {
        // angle in radians = i * 2π / 4096
        // We use a Taylor series approximation since std::f64::sin isn't const
        let angle = (i as f64) * 2.0 * 3.14159265358979323846 / (TRIG_TABLE_SIZE as f64);
        // sin(x) ≈ x - x³/6 + x⁵/120 - x⁷/5040 + x⁹/362880
        // Normalize angle to [-π, π] for better convergence
        let x = normalize_angle(angle);
        let sin_val = taylor_sin(x);
        table[i] = (sin_val * TRIG_SCALE as f64) as i32;
        i += 1;
    }
    table
}

/// Generate cos lookup table at compile time
const fn generate_cos_table() -> [i32; TRIG_TABLE_SIZE] {
    let mut table = [0i32; TRIG_TABLE_SIZE];
    let mut i = 0;
    while i < TRIG_TABLE_SIZE {
        let angle = (i as f64) * 2.0 * 3.14159265358979323846 / (TRIG_TABLE_SIZE as f64);
        let x = normalize_angle(angle);
        let cos_val = taylor_cos(x);
        table[i] = (cos_val * TRIG_SCALE as f64) as i32;
        i += 1;
    }
    table
}

/// Normalize angle to [-π, π] for Taylor series convergence
const fn normalize_angle(angle: f64) -> f64 {
    const PI: f64 = 3.14159265358979323846;
    const TWO_PI: f64 = 2.0 * PI;
    let mut a = angle;
    while a > PI {
        a -= TWO_PI;
    }
    while a < -PI {
        a += TWO_PI;
    }
    a
}

/// Taylor series approximation for sin (const fn compatible)
const fn taylor_sin(x: f64) -> f64 {
    let x2 = x * x;
    let x3 = x2 * x;
    let x5 = x3 * x2;
    let x7 = x5 * x2;
    let x9 = x7 * x2;
    let x11 = x9 * x2;
    x - x3 / 6.0 + x5 / 120.0 - x7 / 5040.0 + x9 / 362880.0 - x11 / 39916800.0
}

/// Taylor series approximation for cos (const fn compatible)
const fn taylor_cos(x: f64) -> f64 {
    let x2 = x * x;
    let x4 = x2 * x2;
    let x6 = x4 * x2;
    let x8 = x6 * x2;
    let x10 = x8 * x2;
    1.0 - x2 / 2.0 + x4 / 24.0 - x6 / 720.0 + x8 / 40320.0 - x10 / 3628800.0
}

/// Get fixed-point sin value for angle (0-4095 = 0°-360°)
/// Returns value scaled by 4096 (so 4096 = 1.0, -4096 = -1.0)
#[inline]
pub fn fixed_sin(angle: u16) -> i32 {
    SIN_TABLE[(angle as usize) & 0xFFF]
}

/// Get fixed-point cos value for angle (0-4095 = 0°-360°)
/// Returns value scaled by 4096 (so 4096 = 1.0, -4096 = -1.0)
#[inline]
pub fn fixed_cos(angle: u16) -> i32 {
    COS_TABLE[(angle as usize) & 0xFFF]
}

/// Convert degrees to table index (0-4095)
#[inline]
pub fn degrees_to_angle(degrees: f32) -> u16 {
    let normalized = degrees.rem_euclid(360.0);
    ((normalized / 360.0) * TRIG_TABLE_SIZE as f32) as u16
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
        // UNR division should be close but not exact
        let a = Fixed32::from_f32(10.0);
        let b = Fixed32::from_f32(3.0);
        let result = a.div_unr(b);
        let expected = 10.0 / 3.0;

        // Should be close but not perfect
        let error = (result.to_f32() - expected).abs();
        assert!(error < 0.1); // Within 0.1
        // But NOT perfect
        assert!(error > 0.0001); // Has some error
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

    #[test]
    fn test_fixed_sin_cos_key_angles() {
        // sin(0°) = 0
        assert_eq!(fixed_sin(0), 0);
        // sin(90°) = 4096 (index 1024)
        assert!((fixed_sin(1024) - 4096).abs() <= 1);
        // sin(180°) = 0 (index 2048)
        assert!(fixed_sin(2048).abs() <= 1);
        // sin(270°) = -4096 (index 3072)
        assert!((fixed_sin(3072) + 4096).abs() <= 1);

        // cos(0°) = 4096
        assert!((fixed_cos(0) - 4096).abs() <= 1);
        // cos(90°) = 0 (index 1024)
        assert!(fixed_cos(1024).abs() <= 1);
        // cos(180°) = -4096 (index 2048)
        assert!((fixed_cos(2048) + 4096).abs() <= 1);
        // cos(270°) = 0 (index 3072)
        assert!(fixed_cos(3072).abs() <= 1);
    }

    #[test]
    fn test_degrees_to_angle() {
        assert_eq!(degrees_to_angle(0.0), 0);
        assert_eq!(degrees_to_angle(90.0), 1024);
        assert_eq!(degrees_to_angle(180.0), 2048);
        assert_eq!(degrees_to_angle(270.0), 3072);
        // Wrap around
        assert_eq!(degrees_to_angle(360.0), 0);
        assert_eq!(degrees_to_angle(450.0), 1024); // 450 = 90
    }
}
