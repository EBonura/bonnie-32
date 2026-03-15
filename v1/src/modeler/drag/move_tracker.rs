//! Move Drag Tracker
//!
//! Handles movement of vertices along an axis or plane.

use crate::rasterizer::Vec3;
use crate::ui::{DragConfig, PickerType, SnapMode, Axis};
use crate::modeler::state::inverse_rotate_by_euler;

/// Tracks a move/translate drag operation
#[derive(Debug, Clone)]
pub struct MoveTracker {
    /// Axis constraint (None = free movement on view plane)
    pub axis: Option<Axis>,
    /// Custom axis direction (overrides axis.unit_vector() for Local mode)
    pub axis_direction: Option<Vec3>,
    /// Indices of vertices being moved
    pub vertex_indices: Vec<usize>,
    /// Initial positions of vertices (index, position)
    pub initial_positions: Vec<(usize, Vec3)>,
    /// Bone rotation for transforming world-space delta to bone-local space
    /// If Some, the delta will be inverse-rotated before applying
    pub bone_rotation: Option<Vec3>,
}

impl MoveTracker {
    pub fn new(
        axis: Option<Axis>,
        vertex_indices: Vec<usize>,
        initial_positions: Vec<(usize, Vec3)>,
    ) -> Self {
        Self {
            axis,
            axis_direction: None,
            vertex_indices,
            initial_positions,
            bone_rotation: None,
        }
    }

    /// Set bone rotation for world-to-local delta transformation
    pub fn with_bone_rotation(mut self, rotation: Option<Vec3>) -> Self {
        self.bone_rotation = rotation;
        self
    }

    /// Set custom axis direction (for Local mode)
    pub fn with_axis_direction(mut self, direction: Option<Vec3>) -> Self {
        self.axis_direction = direction;
        self
    }

    /// Create drag config for this move operation
    pub fn create_config(&self, center: Vec3, snap_enabled: bool, grid_size: f32) -> DragConfig {
        let picker = match self.axis {
            Some(axis) => PickerType::Line {
                origin: center,
                // Use custom axis direction if set (Local mode), otherwise world axis
                direction: self.axis_direction.unwrap_or_else(|| axis.unit_vector()),
            },
            None => PickerType::Screen { sensitivity: 0.5 },
        };

        DragConfig {
            picker,
            snap_mode: if snap_enabled { SnapMode::Relative } else { SnapMode::None },
            grid_size,
        }
    }

    /// Compute new vertex positions given a movement delta
    /// If bone_rotation is set, transforms the delta from world space to bone-local space
    pub fn compute_new_positions(&self, delta: Vec3) -> Vec<(usize, Vec3)> {
        // Transform delta to bone-local space if needed (for bone-bound meshes in Global mode)
        let local_delta = if let Some(bone_rot) = self.bone_rotation {
            inverse_rotate_by_euler(delta, bone_rot)
        } else {
            delta
        };

        self.initial_positions
            .iter()
            .map(|(idx, pos)| (*idx, *pos + local_delta))
            .collect()
    }
}
