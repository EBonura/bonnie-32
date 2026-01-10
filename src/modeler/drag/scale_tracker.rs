//! Scale Drag Tracker
//!
//! Handles scaling of vertices from a center point using integer math.

use crate::rasterizer::{Vec3, IVec3};
use crate::ui::{DragConfig, PickerType, SnapMode, Axis};

/// Tracks a scale drag operation
#[derive(Debug, Clone)]
pub struct ScaleTracker {
    /// Axis constraint (None = uniform scaling)
    pub axis: Option<Axis>,
    /// Center of scaling (integer coordinates)
    pub center: IVec3,
    /// Indices of vertices being scaled
    pub vertex_indices: Vec<usize>,
    /// Initial positions of vertices (index, integer position)
    pub initial_positions: Vec<(usize, IVec3)>,
}

impl ScaleTracker {
    pub fn new(
        axis: Option<Axis>,
        center: IVec3,
        vertex_indices: Vec<usize>,
        initial_positions: Vec<(usize, IVec3)>,
    ) -> Self {
        Self {
            axis,
            center,
            vertex_indices,
            initial_positions,
        }
    }

    /// Create drag config for this scale operation
    /// Uses center converted to float for screen-space calculations
    pub fn create_config(&self, _center: Vec3) -> DragConfig {
        // Scale uses screen-space movement, not 3D picking
        DragConfig {
            picker: PickerType::Screen { sensitivity: 0.01 },
            snap_mode: SnapMode::None,
            grid_size: 0.1, // 10% increments if snapping
        }
    }

    /// Compute new vertex positions given a scale factor
    /// Uses integer math with i64 intermediate to avoid overflow
    pub fn compute_new_positions(&self, factor: f32) -> Vec<(usize, IVec3)> {
        // Convert factor to fixed-point (multiply by 4096, divide result by 4096)
        let factor_fixed = (factor * 4096.0).round() as i64;

        self.initial_positions
            .iter()
            .map(|(idx, pos)| {
                // Vector from center to vertex
                let offset_x = (pos.x - self.center.x) as i64;
                let offset_y = (pos.y - self.center.y) as i64;
                let offset_z = (pos.z - self.center.z) as i64;

                // Apply scale based on axis constraint
                let (scaled_x, scaled_y, scaled_z) = match self.axis {
                    None => {
                        // Uniform scaling
                        (
                            (offset_x * factor_fixed) / 4096,
                            (offset_y * factor_fixed) / 4096,
                            (offset_z * factor_fixed) / 4096,
                        )
                    }
                    Some(Axis::X) => ((offset_x * factor_fixed) / 4096, offset_y, offset_z),
                    Some(Axis::Y) => (offset_x, (offset_y * factor_fixed) / 4096, offset_z),
                    Some(Axis::Z) => (offset_x, offset_y, (offset_z * factor_fixed) / 4096),
                };

                let new_pos = IVec3::new(
                    scaled_x as i32 + self.center.x,
                    scaled_y as i32 + self.center.y,
                    scaled_z as i32 + self.center.z,
                );

                (*idx, new_pos)
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
            IVec3::ZERO,
            vec![0],
            vec![(0, IVec3::new(40, 20, 8))], // 10.0*4, 5.0*4, 2.0*4
        );

        let positions = tracker.compute_new_positions(2.0);
        let (_, new_pos) = positions[0];

        assert!((new_pos.x - 80).abs() < 2); // 20.0*4 = 80
        assert!((new_pos.y - 40).abs() < 2); // 10.0*4 = 40
        assert!((new_pos.z - 16).abs() < 2); // 4.0*4 = 16
    }

    #[test]
    fn test_axis_scale() {
        let tracker = ScaleTracker::new(
            Some(Axis::X),
            IVec3::ZERO,
            vec![0],
            vec![(0, IVec3::new(40, 20, 8))],
        );

        let positions = tracker.compute_new_positions(2.0);
        let (_, new_pos) = positions[0];

        // Only X should be scaled
        assert!((new_pos.x - 80).abs() < 2);
        assert!((new_pos.y - 20).abs() < 2);
        assert!((new_pos.z - 8).abs() < 2);
    }
}
