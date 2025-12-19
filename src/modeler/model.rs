//! Animation and texture atlas types for rigged models
//!
//! Note: The main model types (RiggedModel, RigBone, MeshPart) are in state.rs.
//! This file contains supporting types for animation and texture management.

use serde::{Deserialize, Serialize};
use crate::rasterizer::{Vec3, Color};

// ============================================================================
// Texture Atlas
// ============================================================================

/// Texture atlas (single texture per model)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextureAtlas {
    pub size: AtlasSize,
    pub pixels: Vec<u8>, // RGBA data
}

impl TextureAtlas {
    pub fn new(size: AtlasSize) -> Self {
        let dim = size as usize;
        // Initialize with checkerboard pattern
        let mut pixels = Vec::with_capacity(dim * dim * 4);
        for y in 0..dim {
            for x in 0..dim {
                let checker = ((x / 8) + (y / 8)) % 2 == 0;
                if checker {
                    pixels.extend_from_slice(&[200, 200, 200, 255]);
                } else {
                    pixels.extend_from_slice(&[150, 150, 150, 255]);
                }
            }
        }
        Self { size, pixels }
    }

    pub fn dimension(&self) -> usize {
        self.size as usize
    }

    /// Get pixel color at coordinates
    pub fn get_pixel(&self, x: usize, y: usize) -> Color {
        let dim = self.dimension();
        if x >= dim || y >= dim {
            return Color::BLACK;
        }
        let idx = (y * dim + x) * 4;
        Color::with_alpha(
            self.pixels[idx],
            self.pixels[idx + 1],
            self.pixels[idx + 2],
            self.pixels[idx + 3],
        )
    }

    /// Set pixel color at coordinates
    pub fn set_pixel(&mut self, x: usize, y: usize, color: Color) {
        let dim = self.dimension();
        if x >= dim || y >= dim {
            return;
        }
        let idx = (y * dim + x) * 4;
        self.pixels[idx] = color.r;
        self.pixels[idx + 1] = color.g;
        self.pixels[idx + 2] = color.b;
        self.pixels[idx + 3] = color.a;
    }

    /// Sample texture at UV coordinates (no filtering - PS1 style)
    pub fn sample(&self, u: f32, v: f32) -> Color {
        let dim = self.dimension();
        let x = ((u * dim as f32) as usize) % dim;
        let y = ((v * dim as f32) as usize) % dim;
        self.get_pixel(x, y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(usize)]
pub enum AtlasSize {
    S64 = 64,
    S128 = 128,
    S256 = 256,
    S512 = 512,
}

impl AtlasSize {
    pub fn all() -> [AtlasSize; 4] {
        [AtlasSize::S64, AtlasSize::S128, AtlasSize::S256, AtlasSize::S512]
    }

    pub fn label(&self) -> &'static str {
        match self {
            AtlasSize::S64 => "64x64",
            AtlasSize::S128 => "128x128",
            AtlasSize::S256 => "256x256",
            AtlasSize::S512 => "512x512",
        }
    }
}

// ============================================================================
// Animation (for Step 9: Animation Keyframes)
// ============================================================================

/// Named animation clip
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Animation {
    pub name: String,
    pub fps: u8,
    pub looping: bool,
    pub keyframes: Vec<Keyframe>,
}

impl Animation {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            fps: 15,
            looping: true,
            keyframes: Vec::new(),
        }
    }

    /// Get the last frame number
    pub fn last_frame(&self) -> u32 {
        self.keyframes.last().map(|kf| kf.frame).unwrap_or(0)
    }

    /// Duration in seconds
    pub fn duration(&self) -> f32 {
        self.last_frame() as f32 / self.fps as f32
    }

    /// Find keyframe at exact frame, or None
    pub fn get_keyframe(&self, frame: u32) -> Option<&Keyframe> {
        self.keyframes.iter().find(|kf| kf.frame == frame)
    }

    /// Find keyframe at exact frame mutably
    pub fn get_keyframe_mut(&mut self, frame: u32) -> Option<&mut Keyframe> {
        self.keyframes.iter_mut().find(|kf| kf.frame == frame)
    }

    /// Insert or update keyframe
    pub fn set_keyframe(&mut self, keyframe: Keyframe) {
        let frame = keyframe.frame;
        if let Some(existing) = self.get_keyframe_mut(frame) {
            *existing = keyframe;
        } else {
            self.keyframes.push(keyframe);
            self.keyframes.sort_by_key(|kf| kf.frame);
        }
    }

    /// Remove keyframe at frame
    pub fn remove_keyframe(&mut self, frame: u32) {
        self.keyframes.retain(|kf| kf.frame != frame);
    }
}

/// Single keyframe (stores transform for each bone)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keyframe {
    pub frame: u32,
    pub transforms: Vec<BoneTransform>,
}

impl Keyframe {
    pub fn new(frame: u32, num_bones: usize) -> Self {
        Self {
            frame,
            transforms: vec![BoneTransform::default(); num_bones],
        }
    }
}

/// Local transform for a bone at a keyframe
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct BoneTransform {
    pub position: Vec3,
    pub rotation: Vec3, // Euler angles in degrees
}

impl BoneTransform {
    pub fn new(position: Vec3, rotation: Vec3) -> Self {
        Self { position, rotation }
    }

    /// Linearly interpolate between two transforms
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        Self {
            position: Vec3::new(
                self.position.x + (other.position.x - self.position.x) * t,
                self.position.y + (other.position.y - self.position.y) * t,
                self.position.z + (other.position.z - self.position.z) * t,
            ),
            rotation: Vec3::new(
                self.rotation.x + (other.rotation.x - self.rotation.x) * t,
                self.rotation.y + (other.rotation.y - self.rotation.y) * t,
                self.rotation.z + (other.rotation.z - self.rotation.z) * t,
            ),
        }
    }
}
