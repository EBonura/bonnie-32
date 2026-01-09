//! Core types for the rasterizer

use super::math::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

// =============================================================================
// PS1 RGB555 Color Type
// =============================================================================

/// PS1-authentic 15-bit color with semi-transparency bit
///
/// Format: `sRRRRRGG GGGBBBBB` (big-endian for clarity)
/// - Bit 15 (s): Semi-transparency flag (1 = use face's blend mode, 0 = write directly)
/// - Bits 14-10: Red (0-31)
/// - Bits 9-5: Green (0-31)
/// - Bits 4-0: Blue (0-31)
///
/// Special value: 0x0000 = fully transparent (not drawn, like CLUT transparency)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Color15(pub u16);

impl Color15 {
    /// Fully transparent pixel - will not be drawn (acts as color key)
    pub const TRANSPARENT: Color15 = Color15(0x0000);

    /// Opaque black (bit 15 = 0, so no blending)
    /// Note: To get drawable black, use BLACK_DRAWABLE which sets bit 15
    pub const BLACK: Color15 = Color15(0x0000);

    /// Drawable black (bit 15 set so it's not treated as transparent)
    pub const BLACK_DRAWABLE: Color15 = Color15(0x8000);

    /// Opaque white
    pub const WHITE: Color15 = Color15(0x7FFF);

    /// Semi-transparent white (bit 15 set)
    pub const WHITE_SEMI: Color15 = Color15(0xFFFF);

    /// Create from 5-bit RGB values (0-31 each), opaque (no semi-transparency)
    #[inline]
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        let r = (r.min(31) as u16) << 10;
        let g = (g.min(31) as u16) << 5;
        let b = b.min(31) as u16;
        Color15(r | g | b)
    }

    /// Create from 5-bit RGB with semi-transparency bit
    #[inline]
    pub fn new_semi(r: u8, g: u8, b: u8, semi_transparent: bool) -> Self {
        let mut c = Self::new(r, g, b);
        if semi_transparent {
            c.0 |= 0x8000;
        }
        c
    }

    /// Create from 8-bit RGB values (quantized to 5-bit)
    #[inline]
    pub fn from_rgb888(r: u8, g: u8, b: u8) -> Self {
        Self::new(r >> 3, g >> 3, b >> 3)
    }

    /// Create from 8-bit RGB with semi-transparency
    #[inline]
    pub fn from_rgb888_semi(r: u8, g: u8, b: u8, semi_transparent: bool) -> Self {
        Self::new_semi(r >> 3, g >> 3, b >> 3, semi_transparent)
    }

    /// Create from Color (8-bit) - quantizes to 5-bit
    /// Maps BlendMode::Erase to transparent (0x0000)
    /// Maps non-Opaque blend modes to semi-transparent (bit 15 set)
    #[inline]
    pub fn from_color(c: Color) -> Self {
        if c.blend == BlendMode::Erase {
            return Color15::TRANSPARENT;
        }
        let semi = c.blend != BlendMode::Opaque;
        Self::from_rgb888_semi(c.r, c.g, c.b, semi)
    }

    /// Convert to Color (8-bit) for display
    /// Maps transparent (0x0000) to BlendMode::Erase
    /// Maps semi-transparent (bit 15) to BlendMode::Average (caller should override)
    #[inline]
    pub fn to_color(self) -> Color {
        if self.is_transparent() {
            return Color::TRANSPARENT;
        }
        let blend = if self.is_semi_transparent() {
            BlendMode::Average // Default blend mode, face should override
        } else {
            BlendMode::Opaque
        };
        Color::with_blend(self.r8(), self.g8(), self.b8(), blend)
    }

    /// Check if this pixel is fully transparent (0x0000 = not drawn)
    #[inline]
    pub fn is_transparent(self) -> bool {
        self.0 == 0x0000
    }

    /// Check if semi-transparency bit is set (bit 15)
    #[inline]
    pub fn is_semi_transparent(self) -> bool {
        self.0 & 0x8000 != 0
    }

    /// Set semi-transparency bit
    #[inline]
    pub fn set_semi_transparent(&mut self, semi: bool) {
        if semi {
            self.0 |= 0x8000;
        } else {
            self.0 &= 0x7FFF;
        }
    }

    /// Get red channel as 5-bit value (0-31)
    #[inline]
    pub fn r5(self) -> u8 {
        ((self.0 >> 10) & 0x1F) as u8
    }

    /// Get green channel as 5-bit value (0-31)
    #[inline]
    pub fn g5(self) -> u8 {
        ((self.0 >> 5) & 0x1F) as u8
    }

    /// Get blue channel as 5-bit value (0-31)
    #[inline]
    pub fn b5(self) -> u8 {
        (self.0 & 0x1F) as u8
    }

    /// Get red channel as 8-bit value (0-255, expanded from 5-bit)
    #[inline]
    pub fn r8(self) -> u8 {
        self.r5() << 3
    }

    /// Get green channel as 8-bit value (0-255, expanded from 5-bit)
    #[inline]
    pub fn g8(self) -> u8 {
        self.g5() << 3
    }

    /// Get blue channel as 8-bit value (0-255, expanded from 5-bit)
    #[inline]
    pub fn b8(self) -> u8 {
        self.b5() << 3
    }

    /// PS1-style texture modulation: (self * vertex_color) / 128
    /// Works in 5-bit space for authentic PS1 behavior
    /// Note: If result is all-zero RGB, sets bit 15 to make it drawable black (not transparent)
    #[inline]
    pub fn modulate(self, vertex_r: u8, vertex_g: u8, vertex_b: u8) -> Self {
        // Convert vertex colors from 8-bit to 5-bit scale factor (0-31 maps to 0.0-~2.0)
        // PS1 used 128 as neutral, so vertex 128 = no change
        // In 5-bit: 16 = neutral (128 >> 3)
        let r = ((self.r5() as u16 * vertex_r as u16) / 128).min(31) as u8;
        let g = ((self.g5() as u16 * vertex_g as u16) / 128).min(31) as u8;
        let b = ((self.b5() as u16 * vertex_b as u16) / 128).min(31) as u8;
        // If result is all black (0x0000), set bit 15 to avoid being treated as transparent
        let semi = self.is_semi_transparent() || (r == 0 && g == 0 && b == 0);
        Self::new_semi(r, g, b, semi)
    }

    /// Apply shading (multiply by intensity 0.0-1.0)
    /// Note: If result is all-zero RGB, sets bit 15 to make it drawable black (not transparent)
    #[inline]
    pub fn shade(self, intensity: f32) -> Self {
        let i = intensity.clamp(0.0, 1.0);
        let r = (self.r5() as f32 * i) as u8;
        let g = (self.g5() as f32 * i) as u8;
        let b = (self.b5() as f32 * i) as u8;
        // If result is all black (0x0000), set bit 15 to avoid being treated as transparent
        let semi = self.is_semi_transparent() || (r == 0 && g == 0 && b == 0);
        Self::new_semi(r, g, b, semi)
    }

    /// Apply RGB shading (different intensity per channel)
    /// Note: If result is all-zero RGB, sets bit 15 to make it drawable black (not transparent)
    #[inline]
    pub fn shade_rgb(self, r_shade: f32, g_shade: f32, b_shade: f32) -> Self {
        let r = (self.r5() as f32 * r_shade.clamp(0.0, 2.0)).min(31.0) as u8;
        let g = (self.g5() as f32 * g_shade.clamp(0.0, 2.0)).min(31.0) as u8;
        let b = (self.b5() as f32 * b_shade.clamp(0.0, 2.0)).min(31.0) as u8;
        // If result is all black (0x0000), set bit 15 to avoid being treated as transparent
        let semi = self.is_semi_transparent() || (r == 0 && g == 0 && b == 0);
        Self::new_semi(r, g, b, semi)
    }

    /// Interpolate between two colors (for Gouraud shading)
    /// Note: If result is all-zero RGB, sets bit 15 to make it drawable black (not transparent)
    #[inline]
    pub fn lerp(self, other: Color15, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let inv_t = 1.0 - t;
        let r = (self.r5() as f32 * inv_t + other.r5() as f32 * t) as u8;
        let g = (self.g5() as f32 * inv_t + other.g5() as f32 * t) as u8;
        let b = (self.b5() as f32 * inv_t + other.b5() as f32 * t) as u8;
        // Semi-transparency: if either is semi-transparent, result is semi-transparent
        // Also set bit 15 if result is all black to avoid transparency
        let semi = self.is_semi_transparent() || other.is_semi_transparent() || (r == 0 && g == 0 && b == 0);
        Self::new_semi(r, g, b, semi)
    }

    /// Convert to [u8; 4] RGBA for framebuffer display
    #[inline]
    pub fn to_rgba(self) -> [u8; 4] {
        if self.is_transparent() {
            [0, 0, 0, 0]
        } else {
            [self.r8(), self.g8(), self.b8(), 255]
        }
    }
}

// Serialization for Color15 - store as u16
impl Serialize for Color15 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Color15 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        u16::deserialize(deserializer).map(Color15)
    }
}

// =============================================================================
// PS1 CLUT (Color Look-Up Table) Types
// =============================================================================

/// PS1 CLUT depth modes
/// 4-bit = 16 colors, 8-bit = 256 colors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ClutDepth {
    /// 4-bit indexed (16 colors) - smallest, most VRAM efficient
    #[default]
    Bpp4,
    /// 8-bit indexed (256 colors) - more colors, still efficient
    Bpp8,
}

impl ClutDepth {
    /// Number of colors in this CLUT depth
    #[inline]
    pub fn color_count(&self) -> usize {
        match self {
            ClutDepth::Bpp4 => 16,
            ClutDepth::Bpp8 => 256,
        }
    }

    /// Bits per pixel
    #[inline]
    pub fn bits_per_pixel(&self) -> usize {
        match self {
            ClutDepth::Bpp4 => 4,
            ClutDepth::Bpp8 => 8,
        }
    }

    /// Maximum valid index for this depth
    #[inline]
    pub fn max_index(&self) -> u8 {
        match self {
            ClutDepth::Bpp4 => 15,
            ClutDepth::Bpp8 => 255,
        }
    }

    /// Label for UI display
    pub fn label(&self) -> &'static str {
        match self {
            ClutDepth::Bpp4 => "4-bit (16)",
            ClutDepth::Bpp8 => "8-bit (256)",
        }
    }

    /// Short label for UI badges
    pub fn short_label(&self) -> &'static str {
        match self {
            ClutDepth::Bpp4 => "4b",
            ClutDepth::Bpp8 => "8b",
        }
    }
}

/// Unique identifier for CLUTs in the global pool
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct ClutId(pub u32);

impl ClutId {
    /// Invalid/unassigned CLUT ID
    pub const NONE: ClutId = ClutId(0);

    /// Check if this is a valid (non-zero) ID
    #[inline]
    pub fn is_valid(&self) -> bool {
        self.0 != 0
    }
}

/// PS1-authentic Color Look-Up Table (CLUT)
///
/// Stores 16 (4-bit) or 256 (8-bit) Color15 entries.
/// Index 0 is typically transparent (0x0000) for sprite transparency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clut {
    /// Unique identifier in the CLUT pool
    pub id: ClutId,
    /// Human-readable name
    pub name: String,
    /// CLUT depth (4-bit or 8-bit)
    pub depth: ClutDepth,
    /// Color entries (16 for 4-bit, 256 for 8-bit)
    /// Entry at index 0 is typically Color15::TRANSPARENT for PS1-style color keying
    pub colors: Vec<Color15>,
}

impl Clut {
    /// Create a new 4-bit (16 color) CLUT with default grayscale ramp
    pub fn new_4bit(name: impl Into<String>) -> Self {
        let mut colors = Vec::with_capacity(16);
        // Index 0 = transparent (PS1 convention)
        colors.push(Color15::TRANSPARENT);
        // Indices 1-15 = grayscale ramp
        for i in 1..16 {
            let v = (i * 2) as u8; // 2, 4, 6, ... 30
            colors.push(Color15::new(v, v, v));
        }
        Self {
            id: ClutId::NONE,
            name: name.into(),
            depth: ClutDepth::Bpp4,
            colors,
        }
    }

    /// Create a new 8-bit (256 color) CLUT with default grayscale ramp
    pub fn new_8bit(name: impl Into<String>) -> Self {
        let mut colors = Vec::with_capacity(256);
        // Index 0 = transparent
        colors.push(Color15::TRANSPARENT);
        // Indices 1-255 = grayscale with wrap
        for i in 1..256 {
            let v = ((i * 31) / 255) as u8;
            colors.push(Color15::new(v, v, v));
        }
        Self {
            id: ClutId::NONE,
            name: name.into(),
            depth: ClutDepth::Bpp8,
            colors,
        }
    }

    /// Create an empty CLUT (all transparent) with given depth
    pub fn new_empty(name: impl Into<String>, depth: ClutDepth) -> Self {
        Self {
            id: ClutId::NONE,
            name: name.into(),
            depth,
            colors: vec![Color15::TRANSPARENT; depth.color_count()],
        }
    }

    /// Look up color by palette index
    /// Returns TRANSPARENT for out-of-bounds indices
    #[inline]
    pub fn lookup(&self, index: u8) -> Color15 {
        let idx = index as usize;
        if idx < self.colors.len() {
            self.colors[idx]
        } else {
            Color15::TRANSPARENT
        }
    }

    /// Set color at palette index
    /// Silently ignores out-of-bounds indices
    pub fn set_color(&mut self, index: u8, color: Color15) {
        let idx = index as usize;
        if idx < self.colors.len() {
            self.colors[idx] = color;
        }
    }

    /// Get color at index (with bounds check)
    pub fn get_color(&self, index: u8) -> Option<Color15> {
        self.colors.get(index as usize).copied()
    }

    /// Number of colors in this CLUT
    #[inline]
    pub fn len(&self) -> usize {
        self.colors.len()
    }

    /// Check if CLUT is empty (should never be true for valid CLUTs)
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }
}

/// PS1-authentic indexed texture
///
/// Stores palette indices (0-15 for 4-bit, 0-255 for 8-bit) instead of colors.
/// Actual colors are looked up from a CLUT at render time.
/// This enables palette swapping (same texture, different CLUT = different colors).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedTexture {
    pub width: usize,
    pub height: usize,
    /// CLUT depth determines valid index range
    pub depth: ClutDepth,
    /// Palette indices for each pixel
    pub indices: Vec<u8>,
    /// Default CLUT ID for this texture
    pub default_clut: ClutId,
    /// Human-readable name
    pub name: String,
}

impl IndexedTexture {
    /// Create a new indexed texture filled with index 0 (transparent)
    pub fn new(width: usize, height: usize, depth: ClutDepth) -> Self {
        Self {
            width,
            height,
            depth,
            indices: vec![0; width * height],
            default_clut: ClutId::NONE,
            name: String::new(),
        }
    }

    /// Sample palette index at UV coordinates (PS1-style, no filtering)
    #[inline]
    pub fn sample_index(&self, u: f32, v: f32) -> u8 {
        let u_wrapped = u.rem_euclid(1.0);
        let v_wrapped = v.rem_euclid(1.0);
        let tx = ((u_wrapped * self.width as f32) as usize).min(self.width.saturating_sub(1));
        let ty = ((v_wrapped * self.height as f32) as usize).min(self.height.saturating_sub(1));
        self.indices.get(ty * self.width + tx).copied().unwrap_or(0)
    }

    /// Sample with CLUT lookup - returns final Color15
    #[inline]
    pub fn sample(&self, u: f32, v: f32, clut: &Clut) -> Color15 {
        let index = self.sample_index(u, v);
        clut.lookup(index)
    }

    /// Get palette index at pixel coordinates
    pub fn get_index(&self, x: usize, y: usize) -> u8 {
        if x < self.width && y < self.height {
            self.indices.get(y * self.width + x).copied().unwrap_or(0)
        } else {
            0
        }
    }

    /// Set palette index at pixel coordinates
    /// Index is clamped to valid range for the CLUT depth
    pub fn set_index(&mut self, x: usize, y: usize, index: u8) {
        if x < self.width && y < self.height {
            let clamped = index.min(self.depth.max_index());
            if let Some(pixel) = self.indices.get_mut(y * self.width + x) {
                *pixel = clamped;
            }
        }
    }

    /// Total number of pixels
    #[inline]
    pub fn pixel_count(&self) -> usize {
        self.width * self.height
    }

    /// Convert to direct-color Texture15 using a CLUT
    /// Useful for preview or export
    pub fn to_texture15(&self, clut: &Clut) -> Texture15 {
        let pixels: Vec<Color15> = self.indices
            .iter()
            .map(|&idx| clut.lookup(idx))
            .collect();

        Texture15 {
            width: self.width,
            height: self.height,
            pixels,
            name: self.name.clone(),
            blend_mode: BlendMode::Opaque,
        }
    }
}

// =============================================================================
// PS1 RGB555 Texture Type
// =============================================================================

/// PS1-authentic texture using 15-bit color (RGB555 + semi-transparency bit)
///
/// Uses `Vec<Color15>` for storage - each pixel is a u16 with:
/// - 0x0000 = fully transparent (CLUT-style color key)
/// - Bit 15 = semi-transparency flag (use face's blend mode if set)
/// - Bits 14-10 = Red (5 bits, 0-31)
/// - Bits 9-5 = Green (5 bits, 0-31)
/// - Bits 4-0 = Blue (5 bits, 0-31)
#[derive(Debug, Clone)]
pub struct Texture15 {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<Color15>,
    pub name: String,
    /// Blend mode for semi-transparent pixels (STP bit set)
    pub blend_mode: BlendMode,
}

impl Texture15 {
    /// Create a new texture filled with white
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![Color15::WHITE; width * height],
            name: String::new(),
            blend_mode: BlendMode::Opaque,
        }
    }

    /// Create a new texture filled with a specific color
    pub fn new_filled(width: usize, height: usize, color: Color15) -> Self {
        Self {
            width,
            height,
            pixels: vec![color; width * height],
            name: String::new(),
            blend_mode: BlendMode::Opaque,
        }
    }

    /// Load texture from a PNG file
    /// Alpha channel: 0 = transparent (0x0000), otherwise opaque
    pub fn from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, String> {
        use image::GenericImageView;

        let path = path.as_ref();
        let img = image::open(path)
            .map_err(|e| format!("Failed to load {}: {}", path.display(), e))?;

        let (width, height) = img.dimensions();
        let rgba = img.to_rgba8();

        let pixels: Vec<Color15> = rgba
            .pixels()
            .map(|p| {
                // Alpha 0 = transparent (0x0000), otherwise quantize to RGB555
                if p[3] == 0 {
                    Color15::TRANSPARENT
                } else {
                    Color15::from_rgb888(p[0], p[1], p[2])
                }
            })
            .collect();

        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        Ok(Self {
            width: width as usize,
            height: height as usize,
            pixels,
            name,
            blend_mode: BlendMode::Opaque,
        })
    }

    /// Load texture from raw PNG bytes
    pub fn from_bytes(bytes: &[u8], name: String) -> Result<Self, String> {
        use image::GenericImageView;

        let img = image::load_from_memory(bytes)
            .map_err(|e| format!("Failed to decode image: {}", e))?;

        let (width, height) = img.dimensions();
        let rgba = img.to_rgba8();

        let pixels: Vec<Color15> = rgba
            .pixels()
            .map(|p| {
                if p[3] == 0 {
                    Color15::TRANSPARENT
                } else {
                    Color15::from_rgb888(p[0], p[1], p[2])
                }
            })
            .collect();

        Ok(Self {
            width: width as usize,
            height: height as usize,
            pixels,
            name,
            blend_mode: BlendMode::Opaque,
        })
    }

    /// Convert from an 8-bit Texture
    pub fn from_texture(tex: &Texture) -> Self {
        let pixels: Vec<Color15> = tex.pixels.iter().map(|c| Color15::from_color(*c)).collect();
        Self {
            width: tex.width,
            height: tex.height,
            pixels,
            name: tex.name.clone(),
            blend_mode: BlendMode::Opaque,
        }
    }

    /// Convert from an 8-bit Texture with blend mode
    pub fn from_texture_with_blend(tex: &Texture, blend_mode: BlendMode) -> Self {
        let pixels: Vec<Color15> = tex.pixels.iter().map(|c| Color15::from_color(*c)).collect();
        Self {
            width: tex.width,
            height: tex.height,
            pixels,
            name: tex.name.clone(),
            blend_mode,
        }
    }

    /// Convert to an 8-bit Texture (for backwards compatibility)
    pub fn to_texture(&self) -> Texture {
        let pixels: Vec<Color> = self.pixels.iter().map(|c| c.to_color()).collect();
        Texture {
            width: self.width,
            height: self.height,
            pixels,
            name: self.name.clone(),
            blend_mode: self.blend_mode,
        }
    }

    /// Sample texture at UV coordinates (no filtering - PS1 style)
    /// Handles negative UVs correctly using euclidean modulo for proper tiling
    #[inline]
    pub fn sample(&self, u: f32, v: f32) -> Color15 {
        let u_wrapped = u.rem_euclid(1.0);
        let v_wrapped = v.rem_euclid(1.0);
        let tx = ((u_wrapped * self.width as f32) as usize).min(self.width - 1);
        let ty = ((v_wrapped * self.height as f32) as usize).min(self.height - 1);
        self.pixels[ty * self.width + tx]
    }

    /// Get pixel at x,y coordinates
    #[inline]
    pub fn get_pixel(&self, x: usize, y: usize) -> Color15 {
        if x < self.width && y < self.height {
            self.pixels[y * self.width + x]
        } else {
            Color15::TRANSPARENT
        }
    }

    /// Set pixel at x,y coordinates
    #[inline]
    pub fn set_pixel(&mut self, x: usize, y: usize, color: Color15) {
        if x < self.width && y < self.height {
            self.pixels[y * self.width + x] = color;
        }
    }

    /// Create a checkerboard test texture
    pub fn checkerboard(width: usize, height: usize, color1: Color15, color2: Color15) -> Self {
        let mut pixels = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                let checker = ((x / 4) + (y / 4)) % 2 == 0;
                pixels.push(if checker { color1 } else { color2 });
            }
        }
        Self { width, height, pixels, name: "checkerboard".to_string(), blend_mode: BlendMode::Opaque }
    }
}

// =============================================================================
// Original Color Type (8-bit, for backwards compatibility)
// =============================================================================

/// RGB color with PS1-style blend mode (no 8-bit alpha, just 6 blend states)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "ColorDeserialize")]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub blend: BlendMode,
}

/// Helper for deserializing Color - handles both old (a: u8) and new (blend: BlendMode) formats
#[derive(Deserialize)]
struct ColorDeserialize {
    r: u8,
    g: u8,
    b: u8,
    /// New format: explicit blend mode
    #[serde(default = "default_blend")]
    blend: BlendMode,
    /// Old format: alpha value (ignored if blend is present, used for backwards compat)
    #[serde(default)]
    a: u8,
}

fn default_blend() -> BlendMode {
    BlendMode::Opaque
}

impl From<ColorDeserialize> for Color {
    fn from(c: ColorDeserialize) -> Self {
        // Old format: a=0 means transparent, a=255 means opaque
        // New format: blend field specifies the mode directly
        // Since blend defaults to Opaque, we just use it directly.
        // The only case where we need special handling is a=0 (old transparent).
        // But since `a` defaults to 0 when not present (new format), we can't distinguish.
        // Solution: new files don't have `a` field, so a=0 is the default.
        // Old files always had a=255 for opaque colors.
        // So: if a=0 AND blend is default (Opaque), it's either new format or old transparent.
        // We'll just use blend directly - old files with a=0 were rare (transparent pixels).
        Color { r: c.r, g: c.g, b: c.b, blend: c.blend }
    }
}

impl Color {
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0, blend: BlendMode::Opaque };
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255, blend: BlendMode::Opaque };
    pub const RED: Color = Color { r: 255, g: 0, b: 0, blend: BlendMode::Opaque };
    pub const GREEN: Color = Color { r: 0, g: 255, b: 0, blend: BlendMode::Opaque };
    pub const BLUE: Color = Color { r: 0, g: 0, b: 255, blend: BlendMode::Opaque };
    /// Transparent color (will not be rendered)
    pub const TRANSPARENT: Color = Color { r: 0, g: 0, b: 0, blend: BlendMode::Erase };
    /// Neutral color for PS1 texture modulation (128, 128, 128)
    /// When used with modulate(), texture colors remain unchanged
    pub const NEUTRAL: Color = Color { r: 128, g: 128, b: 128, blend: BlendMode::Opaque };

    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, blend: BlendMode::Opaque }
    }

    /// Create color with specific blend mode
    pub fn with_blend(r: u8, g: u8, b: u8, blend: BlendMode) -> Self {
        Self { r, g, b, blend }
    }

    /// Check if this color is transparent (should not be rendered)
    pub fn is_transparent(&self) -> bool {
        self.blend == BlendMode::Erase
    }

    /// Apply shading (multiply by intensity 0.0-1.0)
    pub fn shade(self, intensity: f32) -> Self {
        let i = intensity.clamp(0.0, 1.0);
        Self {
            r: (self.r as f32 * i) as u8,
            g: (self.g as f32 * i) as u8,
            b: (self.b as f32 * i) as u8,
            blend: self.blend,
        }
    }

    /// PS1-style texture modulation: (self * vertex_color) / 128
    /// vertex_color of 128 = no change, >128 = brighten, <128 = darken
    /// This is the authentic PS1 formula for combining textures with vertex colors
    pub fn modulate(self, vertex_color: Color) -> Self {
        Self {
            r: ((self.r as u16 * vertex_color.r as u16) / 128).min(255) as u8,
            g: ((self.g as u16 * vertex_color.g as u16) / 128).min(255) as u8,
            b: ((self.b as u16 * vertex_color.b as u16) / 128).min(255) as u8,
            blend: self.blend,
        }
    }

    /// Interpolate between two colors (RGB only, keeps self's blend mode)
    pub fn lerp(self, other: Color, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let inv_t = 1.0 - t;
        Self {
            r: (self.r as f32 * inv_t + other.r as f32 * t) as u8,
            g: (self.g as f32 * inv_t + other.g as f32 * t) as u8,
            b: (self.b as f32 * inv_t + other.b as f32 * t) as u8,
            blend: self.blend,
        }
    }

    /// Convert to u32 (RGBA format for macroquad, uses 255 alpha for opaque, 0 for transparent)
    pub fn to_u32(self) -> u32 {
        let a = if self.blend == BlendMode::Erase { 0u8 } else { 255u8 };
        ((self.r as u32) << 24) | ((self.g as u32) << 16) | ((self.b as u32) << 8) | (a as u32)
    }

    /// Convert to [u8; 4] for framebuffer (RGBA, uses 255 alpha for opaque, 0 for transparent)
    pub fn to_bytes(self) -> [u8; 4] {
        let a = if self.blend == BlendMode::Erase { 0u8 } else { 255u8 };
        [self.r, self.g, self.b, a]
    }

    // =====================================================
    // PS1 15-bit color helpers (5 bits per channel, 0-31)
    // =====================================================

    /// Create color from PS1 5-bit values (0-31 per channel)
    pub fn from_ps1(r: u8, g: u8, b: u8) -> Self {
        Self::new((r.min(31)) << 3, (g.min(31)) << 3, (b.min(31)) << 3)
    }

    /// Get red channel as 5-bit value (0-31)
    pub fn r5(&self) -> u8 {
        self.r >> 3
    }

    /// Get green channel as 5-bit value (0-31)
    pub fn g5(&self) -> u8 {
        self.g >> 3
    }

    /// Get blue channel as 5-bit value (0-31)
    pub fn b5(&self) -> u8 {
        self.b >> 3
    }

    /// Set red channel from 5-bit value (0-31)
    pub fn set_r5(&mut self, v: u8) {
        self.r = (v.min(31)) << 3;
    }

    /// Set green channel from 5-bit value (0-31)
    pub fn set_g5(&mut self, v: u8) {
        self.g = (v.min(31)) << 3;
    }

    /// Set blue channel from 5-bit value (0-31)
    pub fn set_b5(&mut self, v: u8) {
        self.b = (v.min(31)) << 3;
    }

    /// Quantize color to PS1 15-bit (5 bits per channel)
    /// Uses the same 0xF8 mask as the PS1 hardware to keep top 5 bits
    #[inline]
    pub fn quantize_15bit(self) -> Self {
        Self {
            r: self.r & 0xF8,
            g: self.g & 0xF8,
            b: self.b & 0xF8,
            blend: self.blend,
        }
    }

    /// PS1-style blend: combine this color (front) with back color using the front color's blend mode
    pub fn blend_with(self, back: Color) -> Color {
        match self.blend {
            BlendMode::Opaque => self,
            BlendMode::Average => {
                // Mode 0: 0.5*B + 0.5*F
                Color::with_blend(
                    ((back.r as u16 + self.r as u16) / 2) as u8,
                    ((back.g as u16 + self.g as u16) / 2) as u8,
                    ((back.b as u16 + self.b as u16) / 2) as u8,
                    BlendMode::Opaque, // Result is opaque
                )
            }
            BlendMode::Add => {
                // Mode 1: B + F (clamped to 255)
                Color::with_blend(
                    (back.r as u16 + self.r as u16).min(255) as u8,
                    (back.g as u16 + self.g as u16).min(255) as u8,
                    (back.b as u16 + self.b as u16).min(255) as u8,
                    BlendMode::Opaque,
                )
            }
            BlendMode::Subtract => {
                // Mode 2: B - F (clamped to 0)
                Color::with_blend(
                    (back.r as i16 - self.r as i16).max(0) as u8,
                    (back.g as i16 - self.g as i16).max(0) as u8,
                    (back.b as i16 - self.b as i16).max(0) as u8,
                    BlendMode::Opaque,
                )
            }
            BlendMode::AddQuarter => {
                // Mode 3: B + 0.25*F (clamped to 255)
                Color::with_blend(
                    (back.r as u16 + self.r as u16 / 4).min(255) as u8,
                    (back.g as u16 + self.g as u16 / 4).min(255) as u8,
                    (back.b as u16 + self.b as u16 / 4).min(255) as u8,
                    BlendMode::Opaque,
                )
            }
            BlendMode::Erase => {
                // Eraser: make transparent
                Color::TRANSPARENT
            }
        }
    }

    /// PS1-style blend: combine this color (front) with back color using explicit blend mode
    /// (for backwards compatibility with code that passes blend mode separately)
    pub fn blend(self, back: Color, mode: BlendMode) -> Color {
        Color::with_blend(self.r, self.g, self.b, mode).blend_with(back)
    }
}

impl Default for Color {
    fn default() -> Self {
        Color::TRANSPARENT
    }
}

/// A vertex with position, texture coordinate, normal, and color
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub struct Vertex {
    pub pos: Vec3,
    pub uv: Vec2,
    pub normal: Vec3,
    /// Per-vertex color for PS1-style texture modulation
    /// Default is (128, 128, 128) = neutral (no change to texture)
    /// Values > 128 brighten, < 128 darken
    pub color: Color,
    /// Optional bone index for mesh editor export
    /// Used when exporting mesh editor models to PS1 format
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub bone_index: Option<usize>,
}

impl Vertex {
    pub fn new(pos: Vec3, uv: Vec2, normal: Vec3) -> Self {
        Self { pos, uv, normal, color: Color::NEUTRAL, bone_index: None }
    }

    /// Create vertex with explicit color
    pub fn with_color(pos: Vec3, uv: Vec2, normal: Vec3, color: Color) -> Self {
        Self { pos, uv, normal, color, bone_index: None }
    }

    pub fn from_pos(x: f32, y: f32, z: f32) -> Self {
        Self {
            pos: Vec3::new(x, y, z),
            uv: Vec2::default(),
            normal: Vec3::ZERO,
            color: Color::NEUTRAL,
            bone_index: None,
        }
    }
}

/// A triangle face (indices into vertex array)
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Face {
    pub v0: usize,
    pub v1: usize,
    pub v2: usize,
    pub texture_id: Option<usize>,
    /// If true, pure black pixels (RGB 0,0,0) are treated as transparent (PS1 CLUT-style)
    /// If false, black pixels are rendered as black
    #[serde(default = "default_black_transparent")]
    pub black_transparent: bool,
    /// PS1 blend mode for this face (per-face, not per-pixel)
    /// Controls how semi-transparent pixels are blended with the background
    #[serde(default)]
    pub blend_mode: BlendMode,
}

fn default_black_transparent() -> bool {
    true
}

impl Face {
    pub fn new(v0: usize, v1: usize, v2: usize) -> Self {
        Self {
            v0,
            v1,
            v2,
            texture_id: None,
            black_transparent: true, // Default: black is transparent (PS1 style)
            blend_mode: BlendMode::Opaque,
        }
    }

    pub fn with_texture(v0: usize, v1: usize, v2: usize, texture_id: usize) -> Self {
        Self {
            v0,
            v1,
            v2,
            texture_id: Some(texture_id),
            black_transparent: true, // Default: black is transparent (PS1 style)
            blend_mode: BlendMode::Opaque,
        }
    }

    /// Set the black_transparent flag (builder pattern)
    pub fn with_black_transparent(mut self, black_transparent: bool) -> Self {
        self.black_transparent = black_transparent;
        self
    }

    /// Set the blend mode (builder pattern)
    pub fn with_blend_mode(mut self, blend_mode: BlendMode) -> Self {
        self.blend_mode = blend_mode;
        self
    }
}

/// Simple texture (array of colors)
#[derive(Debug, Clone)]
pub struct Texture {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<Color>,
    pub name: String,
    /// Global blend mode for semi-transparent pixels (default Opaque)
    pub blend_mode: BlendMode,
}

impl Texture {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![Color::WHITE; width * height],
            name: String::new(),
            blend_mode: BlendMode::Opaque,
        }
    }

    /// Load texture from a PNG file
    /// Alpha channel is converted to blend mode: 0 = Erase (transparent), otherwise Opaque
    pub fn from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, String> {
        use image::GenericImageView;

        let path = path.as_ref();
        let img = image::open(path)
            .map_err(|e| format!("Failed to load {}: {}", path.display(), e))?;

        let (width, height) = img.dimensions();
        let rgba = img.to_rgba8();

        let pixels: Vec<Color> = rgba
            .pixels()
            .map(|p| {
                // Convert alpha to blend mode: 0 = transparent, otherwise opaque
                let blend = if p[3] == 0 { BlendMode::Erase } else { BlendMode::Opaque };
                Color::with_blend(p[0], p[1], p[2], blend)
            })
            .collect();

        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        Ok(Self {
            width: width as usize,
            height: height as usize,
            pixels,
            name,
            blend_mode: BlendMode::Opaque,
        })
    }

    /// Quantize all pixels to PS1 15-bit color (5 bits per channel)
    /// Modifies pixels in-place. Call after loading to simulate PS1 color depth.
    pub fn quantize_15bit(&mut self) {
        for pixel in &mut self.pixels {
            *pixel = pixel.quantize_15bit();
        }
    }

    /// Load all textures from a directory
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_directory<P: AsRef<std::path::Path>>(dir: P) -> Vec<Self> {
        use indicatif::{ProgressBar, ProgressStyle};

        let dir = dir.as_ref();
        let mut textures = Vec::new();

        if let Ok(entries) = std::fs::read_dir(dir) {
            let mut paths: Vec<_> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.extension()
                        .map(|ext| ext.to_ascii_lowercase() == "png")
                        .unwrap_or(false)
                })
                .collect();

            paths.sort();

            let total = paths.len() as u64;
            let pb = ProgressBar::new(total);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("Loading textures [{bar:30}] {pos}/{len} {msg}")
                    .unwrap()
                    .progress_chars("█▓░"),
            );

            for path in paths {
                match Self::from_file(&path) {
                    Ok(tex) => {
                        pb.set_message(format!("{} ({}x{})", tex.name, tex.width, tex.height));
                        textures.push(tex);
                    }
                    Err(e) => {
                        pb.set_message(format!("Error: {}", e));
                    }
                }
                pb.inc(1);
            }

            pb.finish_with_message(format!("Loaded {} textures", textures.len()));
        }

        textures
    }

    /// Load all textures from a directory (WASM - no progress bar)
    #[cfg(target_arch = "wasm32")]
    pub fn load_directory<P: AsRef<std::path::Path>>(dir: P) -> Vec<Self> {
        let dir = dir.as_ref();
        let mut textures = Vec::new();

        if let Ok(entries) = std::fs::read_dir(dir) {
            let mut paths: Vec<_> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.extension()
                        .map(|ext| ext.to_ascii_lowercase() == "png")
                        .unwrap_or(false)
                })
                .collect();

            paths.sort();

            for path in paths {
                if let Ok(tex) = Self::from_file(&path) {
                    textures.push(tex);
                }
            }
        }

        textures
    }

    /// Load texture from raw PNG bytes
    /// Alpha channel is converted to blend mode: 0 = Erase (transparent), otherwise Opaque
    pub fn from_bytes(bytes: &[u8], name: String) -> Result<Self, String> {
        use image::GenericImageView;

        let img = image::load_from_memory(bytes)
            .map_err(|e| format!("Failed to decode image: {}", e))?;

        let (width, height) = img.dimensions();
        let rgba = img.to_rgba8();

        let pixels: Vec<Color> = rgba
            .pixels()
            .map(|p| {
                // Convert alpha to blend mode: 0 = transparent, otherwise opaque
                let blend = if p[3] == 0 { BlendMode::Erase } else { BlendMode::Opaque };
                Color::with_blend(p[0], p[1], p[2], blend)
            })
            .collect();

        Ok(Self {
            width: width as usize,
            height: height as usize,
            pixels,
            name,
            blend_mode: BlendMode::Opaque,
        })
    }

    /// Create a checkerboard test texture
    pub fn checkerboard(width: usize, height: usize, color1: Color, color2: Color) -> Self {
        let mut pixels = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                let checker = ((x / 4) + (y / 4)) % 2 == 0;
                pixels.push(if checker { color1 } else { color2 });
            }
        }
        Self { width, height, pixels, name: "checkerboard".to_string(), blend_mode: BlendMode::Opaque }
    }

    /// Sample texture at UV coordinates (no filtering - PS1 style)
    /// Handles negative UVs correctly using euclidean modulo for proper tiling
    pub fn sample(&self, u: f32, v: f32) -> Color {
        // Use rem_euclid to handle negative UVs correctly (proper tiling)
        let u_wrapped = u.rem_euclid(1.0);
        let v_wrapped = v.rem_euclid(1.0);
        let tx = ((u_wrapped * self.width as f32) as usize).min(self.width - 1);
        let ty = ((v_wrapped * self.height as f32) as usize).min(self.height - 1);
        self.pixels[ty * self.width + tx]
    }

    /// Get pixel at x,y coordinates
    pub fn get_pixel(&self, x: usize, y: usize) -> Color {
        if x < self.width && y < self.height {
            self.pixels[y * self.width + x]
        } else {
            Color::BLACK
        }
    }

    /// Convert to Texture15 (RGB555) for PS1-authentic rendering
    /// Handles blend modes: Erase becomes TRANSPARENT (0x0000),
    /// other modes set semi-transparency bit if not Opaque
    pub fn to_15(&self) -> Texture15 {
        let pixels: Vec<Color15> = self.pixels.iter().map(|c| {
            if c.blend == BlendMode::Erase {
                Color15::TRANSPARENT
            } else {
                let semi = c.blend != BlendMode::Opaque;
                Color15::from_rgb888_semi(c.r, c.g, c.b, semi)
            }
        }).collect();

        Texture15 {
            width: self.width,
            height: self.height,
            pixels,
            name: self.name.clone(),
            blend_mode: self.blend_mode,
        }
    }
}

/// Shading mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadingMode {
    None,     // No shading, raw texture/vertex colors
    Flat,     // One light calculation per face
    Gouraud,  // Interpolate vertex colors (PS1 style)
}

/// Light type (directional, point, or spot)
#[derive(Debug, Clone, Copy)]
pub enum LightType {
    /// Infinite directional light (like the sun)
    Directional { direction: Vec3 },
    /// Point light with falloff radius
    Point { position: Vec3, radius: f32 },
    /// Spot light with cone angle and falloff
    Spot { position: Vec3, direction: Vec3, angle: f32, radius: f32 },
}

/// A light source in the scene
#[derive(Debug, Clone)]
pub struct Light {
    pub light_type: LightType,
    pub color: Color,
    pub intensity: f32,
    pub enabled: bool,
    pub name: String,
}

impl Light {
    /// Create a new directional light
    pub fn directional(direction: Vec3, intensity: f32) -> Self {
        Self {
            light_type: LightType::Directional { direction: direction.normalize() },
            color: Color::WHITE,
            intensity,
            enabled: true,
            name: String::from("Directional"),
        }
    }

    /// Create a new point light
    pub fn point(position: Vec3, radius: f32, intensity: f32) -> Self {
        Self {
            light_type: LightType::Point { position, radius },
            color: Color::WHITE,
            intensity,
            enabled: true,
            name: String::from("Point"),
        }
    }

    /// Create a new spot light
    pub fn spot(position: Vec3, direction: Vec3, angle: f32, radius: f32, intensity: f32) -> Self {
        Self {
            light_type: LightType::Spot {
                position,
                direction: direction.normalize(),
                angle,
                radius,
            },
            color: Color::WHITE,
            intensity,
            enabled: true,
            name: String::from("Spot"),
        }
    }
}

impl Default for Light {
    fn default() -> Self {
        Self::directional(Vec3::new(-1.0, -1.0, -1.0), 0.7)
    }
}

/// PS1 semi-transparency blend modes
/// B = Back pixel (existing framebuffer), F = Front pixel (new pixel)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum BlendMode {
    #[default]
    Opaque,    // No blending, overwrite pixel
    Average,   // Mode 0: 0.5*B + 0.5*F (50/50 mix, water/glass)
    Add,       // Mode 1: B + F (additive glow, clamped to 255)
    Subtract,  // Mode 2: B - F (shadows, clamped to 0)
    AddQuarter,// Mode 3: B + 0.25*F (subtle glow)
    Erase,     // Eraser: Set alpha to 0 (transparent)
}

/// Rasterizer settings
#[derive(Debug, Clone)]
pub struct RasterSettings {
    /// Use affine texture mapping (true = PS1 warping, false = perspective correct)
    pub affine_textures: bool,
    /// Use Z-buffer (false = painter's algorithm)
    pub use_zbuffer: bool,
    /// Shading mode
    pub shading: ShadingMode,
    /// Backface culling
    pub backface_cull: bool,
    /// Show wireframe on back-facing polygons (editor feature, disable in game)
    pub backface_wireframe: bool,
    /// Scene lights (multiple light sources)
    pub lights: Vec<Light>,
    /// Ambient light intensity (0.0-1.0)
    pub ambient: f32,
    /// Use PS1 low resolution (320x240) instead of high resolution
    pub low_resolution: bool,
    /// Enable PS1-style ordered dithering (4x4 Bayer matrix)
    pub dithering: bool,
    /// Stretch to fill viewport (false = maintain 4:3 aspect ratio)
    pub stretch_to_fill: bool,
    /// Wireframe-only mode (no solid mesh, just edges)
    pub wireframe_overlay: bool,
    /// Orthographic projection settings (None = perspective)
    pub ortho_projection: Option<OrthoProjection>,
    /// Use PS1-authentic RGB555 color mode (true = 15-bit color, false = 24-bit)
    /// When enabled, textures and framebuffer use native 15-bit color with
    /// PS1-authentic semi-transparency (bit 15) and CLUT-style transparency (0x0000)
    pub use_rgb555: bool,
    /// Use fixed-point math for projection and interpolation (true = PS1 precision loss)
    /// When enabled, uses 16.16 fixed-point arithmetic which causes the characteristic
    /// PS1 vertex jitter and texture wobble due to limited precision.
    pub use_fixed_point: bool,
}

/// Orthographic projection settings for ortho views
#[derive(Debug, Clone)]
pub struct OrthoProjection {
    /// Zoom level (pixels per world unit)
    pub zoom: f32,
    /// Center offset in world units
    pub center_x: f32,
    pub center_y: f32,
}

impl RasterSettings {
    /// Get the primary light direction (for backwards compatibility)
    /// Returns the first directional light's direction, or a default if none exists
    pub fn primary_light_dir(&self) -> Vec3 {
        for light in &self.lights {
            if let LightType::Directional { direction } = light.light_type {
                if light.enabled {
                    return direction;
                }
            }
        }
        Vec3::new(-1.0, -1.0, -1.0).normalize()
    }

    /// Create settings for in-game rendering (no editor debug features)
    pub fn game() -> Self {
        Self {
            backface_wireframe: false, // Disable editor debug wireframe
            ..Self::default()
        }
    }
}

impl Default for RasterSettings {
    fn default() -> Self {
        Self {
            affine_textures: true,  // PS1 default: affine (warpy)
            use_zbuffer: true,
            shading: ShadingMode::Gouraud,
            backface_cull: true,
            backface_wireframe: true, // Editor default: show backfaces as wireframe
            lights: vec![Light::directional(Vec3::new(-1.0, -1.0, -1.0), 0.7)],
            ambient: 0.3,
            low_resolution: true,   // PS1 default: 320x240
            dithering: true,        // PS1 default: dithering enabled for smooth gradients
            stretch_to_fill: false, // Default: 4:3 aspect ratio with letterboxing
            wireframe_overlay: false, // Default: wireframe off
            ortho_projection: None,  // Default: perspective projection
            use_rgb555: true,        // PS1 default: 15-bit color mode
            use_fixed_point: true,   // PS1 default: fixed-point math (jittery)
        }
    }
}

/// Timing breakdown for rasterization stages (in milliseconds)
#[derive(Debug, Clone, Default)]
pub struct RasterTimings {
    /// Vertex transformation to camera space and projection to screen (ms)
    pub transform_ms: f32,
    /// Surface building and backface culling (ms)
    pub cull_ms: f32,
    /// Depth sorting (painter's algorithm) (ms)
    pub sort_ms: f32,
    /// Triangle rasterization/filling (ms)
    pub draw_ms: f32,
    /// Wireframe rendering (back-face and front-face) (ms)
    pub wireframe_ms: f32,
}

impl RasterTimings {
    /// Accumulate timings from another instance
    pub fn accumulate(&mut self, other: &RasterTimings) {
        self.transform_ms += other.transform_ms;
        self.cull_ms += other.cull_ms;
        self.sort_ms += other.sort_ms;
        self.draw_ms += other.draw_ms;
        self.wireframe_ms += other.wireframe_ms;
    }
}
