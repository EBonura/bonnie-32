//! User texture asset - independent indexed textures with embedded palette
//!
//! UserTexture combines an indexed texture with its CLUT (palette) into a single
//! self-contained asset. Supports 4-bit (16 colors) and 8-bit (256 colors) modes.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::path::Path;
use std::fs;

use crate::rasterizer::{BlendMode, ClutDepth, Color15};

/// Counter for generating unique texture IDs
static TEXTURE_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a unique texture ID
pub fn generate_texture_id() -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::SystemTime;

    let counter = TEXTURE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);

    let time_bits = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);

    let mut hasher = DefaultHasher::new();
    time_bits.hash(&mut hasher);
    counter.hash(&mut hasher);
    hasher.finish()
}

/// Source/origin of a texture
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextureSource {
    /// Bundled sample texture (read-only)
    Sample,
    /// User-created texture (editable)
    #[default]
    User,
}

/// Valid texture sizes for user textures
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TextureSize {
    Size8x8,
    Size16x16,
    Size32x32,
    Size64x64,
    Size128x128,
    Size256x256,
}

impl TextureSize {
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

    pub fn width(&self) -> usize {
        self.dimensions().0
    }

    pub fn height(&self) -> usize {
        self.dimensions().1
    }

    pub fn usable_in_world_editor(&self) -> bool {
        matches!(self, TextureSize::Size32x32 | TextureSize::Size64x64 | TextureSize::Size128x128)
    }

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

    pub const ALL: &'static [TextureSize] = &[
        TextureSize::Size8x8,
        TextureSize::Size16x16,
        TextureSize::Size32x32,
        TextureSize::Size64x64,
        TextureSize::Size128x128,
        TextureSize::Size256x256,
    ];

    pub const WORLD_EDITOR_SIZES: &'static [TextureSize] = &[
        TextureSize::Size32x32,
        TextureSize::Size64x64,
        TextureSize::Size128x128,
    ];
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
/// Includes palette indices for each pixel, RGB555 color palette (CLUT),
/// and size/depth information. Stored as plain `.ron` files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserTexture {
    #[serde(default = "generate_texture_id")]
    pub id: u64,
    pub name: String,
    pub width: usize,
    pub height: usize,
    pub depth: ClutDepth,
    pub indices: Vec<u8>,
    pub palette: Vec<Color15>,
    #[serde(default)]
    pub blend_mode: BlendMode,
    #[serde(skip)]
    pub source: TextureSource,
}

impl UserTexture {
    pub fn content_hash(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        self.width.hash(&mut hasher);
        self.height.hash(&mut hasher);
        (self.depth as u8).hash(&mut hasher);
        self.indices.hash(&mut hasher);
        for color in &self.palette {
            color.0.hash(&mut hasher);
        }
        hasher.finish()
    }

    pub fn new(name: impl Into<String>, size: TextureSize, depth: ClutDepth) -> Self {
        let (width, height) = size.dimensions();
        let pixel_count = width * height;
        let color_count = depth.color_count();

        let mut palette = Vec::with_capacity(color_count);
        palette.push(Color15::TRANSPARENT);
        for i in 1..color_count {
            let v = ((i * 31) / (color_count - 1)) as u8;
            palette.push(Color15::new(v, v, v));
        }

        let indices = vec![0u8; pixel_count];

        Self {
            id: generate_texture_id(),
            name: name.into(),
            width,
            height,
            depth,
            indices,
            palette,
            blend_mode: BlendMode::Opaque,
            source: TextureSource::User,
        }
    }

    pub fn new_64x64(name: impl Into<String>, depth: ClutDepth) -> Self {
        Self::new(name, TextureSize::Size64x64, depth)
    }

    pub fn new_with_data(
        name: impl Into<String>,
        size: TextureSize,
        depth: ClutDepth,
        indices: Vec<u8>,
        palette: Vec<Color15>,
    ) -> Self {
        let (width, height) = size.dimensions();
        Self {
            id: generate_texture_id(),
            name: name.into(),
            width,
            height,
            depth,
            indices,
            palette,
            blend_mode: BlendMode::Opaque,
            source: TextureSource::User,
        }
    }

    pub fn size(&self) -> Option<TextureSize> {
        TextureSize::from_dimensions(self.width, self.height)
    }

    pub fn usable_in_world_editor(&self) -> bool {
        self.width == 64 && self.height == 64
    }

    pub fn get_index(&self, x: usize, y: usize) -> u8 {
        if x < self.width && y < self.height {
            self.indices.get(y * self.width + x).copied().unwrap_or(0)
        } else {
            0
        }
    }

    pub fn set_index(&mut self, x: usize, y: usize, index: u8) {
        if x < self.width && y < self.height {
            let clamped = index.min(self.depth.max_index());
            if let Some(pixel) = self.indices.get_mut(y * self.width + x) {
                *pixel = clamped;
            }
        }
    }

    pub fn get_color(&self, x: usize, y: usize) -> Color15 {
        let index = self.get_index(x, y) as usize;
        self.palette.get(index).copied().unwrap_or(Color15::TRANSPARENT)
    }

    pub fn get_palette_color(&self, index: u8) -> Color15 {
        self.palette
            .get(index as usize)
            .copied()
            .unwrap_or(Color15::TRANSPARENT)
    }

    pub fn set_palette_color(&mut self, index: u8, color: Color15) {
        if (index as usize) < self.palette.len() {
            self.palette[index as usize] = color;
        }
    }

    pub fn sample(&self, u: f32, v: f32) -> Color15 {
        let u_wrapped = u.rem_euclid(1.0);
        let v_wrapped = v.rem_euclid(1.0);
        let tx = ((u_wrapped * self.width as f32) as usize).min(self.width.saturating_sub(1));
        let ty = ((v_wrapped * self.height as f32) as usize).min(self.height.saturating_sub(1));
        self.get_color(tx, ty)
    }

    pub fn fill(&mut self, index: u8) {
        let clamped = index.min(self.depth.max_index());
        for pixel in &mut self.indices {
            *pixel = clamped;
        }
    }

    pub fn clear(&mut self) {
        self.fill(0);
    }

    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, TextureError> {
        let bytes = fs::read(path)?;
        let contents = String::from_utf8(bytes).map_err(|e| {
            TextureError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid UTF-8: {}", e),
            ))
        })?;
        let texture: UserTexture = ron::from_str(&contents)?;
        texture.validate()?;
        Ok(texture)
    }

    pub fn load_from_bytes(bytes: &[u8]) -> Result<Self, TextureError> {
        let contents = String::from_utf8(bytes.to_vec()).map_err(|e| {
            TextureError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid UTF-8: {}", e),
            ))
        })?;
        let texture: UserTexture = ron::from_str(&contents)?;
        texture.validate()?;
        Ok(texture)
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), TextureError> {
        self.validate()?;

        let config = ron::ser::PrettyConfig::new()
            .depth_limit(4)
            .indentor("  ".to_string());

        let ron_string = ron::ser::to_string_pretty(self, config)?;
        fs::write(path, ron_string)?;
        Ok(())
    }

    pub fn to_ron_string(&self) -> Result<String, TextureError> {
        self.validate()?;
        let config = ron::ser::PrettyConfig::new()
            .depth_limit(4)
            .indentor("  ".to_string());
        let ron_string = ron::ser::to_string_pretty(self, config)?;
        Ok(ron_string)
    }

    pub fn validate(&self) -> Result<(), TextureError> {
        if TextureSize::from_dimensions(self.width, self.height).is_none() {
            return Err(TextureError::ValidationError(format!(
                "invalid texture size {}x{}", self.width, self.height
            )));
        }

        let expected_pixels = self.width * self.height;
        if self.indices.len() != expected_pixels {
            return Err(TextureError::ValidationError(format!(
                "indices array size mismatch: expected {}, got {}",
                expected_pixels, self.indices.len()
            )));
        }

        let expected_colors = self.depth.color_count();
        if self.palette.len() != expected_colors {
            return Err(TextureError::ValidationError(format!(
                "palette size mismatch: expected {} for {:?}, got {}",
                expected_colors, self.depth, self.palette.len()
            )));
        }

        let max_index = self.depth.max_index();
        for (i, &index) in self.indices.iter().enumerate() {
            if index > max_index {
                return Err(TextureError::ValidationError(format!(
                    "index {} at position {} exceeds max {} for {:?}",
                    index, i, max_index, self.depth
                )));
            }
        }

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

    pub fn convert_to_4bit(&mut self) -> usize {
        if self.depth == ClutDepth::Bpp4 {
            return 0;
        }

        let affected = self.indices.iter().filter(|&&i| i > 15).count();

        for idx in &mut self.indices {
            *idx = *idx % 16;
        }

        self.palette.truncate(16);
        self.depth = ClutDepth::Bpp4;

        affected
    }

    pub fn convert_to_8bit(&mut self) {
        if self.depth == ClutDepth::Bpp8 {
            return;
        }

        while self.palette.len() < 256 {
            let i = self.palette.len();
            let v = ((i - 16) * 31 / 239) as u8;
            self.palette.push(Color15::new(v, v, v));
        }

        self.depth = ClutDepth::Bpp8;
    }

    pub fn count_high_indices(&self) -> usize {
        if self.depth == ClutDepth::Bpp4 {
            return 0;
        }
        self.indices.iter().filter(|&&i| i > 15).count()
    }

    pub fn to_raster_texture(&self) -> crate::rasterizer::Texture {
        use crate::rasterizer::{Texture as RasterTexture, Color as RasterColor};

        let tex_blend = self.blend_mode;

        let pixels: Vec<RasterColor> = (0..self.height)
            .flat_map(|y| {
                (0..self.width).map(move |x| {
                    let color = self.get_color(x, y);
                    if color.is_transparent() {
                        RasterColor::with_blend(0, 0, 0, BlendMode::Erase)
                    } else {
                        let [r, g, b, _] = color.to_rgba();
                        if color.is_semi_transparent() {
                            RasterColor::with_blend(r, g, b, tex_blend)
                        } else {
                            RasterColor::new(r, g, b)
                        }
                    }
                })
            })
            .collect();

        RasterTexture {
            width: self.width,
            height: self.height,
            pixels,
            name: self.name.clone(),
            blend_mode: self.blend_mode,
        }
    }

    pub fn to_raster_texture_15(&self) -> crate::rasterizer::Texture15 {
        use crate::rasterizer::Texture15;

        let pixels: Vec<Color15> = (0..self.height)
            .flat_map(|y| {
                (0..self.width).map(move |x| self.get_color(x, y))
            })
            .collect();

        Texture15 {
            width: self.width,
            height: self.height,
            pixels,
            name: self.name.clone(),
            blend_mode: self.blend_mode,
        }
    }

    pub fn is_sample(&self) -> bool {
        self.source == TextureSource::Sample
    }

    pub fn is_user_texture(&self) -> bool {
        self.source == TextureSource::User
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

        tex.set_index(5, 10, 20); // 20 > 15 (max for 4-bit)
        assert_eq!(tex.get_index(5, 10), 15);
    }

    #[test]
    fn test_texture_size() {
        assert_eq!(TextureSize::Size64x64.dimensions(), (64, 64));
        assert!(TextureSize::Size64x64.usable_in_world_editor());
        assert!(!TextureSize::Size8x8.usable_in_world_editor());
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

        let mut bad_tex = tex.clone();
        bad_tex.name = String::new();
        assert!(bad_tex.validate().is_err());
    }

    #[test]
    fn test_content_hash_stable() {
        let tex = UserTexture::new("test", TextureSize::Size32x32, ClutDepth::Bpp4);
        let hash1 = tex.content_hash();
        let hash2 = tex.content_hash();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_depth_conversion() {
        let mut tex = UserTexture::new("test", TextureSize::Size32x32, ClutDepth::Bpp4);
        assert_eq!(tex.palette.len(), 16);

        tex.convert_to_8bit();
        assert_eq!(tex.depth, ClutDepth::Bpp8);
        assert_eq!(tex.palette.len(), 256);

        let affected = tex.convert_to_4bit();
        assert_eq!(affected, 0); // no pixels used indices > 15
        assert_eq!(tex.depth, ClutDepth::Bpp4);
        assert_eq!(tex.palette.len(), 16);
    }
}
