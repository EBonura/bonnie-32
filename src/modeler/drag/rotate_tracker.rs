//! Rotate Drag Tracker
//!
//! Handles rotation of vertices around an axis using fixed-point math.

use crate::rasterizer::{Vec3, IVec3, fixed_sin, fixed_cos, degrees_to_angle, TRIG_SCALE};
use crate::ui::{DragConfig, PickerType, SnapMode, Axis};

/// Tracks a rotation drag operation
#[derive(Debug, Clone)]
pub struct RotateTracker {
    /// Which axis to rotate around
    pub axis: Axis,
    /// Center of rotation (integer coordinates)
    pub center: IVec3,
    /// Indices of vertices being rotated
    pub vertex_indices: Vec<usize>,
    /// Initial positions of vertices (index, integer position)
    pub initial_positions: Vec<(usize, IVec3)>,
}

impl RotateTracker {
    pub fn new(
        axis: Axis,
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

    /// Create drag config for this rotation operation
    /// Uses center converted to float for screen-space calculations
    pub fn create_config(&self, snap_enabled: bool, snap_degrees: f32) -> DragConfig {
        let center_f32 = self.center.to_render_f32();

        // Reference vector for angle=0 (perpendicular to rotation axis)
        let ref_vector = match self.axis {
            Axis::X => Vec3::new(0.0, 1.0, 0.0),
            Axis::Y => Vec3::new(1.0, 0.0, 0.0),
            Axis::Z => Vec3::new(1.0, 0.0, 0.0),
        };

        DragConfig {
            picker: PickerType::Circle {
                center: center_f32,
                axis: self.axis.unit_vector(),
                ref_vector,
            },
            snap_mode: if snap_enabled { SnapMode::Relative } else { SnapMode::None },
            grid_size: snap_degrees.to_radians(),
        }
    }

    /// Compute new vertex positions given a rotation angle (in radians)
    /// Uses fixed-point math for PS1-authentic rotation
    pub fn compute_new_positions(&self, angle: f32) -> Vec<(usize, IVec3)> {
        // Convert angle from radians to fixed-point angle (0-4095 = 0-360°)
        let degrees = angle.to_degrees();
        let fixed_angle = degrees_to_angle(degrees);

        let cos_a = fixed_cos(fixed_angle) as i64;
        let sin_a = fixed_sin(fixed_angle) as i64;
        let scale = TRIG_SCALE as i64;

        self.initial_positions
            .iter()
            .map(|(idx, pos)| {
                // Translate to origin (center)
                let px = (pos.x - self.center.x) as i64;
                let py = (pos.y - self.center.y) as i64;
                let pz = (pos.z - self.center.z) as i64;

                // Rodrigues' rotation formula using fixed-point
                // For single-axis rotation, simplify to 2D rotation in the appropriate plane
                let (rx, ry, rz) = match self.axis {
                    Axis::X => {
                        // Rotate in YZ plane
                        let new_y = (py * cos_a - pz * sin_a) / scale;
                        let new_z = (py * sin_a + pz * cos_a) / scale;
                        (px, new_y, new_z)
                    }
                    Axis::Y => {
                        // Rotate in XZ plane
                        let new_x = (px * cos_a + pz * sin_a) / scale;
                        let new_z = (-px * sin_a + pz * cos_a) / scale;
                        (new_x, py, new_z)
                    }
                    Axis::Z => {
                        // Rotate in XY plane
                        let new_x = (px * cos_a - py * sin_a) / scale;
                        let new_y = (px * sin_a + py * cos_a) / scale;
                        (new_x, new_y, pz)
                    }
                };

                // Translate back
                let new_pos = IVec3::new(
                    rx as i32 + self.center.x,
                    ry as i32 + self.center.y,
                    rz as i32 + self.center.z,
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
    fn test_rotate_around_y() {
        let tracker = RotateTracker::new(
            Axis::Y,
            IVec3::ZERO,
            vec![0],
            vec![(0, IVec3::new(40, 0, 0))], // 40 = 10.0 * INT_SCALE
        );

        // Rotate 90 degrees (π/2)
        let positions = tracker.compute_new_positions(std::f32::consts::FRAC_PI_2);
        let (_, new_pos) = positions[0];

        // (40, 0, 0) rotated 90° around Y should be (0, 0, -40)
        // Allow some tolerance for fixed-point rounding
        assert!(new_pos.x.abs() < 2, "x={}", new_pos.x);
        assert!(new_pos.y.abs() < 2, "y={}", new_pos.y);
        assert!((new_pos.z + 40).abs() < 2, "z={}", new_pos.z);
    }
}
