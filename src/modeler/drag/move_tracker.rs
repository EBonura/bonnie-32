//! Move Drag Tracker
//!
//! Handles movement of vertices along an axis or plane.

use crate::rasterizer::{Vec3, IVec3};
use crate::ui::{DragConfig, PickerType, SnapMode, Axis};

/// Tracks a move/translate drag operation
#[derive(Debug, Clone)]
pub struct MoveTracker {
    /// Axis constraint (None = free movement on view plane)
    pub axis: Option<Axis>,
    /// Indices of vertices being moved
    pub vertex_indices: Vec<usize>,
    /// Initial positions of vertices (index, integer position)
    pub initial_positions: Vec<(usize, IVec3)>,
}

impl MoveTracker {
    pub fn new(
        axis: Option<Axis>,
        vertex_indices: Vec<usize>,
        initial_positions: Vec<(usize, IVec3)>,
    ) -> Self {
        Self {
            axis,
            vertex_indices,
            initial_positions,
        }
    }

    /// Create drag config for this move operation
    /// Note: center is in float coordinates for screen projection
    pub fn create_config(&self, center: Vec3, snap_enabled: bool, grid_size: i32) -> DragConfig {
        let picker = match self.axis {
            Some(axis) => PickerType::Line {
                origin: center,
                direction: axis.unit_vector(),
            },
            None => PickerType::Screen { sensitivity: 0.5 },
        };

        DragConfig {
            picker,
            snap_mode: if snap_enabled { SnapMode::Relative } else { SnapMode::None },
            grid_size: grid_size as f32, // DragConfig uses f32 for screen-space operations
        }
    }

    /// Compute new vertex positions given an integer movement delta
    pub fn compute_new_positions(&self, delta: IVec3) -> Vec<(usize, IVec3)> {
        self.initial_positions
            .iter()
            .map(|(idx, pos)| {
                let new_pos = IVec3::new(
                    pos.x + delta.x,
                    pos.y + delta.y,
                    pos.z + delta.z,
                );
                (*idx, new_pos)
            })
            .collect()
    }
}
