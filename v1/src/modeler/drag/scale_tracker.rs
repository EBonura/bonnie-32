//! Scale Drag Tracker
//!
//! Handles scaling of vertices from a center point.

use crate::rasterizer::Vec3;
use crate::ui::{DragConfig, PickerType, SnapMode, Axis};

/// Tracks a scale drag operation
#[derive(Debug, Clone)]
pub struct ScaleTracker {
    /// Axis constraint (None = uniform scaling)
    pub axis: Option<Axis>,
    /// Center of scaling
    pub center: Vec3,
    /// Indices of vertices being scaled
    pub vertex_indices: Vec<usize>,
    /// Initial positions of vertices (index, position)
    pub initial_positions: Vec<(usize, Vec3)>,
}

impl ScaleTracker {
    pub fn new(
        axis: Option<Axis>,
        center: Vec3,
        vertex_indices: Vec<usize>,
        initial_positions: Vec<(usize, Vec3)>,
    ) -> Self {
        Self {
            axis,
            center,
            vertex_indices,
            initial_positions,
        }
    }

    /// Create drag config for this scale operation
    pub fn create_config(&self, _center: Vec3) -> DragConfig {
        // Scale uses screen-space movement, not 3D picking
        DragConfig {
            picker: PickerType::Screen { sensitivity: 0.01 },
            snap_mode: SnapMode::None,
            grid_size: 0.1, // 10% increments if snapping
        }
    }

    /// Compute new vertex positions given a scale factor
    pub fn compute_new_positions(&self, factor: f32) -> Vec<(usize, Vec3)> {
        self.initial_positions
            .iter()
            .map(|(idx, pos)| {
                // Vector from center to vertex
                let offset = *pos - self.center;

                // Apply scale based on axis constraint
                let scaled_offset = match self.axis {
                    None => offset * factor, // Uniform scaling
                    Some(Axis::X) => Vec3::new(offset.x * factor, offset.y, offset.z),
                    Some(Axis::Y) => Vec3::new(offset.x, offset.y * factor, offset.z),
                    Some(Axis::Z) => Vec3::new(offset.x, offset.y, offset.z * factor),
                };

                (*idx, self.center + scaled_offset)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uniform_scale() {
        let tracker = ScaleTracker::new(
            None,
            Vec3::ZERO,
            vec![0],
            vec![(0, Vec3::new(10.0, 5.0, 2.0))],
        );

        let positions = tracker.compute_new_positions(2.0);
        let (_, new_pos) = positions[0];

        assert!((new_pos.x - 20.0).abs() < 0.001);
        assert!((new_pos.y - 10.0).abs() < 0.001);
        assert!((new_pos.z - 4.0).abs() < 0.001);
    }

    #[test]
    fn test_axis_scale() {
        let tracker = ScaleTracker::new(
            Some(Axis::X),
            Vec3::ZERO,
            vec![0],
            vec![(0, Vec3::new(10.0, 5.0, 2.0))],
        );

        let positions = tracker.compute_new_positions(2.0);
        let (_, new_pos) = positions[0];

        // Only X should be scaled
        assert!((new_pos.x - 20.0).abs() < 0.001);
        assert!((new_pos.y - 5.0).abs() < 0.001);
        assert!((new_pos.z - 2.0).abs() < 0.001);
    }
}
