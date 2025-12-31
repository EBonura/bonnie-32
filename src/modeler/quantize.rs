//! PNG color quantization for PS1-style indexed textures
//!
//! Uses median-cut algorithm to reduce full-color images to indexed palettes.
//! Generates both the IndexedTexture (palette indices) and Clut (color palette).

use crate::rasterizer::{Color15, ClutDepth, ClutId, Clut, IndexedTexture};

/// Result of quantizing an image
pub struct QuantizeResult {
    /// Indexed texture with palette indices
    pub texture: IndexedTexture,
    /// Generated CLUT (color palette)
    pub clut: Clut,
}

/// Quantize an RGBA image to an indexed texture + CLUT
///
/// # Arguments
/// * `rgba_pixels` - RGBA pixel data, 4 bytes per pixel
/// * `width` - Image width
/// * `height` - Image height
/// * `depth` - Target CLUT depth (4-bit = 16 colors, 8-bit = 256 colors)
/// * `name` - Name for the resulting CLUT
///
/// # Returns
/// QuantizeResult containing the indexed texture and generated CLUT
pub fn quantize_image(
    rgba_pixels: &[u8],
    width: usize,
    height: usize,
    depth: ClutDepth,
    name: &str,
) -> QuantizeResult {
    let target_colors = depth.color_count();

    // Step 1: Collect all non-transparent pixels as Color15
    let colors: Vec<Color15> = rgba_pixels
        .chunks(4)
        .filter(|p| p[3] > 0) // Skip fully transparent
        .map(|p| Color15::from_rgb888(p[0], p[1], p[2]))
        .collect();

    // Step 2: Use median cut to reduce to target_colors - 1 (reserve index 0 for transparent)
    let palette = if colors.is_empty() {
        // No colors found, just use white
        vec![Color15::WHITE]
    } else {
        median_cut(&colors, target_colors.saturating_sub(1).max(1))
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
    let mut indices = Vec::with_capacity(width * height);
    for chunk in rgba_pixels.chunks(4) {
        let index = if chunk[3] == 0 {
            0 // Transparent pixel -> index 0
        } else {
            let pixel = Color15::from_rgb888(chunk[0], chunk[1], chunk[2]);
            find_nearest_color(&pixel, &palette) + 1 // +1 because index 0 is transparent
        };
        indices.push(index);
    }

    let texture = IndexedTexture {
        width,
        height,
        depth,
        indices,
        default_clut: ClutId::NONE, // Will be assigned when added to pool
        name: name.to_string(),
    };

    QuantizeResult { texture, clut }
}

/// Median cut color quantization
///
/// Recursively divides the color space by splitting along the axis with
/// the largest range until we have the desired number of buckets.
fn median_cut(colors: &[Color15], max_colors: usize) -> Vec<Color15> {
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

    // Start with all colors in one bucket
    let mut buckets: Vec<Vec<Color15>> = vec![colors.to_vec()];

    // Recursively split until we have enough buckets
    while buckets.len() < max_colors {
        // Find bucket with largest volume (color range)
        let (split_idx, max_volume) = buckets
            .iter()
            .enumerate()
            .map(|(i, b)| (i, bucket_volume(b)))
            .max_by_key(|(_, v)| *v)
            .unwrap_or((0, 0));

        // If the largest bucket has no volume, stop
        if max_volume == 0 {
            break;
        }

        let bucket = buckets.remove(split_idx);
        if bucket.len() <= 1 {
            buckets.push(bucket);
            continue;
        }

        // Find axis with largest range
        let (r_range, g_range, b_range) = bucket_ranges(&bucket);
        let split_axis = if r_range >= g_range && r_range >= b_range {
            0 // Red
        } else if g_range >= b_range {
            1 // Green
        } else {
            2 // Blue
        };

        // Sort by that axis and split at median
        let mut sorted = bucket;
        sorted.sort_by_key(|c| match split_axis {
            0 => c.r5(),
            1 => c.g5(),
            _ => c.b5(),
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

    // Compute average color for each bucket
    buckets.iter().map(|b| average_color(b)).collect()
}

/// Calculate the volume (range) of a color bucket
fn bucket_volume(colors: &[Color15]) -> u32 {
    if colors.is_empty() {
        return 0;
    }
    let (r_range, g_range, b_range) = bucket_ranges(colors);
    r_range as u32 * g_range as u32 * b_range as u32
}

/// Calculate the range of each channel in a color bucket
fn bucket_ranges(colors: &[Color15]) -> (u8, u8, u8) {
    if colors.is_empty() {
        return (0, 0, 0);
    }

    let (mut r_min, mut r_max) = (31u8, 0u8);
    let (mut g_min, mut g_max) = (31u8, 0u8);
    let (mut b_min, mut b_max) = (31u8, 0u8);

    for c in colors {
        r_min = r_min.min(c.r5());
        r_max = r_max.max(c.r5());
        g_min = g_min.min(c.g5());
        g_max = g_max.max(c.g5());
        b_min = b_min.min(c.b5());
        b_max = b_max.max(c.b5());
    }

    (
        r_max.saturating_sub(r_min),
        g_max.saturating_sub(g_min),
        b_max.saturating_sub(b_min),
    )
}

/// Calculate the average color of a bucket
fn average_color(colors: &[Color15]) -> Color15 {
    if colors.is_empty() {
        return Color15::WHITE;
    }

    let (mut r_sum, mut g_sum, mut b_sum) = (0u32, 0u32, 0u32);
    for c in colors {
        r_sum += c.r5() as u32;
        g_sum += c.g5() as u32;
        b_sum += c.b5() as u32;
    }

    let n = colors.len() as u32;
    Color15::new(
        (r_sum / n) as u8,
        (g_sum / n) as u8,
        (b_sum / n) as u8,
    )
}

/// Find the index of the nearest color in the palette
fn find_nearest_color(target: &Color15, palette: &[Color15]) -> u8 {
    if palette.is_empty() {
        return 0;
    }

    let mut best_idx = 0u8;
    let mut best_dist = u32::MAX;

    for (i, color) in palette.iter().enumerate() {
        // Calculate squared distance in RGB555 space
        let dr = (target.r5() as i32 - color.r5() as i32).abs() as u32;
        let dg = (target.g5() as i32 - color.g5() as i32).abs() as u32;
        let db = (target.b5() as i32 - color.b5() as i32).abs() as u32;

        // Weighted distance (green is more perceptually important)
        let dist = dr * dr + dg * dg * 2 + db * db;

        if dist < best_dist {
            best_dist = dist;
            best_idx = i as u8;
        }

        // Early exit for exact match
        if dist == 0 {
            break;
        }
    }

    best_idx
}

/// Count unique colors in an RGBA pixel array (ignoring transparency)
/// Returns the number of unique RGB555 colors (not counting fully transparent pixels)
/// Colors are counted in RGB555 space since that's what PS1 uses
pub fn count_unique_colors(rgba_pixels: &[u8]) -> usize {
    use std::collections::HashSet;

    let mut unique_colors: HashSet<u16> = HashSet::new();

    for chunk in rgba_pixels.chunks(4) {
        // Skip fully transparent pixels
        if chunk[3] == 0 {
            continue;
        }
        // Convert to RGB555 and pack (this matches Color15::from_rgb888)
        let r5 = chunk[0] >> 3;
        let g5 = chunk[1] >> 3;
        let b5 = chunk[2] >> 3;
        let packed = ((r5 as u16) << 10) | ((g5 as u16) << 5) | (b5 as u16);
        unique_colors.insert(packed);
    }

    unique_colors.len()
}

/// Count unique colors in a TextureAtlas (convenience wrapper)
/// Colors are counted in RGB555 space since that's what PS1 uses
pub fn count_atlas_colors(atlas: &super::mesh_editor::TextureAtlas) -> usize {
    use std::collections::HashSet;

    let mut unique_colors: HashSet<u16> = HashSet::new();

    for y in 0..atlas.height {
        for x in 0..atlas.width {
            let color = atlas.get_pixel(x, y);
            // Skip transparent pixels
            if color.blend == crate::rasterizer::BlendMode::Erase {
                continue;
            }
            // Convert to RGB555 and pack (matches Color15::from_rgb888)
            let r5 = color.r >> 3;
            let g5 = color.g >> 3;
            let b5 = color.b >> 3;
            let packed = ((r5 as u16) << 10) | ((g5 as u16) << 5) | (b5 as u16);
            unique_colors.insert(packed);
        }
    }

    unique_colors.len()
}

/// Determine optimal CLUT depth based on unique color count
/// Returns Bpp4 (16 colors) if <= 15 unique colors (index 0 reserved for transparent)
/// Returns Bpp8 (256 colors) otherwise
pub fn optimal_clut_depth(unique_colors: usize) -> ClutDepth {
    // Reserve index 0 for transparent, so we can fit up to 15 colors in 4-bit
    if unique_colors <= 15 {
        ClutDepth::Bpp4
    } else {
        ClutDepth::Bpp8
    }
}

/// Auto-quantize a TextureAtlas, automatically choosing optimal CLUT depth
/// Returns (IndexedAtlas, Clut, detected_color_count)
pub fn quantize_atlas_auto(
    atlas: &super::mesh_editor::TextureAtlas,
    name: &str,
) -> (super::mesh_editor::IndexedAtlas, Clut, usize) {
    let color_count = count_atlas_colors(atlas);
    let depth = optimal_clut_depth(color_count);
    let (indexed, clut) = quantize_atlas(atlas, depth, name);
    (indexed, clut, color_count)
}

/// Quantize a TextureAtlas to an IndexedAtlas + Clut
///
/// Convenience function for converting existing RGBA atlases to indexed format.
pub fn quantize_atlas(
    atlas: &super::mesh_editor::TextureAtlas,
    depth: ClutDepth,
    name: &str,
) -> (super::mesh_editor::IndexedAtlas, Clut) {
    // Extract RGBA pixels from atlas
    let mut rgba_pixels = Vec::with_capacity(atlas.width * atlas.height * 4);
    for y in 0..atlas.height {
        for x in 0..atlas.width {
            let color = atlas.get_pixel(x, y);
            rgba_pixels.push(color.r);
            rgba_pixels.push(color.g);
            rgba_pixels.push(color.b);
            // Map BlendMode::Erase to transparent, otherwise opaque
            let alpha = if color.blend == crate::rasterizer::BlendMode::Erase {
                0
            } else {
                255
            };
            rgba_pixels.push(alpha);
        }
    }

    let result = quantize_image(&rgba_pixels, atlas.width, atlas.height, depth, name);

    let indexed_atlas = super::mesh_editor::IndexedAtlas {
        width: atlas.width,
        height: atlas.height,
        depth,
        indices: result.texture.indices,
        default_clut: ClutId::NONE,
    };

    (indexed_atlas, result.clut)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantize_simple() {
        // Create a simple 2x2 image with 4 different colors
        let rgba = vec![
            255, 0, 0, 255,    // Red
            0, 255, 0, 255,    // Green
            0, 0, 255, 255,    // Blue
            255, 255, 0, 255,  // Yellow
        ];

        let result = quantize_image(&rgba, 2, 2, ClutDepth::Bpp4, "Test");

        // Should have 4 unique colors + 1 transparent slot
        assert_eq!(result.texture.width, 2);
        assert_eq!(result.texture.height, 2);
        assert_eq!(result.texture.indices.len(), 4);

        // All indices should be non-zero (not transparent)
        for idx in &result.texture.indices {
            assert!(*idx > 0, "Non-transparent pixel should have index > 0");
        }
    }

    #[test]
    fn test_quantize_with_transparency() {
        // Create a 2x2 image with one transparent pixel
        let rgba = vec![
            255, 0, 0, 255,    // Red
            0, 255, 0, 255,    // Green
            0, 0, 255, 255,    // Blue
            0, 0, 0, 0,        // Transparent
        ];

        let result = quantize_image(&rgba, 2, 2, ClutDepth::Bpp4, "Test");

        // Last pixel should map to index 0 (transparent)
        assert_eq!(result.texture.indices[3], 0);

        // CLUT index 0 should be transparent
        assert!(result.clut.colors[0].is_transparent());
    }

    #[test]
    fn test_find_nearest_color() {
        let palette = vec![
            Color15::new(0, 0, 0),   // Black
            Color15::new(31, 0, 0),  // Red
            Color15::new(0, 31, 0),  // Green
            Color15::new(0, 0, 31),  // Blue
        ];

        // Exact match
        assert_eq!(find_nearest_color(&Color15::new(31, 0, 0), &palette), 1);

        // Near red should match red
        assert_eq!(find_nearest_color(&Color15::new(28, 2, 2), &palette), 1);

        // Near green should match green
        assert_eq!(find_nearest_color(&Color15::new(2, 28, 2), &palette), 2);
    }
}
