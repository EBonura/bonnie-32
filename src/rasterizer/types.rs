//! Core types for the rasterizer

use super::math::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

/// RGBA color (0-255 per channel)
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 255 };
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 255 };
    pub const RED: Color = Color { r: 255, g: 0, b: 0, a: 255 };
    pub const GREEN: Color = Color { r: 0, g: 255, b: 0, a: 255 };
    pub const BLUE: Color = Color { r: 0, g: 0, b: 255, a: 255 };

    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub fn with_alpha(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Apply shading (multiply by intensity 0.0-1.0)
    pub fn shade(self, intensity: f32) -> Self {
        let i = intensity.clamp(0.0, 1.0);
        Self {
            r: (self.r as f32 * i) as u8,
            g: (self.g as f32 * i) as u8,
            b: (self.b as f32 * i) as u8,
            a: self.a,
        }
    }

    /// Convert to u32 (RGBA format for macroquad)
    pub fn to_u32(self) -> u32 {
        ((self.r as u32) << 24) | ((self.g as u32) << 16) | ((self.b as u32) << 8) | (self.a as u32)
    }

    /// Convert to [u8; 4] for framebuffer
    pub fn to_bytes(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

/// A vertex with position, texture coordinate, and normal
#[derive(Debug, Clone, Copy, Default)]
pub struct Vertex {
    pub pos: Vec3,
    pub uv: Vec2,
    pub normal: Vec3,
}

impl Vertex {
    pub fn new(pos: Vec3, uv: Vec2, normal: Vec3) -> Self {
        Self { pos, uv, normal }
    }

    pub fn from_pos(x: f32, y: f32, z: f32) -> Self {
        Self {
            pos: Vec3::new(x, y, z),
            uv: Vec2::default(),
            normal: Vec3::ZERO,
        }
    }
}

/// A triangle face (indices into vertex array)
#[derive(Debug, Clone, Copy)]
pub struct Face {
    pub v0: usize,
    pub v1: usize,
    pub v2: usize,
    pub texture_id: Option<usize>,
}

impl Face {
    pub fn new(v0: usize, v1: usize, v2: usize) -> Self {
        Self {
            v0,
            v1,
            v2,
            texture_id: None,
        }
    }

    pub fn with_texture(v0: usize, v1: usize, v2: usize, texture_id: usize) -> Self {
        Self {
            v0,
            v1,
            v2,
            texture_id: Some(texture_id),
        }
    }
}

/// Simple texture (array of colors)
#[derive(Debug, Clone)]
pub struct Texture {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<Color>,
    pub name: String,
}

impl Texture {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![Color::WHITE; width * height],
            name: String::new(),
        }
    }

    /// Load texture from a PNG file
    pub fn from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, String> {
        use image::GenericImageView;

        let path = path.as_ref();
        let img = image::open(path)
            .map_err(|e| format!("Failed to load {}: {}", path.display(), e))?;

        let (width, height) = img.dimensions();
        let rgba = img.to_rgba8();

        let pixels: Vec<Color> = rgba
            .pixels()
            .map(|p| Color::with_alpha(p[0], p[1], p[2], p[3]))
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
        })
    }

    /// Load all textures from a directory
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
                match Self::from_file(&path) {
                    Ok(tex) => {
                        println!("Loaded texture: {} ({}x{})", tex.name, tex.width, tex.height);
                        textures.push(tex);
                    }
                    Err(e) => {
                        eprintln!("{}", e);
                    }
                }
            }
        }

        textures
    }

    /// Load texture from raw PNG bytes
    pub fn from_bytes(bytes: &[u8], name: String) -> Result<Self, String> {
        use image::GenericImageView;

        let img = image::load_from_memory(bytes)
            .map_err(|e| format!("Failed to decode image: {}", e))?;

        let (width, height) = img.dimensions();
        let rgba = img.to_rgba8();

        let pixels: Vec<Color> = rgba
            .pixels()
            .map(|p| Color::with_alpha(p[0], p[1], p[2], p[3]))
            .collect();

        Ok(Self {
            width: width as usize,
            height: height as usize,
            pixels,
            name,
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
        Self { width, height, pixels, name: "checkerboard".to_string() }
    }

    /// Sample texture at UV coordinates (no filtering - PS1 style)
    pub fn sample(&self, u: f32, v: f32) -> Color {
        let tx = ((u * self.width as f32) as usize) % self.width;
        let ty = ((v * self.height as f32) as usize) % self.height;
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
}

/// Shading mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadingMode {
    None,     // No shading, raw texture/vertex colors
    Flat,     // One light calculation per face
    Gouraud,  // Interpolate vertex colors (PS1 style)
}

/// Rasterizer settings
#[derive(Debug, Clone)]
pub struct RasterSettings {
    /// Use affine texture mapping (true = PS1 warping, false = perspective correct)
    pub affine_textures: bool,
    /// Snap vertices to integer coordinates (PS1 jitter)
    pub vertex_snap: bool,
    /// Use Z-buffer (false = painter's algorithm)
    pub use_zbuffer: bool,
    /// Shading mode
    pub shading: ShadingMode,
    /// Backface culling
    pub backface_cull: bool,
    /// Light direction (for shading)
    pub light_dir: Vec3,
    /// Ambient light intensity (0.0-1.0)
    pub ambient: f32,
    /// Use PS1 low resolution (320x240) instead of high resolution
    pub low_resolution: bool,
    /// Enable PS1-style ordered dithering (4x4 Bayer matrix)
    pub dithering: bool,
}

impl Default for RasterSettings {
    fn default() -> Self {
        Self {
            affine_textures: true,  // PS1 default: affine (warpy)
            vertex_snap: true,      // PS1 default: jittery vertices
            use_zbuffer: true,
            shading: ShadingMode::Gouraud,
            backface_cull: true,
            light_dir: Vec3::new(-1.0, -1.0, -1.0).normalize(),
            ambient: 0.3,
            low_resolution: true,   // PS1 default: 320x240
            dithering: true,        // PS1 default: ordered dithering enabled
        }
    }
}
