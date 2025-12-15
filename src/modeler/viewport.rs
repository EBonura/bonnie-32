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
use super::state::{ModelerState, ModelerSelection, SelectMode, TransformTool, Axis, GizmoHandle};
use super::model::{Model, PartTransform};
use super::spine::SpineModel;

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

    // Mouse wheel: adjust radius (if spine joint selected) OR zoom
    if inside_viewport {
        let scroll = mouse_wheel().1;
        if scroll != 0.0 {
            // Check if we have a spine joint selected
            let has_spine_selection = matches!(&state.selection, ModelerSelection::SpineJoints(j) if !j.is_empty());

            if has_spine_selection {
                // Adjust radius of selected joint(s)
                if let ModelerSelection::SpineJoints(joints) = &state.selection {
                    let joints = joints.clone();
                    // Smaller delta for finer control (was 2.0, now 0.5)
                    let delta = if scroll > 0.0 { 0.5 } else { -0.5 };
                    let mut new_radius = 0.0;

                    if let Some(spine_model) = &mut state.spine_model {
                        for (seg_idx, joint_idx) in &joints {
                            if let Some(segment) = spine_model.segments.get_mut(*seg_idx) {
                                if let Some(joint) = segment.joints.get_mut(*joint_idx) {
                                    joint.radius = (joint.radius + delta).max(1.0);
                                    new_radius = joint.radius;
                                }
                            }
                        }
                    }

                    // Show feedback (outside of spine_model borrow)
                    state.set_status(&format!("Radius: {:.1}", new_radius), 0.5);
                }
            } else {
                // Normal zoom behavior
                let zoom_factor = if scroll > 0.0 { 0.9 } else { 1.1 };
                state.orbit_distance = (state.orbit_distance * zoom_factor).clamp(50.0, 2000.0);
                state.sync_camera_from_orbit();
            }
        }
    }

    // Update mouse position for next frame
    state.viewport_last_mouse = mouse_pos;

    // Toggle snap with 'S' key (when not dragging)
    if inside_viewport && is_key_pressed(KeyCode::S) && !state.spine_drag_active && !state.transform_active {
        state.snap_settings.enabled = !state.snap_settings.enabled;
        let status = if state.snap_settings.enabled {
            format!("Snap ON (grid: {})", state.snap_settings.grid_size)
        } else {
            "Snap OFF".to_string()
        };
        state.set_status(&status, 1.5);
    }

    // Extrude (E key) - add new joint at end of spine when end joint is selected
    if inside_viewport && is_key_pressed(KeyCode::E) && !state.spine_drag_active && !state.transform_active {
        handle_spine_extrude(state);
    }

    // Delete (X key) - delete selected joints or bones (not when Shift held - that's delete segment)
    if inside_viewport && is_key_pressed(KeyCode::X) && !shift_held && !state.spine_drag_active && !state.transform_active {
        handle_spine_delete(state);
    }

    // Subdivide (W key) - insert joint at midpoint of selected bone
    if inside_viewport && is_key_pressed(KeyCode::W) && !state.spine_drag_active && !state.transform_active {
        handle_spine_subdivide(state);
    }

    // Duplicate segment (D key) - copy current segment
    if inside_viewport && is_key_pressed(KeyCode::D) && !state.spine_drag_active && !state.transform_active {
        handle_spine_duplicate_segment(state);
    }

    // New segment (N key) - create a new segment
    if inside_viewport && is_key_pressed(KeyCode::N) && !state.spine_drag_active && !state.transform_active {
        handle_spine_new_segment(state);
    }

    // Delete segment (Shift+X key) - delete entire segment
    if inside_viewport && is_key_pressed(KeyCode::X) && shift_held && !state.spine_drag_active && !state.transform_active {
        handle_spine_delete_segment(state);
    }

    // Mirror segment (M key) - mirror on X axis
    if inside_viewport && is_key_pressed(KeyCode::M) && !state.spine_drag_active && !state.transform_active {
        handle_spine_mirror_segment(state);
    }

    // Handle bone transforms (G=move, R=rotate)
    handle_bone_transforms(ctx, state, inside_viewport, mouse_pos);

    // Clear framebuffer
    fb.clear(RasterColor::new(40, 40, 50));

    // Draw grid on floor
    draw_grid(fb, &state.camera, 0.0, 50.0, 10);

    // Build render data
    let mut all_vertices: Vec<RasterVertex> = Vec::new();
    let mut all_faces: Vec<RasterFace> = Vec::new();

    // Track whether we're using spine or old model (for selection overlays)
    let using_spine = state.spine_model.is_some();

    // Get current pose for animation (needed for old model system)
    let pose = state.get_current_pose();
    let world_matrices = compute_world_matrices(&state.model, &pose);

    // Render spine model if present (new system)
    if let Some(spine_model) = &state.spine_model {
        let (spine_verts, spine_faces) = spine_model.generate_mesh();

        for vert in spine_verts {
            all_vertices.push(vert);
        }

        for face in spine_faces {
            all_faces.push(face);
        }
    } else {
        // Fallback to old model system if no spine model
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
    }

    // Render using software rasterizer
    let empty_textures: Vec<crate::rasterizer::Texture> = Vec::new();
    render_mesh(fb, &all_vertices, &all_faces, &empty_textures, &state.camera, &state.raster_settings);

    // Draw spine joint markers (on top of mesh)
    let mut gizmo_info: Option<GizmoScreenInfo> = None;
    if let Some(spine_model) = &state.spine_model {
        let selected_joints = state.selection.spine_joints().unwrap_or(&[]);
        let selected_bones = state.selection.spine_bones().unwrap_or(&[]);
        draw_spine_joints(fb, spine_model, &state.camera, selected_joints, selected_bones);

        // Draw gizmo at first selected joint
        if let Some(&(seg_idx, joint_idx)) = selected_joints.first() {
            if let Some(segment) = spine_model.segments.get(seg_idx) {
                if let Some(joint) = segment.joints.get(joint_idx) {
                    gizmo_info = Some(draw_gizmo(
                        fb,
                        joint.position,
                        &state.camera,
                        state.gizmo_hover_handle,
                        state.spine_drag_handle,
                    ));
                }
            }
        }
        // Draw gizmo at midpoint of first selected bone
        else if let Some(&(seg_idx, bone_idx)) = selected_bones.first() {
            if let Some(segment) = spine_model.segments.get(seg_idx) {
                if let (Some(joint_a), Some(joint_b)) = (segment.joints.get(bone_idx), segment.joints.get(bone_idx + 1)) {
                    let midpoint = (joint_a.position + joint_b.position) * 0.5;
                    gizmo_info = Some(draw_gizmo(
                        fb,
                        midpoint,
                        &state.camera,
                        state.gizmo_hover_handle,
                        state.spine_drag_handle,
                    ));
                }
            }
        }
    }

    // Draw bones (skeleton visualization) - only for old model system
    if !using_spine && !state.model.bones.is_empty() {
        let bone_transforms = compute_bone_world_transforms(&state.model);
        let selected_bones = match &state.selection {
            ModelerSelection::Bones(bones) => bones.as_slice(),
            _ => &[],
        };
        draw_bones(fb, &state.model, &state.camera, &bone_transforms, selected_bones);
    }

    // Draw part/vertex/edge/face overlays based on selection mode (old model system)
    if !using_spine {
        draw_selection_overlays(ctx, fb, state, &world_matrices, screen_to_fb);
    }

    // Update gizmo hover state
    if let (Some(gizmo), Some((fb_x, fb_y))) = (&gizmo_info, screen_to_fb(mouse_pos.0, mouse_pos.1)) {
        if !state.spine_drag_active {
            state.gizmo_hover_handle = gizmo.hit_test(fb_x, fb_y, 8.0);
        }
    } else {
        state.gizmo_hover_handle = None;
    }

    // Handle click selection FIRST (before drag) - only if not already dragging
    // Skip selection if clicking on gizmo
    let clicking_gizmo = state.gizmo_hover_handle.is_some();
    if inside_viewport && ctx.mouse.left_pressed && !ctx.mouse.right_down && !state.transform_active && !state.spine_drag_active && !clicking_gizmo {
        if using_spine {
            // Handle spine joint selection
            handle_spine_selection_click(state, screen_to_fb, fb.width, fb.height);
        } else {
            // Handle old model selection
            let has_bone_selection = matches!(&state.selection, ModelerSelection::Bones(b) if !b.is_empty());
            let is_transform_tool = matches!(state.tool, TransformTool::Move | TransformTool::Rotate);
            let should_select = !(has_bone_selection && is_transform_tool && state.select_mode == SelectMode::Bone);

            if should_select {
                handle_selection_click(ctx, state, &world_matrices, screen_to_fb, fb.width, fb.height);
            }
        }
    }

    // Handle spine joint dragging AFTER selection (so newly selected joint can be dragged)
    if using_spine {
        handle_spine_joint_drag(ctx, state, inside_viewport, mouse_pos, screen_to_fb, fb.width, fb.height);
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

    // Draw snap indicator
    if state.snap_settings.enabled {
        draw_text(
            &format!("SNAP: {}", state.snap_settings.grid_size),
            rect.x + 5.0,
            rect.y + 15.0,
            12.0,
            Color::from_rgba(100, 255, 100, 255),
        );
    }
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

/// Draw spine joints as markers (for visual feedback during editing)
fn draw_spine_joints(
    fb: &mut Framebuffer,
    spine_model: &SpineModel,
    camera: &crate::rasterizer::Camera,
    selected_joints: &[(usize, usize)],
    selected_bones: &[(usize, usize)],
) {
    let joint_color = RasterColor::new(255, 200, 50);      // Yellow/orange
    let selected_color = RasterColor::new(50, 255, 100);   // Bright green
    let line_color = RasterColor::new(200, 150, 50);       // Darker for spine line
    let selected_bone_color = RasterColor::new(100, 255, 150); // Cyan-green for bones

    for (seg_idx, segment) in spine_model.segments.iter().enumerate() {
        let mut prev_pos: Option<Vec3> = None;

        for (joint_idx, joint) in segment.joints.iter().enumerate() {
            // Draw line connecting to previous joint
            if let Some(prev) = prev_pos {
                // Check if this bone (prev joint to current) is selected
                let bone_idx = joint_idx - 1;
                let is_bone_selected = selected_bones.contains(&(seg_idx, bone_idx));
                let bone_color = if is_bone_selected { selected_bone_color } else { line_color };

                draw_3d_line(fb, prev, joint.position, camera, bone_color);

                // Draw thicker line for selected bones (draw adjacent parallel lines)
                if is_bone_selected {
                    // Get direction perpendicular to the line in screen space
                    let screen_a = world_to_screen(prev, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height);
                    let screen_b = world_to_screen(joint.position, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height);
                    if let (Some((ax, ay)), Some((bx, by))) = (screen_a, screen_b) {
                        // Draw offset lines to make it thicker
                        let dx = bx - ax;
                        let dy = by - ay;
                        let len = (dx * dx + dy * dy).sqrt();
                        if len > 0.001 {
                            let nx = -dy / len;
                            let ny = dx / len;
                            // Draw 2 parallel lines offset by 1 pixel each side
                            for offset in [-1.0_f32, 1.0] {
                                let ox = nx * offset;
                                let oy = ny * offset;
                                fb.draw_line(
                                    (ax + ox) as i32, (ay + oy) as i32,
                                    (bx + ox) as i32, (by + oy) as i32,
                                    bone_color
                                );
                            }
                        }
                    }
                }
            }

            let is_selected = selected_joints.contains(&(seg_idx, joint_idx));

            // Draw joint marker
            if let Some((sx, sy)) = world_to_screen(
                joint.position,
                camera.position,
                camera.basis_x,
                camera.basis_y,
                camera.basis_z,
                fb.width,
                fb.height,
            ) {
                let sx = sx as i32;
                let sy = sy as i32;

                let color = if is_selected { selected_color } else { joint_color };
                let radius = if is_selected { 4 } else { 2 };

                // Draw small circle at each joint (like vertex markers)
                fb.draw_circle(sx, sy, radius, color);
            }

            prev_pos = Some(joint.position);
        }
    }
}

/// Draw a 3D translation gizmo at the given position
/// Returns screen-space bounding info for hit testing
fn draw_gizmo(
    fb: &mut Framebuffer,
    position: Vec3,
    camera: &crate::rasterizer::Camera,
    hover_handle: Option<GizmoHandle>,
    drag_handle: Option<GizmoHandle>,
) -> GizmoScreenInfo {
    // Gizmo size scales with distance to camera for consistent screen size
    let to_camera = camera.position - position;
    let distance = to_camera.len();
    let gizmo_length = distance * 0.15; // 15% of distance to camera
    let plane_size = gizmo_length * 0.3; // Plane squares are 30% of axis length
    let plane_offset = gizmo_length * 0.25; // Offset from origin

    // Helper to check if an axis or plane is active
    let is_axis_active = |axis: Axis| -> bool {
        matches!(hover_handle, Some(GizmoHandle::Axis(a)) if a == axis)
            || matches!(drag_handle, Some(GizmoHandle::Axis(a)) if a == axis)
    };
    let is_plane_active = |a1: Axis, a2: Axis| -> bool {
        matches!(hover_handle, Some(GizmoHandle::Plane { axis1, axis2 }) if (axis1 == a1 && axis2 == a2) || (axis1 == a2 && axis2 == a1))
            || matches!(drag_handle, Some(GizmoHandle::Plane { axis1, axis2 }) if (axis1 == a1 && axis2 == a2) || (axis1 == a2 && axis2 == a1))
    };

    // Axis colors (brighter when hovered/dragged)
    let x_color = if is_axis_active(Axis::X) {
        RasterColor::new(255, 100, 100)
    } else {
        RasterColor::new(200, 60, 60)
    };
    let y_color = if is_axis_active(Axis::Y) {
        RasterColor::new(100, 255, 100)
    } else {
        RasterColor::new(60, 200, 60)
    };
    let z_color = if is_axis_active(Axis::Z) {
        RasterColor::new(100, 100, 255)
    } else {
        RasterColor::new(60, 60, 200)
    };

    // Plane colors (mix of the two axes, semi-transparent feel via lighter colors)
    let xy_color = if is_plane_active(Axis::X, Axis::Y) {
        RasterColor::new(255, 255, 100)  // Yellow (bright)
    } else {
        RasterColor::new(180, 180, 60)   // Yellow (dim)
    };
    let xz_color = if is_plane_active(Axis::X, Axis::Z) {
        RasterColor::new(255, 100, 255)  // Magenta (bright)
    } else {
        RasterColor::new(180, 60, 180)   // Magenta (dim)
    };
    let yz_color = if is_plane_active(Axis::Y, Axis::Z) {
        RasterColor::new(100, 255, 255)  // Cyan (bright)
    } else {
        RasterColor::new(60, 180, 180)   // Cyan (dim)
    };

    // Draw axis lines
    let x_end = position + Vec3::new(gizmo_length, 0.0, 0.0);
    let y_end = position + Vec3::new(0.0, gizmo_length, 0.0);
    let z_end = position + Vec3::new(0.0, 0.0, gizmo_length);

    draw_3d_line(fb, position, x_end, camera, x_color);
    draw_3d_line(fb, position, y_end, camera, y_color);
    draw_3d_line(fb, position, z_end, camera, z_color);

    // Draw arrowheads (small lines at the end)
    let arrow_size = gizmo_length * 0.15;

    // X arrow
    draw_3d_line(fb, x_end, x_end + Vec3::new(-arrow_size, arrow_size * 0.5, 0.0), camera, x_color);
    draw_3d_line(fb, x_end, x_end + Vec3::new(-arrow_size, -arrow_size * 0.5, 0.0), camera, x_color);

    // Y arrow
    draw_3d_line(fb, y_end, y_end + Vec3::new(arrow_size * 0.5, -arrow_size, 0.0), camera, y_color);
    draw_3d_line(fb, y_end, y_end + Vec3::new(-arrow_size * 0.5, -arrow_size, 0.0), camera, y_color);

    // Z arrow
    draw_3d_line(fb, z_end, z_end + Vec3::new(0.0, arrow_size * 0.5, -arrow_size), camera, z_color);
    draw_3d_line(fb, z_end, z_end + Vec3::new(0.0, -arrow_size * 0.5, -arrow_size), camera, z_color);

    // Draw plane squares (small squares offset from origin)
    // XY plane square (at Z=0)
    let xy_p1 = position + Vec3::new(plane_offset, plane_offset, 0.0);
    let xy_p2 = position + Vec3::new(plane_offset + plane_size, plane_offset, 0.0);
    let xy_p3 = position + Vec3::new(plane_offset + plane_size, plane_offset + plane_size, 0.0);
    let xy_p4 = position + Vec3::new(plane_offset, plane_offset + plane_size, 0.0);
    draw_3d_line(fb, xy_p1, xy_p2, camera, xy_color);
    draw_3d_line(fb, xy_p2, xy_p3, camera, xy_color);
    draw_3d_line(fb, xy_p3, xy_p4, camera, xy_color);
    draw_3d_line(fb, xy_p4, xy_p1, camera, xy_color);

    // XZ plane square (at Y=0)
    let xz_p1 = position + Vec3::new(plane_offset, 0.0, plane_offset);
    let xz_p2 = position + Vec3::new(plane_offset + plane_size, 0.0, plane_offset);
    let xz_p3 = position + Vec3::new(plane_offset + plane_size, 0.0, plane_offset + plane_size);
    let xz_p4 = position + Vec3::new(plane_offset, 0.0, plane_offset + plane_size);
    draw_3d_line(fb, xz_p1, xz_p2, camera, xz_color);
    draw_3d_line(fb, xz_p2, xz_p3, camera, xz_color);
    draw_3d_line(fb, xz_p3, xz_p4, camera, xz_color);
    draw_3d_line(fb, xz_p4, xz_p1, camera, xz_color);

    // YZ plane square (at X=0)
    let yz_p1 = position + Vec3::new(0.0, plane_offset, plane_offset);
    let yz_p2 = position + Vec3::new(0.0, plane_offset + plane_size, plane_offset);
    let yz_p3 = position + Vec3::new(0.0, plane_offset + plane_size, plane_offset + plane_size);
    let yz_p4 = position + Vec3::new(0.0, plane_offset, plane_offset + plane_size);
    draw_3d_line(fb, yz_p1, yz_p2, camera, yz_color);
    draw_3d_line(fb, yz_p2, yz_p3, camera, yz_color);
    draw_3d_line(fb, yz_p3, yz_p4, camera, yz_color);
    draw_3d_line(fb, yz_p4, yz_p1, camera, yz_color);

    // Calculate screen positions for hit testing
    let origin_screen = world_to_screen(position, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height);
    let x_screen = world_to_screen(x_end, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height);
    let y_screen = world_to_screen(y_end, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height);
    let z_screen = world_to_screen(z_end, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height);

    // Plane centers for hit testing
    let xy_center = position + Vec3::new(plane_offset + plane_size * 0.5, plane_offset + plane_size * 0.5, 0.0);
    let xz_center = position + Vec3::new(plane_offset + plane_size * 0.5, 0.0, plane_offset + plane_size * 0.5);
    let yz_center = position + Vec3::new(0.0, plane_offset + plane_size * 0.5, plane_offset + plane_size * 0.5);

    let xy_screen = world_to_screen(xy_center, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height);
    let xz_screen = world_to_screen(xz_center, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height);
    let yz_screen = world_to_screen(yz_center, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height);

    GizmoScreenInfo {
        origin: origin_screen,
        x_end: x_screen,
        y_end: y_screen,
        z_end: z_screen,
        xy_center: xy_screen,
        xz_center: xz_screen,
        yz_center: yz_screen,
        plane_screen_radius: (plane_size / gizmo_length) * 30.0, // Approximate screen radius for hit testing
    }
}

/// Screen-space info for gizmo hit testing
struct GizmoScreenInfo {
    origin: Option<(f32, f32)>,
    x_end: Option<(f32, f32)>,
    y_end: Option<(f32, f32)>,
    z_end: Option<(f32, f32)>,
    xy_center: Option<(f32, f32)>,
    xz_center: Option<(f32, f32)>,
    yz_center: Option<(f32, f32)>,
    plane_screen_radius: f32,
}

impl GizmoScreenInfo {
    /// Check if a screen point is near one of the gizmo handles
    /// Returns the handle if within threshold distance
    /// Planes are checked first (they're smaller targets), then axes
    fn hit_test(&self, screen_x: f32, screen_y: f32, threshold: f32) -> Option<GizmoHandle> {
        let Some(origin) = self.origin else { return None };

        let mut best_handle: Option<GizmoHandle> = None;
        let mut best_dist = threshold;

        // Check plane handles first (smaller targets, higher priority when close)
        let plane_threshold = self.plane_screen_radius.max(8.0);

        if let Some(xy) = self.xy_center {
            let dist = ((screen_x - xy.0).powi(2) + (screen_y - xy.1).powi(2)).sqrt();
            if dist < plane_threshold && dist < best_dist {
                best_dist = dist;
                best_handle = Some(GizmoHandle::XY);
            }
        }

        if let Some(xz) = self.xz_center {
            let dist = ((screen_x - xz.0).powi(2) + (screen_y - xz.1).powi(2)).sqrt();
            if dist < plane_threshold && dist < best_dist {
                best_dist = dist;
                best_handle = Some(GizmoHandle::XZ);
            }
        }

        if let Some(yz) = self.yz_center {
            let dist = ((screen_x - yz.0).powi(2) + (screen_y - yz.1).powi(2)).sqrt();
            if dist < plane_threshold && dist < best_dist {
                best_dist = dist;
                best_handle = Some(GizmoHandle::YZ);
            }
        }

        // If no plane was hit, check axes
        if best_handle.is_none() {
            if let Some(x_end) = self.x_end {
                let dist = point_to_segment_dist(screen_x, screen_y, origin.0, origin.1, x_end.0, x_end.1);
                if dist < best_dist {
                    best_dist = dist;
                    best_handle = Some(GizmoHandle::Axis(Axis::X));
                }
            }

            if let Some(y_end) = self.y_end {
                let dist = point_to_segment_dist(screen_x, screen_y, origin.0, origin.1, y_end.0, y_end.1);
                if dist < best_dist {
                    best_dist = dist;
                    best_handle = Some(GizmoHandle::Axis(Axis::Y));
                }
            }

            if let Some(z_end) = self.z_end {
                let dist = point_to_segment_dist(screen_x, screen_y, origin.0, origin.1, z_end.0, z_end.1);
                if dist < best_dist {
                    best_handle = Some(GizmoHandle::Axis(Axis::Z));
                }
            }
        }

        best_handle
    }
}

/// Calculate distance from point (px, py) to line segment (x1,y1)-(x2,y2)
fn point_to_segment_dist(px: f32, py: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let len_sq = dx * dx + dy * dy;

    if len_sq < 0.0001 {
        // Segment is a point
        return ((px - x1).powi(2) + (py - y1).powi(2)).sqrt();
    }

    // Project point onto line, clamped to segment
    let t = ((px - x1) * dx + (py - y1) * dy) / len_sq;
    let t = t.clamp(0.0, 1.0);

    let closest_x = x1 + t * dx;
    let closest_y = y1 + t * dy;

    ((px - closest_x).powi(2) + (py - closest_y).powi(2)).sqrt()
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
        // Recalculate z for the clipped points
        let clipped_rel0 = clipped_p0 - camera.position;
        let clipped_rel1 = clipped_p1 - camera.position;
        let clipped_z0 = clipped_rel0.dot(camera.basis_z);
        let clipped_z1 = clipped_rel1.dot(camera.basis_z);

        // Use draw_line_3d which respects z-buffer
        fb.draw_line_3d(x0f as i32, y0f as i32, clipped_z0, x1f as i32, y1f as i32, clipped_z1, color);
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

/// Handle spine joint/bone dragging (move selected joints with mouse)
fn handle_spine_joint_drag<F>(
    ctx: &UiContext,
    state: &mut ModelerState,
    inside_viewport: bool,
    mouse_pos: (f32, f32),
    _screen_to_fb: F,
    _fb_width: usize,
    _fb_height: usize,
) where F: Fn(f32, f32) -> Option<(f32, f32)>
{
    // Check if we have spine joints or bones selected
    let has_joint_selection = matches!(&state.selection, ModelerSelection::SpineJoints(j) if !j.is_empty());
    let has_bone_selection = matches!(&state.selection, ModelerSelection::SpineBones(b) if !b.is_empty());

    if !has_joint_selection && !has_bone_selection {
        state.spine_drag_active = false;
        state.spine_drag_start_positions.clear();
        return;
    }

    // Start drag on left mouse press (capture initial state)
    if inside_viewport && ctx.mouse.left_pressed && !ctx.mouse.right_down {
        // Store starting positions for joints
        if let ModelerSelection::SpineJoints(joints) = &state.selection {
            let mut start_positions = Vec::new();
            if let Some(spine_model) = &state.spine_model {
                for (seg_idx, joint_idx) in joints {
                    if let Some(segment) = spine_model.segments.get(*seg_idx) {
                        if let Some(joint) = segment.joints.get(*joint_idx) {
                            start_positions.push(joint.position);
                        }
                    }
                }
            }

            if !start_positions.is_empty() {
                state.spine_drag_start_mouse = mouse_pos;
                state.spine_drag_start_positions = start_positions;
                state.spine_drag_handle = state.gizmo_hover_handle;
            }
        }
        // Store starting positions for bones (both joints of each bone)
        else if let ModelerSelection::SpineBones(bones) = &state.selection {
            let mut start_positions = Vec::new();
            if let Some(spine_model) = &state.spine_model {
                for (seg_idx, bone_idx) in bones {
                    if let Some(segment) = spine_model.segments.get(*seg_idx) {
                        // Store both joints of the bone
                        if let Some(joint_a) = segment.joints.get(*bone_idx) {
                            start_positions.push(joint_a.position);
                        }
                        if let Some(joint_b) = segment.joints.get(*bone_idx + 1) {
                            start_positions.push(joint_b.position);
                        }
                    }
                }
            }

            if !start_positions.is_empty() {
                state.spine_drag_start_mouse = mouse_pos;
                state.spine_drag_start_positions = start_positions;
                state.spine_drag_handle = state.gizmo_hover_handle;
            }
        }
    }

    // Check if we should start actual dragging (mouse moved enough from initial press)
    if inside_viewport && ctx.mouse.left_down && !ctx.mouse.right_down && !state.spine_drag_active {
        if !state.spine_drag_start_positions.is_empty() {
            let dx = mouse_pos.0 - state.spine_drag_start_mouse.0;
            let dy = mouse_pos.1 - state.spine_drag_start_mouse.1;
            let moved = (dx * dx + dy * dy).sqrt();

            if moved > 3.0 {
                state.spine_drag_active = true;
            }
        }
    }

    // Update positions during drag
    if state.spine_drag_active && ctx.mouse.left_down {
        let dx = mouse_pos.0 - state.spine_drag_start_mouse.0;
        let dy = mouse_pos.1 - state.spine_drag_start_mouse.1;
        let scale = state.orbit_distance * 0.002;

        // Calculate world delta based on drag handle type
        let world_delta = match state.spine_drag_handle {
            Some(GizmoHandle::Axis(axis)) => {
                // Single axis movement
                let axis_vec = axis.to_vec3();
                let x_proj = state.camera.basis_x.dot(axis_vec);
                let y_proj = state.camera.basis_y.dot(axis_vec);
                let movement = (dx * x_proj + dy * y_proj) * scale;
                axis_vec * movement
            }
            Some(GizmoHandle::Plane { axis1, axis2 }) => {
                // Plane movement - project mouse onto both axes
                let axis1_vec = axis1.to_vec3();
                let axis2_vec = axis2.to_vec3();

                // Project camera basis onto each axis
                let x_proj1 = state.camera.basis_x.dot(axis1_vec);
                let y_proj1 = state.camera.basis_y.dot(axis1_vec);
                let x_proj2 = state.camera.basis_x.dot(axis2_vec);
                let y_proj2 = state.camera.basis_y.dot(axis2_vec);

                let movement1 = (dx * x_proj1 + dy * y_proj1) * scale;
                let movement2 = (dx * x_proj2 + dy * y_proj2) * scale;

                axis1_vec * movement1 + axis2_vec * movement2
            }
            None => {
                // Free drag on camera plane
                let world_dx = state.camera.basis_x * dx * scale;
                let world_dy = state.camera.basis_y * dy * scale;
                world_dx + world_dy
            }
        };

        // Apply snap/quantization if enabled
        let snapped_delta = state.snap_settings.snap_vec3(world_delta);

        // Update joint positions
        if let ModelerSelection::SpineJoints(joints) = &state.selection {
            let joints = joints.clone();
            if let Some(spine_model) = &mut state.spine_model {
                for (i, (seg_idx, joint_idx)) in joints.iter().enumerate() {
                    if let Some(segment) = spine_model.segments.get_mut(*seg_idx) {
                        if let Some(joint) = segment.joints.get_mut(*joint_idx) {
                            if let Some(start_pos) = state.spine_drag_start_positions.get(i) {
                                // With snap: snap the final position, not the delta
                                if state.snap_settings.enabled {
                                    joint.position = state.snap_settings.snap_vec3(*start_pos + world_delta);
                                } else {
                                    joint.position = *start_pos + snapped_delta;
                                }
                            }
                        }
                    }
                }
            }
        }
        // Update bone positions (both joints of each bone)
        else if let ModelerSelection::SpineBones(bones) = &state.selection {
            let bones = bones.clone();
            if let Some(spine_model) = &mut state.spine_model {
                // Start positions are stored as pairs: [bone0_joint_a, bone0_joint_b, bone1_joint_a, bone1_joint_b, ...]
                for (bone_i, (seg_idx, bone_idx)) in bones.iter().enumerate() {
                    if let Some(segment) = spine_model.segments.get_mut(*seg_idx) {
                        let pos_idx_a = bone_i * 2;
                        let pos_idx_b = bone_i * 2 + 1;

                        // Update first joint of bone
                        if let Some(joint_a) = segment.joints.get_mut(*bone_idx) {
                            if let Some(start_pos) = state.spine_drag_start_positions.get(pos_idx_a) {
                                if state.snap_settings.enabled {
                                    joint_a.position = state.snap_settings.snap_vec3(*start_pos + world_delta);
                                } else {
                                    joint_a.position = *start_pos + snapped_delta;
                                }
                            }
                        }
                        // Update second joint of bone
                        if let Some(joint_b) = segment.joints.get_mut(*bone_idx + 1) {
                            if let Some(start_pos) = state.spine_drag_start_positions.get(pos_idx_b) {
                                if state.snap_settings.enabled {
                                    joint_b.position = state.snap_settings.snap_vec3(*start_pos + world_delta);
                                } else {
                                    joint_b.position = *start_pos + snapped_delta;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // End drag on mouse release
    if !ctx.mouse.left_down {
        if state.spine_drag_active {
            let msg = if matches!(&state.selection, ModelerSelection::SpineBones(_)) {
                "Bone moved"
            } else {
                "Joint moved"
            };
            state.set_status(msg, 0.5);
        }
        state.spine_drag_active = false;
        state.spine_drag_start_positions.clear();
        state.spine_drag_handle = None;
    }

    // Cancel drag on escape
    if state.spine_drag_active && is_key_pressed(KeyCode::Escape) {
        // Restore original positions for joints
        if let ModelerSelection::SpineJoints(joints) = &state.selection {
            let joints = joints.clone();
            if let Some(spine_model) = &mut state.spine_model {
                for (i, (seg_idx, joint_idx)) in joints.iter().enumerate() {
                    if let Some(segment) = spine_model.segments.get_mut(*seg_idx) {
                        if let Some(joint) = segment.joints.get_mut(*joint_idx) {
                            if let Some(start_pos) = state.spine_drag_start_positions.get(i) {
                                joint.position = *start_pos;
                            }
                        }
                    }
                }
            }
        }
        // Restore original positions for bones
        else if let ModelerSelection::SpineBones(bones) = &state.selection {
            let bones = bones.clone();
            if let Some(spine_model) = &mut state.spine_model {
                for (bone_i, (seg_idx, bone_idx)) in bones.iter().enumerate() {
                    if let Some(segment) = spine_model.segments.get_mut(*seg_idx) {
                        let pos_idx_a = bone_i * 2;
                        let pos_idx_b = bone_i * 2 + 1;

                        if let Some(joint_a) = segment.joints.get_mut(*bone_idx) {
                            if let Some(start_pos) = state.spine_drag_start_positions.get(pos_idx_a) {
                                joint_a.position = *start_pos;
                            }
                        }
                        if let Some(joint_b) = segment.joints.get_mut(*bone_idx + 1) {
                            if let Some(start_pos) = state.spine_drag_start_positions.get(pos_idx_b) {
                                joint_b.position = *start_pos;
                            }
                        }
                    }
                }
            }
        }
        state.spine_drag_active = false;
        state.spine_drag_handle = None;
        state.set_status("Move cancelled", 0.5);
    }
}

/// Handle spine joint selection on click
/// Supports multi-select with Shift key
fn handle_spine_selection_click<F>(
    state: &mut ModelerState,
    screen_to_fb: F,
    fb_width: usize,
    fb_height: usize,
) where F: Fn(f32, f32) -> Option<(f32, f32)>
{
    let mouse_pos = macroquad::prelude::mouse_position();
    let Some((fb_x, fb_y)) = screen_to_fb(mouse_pos.0, mouse_pos.1) else {
        return;
    };

    let Some(spine_model) = &state.spine_model else {
        return;
    };

    // Check if Shift is held for multi-select
    let shift_held = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);

    // Find closest joint to click position (joints have priority)
    let mut closest_joint: Option<((usize, usize), f32)> = None;
    let joint_threshold = 8.0;

    for (seg_idx, segment) in spine_model.segments.iter().enumerate() {
        for (joint_idx, joint) in segment.joints.iter().enumerate() {
            if let Some((sx, sy)) = world_to_screen(
                joint.position,
                state.camera.position,
                state.camera.basis_x,
                state.camera.basis_y,
                state.camera.basis_z,
                fb_width,
                fb_height,
            ) {
                let dist = ((fb_x - sx).powi(2) + (fb_y - sy).powi(2)).sqrt();
                if dist < joint_threshold {
                    if closest_joint.map_or(true, |(_, best_dist)| dist < best_dist) {
                        closest_joint = Some(((seg_idx, joint_idx), dist));
                    }
                }
            }
        }
    }

    // If a joint was clicked, select it (or add to selection with Shift)
    if let Some(((seg_idx, joint_idx), _)) = closest_joint {
        let joint = &spine_model.segments[seg_idx].joints[joint_idx];

        if shift_held {
            // Multi-select: add to or remove from existing joint selection
            match &state.selection {
                ModelerSelection::SpineJoints(joints) => {
                    let mut new_joints = joints.clone();
                    let item = (seg_idx, joint_idx);
                    if let Some(pos) = new_joints.iter().position(|&j| j == item) {
                        // Already selected - remove it (toggle)
                        new_joints.remove(pos);
                        if new_joints.is_empty() {
                            state.selection = ModelerSelection::None;
                            state.set_status("Selection cleared", 0.5);
                        } else {
                            state.selection = ModelerSelection::SpineJoints(new_joints);
                            state.set_status(&format!("Deselected joint {}", joint_idx), 0.5);
                        }
                    } else {
                        // Not selected - add it
                        new_joints.push(item);
                        state.selection = ModelerSelection::SpineJoints(new_joints.clone());
                        state.set_status(&format!("{} joints selected", new_joints.len()), 0.5);
                    }
                }
                _ => {
                    // Start new joint selection
                    state.selection = ModelerSelection::SpineJoints(vec![(seg_idx, joint_idx)]);
                    state.set_status(&format!("Joint {} (radius: {:.1})", joint_idx, joint.radius), 1.5);
                }
            }
        } else {
            // Normal click: replace selection
            state.selection = ModelerSelection::SpineJoints(vec![(seg_idx, joint_idx)]);
            state.set_status(&format!("Joint {} (radius: {:.1})", joint_idx, joint.radius), 1.5);
        }
        return;
    }

    // No joint clicked, check for bone/segment clicks (lines between joints)
    let mut closest_bone: Option<((usize, usize), f32)> = None;
    let bone_threshold = 12.0;

    for (seg_idx, segment) in spine_model.segments.iter().enumerate() {
        for bone_idx in 0..segment.joints.len().saturating_sub(1) {
            let joint_a = &segment.joints[bone_idx];
            let joint_b = &segment.joints[bone_idx + 1];

            // Project both joints to screen space
            let screen_a = world_to_screen(
                joint_a.position,
                state.camera.position,
                state.camera.basis_x,
                state.camera.basis_y,
                state.camera.basis_z,
                fb_width,
                fb_height,
            );
            let screen_b = world_to_screen(
                joint_b.position,
                state.camera.position,
                state.camera.basis_x,
                state.camera.basis_y,
                state.camera.basis_z,
                fb_width,
                fb_height,
            );

            if let (Some((ax, ay)), Some((bx, by))) = (screen_a, screen_b) {
                // Calculate distance from click to line segment
                let dist = point_to_segment_dist(fb_x, fb_y, ax, ay, bx, by);
                if dist < bone_threshold {
                    if closest_bone.map_or(true, |(_, best_dist)| dist < best_dist) {
                        closest_bone = Some(((seg_idx, bone_idx), dist));
                    }
                }
            }
        }
    }

    // If a bone was clicked, select it (or add to selection with Shift)
    if let Some(((seg_idx, bone_idx), _)) = closest_bone {
        if shift_held {
            // Multi-select: add to or remove from existing bone selection
            match &state.selection {
                ModelerSelection::SpineBones(bones) => {
                    let mut new_bones = bones.clone();
                    let item = (seg_idx, bone_idx);
                    if let Some(pos) = new_bones.iter().position(|&b| b == item) {
                        // Already selected - remove it (toggle)
                        new_bones.remove(pos);
                        if new_bones.is_empty() {
                            state.selection = ModelerSelection::None;
                            state.set_status("Selection cleared", 0.5);
                        } else {
                            state.selection = ModelerSelection::SpineBones(new_bones);
                            state.set_status(&format!("Deselected bone {}", bone_idx), 0.5);
                        }
                    } else {
                        // Not selected - add it
                        new_bones.push(item);
                        state.selection = ModelerSelection::SpineBones(new_bones.clone());
                        state.set_status(&format!("{} bones selected", new_bones.len()), 0.5);
                    }
                }
                _ => {
                    // Start new bone selection
                    state.selection = ModelerSelection::SpineBones(vec![(seg_idx, bone_idx)]);
                    state.set_status(&format!("Bone {} (joints {}-{})", bone_idx, bone_idx, bone_idx + 1), 1.5);
                }
            }
        } else {
            // Normal click: replace selection
            state.selection = ModelerSelection::SpineBones(vec![(seg_idx, bone_idx)]);
            state.set_status(&format!("Bone {} (joints {}-{})", bone_idx, bone_idx, bone_idx + 1), 1.5);
        }
        return;
    }

    // Nothing clicked, clear selection (only if Shift not held)
    if !shift_held {
        state.selection = ModelerSelection::None;
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

/// Handle spine extrusion (E key) - adds a new joint extending from the last joint
fn handle_spine_extrude(state: &mut ModelerState) {
    // Get the selected joint(s) or bone(s)
    let selection = state.selection.clone();

    match selection {
        ModelerSelection::SpineJoints(joints) if !joints.is_empty() => {
            // Get the first selected joint
            let (seg_idx, joint_idx) = joints[0];

            if let Some(spine_model) = &mut state.spine_model {
                if let Some(segment) = spine_model.segments.get_mut(seg_idx) {
                    // Only allow extrusion from the last joint
                    if joint_idx == segment.joints.len() - 1 {
                        // Calculate direction from previous joint (or default up)
                        let direction = if segment.joints.len() >= 2 {
                            let prev_pos = segment.joints[joint_idx - 1].position;
                            let curr_pos = segment.joints[joint_idx].position;
                            (curr_pos - prev_pos).normalize()
                        } else {
                            Vec3::new(0.0, 1.0, 0.0) // Default up
                        };

                        // Get radius from current joint
                        let radius = segment.joints[joint_idx].radius;

                        // New joint extends in same direction, 20 units away
                        let new_pos = segment.joints[joint_idx].position + direction * 20.0;
                        let new_pos = state.snap_settings.snap_vec3(new_pos);

                        segment.joints.push(crate::modeler::SpineJoint::new(new_pos, radius));

                        // Select the new joint
                        let new_joint_idx = segment.joints.len() - 1;
                        state.selection = ModelerSelection::SpineJoints(vec![(seg_idx, new_joint_idx)]);
                        state.set_status(&format!("Extruded new joint {}", new_joint_idx), 1.0);
                    } else {
                        state.set_status("Extrude only from end joint", 1.5);
                    }
                }
            }
        }
        ModelerSelection::SpineBones(bones) if !bones.is_empty() => {
            // For bones, extrude from the end joint of the last bone
            let (seg_idx, bone_idx) = bones[0];

            if let Some(spine_model) = &mut state.spine_model {
                if let Some(segment) = spine_model.segments.get_mut(seg_idx) {
                    let end_joint_idx = bone_idx + 1;

                    // Only allow extrusion from the last bone
                    if end_joint_idx == segment.joints.len() - 1 {
                        // Calculate direction from the bone
                        let start_pos = segment.joints[bone_idx].position;
                        let end_pos = segment.joints[end_joint_idx].position;
                        let direction = (end_pos - start_pos).normalize();
                        let bone_length = (end_pos - start_pos).len();

                        // Get radius from end joint
                        let radius = segment.joints[end_joint_idx].radius;

                        // New joint extends in same direction, same length as current bone
                        let new_pos = end_pos + direction * bone_length;
                        let new_pos = state.snap_settings.snap_vec3(new_pos);

                        segment.joints.push(crate::modeler::SpineJoint::new(new_pos, radius));

                        // Select the new bone (the one we just created)
                        let new_bone_idx = segment.joints.len() - 2;
                        state.selection = ModelerSelection::SpineBones(vec![(seg_idx, new_bone_idx)]);
                        state.set_status(&format!("Extruded new bone {}", new_bone_idx), 1.0);
                    } else {
                        state.set_status("Extrude only from end bone", 1.5);
                    }
                }
            }
        }
        _ => {
            state.set_status("Select a joint or bone to extrude", 1.5);
        }
    }
}

/// Handle spine deletion (X key) - deletes selected joints or bones
fn handle_spine_delete(state: &mut ModelerState) {
    let selection = state.selection.clone();

    match selection {
        ModelerSelection::SpineJoints(joints) if !joints.is_empty() => {
            // Sort joints in reverse order to delete from end first (prevents index shifting issues)
            let mut sorted_joints = joints.clone();
            sorted_joints.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.cmp(&a.0)));

            let mut deleted_count = 0;

            if let Some(spine_model) = &mut state.spine_model {
                for (seg_idx, joint_idx) in sorted_joints {
                    if let Some(segment) = spine_model.segments.get_mut(seg_idx) {
                        // Must keep at least 2 joints for a valid segment
                        if segment.joints.len() > 2 {
                            segment.joints.remove(joint_idx);
                            deleted_count += 1;
                        }
                    }
                }
            }

            if deleted_count > 0 {
                state.selection = ModelerSelection::None;
                state.set_status(&format!("Deleted {} joint(s)", deleted_count), 1.0);
            } else {
                state.set_status("Need at least 2 joints", 1.5);
            }
        }
        ModelerSelection::SpineBones(bones) if !bones.is_empty() => {
            // Deleting a bone means removing one of its joints
            // We'll remove the end joint of each bone (which effectively removes the bone)
            let mut sorted_bones = bones.clone();
            sorted_bones.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.cmp(&a.0)));

            let mut deleted_count = 0;

            if let Some(spine_model) = &mut state.spine_model {
                for (seg_idx, bone_idx) in sorted_bones {
                    if let Some(segment) = spine_model.segments.get_mut(seg_idx) {
                        // Delete the end joint of the bone
                        let joint_to_delete = bone_idx + 1;

                        // Must keep at least 2 joints
                        if segment.joints.len() > 2 && joint_to_delete < segment.joints.len() {
                            segment.joints.remove(joint_to_delete);
                            deleted_count += 1;
                        }
                    }
                }
            }

            if deleted_count > 0 {
                state.selection = ModelerSelection::None;
                state.set_status(&format!("Deleted {} bone(s)", deleted_count), 1.0);
            } else {
                state.set_status("Need at least 2 joints", 1.5);
            }
        }
        _ => {
            state.set_status("Select joint(s) or bone(s) to delete", 1.5);
        }
    }
}

/// Handle spine subdivide (W key) - insert a joint at the midpoint of selected bone
fn handle_spine_subdivide(state: &mut ModelerState) {
    let selection = state.selection.clone();

    match selection {
        ModelerSelection::SpineBones(bones) if !bones.is_empty() => {
            // Get the first selected bone
            let (seg_idx, bone_idx) = bones[0];

            if let Some(spine_model) = &mut state.spine_model {
                if let Some(segment) = spine_model.segments.get_mut(seg_idx) {
                    if bone_idx + 1 < segment.joints.len() {
                        // Get the two joints that form this bone
                        let joint_a = segment.joints[bone_idx].clone();
                        let joint_b = segment.joints[bone_idx + 1].clone();

                        // Calculate midpoint position and interpolated radius
                        let mid_pos = (joint_a.position + joint_b.position) * 0.5;
                        let mid_radius = (joint_a.radius + joint_b.radius) * 0.5;

                        // Snap the position if enabled
                        let mid_pos = state.snap_settings.snap_vec3(mid_pos);

                        // Insert the new joint after joint_a (at bone_idx + 1)
                        let new_joint = crate::modeler::SpineJoint::new(mid_pos, mid_radius);
                        segment.joints.insert(bone_idx + 1, new_joint);

                        // Select the new joint
                        state.selection = ModelerSelection::SpineJoints(vec![(seg_idx, bone_idx + 1)]);
                        state.set_status(&format!("Subdivided bone {} -> joints {}, {}, {}", bone_idx, bone_idx, bone_idx + 1, bone_idx + 2), 1.5);
                    }
                }
            }
        }
        ModelerSelection::SpineJoints(_) => {
            state.set_status("Select a bone to subdivide (W)", 1.5);
        }
        _ => {
            state.set_status("Select a bone to subdivide (W)", 1.5);
        }
    }
}

/// Handle spine duplicate segment (D key) - copy the current segment
fn handle_spine_duplicate_segment(state: &mut ModelerState) {
    // Determine which segment to duplicate based on selection
    let seg_idx = match &state.selection {
        ModelerSelection::SpineJoints(joints) if !joints.is_empty() => joints[0].0,
        ModelerSelection::SpineBones(bones) if !bones.is_empty() => bones[0].0,
        _ => {
            // No selection, try segment 0 if it exists
            if state.spine_model.as_ref().map_or(false, |m| !m.segments.is_empty()) {
                0
            } else {
                state.set_status("No segment to duplicate", 1.5);
                return;
            }
        }
    };

    if let Some(spine_model) = &mut state.spine_model {
        if let Some(segment) = spine_model.segments.get(seg_idx) {
            // Clone the segment
            let mut new_segment = segment.clone();

            // Generate a unique name
            let base_name = segment.name.trim_end_matches(|c: char| c.is_numeric() || c == '_');
            let new_name = format!("{}_copy", base_name);
            new_segment.name = new_name;

            // Offset the new segment (move it to the side)
            let offset = Vec3::new(50.0, 0.0, 0.0);
            for joint in &mut new_segment.joints {
                joint.position = joint.position + offset;
            }

            // Add the new segment
            let new_seg_idx = spine_model.segments.len();
            spine_model.segments.push(new_segment);

            // Select the first joint of the new segment
            state.selection = ModelerSelection::SpineJoints(vec![(new_seg_idx, 0)]);
            state.set_status(&format!("Duplicated segment -> segment {}", new_seg_idx), 1.5);
        }
    }
}

/// Handle spine new segment (N key) - create a new segment
fn handle_spine_new_segment(state: &mut ModelerState) {
    if let Some(spine_model) = &mut state.spine_model {
        let new_seg_idx = spine_model.create_default_segment();
        state.selection = ModelerSelection::SpineJoints(vec![(new_seg_idx, 0)]);
        state.set_status(&format!("Created new segment {}", new_seg_idx), 1.5);
    } else {
        // No spine model exists, create one
        state.spine_model = Some(crate::modeler::SpineModel::new_empty("new_model"));
        state.selection = ModelerSelection::SpineJoints(vec![(0, 0)]);
        state.set_status("Created new spine model", 1.5);
    }
}

/// Handle spine delete segment (Shift+X key) - delete the entire segment
fn handle_spine_delete_segment(state: &mut ModelerState) {
    // Determine which segment to delete based on selection
    let seg_idx = match &state.selection {
        ModelerSelection::SpineJoints(joints) if !joints.is_empty() => joints[0].0,
        ModelerSelection::SpineBones(bones) if !bones.is_empty() => bones[0].0,
        _ => {
            state.set_status("Select a joint or bone to delete its segment", 1.5);
            return;
        }
    };

    if let Some(spine_model) = &mut state.spine_model {
        if spine_model.remove_segment(seg_idx) {
            state.selection = ModelerSelection::None;
            state.set_status(&format!("Deleted segment {}", seg_idx), 1.5);
        } else {
            state.set_status("Cannot delete last segment", 1.5);
        }
    }
}

/// Handle spine mirror segment (M key) - create a mirrored copy on X axis
fn handle_spine_mirror_segment(state: &mut ModelerState) {
    // Determine which segment to mirror based on selection
    let seg_idx = match &state.selection {
        ModelerSelection::SpineJoints(joints) if !joints.is_empty() => joints[0].0,
        ModelerSelection::SpineBones(bones) if !bones.is_empty() => bones[0].0,
        _ => {
            // No selection, try segment 0 if it exists
            if state.spine_model.as_ref().map_or(false, |m| !m.segments.is_empty()) {
                0
            } else {
                state.set_status("No segment to mirror", 1.5);
                return;
            }
        }
    };

    if let Some(spine_model) = &mut state.spine_model {
        if let Some(new_idx) = spine_model.mirror_segment(seg_idx) {
            state.selection = ModelerSelection::SpineJoints(vec![(new_idx, 0)]);
            state.set_status(&format!("Mirrored segment -> segment {}", new_idx), 1.5);
        } else {
            state.set_status("Failed to mirror segment", 1.5);
        }
    }
}
