//! Mesh editor for importing OBJ files and assigning faces to bones
//! PS1-style skeletal animation with binary bone weights

use crate::rasterizer::{Vec3, Face, Vertex};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Main data structure for the mesh editor workflow
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeshEditorModel {
    pub version: u32,
    pub name: String,
    pub skeleton: Skeleton,
    pub mesh: EditableMesh,
    pub bone_assignments: BoneAssignments,
}

impl MeshEditorModel {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            version: 1,
            name: name.into(),
            skeleton: Skeleton::new(),
            mesh: EditableMesh::new(),
            bone_assignments: BoneAssignments::new(),
        }
    }

    pub fn from_mesh(name: impl Into<String>, mesh: EditableMesh) -> Self {
        let face_count = mesh.faces.len();
        Self {
            version: 1,
            name: name.into(),
            skeleton: Skeleton::new(),
            mesh,
            bone_assignments: BoneAssignments::with_face_count(face_count),
        }
    }

    /// Save mesh editor model to file (.bmesh format)
    pub fn save_to_file(&self, path: &Path) -> Result<(), MeshEditorError> {
        let config = ron::ser::PrettyConfig::new()
            .depth_limit(4)
            .indentor("  ".to_string());
        let contents = ron::ser::to_string_pretty(self, config)
            .map_err(|e| MeshEditorError::Serialization(e.to_string()))?;
        std::fs::write(path, contents)
            .map_err(|e| MeshEditorError::Io(e.to_string()))?;
        Ok(())
    }

    /// Load mesh editor model from file (.bmesh format)
    pub fn load_from_file(path: &Path) -> Result<Self, MeshEditorError> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| MeshEditorError::Io(e.to_string()))?;
        let model: MeshEditorModel = ron::from_str(&contents)
            .map_err(|e| MeshEditorError::Serialization(e.to_string()))?;
        Ok(model)
    }

    /// Validate the model before export
    pub fn validate(&self) -> Result<(), MeshEditorError> {
        // Check all faces are assigned
        let unassigned = self.bone_assignments.unassigned_faces();
        if !unassigned.is_empty() {
            return Err(MeshEditorError::Validation(format!(
                "{} faces are not assigned to any bone",
                unassigned.len()
            )));
        }

        // Check skeleton is not empty
        if self.skeleton.bones.is_empty() {
            return Err(MeshEditorError::Validation(
                "Skeleton has no bones".to_string()
            ));
        }

        // Check no cyclic dependencies in bone hierarchy
        for (i, bone) in self.skeleton.bones.iter().enumerate() {
            if let Some(parent) = bone.parent {
                if parent == i {
                    return Err(MeshEditorError::Validation(format!(
                        "Bone '{}' is its own parent",
                        bone.name
                    )));
                }
                // Simple cycle detection (could be more thorough)
                let mut current = parent;
                let mut depth = 0;
                while depth < self.skeleton.bones.len() {
                    if current == i {
                        return Err(MeshEditorError::Validation(format!(
                            "Cyclic dependency in bone hierarchy involving '{}'",
                            bone.name
                        )));
                    }
                    if let Some(next_parent) = self.skeleton.bones[current].parent {
                        current = next_parent;
                    } else {
                        break;
                    }
                    depth += 1;
                }
            }
        }

        Ok(())
    }
}

/// Skeleton with hierarchical bone structure
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Skeleton {
    pub bones: Vec<EditorBone>,
}

impl Skeleton {
    pub fn new() -> Self {
        Self { bones: Vec::new() }
    }

    pub fn add_bone(&mut self, bone: EditorBone) -> usize {
        let index = self.bones.len();
        self.bones.push(bone);
        index
    }

    pub fn remove_bone(&mut self, index: usize) -> Option<EditorBone> {
        if index >= self.bones.len() {
            return None;
        }

        // Update parent indices for bones that reference removed bone
        for bone in &mut self.bones {
            if bone.parent == Some(index) {
                bone.parent = None; // Orphan children
            } else if let Some(parent) = bone.parent {
                if parent > index {
                    bone.parent = Some(parent - 1); // Shift down indices
                }
            }
        }

        Some(self.bones.remove(index))
    }

    pub fn get_root_bones(&self) -> Vec<usize> {
        self.bones
            .iter()
            .enumerate()
            .filter_map(|(i, bone)| if bone.parent.is_none() { Some(i) } else { None })
            .collect()
    }
}

impl Default for Skeleton {
    fn default() -> Self {
        Self::new()
    }
}

/// Bone in the editor (before export to runtime format)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditorBone {
    pub name: String,
    pub parent: Option<usize>,
    pub position: Vec3,
    pub rotation: Vec3, // Euler angles for visualization
    pub length: f32,    // Visual representation length
}

impl EditorBone {
    pub fn new(name: impl Into<String>, position: Vec3) -> Self {
        Self {
            name: name.into(),
            parent: None,
            position,
            rotation: Vec3::ZERO,
            length: 10.0,
        }
    }

    pub fn with_parent(mut self, parent: usize) -> Self {
        self.parent = Some(parent);
        self
    }

    pub fn with_length(mut self, length: f32) -> Self {
        self.length = length;
        self
    }
}

/// Editable mesh (vertices and faces from OBJ import)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditableMesh {
    pub vertices: Vec<Vertex>,
    pub faces: Vec<Face>,
}

impl EditableMesh {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            faces: Vec::new(),
        }
    }

    pub fn from_parts(vertices: Vec<Vertex>, faces: Vec<Face>) -> Self {
        Self { vertices, faces }
    }

    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    /// Get all vertex indices used by a face
    pub fn face_vertices(&self, face_idx: usize) -> Option<[usize; 3]> {
        self.faces.get(face_idx).map(|f| [f.v0, f.v1, f.v2])
    }

    /// Compute centroid of a face
    pub fn face_centroid(&self, face_idx: usize) -> Option<Vec3> {
        let face = self.faces.get(face_idx)?;
        let v0 = self.vertices.get(face.v0)?.pos;
        let v1 = self.vertices.get(face.v1)?.pos;
        let v2 = self.vertices.get(face.v2)?.pos;
        Some(Vec3::new(
            (v0.x + v1.x + v2.x) / 3.0,
            (v0.y + v1.y + v2.y) / 3.0,
            (v0.z + v1.z + v2.z) / 3.0,
        ))
    }
}

impl Default for EditableMesh {
    fn default() -> Self {
        Self::new()
    }
}

/// Binary bone assignments (face index -> bone index)
/// PS1-style: each face is 100% assigned to one bone
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BoneAssignments {
    face_to_bone: Vec<Option<usize>>,
}

impl BoneAssignments {
    pub fn new() -> Self {
        Self {
            face_to_bone: Vec::new(),
        }
    }

    pub fn with_face_count(count: usize) -> Self {
        Self {
            face_to_bone: vec![None; count],
        }
    }

    /// Assign faces to a bone (binary weight)
    pub fn assign_faces(&mut self, face_indices: &[usize], bone_index: usize) {
        for &face_idx in face_indices {
            if face_idx < self.face_to_bone.len() {
                self.face_to_bone[face_idx] = Some(bone_index);
            }
        }
    }

    /// Unassign faces (remove bone assignment)
    pub fn unassign_faces(&mut self, face_indices: &[usize]) {
        for &face_idx in face_indices {
            if face_idx < self.face_to_bone.len() {
                self.face_to_bone[face_idx] = None;
            }
        }
    }

    /// Get bone assignment for a face
    pub fn get_bone_for_face(&self, face_idx: usize) -> Option<usize> {
        self.face_to_bone.get(face_idx).copied().flatten()
    }

    /// Get all faces assigned to a bone
    pub fn get_faces_for_bone(&self, bone_index: usize) -> Vec<usize> {
        self.face_to_bone
            .iter()
            .enumerate()
            .filter_map(|(i, b)| {
                if *b == Some(bone_index) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get all unassigned faces
    pub fn unassigned_faces(&self) -> Vec<usize> {
        self.face_to_bone
            .iter()
            .enumerate()
            .filter_map(|(i, b)| if b.is_none() { Some(i) } else { None })
            .collect()
    }

    /// Get count of faces assigned to each bone
    pub fn bone_face_counts(&self, bone_count: usize) -> Vec<usize> {
        let mut counts = vec![0; bone_count];
        for bone_idx in self.face_to_bone.iter().flatten() {
            if *bone_idx < bone_count {
                counts[*bone_idx] += 1;
            }
        }
        counts
    }

    /// Resize assignments array when mesh changes
    pub fn resize(&mut self, new_face_count: usize) {
        self.face_to_bone.resize(new_face_count, None);
    }
}

impl Default for BoneAssignments {
    fn default() -> Self {
        Self::new()
    }
}

/// Error types for mesh editor operations
#[derive(Debug)]
pub enum MeshEditorError {
    Io(String),
    Serialization(String),
    Validation(String),
    InvalidBoneIndex(usize),
    InvalidFaceIndex(usize),
}

impl std::fmt::Display for MeshEditorError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            MeshEditorError::Io(e) => write!(f, "IO error: {}", e),
            MeshEditorError::Serialization(e) => write!(f, "Serialization error: {}", e),
            MeshEditorError::Validation(e) => write!(f, "Validation error: {}", e),
            MeshEditorError::InvalidBoneIndex(i) => write!(f, "Invalid bone index: {}", i),
            MeshEditorError::InvalidFaceIndex(i) => write!(f, "Invalid face index: {}", i),
        }
    }
}

impl std::error::Error for MeshEditorError {}
