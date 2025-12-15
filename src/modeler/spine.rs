//! Spine-based procedural mesh generation
//!
//! Inspired by Spore's creature editor - define a spine (chain of joints with radii),
//! and geometry is automatically generated as a tube mesh.
//!
//! Key concepts:
//! - SpineJoint: a point along the spine with a radius (thickness)
//! - SpineSegment: a chain of joints that forms a limb/torso/etc
//! - Mesh generation: creates N-gon rings at each joint, connects them with faces
//!
//! Uses RON (Rusty Object Notation) for human-readable spine model files.
//!
//! Future: joints become bones for animation, branches for limbs

use serde::{Deserialize, Serialize};
use crate::rasterizer::{Vec3, Vec2, Color, Vertex, Face};
use std::f32::consts::PI;

/// Validation limits to prevent resource exhaustion from malicious files
pub mod limits {
    /// Maximum number of segments in a model
    pub const MAX_SEGMENTS: usize = 64;
    /// Maximum number of joints per segment
    pub const MAX_JOINTS: usize = 128;
    /// Maximum sides for tube generation
    pub const MAX_SIDES: u8 = 32;
    /// Minimum sides for tube generation
    pub const MIN_SIDES: u8 = 3;
    /// Maximum coordinate value (prevents overflow issues)
    pub const MAX_COORD: f32 = 100_000.0;
    /// Maximum radius value
    pub const MAX_RADIUS: f32 = 10_000.0;
    /// Maximum string length for names
    pub const MAX_STRING_LEN: usize = 256;
}

/// Error type for spine model loading/saving
#[derive(Debug)]
pub enum SpineError {
    IoError(std::io::Error),
    ParseError(ron::error::SpannedError),
    SerializeError(ron::Error),
    ValidationError(String),
}

impl From<std::io::Error> for SpineError {
    fn from(e: std::io::Error) -> Self {
        SpineError::IoError(e)
    }
}

impl From<ron::error::SpannedError> for SpineError {
    fn from(e: ron::error::SpannedError) -> Self {
        SpineError::ParseError(e)
    }
}

impl From<ron::Error> for SpineError {
    fn from(e: ron::Error) -> Self {
        SpineError::SerializeError(e)
    }
}

impl std::fmt::Display for SpineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpineError::IoError(e) => write!(f, "IO error: {}", e),
            SpineError::ParseError(e) => write!(f, "Parse error: {}", e),
            SpineError::SerializeError(e) => write!(f, "Serialize error: {}", e),
            SpineError::ValidationError(e) => write!(f, "Validation error: {}", e),
        }
    }
}

/// Check if a float is valid (not NaN or Inf)
fn is_valid_float(f: f32) -> bool {
    f.is_finite() && f.abs() <= limits::MAX_COORD
}

/// Validate a Vec3 position
fn validate_position(v: &Vec3, context: &str) -> Result<(), String> {
    if !is_valid_float(v.x) || !is_valid_float(v.y) || !is_valid_float(v.z) {
        return Err(format!("{}: invalid position ({}, {}, {})", context, v.x, v.y, v.z));
    }
    Ok(())
}

/// Validate a spine joint
fn validate_joint(joint: &SpineJoint, context: &str) -> Result<(), String> {
    validate_position(&joint.position, context)?;
    if !joint.radius.is_finite() || joint.radius < 0.0 || joint.radius > limits::MAX_RADIUS {
        return Err(format!("{}: invalid radius {}", context, joint.radius));
    }
    Ok(())
}

/// Validate a spine segment
fn validate_segment(segment: &SpineSegment, seg_idx: usize) -> Result<(), String> {
    let context = format!("segment[{}]", seg_idx);

    // Check name length
    if segment.name.len() > limits::MAX_STRING_LEN {
        return Err(format!("{}: name too long ({} > {})",
            context, segment.name.len(), limits::MAX_STRING_LEN));
    }

    // Check joint count
    if segment.joints.len() > limits::MAX_JOINTS {
        return Err(format!("{}: too many joints ({} > {})",
            context, segment.joints.len(), limits::MAX_JOINTS));
    }

    // Check sides value
    if segment.sides < limits::MIN_SIDES || segment.sides > limits::MAX_SIDES {
        return Err(format!("{}: invalid sides {} (must be {}-{})",
            context, segment.sides, limits::MIN_SIDES, limits::MAX_SIDES));
    }

    // Validate each joint
    for (i, joint) in segment.joints.iter().enumerate() {
        validate_joint(joint, &format!("{} joint[{}]", context, i))?;
    }

    Ok(())
}

/// Validate an entire spine model
pub fn validate_spine_model(model: &SpineModel) -> Result<(), SpineError> {
    // Check name length
    if model.name.len() > limits::MAX_STRING_LEN {
        return Err(SpineError::ValidationError(format!(
            "model name too long ({} > {})", model.name.len(), limits::MAX_STRING_LEN
        )));
    }

    // Check segment count
    if model.segments.len() > limits::MAX_SEGMENTS {
        return Err(SpineError::ValidationError(format!(
            "too many segments ({} > {})", model.segments.len(), limits::MAX_SEGMENTS
        )));
    }

    // Validate each segment
    for (i, segment) in model.segments.iter().enumerate() {
        validate_segment(segment, i).map_err(SpineError::ValidationError)?;
    }

    Ok(())
}

/// A single joint in the spine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpineJoint {
    /// Position relative to previous joint (or world position for first joint)
    pub position: Vec3,
    /// Radius (thickness) at this joint
    pub radius: f32,
}

impl SpineJoint {
    pub fn new(position: Vec3, radius: f32) -> Self {
        Self { position, radius }
    }
}

/// A segment of connected joints forming a tube
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpineSegment {
    pub name: String,
    pub joints: Vec<SpineJoint>,
    /// Number of sides for the tube (6 = hexagon, 8 = octagon, etc)
    pub sides: u8,
    /// Close the start of the tube with a cap
    pub cap_start: bool,
    /// Close the end of the tube with a cap
    pub cap_end: bool,
}

impl SpineSegment {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            joints: Vec::new(),
            sides: 6,
            cap_start: true,
            cap_end: true,
        }
    }

    /// Add a joint to the end of the spine
    pub fn add_joint(&mut self, position: Vec3, radius: f32) {
        self.joints.push(SpineJoint::new(position, radius));
    }

    /// Generate mesh vertices and faces from this spine segment
    /// Returns (vertices, faces) ready for rendering
    pub fn generate_mesh(&self) -> (Vec<Vertex>, Vec<Face>) {
        if self.joints.len() < 2 {
            return (Vec::new(), Vec::new());
        }

        let mut vertices = Vec::new();
        let mut faces = Vec::new();

        let sides = self.sides as usize;

        // Generate ring of vertices at each joint
        let mut rings: Vec<Vec<usize>> = Vec::new();

        for (joint_idx, joint) in self.joints.iter().enumerate() {
            // Calculate the direction of the spine at this joint
            let direction = self.calculate_direction(joint_idx);

            // Create a coordinate frame perpendicular to the spine direction
            let (tangent, bitangent) = Self::perpendicular_frame(direction);

            // Generate vertices around the ring
            let mut ring_indices = Vec::with_capacity(sides);
            let base_idx = vertices.len();

            for i in 0..sides {
                // Negative angle for clockwise winding (when looking down the spine)
                let angle = -(i as f32 / sides as f32) * 2.0 * PI;
                let cos_a = angle.cos();
                let sin_a = angle.sin();

                // Position on the ring
                let offset = tangent * cos_a * joint.radius + bitangent * sin_a * joint.radius;
                let pos = joint.position + offset;

                // Normal points outward from the spine axis
                let normal = (tangent * cos_a + bitangent * sin_a).normalize();

                // UV: u wraps around the tube, v goes along the length
                let u = i as f32 / sides as f32;
                let v = joint_idx as f32 / (self.joints.len() - 1).max(1) as f32;

                vertices.push(Vertex {
                    pos,
                    uv: Vec2::new(u, v),
                    normal,
                    color: Color::NEUTRAL,
                });

                ring_indices.push(base_idx + i);
            }

            rings.push(ring_indices);
        }

        // Connect rings with faces (quads split into triangles)
        // Winding order: clockwise when viewed from outside (for this rasterizer's convention)
        for ring_idx in 0..rings.len() - 1 {
            let ring_a = &rings[ring_idx];
            let ring_b = &rings[ring_idx + 1];

            for i in 0..sides {
                let next_i = (i + 1) % sides;

                // Indices for the quad
                let a0 = ring_a[i];
                let a1 = ring_a[next_i];
                let b0 = ring_b[i];
                let b1 = ring_b[next_i];

                // Two triangles per quad
                // Triangle 1: a0, b0, b1
                faces.push(Face::new(a0, b0, b1));
                // Triangle 2: a0, b1, a1
                faces.push(Face::new(a0, b1, a1));
            }
        }

        // Start cap (faces inward along spine direction)
        if self.cap_start && !rings.is_empty() {
            let center_idx = vertices.len();
            let first_joint = &self.joints[0];
            let direction = self.calculate_direction(0);

            // Center vertex
            vertices.push(Vertex {
                pos: first_joint.position,
                uv: Vec2::new(0.5, 0.0),
                normal: direction * -1.0, // Points backward (away from spine)
                color: Color::NEUTRAL,
            });

            // Create fan triangles - winding to face backward (away from spine start)
            let ring = &rings[0];
            for i in 0..sides {
                let next_i = (i + 1) % sides;
                faces.push(Face::new(center_idx, ring[next_i], ring[i]));
            }
        }

        // End cap (faces outward along spine direction)
        if self.cap_end && !rings.is_empty() {
            let center_idx = vertices.len();
            let last_joint = &self.joints[self.joints.len() - 1];
            let direction = self.calculate_direction(self.joints.len() - 1);

            // Center vertex
            vertices.push(Vertex {
                pos: last_joint.position,
                uv: Vec2::new(0.5, 1.0),
                normal: direction, // Points forward (away from spine)
                color: Color::NEUTRAL,
            });

            // Create fan triangles - winding to face forward (away from spine end)
            let ring = &rings[rings.len() - 1];
            for i in 0..sides {
                let next_i = (i + 1) % sides;
                faces.push(Face::new(center_idx, ring[i], ring[next_i]));
            }
        }

        (vertices, faces)
    }

    /// Calculate the direction of the spine at a given joint index
    fn calculate_direction(&self, joint_idx: usize) -> Vec3 {
        if self.joints.len() < 2 {
            return Vec3::new(0.0, 1.0, 0.0); // Default up
        }

        if joint_idx == 0 {
            // First joint: direction to next
            (self.joints[1].position - self.joints[0].position).normalize()
        } else if joint_idx >= self.joints.len() - 1 {
            // Last joint: direction from previous
            let last = self.joints.len() - 1;
            (self.joints[last].position - self.joints[last - 1].position).normalize()
        } else {
            // Middle joint: average of incoming and outgoing directions
            let to_prev = self.joints[joint_idx].position - self.joints[joint_idx - 1].position;
            let to_next = self.joints[joint_idx + 1].position - self.joints[joint_idx].position;
            (to_prev.normalize() + to_next.normalize()).normalize()
        }
    }

    /// Create perpendicular vectors (tangent and bitangent) for a given direction
    fn perpendicular_frame(direction: Vec3) -> (Vec3, Vec3) {
        // Choose a reference vector that's not parallel to direction
        let reference = if direction.y.abs() < 0.9 {
            Vec3::new(0.0, 1.0, 0.0)
        } else {
            Vec3::new(1.0, 0.0, 0.0)
        };

        let tangent = direction.cross(reference).normalize();
        let bitangent = direction.cross(tangent).normalize();

        (tangent, bitangent)
    }
}

/// A complete spine-based model (can have multiple segments for branching)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpineModel {
    pub name: String,
    pub segments: Vec<SpineSegment>,
}

impl SpineModel {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            segments: Vec::new(),
        }
    }

    /// Add a segment to the model
    pub fn add_segment(&mut self, segment: SpineSegment) -> usize {
        let idx = self.segments.len();
        self.segments.push(segment);
        idx
    }

    /// Generate combined mesh from all segments
    pub fn generate_mesh(&self) -> (Vec<Vertex>, Vec<Face>) {
        let mut all_vertices = Vec::new();
        let mut all_faces = Vec::new();

        for segment in &self.segments {
            let vertex_offset = all_vertices.len();
            let (verts, faces) = segment.generate_mesh();

            all_vertices.extend(verts);

            // Offset face indices
            for face in faces {
                all_faces.push(Face::new(
                    face.v0 + vertex_offset,
                    face.v1 + vertex_offset,
                    face.v2 + vertex_offset,
                ));
            }
        }

        (all_vertices, all_faces)
    }

    /// Create a simple test spine (vertical tube)
    pub fn test_tube() -> Self {
        let mut model = Self::new("test_tube");

        let mut segment = SpineSegment::new("body");
        segment.sides = 8;

        // Create a simple vertical tube
        segment.add_joint(Vec3::new(0.0, 0.0, 0.0), 20.0);
        segment.add_joint(Vec3::new(0.0, 30.0, 0.0), 25.0);
        segment.add_joint(Vec3::new(0.0, 60.0, 0.0), 30.0);
        segment.add_joint(Vec3::new(0.0, 90.0, 0.0), 25.0);
        segment.add_joint(Vec3::new(0.0, 120.0, 0.0), 15.0);

        model.add_segment(segment);
        model
    }

    /// Create a snake-like curved spine
    pub fn test_snake() -> Self {
        let mut model = Self::new("test_snake");

        let mut segment = SpineSegment::new("body");
        segment.sides = 6;

        // Create a curved spine
        let segments = 10;
        for i in 0..=segments {
            let t = i as f32 / segments as f32;
            let x = (t * PI * 2.0).sin() * 30.0;
            let y = t * 100.0;
            let z = (t * PI * 2.0).cos() * 20.0;

            // Radius varies - thicker in middle
            let radius = 10.0 + (1.0 - (t - 0.5).abs() * 2.0) * 15.0;

            segment.add_joint(Vec3::new(x, y, z), radius);
        }

        model.add_segment(segment);
        model
    }

    /// Create a simple humanoid-like shape (torso with suggestion of head)
    pub fn test_humanoid() -> Self {
        let mut model = Self::new("test_humanoid");

        // Torso
        let mut torso = SpineSegment::new("torso");
        torso.sides = 8;
        torso.add_joint(Vec3::new(0.0, 0.0, 0.0), 15.0);   // Hips
        torso.add_joint(Vec3::new(0.0, 25.0, 0.0), 20.0);  // Waist
        torso.add_joint(Vec3::new(0.0, 50.0, 0.0), 25.0);  // Chest
        torso.add_joint(Vec3::new(0.0, 70.0, 0.0), 18.0);  // Shoulders
        torso.add_joint(Vec3::new(0.0, 85.0, 0.0), 12.0);  // Neck
        torso.add_joint(Vec3::new(0.0, 100.0, 0.0), 15.0); // Head
        torso.add_joint(Vec3::new(0.0, 115.0, 0.0), 12.0); // Top of head

        model.add_segment(torso);
        model
    }

    /// Create an empty model with a single default segment (2 joints)
    pub fn new_empty(name: &str) -> Self {
        let mut model = Self::new(name);

        let mut segment = SpineSegment::new("segment_0");
        segment.sides = 8;
        segment.add_joint(Vec3::new(0.0, 0.0, 0.0), 15.0);
        segment.add_joint(Vec3::new(0.0, 30.0, 0.0), 15.0);

        model.add_segment(segment);
        model
    }

    /// Save spine model to RON file
    pub fn save_to_file(&self, path: &std::path::Path) -> Result<(), SpineError> {
        let config = ron::ser::PrettyConfig::new()
            .depth_limit(4)
            .indentor("  ".to_string());

        let contents = ron::ser::to_string_pretty(self, config)?;
        std::fs::write(path, contents)?;
        Ok(())
    }

    /// Load spine model from RON file
    pub fn load_from_file(path: &std::path::Path) -> Result<Self, SpineError> {
        let contents = std::fs::read_to_string(path)?;
        let model: SpineModel = ron::from_str(&contents)?;

        // Validate loaded model
        validate_spine_model(&model)?;

        Ok(model)
    }

    /// Load spine model from RON string (for embedded models or testing)
    pub fn load_from_str(s: &str) -> Result<Self, SpineError> {
        let model: SpineModel = ron::from_str(s)?;
        validate_spine_model(&model)?;
        Ok(model)
    }

    /// Create a new segment with default joints at the origin
    pub fn create_default_segment(&mut self) -> usize {
        let seg_idx = self.segments.len();
        let mut segment = SpineSegment::new(&format!("segment_{}", seg_idx));
        segment.sides = 8;
        // Offset from origin based on existing segments
        let offset_x = seg_idx as f32 * 50.0;
        segment.add_joint(Vec3::new(offset_x, 0.0, 0.0), 15.0);
        segment.add_joint(Vec3::new(offset_x, 30.0, 0.0), 15.0);
        self.segments.push(segment);
        seg_idx
    }

    /// Remove a segment by index
    pub fn remove_segment(&mut self, seg_idx: usize) -> bool {
        if seg_idx < self.segments.len() && self.segments.len() > 1 {
            self.segments.remove(seg_idx);
            true
        } else {
            false
        }
    }

    /// Mirror a segment on the X axis (creates a duplicate)
    pub fn mirror_segment(&mut self, seg_idx: usize) -> Option<usize> {
        if let Some(segment) = self.segments.get(seg_idx) {
            let mut mirrored = segment.clone();
            mirrored.name = format!("{}_mirror", segment.name);

            // Mirror all joint positions on X axis
            for joint in &mut mirrored.joints {
                joint.position.x = -joint.position.x;
            }

            let new_idx = self.segments.len();
            self.segments.push(mirrored);
            Some(new_idx)
        } else {
            None
        }
    }
}
