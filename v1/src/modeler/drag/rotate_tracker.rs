//! Rotate Drag Tracker
//!
//! Handles rotation of vertices around an axis.

use crate::rasterizer::Vec3;
use crate::ui::{DragConfig, PickerType, SnapMode, Axis};

/// Tracks a rotation drag operation
#[derive(Debug, Clone)]
pub struct RotateTracker {
    /// Which axis to rotate around
    pub axis: Axis,
    /// Center of rotation
    pub center: Vec3,
    /// Indices of vertices being rotated
    pub vertex_indices: Vec<usize>,
    /// Initial positions of vertices (index, position)
    pub initial_positions: Vec<(usize, Vec3)>,
}

impl RotateTracker {
    pub fn new(
        axis: Axis,
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

    /// Create drag config for this rotation operation
    pub fn create_config(&self, snap_enabled: bool, snap_degrees: f32) -> DragConfig {
        // Reference vector for angle=0 (perpendicular to rotation axis)
        let ref_vector = match self.axis {
            Axis::X => Vec3::new(0.0, 1.0, 0.0),
            Axis::Y => Vec3::new(1.0, 0.0, 0.0),
            Axis::Z => Vec3::new(1.0, 0.0, 0.0),
        };

        DragConfig {
            picker: PickerType::Circle {
                center: self.center,
                axis: self.axis.unit_vector(),
                ref_vector,
            },
            snap_mode: if snap_enabled { SnapMode::Relative } else { SnapMode::None },
            grid_size: snap_degrees.to_radians(),
        }
    }

    /// Compute new vertex positions given a rotation angle (in radians)
    pub fn compute_new_positions(&self, angle: f32) -> Vec<(usize, Vec3)> {
        let axis_vec = self.axis.unit_vector();
        let cos_a = angle.cos();
        let sin_a = angle.sin();

        self.initial_positions
            .iter()
            .map(|(idx, pos)| {
                // Translate to origin (center)
                let p = *pos - self.center;

                // Rodrigues' rotation formula:
                // v_rot = v * cos(θ) + (k × v) * sin(θ) + k * (k · v) * (1 - cos(θ))
                let k = axis_vec;
                let k_cross_p = k.cross(p);
                let k_dot_p = k.dot(p);

                let rotated = p * cos_a + k_cross_p * sin_a + k * k_dot_p * (1.0 - cos_a);

                // Translate back
                (*idx, rotated + self.center)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rotate_around_y() {
        let tracker = RotateTracker::new(
            Axis::Y,
            Vec3::ZERO,
            vec![0],
            vec![(0, Vec3::new(10.0, 0.0, 0.0))],
        );

        // Rotate 90 degrees (π/2)
        let positions = tracker.compute_new_positions(std::f32::consts::FRAC_PI_2);
        let (_, new_pos) = positions[0];

        // (10, 0, 0) rotated 90° around Y should be (0, 0, -10)
        assert!(new_pos.x.abs() < 0.001, "x={}", new_pos.x);
        assert!(new_pos.y.abs() < 0.001, "y={}", new_pos.y);
        assert!((new_pos.z - -10.0).abs() < 0.001, "z={}", new_pos.z);
    }
}
