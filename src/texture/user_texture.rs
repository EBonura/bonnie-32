//! User texture asset - independent indexed textures with embedded palette
//!
//! UserTexture combines an indexed texture with its CLUT (palette) into a single
//! self-contained asset that can be shared across projects.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Cursor;
use std::path::Path;

use crate::rasterizer::{ClutDepth, Color15};

/// Valid texture sizes for user textures
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TextureSize {
    /// 8x8 - Tiny detail textures (Mesh Editor only)
    Size8x8,
    /// 16x16 - Small textures (Mesh Editor only)
    Size16x16,
    /// 32x32 - Medium textures (Mesh Editor only)
    Size32x32,
    /// 64x64 - Standard textures (World Editor + Mesh Editor)
    Size64x64,
    /// 128x128 - Large textures (Mesh Editor only)
    Size128x128,
    /// 256x256 - Very large textures (Mesh Editor only)
    Size256x256,
}

impl TextureSize {
    /// Get the dimensions as (width, height)
    pub fn dimensions(&self) -> (usize, usize) {
        match self {
            TextureSize::Size8x8 => (8, 8),
            TextureSize::Size16x16 => (16, 16),
            TextureSize::Size32x32 => (32, 32),
            TextureSize::Size64x64 => (64, 64),
            TextureSize::Size128x128 => (128, 128),
            TextureSize::Size256x256 => (256, 256),
        }
    }

    /// Get the width
    pub fn width(&self) -> usize {
        self.dimensions().0
    }

    /// Get the height
    pub fn height(&self) -> usize {
        self.dimensions().1
    }

    /// Check if this size is usable in the World Editor (must be 64x64)
    pub fn usable_in_world_editor(&self) -> bool {
        matches!(self, TextureSize::Size64x64)
    }

    /// Get a display label for UI
    pub fn label(&self) -> &'static str {
        match self {
            TextureSize::Size8x8 => "8x8",
            TextureSize::Size16x16 => "16x16",
            TextureSize::Size32x32 => "32x32",
            TextureSize::Size64x64 => "64x64",
            TextureSize::Size128x128 => "128x128",
            TextureSize::Size256x256 => "256x256",
        }
    }

    /// Try to determine size from dimensions
    pub fn from_dimensions(width: usize, height: usize) -> Option<TextureSize> {
        match (width, height) {
            (8, 8) => Some(TextureSize::Size8x8),
            (16, 16) => Some(TextureSize::Size16x16),
            (32, 32) => Some(TextureSize::Size32x32),
            (64, 64) => Some(TextureSize::Size64x64),
            (128, 128) => Some(TextureSize::Size128x128),
            (256, 256) => Some(TextureSize::Size256x256),
            _ => None,
        }
    }

    /// All available sizes
    pub const ALL: &'static [TextureSize] = &[
        TextureSize::Size8x8,
        TextureSize::Size16x16,
        TextureSize::Size32x32,
        TextureSize::Size64x64,
        TextureSize::Size128x128,
        TextureSize::Size256x256,
    ];

    /// Sizes available for the World Editor (64x64 only)
    pub const WORLD_EDITOR_SIZES: &'static [TextureSize] = &[TextureSize::Size64x64];
}

impl Default for TextureSize {
    fn default() -> Self {
        TextureSize::Size64x64
    }
}

/// Error type for texture operations
#[derive(Debug)]
pub enum TextureError {
    IoError(std::io::Error),
    ParseError(ron::error::SpannedError),
    SerializeError(ron::Error),
    ValidationError(String),
}

impl From<std::io::Error> for TextureError {
    fn from(e: std::io::Error) -> Self {
        TextureError::IoError(e)
    }
}

impl From<ron::error::SpannedError> for TextureError {
    fn from(e: ron::error::SpannedError) -> Self {
        TextureError::ParseError(e)
    }
}

impl From<ron::Error> for TextureError {
    fn from(e: ron::Error) -> Self {
        TextureError::SerializeError(e)
    }
}

impl std::fmt::Display for TextureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TextureError::IoError(e) => write!(f, "IO error: {}", e),
            TextureError::ParseError(e) => write!(f, "Parse error: {}", e),
            TextureError::SerializeError(e) => write!(f, "Serialize error: {}", e),
            TextureError::ValidationError(e) => write!(f, "Validation error: {}", e),
        }
    }
}

/// A user-created indexed texture with embedded palette
///
/// This is a self-contained texture asset that includes:
/// - Palette indices for each pixel
/// - RGB555 color palette (CLUT)
/// - Size and depth information
///
/// Stored as `.ron` files with Brotli compression in `assets/textures-user/`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserTexture {
    /// Human-readable name (used as filename without extension)
    pub name: String,
    /// Texture width
    pub width: usize,
    /// Texture height
    pub height: usize,
    /// CLUT depth (4-bit = 16 colors, 8-bit = 256 colors)
    pub depth: ClutDepth,
    /// Palette indices for each pixel (row-major order)
    /// Values are 0-15 for 4-bit, 0-255 for 8-bit
    pub indices: Vec<u8>,
    /// RGB555 color palette (16 entries for 4-bit, 256 for 8-bit)
    /// Index 0 is typically transparent (0x0000)
    pub palette: Vec<Color15>,
}

impl UserTexture {
    /// Create a new texture with a default grayscale palette
    pub fn new(name: impl Into<String>, size: TextureSize, depth: ClutDepth) -> Self {
        let (width, height) = size.dimensions();
        let pixel_count = width * height;
        let color_count = depth.color_count();

        // Create default grayscale palette
        let mut palette = Vec::with_capacity(color_count);
        palette.push(Color15::TRANSPARENT); // Index 0 = transparent
        for i in 1..color_count {
            let v = ((i * 31) / (color_count - 1)) as u8;
            palette.push(Color15::new(v, v, v));
        }

        // Fill indices with 0 (transparent)
        let indices = vec![0u8; pixel_count];

        Self {
            name: name.into(),
            width,
            height,
            depth,
            indices,
            palette,
        }
    }

    /// Create a new 64x64 texture (suitable for World Editor)
    pub fn new_64x64(name: impl Into<String>, depth: ClutDepth) -> Self {
        Self::new(name, TextureSize::Size64x64, depth)
    }

    /// Get the texture size enum if it matches a standard size
    pub fn size(&self) -> Option<TextureSize> {
        TextureSize::from_dimensions(self.width, self.height)
    }

    /// Check if this texture can be used in the World Editor (64x64 only)
    pub fn usable_in_world_editor(&self) -> bool {
        self.width == 64 && self.height == 64
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

    /// Get the color at pixel coordinates (looks up in palette)
    pub fn get_color(&self, x: usize, y: usize) -> Color15 {
        let index = self.get_index(x, y) as usize;
        self.palette.get(index).copied().unwrap_or(Color15::TRANSPARENT)
    }

    /// Get a palette color by index
    pub fn get_palette_color(&self, index: u8) -> Color15 {
        self.palette
            .get(index as usize)
            .copied()
            .unwrap_or(Color15::TRANSPARENT)
    }

    /// Set a palette color by index
    pub fn set_palette_color(&mut self, index: u8, color: Color15) {
        if (index as usize) < self.palette.len() {
            self.palette[index as usize] = color;
        }
    }

    /// Sample texture at UV coordinates (PS1-style, no filtering)
    pub fn sample(&self, u: f32, v: f32) -> Color15 {
        let u_wrapped = u.rem_euclid(1.0);
        let v_wrapped = v.rem_euclid(1.0);
        let tx = ((u_wrapped * self.width as f32) as usize).min(self.width.saturating_sub(1));
        let ty = ((v_wrapped * self.height as f32) as usize).min(self.height.saturating_sub(1));
        self.get_color(tx, ty)
    }

    /// Fill the entire texture with a single palette index
    pub fn fill(&mut self, index: u8) {
        let clamped = index.min(self.depth.max_index());
        for pixel in &mut self.indices {
            *pixel = clamped;
        }
    }

    /// Clear the texture (fill with index 0 = transparent)
    pub fn clear(&mut self) {
        self.fill(0);
    }

    /// Load a texture from a file (supports compressed and uncompressed RON)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, TextureError> {
        let path = path.as_ref();
        let bytes = fs::read(path)?;

        // Detect format: RON files start with '(' or whitespace, brotli is binary
        let is_plain_ron = bytes
            .first()
            .map(|&b| b == b'(' || b == b' ' || b == b'\n' || b == b'\r' || b == b'\t')
            .unwrap_or(false);

        let contents = if is_plain_ron {
            String::from_utf8(bytes).map_err(|e| {
                TextureError::IoError(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid UTF-8: {}", e),
                ))
            })?
        } else {
            // Brotli compressed - decompress first
            let mut decompressed = Vec::new();
            brotli::BrotliDecompress(&mut Cursor::new(&bytes), &mut decompressed).map_err(|e| {
                TextureError::IoError(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("brotli decompression failed: {}", e),
                ))
            })?;
            String::from_utf8(decompressed).map_err(|e| {
                TextureError::IoError(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid UTF-8 after decompression: {}", e),
                ))
            })?
        };

        let texture: UserTexture = ron::from_str(&contents)?;
        texture.validate()?;
        Ok(texture)
    }

    /// Load from bytes (for WASM async loading)
    pub fn load_from_bytes(bytes: &[u8]) -> Result<Self, TextureError> {
        // Detect format
        let is_plain_ron = bytes
            .first()
            .map(|&b| b == b'(' || b == b' ' || b == b'\n' || b == b'\r' || b == b'\t')
            .unwrap_or(false);

        let contents = if is_plain_ron {
            String::from_utf8(bytes.to_vec()).map_err(|e| {
                TextureError::IoError(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid UTF-8: {}", e),
                ))
            })?
        } else {
            let mut decompressed = Vec::new();
            brotli::BrotliDecompress(&mut Cursor::new(bytes), &mut decompressed).map_err(|e| {
                TextureError::IoError(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("brotli decompression failed: {}", e),
                ))
            })?;
            String::from_utf8(decompressed).map_err(|e| {
                TextureError::IoError(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid UTF-8 after decompression: {}", e),
                ))
            })?
        };

        let texture: UserTexture = ron::from_str(&contents)?;
        texture.validate()?;
        Ok(texture)
    }

    /// Save the texture to a file (always uses Brotli compression)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), TextureError> {
        self.validate()?;

        let config = ron::ser::PrettyConfig::new()
            .depth_limit(4)
            .indentor("  ".to_string());

        let ron_string = ron::ser::to_string_pretty(self, config)?;

        // Compress with brotli (quality 6, window 22)
        let mut compressed = Vec::new();
        brotli::BrotliCompress(
            &mut Cursor::new(ron_string.as_bytes()),
            &mut compressed,
            &brotli::enc::BrotliEncoderParams {
                quality: 6,
                lgwin: 22,
                ..Default::default()
            },
        )
        .map_err(|e| {
            TextureError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("brotli compression failed: {}", e),
            ))
        })?;

        fs::write(path, compressed)?;
        Ok(())
    }

    /// Validate the texture data
    pub fn validate(&self) -> Result<(), TextureError> {
        // Check dimensions match a valid size
        if TextureSize::from_dimensions(self.width, self.height).is_none() {
            return Err(TextureError::ValidationError(format!(
                "invalid texture size {}x{} - must be one of: 8x8, 16x16, 32x32, 64x64, 128x128, 256x256",
                self.width, self.height
            )));
        }

        // Check indices array size
        let expected_pixels = self.width * self.height;
        if self.indices.len() != expected_pixels {
            return Err(TextureError::ValidationError(format!(
                "indices array size mismatch: expected {}, got {}",
                expected_pixels,
                self.indices.len()
            )));
        }

        // Check palette size
        let expected_colors = self.depth.color_count();
        if self.palette.len() != expected_colors {
            return Err(TextureError::ValidationError(format!(
                "palette size mismatch: expected {} for {:?}, got {}",
                expected_colors, self.depth, self.palette.len()
            )));
        }

        // Check indices are within valid range
        let max_index = self.depth.max_index();
        for (i, &index) in self.indices.iter().enumerate() {
            if index > max_index {
                return Err(TextureError::ValidationError(format!(
                    "index {} at position {} exceeds max {} for {:?}",
                    index, i, max_index, self.depth
                )));
            }
        }

        // Check name is reasonable
        if self.name.is_empty() {
            return Err(TextureError::ValidationError(
                "texture name cannot be empty".to_string(),
            ));
        }
        if self.name.len() > 256 {
            return Err(TextureError::ValidationError(
                "texture name too long (max 256 chars)".to_string(),
            ));
        }

        Ok(())
    }

    /// Convert to RGBA bytes for display (4 bytes per pixel)
    pub fn to_rgba(&self) -> Vec<u8> {
        let mut rgba = Vec::with_capacity(self.width * self.height * 4);
        for y in 0..self.height {
            for x in 0..self.width {
                let color = self.get_color(x, y);
                let [r, g, b, a] = color.to_rgba();
                rgba.push(r);
                rgba.push(g);
                rgba.push(b);
                rgba.push(a);
            }
        }
        rgba
    }
}

impl Default for UserTexture {
    fn default() -> Self {
        Self::new("untitled", TextureSize::Size64x64, ClutDepth::Bpp4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_texture() {
        let tex = UserTexture::new("test", TextureSize::Size64x64, ClutDepth::Bpp4);
        assert_eq!(tex.name, "test");
        assert_eq!(tex.width, 64);
        assert_eq!(tex.height, 64);
        assert_eq!(tex.indices.len(), 64 * 64);
        assert_eq!(tex.palette.len(), 16);
        assert!(tex.palette[0].is_transparent());
    }

    #[test]
    fn test_get_set_index() {
        let mut tex = UserTexture::new("test", TextureSize::Size32x32, ClutDepth::Bpp4);
        tex.set_index(5, 10, 7);
        assert_eq!(tex.get_index(5, 10), 7);

        // Test clamping
        tex.set_index(5, 10, 20); // 20 > 15 (max for 4-bit)
        assert_eq!(tex.get_index(5, 10), 15);
    }

    #[test]
    fn test_texture_size() {
        assert_eq!(TextureSize::Size64x64.dimensions(), (64, 64));
        assert!(TextureSize::Size64x64.usable_in_world_editor());
        assert!(!TextureSize::Size32x32.usable_in_world_editor());
        assert_eq!(
            TextureSize::from_dimensions(128, 128),
            Some(TextureSize::Size128x128)
        );
        assert_eq!(TextureSize::from_dimensions(100, 100), None);
    }

    #[test]
    fn test_validation() {
        let tex = UserTexture::new("test", TextureSize::Size64x64, ClutDepth::Bpp4);
        assert!(tex.validate().is_ok());

        // Empty name should fail
        let mut bad_tex = tex.clone();
        bad_tex.name = String::new();
        assert!(bad_tex.validate().is_err());
    }
}
