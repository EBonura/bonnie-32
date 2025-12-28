//! Move Drag Tracker
//!
//! Handles movement of vertices along an axis or plane.

use crate::rasterizer::Vec3;
use crate::ui::{DragConfig, PickerType, SnapMode, Axis};

/// Tracks a move/translate drag operation
#[derive(Debug, Clone)]
pub struct MoveTracker {
    /// Axis constraint (None = free movement on view plane)
    pub axis: Option<Axis>,
    /// Indices of vertices being moved
    pub vertex_indices: Vec<usize>,
    /// Initial positions of vertices (index, position)
    pub initial_positions: Vec<(usize, Vec3)>,
}

impl MoveTracker {
    pub fn new(
        axis: Option<Axis>,
        vertex_indices: Vec<usize>,
        initial_positions: Vec<(usize, Vec3)>,
    ) -> Self {
        Self {
            axis,
            vertex_indices,
            initial_positions,
        }
    }

    /// Create drag config for this move operation
    pub fn create_config(&self, center: Vec3, snap_enabled: bool, grid_size: f32) -> DragConfig {
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
            grid_size,
        }
    }

    /// Compute new vertex positions given a movement delta
    pub fn compute_new_positions(&self, delta: Vec3) -> Vec<(usize, Vec3)> {
        self.initial_positions
            .iter()
            .map(|(idx, pos)| (*idx, *pos + delta))
            .collect()
    }
}
