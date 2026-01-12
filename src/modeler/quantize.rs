//! PNG color quantization for PS1-style indexed textures
//!
//! Uses median-cut algorithm to reduce full-color images to indexed palettes.
//! Supports LAB color space, perceptual weighting, saturation bias, and more.
//! Generates both the IndexedTexture (palette indices) and Clut (color palette).

use crate::rasterizer::{Color15, ClutDepth, ClutId, Clut, IndexedTexture};

/// Result of quantizing an image
pub struct QuantizeResult {
    /// Indexed texture with palette indices
    pub texture: IndexedTexture,
    /// Generated CLUT (color palette)
    pub clut: Clut,
}

/// Color quantization strategy for median-cut algorithm
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub enum QuantizeMode {
    /// Split boxes by population (most pixels) - balanced results
    #[default]
    Standard,
    /// Split boxes by unique color count - preserves subtle color variations
    PreserveDetail,
    /// Split boxes by volume (color range) - smoother gradients
    Smooth,
}

/// Advanced quantization options
#[derive(Clone, Copy, Debug)]
pub struct QuantizeOptions {
    /// Quantization strategy (Standard, PreserveDetail, Smooth)
    pub mode: QuantizeMode,
    /// Use LAB color space for perceptually uniform quantization
    pub use_lab: bool,
    /// Denoise: reduce to 4-bit per channel before quantization (0 = off, 1 = on)
    /// Groups similar colors together, reducing noise in photographic images
    pub pre_quantize: u8,
    /// Perceptual weighting - weight green channel more (0.0 = none, 1.0 = full)
    pub perceptual_weight: f32,
    /// Saturation bias - prioritize saturated colors (0.0 = none, 1.0 = strong)
    pub saturation_bias: f32,
    /// Minimum bucket size as fraction of total pixels (0.0 = no minimum, 0.01 = 1%)
    /// Buckets smaller than this get merged with nearest neighbor
    pub min_bucket_fraction: f32,
}

impl Default for QuantizeOptions {
    fn default() -> Self {
        Self {
            mode: QuantizeMode::default(),
            use_lab: false,
            pre_quantize: 0,
            perceptual_weight: 0.0,
            saturation_bias: 0.0,
            min_bucket_fraction: 0.0,
        }
    }
}

impl QuantizeOptions {
    /// Create options from just a mode (for backwards compatibility)
    pub fn from_mode(mode: QuantizeMode) -> Self {
        Self {
            mode,
            ..Default::default()
        }
    }
}

// ============================================================================
// LAB Color Space
// ============================================================================

/// Color in LAB color space (perceptually uniform)
#[derive(Clone, Copy, Debug)]
struct LabColor {
    l: f32, // Lightness: 0-100
    a: f32, // Green-Red: roughly -128 to 128
    b: f32, // Blue-Yellow: roughly -128 to 128
}

impl LabColor {
    /// Convert from RGB (0-255 per channel) to LAB
    fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        // RGB to XYZ (assuming sRGB with D65 white point)
        let r_lin = srgb_to_linear(r as f32 / 255.0);
        let g_lin = srgb_to_linear(g as f32 / 255.0);
        let b_lin = srgb_to_linear(b as f32 / 255.0);

        // RGB to XYZ matrix (sRGB, D65)
        let x = r_lin * 0.4124564 + g_lin * 0.3575761 + b_lin * 0.1804375;
        let y = r_lin * 0.2126729 + g_lin * 0.7151522 + b_lin * 0.0721750;
        let z = r_lin * 0.0193339 + g_lin * 0.1191920 + b_lin * 0.9503041;

        // XYZ to LAB (D65 reference white)
        const REF_X: f32 = 0.95047;
        const REF_Y: f32 = 1.00000;
        const REF_Z: f32 = 1.08883;

        let fx = lab_f(x / REF_X);
        let fy = lab_f(y / REF_Y);
        let fz = lab_f(z / REF_Z);

        Self {
            l: 116.0 * fy - 16.0,
            a: 500.0 * (fx - fy),
            b: 200.0 * (fy - fz),
        }
    }

    /// Convert from Color15 (RGB555) to LAB
    fn from_color15(c: &Color15) -> Self {
        // Convert 5-bit to 8-bit
        let r = (c.r5() as u32 * 255 / 31) as u8;
        let g = (c.g5() as u32 * 255 / 31) as u8;
        let b = (c.b5() as u32 * 255 / 31) as u8;
        Self::from_rgb(r, g, b)
    }

    /// Convert LAB back to RGB (0-255 per channel)
    fn to_rgb(&self) -> (u8, u8, u8) {
        // LAB to XYZ
        const REF_X: f32 = 0.95047;
        const REF_Y: f32 = 1.00000;
        const REF_Z: f32 = 1.08883;

        let fy = (self.l + 16.0) / 116.0;
        let fx = self.a / 500.0 + fy;
        let fz = fy - self.b / 200.0;

        let x = REF_X * lab_f_inv(fx);
        let y = REF_Y * lab_f_inv(fy);
        let z = REF_Z * lab_f_inv(fz);

        // XYZ to RGB matrix (inverse of above)
        let r_lin = x *  3.2404542 + y * -1.5371385 + z * -0.4985314;
        let g_lin = x * -0.9692660 + y *  1.8760108 + z *  0.0415560;
        let b_lin = x *  0.0556434 + y * -0.2040259 + z *  1.0572252;

        // Linear to sRGB
        let r = (linear_to_srgb(r_lin) * 255.0).clamp(0.0, 255.0) as u8;
        let g = (linear_to_srgb(g_lin) * 255.0).clamp(0.0, 255.0) as u8;
        let b = (linear_to_srgb(b_lin) * 255.0).clamp(0.0, 255.0) as u8;

        (r, g, b)
    }

    /// Convert LAB to Color15
    fn to_color15(&self) -> Color15 {
        let (r, g, b) = self.to_rgb();
        Color15::from_rgb888(r, g, b)
    }

    /// Squared distance between two LAB colors (Delta E squared, simplified)
    fn distance_sq(&self, other: &LabColor) -> f32 {
        let dl = self.l - other.l;
        let da = self.a - other.a;
        let db = self.b - other.b;
        dl * dl + da * da + db * db
    }
}

/// sRGB gamma correction (to linear)
fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear to sRGB gamma correction
fn linear_to_srgb(v: f32) -> f32 {
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// LAB f function
fn lab_f(t: f32) -> f32 {
    const DELTA: f32 = 6.0 / 29.0;
    if t > DELTA * DELTA * DELTA {
        t.cbrt()
    } else {
        t / (3.0 * DELTA * DELTA) + 4.0 / 29.0
    }
}

/// LAB f inverse function
fn lab_f_inv(t: f32) -> f32 {
    const DELTA: f32 = 6.0 / 29.0;
    if t > DELTA {
        t * t * t
    } else {
        3.0 * DELTA * DELTA * (t - 4.0 / 29.0)
    }
}

// ============================================================================
// Internal Color Representation for Quantization
// ============================================================================

/// Internal color used during quantization (can be RGB or LAB)
#[derive(Clone, Copy, Debug)]
struct QColor {
    // Store both representations for flexibility
    c0: f32, // R or L
    c1: f32, // G or A
    c2: f32, // B or B
    // Original Color15 for final palette
    original: Color15,
    // Saturation (0-1) for bias calculations
    saturation: f32,
}

impl QColor {
    fn from_color15_rgb(c: Color15, opts: &QuantizeOptions) -> Self {
        let r = c.r5() as f32;
        let g = c.g5() as f32;
        let b = c.b5() as f32;

        // Calculate saturation
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let saturation = if max > 0.0 { (max - min) / max } else { 0.0 };

        // Apply perceptual weighting to green
        let g_weighted = g * (1.0 + opts.perceptual_weight * 0.5);

        Self {
            c0: r,
            c1: g_weighted,
            c2: b,
            original: c,
            saturation,
        }
    }

    fn from_color15_lab(c: Color15, opts: &QuantizeOptions) -> Self {
        let lab = LabColor::from_color15(&c);

        // Calculate saturation from original RGB
        let r = c.r5() as f32;
        let g = c.g5() as f32;
        let b = c.b5() as f32;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let saturation = if max > 0.0 { (max - min) / max } else { 0.0 };

        Self {
            c0: lab.l,
            c1: lab.a,
            c2: lab.b,
            original: c,
            saturation,
        }
    }

    /// Get weighted importance for bucket selection
    fn importance(&self, saturation_bias: f32) -> f32 {
        1.0 + self.saturation * saturation_bias
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Quantize an RGBA image to an indexed texture + CLUT
pub fn quantize_image(
    rgba_pixels: &[u8],
    width: usize,
    height: usize,
    depth: ClutDepth,
    name: &str,
) -> QuantizeResult {
    quantize_image_with_options(rgba_pixels, width, height, depth, name, &QuantizeOptions::default())
}

/// Quantize with just a mode (backwards compatible)
pub fn quantize_image_with_mode(
    rgba_pixels: &[u8],
    width: usize,
    height: usize,
    depth: ClutDepth,
    name: &str,
    mode: QuantizeMode,
) -> QuantizeResult {
    quantize_image_with_options(rgba_pixels, width, height, depth, name, &QuantizeOptions::from_mode(mode))
}

/// Quantize an RGBA image with full options
pub fn quantize_image_with_options(
    rgba_pixels: &[u8],
    width: usize,
    height: usize,
    depth: ClutDepth,
    name: &str,
    opts: &QuantizeOptions,
) -> QuantizeResult {
    let target_colors = depth.color_count();
    let total_pixels = width * height;

    // Step 1: Collect all non-transparent pixels as Color15
    let colors: Vec<Color15> = rgba_pixels
        .chunks(4)
        .filter(|p| p[3] > 0) // Skip fully transparent
        .map(|p| {
            // Apply denoise (4-bit per channel reduction)
            let (r, g, b) = if opts.pre_quantize > 0 {
                // Reduce to 4-bit per channel (16 levels) to group similar colors
                ((p[0] >> 4) << 4, (p[1] >> 4) << 4, (p[2] >> 4) << 4)
            } else {
                (p[0], p[1], p[2])
            };
            Color15::from_rgb888(r, g, b)
        })
        .collect();

    // Step 2: Use median cut to reduce to target_colors - 1 (reserve index 0 for transparent)
    let palette = if colors.is_empty() {
        vec![Color15::WHITE]
    } else {
        median_cut_advanced(&colors, target_colors.saturating_sub(1).max(1), total_pixels, opts)
    };

    // Step 3: Build CLUT (index 0 = transparent)
    let mut clut = Clut::new_empty(name, depth);
    clut.colors[0] = Color15::TRANSPARENT;
    for (i, color) in palette.iter().enumerate() {
        if i + 1 < clut.colors.len() {
            clut.colors[i + 1] = *color;
        }
    }

    // Step 4: Map each pixel to nearest palette index
    // If using LAB, do color matching in LAB space for better results
    let palette_lab: Vec<LabColor> = if opts.use_lab {
        palette.iter().map(|c| LabColor::from_color15(c)).collect()
    } else {
        vec![]
    };

    let mut indices = Vec::with_capacity(width * height);
    for chunk in rgba_pixels.chunks(4) {
        let index = if chunk[3] == 0 {
            0 // Transparent pixel -> index 0
        } else {
            let (r, g, b) = if opts.pre_quantize > 0 {
                ((chunk[0] >> 4) << 4, (chunk[1] >> 4) << 4, (chunk[2] >> 4) << 4)
            } else {
                (chunk[0], chunk[1], chunk[2])
            };

            if opts.use_lab && !palette_lab.is_empty() {
                let pixel_lab = LabColor::from_rgb(r, g, b);
                find_nearest_color_lab(&pixel_lab, &palette_lab) + 1
            } else {
                let pixel = Color15::from_rgb888(r, g, b);
                find_nearest_color(&pixel, &palette, opts.perceptual_weight) + 1
            }
        };
        indices.push(index);
    }

    let texture = IndexedTexture {
        width,
        height,
        depth,
        indices,
        default_clut: ClutId::NONE,
        name: name.to_string(),
    };

    QuantizeResult { texture, clut }
}

// ============================================================================
// Advanced Median Cut
// ============================================================================

/// Advanced median cut with all options
fn median_cut_advanced(
    colors: &[Color15],
    max_colors: usize,
    total_pixels: usize,
    opts: &QuantizeOptions,
) -> Vec<Color15> {
    if colors.is_empty() {
        return vec![Color15::WHITE];
    }

    // If we have fewer unique colors than max, just return them
    let mut unique: Vec<Color15> = colors.to_vec();
    unique.sort_by_key(|c| c.0);
    unique.dedup();
    if unique.len() <= max_colors {
        return unique;
    }

    // Convert to internal representation
    let qcolors: Vec<QColor> = colors.iter().map(|c| {
        if opts.use_lab {
            QColor::from_color15_lab(*c, opts)
        } else {
            QColor::from_color15_rgb(*c, opts)
        }
    }).collect();

    // Start with all colors in one bucket
    let mut buckets: Vec<Vec<QColor>> = vec![qcolors];

    // Calculate minimum bucket size threshold
    let min_bucket_size = (total_pixels as f32 * opts.min_bucket_fraction) as usize;

    // Recursively split until we have enough buckets
    while buckets.len() < max_colors {
        // Find bucket to split based on mode
        let split_idx = find_bucket_to_split(&buckets, opts, min_bucket_size);

        let Some(split_idx) = split_idx else {
            break; // No more buckets can be split
        };

        let bucket = buckets.remove(split_idx);
        if bucket.len() <= 1 {
            buckets.push(bucket);
            continue;
        }

        // Find axis with largest range (in QColor space)
        let (c0_range, c1_range, c2_range) = bucket_ranges_q(&bucket);
        let split_axis = if c0_range >= c1_range && c0_range >= c2_range {
            0
        } else if c1_range >= c2_range {
            1
        } else {
            2
        };

        // Sort by that axis and split at median
        let mut sorted = bucket;
        sorted.sort_by(|a, b| {
            let va = match split_axis { 0 => a.c0, 1 => a.c1, _ => a.c2 };
            let vb = match split_axis { 0 => b.c0, 1 => b.c1, _ => b.c2 };
            va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal)
        });

        let mid = sorted.len() / 2;
        let (left, right) = sorted.split_at(mid);

        if !left.is_empty() {
            buckets.push(left.to_vec());
        }
        if !right.is_empty() {
            buckets.push(right.to_vec());
        }
    }

    // Apply minimum bucket size filter - merge small buckets
    if min_bucket_size > 0 && buckets.len() > 1 {
        buckets = merge_small_buckets(buckets, min_bucket_size, opts);
    }

    // Compute average color for each bucket
    buckets.iter().map(|b| average_color_q(b, opts)).collect()
}

/// Find the best bucket to split based on mode and options
fn find_bucket_to_split(
    buckets: &[Vec<QColor>],
    opts: &QuantizeOptions,
    min_bucket_size: usize,
) -> Option<usize> {
    let candidates = buckets.iter().enumerate()
        .filter(|(_, b)| b.len() > 1 && b.len() > min_bucket_size && bucket_volume_q(b) > 0.0);

    match opts.mode {
        QuantizeMode::Standard => {
            // Split by population, weighted by saturation bias
            candidates
                .max_by(|(_, a), (_, b)| {
                    let score_a = bucket_weighted_size(a, opts.saturation_bias);
                    let score_b = bucket_weighted_size(b, opts.saturation_bias);
                    score_a.partial_cmp(&score_b).unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(i, _)| i)
        }
        QuantizeMode::PreserveDetail => {
            // Split by unique color count
            candidates
                .max_by_key(|(_, b)| bucket_unique_colors_q(b))
                .map(|(i, _)| i)
        }
        QuantizeMode::Smooth => {
            // Split by volume (largest color range)
            candidates
                .max_by(|(_, a), (_, b)| {
                    let vol_a = bucket_volume_q(a);
                    let vol_b = bucket_volume_q(b);
                    vol_a.partial_cmp(&vol_b).unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(i, _)| i)
        }
    }
}

/// Merge buckets smaller than threshold with their nearest neighbor
fn merge_small_buckets(
    mut buckets: Vec<Vec<QColor>>,
    min_size: usize,
    opts: &QuantizeOptions,
) -> Vec<Vec<QColor>> {
    loop {
        // Find smallest bucket below threshold
        let small_idx = buckets.iter().enumerate()
            .filter(|(_, b)| b.len() < min_size)
            .min_by_key(|(_, b)| b.len())
            .map(|(i, _)| i);

        let Some(small_idx) = small_idx else {
            break; // No more small buckets
        };

        if buckets.len() <= 1 {
            break; // Can't merge further
        }

        let small_bucket = buckets.remove(small_idx);
        let small_center = bucket_center(&small_bucket);

        // Find nearest bucket
        let nearest_idx = buckets.iter().enumerate()
            .min_by(|(_, a), (_, b)| {
                let center_a = bucket_center(a);
                let center_b = bucket_center(b);
                let dist_a = color_distance_q(&small_center, &center_a, opts);
                let dist_b = color_distance_q(&small_center, &center_b, opts);
                dist_a.partial_cmp(&dist_b).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
            .unwrap_or(0);

        // Merge
        buckets[nearest_idx].extend(small_bucket);
    }

    buckets
}

/// Get center of a bucket
fn bucket_center(bucket: &[QColor]) -> QColor {
    if bucket.is_empty() {
        return QColor {
            c0: 0.0, c1: 0.0, c2: 0.0,
            original: Color15::WHITE,
            saturation: 0.0,
        };
    }

    let n = bucket.len() as f32;
    let c0 = bucket.iter().map(|c| c.c0).sum::<f32>() / n;
    let c1 = bucket.iter().map(|c| c.c1).sum::<f32>() / n;
    let c2 = bucket.iter().map(|c| c.c2).sum::<f32>() / n;
    let sat = bucket.iter().map(|c| c.saturation).sum::<f32>() / n;

    QColor {
        c0, c1, c2,
        original: Color15::WHITE, // Placeholder
        saturation: sat,
    }
}

/// Distance between two QColors
fn color_distance_q(a: &QColor, b: &QColor, _opts: &QuantizeOptions) -> f32 {
    let d0 = a.c0 - b.c0;
    let d1 = a.c1 - b.c1;
    let d2 = a.c2 - b.c2;
    d0 * d0 + d1 * d1 + d2 * d2
}

// ============================================================================
// Bucket Operations for QColor
// ============================================================================

/// Calculate weighted bucket size (for saturation bias)
fn bucket_weighted_size(bucket: &[QColor], saturation_bias: f32) -> f32 {
    bucket.iter().map(|c| c.importance(saturation_bias)).sum()
}

/// Count unique colors in a QColor bucket
fn bucket_unique_colors_q(colors: &[QColor]) -> usize {
    let mut unique: Vec<u16> = colors.iter().map(|c| c.original.0).collect();
    unique.sort();
    unique.dedup();
    unique.len()
}

/// Calculate the volume of a QColor bucket
fn bucket_volume_q(colors: &[QColor]) -> f32 {
    if colors.is_empty() {
        return 0.0;
    }
    let (c0_range, c1_range, c2_range) = bucket_ranges_q(colors);
    c0_range * c1_range * c2_range
}

/// Calculate ranges for QColor bucket
fn bucket_ranges_q(colors: &[QColor]) -> (f32, f32, f32) {
    if colors.is_empty() {
        return (0.0, 0.0, 0.0);
    }

    let mut c0_min = f32::MAX;
    let mut c0_max = f32::MIN;
    let mut c1_min = f32::MAX;
    let mut c1_max = f32::MIN;
    let mut c2_min = f32::MAX;
    let mut c2_max = f32::MIN;

    for c in colors {
        c0_min = c0_min.min(c.c0);
        c0_max = c0_max.max(c.c0);
        c1_min = c1_min.min(c.c1);
        c1_max = c1_max.max(c.c1);
        c2_min = c2_min.min(c.c2);
        c2_max = c2_max.max(c.c2);
    }

    (c0_max - c0_min, c1_max - c1_min, c2_max - c2_min)
}

/// Calculate average color of a QColor bucket, return as Color15
fn average_color_q(colors: &[QColor], opts: &QuantizeOptions) -> Color15 {
    if colors.is_empty() {
        return Color15::WHITE;
    }

    if opts.use_lab {
        // Average in LAB space, convert back
        let n = colors.len() as f32;
        let l = colors.iter().map(|c| c.c0).sum::<f32>() / n;
        let a = colors.iter().map(|c| c.c1).sum::<f32>() / n;
        let b = colors.iter().map(|c| c.c2).sum::<f32>() / n;
        LabColor { l, a, b }.to_color15()
    } else {
        // Average in RGB space (from original Color15)
        let (mut r_sum, mut g_sum, mut b_sum) = (0u32, 0u32, 0u32);
        for c in colors {
            r_sum += c.original.r5() as u32;
            g_sum += c.original.g5() as u32;
            b_sum += c.original.b5() as u32;
        }
        let n = colors.len() as u32;
        Color15::new(
            (r_sum / n) as u8,
            (g_sum / n) as u8,
            (b_sum / n) as u8,
        )
    }
}

// ============================================================================
// Color Matching
// ============================================================================

/// Find nearest color in palette (RGB space with optional perceptual weight)
fn find_nearest_color(target: &Color15, palette: &[Color15], perceptual_weight: f32) -> u8 {
    if palette.is_empty() {
        return 0;
    }

    let mut best_idx = 0u8;
    let mut best_dist = f32::MAX;

    // Green weight: 1.0 (no extra) to 2.0 (full perceptual)
    let g_weight = 1.0 + perceptual_weight;

    for (i, color) in palette.iter().enumerate() {
        let dr = (target.r5() as f32 - color.r5() as f32).abs();
        let dg = (target.g5() as f32 - color.g5() as f32).abs();
        let db = (target.b5() as f32 - color.b5() as f32).abs();

        let dist = dr * dr + dg * dg * g_weight + db * db;

        if dist < best_dist {
            best_dist = dist;
            best_idx = i as u8;
        }

        if dist == 0.0 {
            break;
        }
    }

    best_idx
}

/// Find nearest color in palette (LAB space)
fn find_nearest_color_lab(target: &LabColor, palette: &[LabColor]) -> u8 {
    if palette.is_empty() {
        return 0;
    }

    let mut best_idx = 0u8;
    let mut best_dist = f32::MAX;

    for (i, color) in palette.iter().enumerate() {
        let dist = target.distance_sq(color);

        if dist < best_dist {
            best_dist = dist;
            best_idx = i as u8;
        }

        if dist == 0.0 {
            break;
        }
    }

    best_idx
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Count unique colors in an RGBA pixel array (ignoring transparency)
pub fn count_unique_colors(rgba_pixels: &[u8]) -> usize {
    use std::collections::HashSet;

    let mut unique_colors: HashSet<u16> = HashSet::new();

    for chunk in rgba_pixels.chunks(4) {
        if chunk[3] == 0 {
            continue;
        }
        let r5 = chunk[0] >> 3;
        let g5 = chunk[1] >> 3;
        let b5 = chunk[2] >> 3;
        let packed = ((r5 as u16) << 10) | ((g5 as u16) << 5) | (b5 as u16);
        unique_colors.insert(packed);
    }

    unique_colors.len()
}

/// Determine optimal CLUT depth based on unique color count
pub fn optimal_clut_depth(unique_colors: usize) -> ClutDepth {
    if unique_colors <= 15 {
        ClutDepth::Bpp4
    } else {
        ClutDepth::Bpp8
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantize_simple() {
        let rgba = vec![
            255, 0, 0, 255,    // Red
            0, 255, 0, 255,    // Green
            0, 0, 255, 255,    // Blue
            255, 255, 0, 255,  // Yellow
        ];

        let result = quantize_image(&rgba, 2, 2, ClutDepth::Bpp4, "Test");

        assert_eq!(result.texture.width, 2);
        assert_eq!(result.texture.height, 2);
        assert_eq!(result.texture.indices.len(), 4);

        for idx in &result.texture.indices {
            assert!(*idx > 0, "Non-transparent pixel should have index > 0");
        }
    }

    #[test]
    fn test_quantize_with_transparency() {
        let rgba = vec![
            255, 0, 0, 255,    // Red
            0, 255, 0, 255,    // Green
            0, 0, 255, 255,    // Blue
            0, 0, 0, 0,        // Transparent
        ];

        let result = quantize_image(&rgba, 2, 2, ClutDepth::Bpp4, "Test");

        assert_eq!(result.texture.indices[3], 0);
        assert!(result.clut.colors[0].is_transparent());
    }

    #[test]
    fn test_lab_conversion_roundtrip() {
        // Test that LAB conversion roundtrips reasonably well
        let colors = [
            (255, 0, 0),   // Red
            (0, 255, 0),   // Green
            (0, 0, 255),   // Blue
            (128, 128, 128), // Gray
            (255, 255, 255), // White
            (0, 0, 0),     // Black
        ];

        for (r, g, b) in colors {
            let lab = LabColor::from_rgb(r, g, b);
            let (r2, g2, b2) = lab.to_rgb();
            // Allow small rounding errors
            assert!((r as i32 - r2 as i32).abs() <= 2, "Red mismatch: {} vs {}", r, r2);
            assert!((g as i32 - g2 as i32).abs() <= 2, "Green mismatch: {} vs {}", g, g2);
            assert!((b as i32 - b2 as i32).abs() <= 2, "Blue mismatch: {} vs {}", b, b2);
        }
    }

    #[test]
    fn test_quantize_with_lab() {
        let rgba = vec![
            255, 0, 0, 255,
            0, 255, 0, 255,
            0, 0, 255, 255,
            255, 255, 0, 255,
        ];

        let opts = QuantizeOptions {
            use_lab: true,
            ..Default::default()
        };

        let result = quantize_image_with_options(&rgba, 2, 2, ClutDepth::Bpp4, "Test", &opts);

        assert_eq!(result.texture.indices.len(), 4);
        for idx in &result.texture.indices {
            assert!(*idx > 0);
        }
    }
}
