//! Transform Components
//!
//! Two-tier transform system inspired by Bevy:
//! - Transform: Local position/rotation/scale (relative to parent)
//! - GlobalTransform: Computed world-space position (for rendering/physics)
//!
//! For entities with parents, GlobalTransform = parent.GlobalTransform * self.Transform
//! For root entities, GlobalTransform = Transform

use serde::{Serialize, Deserialize};
use crate::rasterizer::{Vec3, Mat4, mat4_mul, mat4_from_position_rotation, mat4_transform_point};

/// Local transform relative to parent (or world if no parent).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Transform {
    /// Position relative to parent
    pub position: Vec3,
    /// Rotation in euler angles (degrees) - matches your modeler
    pub rotation: Vec3,
    /// Scale factor (uniform for simplicity)
    pub scale: f32,
}

impl Transform {
    /// Identity transform (origin, no rotation, scale 1)
    pub const IDENTITY: Transform = Transform {
        position: Vec3::ZERO,
        rotation: Vec3::ZERO,
        scale: 1.0,
    };

    /// Create transform at a position
    pub fn from_position(position: Vec3) -> Self {
        Self {
            position,
            rotation: Vec3::ZERO,
            scale: 1.0,
        }
    }

    /// Create transform with position and rotation
    pub fn from_position_rotation(position: Vec3, rotation: Vec3) -> Self {
        Self {
            position,
            rotation,
            scale: 1.0,
        }
    }

    /// Convert to a 4x4 transformation matrix
    pub fn to_matrix(&self) -> Mat4 {
        let base = mat4_from_position_rotation(self.position, self.rotation);
        if (self.scale - 1.0).abs() < 0.0001 {
            base
        } else {
            // Apply scale
            let mut result = base;
            for i in 0..3 {
                for j in 0..3 {
                    result[i][j] *= self.scale;
                }
            }
            result
        }
    }

    /// Translate by an offset
    pub fn translate(&mut self, offset: Vec3) {
        self.position = self.position + offset;
    }

    /// Rotate by euler angles (degrees)
    pub fn rotate(&mut self, angles: Vec3) {
        self.rotation = self.rotation + angles;
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// World-space transform, computed from hierarchy.
///
/// For rendering and physics, you always use GlobalTransform.
/// It's recomputed each frame after Transform changes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GlobalTransform {
    /// The full 4x4 world transformation matrix
    matrix: Mat4,
}

impl GlobalTransform {
    /// Identity global transform
    pub const IDENTITY: GlobalTransform = GlobalTransform {
        matrix: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    /// Create from a local transform (for root entities)
    pub fn from_transform(transform: &Transform) -> Self {
        Self {
            matrix: transform.to_matrix(),
        }
    }

    /// Create at a world position
    pub fn from_position(position: Vec3) -> Self {
        Self::from_transform(&Transform::from_position(position))
    }

    /// Compute child's global transform from parent's global and child's local
    pub fn from_parent_and_local(parent: &GlobalTransform, local: &Transform) -> Self {
        Self {
            matrix: mat4_mul(&parent.matrix, &local.to_matrix()),
        }
    }

    /// Get the world position (translation component)
    pub fn position(&self) -> Vec3 {
        Vec3::new(self.matrix[0][3], self.matrix[1][3], self.matrix[2][3])
    }

    /// Get the full transformation matrix
    pub fn matrix(&self) -> &Mat4 {
        &self.matrix
    }

    /// Transform a point from local space to world space
    pub fn transform_point(&self, point: Vec3) -> Vec3 {
        mat4_transform_point(&self.matrix, point)
    }

    /// Get the forward direction (Z axis)
    pub fn forward(&self) -> Vec3 {
        Vec3::new(self.matrix[0][2], self.matrix[1][2], self.matrix[2][2]).normalize()
    }

    /// Get the right direction (X axis)
    pub fn right(&self) -> Vec3 {
        Vec3::new(self.matrix[0][0], self.matrix[1][0], self.matrix[2][0]).normalize()
    }

    /// Get the up direction (Y axis)
    pub fn up(&self) -> Vec3 {
        Vec3::new(self.matrix[0][1], self.matrix[1][1], self.matrix[2][1]).normalize()
    }
}

impl Default for GlobalTransform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// Propagate transforms through the hierarchy.
/// Call this each frame after updating local transforms.
///
/// This function updates all GlobalTransforms based on the hierarchy.
/// Root entities (no parent) get GlobalTransform = Transform.
/// Child entities get GlobalTransform = parent.GlobalTransform * child.Transform.
pub fn propagate_transforms(
    transforms: &crate::game::component::ComponentStorage<Transform>,
    global_transforms: &mut crate::game::component::ComponentStorage<GlobalTransform>,
    parents: &crate::game::component::ComponentStorage<crate::game::entity::Entity>,
    children: &crate::game::component::ComponentStorage<Vec<crate::game::entity::Entity>>,
    entities: &crate::game::entity::EntityAllocator,
) {
    // First pass: update root entities (those without parents)
    for (idx, transform) in transforms.iter() {
        let entity = crate::game::entity::Entity::new(idx, 0); // TODO: proper generation
        if !entities.is_alive(entity) {
            continue;
        }

        // If no parent, this is a root - compute directly
        if !parents.contains(entity) {
            global_transforms.insert(entity, GlobalTransform::from_transform(transform));
        }
    }

    // Second pass: propagate to children (recursive)
    // For simplicity, we do a simple iteration - works for shallow hierarchies
    // For deep hierarchies, would need proper topological sort
    for (idx, child_list) in children.iter() {
        let parent_entity = crate::game::entity::Entity::new(idx, 0);
        if let Some(parent_global) = global_transforms.get(parent_entity) {
            let parent_global = *parent_global; // Copy to avoid borrow issues
            for &child in child_list {
                if let Some(child_local) = transforms.get(child) {
                    let child_global = GlobalTransform::from_parent_and_local(&parent_global, child_local);
                    global_transforms.insert(child, child_global);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_to_matrix() {
        let t = Transform::from_position(Vec3::new(10.0, 20.0, 30.0));
        let m = t.to_matrix();

        // Translation should be in the last column
        assert!((m[0][3] - 10.0).abs() < 0.001);
        assert!((m[1][3] - 20.0).abs() < 0.001);
        assert!((m[2][3] - 30.0).abs() < 0.001);
    }

    #[test]
    fn test_global_transform_position() {
        let gt = GlobalTransform::from_position(Vec3::new(5.0, 10.0, 15.0));
        let pos = gt.position();

        assert!((pos.x - 5.0).abs() < 0.001);
        assert!((pos.y - 10.0).abs() < 0.001);
        assert!((pos.z - 15.0).abs() < 0.001);
    }

    #[test]
    fn test_parent_child_transform() {
        let parent = GlobalTransform::from_position(Vec3::new(100.0, 0.0, 0.0));
        let child_local = Transform::from_position(Vec3::new(10.0, 0.0, 0.0));

        let child_global = GlobalTransform::from_parent_and_local(&parent, &child_local);
        let pos = child_global.position();

        // Child should be at parent + local = (110, 0, 0)
        assert!((pos.x - 110.0).abs() < 0.001);
    }
}
