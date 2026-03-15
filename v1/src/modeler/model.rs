//! Animation types for rigged models
//!
//! Note: The main model types (RiggedModel, RigBone, RigPart) are in state.rs.
//! IndexedAtlas is in mesh_editor.rs.

use serde::{Deserialize, Serialize};
use crate::rasterizer::Vec3;

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
