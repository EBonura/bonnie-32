//! 3D Viewport for the modeler - renders models using the software rasterizer

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext};
use crate::rasterizer::{
    Framebuffer, render_mesh, Color as RasterColor, Vec3,
    Vertex as RasterVertex, Face as RasterFace, WIDTH, HEIGHT,
    world_to_screen, world_to_screen_with_depth, Mat4,
    mat4_identity, mat4_mul, mat4_transform_point, mat4_from_position_rotation,
};
use super::state::{ModelerState, ModelerSelection, SelectMode, Axis, GizmoHandle, ModalTransform, DataContext, RiggedModel, RigSubMode};
use super::spine::SpineModel;

/// Compute world matrices for all bones in a rigged model's skeleton
fn compute_rig_bone_world_transforms(rig: &RiggedModel) -> Vec<Mat4> {
    let mut matrices = vec![mat4_identity(); rig.skeleton.len()];

    for (i, bone) in rig.skeleton.iter().enumerate() {
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

/// Handle rig bone transform interactions (placeholder - to be implemented)
fn handle_rig_bone_transforms(
    _ctx: &UiContext,
    _state: &mut ModelerState,
    _inside_viewport: bool,
    _mouse_pos: (f32, f32),
) {
    // Rig bone transforms to be implemented in later phase
}

/// Get all selected element positions for modal transforms
fn get_selected_positions(state: &ModelerState) -> Vec<Vec3> {
    let mut positions = Vec::new();

    match &state.selection {
        ModelerSelection::SpineJoints(joints) => {
            if let Some(spine_model) = &state.spine_model {
                for (seg_idx, joint_idx) in joints {
                    if let Some(segment) = spine_model.segments.get(*seg_idx) {
                        if let Some(joint) = segment.joints.get(*joint_idx) {
                            positions.push(joint.position);
                        }
                    }
                }
            }
        }
        ModelerSelection::SpineBones(bones) => {
            if let Some(spine_model) = &state.spine_model {
                for (seg_idx, bone_idx) in bones {
                    if let Some(segment) = spine_model.segments.get(*seg_idx) {
                        if let Some(joint_a) = segment.joints.get(*bone_idx) {
                            positions.push(joint_a.position);
                        }
                        if let Some(joint_b) = segment.joints.get(*bone_idx + 1) {
                            positions.push(joint_b.position);
                        }
                    }
                }
            }
        }
        ModelerSelection::MeshVertices(verts) => {
            if let Some(editable_mesh) = &state.editable_mesh {
                for vert_idx in verts {
                    if let Some(vert) = editable_mesh.vertices.get(*vert_idx) {
                        positions.push(vert.pos);
                    }
                }
            }
        }
        ModelerSelection::MeshEdges(edges) => {
            if let Some(editable_mesh) = &state.editable_mesh {
                for (v0, v1) in edges {
                    if let Some(vert0) = editable_mesh.vertices.get(*v0) {
                        positions.push(vert0.pos);
                    }
                    if let Some(vert1) = editable_mesh.vertices.get(*v1) {
                        positions.push(vert1.pos);
                    }
                }
            }
        }
        ModelerSelection::MeshFaces(faces) => {
            if let Some(editable_mesh) = &state.editable_mesh {
                for face_idx in faces {
                    if let Some(face) = editable_mesh.faces.get(*face_idx) {
                        if let Some(v0) = editable_mesh.vertices.get(face.v0) {
                            positions.push(v0.pos);
                        }
                        if let Some(v1) = editable_mesh.vertices.get(face.v1) {
                            positions.push(v1.pos);
                        }
                        if let Some(v2) = editable_mesh.vertices.get(face.v2) {
                            positions.push(v2.pos);
                        }
                    }
                }
            }
        }
        ModelerSelection::RigBones(bones) => {
            // For rig bones, return the bone joint position in world space
            if let Some(rig) = &state.rigged_model {
                let bone_transforms = compute_rig_bone_world_transforms(rig);
                for &bone_idx in bones {
                    if let Some(world_mat) = bone_transforms.get(bone_idx) {
                        // Position is the translation component of the matrix
                        positions.push(Vec3::new(world_mat[0][3], world_mat[1][3], world_mat[2][3]));
                    }
                }
            }
        }
        _ => {}
    }

    positions
}

/// Apply positions back to selected elements
fn apply_selected_positions(state: &mut ModelerState, positions: &[Vec3]) {
    let mut pos_idx = 0;

    match &state.selection.clone() {
        ModelerSelection::SpineJoints(joints) => {
            if let Some(spine_model) = &mut state.spine_model {
                for (seg_idx, joint_idx) in joints {
                    if let Some(segment) = spine_model.segments.get_mut(*seg_idx) {
                        if let Some(joint) = segment.joints.get_mut(*joint_idx) {
                            if let Some(&new_pos) = positions.get(pos_idx) {
                                joint.position = new_pos;
                            }
                            pos_idx += 1;
                        }
                    }
                }
            }
            state.spine_mesh_dirty = true;
        }
        ModelerSelection::SpineBones(bones) => {
            if let Some(spine_model) = &mut state.spine_model {
                for (seg_idx, bone_idx) in bones {
                    if let Some(segment) = spine_model.segments.get_mut(*seg_idx) {
                        if let Some(joint_a) = segment.joints.get_mut(*bone_idx) {
                            if let Some(&new_pos) = positions.get(pos_idx) {
                                joint_a.position = new_pos;
                            }
                            pos_idx += 1;
                        }
                        if let Some(joint_b) = segment.joints.get_mut(*bone_idx + 1) {
                            if let Some(&new_pos) = positions.get(pos_idx) {
                                joint_b.position = new_pos;
                            }
                            pos_idx += 1;
                        }
                    }
                }
            }
            state.spine_mesh_dirty = true;
        }
        ModelerSelection::MeshVertices(verts) => {
            if let Some(editable_mesh) = &mut state.editable_mesh {
                for vert_idx in verts {
                    if let Some(vert) = editable_mesh.vertices.get_mut(*vert_idx) {
                        if let Some(&new_pos) = positions.get(pos_idx) {
                            vert.pos = new_pos;
                        }
                        pos_idx += 1;
                    }
                }
            }
        }
        ModelerSelection::MeshEdges(edges) => {
            if let Some(editable_mesh) = &mut state.editable_mesh {
                for (v0, v1) in edges {
                    if let Some(vert0) = editable_mesh.vertices.get_mut(*v0) {
                        if let Some(&new_pos) = positions.get(pos_idx) {
                            vert0.pos = new_pos;
                        }
                        pos_idx += 1;
                    }
                    if let Some(vert1) = editable_mesh.vertices.get_mut(*v1) {
                        if let Some(&new_pos) = positions.get(pos_idx) {
                            vert1.pos = new_pos;
                        }
                        pos_idx += 1;
                    }
                }
            }
        }
        ModelerSelection::MeshFaces(faces) => {
            if let Some(editable_mesh) = &mut state.editable_mesh {
                let mesh_faces = editable_mesh.faces.clone();
                for face_idx in faces {
                    if let Some(face) = mesh_faces.get(*face_idx) {
                        if let Some(v0) = editable_mesh.vertices.get_mut(face.v0) {
                            if let Some(&new_pos) = positions.get(pos_idx) {
                                v0.pos = new_pos;
                            }
                            pos_idx += 1;
                        }
                        if let Some(v1) = editable_mesh.vertices.get_mut(face.v1) {
                            if let Some(&new_pos) = positions.get(pos_idx) {
                                v1.pos = new_pos;
                            }
                            pos_idx += 1;
                        }
                        if let Some(v2) = editable_mesh.vertices.get_mut(face.v2) {
                            if let Some(&new_pos) = positions.get(pos_idx) {
                                v2.pos = new_pos;
                            }
                            pos_idx += 1;
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

/// Start rig bone rotation mode (R key in Animate mode)
fn start_rig_bone_rotation(state: &mut ModelerState, mouse_pos: (f32, f32)) {
    let selected_bones = match &state.selection {
        ModelerSelection::RigBones(bones) => bones.clone(),
        _ => {
            state.set_status("Select bones to rotate", 1.0);
            return;
        }
    };

    if selected_bones.is_empty() {
        state.set_status("No bones selected", 1.0);
        return;
    }

    let Some(rig) = &state.rigged_model else {
        return;
    };

    // Store original rotations
    let start_rotations: Vec<Vec3> = selected_bones.iter()
        .filter_map(|&idx| rig.skeleton.get(idx).map(|b| b.local_rotation))
        .collect();

    state.rig_bone_rotating = true;
    state.modal_transform_start_mouse = mouse_pos;
    state.rig_bone_start_rotations = start_rotations;
    state.axis_lock = None;
    state.set_status("Rotate: move mouse. X/Y/Z to constrain. LMB confirm, RMB/Esc cancel", 5.0);
}

/// Handle rig bone rotation (called every frame when rig_bone_rotating is true)
fn handle_rig_bone_rotation(state: &mut ModelerState, mouse_pos: (f32, f32)) {
    if !state.rig_bone_rotating {
        return;
    }

    // Check for axis constraints
    if is_key_pressed(KeyCode::X) {
        state.axis_lock = Some(Axis::X);
        state.set_status("Rotate constrained to X axis", 2.0);
    }
    if is_key_pressed(KeyCode::Y) {
        state.axis_lock = Some(Axis::Y);
        state.set_status("Rotate constrained to Y axis", 2.0);
    }
    if is_key_pressed(KeyCode::Z) {
        state.axis_lock = Some(Axis::Z);
        state.set_status("Rotate constrained to Z axis", 2.0);
    }

    // Calculate rotation delta (degrees per pixel)
    let dx = mouse_pos.0 - state.modal_transform_start_mouse.0;
    let rotation_degrees = dx * 0.5; // 0.5 degrees per pixel

    let selected_bones = match &state.selection {
        ModelerSelection::RigBones(bones) => bones.clone(),
        _ => return,
    };

    let axis_lock = state.axis_lock;
    let start_rotations = state.rig_bone_start_rotations.clone();

    // Check for confirm/cancel before mutating
    let confirm = is_mouse_button_pressed(MouseButton::Left);
    let cancel = is_key_pressed(KeyCode::Escape) || is_mouse_button_pressed(MouseButton::Right);

    let Some(rig) = &mut state.rigged_model else { return; };

    if cancel {
        // Restore original rotations
        for (i, &bone_idx) in selected_bones.iter().enumerate() {
            if let (Some(bone), Some(&start_rot)) = (rig.skeleton.get_mut(bone_idx), start_rotations.get(i)) {
                bone.local_rotation = start_rot;
            }
        }
        state.rig_bone_rotating = false;
        state.set_status("Rotation cancelled", 1.0);
        return;
    }

    // Apply rotation to each selected bone
    for (i, &bone_idx) in selected_bones.iter().enumerate() {
        if let (Some(bone), Some(&start_rot)) = (rig.skeleton.get_mut(bone_idx), start_rotations.get(i)) {
            bone.local_rotation = match axis_lock {
                Some(Axis::X) => Vec3::new(start_rot.x + rotation_degrees, start_rot.y, start_rot.z),
                Some(Axis::Y) => Vec3::new(start_rot.x, start_rot.y + rotation_degrees, start_rot.z),
                Some(Axis::Z) => Vec3::new(start_rot.x, start_rot.y, start_rot.z + rotation_degrees),
                None => Vec3::new(start_rot.x, start_rot.y + rotation_degrees, start_rot.z), // Default Y axis
            };
        }
    }

    // Confirm on left click
    if confirm {
        state.rig_bone_rotating = false;
        state.set_status("Rotation confirmed", 1.0);
    }
}

/// Handle active modal transform (G/S/R)
fn handle_modal_transform(state: &mut ModelerState, mouse_pos: (f32, f32), _inside_viewport: bool) {
    if state.modal_transform == ModalTransform::None {
        return;
    }

    // Check for axis constraints
    if is_key_pressed(KeyCode::X) {
        state.axis_lock = Some(Axis::X);
        state.set_status(&format!("{} constrained to X axis", state.modal_transform.label()), 2.0);
    }
    if is_key_pressed(KeyCode::Y) {
        state.axis_lock = Some(Axis::Y);
        state.set_status(&format!("{} constrained to Y axis", state.modal_transform.label()), 2.0);
    }
    if is_key_pressed(KeyCode::Z) {
        state.axis_lock = Some(Axis::Z);
        state.set_status(&format!("{} constrained to Z axis", state.modal_transform.label()), 2.0);
    }

    // Calculate mouse delta from start
    let dx = mouse_pos.0 - state.modal_transform_start_mouse.0;
    let dy = mouse_pos.1 - state.modal_transform_start_mouse.1;

    // Calculate new positions based on transform type
    let new_positions: Vec<Vec3> = match state.modal_transform {
        ModalTransform::Grab => {
            // Move: translate by mouse delta
            let move_scale = 0.5; // Pixels to world units
            let move_x = dx * move_scale;
            let move_y = -dy * move_scale; // Invert Y for screen coords

            // Get camera vectors for world-space movement
            let cam_right = state.camera.basis_x;
            let cam_up = state.camera.basis_y;

            state.modal_transform_start_positions.iter().map(|&start_pos| {
                let delta = match state.axis_lock {
                    Some(Axis::X) => Vec3::new(move_x + move_y, 0.0, 0.0),
                    Some(Axis::Y) => Vec3::new(0.0, move_x + move_y, 0.0),
                    Some(Axis::Z) => Vec3::new(0.0, 0.0, move_x + move_y),
                    None => cam_right * move_x + cam_up * move_y,
                };
                start_pos + delta
            }).collect()
        }
        ModalTransform::Scale => {
            // Scale factor based on horizontal mouse movement (like Blender)
            let scale = (1.0 + dx * 0.01).max(0.01);

            state.modal_transform_start_positions.iter().map(|&start_pos| {
                let offset = start_pos - state.modal_transform_center;
                let scaled_offset = match state.axis_lock {
                    Some(Axis::X) => Vec3::new(offset.x * scale, offset.y, offset.z),
                    Some(Axis::Y) => Vec3::new(offset.x, offset.y * scale, offset.z),
                    Some(Axis::Z) => Vec3::new(offset.x, offset.y, offset.z * scale),
                    None => offset * scale,
                };
                state.modal_transform_center + scaled_offset
            }).collect()
        }
        ModalTransform::Rotate => {
            // Rotate: rotate around center based on mouse horizontal movement
            let angle = dx * 0.01; // Radians per pixel

            state.modal_transform_start_positions.iter().map(|&start_pos| {
                let offset = start_pos - state.modal_transform_center;

                // Rotate based on axis constraint
                let rotated_offset = match state.axis_lock {
                    Some(Axis::X) => {
                        // Rotate around X axis
                        let cos_a = angle.cos();
                        let sin_a = angle.sin();
                        Vec3::new(
                            offset.x,
                            offset.y * cos_a - offset.z * sin_a,
                            offset.y * sin_a + offset.z * cos_a,
                        )
                    }
                    Some(Axis::Y) => {
                        // Rotate around Y axis
                        let cos_a = angle.cos();
                        let sin_a = angle.sin();
                        Vec3::new(
                            offset.x * cos_a + offset.z * sin_a,
                            offset.y,
                            -offset.x * sin_a + offset.z * cos_a,
                        )
                    }
                    Some(Axis::Z) => {
                        // Rotate around Z axis
                        let cos_a = angle.cos();
                        let sin_a = angle.sin();
                        Vec3::new(
                            offset.x * cos_a - offset.y * sin_a,
                            offset.x * sin_a + offset.y * cos_a,
                            offset.z,
                        )
                    }
                    None => {
                        // Default: rotate around Y axis (vertical axis)
                        let cos_a = angle.cos();
                        let sin_a = angle.sin();
                        Vec3::new(
                            offset.x * cos_a + offset.z * sin_a,
                            offset.y,
                            -offset.x * sin_a + offset.z * cos_a,
                        )
                    }
                };

                state.modal_transform_center + rotated_offset
            }).collect()
        }
        ModalTransform::None => return,
    };

    // Apply positions temporarily (real-time preview)
    apply_selected_positions(state, &new_positions);

    // Confirm on left click
    if is_mouse_button_pressed(MouseButton::Left) {
        state.modal_transform = ModalTransform::None;
        state.dirty = true;
        state.set_status("Transform applied", 1.0);
    }

    // Cancel on ESC or right click
    if is_key_pressed(KeyCode::Escape) || is_mouse_button_pressed(MouseButton::Right) {
        // Restore original positions
        apply_selected_positions(state, &state.modal_transform_start_positions.clone());
        state.modal_transform = ModalTransform::None;
        state.set_status("Transform cancelled", 1.0);
    }
}

/// Compute world matrices for rigged model parts
fn compute_part_world_matrices(rig: &RiggedModel) -> Vec<Mat4> {
    // First compute bone matrices
    let bone_matrices = compute_rig_bone_world_transforms(rig);

    // Then compute part matrices (each part follows its bone)
    let mut part_matrices = Vec::with_capacity(rig.parts.len());

    for part in &rig.parts {
        let mat = if let Some(bone_idx) = part.bone_index {
            bone_matrices.get(bone_idx).copied().unwrap_or(mat4_identity())
        } else {
            // Unassigned parts stay at identity
            mat4_identity()
        };
        part_matrices.push(mat);
    }

    part_matrices
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

    // Mouse wheel: zoom (use ctx.mouse.scroll to respect modal blocking)
    if inside_viewport {
        let scroll = ctx.mouse.scroll;
        if scroll != 0.0 {
            let zoom_factor = if scroll > 0.0 { 0.98 } else { 1.02 };
            state.orbit_distance = (state.orbit_distance * zoom_factor).clamp(50.0, 2000.0);
            state.sync_camera_from_orbit();
        }
    }

    // Update mouse position for next frame
    state.viewport_last_mouse = mouse_pos;

    // Note: 1/2/3 keys are handled in layout.rs for context switching (Spine/Mesh/Rig)
    // Select mode switching (Vertex/Edge/Face) is done via toolbar buttons to avoid conflicts
    // 4 key for SpineBone mode (doesn't conflict with context switching)
    if inside_viewport && !state.spine_drag_active && !state.transform_active {
        if is_key_pressed(KeyCode::Key4) {
            state.select_mode = SelectMode::SpineBone;
            state.selection = ModelerSelection::None;
            state.set_status("Bone mode", 1.0);
        }
    }

    // Modal transforms: G = Grab, S = Scale, R = Rotate (Blender-style)
    // Only start if we have a selection and no transform is already active
    let has_selection = !state.selection.is_empty();
    let no_transform_active = !state.spine_drag_active && !state.transform_active && state.modal_transform == ModalTransform::None;

    if inside_viewport && has_selection && no_transform_active {
        let start_modal = |mode: ModalTransform, state: &mut ModelerState, mouse: (f32, f32)| {
            // Collect all selected positions
            let positions = get_selected_positions(state);
            if positions.is_empty() {
                return;
            }

            // Calculate center of selection
            let center = positions.iter().fold(Vec3::ZERO, |acc, p| acc + *p) * (1.0 / positions.len() as f32);

            state.modal_transform = mode;
            state.modal_transform_start_mouse = mouse;
            state.modal_transform_start_positions = positions;
            state.modal_transform_center = center;
            state.axis_lock = None;

            state.set_status(&format!("{} - move mouse, click to confirm, ESC to cancel, X/Y/Z to constrain", mode.label()), 5.0);
        };

        if is_key_pressed(KeyCode::G) {
            start_modal(ModalTransform::Grab, state, mouse_pos);
        } else if is_key_pressed(KeyCode::S) {
            start_modal(ModalTransform::Scale, state, mouse_pos);
        } else if is_key_pressed(KeyCode::R) {
            start_modal(ModalTransform::Rotate, state, mouse_pos);
        }
    }

    // Handle active modal transform
    handle_modal_transform(state, mouse_pos, inside_viewport);

    // Handle rig bone rotation (separate from modal transform)
    handle_rig_bone_rotation(state, mouse_pos);

    // Extrude (E key) - context-dependent:
    // - In face mode: extrude selected faces
    // - In bone mode: add new joint at end of spine
    if inside_viewport && is_key_pressed(KeyCode::E) && !state.spine_drag_active && !state.transform_active {
        if state.select_mode == SelectMode::Face {
            handle_face_extrude(state);
        } else {
            handle_spine_extrude(state);
        }
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

    // Separate mesh (P key) - split selected faces into new MeshPart, create RiggedModel
    if inside_viewport && is_key_pressed(KeyCode::P) && !state.spine_drag_active && !state.transform_active {
        if state.data_context == DataContext::Mesh {
            handle_mesh_separate(state);
        }
    }

    // Rig bone operations (in Rig context, Skeleton or Animate sub-mode)
    if inside_viewport && state.data_context == DataContext::Rig && !state.transform_active {
        match state.rig_sub_mode {
            RigSubMode::Skeleton => {
                // Extrude bone (E key) - create child bone from selected
                if is_key_pressed(KeyCode::E) {
                    handle_rig_bone_extrude(state);
                }
                // Delete bone (X key) - remove selected bones
                if is_key_pressed(KeyCode::X) && !shift_held {
                    handle_rig_bone_delete(state);
                }
                // Add root bone (N key) - create new root bone at origin
                if is_key_pressed(KeyCode::N) {
                    handle_rig_add_root_bone(state);
                }
            }
            RigSubMode::Parts => {
                // Ctrl+P to assign part to bone 0
                let ctrl_held = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl);
                if is_key_pressed(KeyCode::P) && ctrl_held {
                    handle_rig_assign_part_to_bone(state, 0);
                }
                // Alt+P to unassign part from bone
                let alt_held = is_key_down(KeyCode::LeftAlt) || is_key_down(KeyCode::RightAlt);
                if is_key_pressed(KeyCode::P) && alt_held {
                    handle_rig_unassign_part(state);
                }
                // Number keys 1-9 to assign to bone index 0-8
                for (key, idx) in [
                    (KeyCode::Key1, 0), (KeyCode::Key2, 1), (KeyCode::Key3, 2),
                    (KeyCode::Key4, 3), (KeyCode::Key5, 4), (KeyCode::Key6, 5),
                    (KeyCode::Key7, 6), (KeyCode::Key8, 7), (KeyCode::Key9, 8),
                ] {
                    if is_key_pressed(key) && ctrl_held {
                        handle_rig_assign_part_to_bone(state, idx);
                        break;
                    }
                }
            }
            RigSubMode::Animate => {
                // R key for bone rotation in Animate mode
                if is_key_pressed(KeyCode::R) {
                    start_rig_bone_rotation(state, mouse_pos);
                }
            }
        }
    }

    // Handle rig bone transforms (gizmo interactions)
    handle_rig_bone_transforms(ctx, state, inside_viewport, mouse_pos);

    // Clear framebuffer
    fb.clear(RasterColor::new(40, 40, 50));

    // Draw grid on floor
    draw_grid(fb, &state.camera, 0.0, 50.0, 10);

    // Build render data based on current data context
    let mut all_vertices: Vec<RasterVertex> = Vec::new();
    let mut all_faces: Vec<RasterFace> = Vec::new();

    // Render based on data context
    match state.data_context {
        DataContext::Mesh => {
            // Render editable mesh
            if let Some(mesh) = &state.editable_mesh {
                let vertex_offset = all_vertices.len();

                for vert in &mesh.vertices {
                    all_vertices.push(vert.clone());
                }

                for face in &mesh.faces {
                    all_faces.push(RasterFace {
                        v0: face.v0 + vertex_offset,
                        v1: face.v1 + vertex_offset,
                        v2: face.v2 + vertex_offset,
                        texture_id: face.texture_id,
                    });
                }
            }
        }
        DataContext::Spine => {
            // Render spine model
            if let Some(spine_model) = &state.spine_model {
                let (spine_verts, spine_faces) = spine_model.generate_mesh();

                for vert in spine_verts {
                    all_vertices.push(vert);
                }

                for face in spine_faces {
                    all_faces.push(face);
                }
            }
        }
        DataContext::Rig => {
            // Render rigged model parts
            if let Some(rig) = &state.rigged_model {
                let part_matrices = compute_part_world_matrices(rig);

                for (part_idx, part) in rig.parts.iter().enumerate() {
                    let world_mat = part_matrices.get(part_idx).copied().unwrap_or(mat4_identity());
                    let vertex_offset = all_vertices.len();

                    // Transform vertices by part's world matrix
                    for vert in &part.mesh.vertices {
                        let world_pos = mat4_transform_point(&world_mat, vert.pos);

                        all_vertices.push(RasterVertex {
                            pos: world_pos,
                            uv: vert.uv,
                            normal: vert.normal,
                            color: RasterColor::NEUTRAL,
                            bone_index: part.bone_index,
                        });
                    }

                    // Add faces with offset indices
                    for face in &part.mesh.faces {
                        all_faces.push(RasterFace {
                            v0: face.v0 + vertex_offset,
                            v1: face.v1 + vertex_offset,
                            v2: face.v2 + vertex_offset,
                            texture_id: face.texture_id,
                        });
                    }
                }
            }
        }
    }

    // Track which model systems are in use for selection overlays and drag handling
    let using_spine = state.data_context == DataContext::Spine && state.spine_model.is_some();
    let using_mesh = state.data_context == DataContext::Mesh && state.editable_mesh.is_some();

    // Render using software rasterizer
    let empty_textures: Vec<crate::rasterizer::Texture> = Vec::new();
    render_mesh(fb, &all_vertices, &all_faces, &empty_textures, &state.camera, &state.raster_settings);

    // Draw spine joint markers and mesh overlays
    let mut gizmo_info: Option<GizmoScreenInfo> = None;
    if let Some(spine_model) = &state.spine_model {
        let selected_joints = state.selection.spine_joints().unwrap_or(&[]);
        let selected_bones = state.selection.spine_bones().unwrap_or(&[]);

        // Draw spine joints in Joint/Bone mode
        if matches!(state.select_mode, SelectMode::Joint | SelectMode::SpineBone) {
            draw_spine_joints(fb, spine_model, &state.camera, selected_joints, selected_bones);
        }

        // Draw mesh vertices/edges/faces overlays in mesh editing modes
        draw_spine_mesh_overlays(fb, spine_model, &state.camera, &state.selection, state.select_mode);

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

    // Draw rig skeleton and part selection if in Rig context
    if state.data_context == DataContext::Rig {
        if let Some(rig) = &state.rigged_model {
            let bone_transforms = compute_rig_bone_world_transforms(rig);
            let part_matrices = compute_part_world_matrices(rig);

            // Draw bone skeleton
            let selected_bones = match &state.selection {
                ModelerSelection::RigBones(bones) => bones.as_slice(),
                _ => &[],
            };
            draw_rig_bones(fb, rig, &state.camera, &bone_transforms, selected_bones);

            // Draw part selection overlay
            let selected_parts = match &state.selection {
                ModelerSelection::RigParts(parts) => parts.as_slice(),
                _ => &[],
            };
            if !selected_parts.is_empty() {
                draw_rig_part_selection(fb, rig, &state.camera, &part_matrices, selected_parts);
            }
        }
    }

    // Draw editable mesh selection overlays in Mesh context
    if state.data_context == DataContext::Mesh {
        draw_mesh_selection_overlays(fb, state);

        // Draw gizmo for mesh selections (at center of all selected elements)
        if let Some(mesh) = &state.editable_mesh {
            let gizmo_center = match &state.selection {
                ModelerSelection::MeshVertices(verts) if !verts.is_empty() => {
                    // Gizmo at center of all selected vertices
                    let mut sum = Vec3::ZERO;
                    let mut count = 0;
                    for &idx in verts {
                        if let Some(vert) = mesh.vertices.get(idx) {
                            sum = sum + vert.pos;
                            count += 1;
                        }
                    }
                    if count > 0 {
                        Some(sum * (1.0 / count as f32))
                    } else {
                        None
                    }
                }
                ModelerSelection::MeshEdges(edges) if !edges.is_empty() => {
                    // Gizmo at center of all selected edge midpoints
                    let mut sum = Vec3::ZERO;
                    let mut count = 0;
                    for &(v0, v1) in edges {
                        if let (Some(vert0), Some(vert1)) = (mesh.vertices.get(v0), mesh.vertices.get(v1)) {
                            sum = sum + (vert0.pos + vert1.pos) * 0.5;
                            count += 1;
                        }
                    }
                    if count > 0 {
                        Some(sum * (1.0 / count as f32))
                    } else {
                        None
                    }
                }
                ModelerSelection::MeshFaces(faces) if !faces.is_empty() => {
                    // Gizmo at center of all selected face centroids
                    let mut sum = Vec3::ZERO;
                    let mut count = 0;
                    for &idx in faces {
                        if let Some(face) = mesh.faces.get(idx) {
                            if let (Some(v0), Some(v1), Some(v2)) = (
                                mesh.vertices.get(face.v0),
                                mesh.vertices.get(face.v1),
                                mesh.vertices.get(face.v2),
                            ) {
                                sum = sum + (v0.pos + v1.pos + v2.pos) * (1.0 / 3.0);
                                count += 1;
                            }
                        }
                    }
                    if count > 0 {
                        Some(sum * (1.0 / count as f32))
                    } else {
                        None
                    }
                }
                _ => None,
            };

            if let Some(center) = gizmo_center {
                gizmo_info = Some(draw_gizmo(
                    fb,
                    center,
                    &state.camera,
                    state.gizmo_hover_handle,
                    state.spine_drag_handle,
                ));
            }
        }
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
        // Route to appropriate selection handler based on data context and select mode
        match state.data_context {
            DataContext::Spine => {
                match state.select_mode {
                    SelectMode::Joint | SelectMode::SpineBone => {
                        handle_spine_selection_click(state, screen_to_fb, fb.width, fb.height);
                    }
                    SelectMode::Vertex | SelectMode::Edge | SelectMode::Face => {
                        // Mesh editing modes (for spine mesh) - placeholder
                        handle_spine_selection_click(state, screen_to_fb, fb.width, fb.height);
                    }
                    _ => {
                        handle_spine_selection_click(state, screen_to_fb, fb.width, fb.height);
                    }
                }
            }
            DataContext::Mesh => {
                // Editable mesh selection
                if state.editable_mesh.is_some() {
                    handle_mesh_selection_click(state, screen_to_fb, fb.width, fb.height);
                }
            }
            DataContext::Rig => {
                // Rig selection (parts or bones depending on sub-mode)
                if state.rigged_model.is_some() {
                    handle_rig_selection_click(state, screen_to_fb, fb.width, fb.height);
                }
            }
        }
    }

    // Handle box selection (B key to start, or auto-start when clicking on empty space in mesh modes)
    handle_box_selection(ctx, state, inside_viewport, mouse_pos, screen_to_fb, fb.width, fb.height);

    // Draw box selection rectangle if active
    if state.box_select_active {
        let (start_x, start_y) = state.box_select_start;
        let min_x = start_x.min(mouse_pos.0);
        let min_y = start_y.min(mouse_pos.1);
        let max_x = start_x.max(mouse_pos.0);
        let max_y = start_y.max(mouse_pos.1);

        // Draw on top of framebuffer (macroquad coordinates)
        draw_rectangle_lines(min_x, min_y, max_x - min_x, max_y - min_y, 1.0, Color::from_rgba(255, 200, 50, 255));
        draw_rectangle(min_x, min_y, max_x - min_x, max_y - min_y, Color::from_rgba(255, 200, 50, 30));
    }

    // Handle element dragging AFTER selection (so newly selected element can be dragged)
    // Works for spine joints/bones AND mesh vertices/edges/faces
    if (using_spine || using_mesh) && !state.box_select_active {
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

/// Draw the skeleton bones for a rigged model
fn draw_rig_bones(
    fb: &mut Framebuffer,
    rig: &RiggedModel,
    camera: &crate::rasterizer::Camera,
    bone_transforms: &[[[f32; 4]; 4]],
    selected_bones: &[usize],
) {
    let bone_color = RasterColor::new(220, 200, 50); // Yellow
    let selected_color = RasterColor::new(50, 255, 100); // Bright green
    let joint_color = RasterColor::new(255, 150, 50); // Orange

    for (bone_idx, bone) in rig.skeleton.iter().enumerate() {
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

/// Draw selected rig parts with vertex/edge highlights
fn draw_rig_part_selection(
    fb: &mut Framebuffer,
    rig: &RiggedModel,
    camera: &crate::rasterizer::Camera,
    part_matrices: &[[[f32; 4]; 4]],
    selected_parts: &[usize],
) {
    let vertex_color = RasterColor::new(50, 255, 100);  // Bright green
    let edge_color = RasterColor::new(100, 200, 80);    // Slightly darker green

    for &part_idx in selected_parts {
        if let Some(part) = rig.parts.get(part_idx) {
            let world_mat = part_matrices.get(part_idx).copied().unwrap_or(mat4_identity());

            // Draw edges of selected part
            for face in &part.mesh.faces {
                let v0 = mat4_transform_point(&world_mat, part.mesh.vertices[face.v0].pos);
                let v1 = mat4_transform_point(&world_mat, part.mesh.vertices[face.v1].pos);
                let v2 = mat4_transform_point(&world_mat, part.mesh.vertices[face.v2].pos);

                draw_3d_line(fb, v0, v1, camera, edge_color);
                draw_3d_line(fb, v1, v2, camera, edge_color);
                draw_3d_line(fb, v2, v0, camera, edge_color);
            }

            // Draw vertex markers
            for vert in &part.mesh.vertices {
                let world_pos = mat4_transform_point(&world_mat, vert.pos);
                if let Some((sx, sy)) = world_to_screen(
                    world_pos,
                    camera.position,
                    camera.basis_x,
                    camera.basis_y,
                    camera.basis_z,
                    fb.width,
                    fb.height,
                ) {
                    let sx = sx as i32;
                    let sy = sy as i32;
                    // Draw small cross at vertex
                    fb.draw_line(sx - 2, sy, sx + 2, sy, vertex_color);
                    fb.draw_line(sx, sy - 2, sx, sy + 2, vertex_color);
                }
            }
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

/// Draw mesh vertex/edge/face overlays for spine mesh editing
/// Note: Spine mesh editing was simplified in the refactor - this is now a placeholder
fn draw_spine_mesh_overlays(
    _fb: &mut Framebuffer,
    _spine_model: &crate::modeler::SpineModel,
    _camera: &crate::rasterizer::Camera,
    _selection: &ModelerSelection,
    _select_mode: SelectMode,
) {
    // Spine mesh vertex/edge/face editing is deferred to later phase
    // For now, spine editing focuses on joints and bones only
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
        // Use draw_line (no depth test) so bones/gizmos render on top of geometry
        fb.draw_line(x0f as i32, y0f as i32, x1f as i32, y1f as i32, color);
    }
}

/// Check if a world-space point is visible (not occluded by geometry in the zbuffer)
/// Returns Some((sx, sy)) if visible, None if occluded or behind camera
fn is_point_visible(
    fb: &Framebuffer,
    world_pos: Vec3,
    camera_pos: Vec3,
    basis_x: Vec3,
    basis_y: Vec3,
    basis_z: Vec3,
) -> Option<(f32, f32)> {
    // Small bias to prevent z-fighting (vertices are exactly on the mesh surface)
    const DEPTH_BIAS: f32 = 0.05;

    let Some((sx, sy, depth)) = world_to_screen_with_depth(
        world_pos, camera_pos, basis_x, basis_y, basis_z, fb.width, fb.height
    ) else {
        return None;
    };

    // Check if the screen position is within bounds
    let ix = sx as i32;
    let iy = sy as i32;
    if ix < 0 || iy < 0 || ix >= fb.width as i32 || iy >= fb.height as i32 {
        return None;
    }

    // Check zbuffer - if our depth (with bias) is less than the stored depth, we're visible
    let idx = iy as usize * fb.width + ix as usize;
    let zbuffer_depth = fb.zbuffer[idx];

    if depth - DEPTH_BIAS <= zbuffer_depth {
        Some((sx, sy))
    } else {
        None
    }
}

/// Draw selection overlays for editable mesh
/// Note: Legacy model selection overlays removed in refactor - this now handles editable mesh
fn draw_mesh_selection_overlays(
    fb: &mut Framebuffer,
    state: &ModelerState,
) {
    let Some(mesh) = &state.editable_mesh else {
        return;
    };

    let vertex_color = RasterColor::new(100, 100, 255);
    let edge_color = RasterColor::new(80, 180, 255);
    let face_color = RasterColor::new(255, 200, 100);
    let selected_color = RasterColor::new(255, 150, 50);

    let selected_verts = state.selection.mesh_vertices().unwrap_or(&[]);
    let selected_edges = state.selection.mesh_edges().unwrap_or(&[]);
    let selected_faces = state.selection.mesh_faces().unwrap_or(&[]);

    // Draw based on current select mode
    match state.select_mode {
        SelectMode::Vertex => {
            // Draw vertices as dots (only if visible)
            for (vert_idx, vert) in mesh.vertices.iter().enumerate() {
                if let Some((sx, sy)) = is_point_visible(
                    fb,
                    vert.pos,
                    state.camera.position,
                    state.camera.basis_x,
                    state.camera.basis_y,
                    state.camera.basis_z,
                ) {
                    let is_selected = selected_verts.contains(&vert_idx);
                    let color = if is_selected { selected_color } else { vertex_color };
                    let radius = if is_selected { 4 } else { 2 };
                    fb.draw_circle(sx as i32, sy as i32, radius, color);
                }
            }
        }

        SelectMode::Edge => {
            // Collect unique edges from faces
            let mut edges: Vec<(usize, usize)> = Vec::new();
            for face in &mesh.faces {
                let e0 = if face.v0 < face.v1 { (face.v0, face.v1) } else { (face.v1, face.v0) };
                let e1 = if face.v1 < face.v2 { (face.v1, face.v2) } else { (face.v2, face.v1) };
                let e2 = if face.v2 < face.v0 { (face.v2, face.v0) } else { (face.v0, face.v2) };
                if !edges.contains(&e0) { edges.push(e0); }
                if !edges.contains(&e1) { edges.push(e1); }
                if !edges.contains(&e2) { edges.push(e2); }
            }

            // Draw edges (check midpoint visibility for each edge)
            for &(v0, v1) in &edges {
                let p0 = mesh.vertices.get(v0).map(|v| v.pos);
                let p1 = mesh.vertices.get(v1).map(|v| v.pos);
                if let (Some(pos0), Some(pos1)) = (p0, p1) {
                    // Check if midpoint is visible
                    let midpoint = (pos0 + pos1) * 0.5;
                    if let Some((mid_sx, mid_sy)) = is_point_visible(
                        fb,
                        midpoint,
                        state.camera.position,
                        state.camera.basis_x,
                        state.camera.basis_y,
                        state.camera.basis_z,
                    ) {
                        let s0 = world_to_screen(pos0, state.camera.position, state.camera.basis_x, state.camera.basis_y, state.camera.basis_z, fb.width, fb.height);
                        let s1 = world_to_screen(pos1, state.camera.position, state.camera.basis_x, state.camera.basis_y, state.camera.basis_z, fb.width, fb.height);
                        if let (Some((sx0, sy0)), Some((sx1, sy1))) = (s0, s1) {
                            let is_selected = selected_edges.contains(&(v0, v1));
                            let color = if is_selected { selected_color } else { edge_color };
                            fb.draw_line(sx0 as i32, sy0 as i32, sx1 as i32, sy1 as i32, color);

                            // Draw midpoint dot for easier clicking
                            let radius = if is_selected { 4 } else { 2 };
                            fb.draw_circle(mid_sx as i32, mid_sy as i32, radius, color);
                        }
                    }
                }
            }
        }

        SelectMode::Face => {
            // Draw face centroids as dots (only if visible)
            for (face_idx, face) in mesh.faces.iter().enumerate() {
                let p0 = mesh.vertices.get(face.v0).map(|v| v.pos);
                let p1 = mesh.vertices.get(face.v1).map(|v| v.pos);
                let p2 = mesh.vertices.get(face.v2).map(|v| v.pos);
                if let (Some(pos0), Some(pos1), Some(pos2)) = (p0, p1, p2) {
                    let centroid = (pos0 + pos1 + pos2) * (1.0 / 3.0);
                    if let Some((sx, sy)) = is_point_visible(
                        fb,
                        centroid,
                        state.camera.position,
                        state.camera.basis_x,
                        state.camera.basis_y,
                        state.camera.basis_z,
                    ) {
                        let is_selected = selected_faces.contains(&face_idx);
                        let color = if is_selected { selected_color } else { face_color };
                        let radius = if is_selected { 5 } else { 3 };
                        fb.draw_circle(sx as i32, sy as i32, radius, color);

                        // Also highlight face edges when selected
                        if is_selected {
                            let s0 = world_to_screen(pos0, state.camera.position, state.camera.basis_x, state.camera.basis_y, state.camera.basis_z, fb.width, fb.height);
                            let s1 = world_to_screen(pos1, state.camera.position, state.camera.basis_x, state.camera.basis_y, state.camera.basis_z, fb.width, fb.height);
                            let s2 = world_to_screen(pos2, state.camera.position, state.camera.basis_x, state.camera.basis_y, state.camera.basis_z, fb.width, fb.height);
                            if let (Some((sx0, sy0)), Some((sx1, sy1)), Some((sx2, sy2))) = (s0, s1, s2) {
                                fb.draw_line(sx0 as i32, sy0 as i32, sx1 as i32, sy1 as i32, selected_color);
                                fb.draw_line(sx1 as i32, sy1 as i32, sx2 as i32, sy2 as i32, selected_color);
                                fb.draw_line(sx2 as i32, sy2 as i32, sx0 as i32, sy0 as i32, selected_color);
                            }
                        }
                    }
                }
            }
        }

        _ => {
            // For other select modes (spine-related), still show vertices with depth test
            for (vert_idx, vert) in mesh.vertices.iter().enumerate() {
                if let Some((sx, sy)) = is_point_visible(
                    fb,
                    vert.pos,
                    state.camera.position,
                    state.camera.basis_x,
                    state.camera.basis_y,
                    state.camera.basis_z,
                ) {
                    let is_selected = selected_verts.contains(&vert_idx);
                    let color = if is_selected { selected_color } else { vertex_color };
                    let radius = if is_selected { 4 } else { 2 };
                    fb.draw_circle(sx as i32, sy as i32, radius, color);
                }
            }
        }
    }
}

/// Handle spine joint/bone/mesh-vertex dragging (move selected elements with mouse)
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
    // Check if we have spine joints, bones, or mesh elements selected
    let has_joint_selection = matches!(&state.selection, ModelerSelection::SpineJoints(j) if !j.is_empty());
    let has_bone_selection = matches!(&state.selection, ModelerSelection::SpineBones(b) if !b.is_empty());
    let has_mesh_vertex_selection = matches!(&state.selection, ModelerSelection::MeshVertices(v) if !v.is_empty());
    let has_mesh_edge_selection = matches!(&state.selection, ModelerSelection::MeshEdges(e) if !e.is_empty());
    let has_mesh_face_selection = matches!(&state.selection, ModelerSelection::MeshFaces(f) if !f.is_empty());

    if !has_joint_selection && !has_bone_selection && !has_mesh_vertex_selection && !has_mesh_edge_selection && !has_mesh_face_selection {
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
        // Store starting positions for mesh vertices
        else if let ModelerSelection::MeshVertices(verts) = &state.selection {
            let mut start_positions = Vec::new();
            if let Some(mesh) = &state.editable_mesh {
                for &vert_idx in verts {
                    if let Some(vert) = mesh.vertices.get(vert_idx) {
                        start_positions.push(vert.pos);
                    }
                }
            }

            if !start_positions.is_empty() {
                state.spine_drag_start_mouse = mouse_pos;
                state.spine_drag_start_positions = start_positions;
                state.spine_drag_handle = state.gizmo_hover_handle;
            }
        }
        // Store starting positions for mesh edges (both vertices of each edge)
        else if let ModelerSelection::MeshEdges(edges) = &state.selection {
            let mut start_positions = Vec::new();
            if let Some(mesh) = &state.editable_mesh {
                for &(v0, v1) in edges {
                    if let Some(vert0) = mesh.vertices.get(v0) {
                        start_positions.push(vert0.pos);
                    }
                    if let Some(vert1) = mesh.vertices.get(v1) {
                        start_positions.push(vert1.pos);
                    }
                }
            }

            if !start_positions.is_empty() {
                state.spine_drag_start_mouse = mouse_pos;
                state.spine_drag_start_positions = start_positions;
                state.spine_drag_handle = state.gizmo_hover_handle;
            }
        }
        // Store starting positions for mesh faces (all 3 vertices of each face)
        else if let ModelerSelection::MeshFaces(faces) = &state.selection {
            let mut start_positions = Vec::new();
            if let Some(mesh) = &state.editable_mesh {
                for &face_idx in faces {
                    if let Some(face) = mesh.faces.get(face_idx) {
                        if let Some(v0) = mesh.vertices.get(face.v0) {
                            start_positions.push(v0.pos);
                        }
                        if let Some(v1) = mesh.vertices.get(face.v1) {
                            start_positions.push(v1.pos);
                        }
                        if let Some(v2) = mesh.vertices.get(face.v2) {
                            start_positions.push(v2.pos);
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
        // Update mesh vertex positions
        else if let ModelerSelection::MeshVertices(verts) = &state.selection {
            let verts = verts.clone();
            if let Some(mesh) = &mut state.editable_mesh {
                for (i, vert_idx) in verts.iter().enumerate() {
                    if let Some(vert) = mesh.vertices.get_mut(*vert_idx) {
                        if let Some(start_pos) = state.spine_drag_start_positions.get(i) {
                            if state.snap_settings.enabled {
                                vert.pos = state.snap_settings.snap_vec3(*start_pos + world_delta);
                            } else {
                                vert.pos = *start_pos + snapped_delta;
                            }
                        }
                    }
                }
            }
        }
        // Update mesh edge positions (both vertices of each edge)
        else if let ModelerSelection::MeshEdges(edges) = &state.selection {
            let edges = edges.clone();
            if let Some(mesh) = &mut state.editable_mesh {
                for (edge_i, (v0, v1)) in edges.iter().enumerate() {
                    let pos_idx_0 = edge_i * 2;
                    let pos_idx_1 = edge_i * 2 + 1;

                    if let Some(vert0) = mesh.vertices.get_mut(*v0) {
                        if let Some(start_pos) = state.spine_drag_start_positions.get(pos_idx_0) {
                            if state.snap_settings.enabled {
                                vert0.pos = state.snap_settings.snap_vec3(*start_pos + world_delta);
                            } else {
                                vert0.pos = *start_pos + snapped_delta;
                            }
                        }
                    }
                    if let Some(vert1) = mesh.vertices.get_mut(*v1) {
                        if let Some(start_pos) = state.spine_drag_start_positions.get(pos_idx_1) {
                            if state.snap_settings.enabled {
                                vert1.pos = state.snap_settings.snap_vec3(*start_pos + world_delta);
                            } else {
                                vert1.pos = *start_pos + snapped_delta;
                            }
                        }
                    }
                }
            }
        }
        // Update mesh face positions (all 3 vertices of each face)
        else if let ModelerSelection::MeshFaces(faces) = &state.selection {
            let faces_sel = faces.clone();
            if let Some(mesh) = &mut state.editable_mesh {
                for (face_i, face_idx) in faces_sel.iter().enumerate() {
                    // Get face vertex indices first
                    let face_verts = mesh.faces.get(*face_idx).map(|f| (f.v0, f.v1, f.v2));

                    if let Some((v0, v1, v2)) = face_verts {
                        let pos_idx_0 = face_i * 3;
                        let pos_idx_1 = face_i * 3 + 1;
                        let pos_idx_2 = face_i * 3 + 2;

                        if let Some(vert) = mesh.vertices.get_mut(v0) {
                            if let Some(start_pos) = state.spine_drag_start_positions.get(pos_idx_0) {
                                if state.snap_settings.enabled {
                                    vert.pos = state.snap_settings.snap_vec3(*start_pos + world_delta);
                                } else {
                                    vert.pos = *start_pos + snapped_delta;
                                }
                            }
                        }
                        if let Some(vert) = mesh.vertices.get_mut(v1) {
                            if let Some(start_pos) = state.spine_drag_start_positions.get(pos_idx_1) {
                                if state.snap_settings.enabled {
                                    vert.pos = state.snap_settings.snap_vec3(*start_pos + world_delta);
                                } else {
                                    vert.pos = *start_pos + snapped_delta;
                                }
                            }
                        }
                        if let Some(vert) = mesh.vertices.get_mut(v2) {
                            if let Some(start_pos) = state.spine_drag_start_positions.get(pos_idx_2) {
                                if state.snap_settings.enabled {
                                    vert.pos = state.snap_settings.snap_vec3(*start_pos + world_delta);
                                } else {
                                    vert.pos = *start_pos + snapped_delta;
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
            } else if matches!(&state.selection, ModelerSelection::MeshVertices(_)) {
                "Vertex moved"
            } else if matches!(&state.selection, ModelerSelection::MeshEdges(_)) {
                "Edge moved"
            } else if matches!(&state.selection, ModelerSelection::MeshFaces(_)) {
                "Face moved"
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
        // Restore original positions for mesh vertices
        else if let ModelerSelection::MeshVertices(verts) = &state.selection {
            let verts = verts.clone();
            if let Some(mesh) = &mut state.editable_mesh {
                for (i, vert_idx) in verts.iter().enumerate() {
                    if let Some(vert) = mesh.vertices.get_mut(*vert_idx) {
                        if let Some(start_pos) = state.spine_drag_start_positions.get(i) {
                            vert.pos = *start_pos;
                        }
                    }
                }
            }
        }
        state.spine_drag_active = false;
        state.spine_drag_handle = None;
        state.set_status("Move cancelled", 0.5);
    }
    // Clear selection on Escape (when not dragging)
    else if is_key_pressed(KeyCode::Escape) && !state.selection.is_empty() {
        state.selection = ModelerSelection::None;
        state.set_status("Selection cleared", 0.5);
    }
}

/// Handle mesh separation (P key) - split selected faces into new MeshPart
/// Creates a RiggedModel if one doesn't exist
fn handle_mesh_separate(state: &mut ModelerState) {
    use super::mesh_editor::EditableMesh;
    use super::state::{RiggedModel, MeshPart, RigSubMode};
    use std::collections::{HashMap, HashSet};

    // Get selected faces
    let selected_faces: Vec<usize> = match &state.selection {
        ModelerSelection::MeshFaces(faces) => faces.clone(),
        _ => {
            state.set_status("Select faces first (press 3 for face mode)", 1.5);
            return;
        }
    };

    if selected_faces.is_empty() {
        state.set_status("No faces selected to separate", 1.0);
        return;
    }

    // Get the editable mesh
    let Some(mesh) = state.editable_mesh.take() else {
        state.set_status("No mesh to separate", 1.0);
        return;
    };

    // Check that all selected face indices are valid
    for &face_idx in &selected_faces {
        if face_idx >= mesh.faces.len() {
            state.editable_mesh = Some(mesh);
            state.set_status("Invalid face selection", 1.0);
            return;
        }
    }

    let selected_set: HashSet<usize> = selected_faces.iter().copied().collect();

    // Collect vertices used by selected faces
    let mut used_vertices: HashSet<usize> = HashSet::new();
    for &face_idx in &selected_faces {
        let face = &mesh.faces[face_idx];
        used_vertices.insert(face.v0);
        used_vertices.insert(face.v1);
        used_vertices.insert(face.v2);
    }

    // Create vertex remapping for separated mesh
    let mut old_to_new: HashMap<usize, usize> = HashMap::new();
    let mut new_vertices = Vec::new();
    for &old_idx in &used_vertices {
        old_to_new.insert(old_idx, new_vertices.len());
        new_vertices.push(mesh.vertices[old_idx].clone());
    }

    // Create new faces with remapped indices
    let mut new_faces = Vec::new();
    for &face_idx in &selected_faces {
        let old_face = &mesh.faces[face_idx];
        new_faces.push(crate::rasterizer::Face::new(
            old_to_new[&old_face.v0],
            old_to_new[&old_face.v1],
            old_to_new[&old_face.v2],
        ));
    }

    // Create separated mesh
    let separated_mesh = EditableMesh::from_parts(new_vertices, new_faces);

    // Create remaining mesh (faces not selected)
    let remaining_faces: Vec<_> = mesh.faces.iter()
        .enumerate()
        .filter(|(i, _)| !selected_set.contains(i))
        .map(|(_, f)| f.clone())
        .collect();

    // Collect vertices still in use by remaining faces
    let mut remaining_vertices_used: HashSet<usize> = HashSet::new();
    for face in &remaining_faces {
        remaining_vertices_used.insert(face.v0);
        remaining_vertices_used.insert(face.v1);
        remaining_vertices_used.insert(face.v2);
    }

    // Remap remaining mesh vertices
    let mut remaining_old_to_new: HashMap<usize, usize> = HashMap::new();
    let mut remaining_vertices = Vec::new();
    for (old_idx, vert) in mesh.vertices.iter().enumerate() {
        if remaining_vertices_used.contains(&old_idx) {
            remaining_old_to_new.insert(old_idx, remaining_vertices.len());
            remaining_vertices.push(vert.clone());
        }
    }

    // Remap remaining faces
    let remaining_faces: Vec<_> = remaining_faces.iter()
        .map(|f| crate::rasterizer::Face::new(
            remaining_old_to_new[&f.v0],
            remaining_old_to_new[&f.v1],
            remaining_old_to_new[&f.v2],
        ))
        .collect();

    let remaining_mesh = EditableMesh::from_parts(remaining_vertices, remaining_faces);

    // Create RiggedModel with two parts
    let mut rig = RiggedModel::new("separated_model");

    // Add remaining mesh as first part (if any faces remain)
    if !remaining_mesh.faces.is_empty() {
        rig.parts.push(MeshPart {
            name: "base".to_string(),
            bone_index: None,
            mesh: remaining_mesh,
            pivot: Vec3::ZERO,
        });
    }

    // Add separated mesh as second part
    rig.parts.push(MeshPart {
        name: format!("part_{}", rig.parts.len()),
        bone_index: None,
        mesh: separated_mesh,
        pivot: Vec3::ZERO,
    });

    // Switch to Rig context with Parts sub-mode
    state.rigged_model = Some(rig);
    state.editable_mesh = None;
    state.data_context = DataContext::Rig;
    state.rig_sub_mode = RigSubMode::Parts;
    state.selection = ModelerSelection::RigParts(vec![state.rigged_model.as_ref().unwrap().parts.len() - 1]);

    let num_parts = state.rigged_model.as_ref().unwrap().parts.len();
    state.set_status(&format!("Separated {} faces into {} parts", selected_faces.len(), num_parts), 2.0);
}

/// Handle rig bone extrusion (E key) - create child bone from selected
fn handle_rig_bone_extrude(state: &mut ModelerState) {
    use super::state::RigBone;

    let selected_bones = match &state.selection {
        ModelerSelection::RigBones(bones) => bones.clone(),
        _ => {
            state.set_status("Select a bone first", 1.0);
            return;
        }
    };

    if selected_bones.is_empty() {
        state.set_status("No bone selected to extrude from", 1.0);
        return;
    }

    let Some(rig) = &mut state.rigged_model else {
        state.set_status("No rigged model", 1.0);
        return;
    };

    // Extrude from first selected bone
    let parent_idx = selected_bones[0];

    // Copy needed data before mutable borrow
    let parent_length = rig.skeleton[parent_idx].length;
    let parent_name = rig.skeleton[parent_idx].name.clone();
    let bone_count = rig.skeleton.len();

    // New bone extends from parent's tip (along Y in local space)
    let new_bone = RigBone {
        name: format!("Bone.{}", bone_count),
        parent: Some(parent_idx),
        local_position: Vec3::new(0.0, parent_length, 0.0), // At parent's tip
        local_rotation: Vec3::ZERO,
        length: 20.0,
    };

    let new_idx = rig.add_bone(new_bone);

    // Select the new bone
    state.selection = ModelerSelection::RigBones(vec![new_idx]);
    state.set_status(&format!("Extruded bone {} from {}", new_idx, parent_name), 1.0);
}

/// Handle rig bone deletion (X key)
fn handle_rig_bone_delete(state: &mut ModelerState) {
    let selected_bones = match &state.selection {
        ModelerSelection::RigBones(bones) => bones.clone(),
        _ => {
            state.set_status("Select bones to delete", 1.0);
            return;
        }
    };

    if selected_bones.is_empty() {
        state.set_status("No bones selected to delete", 1.0);
        return;
    }

    let Some(rig) = &mut state.rigged_model else {
        state.set_status("No rigged model", 1.0);
        return;
    };

    // Check if any parts are assigned to these bones
    for &bone_idx in &selected_bones {
        for part in &rig.parts {
            if part.bone_index == Some(bone_idx) {
                state.set_status(&format!("Cannot delete bone {} - parts are assigned to it", bone_idx), 1.5);
                return;
            }
        }
    }

    // Check if any other bones are children of these bones
    for &bone_idx in &selected_bones {
        for (i, bone) in rig.skeleton.iter().enumerate() {
            if bone.parent == Some(bone_idx) && !selected_bones.contains(&i) {
                state.set_status(&format!("Cannot delete bone {} - has children", bone_idx), 1.5);
                return;
            }
        }
    }

    // Delete bones in reverse order to maintain indices
    let mut to_delete: Vec<_> = selected_bones.iter().copied().collect();
    to_delete.sort_by(|a, b| b.cmp(a)); // Reverse order

    for bone_idx in to_delete {
        // Update parent references for bones after this one
        for bone in &mut rig.skeleton {
            if let Some(parent) = bone.parent {
                if parent > bone_idx {
                    bone.parent = Some(parent - 1);
                } else if parent == bone_idx {
                    bone.parent = None; // Should not happen due to earlier check
                }
            }
        }

        // Update part bone assignments
        for part in &mut rig.parts {
            if let Some(idx) = part.bone_index {
                if idx > bone_idx {
                    part.bone_index = Some(idx - 1);
                }
            }
        }

        rig.skeleton.remove(bone_idx);
    }

    state.selection = ModelerSelection::None;
    state.set_status(&format!("Deleted {} bone(s)", selected_bones.len()), 1.0);
}

/// Handle adding a new root bone (N key)
fn handle_rig_add_root_bone(state: &mut ModelerState) {
    use super::state::RigBone;

    let Some(rig) = &mut state.rigged_model else {
        state.set_status("No rigged model", 1.0);
        return;
    };

    let new_bone = RigBone {
        name: format!("Bone.{}", rig.skeleton.len()),
        parent: None,
        local_position: Vec3::new(0.0, 0.0, 0.0),
        local_rotation: Vec3::ZERO,
        length: 30.0,
    };

    let new_idx = rig.add_bone(new_bone);
    state.selection = ModelerSelection::RigBones(vec![new_idx]);
    state.set_status(&format!("Added root bone {}", new_idx), 1.0);
}

/// Handle assigning a part to a specific bone (Ctrl+1-9 in Parts sub-mode)
fn handle_rig_assign_part_to_bone(state: &mut ModelerState, bone_idx: usize) {
    let selected_parts = match &state.selection {
        ModelerSelection::RigParts(parts) => parts.clone(),
        _ => {
            state.set_status("Select a part first (in Parts mode)", 1.0);
            return;
        }
    };

    if selected_parts.is_empty() {
        state.set_status("No part selected to assign", 1.0);
        return;
    }

    // Check rigged model exists and has bones
    let (_bone_count, bone_name) = {
        let Some(rig) = &state.rigged_model else {
            state.set_status("No rigged model", 1.0);
            return;
        };

        if rig.skeleton.is_empty() {
            state.set_status("Create bones first (Shift+1 for Skeleton mode, N for new bone)", 1.5);
            return;
        }

        if bone_idx >= rig.skeleton.len() {
            state.set_status(&format!("Bone {} doesn't exist (only {} bones)", bone_idx, rig.skeleton.len()), 1.0);
            return;
        }

        (rig.skeleton.len(), rig.skeleton[bone_idx].name.clone())
    };

    // Assign all selected parts to this bone
    let Some(rig) = &mut state.rigged_model else { return; };
    for &part_idx in &selected_parts {
        if part_idx < rig.parts.len() {
            rig.parts[part_idx].bone_index = Some(bone_idx);
        }
    }

    state.set_status(&format!("Assigned {} part(s) to bone '{}'", selected_parts.len(), bone_name), 1.5);
}

/// Handle unassigning a part from its bone (Alt+P in Parts sub-mode)
fn handle_rig_unassign_part(state: &mut ModelerState) {
    let selected_parts = match &state.selection {
        ModelerSelection::RigParts(parts) => parts.clone(),
        _ => {
            state.set_status("Select a part to unassign", 1.0);
            return;
        }
    };

    if selected_parts.is_empty() {
        state.set_status("No part selected", 1.0);
        return;
    }

    let Some(rig) = &mut state.rigged_model else {
        state.set_status("No rigged model", 1.0);
        return;
    };

    for &part_idx in &selected_parts {
        if part_idx < rig.parts.len() {
            rig.parts[part_idx].bone_index = None;
        }
    }

    state.set_status(&format!("Unassigned {} part(s) from bones", selected_parts.len()), 1.0);
}

/// Handle editable mesh selection on click
/// Supports multi-select with Shift key
/// Routes to vertex/edge/face selection based on state.select_mode
fn handle_mesh_selection_click<F>(
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

    let Some(mesh) = &state.editable_mesh else {
        return;
    };

    let shift_held = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);

    match state.select_mode {
        SelectMode::Vertex => {
            // Find closest vertex to click position
            let mut closest_vert: Option<(usize, f32)> = None;
            let threshold = 10.0;

            for (vert_idx, vert) in mesh.vertices.iter().enumerate() {
                if let Some((sx, sy)) = world_to_screen(
                    vert.pos,
                    state.camera.position,
                    state.camera.basis_x,
                    state.camera.basis_y,
                    state.camera.basis_z,
                    fb_width,
                    fb_height,
                ) {
                    let dist = ((sx - fb_x).powi(2) + (sy - fb_y).powi(2)).sqrt();
                    if dist < threshold {
                        if let Some((_, best_dist)) = closest_vert {
                            if dist < best_dist {
                                closest_vert = Some((vert_idx, dist));
                            }
                        } else {
                            closest_vert = Some((vert_idx, dist));
                        }
                    }
                }
            }

            if let Some((vert_idx, _)) = closest_vert {
                if shift_held {
                    let mut current = state.selection.mesh_vertices().map(|v| v.to_vec()).unwrap_or_default();
                    if !current.contains(&vert_idx) {
                        current.push(vert_idx);
                    }
                    state.selection = ModelerSelection::MeshVertices(current);
                } else {
                    state.selection = ModelerSelection::MeshVertices(vec![vert_idx]);
                }
                state.set_status(&format!("Selected vertex {}", vert_idx), 0.5);
            } else if !shift_held {
                state.selection = ModelerSelection::None;
            }
        }

        SelectMode::Edge => {
            // Find closest edge to click position (by checking distance to edge midpoint and line)
            let mut closest_edge: Option<((usize, usize), f32)> = None;
            let threshold = 12.0;

            // Collect unique edges from faces
            let mut edges: Vec<(usize, usize)> = Vec::new();
            for face in &mesh.faces {
                let e0 = if face.v0 < face.v1 { (face.v0, face.v1) } else { (face.v1, face.v0) };
                let e1 = if face.v1 < face.v2 { (face.v1, face.v2) } else { (face.v2, face.v1) };
                let e2 = if face.v2 < face.v0 { (face.v2, face.v0) } else { (face.v0, face.v2) };
                if !edges.contains(&e0) { edges.push(e0); }
                if !edges.contains(&e1) { edges.push(e1); }
                if !edges.contains(&e2) { edges.push(e2); }
            }

            for &(v0, v1) in &edges {
                let p0 = mesh.vertices.get(v0).map(|v| v.pos);
                let p1 = mesh.vertices.get(v1).map(|v| v.pos);
                if let (Some(pos0), Some(pos1)) = (p0, p1) {
                    // Project both endpoints to screen
                    let s0 = world_to_screen(pos0, state.camera.position, state.camera.basis_x, state.camera.basis_y, state.camera.basis_z, fb_width, fb_height);
                    let s1 = world_to_screen(pos1, state.camera.position, state.camera.basis_x, state.camera.basis_y, state.camera.basis_z, fb_width, fb_height);
                    if let (Some((sx0, sy0)), Some((sx1, sy1))) = (s0, s1) {
                        // Distance from click to line segment
                        let dist = point_to_segment_distance(fb_x, fb_y, sx0, sy0, sx1, sy1);
                        if dist < threshold {
                            if let Some((_, best_dist)) = closest_edge {
                                if dist < best_dist {
                                    closest_edge = Some(((v0, v1), dist));
                                }
                            } else {
                                closest_edge = Some(((v0, v1), dist));
                            }
                        }
                    }
                }
            }

            if let Some(((v0, v1), _)) = closest_edge {
                if shift_held {
                    let mut current = state.selection.mesh_edges().map(|e| e.to_vec()).unwrap_or_default();
                    if !current.contains(&(v0, v1)) {
                        current.push((v0, v1));
                    }
                    state.selection = ModelerSelection::MeshEdges(current);
                } else {
                    state.selection = ModelerSelection::MeshEdges(vec![(v0, v1)]);
                }
                state.set_status(&format!("Selected edge ({}, {})", v0, v1), 0.5);
            } else if !shift_held {
                state.selection = ModelerSelection::None;
            }
        }

        SelectMode::Face => {
            // Find closest face to click position (by checking distance to face centroid)
            let mut closest_face: Option<(usize, f32)> = None;
            let threshold = 20.0;

            for (face_idx, face) in mesh.faces.iter().enumerate() {
                let p0 = mesh.vertices.get(face.v0).map(|v| v.pos);
                let p1 = mesh.vertices.get(face.v1).map(|v| v.pos);
                let p2 = mesh.vertices.get(face.v2).map(|v| v.pos);
                if let (Some(pos0), Some(pos1), Some(pos2)) = (p0, p1, p2) {
                    let centroid = (pos0 + pos1 + pos2) * (1.0 / 3.0);
                    if let Some((sx, sy)) = world_to_screen(
                        centroid,
                        state.camera.position,
                        state.camera.basis_x,
                        state.camera.basis_y,
                        state.camera.basis_z,
                        fb_width,
                        fb_height,
                    ) {
                        let dist = ((sx - fb_x).powi(2) + (sy - fb_y).powi(2)).sqrt();
                        if dist < threshold {
                            if let Some((_, best_dist)) = closest_face {
                                if dist < best_dist {
                                    closest_face = Some((face_idx, dist));
                                }
                            } else {
                                closest_face = Some((face_idx, dist));
                            }
                        }
                    }
                }
            }

            if let Some((face_idx, _)) = closest_face {
                if shift_held {
                    let mut current = state.selection.mesh_faces().map(|f| f.to_vec()).unwrap_or_default();
                    if !current.contains(&face_idx) {
                        current.push(face_idx);
                    }
                    state.selection = ModelerSelection::MeshFaces(current);
                } else {
                    state.selection = ModelerSelection::MeshFaces(vec![face_idx]);
                }
                state.set_status(&format!("Selected face {}", face_idx), 0.5);
            } else if !shift_held {
                state.selection = ModelerSelection::None;
            }
        }

        _ => {
            // Other select modes (Joint, SpineBone, Segment, Part, RigBone) don't apply to editable mesh
        }
    }
}

/// Calculate distance from point (px, py) to line segment (x0, y0) -> (x1, y1)
fn point_to_segment_distance(px: f32, py: f32, x0: f32, y0: f32, x1: f32, y1: f32) -> f32 {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len_sq = dx * dx + dy * dy;

    if len_sq < 0.0001 {
        // Degenerate segment (both endpoints same)
        return ((px - x0).powi(2) + (py - y0).powi(2)).sqrt();
    }

    // Parameter t is projection of point onto line, clamped to [0, 1]
    let t = ((px - x0) * dx + (py - y0) * dy) / len_sq;
    let t = t.clamp(0.0, 1.0);

    // Closest point on segment
    let closest_x = x0 + t * dx;
    let closest_y = y0 + t * dy;

    ((px - closest_x).powi(2) + (py - closest_y).powi(2)).sqrt()
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

/// Handle rig selection click (bones in Skeleton/Animate mode, parts in Parts mode)
fn handle_rig_selection_click<F>(
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

    let Some(rig) = &state.rigged_model else {
        return;
    };

    let shift_held = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);

    match state.rig_sub_mode {
        RigSubMode::Skeleton | RigSubMode::Animate => {
            // Select bones by clicking on their joint markers
            let bone_transforms = compute_rig_bone_world_transforms(rig);
            let threshold = 10.0;
            let mut closest_bone: Option<(usize, f32)> = None;

            for (bone_idx, bone) in rig.skeleton.iter().enumerate() {
                let world_mat = &bone_transforms[bone_idx];
                // Joint position is the bone origin in world space
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
                    let dist = ((sx - fb_x).powi(2) + (sy - fb_y).powi(2)).sqrt();
                    if dist < threshold {
                        if let Some((_, best_dist)) = closest_bone {
                            if dist < best_dist {
                                closest_bone = Some((bone_idx, dist));
                            }
                        } else {
                            closest_bone = Some((bone_idx, dist));
                        }
                    }
                }

                // Also check bone tip
                let tip_local = Vec3::new(0.0, bone.length, 0.0);
                let tip_pos = mat4_transform_point(world_mat, tip_local);

                if let Some((sx, sy)) = world_to_screen(
                    tip_pos,
                    state.camera.position,
                    state.camera.basis_x,
                    state.camera.basis_y,
                    state.camera.basis_z,
                    fb_width,
                    fb_height,
                ) {
                    let dist = ((sx - fb_x).powi(2) + (sy - fb_y).powi(2)).sqrt();
                    if dist < threshold {
                        if let Some((_, best_dist)) = closest_bone {
                            if dist < best_dist {
                                closest_bone = Some((bone_idx, dist));
                            }
                        } else {
                            closest_bone = Some((bone_idx, dist));
                        }
                    }
                }
            }

            if let Some((bone_idx, _)) = closest_bone {
                let bone_name = &rig.skeleton[bone_idx].name;
                if shift_held {
                    let mut current = state.selection.rig_bones().map(|b| b.to_vec()).unwrap_or_default();
                    if let Some(pos) = current.iter().position(|&b| b == bone_idx) {
                        current.remove(pos);
                        if current.is_empty() {
                            state.selection = ModelerSelection::None;
                            state.set_status("Selection cleared", 0.5);
                        } else {
                            state.selection = ModelerSelection::RigBones(current);
                            state.set_status(&format!("Deselected bone '{}'", bone_name), 0.5);
                        }
                    } else {
                        current.push(bone_idx);
                        state.selection = ModelerSelection::RigBones(current.clone());
                        state.set_status(&format!("{} bones selected", current.len()), 0.5);
                    }
                } else {
                    state.selection = ModelerSelection::RigBones(vec![bone_idx]);
                    state.set_status(&format!("Selected bone '{}'", bone_name), 0.5);
                }
            } else if !shift_held {
                state.selection = ModelerSelection::None;
            }
        }
        RigSubMode::Parts => {
            // Select parts by clicking near their vertices
            let part_matrices = compute_part_world_matrices(rig);
            let threshold = 12.0;
            let mut closest_part: Option<(usize, f32)> = None;

            for (part_idx, part) in rig.parts.iter().enumerate() {
                let world_mat = part_matrices.get(part_idx).copied().unwrap_or(mat4_identity());

                // Check vertices of this part
                for vert in &part.mesh.vertices {
                    let world_pos = mat4_transform_point(&world_mat, vert.pos);

                    if let Some((sx, sy)) = world_to_screen(
                        world_pos,
                        state.camera.position,
                        state.camera.basis_x,
                        state.camera.basis_y,
                        state.camera.basis_z,
                        fb_width,
                        fb_height,
                    ) {
                        let dist = ((sx - fb_x).powi(2) + (sy - fb_y).powi(2)).sqrt();
                        if dist < threshold {
                            if let Some((_, best_dist)) = closest_part {
                                if dist < best_dist {
                                    closest_part = Some((part_idx, dist));
                                }
                            } else {
                                closest_part = Some((part_idx, dist));
                            }
                        }
                    }
                }
            }

            if let Some((part_idx, _)) = closest_part {
                let part_name = &rig.parts[part_idx].name;
                if shift_held {
                    let mut current = state.selection.rig_parts().map(|p| p.to_vec()).unwrap_or_default();
                    if let Some(pos) = current.iter().position(|&p| p == part_idx) {
                        current.remove(pos);
                        if current.is_empty() {
                            state.selection = ModelerSelection::None;
                            state.set_status("Selection cleared", 0.5);
                        } else {
                            state.selection = ModelerSelection::RigParts(current);
                            state.set_status(&format!("Deselected part '{}'", part_name), 0.5);
                        }
                    } else {
                        current.push(part_idx);
                        state.selection = ModelerSelection::RigParts(current.clone());
                        state.set_status(&format!("{} parts selected", current.len()), 0.5);
                    }
                } else {
                    state.selection = ModelerSelection::RigParts(vec![part_idx]);
                    state.set_status(&format!("Selected part '{}'", part_name), 0.5);
                }
            } else if !shift_held {
                state.selection = ModelerSelection::None;
            }
        }
    }
}

/// Handle click selection in viewport - stub that delegates to context-specific handlers
fn handle_selection_click<F>(
    _ctx: &UiContext,
    state: &mut ModelerState,
    _world_matrices: &[[[f32; 4]; 4]],
    screen_to_fb: F,
    fb_width: usize,
    fb_height: usize,
) where F: Fn(f32, f32) -> Option<(f32, f32)>
{
    // Selection is now handled by context-specific functions
    match state.data_context {
        DataContext::Mesh => {
            handle_mesh_selection_click(state, screen_to_fb, fb_width, fb_height);
        }
        DataContext::Spine => {
            // Spine selection is handled by the main viewport event loop
            // (handle_spine_selection_click is called directly there)
        }
        DataContext::Rig => {
            handle_rig_selection_click(state, screen_to_fb, fb_width, fb_height);
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

/// Handle box selection for mesh elements
/// TODO: Implement context-aware box selection
fn handle_box_selection<F>(
    ctx: &UiContext,
    state: &mut ModelerState,
    inside_viewport: bool,
    mouse_pos: (f32, f32),
    screen_to_fb: F,
    fb_width: usize,
    fb_height: usize,
) where F: Fn(f32, f32) -> Option<(f32, f32)>
{
    // Only allow box selection in mesh editing modes
    let is_mesh_mode = matches!(state.select_mode, SelectMode::Vertex | SelectMode::Edge | SelectMode::Face);
    if !is_mesh_mode {
        state.box_select_active = false;
        return;
    }

    // Start box selection with B key
    if inside_viewport && is_key_pressed(KeyCode::B) && !state.spine_drag_active {
        state.box_select_active = true;
        state.box_select_start = mouse_pos;
        state.set_status("Box select started", 0.5);
        return;
    }

    // Update box selection during drag
    if state.box_select_active {
        // Cancel with Escape or right click
        if is_key_pressed(KeyCode::Escape) || ctx.mouse.right_pressed {
            state.box_select_active = false;
            state.set_status("Box select cancelled", 0.5);
            return;
        }

        // Complete box selection on left click
        if ctx.mouse.left_pressed {
            let Some((start_fb_x, start_fb_y)) = screen_to_fb(state.box_select_start.0, state.box_select_start.1) else {
                state.box_select_active = false;
                return;
            };
            let Some((end_fb_x, end_fb_y)) = screen_to_fb(mouse_pos.0, mouse_pos.1) else {
                state.box_select_active = false;
                return;
            };

            let min_x = start_fb_x.min(end_fb_x);
            let max_x = start_fb_x.max(end_fb_x);
            let min_y = start_fb_y.min(end_fb_y);
            let max_y = start_fb_y.max(end_fb_y);

            let shift_held = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);

            // Box selection for editable mesh (Mesh context)
            if let Some(mesh) = &state.editable_mesh {
                match state.select_mode {
                    SelectMode::Vertex => {
                        let mut selected_verts: Vec<usize> = if shift_held {
                            state.selection.mesh_vertices().map(|v| v.to_vec()).unwrap_or_default()
                        } else {
                            Vec::new()
                        };

                        for (vert_idx, vert) in mesh.vertices.iter().enumerate() {
                            if let Some((sx, sy)) = world_to_screen(
                                vert.pos,
                                state.camera.position,
                                state.camera.basis_x,
                                state.camera.basis_y,
                                state.camera.basis_z,
                                fb_width,
                                fb_height,
                            ) {
                                if sx >= min_x && sx <= max_x && sy >= min_y && sy <= max_y {
                                    if !selected_verts.contains(&vert_idx) {
                                        selected_verts.push(vert_idx);
                                    }
                                }
                            }
                        }

                        if !selected_verts.is_empty() {
                            state.selection = ModelerSelection::MeshVertices(selected_verts.clone());
                            state.set_status(&format!("Selected {} vertices", selected_verts.len()), 1.0);
                        } else if !shift_held {
                            state.selection = ModelerSelection::None;
                        }
                    }
                    SelectMode::Edge => {
                        let mut selected_edges: Vec<(usize, usize)> = if shift_held {
                            state.selection.mesh_edges().map(|e| e.to_vec()).unwrap_or_default()
                        } else {
                            Vec::new()
                        };

                        // Build unique edges from faces
                        let mut edges: Vec<(usize, usize)> = Vec::new();
                        for face in &mesh.faces {
                            let e1 = if face.v0 < face.v1 { (face.v0, face.v1) } else { (face.v1, face.v0) };
                            let e2 = if face.v1 < face.v2 { (face.v1, face.v2) } else { (face.v2, face.v1) };
                            let e3 = if face.v2 < face.v0 { (face.v2, face.v0) } else { (face.v0, face.v2) };
                            if !edges.contains(&e1) { edges.push(e1); }
                            if !edges.contains(&e2) { edges.push(e2); }
                            if !edges.contains(&e3) { edges.push(e3); }
                        }

                        for edge in &edges {
                            let mid = (mesh.vertices[edge.0].pos + mesh.vertices[edge.1].pos) * 0.5;
                            if let Some((sx, sy)) = world_to_screen(
                                mid,
                                state.camera.position,
                                state.camera.basis_x,
                                state.camera.basis_y,
                                state.camera.basis_z,
                                fb_width,
                                fb_height,
                            ) {
                                if sx >= min_x && sx <= max_x && sy >= min_y && sy <= max_y {
                                    if !selected_edges.contains(edge) {
                                        selected_edges.push(*edge);
                                    }
                                }
                            }
                        }

                        if !selected_edges.is_empty() {
                            state.selection = ModelerSelection::MeshEdges(selected_edges.clone());
                            state.set_status(&format!("Selected {} edges", selected_edges.len()), 1.0);
                        } else if !shift_held {
                            state.selection = ModelerSelection::None;
                        }
                    }
                    SelectMode::Face => {
                        let mut selected_faces: Vec<usize> = if shift_held {
                            state.selection.mesh_faces().map(|f| f.to_vec()).unwrap_or_default()
                        } else {
                            Vec::new()
                        };

                        for (face_idx, face) in mesh.faces.iter().enumerate() {
                            let center = (mesh.vertices[face.v0].pos + mesh.vertices[face.v1].pos + mesh.vertices[face.v2].pos) * (1.0 / 3.0);
                            if let Some((sx, sy)) = world_to_screen(
                                center,
                                state.camera.position,
                                state.camera.basis_x,
                                state.camera.basis_y,
                                state.camera.basis_z,
                                fb_width,
                                fb_height,
                            ) {
                                if sx >= min_x && sx <= max_x && sy >= min_y && sy <= max_y {
                                    if !selected_faces.contains(&face_idx) {
                                        selected_faces.push(face_idx);
                                    }
                                }
                            }
                        }

                        if !selected_faces.is_empty() {
                            state.selection = ModelerSelection::MeshFaces(selected_faces.clone());
                            state.set_status(&format!("Selected {} faces", selected_faces.len()), 1.0);
                        } else if !shift_held {
                            state.selection = ModelerSelection::None;
                        }
                    }
                    _ => {}
                }
            }

            state.box_select_active = false;
        }
    }
}

/// Handle face extrusion (E key in face mode)
/// Creates new geometry by duplicating selected faces and connecting them with side faces
fn handle_face_extrude(state: &mut ModelerState) {
    let Some(selected_faces) = state.selection.mesh_faces() else {
        state.set_status("No faces selected", 1.0);
        return;
    };

    if selected_faces.is_empty() {
        state.set_status("No faces selected", 1.0);
        return;
    }

    let selected_faces = selected_faces.to_vec();

    let Some(mesh) = &mut state.editable_mesh else {
        state.set_status("No mesh loaded", 1.0);
        return;
    };

    let mut new_face_indices: Vec<usize> = Vec::new();

    // For each selected face, extrude it
    for &face_idx in &selected_faces {
        let Some(face) = mesh.faces.get(face_idx).cloned() else {
            continue;
        };

        // Get the face vertices
        let v0 = mesh.vertices[face.v0].clone();
        let v1 = mesh.vertices[face.v1].clone();
        let v2 = mesh.vertices[face.v2].clone();

        // Calculate face normal for extrusion direction
        let edge1 = v1.pos - v0.pos;
        let edge2 = v2.pos - v0.pos;
        let normal = edge1.cross(edge2).normalize();

        // Extrude distance (small initial offset, user will drag to final position)
        let extrude_dist = 10.0;
        let offset = normal * extrude_dist;

        // Create new vertices (extruded copies)
        let new_v0_idx = mesh.vertices.len();
        let new_v1_idx = new_v0_idx + 1;
        let new_v2_idx = new_v0_idx + 2;

        mesh.vertices.push(crate::rasterizer::Vertex::new(v0.pos + offset, v0.uv, normal));
        mesh.vertices.push(crate::rasterizer::Vertex::new(v1.pos + offset, v1.uv, normal));
        mesh.vertices.push(crate::rasterizer::Vertex::new(v2.pos + offset, v2.uv, normal));

        // Update the original face to point to new vertices (this becomes the "top" face)
        mesh.faces[face_idx] = crate::rasterizer::Face::new(new_v0_idx, new_v1_idx, new_v2_idx);

        // Create side faces connecting old and new vertices
        // Side 1: v0 -> v1 -> new_v1 -> new_v0 (two triangles)
        mesh.faces.push(crate::rasterizer::Face::new(face.v0, face.v1, new_v1_idx));
        mesh.faces.push(crate::rasterizer::Face::new(face.v0, new_v1_idx, new_v0_idx));

        // Side 2: v1 -> v2 -> new_v2 -> new_v1
        mesh.faces.push(crate::rasterizer::Face::new(face.v1, face.v2, new_v2_idx));
        mesh.faces.push(crate::rasterizer::Face::new(face.v1, new_v2_idx, new_v1_idx));

        // Side 3: v2 -> v0 -> new_v0 -> new_v2
        mesh.faces.push(crate::rasterizer::Face::new(face.v2, face.v0, new_v0_idx));
        mesh.faces.push(crate::rasterizer::Face::new(face.v2, new_v0_idx, new_v2_idx));

        // Track the new top face for selection
        new_face_indices.push(face_idx);
    }

    // Select the new extruded faces
    if !new_face_indices.is_empty() {
        state.selection = ModelerSelection::MeshFaces(new_face_indices.clone());
        state.set_status(&format!("Extruded {} face(s) - drag to position", new_face_indices.len()), 1.5);
    }
}
