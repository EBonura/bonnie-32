//! Camera for 3D rendering
//!
//! Provides camera positioning and orientation for perspective projection.

use super::math::Vec3;

/// Camera state for 3D rendering
#[derive(Clone, Debug)]
pub struct Camera {
    pub position: Vec3,
    pub rotation_x: f32, // Pitch
    pub rotation_y: f32, // Yaw

    // Computed basis vectors
    pub basis_x: Vec3,
    pub basis_y: Vec3,
    pub basis_z: Vec3,
}

impl Camera {
    pub fn new() -> Self {
        let mut cam = Self {
            position: Vec3::ZERO,
            rotation_x: 0.0,
            rotation_y: 0.0,
            basis_x: Vec3::new(1.0, 0.0, 0.0),
            basis_y: Vec3::new(0.0, 1.0, 0.0),
            basis_z: Vec3::new(0.0, 0.0, 1.0),
        };
        cam.update_basis();
        cam
    }

    /// Create a camera looking down the Y axis (top-down view, XZ plane)
    pub fn ortho_top() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation_x: 0.0,
            rotation_y: 0.0,
            // Looking down +Y (from above), so: right=-X, up=+Z, forward=+Y
            basis_x: Vec3::new(-1.0, 0.0, 0.0),  // Right (flipped to maintain handedness)
            basis_y: Vec3::new(0.0, 0.0, 1.0),   // Up (Z goes up on screen)
            basis_z: Vec3::new(0.0, 1.0, 0.0),   // Forward (into the scene, +Y)
        }
    }

    /// Create a camera looking down the Z axis (front view, XY plane)
    pub fn ortho_front() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation_x: 0.0,
            rotation_y: 0.0,
            // Looking down -Z, so: right=+X, up=+Y, forward=-Z
            // Don't negate Y here - project_ortho handles the screen Y flip
            basis_x: Vec3::new(1.0, 0.0, 0.0),   // Right: world +X -> camera +X
            basis_y: Vec3::new(0.0, 1.0, 0.0),   // Up: world +Y -> camera +Y
            basis_z: Vec3::new(0.0, 0.0, -1.0),  // Forward (into the scene)
        }
    }

    /// Create a camera looking down the X axis (side view, ZY plane)
    pub fn ortho_side() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation_x: 0.0,
            rotation_y: 0.0,
            // Looking down -X, so: right=+Z, up=+Y, forward=-X
            // Wireframe uses (v.pos.z, v.pos.y), so:
            // cam_pos.x should be v.pos.z, cam_pos.y should be v.pos.y
            basis_x: Vec3::new(0.0, 0.0, 1.0),   // Right: world +Z -> camera +X
            basis_y: Vec3::new(0.0, 1.0, 0.0),   // Up: world +Y -> camera +Y
            basis_z: Vec3::new(-1.0, 0.0, 0.0),  // Forward (into the scene)
        }
    }

    pub fn update_basis(&mut self) {
        let upward = Vec3::new(0.0, -1.0, 0.0);  // Use -Y as up to match screen coordinates

        // Forward vector based on rotation
        self.basis_z = Vec3 {
            x: self.rotation_x.cos() * self.rotation_y.sin(),
            y: -self.rotation_x.sin(),  // Back to original with negation
            z: self.rotation_x.cos() * self.rotation_y.cos(),
        };

        // Right vector
        self.basis_x = upward.cross(self.basis_z).normalize();

        // Up vector
        self.basis_y = self.basis_z.cross(self.basis_x);
    }

    pub fn rotate(&mut self, dx: f32, dy: f32) {
        self.rotation_y += dy;
        self.rotation_x = (self.rotation_x + dx).clamp(
            -std::f32::consts::FRAC_PI_2 + 0.01,
            std::f32::consts::FRAC_PI_2 - 0.01,
        );
        self.update_basis();
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self::new()
    }
}
