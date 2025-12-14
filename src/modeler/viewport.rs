//! 3D Viewport for the modeler - renders models using the software rasterizer

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext};
use crate::rasterizer::{
    Framebuffer, render_mesh, Color as RasterColor, Vec3, Vec2 as RasterVec2,
    Vertex as RasterVertex, Face as RasterFace, WIDTH, HEIGHT,
    world_to_screen, Mat4,
    mat4_identity, mat4_mul, mat4_transform_point, mat4_from_position_rotation,
    mat4_rotation, mat4_translation,
};
use super::state::{ModelerState, ModelerSelection, SelectMode, TransformTool, Axis};
use super::model::{Model, PartTransform};

/// Compute world matrices for all bones in the skeleton hierarchy
fn compute_bone_world_transforms(model: &Model) -> Vec<Mat4> {
    let mut matrices = vec![mat4_identity(); model.bones.len()];

    for (i, bone) in model.bones.iter().enumerate() {
        let local = mat4_from_position_rotation(bone.local_position, bone.local_rotation);

        let world = if let Some(parent_idx) = bone.parent {
            if parent_idx < i {
                mat4_mul(&matrices[parent_idx], &local)
            } else {
                local
            }
        } else {
            local
        };

        matrices[i] = world;
    }

    matrices
}

/// Handle bone transform interactions (left-drag to move/rotate based on tool)
fn handle_bone_transforms(
    ctx: &UiContext,
    state: &mut ModelerState,
    inside_viewport: bool,
    mouse_pos: (f32, f32),
) {
    // Only handle if we have bones selected
    let selected_bones = match &state.selection {
        ModelerSelection::Bones(bones) if !bones.is_empty() => bones.clone(),
        _ => {
            // No bones selected, ensure transform is not active
            state.transform_active = false;
            return;
        }
    };

    // Start transform on left mouse down (when Move or Rotate tool is active)
    if inside_viewport && !state.transform_active && ctx.mouse.left_down && !ctx.mouse.right_down {
        let can_transform = matches!(state.tool, TransformTool::Move | TransformTool::Rotate);

        if can_transform {
            // Save undo state
            state.save_undo();

            // Store starting values
            state.transform_active = true;
            state.transform_start_mouse = mouse_pos;
            state.axis_lock = None;

            // Store original bone positions and rotations
            state.transform_start_positions = selected_bones
                .iter()
                .map(|&idx| state.model.bones.get(idx).map(|b| b.local_position).unwrap_or(Vec3::ZERO))
                .collect();
            state.transform_start_rotations = selected_bones
                .iter()
                .map(|&idx| state.model.bones.get(idx).map(|b| b.local_rotation).unwrap_or(Vec3::ZERO))
                .collect();

            let tool_name = match state.tool {
                TransformTool::Move => "Move",
                TransformTool::Rotate => "Rotate",
                _ => "Transform",
            };
            state.set_status(&format!("{} bone - drag to transform, X/Y/Z to constrain", tool_name), 3.0);
        }
    }

    // Handle active transform (while dragging)
    if state.transform_active {
        // Check for axis lock
        if is_key_pressed(KeyCode::X) {
            state.axis_lock = Some(Axis::X);
            state.set_status("Constrained to X axis", 2.0);
        }
        if is_key_pressed(KeyCode::Y) {
            state.axis_lock = Some(Axis::Y);
            state.set_status("Constrained to Y axis", 2.0);
        }
        if is_key_pressed(KeyCode::Z) {
            state.axis_lock = Some(Axis::Z);
            state.set_status("Constrained to Z axis", 2.0);
        }

        // Calculate delta from start position
        let dx = mouse_pos.0 - state.transform_start_mouse.0;
        let dy = mouse_pos.1 - state.transform_start_mouse.1;

        // Apply transform based on tool
        match state.tool {
            TransformTool::Move => {
                let move_scale = 0.5; // Pixels to world units

                for (i, &bone_idx) in selected_bones.iter().enumerate() {
                    if let Some(bone) = state.model.bones.get_mut(bone_idx) {
                        let start_pos = state.transform_start_positions.get(i).copied().unwrap_or(Vec3::ZERO);

                        // Calculate movement in camera space then convert to world
                        let move_x = dx * move_scale;
                        let move_y = -dy * move_scale; // Invert Y for screen coords

                        // Apply axis constraint
                        let delta = match state.axis_lock {
                            Some(Axis::X) => Vec3::new(move_x, 0.0, 0.0),
                            Some(Axis::Y) => Vec3::new(0.0, move_y, 0.0),
                            Some(Axis::Z) => Vec3::new(0.0, 0.0, move_x),
                            None => Vec3::new(move_x, move_y, 0.0),
                        };

                        bone.local_position = start_pos + delta;
                    }
                }
            }
            TransformTool::Rotate => {
                let rotate_scale = 0.5; // Pixels to degrees

                for (i, &bone_idx) in selected_bones.iter().enumerate() {
                    if let Some(bone) = state.model.bones.get_mut(bone_idx) {
                        let start_rot = state.transform_start_rotations.get(i).copied().unwrap_or(Vec3::ZERO);

                        // Calculate rotation based on mouse movement
                        let rot_amount = dx * rotate_scale;

                        // Apply axis constraint (default to Z for bone rotation)
                        let delta = match state.axis_lock {
                            Some(Axis::X) => Vec3::new(rot_amount, 0.0, 0.0),
                            Some(Axis::Y) => Vec3::new(0.0, rot_amount, 0.0),
                            Some(Axis::Z) | None => Vec3::new(0.0, 0.0, rot_amount),
                        };

                        bone.local_rotation = start_rot + delta;
                    }
                }
            }
            _ => {}
        }

        // Finish transform on mouse release
        if !ctx.mouse.left_down {
            state.transform_active = false;
            state.dirty = true;
            state.set_status("Transform applied", 1.0);
        }

        // Cancel transform on Escape
        if is_key_pressed(KeyCode::Escape) {
            // Restore original values
            for (i, &bone_idx) in selected_bones.iter().enumerate() {
                if let Some(bone) = state.model.bones.get_mut(bone_idx) {
                    if let Some(&pos) = state.transform_start_positions.get(i) {
                        bone.local_position = pos;
                    }
                    if let Some(&rot) = state.transform_start_rotations.get(i) {
                        bone.local_rotation = rot;
                    }
                }
            }
            state.transform_active = false;
            state.undo(); // Pop the undo we saved
            state.set_status("Transform cancelled", 1.0);
        }
    }
}

/// Compute world matrices for all parts given animation pose
fn compute_world_matrices(model: &Model, pose: &[PartTransform]) -> Vec<Mat4> {
    let mut matrices = Vec::with_capacity(model.parts.len());

    for (i, part) in model.parts.iter().enumerate() {
        let transform = pose.get(i).copied().unwrap_or_default();

        // Build local matrix: translate by position offset, then rotate
        let rot_mat = mat4_rotation(transform.rotation);
        let trans_mat = mat4_translation(transform.position + part.pivot);

        let local = mat4_mul(&trans_mat, &rot_mat);

        // Multiply by parent's world matrix
        let world = if let Some(parent_idx) = part.parent {
            if parent_idx < matrices.len() {
                mat4_mul(&matrices[parent_idx], &local)
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
    let (target_w, target_h) = if state.raster_settings.stretch_to_fill {
        // Keep horizontal resolution fixed, scale vertical to match viewport aspect ratio
        let base_w = if state.raster_settings.low_resolution { WIDTH } else { crate::rasterizer::WIDTH_HI };
        let viewport_aspect = rect.h / rect.w;
        let scaled_h = (base_w as f32 * viewport_aspect) as usize;
        (base_w, scaled_h.max(1))
    } else if state.raster_settings.low_resolution {
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
    let (draw_w, draw_h, draw_x, draw_y) = if state.raster_settings.stretch_to_fill {
        // Framebuffer matches viewport, no scaling needed
        (rect.w, rect.h, rect.x, rect.y)
    } else {
        // Maintain aspect ratio (4:3 for PS1)
        let fb_aspect = fb_width as f32 / fb_height as f32;
        let rect_aspect = rect.w / rect.h;
        if fb_aspect > rect_aspect {
            let w = rect.w;
            let h = rect.w / fb_aspect;
            (w, h, rect.x, rect.y + (rect.h - h) * 0.5)
        } else {
            let h = rect.h;
            let w = rect.h * fb_aspect;
            (w, h, rect.x + (rect.w - w) * 0.5, rect.y)
        }
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

    // Orbit camera controls
    let shift_held = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);

    // Right mouse drag: rotate around target (or pan if Shift held)
    if ctx.mouse.right_down && (inside_viewport || state.viewport_mouse_captured) {
        if state.viewport_mouse_captured {
            let dx = mouse_pos.0 - state.viewport_last_mouse.0;
            let dy = mouse_pos.1 - state.viewport_last_mouse.1;

            if shift_held {
                // Shift+Right drag: pan the orbit target
                let pan_speed = state.orbit_distance * 0.002; // Scale with distance
                state.orbit_target = state.orbit_target - state.camera.basis_x * dx * pan_speed;
                state.orbit_target = state.orbit_target + state.camera.basis_y * dy * pan_speed;
            } else {
                // Right drag: rotate around target
                state.orbit_azimuth += dx * 0.005;
                state.orbit_elevation = (state.orbit_elevation + dy * 0.005).clamp(-1.4, 1.4);
            }
            state.sync_camera_from_orbit();
        }
        state.viewport_mouse_captured = true;
    } else if !ctx.mouse.right_down {
        state.viewport_mouse_captured = false;
    }

    // Mouse wheel: zoom in/out (change orbit distance)
    if inside_viewport {
        let scroll = mouse_wheel().1;
        if scroll != 0.0 {
            let zoom_factor = if scroll > 0.0 { 0.9 } else { 1.1 };
            state.orbit_distance = (state.orbit_distance * zoom_factor).clamp(50.0, 2000.0);
            state.sync_camera_from_orbit();
        }
    }

    // Update mouse position for next frame
    state.viewport_last_mouse = mouse_pos;

    // Handle bone transforms (G=move, R=rotate)
    handle_bone_transforms(ctx, state, inside_viewport, mouse_pos);

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
            let world_pos = mat4_transform_point(world_mat, vert.position);

            // Calculate normal (simplified - just use up vector for now)
            let normal = Vec3::new(0.0, 1.0, 0.0);

            all_vertices.push(RasterVertex {
                pos: world_pos,
                uv: RasterVec2::new(vert.uv.x, vert.uv.y),
                normal,
                color: RasterColor::NEUTRAL,
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

    // Draw bones (skeleton visualization)
    if !state.model.bones.is_empty() {
        let bone_transforms = compute_bone_world_transforms(&state.model);
        let selected_bones = match &state.selection {
            ModelerSelection::Bones(bones) => bones.as_slice(),
            _ => &[],
        };
        draw_bones(fb, &state.model, &state.camera, &bone_transforms, selected_bones);
    }

    // Draw part/vertex/edge/face overlays based on selection mode
    draw_selection_overlays(ctx, fb, state, &world_matrices, screen_to_fb);

    // Handle click selection (but not if we're transforming or if a transform tool is active with selection)
    let has_bone_selection = matches!(&state.selection, ModelerSelection::Bones(b) if !b.is_empty());
    let is_transform_tool = matches!(state.tool, TransformTool::Move | TransformTool::Rotate);
    let should_select = !state.transform_active && !(has_bone_selection && is_transform_tool && state.select_mode == SelectMode::Bone);

    if inside_viewport && ctx.mouse.left_pressed && !ctx.mouse.right_down && should_select {
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

/// Draw the skeleton bones
fn draw_bones(
    fb: &mut Framebuffer,
    model: &Model,
    camera: &crate::rasterizer::Camera,
    bone_transforms: &[[[f32; 4]; 4]],
    selected_bones: &[usize],
) {
    let bone_color = RasterColor::new(220, 200, 50); // Yellow
    let selected_color = RasterColor::new(50, 255, 100); // Bright green
    let joint_color = RasterColor::new(255, 150, 50); // Orange

    for (bone_idx, bone) in model.bones.iter().enumerate() {
        let world_mat = &bone_transforms[bone_idx];

        // Joint position (origin of bone in world space)
        let joint_pos = Vec3::new(world_mat[0][3], world_mat[1][3], world_mat[2][3]);

        // Bone tip position (extends along local Y axis by bone length)
        let tip_local = Vec3::new(0.0, bone.length, 0.0);
        let tip_pos = mat4_transform_point(world_mat, tip_local);

        // Choose color based on selection
        let color = if selected_bones.contains(&bone_idx) {
            selected_color
        } else {
            bone_color
        };

        // Draw bone line from joint to tip
        draw_3d_line(fb, joint_pos, tip_pos, camera, color);

        // Draw joint marker (small cross)
        if let Some((sx, sy)) = world_to_screen(
            joint_pos,
            camera.position,
            camera.basis_x,
            camera.basis_y,
            camera.basis_z,
            fb.width,
            fb.height,
        ) {
            let marker_color = if selected_bones.contains(&bone_idx) {
                selected_color
            } else {
                joint_color
            };
            let size = if selected_bones.contains(&bone_idx) { 5 } else { 3 };
            let sx = sx as i32;
            let sy = sy as i32;
            fb.draw_line(sx - size, sy, sx + size, sy, marker_color);
            fb.draw_line(sx, sy - size, sx, sy + size, marker_color);
        }
    }
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
                let world_pos = mat4_transform_point(world_mat, vert.position);

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
                        let p0 = mat4_transform_point(world_mat, part.vertices[v0].position);
                        let p1 = mat4_transform_point(world_mat, part.vertices[v1].position);

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

                        let p0 = mat4_transform_point(world_mat, part.vertices[v0].position);
                        let p1 = mat4_transform_point(world_mat, part.vertices[v1].position);

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
        SelectMode::Bone => {
            // Find closest bone (check joint positions)
            let bone_transforms = compute_bone_world_transforms(&state.model);
            let mut closest: Option<(usize, f32)> = None;

            for (bone_idx, _bone) in state.model.bones.iter().enumerate() {
                let world_mat = &bone_transforms[bone_idx];
                let joint_pos = Vec3::new(world_mat[0][3], world_mat[1][3], world_mat[2][3]);

                if let Some((sx, sy)) = world_to_screen(
                    joint_pos,
                    state.camera.position,
                    state.camera.basis_x,
                    state.camera.basis_y,
                    state.camera.basis_z,
                    fb_width,
                    fb_height,
                ) {
                    let dist = ((fb_x - sx).powi(2) + (fb_y - sy).powi(2)).sqrt();
                    if dist < 15.0 {
                        if closest.map_or(true, |(_, best_dist)| dist < best_dist) {
                            closest = Some((bone_idx, dist));
                        }
                    }
                }
            }

            if let Some((bone_idx, _)) = closest {
                state.selection = ModelerSelection::Bones(vec![bone_idx]);
                state.set_status(&format!("Selected bone: {}", state.model.bones[bone_idx].name), 1.5);
            } else {
                state.selection = ModelerSelection::None;
            }
        }

        SelectMode::Part => {
            // Find closest part (check all vertices, pick part with closest vertex)
            let mut closest: Option<(usize, f32)> = None;

            for (part_idx, part) in state.model.parts.iter().enumerate() {
                if !part.visible {
                    continue;
                }

                let world_mat = &world_matrices[part_idx];

                for vert in &part.vertices {
                    let world_pos = mat4_transform_point(world_mat, vert.position);

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
                    let world_pos = mat4_transform_point(world_mat, vert.position);

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
