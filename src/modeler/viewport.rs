//! 3D Viewport for the modeler - renders models using the software rasterizer

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext};
use crate::rasterizer::{
    Framebuffer, render_mesh, Color as RasterColor, Vec3, Vec2 as RasterVec2,
    Vertex as RasterVertex, Face as RasterFace, WIDTH, HEIGHT,
};
use super::state::{ModelerState, ModelerSelection, SelectMode};
use super::model::{Model, PartTransform};

/// Project a world-space point to framebuffer coordinates
fn world_to_screen(
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

/// Build a 4x4 rotation matrix from euler angles (degrees)
fn rotation_matrix(rot: Vec3) -> [[f32; 4]; 4] {
    let (sx, cx) = rot.x.to_radians().sin_cos();
    let (sy, cy) = rot.y.to_radians().sin_cos();
    let (sz, cz) = rot.z.to_radians().sin_cos();

    // Rotation order: Z * Y * X (matches Blender default)
    [
        [cy * cz, sx * sy * cz - cx * sz, cx * sy * cz + sx * sz, 0.0],
        [cy * sz, sx * sy * sz + cx * cz, cx * sy * sz - sx * cz, 0.0],
        [-sy, sx * cy, cx * cy, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// Transform a point by a 4x4 matrix
fn transform_point(m: &[[f32; 4]; 4], p: Vec3) -> Vec3 {
    Vec3::new(
        m[0][0] * p.x + m[0][1] * p.y + m[0][2] * p.z + m[0][3],
        m[1][0] * p.x + m[1][1] * p.y + m[1][2] * p.z + m[1][3],
        m[2][0] * p.x + m[2][1] * p.y + m[2][2] * p.z + m[2][3],
    )
}

/// Multiply two 4x4 matrices
fn mat_mul(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
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

/// Create translation matrix
fn translation_matrix(t: Vec3) -> [[f32; 4]; 4] {
    [
        [1.0, 0.0, 0.0, t.x],
        [0.0, 1.0, 0.0, t.y],
        [0.0, 0.0, 1.0, t.z],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// Identity matrix
fn identity_matrix() -> [[f32; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// Compute world matrices for all parts given animation pose
fn compute_world_matrices(model: &Model, pose: &[PartTransform]) -> Vec<[[f32; 4]; 4]> {
    let mut matrices = Vec::with_capacity(model.parts.len());

    for (i, part) in model.parts.iter().enumerate() {
        let transform = pose.get(i).copied().unwrap_or_default();

        // Build local matrix: translate by position offset, then rotate
        let rot_mat = rotation_matrix(transform.rotation);
        let trans_mat = translation_matrix(transform.position + part.pivot);

        let local = mat_mul(&trans_mat, &rot_mat);

        // Multiply by parent's world matrix
        let world = if let Some(parent_idx) = part.parent {
            if parent_idx < matrices.len() {
                mat_mul(&matrices[parent_idx], &local)
            } else {
                local
            }
        } else {
            local
        };

        matrices.push(world);
    }

    matrices
}

/// Draw the 3D modeler viewport
pub fn draw_modeler_viewport(
    ctx: &mut UiContext,
    rect: Rect,
    state: &mut ModelerState,
    fb: &mut Framebuffer,
) {
    // Resize framebuffer based on resolution setting
    let (target_w, target_h) = if state.raster_settings.low_resolution {
        (WIDTH, HEIGHT) // 320x240
    } else {
        (crate::rasterizer::WIDTH_HI, crate::rasterizer::HEIGHT_HI) // 640x480
    };
    fb.resize(target_w, target_h);

    let mouse_pos = (ctx.mouse.x, ctx.mouse.y);
    let inside_viewport = ctx.mouse.inside(&rect);

    // Calculate viewport scaling
    let fb_width = fb.width;
    let fb_height = fb.height;
    let fb_aspect = fb_width as f32 / fb_height as f32;
    let rect_aspect = rect.w / rect.h;
    let (draw_w, draw_h, draw_x, draw_y) = if fb_aspect > rect_aspect {
        let w = rect.w;
        let h = rect.w / fb_aspect;
        (w, h, rect.x, rect.y + (rect.h - h) * 0.5)
    } else {
        let h = rect.h;
        let w = rect.h * fb_aspect;
        (w, h, rect.x + (rect.w - w) * 0.5, rect.y)
    };

    // Helper to convert screen mouse to framebuffer coordinates
    let screen_to_fb = |mx: f32, my: f32| -> Option<(f32, f32)> {
        if mx >= draw_x && mx < draw_x + draw_w && my >= draw_y && my < draw_y + draw_h {
            let fb_x = (mx - draw_x) / draw_w * fb_width as f32;
            let fb_y = (my - draw_y) / draw_h * fb_height as f32;
            Some((fb_x, fb_y))
        } else {
            None
        }
    };

    // Camera rotation with right mouse button (orbit)
    if ctx.mouse.right_down && inside_viewport {
        if state.viewport_mouse_captured {
            let dx = (mouse_pos.1 - state.viewport_last_mouse.1) * 0.005;
            let dy = -(mouse_pos.0 - state.viewport_last_mouse.0) * 0.005;
            state.camera.rotate(dx, dy);
        }
        state.viewport_mouse_captured = true;
    } else if !ctx.mouse.right_down {
        state.viewport_mouse_captured = false;
    }

    // Keyboard camera movement (WASD + Q/E)
    let move_speed = 10.0; // Smaller scale for models
    if inside_viewport || state.viewport_mouse_captured {
        if is_key_down(KeyCode::W) {
            state.camera.position = state.camera.position + state.camera.basis_z * move_speed;
        }
        if is_key_down(KeyCode::S) && !is_key_down(KeyCode::LeftControl) && !is_key_down(KeyCode::RightControl) {
            state.camera.position = state.camera.position - state.camera.basis_z * move_speed;
        }
        if is_key_down(KeyCode::A) {
            state.camera.position = state.camera.position - state.camera.basis_x * move_speed;
        }
        if is_key_down(KeyCode::D) {
            state.camera.position = state.camera.position + state.camera.basis_x * move_speed;
        }
        if is_key_down(KeyCode::Q) {
            state.camera.position = state.camera.position - state.camera.basis_y * move_speed;
        }
        if is_key_down(KeyCode::E) {
            state.camera.position = state.camera.position + state.camera.basis_y * move_speed;
        }
    }

    // Mouse wheel zoom
    if inside_viewport {
        let scroll = mouse_wheel().1;
        if scroll != 0.0 {
            let zoom_speed = 20.0;
            let zoom_dir = if scroll > 0.0 { 1.0 } else { -1.0 };
            state.camera.position = state.camera.position + state.camera.basis_z * zoom_speed * zoom_dir;
        }
    }

    // Update mouse position for next frame
    state.viewport_last_mouse = mouse_pos;

    // Clear framebuffer
    fb.clear(RasterColor::new(40, 40, 50));

    // Draw grid on floor
    draw_grid(fb, &state.camera, 0.0, 50.0, 10);

    // Get current pose for animation
    let pose = state.get_current_pose();

    // Compute world matrices for all parts
    let world_matrices = compute_world_matrices(&state.model, &pose);

    // Build render data for all parts
    let mut all_vertices: Vec<RasterVertex> = Vec::new();
    let mut all_faces: Vec<RasterFace> = Vec::new();

    for (part_idx, part) in state.model.parts.iter().enumerate() {
        if !part.visible {
            continue;
        }

        let world_mat = &world_matrices[part_idx];
        let vertex_offset = all_vertices.len();

        // Transform vertices
        for vert in &part.vertices {
            let world_pos = transform_point(world_mat, vert.position);

            // Calculate normal (simplified - just use up vector for now)
            let normal = Vec3::new(0.0, 1.0, 0.0);

            all_vertices.push(RasterVertex {
                pos: world_pos,
                uv: RasterVec2::new(vert.uv.x, vert.uv.y),
                normal,
            });
        }

        // Add faces with offset indices
        for face in &part.faces {
            all_faces.push(RasterFace {
                v0: face.indices[0] + vertex_offset,
                v1: face.indices[1] + vertex_offset,
                v2: face.indices[2] + vertex_offset,
                texture_id: None, // TODO: Use atlas texture
            });
        }
    }

    // Render using software rasterizer
    let empty_textures: Vec<crate::rasterizer::Texture> = Vec::new();
    render_mesh(fb, &all_vertices, &all_faces, &empty_textures, &state.camera, &state.raster_settings);

    // Draw part/vertex/edge/face overlays based on selection mode
    draw_selection_overlays(ctx, fb, state, &world_matrices, screen_to_fb);

    // Handle click selection
    if inside_viewport && ctx.mouse.left_pressed && !ctx.mouse.right_down {
        handle_selection_click(ctx, state, &world_matrices, screen_to_fb, fb.width, fb.height);
    }

    // Convert framebuffer to texture and draw
    let texture = Texture2D::from_rgba8(fb.width as u16, fb.height as u16, &fb.pixels);
    texture.set_filter(FilterMode::Nearest);

    draw_texture_ex(
        &texture,
        draw_x,
        draw_y,
        WHITE,
        DrawTextureParams {
            dest_size: Some(macroquad::math::Vec2::new(draw_w, draw_h)),
            ..Default::default()
        },
    );

    // Draw viewport border
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, Color::from_rgba(60, 60, 60, 255));

    // Draw camera info
    draw_text(
        &format!(
            "Cam: ({:.0}, {:.0}, {:.0})",
            state.camera.position.x,
            state.camera.position.y,
            state.camera.position.z,
        ),
        rect.x + 5.0,
        rect.bottom() - 5.0,
        12.0,
        Color::from_rgba(180, 180, 180, 255),
    );
}

/// Draw floor grid
fn draw_grid(fb: &mut Framebuffer, camera: &crate::rasterizer::Camera, y: f32, spacing: f32, count: i32) {
    let grid_color = RasterColor::new(60, 60, 70);
    let axis_color_x = RasterColor::new(150, 60, 60);
    let axis_color_z = RasterColor::new(60, 60, 150);

    let extent = spacing * count as f32;

    // Draw grid lines
    for i in -count..=count {
        let offset = i as f32 * spacing;

        // X-parallel lines
        let color = if i == 0 { axis_color_z } else { grid_color };
        draw_3d_line(fb, Vec3::new(-extent, y, offset), Vec3::new(extent, y, offset), camera, color);

        // Z-parallel lines
        let color = if i == 0 { axis_color_x } else { grid_color };
        draw_3d_line(fb, Vec3::new(offset, y, -extent), Vec3::new(offset, y, extent), camera, color);
    }

    // Draw Y axis
    draw_3d_line(fb, Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 100.0, 0.0), camera, RasterColor::new(60, 150, 60));
}

/// Draw a 3D line into the framebuffer
fn draw_3d_line(
    fb: &mut Framebuffer,
    p0: Vec3,
    p1: Vec3,
    camera: &crate::rasterizer::Camera,
    color: RasterColor,
) {
    const NEAR_PLANE: f32 = 0.1;

    let rel0 = p0 - camera.position;
    let rel1 = p1 - camera.position;

    let z0 = rel0.dot(camera.basis_z);
    let z1 = rel1.dot(camera.basis_z);

    if z0 <= NEAR_PLANE && z1 <= NEAR_PLANE {
        return;
    }

    // Clip to near plane
    let (clipped_p0, clipped_p1) = if z0 <= NEAR_PLANE {
        let t = (NEAR_PLANE - z0) / (z1 - z0);
        (p0 + (p1 - p0) * t, p1)
    } else if z1 <= NEAR_PLANE {
        let t = (NEAR_PLANE - z0) / (z1 - z0);
        (p0, p0 + (p1 - p0) * t)
    } else {
        (p0, p1)
    };

    let s0 = world_to_screen(clipped_p0, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height);
    let s1 = world_to_screen(clipped_p1, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height);

    if let (Some((x0f, y0f)), Some((x1f, y1f))) = (s0, s1) {
        fb.draw_line(x0f as i32, y0f as i32, x1f as i32, y1f as i32, color);
    }
}

/// Draw selection overlays (vertices, edges, etc.)
fn draw_selection_overlays<F>(
    _ctx: &mut UiContext,
    fb: &mut Framebuffer,
    state: &ModelerState,
    world_matrices: &[[[f32; 4]; 4]],
    _screen_to_fb: F,
) where F: Fn(f32, f32) -> Option<(f32, f32)>
{
    // Draw vertices if in vertex select mode
    if state.select_mode == SelectMode::Vertex || state.select_mode == SelectMode::Part {
        for (part_idx, part) in state.model.parts.iter().enumerate() {
            if !part.visible {
                continue;
            }

            let world_mat = &world_matrices[part_idx];

            for (vert_idx, vert) in part.vertices.iter().enumerate() {
                let world_pos = transform_point(world_mat, vert.position);

                if let Some((sx, sy)) = world_to_screen(
                    world_pos,
                    state.camera.position,
                    state.camera.basis_x,
                    state.camera.basis_y,
                    state.camera.basis_z,
                    fb.width,
                    fb.height,
                ) {
                    // Check if selected
                    let is_selected = match &state.selection {
                        ModelerSelection::Vertices { part, verts } => {
                            *part == part_idx && verts.contains(&vert_idx)
                        }
                        ModelerSelection::Parts(parts) => parts.contains(&part_idx),
                        _ => false,
                    };

                    let color = if is_selected {
                        RasterColor::new(100, 255, 100)
                    } else {
                        RasterColor::with_alpha(180, 180, 200, 180)
                    };

                    let radius = if is_selected { 4 } else { 2 };
                    fb.draw_circle(sx as i32, sy as i32, radius, color);
                }
            }
        }
    }

    // Draw edges if in edge select mode
    if state.select_mode == SelectMode::Edge {
        for (part_idx, part) in state.model.parts.iter().enumerate() {
            if !part.visible {
                continue;
            }

            let world_mat = &world_matrices[part_idx];

            // Collect unique edges from faces
            let mut drawn_edges: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();

            for face in &part.faces {
                for i in 0..3 {
                    let v0 = face.indices[i];
                    let v1 = face.indices[(i + 1) % 3];
                    let edge = if v0 < v1 { (v0, v1) } else { (v1, v0) };

                    if drawn_edges.insert(edge) {
                        let p0 = transform_point(world_mat, part.vertices[v0].position);
                        let p1 = transform_point(world_mat, part.vertices[v1].position);

                        let is_selected = match &state.selection {
                            ModelerSelection::Edges { part, edges } => {
                                *part == part_idx && edges.contains(&edge)
                            }
                            _ => false,
                        };

                        let color = if is_selected {
                            RasterColor::new(100, 255, 100)
                        } else {
                            RasterColor::new(100, 100, 120)
                        };

                        draw_3d_line(fb, p0, p1, &state.camera, color);
                    }
                }
            }
        }
    }

    // Draw selected part outline
    if let ModelerSelection::Parts(parts) = &state.selection {
        for &part_idx in parts {
            if let Some(part) = state.model.parts.get(part_idx) {
                if !part.visible {
                    continue;
                }

                let world_mat = &world_matrices[part_idx];

                // Draw all edges of selected part
                for face in &part.faces {
                    for i in 0..3 {
                        let v0 = face.indices[i];
                        let v1 = face.indices[(i + 1) % 3];

                        let p0 = transform_point(world_mat, part.vertices[v0].position);
                        let p1 = transform_point(world_mat, part.vertices[v1].position);

                        draw_3d_line(fb, p0, p1, &state.camera, RasterColor::new(255, 200, 50));
                    }
                }
            }
        }
    }
}

/// Handle click selection in viewport
fn handle_selection_click<F>(
    ctx: &UiContext,
    state: &mut ModelerState,
    world_matrices: &[[[f32; 4]; 4]],
    screen_to_fb: F,
    fb_width: usize,
    fb_height: usize,
) where F: Fn(f32, f32) -> Option<(f32, f32)>
{
    let Some((fb_x, fb_y)) = screen_to_fb(ctx.mouse.x, ctx.mouse.y) else {
        return;
    };

    match state.select_mode {
        SelectMode::Part => {
            // Find closest part (check all vertices, pick part with closest vertex)
            let mut closest: Option<(usize, f32)> = None;

            for (part_idx, part) in state.model.parts.iter().enumerate() {
                if !part.visible {
                    continue;
                }

                let world_mat = &world_matrices[part_idx];

                for vert in &part.vertices {
                    let world_pos = transform_point(world_mat, vert.position);

                    if let Some((sx, sy)) = world_to_screen(
                        world_pos,
                        state.camera.position,
                        state.camera.basis_x,
                        state.camera.basis_y,
                        state.camera.basis_z,
                        fb_width,
                        fb_height,
                    ) {
                        let dist = ((fb_x - sx).powi(2) + (fb_y - sy).powi(2)).sqrt();
                        if dist < 20.0 {
                            if closest.map_or(true, |(_, best_dist)| dist < best_dist) {
                                closest = Some((part_idx, dist));
                            }
                        }
                    }
                }
            }

            if let Some((part_idx, _)) = closest {
                state.selection = ModelerSelection::Parts(vec![part_idx]);
                state.set_status(&format!("Selected part: {}", state.model.parts[part_idx].name), 1.5);
            } else {
                state.selection = ModelerSelection::None;
            }
        }

        SelectMode::Vertex => {
            // Find closest vertex
            let mut closest: Option<(usize, usize, f32)> = None;

            for (part_idx, part) in state.model.parts.iter().enumerate() {
                if !part.visible {
                    continue;
                }

                let world_mat = &world_matrices[part_idx];

                for (vert_idx, vert) in part.vertices.iter().enumerate() {
                    let world_pos = transform_point(world_mat, vert.position);

                    if let Some((sx, sy)) = world_to_screen(
                        world_pos,
                        state.camera.position,
                        state.camera.basis_x,
                        state.camera.basis_y,
                        state.camera.basis_z,
                        fb_width,
                        fb_height,
                    ) {
                        let dist = ((fb_x - sx).powi(2) + (fb_y - sy).powi(2)).sqrt();
                        if dist < 10.0 {
                            if closest.map_or(true, |(_, _, best_dist)| dist < best_dist) {
                                closest = Some((part_idx, vert_idx, dist));
                            }
                        }
                    }
                }
            }

            if let Some((part_idx, vert_idx, _)) = closest {
                state.selection = ModelerSelection::Vertices {
                    part: part_idx,
                    verts: vec![vert_idx],
                };
                state.set_status(&format!("Selected vertex {}", vert_idx), 1.5);
            } else {
                state.selection = ModelerSelection::None;
            }
        }

        SelectMode::Edge => {
            // TODO: Implement edge selection
            state.set_status("Edge selection TODO", 1.0);
        }

        SelectMode::Face => {
            // TODO: Implement face selection
            state.set_status("Face selection TODO", 1.0);
        }
    }
}
