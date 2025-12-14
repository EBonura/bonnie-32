//! Model data structures for segmented/hierarchy animation
//!
//! PS1-style rigid binding: each mesh part is bound to exactly ONE bone.
//! No vertex weights - the entire part transforms with its bone.

use serde::{Deserialize, Serialize};
use crate::rasterizer::{Vec2, Vec3, Color};

// ============================================================================
// Skeleton (Bones)
// ============================================================================

/// A bone in the skeleton hierarchy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bone {
    pub name: String,
    /// Parent bone index (None = root bone)
    pub parent: Option<usize>,
    /// Local position relative to parent (bind pose)
    pub local_position: Vec3,
    /// Local rotation in degrees (bind pose)
    pub local_rotation: Vec3,
    /// Length of bone for visualization (tip extends along local Y axis)
    pub length: f32,
}

impl Bone {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            parent: None,
            local_position: Vec3::ZERO,
            local_rotation: Vec3::ZERO,
            length: 20.0,
        }
    }

    pub fn with_parent(name: &str, parent: usize) -> Self {
        Self {
            name: name.to_string(),
            parent: Some(parent),
            local_position: Vec3::ZERO,
            local_rotation: Vec3::ZERO,
            length: 20.0,
        }
    }
}

/// A segmented 3D model (PS1-style hierarchy animation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub name: String,
    pub bones: Vec<Bone>,
    pub parts: Vec<ModelPart>,
    pub animations: Vec<Animation>,
    pub atlas: TextureAtlas,
}

impl Model {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            bones: Vec::new(),
            parts: Vec::new(),
            animations: Vec::new(),
            atlas: TextureAtlas::new(AtlasSize::S128),
        }
    }

    /// Add a bone and return its index
    pub fn add_bone(&mut self, bone: Bone) -> usize {
        let idx = self.bones.len();
        self.bones.push(bone);
        idx
    }

    /// Get root bones (no parent)
    pub fn root_bones(&self) -> Vec<usize> {
        self.bones
            .iter()
            .enumerate()
            .filter(|(_, b)| b.parent.is_none())
            .map(|(i, _)| i)
            .collect()
    }

    /// Get children of a bone
    pub fn bone_children(&self, parent_index: usize) -> Vec<usize> {
        self.bones
            .iter()
            .enumerate()
            .filter(|(_, b)| b.parent == Some(parent_index))
            .map(|(i, _)| i)
            .collect()
    }

    /// Create a simple test cube model
    pub fn test_cube() -> Self {
        let mut model = Self::new("cube");

        // Add a root bone for the cube
        let root_bone = model.add_bone(Bone {
            name: "root".to_string(),
            parent: None,
            local_position: Vec3::new(0.0, 50.0, 0.0),
            local_rotation: Vec3::ZERO,
            length: 50.0,
        });

        // Single part: a cube bound to root bone
        let mut cube = ModelPart::new("cube");
        cube.bone_index = Some(root_bone);
        cube.vertices = vec![
            // Front face
            ModelVertex::new(Vec3::new(-50.0, -50.0,  50.0), Vec2::new(0.0, 0.0)),
            ModelVertex::new(Vec3::new( 50.0, -50.0,  50.0), Vec2::new(0.25, 0.0)),
            ModelVertex::new(Vec3::new( 50.0,  50.0,  50.0), Vec2::new(0.25, 0.25)),
            ModelVertex::new(Vec3::new(-50.0,  50.0,  50.0), Vec2::new(0.0, 0.25)),
            // Back face
            ModelVertex::new(Vec3::new(-50.0, -50.0, -50.0), Vec2::new(0.25, 0.0)),
            ModelVertex::new(Vec3::new( 50.0, -50.0, -50.0), Vec2::new(0.5, 0.0)),
            ModelVertex::new(Vec3::new( 50.0,  50.0, -50.0), Vec2::new(0.5, 0.25)),
            ModelVertex::new(Vec3::new(-50.0,  50.0, -50.0), Vec2::new(0.25, 0.25)),
        ];
        cube.faces = vec![
            // Front
            ModelFace::new([0, 1, 2]),
            ModelFace::new([0, 2, 3]),
            // Back
            ModelFace::new([5, 4, 7]),
            ModelFace::new([5, 7, 6]),
            // Top
            ModelFace::new([3, 2, 6]),
            ModelFace::new([3, 6, 7]),
            // Bottom
            ModelFace::new([4, 5, 1]),
            ModelFace::new([4, 1, 0]),
            // Right
            ModelFace::new([1, 5, 6]),
            ModelFace::new([1, 6, 2]),
            // Left
            ModelFace::new([4, 0, 3]),
            ModelFace::new([4, 3, 7]),
        ];

        model.parts.push(cube);
        model
    }

    /// Create a humanoid skeleton for testing bone hierarchy
    pub fn test_humanoid() -> Self {
        let mut model = Self::new("humanoid");

        // Root at pelvis (bone 0)
        let pelvis = model.add_bone(Bone {
            name: "pelvis".to_string(),
            parent: None,
            local_position: Vec3::new(0.0, 60.0, 0.0),
            local_rotation: Vec3::ZERO,
            length: 15.0,
        });

        // Spine (bone 1)
        let spine = model.add_bone(Bone {
            name: "spine".to_string(),
            parent: Some(pelvis),
            local_position: Vec3::new(0.0, 15.0, 0.0),
            local_rotation: Vec3::ZERO,
            length: 25.0,
        });

        // Head (bone 2)
        let _head = model.add_bone(Bone {
            name: "head".to_string(),
            parent: Some(spine),
            local_position: Vec3::new(0.0, 25.0, 0.0),
            local_rotation: Vec3::ZERO,
            length: 20.0,
        });

        // Left arm (bone 3)
        let l_arm = model.add_bone(Bone {
            name: "l_arm".to_string(),
            parent: Some(spine),
            local_position: Vec3::new(-15.0, 20.0, 0.0),
            local_rotation: Vec3::new(0.0, 0.0, -90.0),
            length: 25.0,
        });

        // Left forearm (bone 4)
        let _l_forearm = model.add_bone(Bone {
            name: "l_forearm".to_string(),
            parent: Some(l_arm),
            local_position: Vec3::new(0.0, 25.0, 0.0),
            local_rotation: Vec3::ZERO,
            length: 20.0,
        });

        // Right arm (bone 5)
        let r_arm = model.add_bone(Bone {
            name: "r_arm".to_string(),
            parent: Some(spine),
            local_position: Vec3::new(15.0, 20.0, 0.0),
            local_rotation: Vec3::new(0.0, 0.0, 90.0),
            length: 25.0,
        });

        // Right forearm (bone 6)
        let _r_forearm = model.add_bone(Bone {
            name: "r_forearm".to_string(),
            parent: Some(r_arm),
            local_position: Vec3::new(0.0, 25.0, 0.0),
            local_rotation: Vec3::ZERO,
            length: 20.0,
        });

        // Left leg (bone 7)
        let l_leg = model.add_bone(Bone {
            name: "l_leg".to_string(),
            parent: Some(pelvis),
            local_position: Vec3::new(-10.0, 0.0, 0.0),
            local_rotation: Vec3::new(0.0, 0.0, 180.0),
            length: 30.0,
        });

        // Left shin (bone 8)
        let _l_shin = model.add_bone(Bone {
            name: "l_shin".to_string(),
            parent: Some(l_leg),
            local_position: Vec3::new(0.0, 30.0, 0.0),
            local_rotation: Vec3::ZERO,
            length: 25.0,
        });

        // Right leg (bone 9)
        let r_leg = model.add_bone(Bone {
            name: "r_leg".to_string(),
            parent: Some(pelvis),
            local_position: Vec3::new(10.0, 0.0, 0.0),
            local_rotation: Vec3::new(0.0, 0.0, 180.0),
            length: 30.0,
        });

        // Right shin (bone 10)
        let _r_shin = model.add_bone(Bone {
            name: "r_shin".to_string(),
            parent: Some(r_leg),
            local_position: Vec3::new(0.0, 30.0, 0.0),
            local_rotation: Vec3::ZERO,
            length: 25.0,
        });

        model
    }

    /// Get part by index
    pub fn get_part(&self, index: usize) -> Option<&ModelPart> {
        self.parts.get(index)
    }

    /// Get part mutably by index
    pub fn get_part_mut(&mut self, index: usize) -> Option<&mut ModelPart> {
        self.parts.get_mut(index)
    }

    /// Get children of a part
    pub fn get_children(&self, parent_index: usize) -> Vec<usize> {
        self.parts
            .iter()
            .enumerate()
            .filter(|(_, p)| p.parent == Some(parent_index))
            .map(|(i, _)| i)
            .collect()
    }

    /// Get root parts (no parent)
    pub fn get_roots(&self) -> Vec<usize> {
        self.parts
            .iter()
            .enumerate()
            .filter(|(_, p)| p.parent.is_none())
            .map(|(i, _)| i)
            .collect()
    }

    /// Total vertex count across all parts
    pub fn vertex_count(&self) -> usize {
        self.parts.iter().map(|p| p.vertices.len()).sum()
    }

    /// Total face count across all parts
    pub fn face_count(&self) -> usize {
        self.parts.iter().map(|p| p.faces.len()).sum()
    }
}

/// A single part of the model (its own mesh + transform)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPart {
    pub name: String,
    pub parent: Option<usize>,
    pub pivot: Vec3,
    pub vertices: Vec<ModelVertex>,
    pub faces: Vec<ModelFace>,
    pub visible: bool,
    /// Which bone this part is rigidly bound to (PS1-style - no weights)
    pub bone_index: Option<usize>,
}

impl ModelPart {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            parent: None,
            pivot: Vec3::ZERO,
            vertices: Vec::new(),
            faces: Vec::new(),
            visible: true,
            bone_index: None,
        }
    }

    /// Calculate bounding box of this part
    pub fn bounds(&self) -> (Vec3, Vec3) {
        if self.vertices.is_empty() {
            return (Vec3::ZERO, Vec3::ZERO);
        }

        let mut min = self.vertices[0].position;
        let mut max = self.vertices[0].position;

        for v in &self.vertices {
            min.x = min.x.min(v.position.x);
            min.y = min.y.min(v.position.y);
            min.z = min.z.min(v.position.z);
            max.x = max.x.max(v.position.x);
            max.y = max.y.max(v.position.y);
            max.z = max.z.max(v.position.z);
        }

        (min, max)
    }

    /// Calculate center of this part
    pub fn center(&self) -> Vec3 {
        let (min, max) = self.bounds();
        Vec3::new(
            (min.x + max.x) * 0.5,
            (min.y + max.y) * 0.5,
            (min.z + max.z) * 0.5,
        )
    }
}

/// Vertex data (no bone weights needed for segmented animation)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ModelVertex {
    pub position: Vec3,
    pub uv: Vec2,
    pub color: Color,
}

impl ModelVertex {
    pub fn new(position: Vec3, uv: Vec2) -> Self {
        Self {
            position,
            uv,
            color: Color::WHITE,
        }
    }

    pub fn with_color(position: Vec3, uv: Vec2, color: Color) -> Self {
        Self { position, uv, color }
    }
}

/// Triangle face
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ModelFace {
    pub indices: [usize; 3],
    pub double_sided: bool,
}

impl ModelFace {
    pub fn new(indices: [usize; 3]) -> Self {
        Self {
            indices,
            double_sided: false,
        }
    }

    pub fn double_sided(indices: [usize; 3]) -> Self {
        Self {
            indices,
            double_sided: true,
        }
    }
}

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
// Animation
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

/// Single keyframe (stores transform for each part)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keyframe {
    pub frame: u32,
    pub transforms: Vec<PartTransform>,
}

impl Keyframe {
    pub fn new(frame: u32, num_parts: usize) -> Self {
        Self {
            frame,
            transforms: vec![PartTransform::default(); num_parts],
        }
    }
}

/// Local transform for a part at a keyframe
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct PartTransform {
    pub position: Vec3,
    pub rotation: Vec3, // Euler angles in degrees
}

impl PartTransform {
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
