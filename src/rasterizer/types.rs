//! Core types for the rasterizer

use super::math::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

/// RGBA color (0-255 per channel)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Neutral color for PS1 texture modulation (128, 128, 128)
    /// When used with modulate(), texture colors remain unchanged
    pub const NEUTRAL: Color = Color { r: 128, g: 128, b: 128, a: 255 };

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

    /// PS1-style texture modulation: (self * vertex_color) / 128
    /// vertex_color of 128 = no change, >128 = brighten, <128 = darken
    /// This is the authentic PS1 formula for combining textures with vertex colors
    pub fn modulate(self, vertex_color: Color) -> Self {
        Self {
            r: ((self.r as u16 * vertex_color.r as u16) / 128).min(255) as u8,
            g: ((self.g as u16 * vertex_color.g as u16) / 128).min(255) as u8,
            b: ((self.b as u16 * vertex_color.b as u16) / 128).min(255) as u8,
            a: self.a,
        }
    }

    /// Interpolate between two colors
    pub fn lerp(self, other: Color, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let inv_t = 1.0 - t;
        Self {
            r: (self.r as f32 * inv_t + other.r as f32 * t) as u8,
            g: (self.g as f32 * inv_t + other.g as f32 * t) as u8,
            b: (self.b as f32 * inv_t + other.b as f32 * t) as u8,
            a: (self.a as f32 * inv_t + other.a as f32 * t) as u8,
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

    /// PS1-style blend: combine this color (front) with back color using blend mode
    pub fn blend(self, back: Color, mode: BlendMode) -> Color {
        match mode {
            BlendMode::Opaque => self,
            BlendMode::Average => {
                // Mode 0: 0.5*B + 0.5*F
                Color::with_alpha(
                    ((back.r as u16 + self.r as u16) / 2) as u8,
                    ((back.g as u16 + self.g as u16) / 2) as u8,
                    ((back.b as u16 + self.b as u16) / 2) as u8,
                    self.a,
                )
            }
            BlendMode::Add => {
                // Mode 1: B + F (clamped to 255)
                Color::with_alpha(
                    (back.r as u16 + self.r as u16).min(255) as u8,
                    (back.g as u16 + self.g as u16).min(255) as u8,
                    (back.b as u16 + self.b as u16).min(255) as u8,
                    self.a,
                )
            }
            BlendMode::Subtract => {
                // Mode 2: B - F (clamped to 0)
                Color::with_alpha(
                    (back.r as i16 - self.r as i16).max(0) as u8,
                    (back.g as i16 - self.g as i16).max(0) as u8,
                    (back.b as i16 - self.b as i16).max(0) as u8,
                    self.a,
                )
            }
            BlendMode::AddQuarter => {
                // Mode 3: B + 0.25*F (clamped to 255)
                Color::with_alpha(
                    (back.r as u16 + self.r as u16 / 4).min(255) as u8,
                    (back.g as u16 + self.g as u16 / 4).min(255) as u8,
                    (back.b as u16 + self.b as u16 / 4).min(255) as u8,
                    self.a,
                )
            }
        }
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
    /// Show wireframe overlay on front-facing polygons
    pub wireframe_overlay: bool,
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
            vertex_snap: true,      // PS1 default: jittery vertices
            use_zbuffer: true,
            shading: ShadingMode::Gouraud,
            backface_cull: true,
            backface_wireframe: true, // Editor default: show backfaces as wireframe
            lights: vec![Light::directional(Vec3::new(-1.0, -1.0, -1.0), 0.7)],
            ambient: 0.3,
            low_resolution: true,   // PS1 default: 320x240
            dithering: true,        // PS1 default: ordered dithering enabled
            stretch_to_fill: true,  // Default: use full viewport space
            wireframe_overlay: false, // Default: wireframe off
        }
    }
}
