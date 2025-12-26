//! Vector math for 3D rendering
//! Ported from tipsy's C implementation

use std::ops::{Add, Sub, Mul};
use serde::{Serialize, Deserialize};

/// 3D Vector
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 0.0 };
    pub const UP: Vec3 = Vec3 { x: 0.0, y: 1.0, z: 0.0 };

    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn dot(self, other: Vec3) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn cross(self, other: Vec3) -> Vec3 {
        Vec3 {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }

    pub fn len(self) -> f32 {
        self.dot(self).sqrt()
    }

    pub fn normalize(self) -> Vec3 {
        let l = self.len();
        if l == 0.0 {
            return Vec3::ZERO;
        }
        Vec3 {
            x: self.x / l,
            y: self.y / l,
            z: self.z / l,
        }
    }

    pub fn scale(self, s: f32) -> Vec3 {
        Vec3 {
            x: self.x * s,
            y: self.y * s,
            z: self.z * s,
        }
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, other: Vec3) -> Vec3 {
        Vec3 {
            x: self.x + other.x,
            y: self.y + other.y,
            z: self.z + other.z,
        }
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, other: Vec3) -> Vec3 {
        Vec3 {
            x: self.x - other.x,
            y: self.y - other.y,
            z: self.z - other.z,
        }
    }
}

impl Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f32) -> Vec3 {
        self.scale(s)
    }
}

/// 2D Vector (for texture coordinates)
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// Transform a vertex by camera basis vectors (rotation)
pub fn perspective_transform(v: Vec3, cam_x: Vec3, cam_y: Vec3, cam_z: Vec3) -> Vec3 {
    Vec3 {
        x: v.dot(cam_x),
        y: v.dot(cam_y),
        z: v.dot(cam_z),
    }
}

/// Project a 3D point to 2D screen coordinates
/// If `snap` is true, coordinates are floored to integers (PS1 jitter effect)
/// Returns Vec3 where x,y are screen coords and z is the ORIGINAL camera-space depth
/// (needed for perspective-correct texture interpolation)
pub fn project(v: Vec3, snap: bool, width: usize, height: usize) -> Vec3 {
    const DISTANCE: f32 = 5.0;
    const SCALE: f32 = 0.75;

    let ud = DISTANCE;
    let us = ud - 1.0;
    let vs = (width.min(height) as f32 / 2.0) * SCALE;

    // Perspective divide
    let denom = v.z + ud;
    if denom.abs() < 0.001 {
        return Vec3::new(width as f32 / 2.0, height as f32 / 2.0, v.z);
    }

    let mut result = Vec3 {
        x: (v.x * us) / denom,
        y: (v.y * us) / denom,
        z: v.z, // Store ORIGINAL camera-space Z for perspective-correct interpolation
    };

    // Scale to screen
    result.x = result.x * vs + (width as f32 / 2.0);
    result.y = result.y * vs + (height as f32 / 2.0);
    // z stays as original camera-space depth

    // PS1 vertex snapping
    if snap {
        result.x = result.x.floor();
        result.y = result.y.floor();
    }

    result
}

/// Orthographic projection (no perspective divide)
/// Projects camera-space coordinates to screen-space for ortho views
pub fn project_ortho(v: Vec3, zoom: f32, center_x: f32, center_y: f32, width: usize, height: usize) -> Vec3 {
    // In ortho view, x and y map directly (scaled by zoom), z is depth
    Vec3 {
        x: (v.x - center_x) * zoom + (width as f32 / 2.0),
        y: (-v.y - center_y) * zoom + (height as f32 / 2.0), // Flip Y for screen coords
        z: v.z, // Keep z for depth sorting/testing
    }
}

// =============================================================================
// Near-plane triangle clipping
// =============================================================================

/// Near plane distance threshold
pub const NEAR_PLANE: f32 = 0.1;

/// Result of clipping a triangle against the near plane.
/// A triangle can produce 0, 1, or 2 triangles after clipping.
pub enum ClipResult {
    /// Triangle is entirely behind the near plane (culled)
    Culled,
    /// Triangle is entirely in front of the near plane (unchanged)
    Unclipped,
    /// Triangle was clipped, producing 1 triangle
    One {
        v1: Vec3,
        v2: Vec3,
        v3: Vec3,
        /// Barycentric weights for interpolating attributes (UVs, colors)
        /// Each vertex's weights are (w1, w2, w3) relative to original triangle
        weights: [(f32, f32, f32); 3],
    },
    /// Triangle was clipped, producing 2 triangles
    Two {
        // First triangle
        t1_v1: Vec3,
        t1_v2: Vec3,
        t1_v3: Vec3,
        t1_weights: [(f32, f32, f32); 3],
        // Second triangle
        t2_v1: Vec3,
        t2_v2: Vec3,
        t2_v3: Vec3,
        t2_weights: [(f32, f32, f32); 3],
    },
}

/// Clip a triangle against the near plane (z = NEAR_PLANE).
/// Takes camera-space vertices and returns clipped result.
///
/// When a triangle crosses the near plane:
/// - If 1 vertex is in front, clip produces 1 triangle
/// - If 2 vertices are in front, clip produces 2 triangles (a quad split)
pub fn clip_triangle_to_near_plane(v1: Vec3, v2: Vec3, v3: Vec3) -> ClipResult {
    // Classify vertices: true = in front of near plane (visible)
    let in_front = [
        v1.z > NEAR_PLANE,
        v2.z > NEAR_PLANE,
        v3.z > NEAR_PLANE,
    ];

    let count_in_front = in_front.iter().filter(|&&b| b).count();

    match count_in_front {
        0 => ClipResult::Culled,
        3 => ClipResult::Unclipped,
        1 => {
            // One vertex in front: clip to single triangle
            // Find which vertex is in front
            let (front_idx, back1_idx, back2_idx) = if in_front[0] {
                (0, 1, 2)
            } else if in_front[1] {
                (1, 2, 0)
            } else {
                (2, 0, 1)
            };

            let verts = [v1, v2, v3];
            let front = verts[front_idx];
            let back1 = verts[back1_idx];
            let back2 = verts[back2_idx];

            // Find intersection points on the near plane
            let t1 = (NEAR_PLANE - front.z) / (back1.z - front.z);
            let t2 = (NEAR_PLANE - front.z) / (back2.z - front.z);

            let clip1 = lerp_vec3(front, back1, t1);
            let clip2 = lerp_vec3(front, back2, t2);

            // Build weight arrays for attribute interpolation
            // The front vertex keeps its original weights (1,0,0), (0,1,0), or (0,0,1)
            // The clipped vertices interpolate between front and back vertices
            let mut weights = [(0.0, 0.0, 0.0); 3];

            // Front vertex weight
            weights[0] = match front_idx {
                0 => (1.0, 0.0, 0.0),
                1 => (0.0, 1.0, 0.0),
                _ => (0.0, 0.0, 1.0),
            };

            // Clip1 is between front and back1
            let w_front = 1.0 - t1;
            let w_back1 = t1;
            weights[1] = match (front_idx, back1_idx) {
                (0, 1) => (w_front, w_back1, 0.0),
                (0, 2) => (w_front, 0.0, w_back1),
                (1, 0) => (w_back1, w_front, 0.0),
                (1, 2) => (0.0, w_front, w_back1),
                (2, 0) => (w_back1, 0.0, w_front),
                (2, 1) => (0.0, w_back1, w_front),
                _ => (0.0, 0.0, 0.0),
            };

            // Clip2 is between front and back2
            let w_front = 1.0 - t2;
            let w_back2 = t2;
            weights[2] = match (front_idx, back2_idx) {
                (0, 1) => (w_front, w_back2, 0.0),
                (0, 2) => (w_front, 0.0, w_back2),
                (1, 0) => (w_back2, w_front, 0.0),
                (1, 2) => (0.0, w_front, w_back2),
                (2, 0) => (w_back2, 0.0, w_front),
                (2, 1) => (0.0, w_back2, w_front),
                _ => (0.0, 0.0, 0.0),
            };

            ClipResult::One {
                v1: front,
                v2: clip1,
                v3: clip2,
                weights,
            }
        }
        2 => {
            // Two vertices in front: clip to quad (2 triangles)
            // Find which vertex is behind
            let (back_idx, front1_idx, front2_idx) = if !in_front[0] {
                (0, 1, 2)
            } else if !in_front[1] {
                (1, 2, 0)
            } else {
                (2, 0, 1)
            };

            let verts = [v1, v2, v3];
            let back = verts[back_idx];
            let front1 = verts[front1_idx];
            let front2 = verts[front2_idx];

            // Find intersection points
            let t1 = (NEAR_PLANE - front1.z) / (back.z - front1.z);
            let t2 = (NEAR_PLANE - front2.z) / (back.z - front2.z);

            let clip1 = lerp_vec3(front1, back, t1);
            let clip2 = lerp_vec3(front2, back, t2);

            // Build weights for first triangle: front1, clip1, front2
            let mut t1_weights = [(0.0, 0.0, 0.0); 3];
            t1_weights[0] = match front1_idx {
                0 => (1.0, 0.0, 0.0),
                1 => (0.0, 1.0, 0.0),
                _ => (0.0, 0.0, 1.0),
            };
            // clip1 interpolates between front1 and back
            let w_front1 = 1.0 - t1;
            let w_back = t1;
            t1_weights[1] = match (front1_idx, back_idx) {
                (0, 1) => (w_front1, w_back, 0.0),
                (0, 2) => (w_front1, 0.0, w_back),
                (1, 0) => (w_back, w_front1, 0.0),
                (1, 2) => (0.0, w_front1, w_back),
                (2, 0) => (w_back, 0.0, w_front1),
                (2, 1) => (0.0, w_back, w_front1),
                _ => (0.0, 0.0, 0.0),
            };
            t1_weights[2] = match front2_idx {
                0 => (1.0, 0.0, 0.0),
                1 => (0.0, 1.0, 0.0),
                _ => (0.0, 0.0, 1.0),
            };

            // Build weights for second triangle: clip1, clip2, front2
            let mut t2_weights = [(0.0, 0.0, 0.0); 3];
            t2_weights[0] = t1_weights[1]; // clip1 same as before
            // clip2 interpolates between front2 and back
            let w_front2 = 1.0 - t2;
            let w_back = t2;
            t2_weights[1] = match (front2_idx, back_idx) {
                (0, 1) => (w_front2, w_back, 0.0),
                (0, 2) => (w_front2, 0.0, w_back),
                (1, 0) => (w_back, w_front2, 0.0),
                (1, 2) => (0.0, w_front2, w_back),
                (2, 0) => (w_back, 0.0, w_front2),
                (2, 1) => (0.0, w_back, w_front2),
                _ => (0.0, 0.0, 0.0),
            };
            t2_weights[2] = t1_weights[2]; // front2 same as before

            ClipResult::Two {
                t1_v1: front1,
                t1_v2: clip1,
                t1_v3: front2,
                t1_weights,
                t2_v1: clip1,
                t2_v2: clip2,
                t2_v3: front2,
                t2_weights,
            }
        }
        _ => unreachable!(),
    }
}

/// Linear interpolation between two Vec3 values
fn lerp_vec3(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    Vec3::new(
        a.x + (b.x - a.x) * t,
        a.y + (b.y - a.y) * t,
        a.z + (b.z - a.z) * t,
    )
}

/// Clip an edge against the near plane for wireframe rendering.
/// Returns None if entirely behind, Some((start, end)) for the visible portion.
pub fn clip_edge_to_near_plane(v1: Vec3, v2: Vec3) -> Option<(Vec3, Vec3)> {
    let in_front1 = v1.z > NEAR_PLANE;
    let in_front2 = v2.z > NEAR_PLANE;

    match (in_front1, in_front2) {
        (false, false) => None, // Both behind
        (true, true) => Some((v1, v2)), // Both in front
        (true, false) => {
            // v1 in front, v2 behind - clip v2
            let t = (NEAR_PLANE - v1.z) / (v2.z - v1.z);
            let clip = lerp_vec3(v1, v2, t);
            Some((v1, clip))
        }
        (false, true) => {
            // v1 behind, v2 in front - clip v1
            let t = (NEAR_PLANE - v2.z) / (v1.z - v2.z);
            let clip = lerp_vec3(v2, v1, t);
            Some((clip, v2))
        }
    }
}

/// Calculate barycentric coordinates for point p in triangle (v1, v2, v3)
/// Returns (u, v, w) where u + v + w = 1 if point is inside triangle
pub fn barycentric(p: Vec3, v1: Vec3, v2: Vec3, v3: Vec3) -> Vec3 {
    let d = (v2.y - v3.y) * (v1.x - v3.x) + (v3.x - v2.x) * (v1.y - v3.y);

    // Threshold lowered to allow steep-angle triangles (nearly edge-on to camera)
    // Very thin triangles at grazing angles have small screen-space area
    if d.abs() < 0.00001 {
        return Vec3::new(-1.0, -1.0, -1.0); // Degenerate triangle
    }

    let u = ((v2.y - v3.y) * (p.x - v3.x) + (v3.x - v2.x) * (p.y - v3.y)) / d;
    let v = ((v3.y - v1.y) * (p.x - v3.x) + (v1.x - v3.x) * (p.y - v3.y)) / d;
    let w = 1.0 - u - v;

    Vec3::new(u, v, w)
}

/// Ray-triangle intersection using Möller–Trumbore algorithm
/// Returns Some(t) if ray hits, where t is the distance along the ray
/// ray_origin: starting point of ray
/// ray_dir: normalized direction of ray
/// v0, v1, v2: triangle vertices
/// Note: Currently unused but reserved for future 3D picking feature
#[allow(dead_code)]
pub fn ray_triangle_intersect(
    ray_origin: Vec3,
    ray_dir: Vec3,
    v0: Vec3,
    v1: Vec3,
    v2: Vec3,
) -> Option<f32> {
    const EPSILON: f32 = 0.0000001;

    let edge1 = v1 - v0;
    let edge2 = v2 - v0;
    let h = ray_dir.cross(edge2);
    let a = edge1.dot(h);

    // Ray is parallel to triangle
    if a.abs() < EPSILON {
        return None;
    }

    let f = 1.0 / a;
    let s = ray_origin - v0;
    let u = f * s.dot(h);

    if u < 0.0 || u > 1.0 {
        return None;
    }

    let q = s.cross(edge1);
    let v = f * ray_dir.dot(q);

    if v < 0.0 || u + v > 1.0 {
        return None;
    }

    let t = f * edge2.dot(q);

    if t > EPSILON {
        Some(t)
    } else {
        None
    }
}

/// Generate a ray from screen coordinates through the camera
/// Returns (ray_origin, ray_direction)
/// screen_x, screen_y: pixel coordinates
/// screen_width, screen_height: framebuffer dimensions
/// camera: the camera to cast from
/// Note: Currently unused but reserved for future 3D picking feature
#[allow(dead_code)]
pub fn screen_to_ray(
    screen_x: f32,
    screen_y: f32,
    screen_width: usize,
    screen_height: usize,
    cam_pos: Vec3,
    cam_x: Vec3,
    cam_y: Vec3,
    cam_z: Vec3,
) -> (Vec3, Vec3) {
    // Reverse the projection math from project()
    const DISTANCE: f32 = 5.0;
    const SCALE: f32 = 0.75;

    let vs = (screen_width.min(screen_height) as f32 / 2.0) * SCALE;

    // Convert screen coordinates to normalized device coordinates
    let ndc_x = (screen_x - screen_width as f32 / 2.0) / vs;
    let ndc_y = (screen_y - screen_height as f32 / 2.0) / vs;

    // The ray direction in camera space
    // At z=1 (unit distance in front of camera), the point would be at (ndc_x, ndc_y, 1)
    let cam_space_dir = Vec3::new(ndc_x, ndc_y, 1.0).normalize();

    // Transform ray direction from camera space to world space
    let world_dir = Vec3::new(
        cam_space_dir.x * cam_x.x + cam_space_dir.y * cam_y.x + cam_space_dir.z * cam_z.x,
        cam_space_dir.x * cam_x.y + cam_space_dir.y * cam_y.y + cam_space_dir.z * cam_z.y,
        cam_space_dir.x * cam_x.z + cam_space_dir.y * cam_y.z + cam_space_dir.z * cam_z.z,
    ).normalize();

    (cam_pos, world_dir)
}

// =============================================================================
// Viewport projection helpers
// =============================================================================

/// Project a world-space point to framebuffer coordinates.
/// Used by both editor and modeler viewports for UI overlay rendering.
pub fn world_to_screen(
    world_pos: Vec3,
    camera_pos: Vec3,
    basis_x: Vec3,
    basis_y: Vec3,
    basis_z: Vec3,
    fb_width: usize,
    fb_height: usize,
) -> Option<(f32, f32)> {
    let rel = world_pos - camera_pos;
    let cam_z = rel.dot(basis_z);

    // Behind camera
    if cam_z <= 0.1 {
        return None;
    }

    let cam_x = rel.dot(basis_x);
    let cam_y = rel.dot(basis_y);

    // Same projection as the rasterizer
    const SCALE: f32 = 0.75;
    let vs = (fb_width.min(fb_height) as f32 / 2.0) * SCALE;
    let ud = 5.0;
    let us = ud - 1.0;

    let denom = cam_z + ud;
    let sx = (cam_x * us / denom) * vs + (fb_width as f32 / 2.0);
    let sy = (cam_y * us / denom) * vs + (fb_height as f32 / 2.0);

    Some((sx, sy))
}

/// Project a world-space point to framebuffer coordinates with depth.
/// Returns (screen_x, screen_y, depth) where depth is camera-space Z.
pub fn world_to_screen_with_depth(
    world_pos: Vec3,
    camera_pos: Vec3,
    basis_x: Vec3,
    basis_y: Vec3,
    basis_z: Vec3,
    fb_width: usize,
    fb_height: usize,
) -> Option<(f32, f32, f32)> {
    let rel = world_pos - camera_pos;
    let cam_z = rel.dot(basis_z);

    // Behind camera
    if cam_z <= 0.1 {
        return None;
    }

    let cam_x = rel.dot(basis_x);
    let cam_y = rel.dot(basis_y);

    // Same projection as the rasterizer
    const SCALE: f32 = 0.75;
    let vs = (fb_width.min(fb_height) as f32 / 2.0) * SCALE;
    let ud = 5.0;
    let us = ud - 1.0;

    let denom = cam_z + ud;
    let sx = (cam_x * us / denom) * vs + (fb_width as f32 / 2.0);
    let sy = (cam_y * us / denom) * vs + (fb_height as f32 / 2.0);

    Some((sx, sy, cam_z))
}

/// Calculate distance from point to line segment in 2D screen space.
pub fn point_to_segment_distance(
    px: f32, py: f32,      // Point
    x1: f32, y1: f32,      // Segment start
    x2: f32, y2: f32,      // Segment end
) -> f32 {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let len_sq = dx * dx + dy * dy;

    if len_sq < 1e-6 {
        // Segment is essentially a point
        let pdx = px - x1;
        let pdy = py - y1;
        return (pdx * pdx + pdy * pdy).sqrt();
    }

    // Project point onto line segment
    let t = ((px - x1) * dx + (py - y1) * dy) / len_sq;
    let t = t.clamp(0.0, 1.0);

    // Find closest point on segment
    let closest_x = x1 + t * dx;
    let closest_y = y1 + t * dy;

    // Distance from point to closest point
    let dist_x = px - closest_x;
    let dist_y = py - closest_y;
    (dist_x * dist_x + dist_y * dist_y).sqrt()
}

/// Test if point is inside 2D triangle using sign-based edge test.
/// Works regardless of triangle winding order.
pub fn point_in_triangle_2d(
    px: f32, py: f32,      // Point
    x1: f32, y1: f32,      // Triangle v1
    x2: f32, y2: f32,      // Triangle v2
    x3: f32, y3: f32,      // Triangle v3
) -> bool {
    fn sign(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
        (px - bx) * (ay - by) - (ax - bx) * (py - by)
    }

    let d1 = sign(px, py, x1, y1, x2, y2);
    let d2 = sign(px, py, x2, y2, x3, y3);
    let d3 = sign(px, py, x3, y3, x1, y1);

    let has_neg = (d1 < 0.0) || (d2 < 0.0) || (d3 < 0.0);
    let has_pos = (d1 > 0.0) || (d2 > 0.0) || (d3 > 0.0);

    // Point is inside if all signs are same (all positive or all negative)
    !(has_neg && has_pos)
}

// =============================================================================
// 4x4 Matrix operations (for transforms)
// =============================================================================

/// 4x4 transformation matrix type
pub type Mat4 = [[f32; 4]; 4];

/// Identity matrix
pub fn mat4_identity() -> Mat4 {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// Create translation matrix
pub fn mat4_translation(t: Vec3) -> Mat4 {
    [
        [1.0, 0.0, 0.0, t.x],
        [0.0, 1.0, 0.0, t.y],
        [0.0, 0.0, 1.0, t.z],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// Build a rotation matrix from euler angles (degrees).
/// Rotation order: Z * Y * X (matches Blender default).
pub fn mat4_rotation(rot: Vec3) -> Mat4 {
    let (sx, cx) = rot.x.to_radians().sin_cos();
    let (sy, cy) = rot.y.to_radians().sin_cos();
    let (sz, cz) = rot.z.to_radians().sin_cos();

    [
        [cy * cz, sx * sy * cz - cx * sz, cx * sy * cz + sx * sz, 0.0],
        [cy * sz, sx * sy * sz + cx * cz, cx * sy * sz - sx * cz, 0.0],
        [-sy, sx * cy, cx * cy, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// Multiply two 4x4 matrices
pub fn mat4_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut result = [[0.0; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            for k in 0..4 {
                result[i][j] += a[i][k] * b[k][j];
            }
        }
    }
    result
}

/// Transform a point by a 4x4 matrix
pub fn mat4_transform_point(m: &Mat4, p: Vec3) -> Vec3 {
    Vec3::new(
        m[0][0] * p.x + m[0][1] * p.y + m[0][2] * p.z + m[0][3],
        m[1][0] * p.x + m[1][1] * p.y + m[1][2] * p.z + m[1][3],
        m[2][0] * p.x + m[2][1] * p.y + m[2][2] * p.z + m[2][3],
    )
}

/// Build a combined transform matrix from position and rotation
pub fn mat4_from_position_rotation(position: Vec3, rotation: Vec3) -> Mat4 {
    let rot_mat = mat4_rotation(rotation);
    let trans_mat = mat4_translation(position);
    mat4_mul(&trans_mat, &rot_mat)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vec3_dot() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(4.0, 5.0, 6.0);
        assert!((a.dot(b) - 32.0).abs() < 0.001);
    }

    #[test]
    fn test_vec3_cross() {
        let a = Vec3::new(1.0, 0.0, 0.0);
        let b = Vec3::new(0.0, 1.0, 0.0);
        let c = a.cross(b);
        assert!((c.z - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_barycentric_inside() {
        let v1 = Vec3::new(0.0, 0.0, 0.0);
        let v2 = Vec3::new(10.0, 0.0, 0.0);
        let v3 = Vec3::new(5.0, 10.0, 0.0);
        let p = Vec3::new(5.0, 3.0, 0.0);
        let bc = barycentric(p, v1, v2, v3);
        assert!(bc.x >= 0.0 && bc.y >= 0.0 && bc.z >= 0.0);
    }
}
