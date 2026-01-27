//! Drawing utilities for 3D rendering
//!
//! Standalone functions for drawing lines, grids, and test geometry.

use super::camera::Camera;
use super::math::{Vec2, Vec3, world_to_screen, NEAR_PLANE};
use super::render::Framebuffer;
use super::types::{Color, Face, Vertex};

/// Draw a 3D line with proper near-plane clipping.
/// Used by both world editor and modeler for grid/wireframe rendering.
pub fn draw_3d_line_clipped(
    fb: &mut Framebuffer,
    camera: &Camera,
    p0: Vec3,
    p1: Vec3,
    color: Color,
) {
    // Transform to camera space
    let rel0 = p0 - camera.position;
    let rel1 = p1 - camera.position;

    let z0 = rel0.dot(camera.basis_z);
    let z1 = rel1.dot(camera.basis_z);

    // Both behind camera - skip entirely
    if z0 <= NEAR_PLANE && z1 <= NEAR_PLANE {
        return;
    }

    // Clip line to near plane if needed
    let (clipped_p0, clipped_p1) = if z0 <= NEAR_PLANE {
        let t = (NEAR_PLANE - z0) / (z1 - z0);
        let new_p0 = p0 + (p1 - p0) * t;
        (new_p0, p1)
    } else if z1 <= NEAR_PLANE {
        let t = (NEAR_PLANE - z0) / (z1 - z0);
        let new_p1 = p0 + (p1 - p0) * t;
        (p0, new_p1)
    } else {
        (p0, p1)
    };

    // Project clipped endpoints to screen space
    let s0 = world_to_screen(
        clipped_p0,
        camera.position,
        camera.basis_x,
        camera.basis_y,
        camera.basis_z,
        fb.width,
        fb.height,
    );
    let s1 = world_to_screen(
        clipped_p1,
        camera.position,
        camera.basis_x,
        camera.basis_y,
        camera.basis_z,
        fb.width,
        fb.height,
    );

    if let (Some((x0, y0)), Some((x1, y1))) = (s0, s1) {
        fb.draw_line(x0 as i32, y0 as i32, x1 as i32, y1 as i32, color);
    }
}

/// Draw a floor grid on a horizontal plane.
/// Uses short segments for better near-plane clipping behavior.
///
/// # Arguments
/// * `fb` - Framebuffer to draw to
/// * `camera` - Camera for projection
/// * `y` - Y height of the grid plane
/// * `spacing` - Distance between grid lines
/// * `extent` - Half-size of the grid (grid goes from -extent to +extent)
/// * `grid_color` - Color for regular grid lines
/// * `x_axis_color` - Color for the X axis (line at Z=0)
/// * `z_axis_color` - Color for the Z axis (line at X=0)
pub fn draw_floor_grid(
    fb: &mut Framebuffer,
    camera: &Camera,
    y: f32,
    spacing: f32,
    extent: f32,
    grid_color: Color,
    x_axis_color: Color,
    z_axis_color: Color,
) {
    // Use shorter segments for better clipping behavior
    let segment_length = spacing;

    // X-parallel lines (varying X, fixed Z)
    let mut z = -extent;
    while z <= extent {
        let is_z_axis = z.abs() < 0.001;
        let color = if is_z_axis { z_axis_color } else { grid_color };

        let mut x = -extent;
        while x < extent {
            let x_end = (x + segment_length).min(extent);
            draw_3d_line_clipped(
                fb,
                camera,
                Vec3::new(x, y, z),
                Vec3::new(x_end, y, z),
                color,
            );
            x += segment_length;
        }
        z += spacing;
    }

    // Z-parallel lines (fixed X, varying Z)
    let mut x = -extent;
    while x <= extent {
        let is_x_axis = x.abs() < 0.001;
        let color = if is_x_axis { x_axis_color } else { grid_color };

        let mut z = -extent;
        while z < extent {
            let z_end = (z + segment_length).min(extent);
            draw_3d_line_clipped(
                fb,
                camera,
                Vec3::new(x, y, z),
                Vec3::new(x, y, z_end),
                color,
            );
            z += segment_length;
        }
        x += spacing;
    }
}

/// Create a simple test cube mesh
pub fn create_test_cube() -> (Vec<Vertex>, Vec<Face>) {
    let mut vertices = Vec::new();
    let mut faces = Vec::new();

    // Cube vertices with positions, UVs, and normals
    let positions = [
        // Front face
        Vec3::new(-1.0, -1.0, 1.0),
        Vec3::new(1.0, -1.0, 1.0),
        Vec3::new(1.0, 1.0, 1.0),
        Vec3::new(-1.0, 1.0, 1.0),
        // Back face
        Vec3::new(-1.0, -1.0, -1.0),
        Vec3::new(-1.0, 1.0, -1.0),
        Vec3::new(1.0, 1.0, -1.0),
        Vec3::new(1.0, -1.0, -1.0),
        // Top face
        Vec3::new(-1.0, 1.0, -1.0),
        Vec3::new(-1.0, 1.0, 1.0),
        Vec3::new(1.0, 1.0, 1.0),
        Vec3::new(1.0, 1.0, -1.0),
        // Bottom face
        Vec3::new(-1.0, -1.0, -1.0),
        Vec3::new(1.0, -1.0, -1.0),
        Vec3::new(1.0, -1.0, 1.0),
        Vec3::new(-1.0, -1.0, 1.0),
        // Right face
        Vec3::new(1.0, -1.0, -1.0),
        Vec3::new(1.0, 1.0, -1.0),
        Vec3::new(1.0, 1.0, 1.0),
        Vec3::new(1.0, -1.0, 1.0),
        // Left face
        Vec3::new(-1.0, -1.0, -1.0),
        Vec3::new(-1.0, -1.0, 1.0),
        Vec3::new(-1.0, 1.0, 1.0),
        Vec3::new(-1.0, 1.0, -1.0),
    ];

    let normals = [
        Vec3::new(0.0, 0.0, 1.0),  // Front
        Vec3::new(0.0, 0.0, -1.0), // Back
        Vec3::new(0.0, 1.0, 0.0),  // Top
        Vec3::new(0.0, -1.0, 0.0), // Bottom
        Vec3::new(1.0, 0.0, 0.0),  // Right
        Vec3::new(-1.0, 0.0, 0.0), // Left
    ];

    let uvs = [
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(0.0, 1.0),
    ];

    // Build vertices for each face
    for face_idx in 0..6 {
        let base = face_idx * 4;
        let normal = normals[face_idx];

        for i in 0..4 {
            vertices.push(Vertex {
                pos: positions[base + i],
                uv: uvs[i],
                normal,
                color: Color::NEUTRAL,
                bone_index: None,
            });
        }

        // Two triangles per face
        let vbase = face_idx * 4;
        faces.push(Face::with_texture(vbase, vbase + 1, vbase + 2, 0));
        faces.push(Face::with_texture(vbase, vbase + 2, vbase + 3, 0));
    }

    (vertices, faces)
}
