//! 3D Viewport for the modeler - renders meshes using the software rasterizer
//!
//! Simplified PicoCAD-style viewport with mesh editing only.

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext, Axis as UiAxis};
use crate::rasterizer::{
    Framebuffer, render_mesh, render_mesh_15, Color as RasterColor, Vec3,
    Vertex as RasterVertex, Face as RasterFace, WIDTH, HEIGHT,
    world_to_screen_with_ortho, world_to_screen_with_ortho_depth, draw_floor_grid, point_in_triangle_2d,
    OrthoProjection, Camera, draw_3d_line_clipped,
    screen_to_ray, ray_circle_angle,
};
use super::state::{ModelerState, ModelerSelection, SelectMode, Axis, ModalTransform, CameraMode, ViewportId, rotate_by_euler};
use super::drag::{DragUpdateResult, ActiveDrag};
use super::tools::ModelerToolId;
use super::skeleton::{draw_skeleton, ray_bone_intersect};

/// Convert state::Axis to ui::Axis
fn to_ui_axis(axis: Axis) -> UiAxis {
    match axis {
        Axis::X => UiAxis::X,
        Axis::Y => UiAxis::Y,
        Axis::Z => UiAxis::Z,
    }
}

/// Convert ui::Axis to state::Axis
fn from_ui_axis(axis: UiAxis) -> Axis {
    match axis {
        UiAxis::X => Axis::X,
        UiAxis::Y => Axis::Y,
        UiAxis::Z => Axis::Z,
    }
}

/// Get all selected element positions for modal transforms
fn get_selected_positions(state: &ModelerState) -> Vec<Vec3> {
    let mut positions = Vec::new();

    match &state.selection {
        ModelerSelection::Vertices(verts) => {
            let mesh = state.mesh();
            for &idx in verts {
                if let Some(vert) = mesh.vertices.get(idx) {
                    positions.push(vert.pos);
                }
            }
        }
        ModelerSelection::Edges(edges) => {
            let mesh = state.mesh();
            for (v0, v1) in edges {
                if let Some(vert0) = mesh.vertices.get(*v0) {
                    positions.push(vert0.pos);
                }
                if let Some(vert1) = mesh.vertices.get(*v1) {
                    positions.push(vert1.pos);
                }
            }
        }
        ModelerSelection::Faces(faces) => {
            let mesh = state.mesh();
            for &face_idx in faces {
                if let Some(face) = mesh.faces.get(face_idx) {
                    for &vi in &face.vertices {
                        if let Some(v) = mesh.vertices.get(vi) {
                            positions.push(v.pos);
                        }
                    }
                }
            }
        }
        ModelerSelection::Bones(bones) => {
            // For bones, return the world-space base position of each bone
            for &bone_idx in bones {
                let (base_pos, _) = state.get_bone_world_transform(bone_idx);
                positions.push(base_pos);
            }
        }
        ModelerSelection::BoneTips(tips) => {
            // For bone tips, return the world-space tip position of each bone
            for &bone_idx in tips {
                let tip_pos = state.get_bone_tip_position(bone_idx);
                positions.push(tip_pos);
            }
        }
        _ => {}
    }

    positions
}

/// Apply positions back to selected elements
/// If vertex_linking is enabled, also moves coincident vertices
fn apply_selected_positions(state: &mut ModelerState, positions: &[Vec3]) {
    const LINK_EPSILON: f32 = 0.001;
    let linking = state.vertex_linking;

    // Collect vertex movements: (idx, old_pos, new_pos)
    let mut movements: Vec<(usize, Vec3, Vec3)> = Vec::new();
    let mut pos_idx = 0;
    let selection = state.selection.clone();

    // First pass: collect movements (read-only)
    {
        let mesh = state.mesh();
        match &selection {
            ModelerSelection::Vertices(verts) => {
                for &vert_idx in verts {
                    if let Some(vert) = mesh.vertices.get(vert_idx) {
                        if let Some(&new_pos) = positions.get(pos_idx) {
                            movements.push((vert_idx, vert.pos, new_pos));
                        }
                        pos_idx += 1;
                    }
                }
            }
            ModelerSelection::Edges(edges) => {
                for (v0, v1) in edges {
                    if let Some(vert) = mesh.vertices.get(*v0) {
                        if let Some(&new_pos) = positions.get(pos_idx) {
                            movements.push((*v0, vert.pos, new_pos));
                        }
                        pos_idx += 1;
                    }
                    if let Some(vert) = mesh.vertices.get(*v1) {
                        if let Some(&new_pos) = positions.get(pos_idx) {
                            movements.push((*v1, vert.pos, new_pos));
                        }
                        pos_idx += 1;
                    }
                }
            }
            ModelerSelection::Faces(faces) => {
                for &face_idx in faces {
                    if let Some(face) = mesh.faces.get(face_idx).cloned() {
                        for &vi in &face.vertices {
                            if let Some(vert) = mesh.vertices.get(vi) {
                                if let Some(&new_pos) = positions.get(pos_idx) {
                                    movements.push((vi, vert.pos, new_pos));
                                }
                                pos_idx += 1;
                            }
                        }
                    }
                }
            }
            ModelerSelection::Bones(bones) => {
                // For bones, collect bone movements
                for &bone_idx in bones {
                    let (old_pos, _) = state.get_bone_world_transform(bone_idx);
                    if let Some(&new_pos) = positions.get(pos_idx) {
                        // Store bone_idx in the movements list (we'll handle it specially)
                        // Use a sentinel value to indicate this is a bone base, not a vertex
                        // 0x80000000 = bone base movement
                        movements.push((bone_idx | 0x80000000, old_pos, new_pos));
                    }
                    pos_idx += 1;
                }
            }
            ModelerSelection::BoneTips(tips) => {
                // For bone tips, collect tip movements
                for &bone_idx in tips {
                    let old_tip = state.get_bone_tip_position(bone_idx);
                    if let Some(&new_tip) = positions.get(pos_idx) {
                        // Use 0x40000000 to indicate this is a bone TIP, not a base
                        movements.push((bone_idx | 0x40000000, old_tip, new_tip));
                    }
                    pos_idx += 1;
                }
            }
            _ => {}
        }
    }

    // Second pass: apply movements (mutable)
    let mirror_settings = state.current_mirror_settings();

    // Handle bone movements first (marked with high bit)
    let bone_movements: Vec<_> = movements.iter()
        .filter(|(idx, _, _)| *idx & 0x80000000 != 0)
        .map(|(idx, old, new)| (*idx & 0x7FFFFFFF, *old, *new))
        .collect();

    for (bone_idx, old_pos, new_pos) in bone_movements {
        let delta = new_pos - old_pos;
        // Apply delta to bone's local_position
        if let Some(bones) = state.asset.skeleton_mut() {
            if let Some(bone) = bones.get_mut(bone_idx) {
                bone.local_position = bone.local_position + delta;
                state.dirty = true;
            }
        }
    }

    // Handle bone TIP movements (marked with 0x40000000)
    let tip_movements: Vec<_> = movements.iter()
        .filter(|(idx, _, _)| *idx & 0x40000000 != 0 && *idx & 0x80000000 == 0)
        .map(|(idx, old, new)| (*idx & 0x3FFFFFFF, *old, *new))
        .collect();

    for (bone_idx, _old_tip, new_tip) in tip_movements {
        // Get the bone's base position to calculate new direction
        let (base_pos, _) = state.get_bone_world_transform(bone_idx);
        let direction = new_tip - base_pos;
        let new_length = direction.len();

        if new_length > 0.001 {
            // Calculate new rotation from direction
            let new_rotation = direction_to_rotation(direction);

            let old_length = state.skeleton().get(bone_idx).map(|b| b.length).unwrap_or(0.0);
            if let Some(bones) = state.asset.skeleton_mut() {
                if let Some(bone) = bones.get_mut(bone_idx) {
                    bone.local_rotation = new_rotation;
                    bone.length = new_length;
                    state.dirty = true;
                }
                // Smart mode: only update children that were at the tip
                for bone in bones.iter_mut() {
                    if bone.parent == Some(bone_idx) {
                        let was_at_tip = (bone.local_position.y - old_length).abs() < 1.0;
                        if was_at_tip {
                            bone.local_position.y = new_length;
                        }
                    }
                }
            }
        }
    }

    // Handle mesh vertex movements
    if let Some(mesh) = state.mesh_mut() {
        let mut already_moved = std::collections::HashSet::new();
        for (idx, old_pos, new_pos) in &movements {
            // Skip bone movements (marked with 0x80000000 for base, 0x40000000 for tip)
            if *idx & 0xC0000000 != 0 {
                continue;
            }

            let delta = *new_pos - *old_pos;

            if linking {
                // Find all coincident vertices and move them by the same delta
                let coincident = mesh.find_coincident_vertices(*idx, LINK_EPSILON);
                for ci in coincident {
                    if !already_moved.contains(&ci) {
                        if let Some(vert) = mesh.vertices.get_mut(ci) {
                            let final_pos = vert.pos + delta;
                            // Constrain center vertices to mirror plane
                            vert.pos = mirror_settings.constrain_to_plane(final_pos);
                        }
                        already_moved.insert(ci);
                    }
                }
            } else {
                // Just move the single vertex
                if !already_moved.contains(idx) {
                    if let Some(vert) = mesh.vertices.get_mut(*idx) {
                        // Constrain center vertices to mirror plane
                        vert.pos = mirror_settings.constrain_to_plane(*new_pos);
                    }
                    already_moved.insert(*idx);
                }
            }
        }
    }
}

/// Handle modal transforms (G=Grab, S=Scale, R=Rotate) using DragManager
fn handle_modal_transform(state: &mut ModelerState, mouse_pos: (f32, f32), ctx: &crate::ui::UiContext) {
    if state.modal_transform == ModalTransform::None {
        return;
    }

    // Must have an active drag
    if !state.drag_manager.is_dragging() {
        state.modal_transform = ModalTransform::None;
        return;
    }

    // Note: Axis constraints (X/Y/Z keys) are now handled through ActionRegistry in handle_actions()

    // Update the drag with current mouse position
    // Modal transforms use screen-space coordinates
    let result = state.drag_manager.update(
        mouse_pos,
        &state.camera,
        1, // Not used for screen-space transforms
        1,
    );

    // Apply the updated positions
    let mut made_changes = false;
    let mirror_settings = state.current_mirror_settings();
    if let Some(mesh) = state.mesh_mut() {
        match result {
            DragUpdateResult::Move { positions, .. } => {
                for (vert_idx, new_pos) in positions {
                    if let Some(vert) = mesh.vertices.get_mut(vert_idx) {
                        // Constrain center vertices to mirror plane
                        vert.pos = mirror_settings.constrain_to_plane(new_pos);
                    }
                }
                made_changes = true;
            }
            DragUpdateResult::Scale { positions, .. } => {
                for (vert_idx, new_pos) in positions {
                    if let Some(vert) = mesh.vertices.get_mut(vert_idx) {
                        // Constrain center vertices to mirror plane
                        vert.pos = mirror_settings.constrain_to_plane(new_pos);
                    }
                }
                made_changes = true;
            }
            DragUpdateResult::Rotate { positions, .. } => {
                for (vert_idx, new_pos) in positions {
                    if let Some(vert) = mesh.vertices.get_mut(vert_idx) {
                        // Constrain center vertices to mirror plane
                        vert.pos = mirror_settings.constrain_to_plane(new_pos);
                    }
                }
                made_changes = true;
            }
            _ => {}
        }
    }
    if made_changes {
        state.dirty = true;
    }

    // Confirm on left click
    if ctx.mouse.left_pressed {
        // Sync tool state before ending
        match state.modal_transform {
            ModalTransform::Grab => state.tool_box.tools.move_tool.end_drag(),
            ModalTransform::Scale => state.tool_box.tools.scale.end_drag(),
            ModalTransform::Rotate => state.tool_box.tools.rotate.end_drag(),
            ModalTransform::None => {}
        }
        state.drag_manager.end();
        state.modal_transform = ModalTransform::None;
        state.dirty = true;
        state.set_status("Transform applied", 1.0);
    }

    // Cancel on right click (context menu handled separately in main viewport function)
    if ctx.mouse.right_pressed {
        // Sync tool state before cancelling
        match state.modal_transform {
            ModalTransform::Grab => state.tool_box.tools.move_tool.end_drag(),
            ModalTransform::Scale => state.tool_box.tools.scale.end_drag(),
            ModalTransform::Rotate => state.tool_box.tools.rotate.end_drag(),
            ModalTransform::None => {}
        }
        if let Some(original_positions) = state.drag_manager.cancel() {
            if let Some(mesh) = state.mesh_mut() {
                for (vert_idx, original_pos) in original_positions {
                    if let Some(vert) = mesh.vertices.get_mut(vert_idx) {
                        vert.pos = original_pos;
                    }
                }
            }
        }
        state.modal_transform = ModalTransform::None;
        state.set_status("Transform cancelled", 1.0);
    }
}

/// Handle left-drag to move selection in the viewport using DragManager
fn handle_drag_move(
    ctx: &UiContext,
    state: &mut ModelerState,
    mouse_pos: (f32, f32),
    inside_viewport: bool,
    fb_width: usize,
    fb_height: usize,
    viewport_id: ViewportId,
) {
    // Don't interfere with modal transforms
    if state.modal_transform != ModalTransform::None {
        return;
    }

    // Only active when we have a selection
    if state.selection.is_empty() {
        return;
    }

    // Check if we're already in a free move drag
    let is_free_moving = state.drag_manager.active.is_free_move();
    let is_ortho = viewport_id != ViewportId::Perspective;

    // Check if this viewport owns the ortho drag
    let owns_ortho_drag = state.ortho_drag_viewport == Some(viewport_id);
    // Skip if another ortho viewport owns this drag
    if state.ortho_drag_viewport.is_some() && !owns_ortho_drag {
        return;
    }

    if is_free_moving {
        if ctx.mouse.left_down {
            if is_ortho && owns_ortho_drag {
                // Ortho mode: use screen-to-world delta conversion
                let drag_zoom = state.ortho_drag_zoom;

                if let Some(drag_state) = &state.drag_manager.state {
                    let dx = mouse_pos.0 - drag_state.initial_mouse.0;
                    let dy = mouse_pos.1 - drag_state.initial_mouse.1;

                    // Convert screen delta to world delta based on viewport
                    let world_dx = dx / drag_zoom;
                    let world_dy = -dy / drag_zoom; // Y inverted

                    // Free move: movement on viewport plane (no axis constraint)
                    let delta = match viewport_id {
                        ViewportId::Top => Vec3::new(world_dx, 0.0, world_dy),    // XZ plane
                        ViewportId::Front => Vec3::new(world_dx, world_dy, 0.0),  // XY plane
                        ViewportId::Side => Vec3::new(0.0, world_dy, world_dx),   // ZY plane
                        ViewportId::Perspective => Vec3::ZERO,
                    };

                    // Apply delta to initial positions
                    if let super::drag::ActiveDrag::Move(tracker) = &state.drag_manager.active {
                        let snap_disabled = is_key_down(KeyCode::Z);
                        let snap_enabled = state.snap_settings.enabled && !snap_disabled;
                        let snap_size = state.snap_settings.grid_size;

                        let updates: Vec<_> = tracker.initial_positions.iter()
                            .map(|(idx, start_pos)| (*idx, *start_pos + delta))
                            .collect();

                        if let Some(mesh) = state.mesh_mut() {
                            for (idx, mut new_pos) in updates {
                                if snap_enabled {
                                    new_pos.x = (new_pos.x / snap_size).round() * snap_size;
                                    new_pos.y = (new_pos.y / snap_size).round() * snap_size;
                                    new_pos.z = (new_pos.z / snap_size).round() * snap_size;
                                }
                                if let Some(vert) = mesh.vertices.get_mut(idx) {
                                    vert.pos = new_pos;
                                }
                            }
                        }
                        state.dirty = true;
                    }
                }
            } else {
                // Perspective mode: use ray casting via DragManager
                // DEBUG: Free move uses PickerType::Screen which only uses deltas
                // Both initial_mouse and mouse_pos are in screen coords, so delta is correct
                let result = state.drag_manager.update(
                    mouse_pos,
                    &state.camera,
                    fb_width,
                    fb_height,
                );

                if let DragUpdateResult::Move { positions, .. } = result {
                    let snap_disabled = is_key_down(KeyCode::Z);
                    // Capture snap settings before borrowing mesh
                    let snap_enabled = state.snap_settings.enabled && !snap_disabled;
                    let snap_settings = state.snap_settings.clone();
                    if let Some(mesh) = state.mesh_mut() {
                        for (vert_idx, new_pos) in positions {
                            if let Some(vert) = mesh.vertices.get_mut(vert_idx) {
                                vert.pos = if snap_enabled {
                                    snap_settings.snap_vec3(new_pos)
                                } else {
                                    new_pos
                                };
                            }
                        }
                    }
                    state.dirty = true;
                }
            }
        } else {
            // End drag on mouse release
            state.drag_manager.end();
            if owns_ortho_drag {
                state.ortho_drag_viewport = None;
            }
            state.set_status("Moved", 0.5);
        }

        // Cancel drag on right-click
        if ctx.mouse.right_pressed {
            if let Some(original_positions) = state.drag_manager.cancel() {
                if let Some(mesh) = state.mesh_mut() {
                    for (vert_idx, original_pos) in original_positions {
                        if let Some(vert) = mesh.vertices.get_mut(vert_idx) {
                            vert.pos = original_pos;
                        }
                    }
                }
            }
            if owns_ortho_drag {
                state.ortho_drag_viewport = None;
            }
            state.set_status("Move cancelled", 0.5);
        }
    } else if !state.drag_manager.is_dragging() {
        // Not in any drag - check for free move start
        // Store potential start position (similar to box select)
        if ctx.mouse.left_pressed && inside_viewport {
            state.free_drag_pending_start = Some(mouse_pos);
        }

        // Check if we should convert pending start to actual free move
        if let Some(start_pos) = state.free_drag_pending_start {
            if ctx.mouse.left_down {
                let dx = (mouse_pos.0 - start_pos.0).abs();
                let dy = (mouse_pos.1 - start_pos.1).abs();
                // Only become free move if moved at least 3 pixels (distinguish from click)
                if dx > 3.0 || dy > 3.0 {
                    // Get vertex indices and initial positions
                    let mesh = state.mesh();
                    let mut indices = state.selection.get_affected_vertex_indices(mesh);
                    if state.vertex_linking {
                        indices = mesh.expand_to_coincident(&indices, 0.001);
                    }

                    let initial_positions: Vec<(usize, Vec3)> = indices.iter()
                        .filter_map(|&idx| mesh.vertices.get(idx).map(|v| (idx, v.pos)))
                        .collect();

                    if !initial_positions.is_empty() {
                        // Calculate center
                        let sum: Vec3 = initial_positions.iter().map(|(_, p)| *p).fold(Vec3::ZERO, |acc, p| acc + p);
                        let center = sum * (1.0 / initial_positions.len() as f32);

                        // Save undo state before starting
                        state.push_undo("Drag move");

                        // Use CURRENT mouse position as reference, not original click position.
                        // This prevents snapping - delta starts at 0 and accumulates from here.
                        let drag_start_mouse = mouse_pos;

                        // For ortho viewports, capture zoom and set ortho_drag_viewport
                        let is_ortho = viewport_id != ViewportId::Perspective;
                        if is_ortho {
                            state.ortho_drag_viewport = Some(viewport_id);
                            state.ortho_drag_zoom = state.get_ortho_camera(viewport_id).zoom;
                        }

                        // Get bone rotation for world-to-local delta transformation (bone-bound meshes)
                        let bone_rotation = state.selected_object()
                            .and_then(|obj| obj.default_bone_index)
                            .map(|bone_idx| state.get_bone_world_transform(bone_idx).1);

                        // Start free move drag (axis = None for screen-space movement)
                        state.drag_manager.start_move_with_bone(
                            center,
                            drag_start_mouse,
                            None,      // No axis = free movement
                            None,      // No axis direction for free movement
                            indices,
                            initial_positions,
                            state.snap_settings.enabled,
                            state.snap_settings.grid_size,
                            bone_rotation,
                        );

                        state.set_status("Drag to move (hold Shift for fine)", 3.0);
                    }

                    state.free_drag_pending_start = None;
                }
            } else {
                // Mouse released without dragging - clear pending
                state.free_drag_pending_start = None;
            }
        }
    }
}

/// Draw a 2D grid for orthographic views that respects pan and zoom
fn draw_ortho_grid(
    fb: &mut Framebuffer,
    state: &ModelerState,
    viewport_id: ViewportId,
    grid_color: RasterColor,
    x_axis_color: RasterColor,
    z_axis_color: RasterColor,
) {
    let ortho_cam = state.get_ortho_camera(viewport_id);
    let zoom = ortho_cam.zoom;
    let center = ortho_cam.center;

    let grid_size = crate::world::SECTOR_SIZE;
    let fb_w = fb.width as f32;
    let fb_h = fb.height as f32;

    // Calculate visible range in world units
    let half_w = fb_w / (2.0 * zoom);
    let half_h = fb_h / (2.0 * zoom);

    // World to framebuffer coords for this ortho view
    let world_to_fb = |wx: f32, wy: f32| -> (f32, f32) {
        let sx = (wx - center.x) * zoom + fb_w / 2.0;
        let sy = -(wy - center.y) * zoom + fb_h / 2.0; // Y flipped for screen coords
        (sx, sy)
    };

    // Calculate grid line range
    let start_x = ((center.x - half_w) / grid_size).floor() as i32;
    let end_x = ((center.x + half_w) / grid_size).ceil() as i32;
    let start_y = ((center.y - half_h) / grid_size).floor() as i32;
    let end_y = ((center.y + half_h) / grid_size).ceil() as i32;

    // Determine which world axis maps to screen X and Y for axis coloring
    // Top: X->screen_x, Z->screen_y (Y axis points out of screen)
    // Front: X->screen_x, Y->screen_y (Z axis points out of screen)
    // Side: Z->screen_x, Y->screen_y (X axis points out of screen)
    let (h_axis_color, v_axis_color) = match viewport_id {
        ViewportId::Top => (x_axis_color, z_axis_color),   // H=X axis, V=Z axis
        ViewportId::Front => (x_axis_color, grid_color),   // H=X axis, V=Y (no special color)
        ViewportId::Side => (z_axis_color, grid_color),    // H=Z axis, V=Y (no special color)
        ViewportId::Perspective => (grid_color, grid_color),
    };

    // Vertical lines (constant world X or Z depending on view)
    for i in start_x..=end_x {
        let wx = i as f32 * grid_size;
        let (sx, _) = world_to_fb(wx, 0.0);
        if sx >= 0.0 && sx < fb_w {
            let color = if i == 0 { v_axis_color } else { grid_color };
            fb.draw_line(sx as i32, 0, sx as i32, fb_h as i32, color);
        }
    }

    // Horizontal lines (constant world Y or Z depending on view)
    for i in start_y..=end_y {
        let wy = i as f32 * grid_size;
        let (_, sy) = world_to_fb(0.0, wy);
        if sy >= 0.0 && sy < fb_h {
            let color = if i == 0 { h_axis_color } else { grid_color };
            fb.draw_line(0, sy as i32, fb_w as i32, sy as i32, color);
        }
    }
}

/// Draw the 3D modeler viewport (perspective mode - backwards compatible)
pub fn draw_modeler_viewport(
    ctx: &mut UiContext,
    rect: Rect,
    state: &mut ModelerState,
    fb: &mut Framebuffer,
) {
    draw_modeler_viewport_ext(ctx, rect, state, fb, ViewportId::Perspective);
}

/// Draw the modeler viewport with configurable view type
///
/// This unified function handles both perspective and orthographic views.
/// For ortho views (Top/Front/Side), it sets up the appropriate camera and projection.
pub fn draw_modeler_viewport_ext(
    ctx: &mut UiContext,
    rect: Rect,
    state: &mut ModelerState,
    fb: &mut Framebuffer,
    viewport_id: ViewportId,
) {
    let is_ortho = viewport_id != ViewportId::Perspective;
    // Resize framebuffer based on resolution setting
    let (target_w, target_h) = if state.raster_settings.stretch_to_fill {
        let base_w = if state.raster_settings.low_resolution { WIDTH } else { crate::rasterizer::WIDTH_HI };
        let viewport_aspect = rect.h / rect.w;
        let scaled_h = (base_w as f32 * viewport_aspect) as usize;
        (base_w, scaled_h.max(1))
    } else if state.raster_settings.low_resolution {
        (WIDTH, HEIGHT)
    } else {
        (crate::rasterizer::WIDTH_HI, crate::rasterizer::HEIGHT_HI)
    };
    fb.resize(target_w, target_h);

    let mouse_pos = (ctx.mouse.x, ctx.mouse.y);
    let inside_viewport = ctx.mouse.inside(&rect);

    // Calculate viewport scaling
    let fb_width = fb.width;
    let fb_height = fb.height;
    let (draw_w, draw_h, draw_x, draw_y) = if state.raster_settings.stretch_to_fill {
        (rect.w, rect.h, rect.x, rect.y)
    } else {
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

    // =========================================================================
    // Ortho view setup and camera controls
    // =========================================================================

    // For ortho views, set up orthographic camera and projection
    let (ortho_camera, saved_ortho_projection) = if is_ortho {
        // Get ortho camera settings from state
        let ortho_cam = state.get_ortho_camera(viewport_id);
        let zoom = ortho_cam.zoom;
        let center = ortho_cam.center;

        // Create camera with appropriate orientation for this view
        let mut cam = match viewport_id {
            ViewportId::Top => Camera::ortho_top(),
            ViewportId::Front => Camera::ortho_front(),
            ViewportId::Side => Camera::ortho_side(),
            ViewportId::Perspective => unreachable!(),
        };

        // Position camera at a fixed distance along its view axis, looking toward origin
        // The camera looks along +basis_z, so position it behind the origin
        let view_distance = 50000.0; // Far enough to not clip anything
        match viewport_id {
            ViewportId::Top => {
                // Looking down -Y at XZ plane, camera above origin
                cam.position = Vec3::new(0.0, view_distance, 0.0);
            }
            ViewportId::Front => {
                // Looking down -Z at XY plane, camera in front of origin
                cam.position = Vec3::new(0.0, 0.0, view_distance);
            }
            ViewportId::Side => {
                // Looking down -X at ZY plane, camera to the right of origin
                cam.position = Vec3::new(view_distance, 0.0, 0.0);
            }
            ViewportId::Perspective => unreachable!(),
        }

        // Set up ortho projection - center values control panning
        // These are in camera-space coordinates:
        // - For Top view:   cam_x = world_x, cam_y = world_z, so center = (world_x_offset, world_z_offset)
        // - For Front view: cam_x = world_x, cam_y = world_y, so center = (world_x_offset, world_y_offset)
        // - For Side view:  cam_x = world_z, cam_y = world_y, so center = (world_z_offset, world_y_offset)
        // The ortho_cam.center stores (horizontal_offset, vertical_offset) which maps correctly
        let ortho_proj = OrthoProjection {
            zoom,
            center_x: center.x,
            center_y: center.y,
        };

        // Save current ortho_projection to restore later
        let saved = state.raster_settings.ortho_projection.take();
        state.raster_settings.ortho_projection = Some(ortho_proj);

        (Some(cam), saved)
    } else {
        (None, None)
    };

    // Handle ortho-specific camera controls (pan and zoom)
    // Use dedicated ortho_pan_viewport to track which viewport owns the pan drag
    if is_ortho {
        let owns_pan = state.ortho_pan_viewport == Some(viewport_id);
        let scroll = ctx.mouse.scroll;

        // Right-drag to pan
        if ctx.mouse.right_down {
            // Start capture if clicking inside viewport and no pan active
            if inside_viewport && state.ortho_pan_viewport.is_none() {
                state.ortho_pan_viewport = Some(viewport_id);
                state.ortho_last_mouse = (ctx.mouse.x, ctx.mouse.y);
            }

            // If this viewport owns the pan, continue panning (even if mouse left viewport)
            if owns_pan || (inside_viewport && state.ortho_pan_viewport == Some(viewport_id)) {
                let dx = ctx.mouse.x - state.ortho_last_mouse.0;
                let dy = ctx.mouse.y - state.ortho_last_mouse.1;
                // Pan in world units (inverse of zoom)
                let ortho_cam = state.get_ortho_camera_mut(viewport_id);
                ortho_cam.center.x -= dx / ortho_cam.zoom;
                ortho_cam.center.y += dy / ortho_cam.zoom; // Y inverted for screen coords
                state.ortho_last_mouse = (ctx.mouse.x, ctx.mouse.y);
            }
        } else {
            // Release pan capture when right mouse released
            if state.ortho_pan_viewport == Some(viewport_id) {
                state.ortho_pan_viewport = None;
            }
        }

        // Mouse wheel to zoom (only when inside this viewport)
        if inside_viewport && scroll != 0.0 {
            let zoom_factor = if scroll > 0.0 { 1.1 } else { 0.9 };
            let ortho_cam = state.get_ortho_camera_mut(viewport_id);
            ortho_cam.zoom = (ortho_cam.zoom * zoom_factor).clamp(0.001, 10.0);
        }

        // Update ortho_projection with new pan/zoom values so mesh and grid stay in sync
        let updated_cam = state.get_ortho_camera(viewport_id);
        state.raster_settings.ortho_projection = Some(OrthoProjection {
            zoom: updated_cam.zoom,
            center_x: updated_cam.center.x,
            center_y: updated_cam.center.y,
        });
    }

    // Perspective camera controls (free or orbit mode) - skip for ortho views
    // Use get_keys_down() which returns all currently pressed keys
    // This is more reliable on macOS where modifier key presses can cause
    // is_key_down() to return stale state for other keys
    let all_keys = get_keys_down(); // Returns HashSet<KeyCode>

    let shift_held = all_keys.contains(&KeyCode::LeftShift) || all_keys.contains(&KeyCode::RightShift);
    let ctrl_held = all_keys.contains(&KeyCode::LeftControl) || all_keys.contains(&KeyCode::RightControl)
        || all_keys.contains(&KeyCode::LeftSuper) || all_keys.contains(&KeyCode::RightSuper);
    let alt_held = all_keys.contains(&KeyCode::LeftAlt) || all_keys.contains(&KeyCode::RightAlt);
    // Shift is allowed with WASD (for speed boost), only Ctrl/Alt block camera movement
    let blocking_modifier = ctrl_held || alt_held;

    // macOS workaround: When Cmd+key is released, macOS doesn't send key-up for the letter key.
    // We track "trusted" state for each movement key:
    // - When blocking modifier (Ctrl/Alt) is released, all keys become untrusted
    // - Keys become trusted again when freshly pressed (is_key_pressed)
    // - Keys become trusted when released (not in all_keys) - ready for next press
    // Note: Shift doesn't cause stuck keys, so we don't include it here
    let modifier_just_released = state.modifier_was_held && !blocking_modifier;
    state.modifier_was_held = blocking_modifier;

    // Key indices: 0=W, 1=A, 2=S, 3=D, 4=Q, 5=E
    let movement_keys = [KeyCode::W, KeyCode::A, KeyCode::S, KeyCode::D, KeyCode::Q, KeyCode::E];

    // When modifier released, mark all movement keys as untrusted
    if modifier_just_released {
        state.trusted_movement_keys = [false; 6];
    }

    // Update trust status for each key
    for (i, &key) in movement_keys.iter().enumerate() {
        let key_in_all_keys = all_keys.contains(&key);

        // If key is not in all_keys, it's been released - mark as trusted (ready for next press)
        if !key_in_all_keys {
            state.trusted_movement_keys[i] = true;
        }

        // If key was just pressed this frame, it's definitely trusted
        if is_key_pressed(key) {
            state.trusted_movement_keys[i] = true;
        }
    }

    // Only consider a key "down" if it's both in all_keys AND trusted
    let w_down = all_keys.contains(&KeyCode::W) && state.trusted_movement_keys[0];
    let a_down = all_keys.contains(&KeyCode::A) && state.trusted_movement_keys[1];
    let s_down = all_keys.contains(&KeyCode::S) && state.trusted_movement_keys[2];
    let d_down = all_keys.contains(&KeyCode::D) && state.trusted_movement_keys[3];
    let q_down = all_keys.contains(&KeyCode::Q) && state.trusted_movement_keys[4];
    let e_down = all_keys.contains(&KeyCode::E) && state.trusted_movement_keys[5];

    if !is_ortho {
    match state.camera_mode {
        CameraMode::Free => {
            // Free camera: right-drag to look around, WASD to move
            if ctx.mouse.right_down && inside_viewport && !state.drag_manager.is_dragging() {
                if state.viewport_mouse_captured {
                    // Inverted to match Y-down coordinate system (same as world editor)
                    let dx = (mouse_pos.1 - state.viewport_last_mouse.1) * 0.005;
                    let dy = -(mouse_pos.0 - state.viewport_last_mouse.0) * 0.005;
                    state.camera.rotation_x += dx;
                    state.camera.rotation_y += dy;
                    state.camera.update_basis();
                }
                state.viewport_mouse_captured = true;
            } else if !ctx.mouse.right_down {
                state.viewport_mouse_captured = false;
            }

            // Keyboard camera movement (WASD + Q/E) - only while right-click held (like Unity/Unreal)
            // This prevents conflicts with editing shortcuts like E for extrude
            // Shift increases movement speed
            let base_speed = 50.0; // Scaled for TRLE units (1024 per sector)
            let speed = if shift_held { base_speed * 4.0 } else { base_speed };
            if ctx.mouse.right_down && (inside_viewport || state.viewport_mouse_captured) && !state.drag_manager.is_dragging() && !blocking_modifier {
                if w_down {
                    state.camera.position = state.camera.position + state.camera.basis_z * speed;
                }
                if s_down {
                    state.camera.position = state.camera.position - state.camera.basis_z * speed;
                }
                if a_down {
                    state.camera.position = state.camera.position - state.camera.basis_x * speed;
                }
                if d_down {
                    state.camera.position = state.camera.position + state.camera.basis_x * speed;
                }
                if q_down {
                    state.camera.position = state.camera.position - state.camera.basis_y * speed;
                }
                if e_down {
                    state.camera.position = state.camera.position + state.camera.basis_y * speed;
                }
            }
        }

        CameraMode::Orbit => {
            // Orbit camera: right-drag rotates around target (or pans with Shift)
            if ctx.mouse.right_down && (inside_viewport || state.viewport_mouse_captured) {
                if state.viewport_mouse_captured {
                    let dx = mouse_pos.0 - state.viewport_last_mouse.0;
                    let dy = mouse_pos.1 - state.viewport_last_mouse.1;

                    if shift_held {
                        // Shift+Right drag: pan the orbit target
                        let pan_speed = state.orbit_distance * 0.002;
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
                let scroll = ctx.mouse.scroll;
                if scroll != 0.0 {
                    let zoom_factor = if scroll > 0.0 { 0.98 } else { 1.02 };
                    // Scale: 1024 units = 1 meter, allow 1m to 40m camera distance
                    state.orbit_distance = (state.orbit_distance * zoom_factor).clamp(1024.0, 40960.0);
                    state.sync_camera_from_orbit();
                }
            }
        }
    }
    } // end if !is_ortho

    state.viewport_last_mouse = mouse_pos;

    // Use ortho camera for rendering if in ortho mode
    // We need to temporarily swap in the ortho camera for the rendering pass
    let original_camera = if let Some(ref ortho_cam) = ortho_camera {
        let original = state.camera.clone();
        state.camera = ortho_cam.clone();
        Some(original)
    } else {
        None
    };

    // Modal transforms: G = Grab, S = Scale, R = Rotate (now using DragManager)
    // Note: G/S/R keys are now handled through ActionRegistry in handle_actions()
    // which sets state.modal_transform. Here we just start the drag when needed.
    let has_selection = !state.selection.is_empty();
    let modal_requested = state.modal_transform != ModalTransform::None;
    let drag_not_started = !state.drag_manager.is_dragging();

    // If modal_transform was set by ActionRegistry but drag not started, start it now
    if has_selection && modal_requested && drag_not_started {
        let mode = state.modal_transform;

        // Get vertex indices and initial positions (same as gizmo drags)
        let mesh = state.mesh();
        let mut indices = state.selection.get_affected_vertex_indices(mesh);
        if state.vertex_linking {
            indices = mesh.expand_to_coincident(&indices, 0.001);
        }

        let initial_positions: Vec<(usize, Vec3)> = indices.iter()
            .filter_map(|&idx| mesh.vertices.get(idx).map(|v| (idx, v.pos)))
            .collect();

        if !initial_positions.is_empty() {
            // Calculate center
            let sum: Vec3 = initial_positions.iter().map(|(_, p)| *p).fold(Vec3::ZERO, |acc, p| acc + p);
            let center = sum * (1.0 / initial_positions.len() as f32);

            // Get bone rotation for world-to-local delta transformation (for bone-bound meshes)
            let bone_rotation = state.selected_object()
                .and_then(|obj| obj.default_bone_index)
                .map(|bone_idx| state.get_bone_world_transform(bone_idx).1);

            // Save undo state before starting transform
            state.push_undo(mode.label());

            // Start the appropriate DragManager drag and sync tool state
            match mode {
                ModalTransform::Grab => {
                    state.tool_box.tools.move_tool.start_drag(None);
                    state.drag_manager.start_move_with_bone(
                        center,
                        mouse_pos,
                        None, // No axis constraint initially
                        None, // No axis direction for free movement
                        indices,
                        initial_positions,
                        state.snap_settings.enabled,
                        state.snap_settings.grid_size,
                        bone_rotation,
                    );
                }
                ModalTransform::Scale => {
                    state.tool_box.tools.scale.start_drag(None);
                    state.drag_manager.start_scale(
                        center,
                        mouse_pos,
                        None, // No axis constraint initially
                        indices,
                        initial_positions,
                        mouse_pos, // Use mouse_pos as center for screen-space scaling
                    );
                }
                ModalTransform::Rotate => {
                    // Convert screen mouse to framebuffer coordinates for initial angle calculation
                    let fb_mouse = (
                        (mouse_pos.0 - draw_x) / draw_w * fb_width as f32,
                        (mouse_pos.1 - draw_y) / draw_h * fb_height as f32,
                    );

                    // Calculate initial angle using ray-circle intersection
                    // Default to Y axis rotation
                    let ref_vector = Vec3::new(1.0, 0.0, 0.0);
                    let axis_vec = Vec3::new(0.0, 1.0, 0.0);
                    let ray = screen_to_ray(fb_mouse.0, fb_mouse.1, fb_width, fb_height, &state.camera);
                    let initial_angle = ray_circle_angle(&ray, center, axis_vec, ref_vector)
                        .unwrap_or(0.0);

                    state.tool_box.tools.rotate.start_drag(Some(UiAxis::Y), initial_angle);
                    state.drag_manager.start_rotate(
                        center,
                        initial_angle,
                        mouse_pos, // raw screen-space mouse (converted internally using viewport transform)
                        mouse_pos, // screen-space center (fallback)
                        UiAxis::Y, // Default to Y axis rotation
                        indices,
                        initial_positions,
                        state.snap_settings.enabled,
                        15.0, // 15-degree snap increments
                        &state.camera,
                        fb_width,
                        fb_height,
                        (draw_x, draw_y, draw_w, draw_h), // viewport transform
                    );
                }
                ModalTransform::None => {}
            }

            state.set_status(&format!("{} - X/Y/Z to constrain, click to confirm", mode.label()), 5.0);
        } else {
            // No valid vertices, cancel the modal transform
            state.modal_transform = ModalTransform::None;
        }
    }

    handle_modal_transform(state, mouse_pos, ctx);

    // Handle left-click drag to move selection (if not in modal transform)
    handle_drag_move(ctx, state, mouse_pos, inside_viewport, fb_width, fb_height, viewport_id);

    // Clear and render
    fb.clear(RasterColor::new(30, 30, 35));

    // Draw grid
    let grid_color = RasterColor::new(50, 50, 60);
    let x_axis_color = RasterColor::new(100, 60, 60); // Red-ish for X axis
    let z_axis_color = RasterColor::new(60, 60, 100); // Blue-ish for Z axis

    if is_ortho {
        // For ortho views, draw a 2D grid that respects pan and zoom
        draw_ortho_grid(fb, state, viewport_id, grid_color, x_axis_color, z_axis_color);
    } else {
        // For perspective, use the 3D floor grid
        draw_floor_grid(fb, &state.camera, 0.0, crate::world::SECTOR_SIZE, crate::world::SECTOR_SIZE * 10.0, grid_color, x_axis_color, z_axis_color);
    }

    // Draw mirror plane indicator when mirror mode is enabled
    if state.current_mirror_settings().enabled {
        let mirror_color = RasterColor::new(255, 100, 100); // Pinkish-red for mirror plane
        let extent = crate::world::SECTOR_SIZE * 5.0; // Large enough to be visible

        // Draw cross lines along the mirror plane
        match state.current_mirror_settings().axis {
            Axis::X => {
                // X mirror: plane at X=0, draw vertical line in YZ plane
                draw_3d_line_clipped(fb, &state.camera, Vec3::new(0.0, -extent, 0.0), Vec3::new(0.0, extent, 0.0), mirror_color);
                draw_3d_line_clipped(fb, &state.camera, Vec3::new(0.0, 0.0, -extent), Vec3::new(0.0, 0.0, extent), mirror_color);
            }
            Axis::Y => {
                // Y mirror: plane at Y=0, draw lines in XZ plane
                draw_3d_line_clipped(fb, &state.camera, Vec3::new(-extent, 0.0, 0.0), Vec3::new(extent, 0.0, 0.0), mirror_color);
                draw_3d_line_clipped(fb, &state.camera, Vec3::new(0.0, 0.0, -extent), Vec3::new(0.0, 0.0, extent), mirror_color);
            }
            Axis::Z => {
                // Z mirror: plane at Z=0, draw lines in XY plane
                draw_3d_line_clipped(fb, &state.camera, Vec3::new(-extent, 0.0, 0.0), Vec3::new(extent, 0.0, 0.0), mirror_color);
                draw_3d_line_clipped(fb, &state.camera, Vec3::new(0.0, -extent, 0.0), Vec3::new(0.0, extent, 0.0), mirror_color);
            }
        }
    }

    // Render all visible objects using combined geometry
    // This ensures proper depth sorting across all objects (painter's algorithm and z-buffer)
    let use_rgb555 = state.raster_settings.use_rgb555;

    // Check if the Mesh component itself is hidden
    let mesh_component_hidden = state.asset.components.iter()
        .enumerate()
        .find(|(_, c)| matches!(c, crate::asset::AssetComponent::Mesh { .. }))
        .map(|(idx, _)| state.is_component_hidden(idx))
        .unwrap_or(false);

    // Fallback CLUT for objects with no assigned CLUT
    let fallback_clut = state.clut_pool.first_id()
        .and_then(|id| state.clut_pool.get(id));

    // Collect all geometry from all objects into combined lists
    let mut all_vertices: Vec<RasterVertex> = Vec::new();
    let mut all_faces: Vec<RasterFace> = Vec::new();
    let mut all_textures: Vec<crate::rasterizer::Texture> = Vec::new();
    let mut all_textures_15: Vec<crate::rasterizer::Texture15> = Vec::new();
    let mut all_blend_modes: Vec<crate::rasterizer::BlendMode> = Vec::new();

    // Cache transformed vertex positions for the selected object (for selection overlays)
    // This avoids redundant bone transforms - compute once, use everywhere
    let mut selected_object_world_vertices: Option<Vec<Vec3>> = None;

    // Track if any object needs backface culling disabled
    let mut any_double_sided = false;

    for (obj_idx, obj) in state.objects().iter().enumerate() {
        // Skip all mesh objects if Mesh component is hidden
        if mesh_component_hidden {
            continue;
        }
        // Skip hidden objects
        if !obj.visible {
            continue;
        }

        if obj.double_sided {
            any_double_sided = true;
        }

        // Get this object's CLUT from the shared pool
        let obj_clut = if obj.atlas.default_clut.is_valid() {
            state.clut_pool.get(obj.atlas.default_clut).or(fallback_clut)
        } else {
            fallback_clut
        };

        // Convert this object's atlas to rasterizer texture
        let texture_idx = all_textures.len();
        if let Some(clut) = obj_clut {
            all_textures.push(obj.atlas.to_raster_texture(clut, &format!("atlas_{}", obj_idx)));
            if use_rgb555 {
                all_textures_15.push(obj.atlas.to_texture15(clut, &format!("atlas_{}", obj_idx)));
            }
        }

        let mesh = &obj.mesh;

        // All objects use same brightness (selection shown via corner brackets)
        let base_color = 180u8;

        // Track vertex offset for this object
        let vertex_offset = all_vertices.len();

        // Pre-compute all bone transforms for per-vertex skinning
        // Each vertex can have its own bone_index, with fallback to mesh's default_bone_index
        let bone_transforms: Vec<(Vec3, Vec3)> = (0..state.skeleton().len())
            .map(|i| state.get_bone_world_transform(i))
            .collect();

        // For selected object, we'll cache world positions for selection overlays
        let is_selected = state.selected_object == Some(obj_idx);
        let mut world_positions: Vec<Vec3> = if is_selected {
            Vec::with_capacity(mesh.vertices.len())
        } else {
            Vec::new()
        };

        // Add vertices from this object (with per-vertex bone transforms)
        for v in &mesh.vertices {
            // Per-vertex bone assignment with fallback to mesh default
            let bone_idx = v.bone_index.or(obj.default_bone_index);
            let bone_transform = bone_idx.and_then(|idx| bone_transforms.get(idx)).copied();

            let (pos, normal) = if let Some((bone_pos, bone_rot)) = bone_transform {
                // Vertices are in bone-local space, transform to world space
                let rotated = rotate_by_euler(v.pos, bone_rot);
                let world_pos = rotated + bone_pos;
                // Rotate normal (no translation)
                let world_normal = rotate_by_euler(v.normal, bone_rot);
                (world_pos, world_normal)
            } else {
                (v.pos, v.normal)
            };

            // Cache world position for selected object
            if is_selected {
                world_positions.push(pos);
            }

            // Highlight vertices assigned to selected bone (green tint)
            // Only show when skeleton component is selected AND a specific bone is selected
            let skeleton_component_selected = state.selected_component
                .and_then(|idx| state.asset.components.get(idx))
                .map(|c| c.is_skeleton())
                .unwrap_or(false);
            let color = if is_selected && skeleton_component_selected && state.selected_bone.is_some() && bone_idx == state.selected_bone {
                // Bright green for vertices on selected bone
                RasterColor::new(100, 220, 120)
            } else {
                RasterColor::new(base_color, base_color, base_color)
            };

            all_vertices.push(RasterVertex {
                pos,
                normal,
                uv: v.uv,
                color,
                bone_index: None,
            });
        }

        // Store cached positions for selected object
        if is_selected {
            selected_object_world_vertices = Some(world_positions);
        }

        // Add mirrored vertices if mirror mode is enabled for selected object
        let mirror_vertex_offset = if state.current_mirror_settings().enabled && state.selected_object == Some(obj_idx) {
            let offset = all_vertices.len();
            for v in &mesh.vertices {
                // First mirror in local space, then apply per-vertex bone transform
                let mirrored_pos = state.current_mirror_settings().mirror_position(v.pos);
                let mirrored_normal = state.current_mirror_settings().mirror_normal(v.normal);

                // Per-vertex bone assignment with fallback to mesh default
                let bone_idx = v.bone_index.or(obj.default_bone_index);
                let bone_transform = bone_idx.and_then(|idx| bone_transforms.get(idx)).copied();

                let (pos, normal) = if let Some((bone_pos, bone_rot)) = bone_transform {
                    let rotated = rotate_by_euler(mirrored_pos, bone_rot);
                    let world_pos = rotated + bone_pos;
                    let world_normal = rotate_by_euler(mirrored_normal, bone_rot);
                    (world_pos, world_normal)
                } else {
                    (mirrored_pos, mirrored_normal)
                };

                all_vertices.push(RasterVertex {
                    pos,
                    normal,
                    uv: v.uv,
                    color: RasterColor::new(
                        (base_color as f32 * 0.85) as u8,
                        (base_color as f32 * 0.85) as u8,
                        (base_color as f32 * 0.85) as u8,
                    ),
                    bone_index: None,
                });
            }
            Some(offset)
        } else {
            None
        };

        // Add faces from this object (with adjusted vertex indices and texture ID)
        for edit_face in &mesh.faces {
            for [v0, v1, v2] in edit_face.triangulate() {
                all_faces.push(RasterFace {
                    v0: vertex_offset + v0,
                    v1: vertex_offset + v1,
                    v2: vertex_offset + v2,
                    texture_id: Some(texture_idx),
                    black_transparent: edit_face.black_transparent,
                    blend_mode: edit_face.blend_mode,
                });
                all_blend_modes.push(edit_face.blend_mode);

                // Add mirrored face if mirror mode enabled
                if let Some(mirror_offset) = mirror_vertex_offset {
                    all_faces.push(RasterFace {
                        v0: mirror_offset + v0,
                        v1: mirror_offset + v2,  // Swap to reverse winding
                        v2: mirror_offset + v1,
                        texture_id: Some(texture_idx),
                        black_transparent: edit_face.black_transparent,
                        blend_mode: edit_face.blend_mode,
                    });
                    all_blend_modes.push(edit_face.blend_mode);
                }
            }
        }
    }

    // Render all combined geometry in one pass
    if !all_vertices.is_empty() && !all_faces.is_empty() {
        // Use combined raster settings
        let mut combined_settings = if any_double_sided {
            let mut settings = state.raster_settings.clone();
            settings.backface_cull = false;
            settings.backface_wireframe = false;
            settings
        } else {
            state.raster_settings.clone()
        };

        // Add lights from Light components to the render settings
        for (comp_idx, component) in state.asset.components.iter().enumerate() {
            if state.is_component_hidden(comp_idx) {
                continue;
            }
            if let crate::asset::AssetComponent::Light { color, intensity, radius, offset } = component {
                use crate::rasterizer::{Light, LightType};
                let light = Light {
                    name: format!("Component Light {}", comp_idx),
                    light_type: LightType::Point {
                        position: Vec3::new(offset[0], offset[1], offset[2]),
                        radius: *radius,
                    },
                    color: RasterColor::new(color[0], color[1], color[2]),
                    intensity: *intensity,
                    enabled: true,
                };
                combined_settings.lights.push(light);
            }
        }

        if use_rgb555 {
            render_mesh_15(
                fb,
                &all_vertices,
                &all_faces,
                &all_textures_15,
                Some(&all_blend_modes),
                &state.camera,
                &combined_settings,
                None,
            );
        } else {
            render_mesh(
                fb,
                &all_vertices,
                &all_faces,
                &all_textures,
                &state.camera,
                &combined_settings,
            );
        }
    }

    // Draw corner brackets around selected object's bounding box
    draw_selected_object_brackets(state, fb);

    // Draw selection overlays (using cached world positions to avoid redundant bone transforms)
    draw_mesh_selection_overlays(state, fb, selected_object_world_vertices.as_deref());

    // Draw component gizmos (lights, etc.)
    draw_component_gizmos(state, fb);

    // Draw skeleton bones (if visible or in skeleton mode)
    let ortho_proj = state.raster_settings.ortho_projection.as_ref();
    draw_skeleton(fb, state, ortho_proj);

    // Draw box selection preview (highlight elements that would be selected)
    if state.box_select_viewport == Some(viewport_id) {
        if let ActiveDrag::BoxSelect(tracker) = &state.drag_manager.active {
            let (min_x, min_y, max_x, max_y) = tracker.bounds();
            // Convert screen coords to framebuffer coords
            let fb_x0 = (min_x - draw_x) / draw_w * fb_width as f32;
            let fb_y0 = (min_y - draw_y) / draw_h * fb_height as f32;
            let fb_x1 = (max_x - draw_x) / draw_w * fb_width as f32;
            let fb_y1 = (max_y - draw_y) / draw_h * fb_height as f32;
            draw_box_selection_preview(state, fb, fb_x0, fb_y0, fb_x1, fb_y1, viewport_id, selected_object_world_vertices.as_deref());
        }
    }

    // Blit framebuffer to screen
    let texture = Texture2D::from_rgba8(
        fb.width as u16,
        fb.height as u16,
        &fb.pixels,
    );
    texture.set_filter(FilterMode::Nearest);
    draw_texture_ex(
        &texture,
        draw_x,
        draw_y,
        WHITE,
        DrawTextureParams {
            dest_size: Some(vec2(draw_w, draw_h)),
            ..Default::default()
        },
    );

    // Check if a non-Mesh/non-Skeleton component is selected (Light, etc.)
    // When true: disable all mesh interaction, only component gizmo works
    // When false: normal mesh editing mode (includes Skeleton for bone selection/gizmo)
    let non_mesh_component_selected = state.selected_component
        .and_then(|idx| state.asset.components.get(idx))
        .map(|c| !matches!(c, crate::asset::AssetComponent::Mesh { .. } | crate::asset::AssetComponent::Skeleton { .. }))
        .unwrap_or(false);

    if non_mesh_component_selected {
        // Component editing mode: only component gizmo, no mesh interaction
        handle_component_move_gizmo(ctx, state, draw_x, draw_y, draw_w, draw_h, fb_width, fb_height, viewport_id);
    } else {
        // Mesh editing mode: normal mesh tools and interaction
        handle_transform_gizmo(ctx, state, mouse_pos, inside_viewport, draw_x, draw_y, draw_w, draw_h, fb_width, fb_height, viewport_id);

        // Update hover state every frame (like world editor) - but not when gizmo is active
        let is_active_viewport = state.active_viewport == viewport_id;
        if is_active_viewport && !state.drag_manager.is_dragging() && state.gizmo_hovered_axis.is_none() {
            update_hover_state(state, mouse_pos, draw_x, draw_y, draw_w, draw_h, fb_width, fb_height, viewport_id);
        }

        // Handle box selection (left-drag without hitting an element or gizmo)
        if state.gizmo_hovered_axis.is_none() {
            handle_box_selection(ctx, state, mouse_pos, inside_viewport, draw_x, draw_y, draw_w, draw_h, fb_width, fb_height, viewport_id);
        }

        // Draw box selection overlay only for the viewport that owns it
        if state.box_select_viewport == Some(viewport_id) {
            if let ActiveDrag::BoxSelect(tracker) = &state.drag_manager.active {
                let (min_x, min_y, max_x, max_y) = tracker.bounds();

                // Clip box selection rectangle to viewport bounds
                let clip_min_x = min_x.max(draw_x);
                let clip_min_y = min_y.max(draw_y);
                let clip_max_x = max_x.min(draw_x + draw_w);
                let clip_max_y = max_y.min(draw_y + draw_h);

                // Only draw if the clipped rectangle has positive area
                if clip_max_x > clip_min_x && clip_max_y > clip_min_y {
                    draw_rectangle(clip_min_x, clip_min_y, clip_max_x - clip_min_x, clip_max_y - clip_min_y, Color::from_rgba(100, 150, 255, 50));
                    draw_rectangle_lines(clip_min_x, clip_min_y, clip_max_x - clip_min_x, clip_max_y - clip_min_y, 1.0, Color::from_rgba(100, 150, 255, 200));
                }
            }
        }
    }

    // Handle skeleton interaction (when Skeleton component selected)
    let skeleton_selected = state.selected_component
        .and_then(|idx| state.asset.components.get(idx))
        .map(|c| c.is_skeleton())
        .unwrap_or(false);

    if skeleton_selected && inside_viewport {
        handle_skeleton_interaction(ctx, state, mouse_pos, draw_x, draw_y, draw_w, draw_h, fb_width, fb_height, viewport_id);
    }

    // Handle single-click selection using hover system (like world editor)
    // Handles both mesh selection and bone selection (click-based)
    // Skip if radial menu is open - menu consumes clicks
    if inside_viewport && ctx.mouse.left_pressed
        && state.modal_transform == ModalTransform::None
        && state.gizmo_hovered_axis.is_none()
        && !state.drag_manager.is_dragging()
        && !state.radial_menu.is_open
    {
        handle_hover_click(state);
    }

    // Restore original camera and ortho settings after ortho rendering
    if let Some(original) = original_camera {
        state.camera = original;
    }
    state.raster_settings.ortho_projection = saved_ortho_projection;
}

/// Handle box/rectangle selection using DragManager
fn handle_box_selection(
    ctx: &UiContext,
    state: &mut ModelerState,
    mouse_pos: (f32, f32),
    inside_viewport: bool,
    draw_x: f32,
    draw_y: f32,
    draw_w: f32,
    draw_h: f32,
    fb_width: usize,
    fb_height: usize,
    viewport_id: ViewportId,
) {
    // Don't start box select during modal transforms, other drags, or bone tip dragging
    if state.modal_transform != ModalTransform::None || state.bone_creation.is_some() {
        return;
    }

    // Check if we're already in a box select drag
    let is_box_selecting = state.drag_manager.active.is_box_select();

    // Only process active box select for the viewport that owns it
    if is_box_selecting && state.box_select_viewport == Some(viewport_id) {
        // Update the box select tracker with current mouse position
        if let ActiveDrag::BoxSelect(tracker) = &mut state.drag_manager.active {
            tracker.current_mouse = mouse_pos;
        }

        // On mouse release, apply the selection
        if !ctx.mouse.left_down {
            // Get bounds from tracker before ending drag
            let bounds = if let ActiveDrag::BoxSelect(tracker) = &state.drag_manager.active {
                Some(tracker.bounds())
            } else {
                None
            };

            if let Some((x0, y0, x1, y1)) = bounds {
                // Convert screen coords to framebuffer coords
                let fb_x0 = (x0 - draw_x) / draw_w * fb_width as f32;
                let fb_y0 = (y0 - draw_y) / draw_h * fb_height as f32;
                let fb_x1 = (x1 - draw_x) / draw_w * fb_width as f32;
                let fb_y1 = (y1 - draw_y) / draw_h * fb_height as f32;

                apply_box_selection(state, fb_x0, fb_y0, fb_x1, fb_y1, fb_width, fb_height, viewport_id);
            }

            // End the drag and clear viewport ownership
            state.drag_manager.end();
            state.box_select_viewport = None;
        }
    } else if !state.drag_manager.is_dragging() {
        // Not in any drag - check for box select start
        // We need to detect drag start vs click, so we track potential start position
        if ctx.mouse.left_pressed && inside_viewport {
            // Store potential start position (will become box select if dragged far enough)
            state.box_select_pending_start = Some(mouse_pos);
            state.box_select_viewport = Some(viewport_id);
        }

        // Check if we should convert pending start to actual box select
        // IMPORTANT: Only process if this viewport owns the pending box select
        if state.box_select_viewport == Some(viewport_id) {
            if let Some(start_pos) = state.box_select_pending_start {
                if ctx.mouse.left_down {
                    let dx = (mouse_pos.0 - start_pos.0).abs();
                    let dy = (mouse_pos.1 - start_pos.1).abs();
                    // Only become box select if moved at least 5 pixels
                    if dx > 5.0 || dy > 5.0 {
                        state.drag_manager.start_box_select(start_pos);
                        // Update with current position
                        if let ActiveDrag::BoxSelect(tracker) = &mut state.drag_manager.active {
                            tracker.current_mouse = mouse_pos;
                        }
                        state.box_select_pending_start = None;
                        // Note: box_select_viewport stays set to track which viewport owns the active drag
                    }
                } else {
                    // Mouse released without dragging - clear pending
                    state.box_select_pending_start = None;
                    state.box_select_viewport = None;
                }
            }
        }
    }

    // Note: Cancel box selection on ESC is handled through ActionRegistry in handle_actions()
}

/// Apply box selection to mesh elements
fn apply_box_selection(
    state: &mut ModelerState,
    fb_x0: f32,
    fb_y0: f32,
    fb_x1: f32,
    fb_y1: f32,
    fb_width: usize,
    fb_height: usize,
    viewport_id: ViewportId,
) {
    // Skip if Mesh component is hidden - nothing to select
    let mesh_hidden = state.asset.components.iter()
        .enumerate()
        .find(|(_, c)| matches!(c, crate::asset::AssetComponent::Mesh { .. }))
        .map(|(idx, _)| state.is_component_hidden(idx))
        .unwrap_or(false);
    if mesh_hidden {
        return;
    }

    // For ortho viewports, we need to set up the ortho projection from the viewport's camera
    // The raster_settings.ortho_projection is None at this point because it gets reset after rendering
    let is_ortho = matches!(viewport_id, ViewportId::Top | ViewportId::Front | ViewportId::Side);

    // Create a temporary ortho projection for ortho viewports
    let ortho_proj_temp;
    let ortho = if is_ortho {
        let ortho_cam = state.get_ortho_camera(viewport_id);
        ortho_proj_temp = Some(OrthoProjection {
            zoom: ortho_cam.zoom,
            center_x: ortho_cam.center.x,
            center_y: ortho_cam.center.y,
        });
        ortho_proj_temp.as_ref()
    } else {
        state.raster_settings.ortho_projection.as_ref()
    };

    // For ortho viewports, we need to use the appropriate camera orientation
    let camera = if is_ortho {
        match viewport_id {
            ViewportId::Top => Camera::ortho_top(),
            ViewportId::Front => Camera::ortho_front(),
            ViewportId::Side => Camera::ortho_side(),
            ViewportId::Perspective => state.camera.clone(),
        }
    } else {
        state.camera.clone()
    };

    let mesh = state.mesh();

    // Pre-compute all bone transforms for per-vertex skinning (same as find_hovered_element)
    let bone_transforms: Vec<(Vec3, Vec3)> = (0..state.skeleton().len())
        .map(|i| state.get_bone_world_transform(i))
        .collect();

    // Get mesh's default bone index
    let default_bone_idx = state.selected_object().and_then(|obj| obj.default_bone_index);

    // Helper to transform vertex position to world space (per-vertex bone)
    let get_world_pos = |v: &crate::rasterizer::Vertex| -> Vec3 {
        let bone_idx = v.bone_index.or(default_bone_idx);
        let bone_transform = bone_idx.and_then(|idx| bone_transforms.get(idx)).copied();

        if let Some((bone_pos, bone_rot)) = bone_transform {
            rotate_by_euler(v.pos, bone_rot) + bone_pos
        } else {
            v.pos
        }
    };

    // Check if adding to selection (Shift or X held)
    let add_to_selection = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift)
                        || is_key_down(KeyCode::X);

    match state.select_mode {
        SelectMode::Vertex => {
            let mut selected = if add_to_selection {
                if let ModelerSelection::Vertices(v) = &state.selection { v.clone() } else { Vec::new() }
            } else {
                Vec::new()
            };

            for (idx, vert) in mesh.vertices.iter().enumerate() {
                let world_pos = get_world_pos(vert);
                if let Some((sx, sy)) = world_to_screen_with_ortho(
                    world_pos,
                    camera.position,
                    camera.basis_x,
                    camera.basis_y,
                    camera.basis_z,
                    fb_width,
                    fb_height,
                    ortho,
                ) {
                    if sx >= fb_x0 && sx <= fb_x1 && sy >= fb_y0 && sy <= fb_y1 {
                        if !selected.contains(&idx) {
                            selected.push(idx);
                        }
                    }
                }
            }

            if !selected.is_empty() {
                let count = selected.len();
                state.set_selection(ModelerSelection::Vertices(selected));
                state.set_status(&format!("Selected {} vertex(es)", count), 0.5);
            } else if !add_to_selection {
                state.set_selection(ModelerSelection::None);
            }
        }
        SelectMode::Face => {
            let mut selected = if add_to_selection {
                if let ModelerSelection::Faces(f) = &state.selection { f.clone() } else { Vec::new() }
            } else {
                Vec::new()
            };

            for (idx, face) in mesh.faces.iter().enumerate() {
                // Use face center for box selection (average of transformed vertices)
                let world_positions: Vec<_> = face.vertices.iter()
                    .filter_map(|&vi| mesh.vertices.get(vi).map(|v| get_world_pos(v)))
                    .collect();
                if !world_positions.is_empty() {
                    let center = world_positions.iter().fold(Vec3::ZERO, |acc, &p| acc + p) * (1.0 / world_positions.len() as f32);
                    if let Some((sx, sy)) = world_to_screen_with_ortho(
                        center,
                        camera.position,
                        camera.basis_x,
                        camera.basis_y,
                        camera.basis_z,
                        fb_width,
                        fb_height,
                        ortho,
                    ) {
                        if sx >= fb_x0 && sx <= fb_x1 && sy >= fb_y0 && sy <= fb_y1 {
                            if !selected.contains(&idx) {
                                selected.push(idx);
                            }
                        }
                    }
                }
            }

            if !selected.is_empty() {
                let count = selected.len();
                state.set_selection(ModelerSelection::Faces(selected));
                state.set_status(&format!("Selected {} face(s)", count), 0.5);
            } else if !add_to_selection {
                state.set_selection(ModelerSelection::None);
            }
        }
        _ => {}
    }
}

/// Draw corner brackets around the selected object's bounding box
fn draw_selected_object_brackets(state: &ModelerState, fb: &mut Framebuffer) {
    // Only draw if an object is selected
    let obj = match state.selected_object() {
        Some(obj) if obj.visible => obj,
        _ => return,
    };

    // Compute bounding box
    let mesh = &obj.mesh;
    if mesh.vertices.is_empty() {
        return;
    }

    // Pre-compute all bone transforms for per-vertex skinning
    let bone_transforms: Vec<(Vec3, Vec3)> = (0..state.skeleton().len())
        .map(|i| state.get_bone_world_transform(i))
        .collect();

    let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    for v in &mesh.vertices {
        // Per-vertex bone assignment with fallback to mesh default
        let bone_idx = v.bone_index.or(obj.default_bone_index);
        let bone_transform = bone_idx.and_then(|idx| bone_transforms.get(idx)).copied();

        // Transform vertex to world space
        let pos = if let Some((bone_pos, bone_rot)) = bone_transform {
            rotate_by_euler(v.pos, bone_rot) + bone_pos
        } else {
            v.pos
        };
        min.x = min.x.min(pos.x);
        min.y = min.y.min(pos.y);
        min.z = min.z.min(pos.z);
        max.x = max.x.max(pos.x);
        max.y = max.y.max(pos.y);
        max.z = max.z.max(pos.z);
    }

    // Add margin to avoid z-fighting with mesh geometry
    let margin = 4.0;
    min.x -= margin;
    min.y -= margin;
    min.z -= margin;
    max.x += margin;
    max.y += margin;
    max.z += margin;

    // Bracket length: proportion of smallest box dimension
    let size = Vec3::new(max.x - min.x, max.y - min.y, max.z - min.z);
    let bracket_len = size.x.min(size.y).min(size.z) * 0.25;

    // Bracket color (cyan, matches accent color)
    let color = RasterColor::new(0, 200, 230);

    // Camera and projection info for screen-space conversion
    let camera = &state.camera;
    let ortho = state.raster_settings.ortho_projection.as_ref();

    // 8 corners of the box
    let corners = [
        Vec3::new(min.x, min.y, min.z), // 0: bottom-back-left
        Vec3::new(max.x, min.y, min.z), // 1: bottom-back-right
        Vec3::new(max.x, min.y, max.z), // 2: bottom-front-right
        Vec3::new(min.x, min.y, max.z), // 3: bottom-front-left
        Vec3::new(min.x, max.y, min.z), // 4: top-back-left
        Vec3::new(max.x, max.y, min.z), // 5: top-back-right
        Vec3::new(max.x, max.y, max.z), // 6: top-front-right
        Vec3::new(min.x, max.y, max.z), // 7: top-front-left
    ];

    // Direction vectors from each corner (normalized, towards adjacent corners)
    let dirs: [(usize, [Vec3; 3]); 8] = [
        (0, [Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 1.0)]),   // corner 0
        (1, [Vec3::new(-1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 1.0)]),  // corner 1
        (2, [Vec3::new(-1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, -1.0)]), // corner 2
        (3, [Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, -1.0)]),  // corner 3
        (4, [Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, -1.0, 0.0), Vec3::new(0.0, 0.0, 1.0)]),  // corner 4
        (5, [Vec3::new(-1.0, 0.0, 0.0), Vec3::new(0.0, -1.0, 0.0), Vec3::new(0.0, 0.0, 1.0)]), // corner 5
        (6, [Vec3::new(-1.0, 0.0, 0.0), Vec3::new(0.0, -1.0, 0.0), Vec3::new(0.0, 0.0, -1.0)]),// corner 6
        (7, [Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, -1.0, 0.0), Vec3::new(0.0, 0.0, -1.0)]), // corner 7
    ];

    // Draw 3 bracket lines from each corner with z-buffer testing
    for (corner_idx, edge_dirs) in &dirs {
        let corner = corners[*corner_idx];
        for dir in edge_dirs {
            let end = Vec3::new(
                corner.x + dir.x * bracket_len,
                corner.y + dir.y * bracket_len,
                corner.z + dir.z * bracket_len,
            );

            // Project both points to screen space with depth
            if let (Some((sx0, sy0, z0)), Some((sx1, sy1, z1))) = (
                world_to_screen_with_ortho_depth(corner, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height, ortho),
                world_to_screen_with_ortho_depth(end, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height, ortho),
            ) {
                fb.draw_line_3d(sx0 as i32, sy0 as i32, z0, sx1 as i32, sy1 as i32, z1, color);
            }
        }
    }
}

/// Draw selection and hover overlays for mesh editing (like world editor)
///
/// `world_vertices` - Pre-computed world-space vertex positions (with bone transforms applied).
/// If None, falls back to reading positions directly from mesh (for non-bone-bound meshes).
fn draw_mesh_selection_overlays(state: &ModelerState, fb: &mut Framebuffer, world_vertices: Option<&[Vec3]>) {
    // Skip if Mesh component is hidden
    let mesh_hidden = state.asset.components.iter()
        .enumerate()
        .find(|(_, c)| matches!(c, crate::asset::AssetComponent::Mesh { .. }))
        .map(|(idx, _)| state.is_component_hidden(idx))
        .unwrap_or(false);
    if mesh_hidden {
        return;
    }

    let mesh = state.mesh();
    let camera = &state.camera;
    let ortho = state.raster_settings.ortho_projection.as_ref();

    let hover_color = RasterColor::new(255, 200, 150);   // Orange for hover
    let select_color = RasterColor::new(100, 180, 255);  // Blue for selection
    let edge_overlay_color = RasterColor::new(80, 80, 80);  // Gray for edge overlay

    // Helper to get vertex world position - uses cached positions if available
    let get_pos = |idx: usize| -> Option<Vec3> {
        if let Some(positions) = world_vertices {
            positions.get(idx).copied()
        } else {
            mesh.vertices.get(idx).map(|v| v.pos)
        }
    };

    // =========================================================================
    // Draw all edges with semi-transparent overlay (always visible in solid mode)
    // Skip if in pure wireframe mode (edges already shown via backface_wireframe)
    // Uses depth testing so edges respect z-order
    // =========================================================================
    if !state.raster_settings.wireframe_overlay {
        for face in &mesh.faces {
            for (v0_idx, v1_idx) in face.edges() {
                if let (Some(p0), Some(p1)) = (get_pos(v0_idx), get_pos(v1_idx)) {
                    if let (Some((sx0, sy0, z0)), Some((sx1, sy1, z1))) = (
                        world_to_screen_with_ortho_depth(p0, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height, ortho),
                        world_to_screen_with_ortho_depth(p1, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height, ortho),
                    ) {
                        fb.draw_line_3d_alpha(sx0 as i32, sy0 as i32, z0, sx1 as i32, sy1 as i32, z1, edge_overlay_color, 191);
                    }
                }
            }
        }

        // Draw vertex dots with semi-transparent overlay
        let vertex_overlay_color = RasterColor::new(40, 40, 50);
        for idx in 0..mesh.vertices.len() {
            if let Some(pos) = get_pos(idx) {
                if let Some((sx, sy)) = world_to_screen_with_ortho(
                    pos,
                    camera.position,
                    camera.basis_x,
                    camera.basis_y,
                    camera.basis_z,
                    fb.width,
                    fb.height,
                    ortho,
                ) {
                    fb.draw_circle_alpha(sx as i32, sy as i32, 3, vertex_overlay_color, 140);
                }
            }
        }
    }

    // =========================================================================
    // Draw hovered vertex (if any) - orange dot
    // =========================================================================
    if let Some(hovered_idx) = state.hovered_vertex {
        if let Some(pos) = get_pos(hovered_idx) {
            if let Some((sx, sy)) = world_to_screen_with_ortho(
                pos,
                camera.position,
                camera.basis_x,
                camera.basis_y,
                camera.basis_z,
                fb.width,
                fb.height,
                ortho,
            ) {
                fb.draw_circle(sx as i32, sy as i32, 5, hover_color);
            }
        }
    }

    // =========================================================================
    // Draw hovered edge (if any) - orange line
    // =========================================================================
    if let Some((v0_idx, v1_idx)) = state.hovered_edge {
        if let (Some(p0), Some(p1)) = (get_pos(v0_idx), get_pos(v1_idx)) {
            if let (Some((sx0, sy0)), Some((sx1, sy1))) = (
                world_to_screen_with_ortho(p0, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height, ortho),
                world_to_screen_with_ortho(p1, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height, ortho),
            ) {
                fb.draw_line(sx0 as i32, sy0 as i32, sx1 as i32, sy1 as i32, hover_color);
                // Draw thicker by drawing adjacent lines
                fb.draw_line(sx0 as i32 + 1, sy0 as i32, sx1 as i32 + 1, sy1 as i32, hover_color);
                fb.draw_line(sx0 as i32, sy0 as i32 + 1, sx1 as i32, sy1 as i32 + 1, hover_color);
            }
        }
    }

    // =========================================================================
    // Draw hovered face (if any) - orange outline + center
    // =========================================================================
    if let Some(hovered_idx) = state.hovered_face {
        if let Some(face) = mesh.faces.get(hovered_idx) {
            // Draw face edges (all edges of n-gon)
            let screen_positions: Vec<_> = face.vertices.iter()
                .filter_map(|&vi| get_pos(vi))
                .filter_map(|p| world_to_screen_with_ortho(p, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height, ortho))
                .collect();

            let n = screen_positions.len();
            if n >= 3 {
                for i in 0..n {
                    let (sx0, sy0) = screen_positions[i];
                    let (sx1, sy1) = screen_positions[(i + 1) % n];
                    fb.draw_line(sx0 as i32, sy0 as i32, sx1 as i32, sy1 as i32, hover_color);
                }
                // For quads+, draw diagonals to indicate it's a face
                if n >= 4 {
                    let (sx0, sy0) = screen_positions[0];
                    let (sx2, sy2) = screen_positions[2];
                    fb.draw_line(sx0 as i32, sy0 as i32, sx2 as i32, sy2 as i32, hover_color);
                }
            }
        }
    }

    // =========================================================================
    // Draw selected vertices - blue dots
    // =========================================================================
    if let ModelerSelection::Vertices(selected_verts) = &state.selection {
        for &idx in selected_verts {
            if let Some(pos) = get_pos(idx) {
                if let Some((sx, sy)) = world_to_screen_with_ortho(
                    pos,
                    camera.position,
                    camera.basis_x,
                    camera.basis_y,
                    camera.basis_z,
                    fb.width,
                    fb.height,
                    ortho,
                ) {
                    fb.draw_circle(sx as i32, sy as i32, 4, select_color);
                }
            }
        }
    }

    // =========================================================================
    // Draw selected edges - blue lines
    // =========================================================================
    if let ModelerSelection::Edges(selected_edges) = &state.selection {
        for (v0_idx, v1_idx) in selected_edges {
            if let (Some(p0), Some(p1)) = (get_pos(*v0_idx), get_pos(*v1_idx)) {
                if let (Some((sx0, sy0)), Some((sx1, sy1))) = (
                    world_to_screen_with_ortho(p0, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height, ortho),
                    world_to_screen_with_ortho(p1, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height, ortho),
                ) {
                    fb.draw_line(sx0 as i32, sy0 as i32, sx1 as i32, sy1 as i32, select_color);
                    fb.draw_line(sx0 as i32 + 1, sy0 as i32, sx1 as i32 + 1, sy1 as i32, select_color);
                    // Draw endpoint dots
                    fb.draw_circle(sx0 as i32, sy0 as i32, 3, select_color);
                    fb.draw_circle(sx1 as i32, sy1 as i32, 3, select_color);
                }
            }
        }
    }

    // =========================================================================
    // Draw selected faces - blue outline
    // =========================================================================
    if let ModelerSelection::Faces(selected_faces) = &state.selection {
        for &face_idx in selected_faces {
            if let Some(face) = mesh.faces.get(face_idx) {
                // Collect world positions and screen positions
                let world_positions: Vec<_> = face.vertices.iter()
                    .filter_map(|&vi| get_pos(vi))
                    .collect();
                let screen_positions: Vec<_> = world_positions.iter()
                    .filter_map(|&p| world_to_screen_with_ortho(p, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height, ortho))
                    .collect();

                let n = screen_positions.len();
                if n >= 3 {
                    for i in 0..n {
                        let (sx0, sy0) = screen_positions[i];
                        let (sx1, sy1) = screen_positions[(i + 1) % n];
                        fb.draw_line(sx0 as i32, sy0 as i32, sx1 as i32, sy1 as i32, select_color);
                        // Thicker line
                        fb.draw_line(sx0 as i32 + 1, sy0 as i32, sx1 as i32 + 1, sy1 as i32, select_color);
                    }
                    // Draw center dot
                    let center = world_positions.iter().copied().fold(Vec3::ZERO, |acc, p| acc + p) * (1.0 / n as f32);
                    if let Some((cx, cy)) = world_to_screen_with_ortho(
                        center,
                        camera.position,
                        camera.basis_x,
                        camera.basis_y,
                        camera.basis_z,
                        fb.width,
                        fb.height,
                        ortho,
                    ) {
                        fb.draw_circle(cx as i32, cy as i32, 4, select_color);
                    }
                }
            }
        }
    }
}

/// Draw preview highlights for elements inside the box selection rectangle
fn draw_box_selection_preview(
    state: &ModelerState,
    fb: &mut Framebuffer,
    fb_x0: f32,
    fb_y0: f32,
    fb_x1: f32,
    fb_y1: f32,
    viewport_id: ViewportId,
    world_vertices: Option<&[Vec3]>,
) {
    let is_ortho = matches!(viewport_id, ViewportId::Top | ViewportId::Front | ViewportId::Side);

    // Set up camera and projection for this viewport
    let ortho_proj_temp;
    let ortho = if is_ortho {
        let ortho_cam = state.get_ortho_camera(viewport_id);
        ortho_proj_temp = Some(OrthoProjection {
            zoom: ortho_cam.zoom,
            center_x: ortho_cam.center.x,
            center_y: ortho_cam.center.y,
        });
        ortho_proj_temp.as_ref()
    } else {
        state.raster_settings.ortho_projection.as_ref()
    };

    let camera = if is_ortho {
        match viewport_id {
            ViewportId::Top => Camera::ortho_top(),
            ViewportId::Front => Camera::ortho_front(),
            ViewportId::Side => Camera::ortho_side(),
            ViewportId::Perspective => state.camera.clone(),
        }
    } else {
        state.camera.clone()
    };

    let mesh = state.mesh();
    let preview_color = RasterColor::new(255, 220, 100); // Yellow/gold for preview

    // Helper to get vertex world position - uses cached positions if available
    let get_pos = |idx: usize| -> Option<Vec3> {
        if let Some(positions) = world_vertices {
            positions.get(idx).copied()
        } else {
            mesh.vertices.get(idx).map(|v| v.pos)
        }
    };

    match state.select_mode {
        SelectMode::Vertex => {
            // Highlight vertices inside the box
            for idx in 0..mesh.vertices.len() {
                if let Some(pos) = get_pos(idx) {
                    if let Some((sx, sy)) = world_to_screen_with_ortho(
                        pos,
                        camera.position,
                        camera.basis_x,
                        camera.basis_y,
                        camera.basis_z,
                        fb.width,
                        fb.height,
                        ortho,
                    ) {
                        if sx >= fb_x0 && sx <= fb_x1 && sy >= fb_y0 && sy <= fb_y1 {
                            // Draw highlighted vertex
                            fb.draw_circle(sx as i32, sy as i32, 6, preview_color);
                        }
                    }
                }
            }
        }
        SelectMode::Edge => {
            // Highlight edges where both vertices are inside the box
            let mut drawn_edges = std::collections::HashSet::new();
            for face in &mesh.faces {
                for (v0_idx, v1_idx) in face.edges() {
                    let edge = (v0_idx.min(v1_idx), v0_idx.max(v1_idx));
                    if drawn_edges.contains(&edge) {
                        continue;
                    }

                    if let (Some(p0), Some(p1)) = (get_pos(v0_idx), get_pos(v1_idx)) {
                        if let (Some((sx0, sy0)), Some((sx1, sy1))) = (
                            world_to_screen_with_ortho(p0, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height, ortho),
                            world_to_screen_with_ortho(p1, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height, ortho),
                        ) {
                            // Check if midpoint is inside box
                            let mid_x = (sx0 + sx1) / 2.0;
                            let mid_y = (sy0 + sy1) / 2.0;
                            if mid_x >= fb_x0 && mid_x <= fb_x1 && mid_y >= fb_y0 && mid_y <= fb_y1 {
                                fb.draw_line(sx0 as i32, sy0 as i32, sx1 as i32, sy1 as i32, preview_color);
                                fb.draw_line(sx0 as i32 + 1, sy0 as i32, sx1 as i32 + 1, sy1 as i32, preview_color);
                                drawn_edges.insert(edge);
                            }
                        }
                    }
                }
            }
        }
        SelectMode::Face => {
            // Highlight faces where center is inside the box
            for face in &mesh.faces {
                let world_positions: Vec<_> = face.vertices.iter()
                    .filter_map(|&vi| get_pos(vi))
                    .collect();
                if !world_positions.is_empty() {
                    let center = world_positions.iter().copied().fold(Vec3::ZERO, |acc, p| acc + p) * (1.0 / world_positions.len() as f32);
                    if let Some((cx, cy)) = world_to_screen_with_ortho(
                        center,
                        camera.position,
                        camera.basis_x,
                        camera.basis_y,
                        camera.basis_z,
                        fb.width,
                        fb.height,
                        ortho,
                    ) {
                        if cx >= fb_x0 && cx <= fb_x1 && cy >= fb_y0 && cy <= fb_y1 {
                            // Draw face outline in preview color
                            let screen_positions: Vec<_> = world_positions.iter()
                                .filter_map(|&p| world_to_screen_with_ortho(p, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height, ortho))
                                .collect();
                            let n = screen_positions.len();
                            if n >= 3 {
                                for i in 0..n {
                                    let (sx0, sy0) = screen_positions[i];
                                    let (sx1, sy1) = screen_positions[(i + 1) % n];
                                    fb.draw_line(sx0 as i32, sy0 as i32, sx1 as i32, sy1 as i32, preview_color);
                                }
                                // Draw center dot
                                fb.draw_circle(cx as i32, cy as i32, 4, preview_color);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Handle mesh selection click
fn handle_mesh_selection_click(
    state: &mut ModelerState,
    mouse_pos: (f32, f32),
    draw_x: f32, draw_y: f32,
    draw_w: f32, draw_h: f32,
    fb_width: usize, fb_height: usize,
) {
    // Convert screen to framebuffer coords
    let fb_x = (mouse_pos.0 - draw_x) / draw_w * fb_width as f32;
    let fb_y = (mouse_pos.1 - draw_y) / draw_h * fb_height as f32;

    let camera = &state.camera;
    let mesh = state.mesh();
    let ortho = state.raster_settings.ortho_projection.as_ref();

    match state.select_mode {
        SelectMode::Vertex => {
            // Find closest vertex
            let mut best_idx = None;
            let mut best_dist = 20.0_f32; // Max distance in pixels

            for (idx, vert) in mesh.vertices.iter().enumerate() {
                if let Some((sx, sy)) = world_to_screen_with_ortho(
                    vert.pos,
                    camera.position,
                    camera.basis_x,
                    camera.basis_y,
                    camera.basis_z,
                    fb_width,
                    fb_height,
                    ortho,
                ) {
                    let dist = ((sx - fb_x).powi(2) + (sy - fb_y).powi(2)).sqrt();
                    if dist < best_dist {
                        best_dist = dist;
                        best_idx = Some(idx);
                    }
                }
            }

            if let Some(idx) = best_idx {
                // Multi-select with Shift OR X key (PicoCAD uses X)
                let multi_select = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift)
                                || is_key_down(KeyCode::X);
                if multi_select {
                    // Toggle selection - save undo first
                    state.save_selection_undo();
                    if let ModelerSelection::Vertices(ref mut verts) = state.selection {
                        if let Some(pos) = verts.iter().position(|&v| v == idx) {
                            verts.remove(pos);
                        } else {
                            verts.push(idx);
                        }
                    } else {
                        state.selection = ModelerSelection::Vertices(vec![idx]);
                    }
                } else {
                    state.set_selection(ModelerSelection::Vertices(vec![idx]));
                }
            } else if !is_key_down(KeyCode::X) {
                // Only clear selection if not holding X (multi-select mode)
                state.set_selection(ModelerSelection::None);
            }
        }
        SelectMode::Face => {
            // Find clicked face by checking face centers
            let mut best_idx = None;
            let mut best_dist = 30.0_f32;

            for (idx, face) in mesh.faces.iter().enumerate() {
                // Calculate center from all vertices of n-gon
                let verts: Vec<_> = face.vertices.iter()
                    .filter_map(|&vi| mesh.vertices.get(vi))
                    .collect();
                if !verts.is_empty() {
                    let center = verts.iter().map(|v| v.pos).fold(Vec3::ZERO, |acc, p| acc + p) * (1.0 / verts.len() as f32);
                    if let Some((sx, sy)) = world_to_screen_with_ortho(
                        center,
                        camera.position,
                        camera.basis_x,
                        camera.basis_y,
                        camera.basis_z,
                        fb_width,
                        fb_height,
                        ortho,
                    ) {
                        let dist = ((sx - fb_x).powi(2) + (sy - fb_y).powi(2)).sqrt();
                        if dist < best_dist {
                            best_dist = dist;
                            best_idx = Some(idx);
                        }
                    }
                }
            }

            if let Some(idx) = best_idx {
                // Multi-select with Shift OR X key (PicoCAD uses X)
                let multi_select = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift)
                                || is_key_down(KeyCode::X);
                if multi_select {
                    // Toggle selection - save undo first
                    state.save_selection_undo();
                    if let ModelerSelection::Faces(ref mut faces) = state.selection {
                        if let Some(pos) = faces.iter().position(|&f| f == idx) {
                            faces.remove(pos);
                        } else {
                            faces.push(idx);
                        }
                    } else {
                        state.selection = ModelerSelection::Faces(vec![idx]);
                    }
                } else {
                    state.set_selection(ModelerSelection::Faces(vec![idx]));
                }
            } else if !is_key_down(KeyCode::X) {
                // Only clear selection if not holding X (multi-select mode)
                state.set_selection(ModelerSelection::None);
            }
        }
        _ => {}
    }
}

// =============================================================================
// Hover Detection System (like world editor)
// =============================================================================

/// Detect which element is under the cursor, with priority: vertex > edge > face
/// This enables hover-to-highlight, click-to-select workflow like the world editor.
fn find_hovered_element(
    state: &ModelerState,
    mouse_fb: (f32, f32),
    fb_width: usize,
    fb_height: usize,
) -> (Option<usize>, Option<(usize, usize)>, Option<usize>) {
    // Skip if Mesh component is hidden - nothing to hover
    let mesh_hidden = state.asset.components.iter()
        .enumerate()
        .find(|(_, c)| matches!(c, crate::asset::AssetComponent::Mesh { .. }))
        .map(|(idx, _)| state.is_component_hidden(idx))
        .unwrap_or(false);
    if mesh_hidden {
        return (None, None, None);
    }

    let (mouse_fb_x, mouse_fb_y) = mouse_fb;
    let camera = &state.camera;
    let mesh = state.mesh();
    let ortho = state.raster_settings.ortho_projection.as_ref();

    // Pre-compute all bone transforms for per-vertex skinning
    let bone_transforms: Vec<(Vec3, Vec3)> = (0..state.skeleton().len())
        .map(|i| state.get_bone_world_transform(i))
        .collect();

    // Get mesh's default bone index
    let default_bone_idx = state.selected_object().and_then(|obj| obj.default_bone_index);

    // Helper to transform vertex position to world space (per-vertex bone)
    let get_world_pos = |idx: usize| -> Option<Vec3> {
        mesh.vertices.get(idx).map(|v| {
            // Per-vertex bone assignment with fallback to mesh default
            let bone_idx = v.bone_index.or(default_bone_idx);
            let bone_transform = bone_idx.and_then(|idx| bone_transforms.get(idx)).copied();

            if let Some((bone_pos, bone_rot)) = bone_transform {
                rotate_by_euler(v.pos, bone_rot) + bone_pos
            } else {
                v.pos
            }
        })
    };

    // Check if current object is double-sided (allow selecting backfaces)
    let double_sided = state.selected_object()
        .map(|obj| obj.double_sided)
        .unwrap_or(false);

    const VERTEX_THRESHOLD: f32 = 6.0;
    const EDGE_THRESHOLD: f32 = 4.0;

    let mut hovered_vertex: Option<(usize, f32)> = None; // (index, distance)
    let mut hovered_edge: Option<((usize, usize), f32)> = None;
    let mut hovered_face: Option<(usize, f32)> = None; // (index, depth) - depth for Z-ordering

    // Precompute which vertices are on front-facing faces (for backface culling)
    let mut vertex_on_front_face = vec![false; mesh.vertices.len()];
    let mut edge_on_front_face = std::collections::HashSet::<(usize, usize)>::new();

    for face in &mesh.faces {
        // For backface culling, use first 3 vertices (like face_normal calculation)
        if face.vertices.len() >= 3 {
            if let (Some(p0), Some(p1), Some(p2)) = (
                get_world_pos(face.vertices[0]),
                get_world_pos(face.vertices[1]),
                get_world_pos(face.vertices[2]),
            ) {
                // Use screen-space signed area for backface culling (same as rasterizer)
                if let (Some((sx0, sy0)), Some((sx1, sy1)), Some((sx2, sy2))) = (
                    world_to_screen_with_ortho(p0, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height, ortho),
                    world_to_screen_with_ortho(p1, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height, ortho),
                    world_to_screen_with_ortho(p2, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height, ortho),
                ) {
                    // 2D screen-space signed area (PS1-style) - positive = front-facing
                    let signed_area = (sx1 - sx0) * (sy2 - sy0) - (sx2 - sx0) * (sy1 - sy0);

                    if signed_area > 0.0 {
                        // Front-facing: mark all vertices and edges of n-gon
                        for &vi in &face.vertices {
                            if vi < vertex_on_front_face.len() {
                                vertex_on_front_face[vi] = true;
                            }
                        }

                        // Add all edges of n-gon (normalized order)
                        for (v0_idx, v1_idx) in face.edges() {
                            let e = (v0_idx.min(v1_idx), v0_idx.max(v1_idx));
                            edge_on_front_face.insert(e);
                        }
                    }
                }
            }
        }
    }

    // Check vertices first (highest priority) - only if on front-facing face (unless X-ray or double-sided)
    for idx in 0..mesh.vertices.len() {
        if !state.xray_mode && !double_sided && !vertex_on_front_face[idx] {
            continue; // Skip vertices only on backfaces (X-ray/double-sided allows selecting through)
        }
        if let Some(pos) = get_world_pos(idx) {
            // Skip vertices on the non-editable side when mirror is enabled
            // Note: mirror check uses local position since mirror plane is in local space
            let local_pos = mesh.vertices[idx].pos;
            if !state.current_mirror_settings().is_editable_side(local_pos) {
                continue;
            }
            if let Some((sx, sy)) = world_to_screen_with_ortho(
                pos,
                camera.position,
                camera.basis_x,
                camera.basis_y,
                camera.basis_z,
                fb_width,
                fb_height,
                ortho,
            ) {
                let dist = ((mouse_fb_x - sx).powi(2) + (mouse_fb_y - sy).powi(2)).sqrt();
                if dist < VERTEX_THRESHOLD {
                    if hovered_vertex.map_or(true, |(_, best_dist)| dist < best_dist) {
                        hovered_vertex = Some((idx, dist));
                    }
                }
            }
        }
    }

    // If no vertex hovered, check edges - only if on front-facing face
    if hovered_vertex.is_none() {
        // Collect edges from faces (iterate over n-gon edges)
        for face in &mesh.faces {
            for (v0_idx, v1_idx) in face.edges() {
                // Normalize edge order for consistency
                let edge = if v0_idx < v1_idx { (v0_idx, v1_idx) } else { (v1_idx, v0_idx) };

                // Skip edges only on backfaces (unless X-ray or double-sided)
                if !state.xray_mode && !double_sided && !edge_on_front_face.contains(&edge) {
                    continue;
                }

                if let (Some(p0), Some(p1)) = (get_world_pos(v0_idx), get_world_pos(v1_idx)) {
                    // Skip edges with any vertex on non-editable side when mirror is enabled
                    // Note: mirror check uses local positions
                    let local_p0 = mesh.vertices[v0_idx].pos;
                    let local_p1 = mesh.vertices[v1_idx].pos;
                    if !state.current_mirror_settings().is_editable_side(local_p0) || !state.current_mirror_settings().is_editable_side(local_p1) {
                        continue;
                    }
                    if let (Some((sx0, sy0)), Some((sx1, sy1))) = (
                        world_to_screen_with_ortho(p0, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height, ortho),
                        world_to_screen_with_ortho(p1, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height, ortho),
                    ) {
                        let dist = point_to_line_distance(mouse_fb_x, mouse_fb_y, sx0, sy0, sx1, sy1);
                        if dist < EDGE_THRESHOLD {
                            if hovered_edge.map_or(true, |(_, best_dist)| dist < best_dist) {
                                hovered_edge = Some((edge, dist));
                            }
                        }
                    }
                }
            }
        }
    }

    // If no vertex or edge hovered, check faces using point-in-triangle (triangulate n-gons)
    if hovered_vertex.is_none() && hovered_edge.is_none() {
        for (idx, face) in mesh.faces.iter().enumerate() {
            // Skip faces with any vertex on non-editable side when mirror is enabled
            // Note: mirror check uses local positions
            let face_on_editable_side = face.vertices.iter().all(|&vi| {
                mesh.vertices.get(vi)
                    .map(|v| state.current_mirror_settings().is_editable_side(v.pos))
                    .unwrap_or(false)
            });
            if !face_on_editable_side {
                continue;
            }
            // Triangulate the n-gon face and check each triangle
            for [i0, i1, i2] in face.triangulate() {
                if let (Some(p0), Some(p1), Some(p2)) = (
                    get_world_pos(i0),
                    get_world_pos(i1),
                    get_world_pos(i2),
                ) {
                    // Use screen-space signed area for backface culling (same as rasterizer)
                    if let (Some((sx0, sy0)), Some((sx1, sy1)), Some((sx2, sy2))) = (
                        world_to_screen_with_ortho(p0, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height, ortho),
                        world_to_screen_with_ortho(p1, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height, ortho),
                        world_to_screen_with_ortho(p2, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height, ortho),
                    ) {
                        // 2D screen-space signed area (PS1-style) - positive = front-facing
                        let signed_area = (sx1 - sx0) * (sy2 - sy0) - (sx2 - sx0) * (sy1 - sy0);
                        if !state.xray_mode && !double_sided && signed_area <= 0.0 {
                            continue; // Backface - skip (X-ray/double-sided allows selecting through)
                        }

                        // Check if mouse is inside the triangle
                        if point_in_triangle_2d(mouse_fb_x, mouse_fb_y, sx0, sy0, sx1, sy1, sx2, sy2) {
                            // Calculate depth at mouse position for Z-ordering
                            let depth = interpolate_depth_in_triangle(
                                mouse_fb_x, mouse_fb_y,
                                sx0, sy0, (p0 - camera.position).dot(camera.basis_z),
                                sx1, sy1, (p1 - camera.position).dot(camera.basis_z),
                                sx2, sy2, (p2 - camera.position).dot(camera.basis_z),
                            );
                            // Pick the closest (smallest depth) face
                            if hovered_face.map_or(true, |(_, best_depth)| depth < best_depth) {
                                hovered_face = Some((idx, depth));
                            }
                        }
                    }
                }
            }
        }
    }

    (
        hovered_vertex.map(|(idx, _)| idx),
        hovered_edge.map(|(edge, _)| edge),
        hovered_face.map(|(idx, _)| idx),
    )
}

/// Calculate distance from point to line segment
fn point_to_line_distance(px: f32, py: f32, x0: f32, y0: f32, x1: f32, y1: f32) -> f32 {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len_sq = dx * dx + dy * dy;

    if len_sq < 0.001 {
        // Line is effectively a point
        return ((px - x0).powi(2) + (py - y0).powi(2)).sqrt();
    }

    // Project point onto line, clamped to segment
    let t = ((px - x0) * dx + (py - y0) * dy) / len_sq;
    let t = t.clamp(0.0, 1.0);

    let proj_x = x0 + t * dx;
    let proj_y = y0 + t * dy;

    ((px - proj_x).powi(2) + (py - proj_y).powi(2)).sqrt()
}

/// Interpolate depth at a point inside a triangle using barycentric coordinates
fn interpolate_depth_in_triangle(
    px: f32, py: f32,
    x0: f32, y0: f32, d0: f32,
    x1: f32, y1: f32, d1: f32,
    x2: f32, y2: f32, d2: f32,
) -> f32 {
    // Signed area of full triangle (v0, v1, v2)
    let area = (x1 - x0) * (y2 - y0) - (x2 - x0) * (y1 - y0);
    if area.abs() < 0.0001 {
        // Degenerate triangle, just return average depth
        return (d0 + d1 + d2) / 3.0;
    }

    // Barycentric coordinates using signed areas of sub-triangles
    let w0 = ((x1 - px) * (y2 - py) - (x2 - px) * (y1 - py)) / area;
    let w1 = ((x2 - px) * (y0 - py) - (x0 - px) * (y2 - py)) / area;
    let w2 = 1.0 - w0 - w1;

    // Interpolate depth
    w0 * d0 + w1 * d1 + w2 * d2
}

/// Update hover state based on current mouse position
fn update_hover_state(
    state: &mut ModelerState,
    mouse_pos: (f32, f32),
    draw_x: f32, draw_y: f32,
    draw_w: f32, draw_h: f32,
    fb_width: usize, fb_height: usize,
    viewport_id: ViewportId,
) {
    // Don't update hover during transforms or box select
    if state.modal_transform != ModalTransform::None || state.drag_manager.is_dragging() {
        state.hovered_vertex = None;
        state.hovered_edge = None;
        state.hovered_face = None;
        state.hovered_bone = None;
        state.hovered_bone_tip = None;
        return;
    }

    // Check if mouse is inside viewport
    let inside = mouse_pos.0 >= draw_x && mouse_pos.0 < draw_x + draw_w
              && mouse_pos.1 >= draw_y && mouse_pos.1 < draw_y + draw_h;

    if !inside {
        state.hovered_vertex = None;
        state.hovered_edge = None;
        state.hovered_face = None;
        state.hovered_bone = None;
        state.hovered_bone_tip = None;
        return;
    }

    // Convert screen to framebuffer coords
    let fb_x = (mouse_pos.0 - draw_x) / draw_w * fb_width as f32;
    let fb_y = (mouse_pos.1 - draw_y) / draw_h * fb_height as f32;

    // Determine which component type is selected for hover isolation
    let selected_component_type = state.selected_component
        .and_then(|idx| state.asset.components.get(idx));
    let mesh_component_selected = selected_component_type.map(|c| c.is_mesh()).unwrap_or(false);
    let skeleton_component_selected = selected_component_type.map(|c| c.is_skeleton()).unwrap_or(false);

    // Clear all hover states first
    state.hovered_bone = None;
    state.hovered_bone_tip = None;
    state.hovered_vertex = None;
    state.hovered_edge = None;
    state.hovered_face = None;

    // Only check bone hover when skeleton component is selected
    if skeleton_component_selected && state.show_bones && !state.skeleton().is_empty() {
        let (base, tip) = find_hovered_bone_part(state, (fb_x, fb_y), fb_width, fb_height, viewport_id);
        state.hovered_bone = base;
        state.hovered_bone_tip = tip;
    }

    // Only check mesh hover when mesh component is selected (and not hovering over bone)
    if mesh_component_selected && state.hovered_bone.is_none() && state.hovered_bone_tip.is_none() {
        let (vert, edge, face) = find_hovered_element(state, (fb_x, fb_y), fb_width, fb_height);
        state.hovered_vertex = vert;
        state.hovered_edge = edge;
        state.hovered_face = face;
    }
}

/// Find the bone part under the cursor: (base_hover, tip_hover)
/// Returns which bone's base or tip is being hovered (mutually exclusive)
fn find_hovered_bone_part(
    state: &ModelerState,
    fb_pos: (f32, f32),
    fb_width: usize,
    fb_height: usize,
    viewport_id: ViewportId,
) -> (Option<usize>, Option<usize>) {
    use crate::rasterizer::{Camera, OrthoProjection};

    let skeleton = state.skeleton();
    if skeleton.is_empty() {
        return (None, None);
    }

    // Use ortho camera for ortho viewports, perspective camera otherwise
    let is_ortho = viewport_id != ViewportId::Perspective;

    // Construct camera and ortho projection based on viewport type
    let (camera, ortho) = if is_ortho {
        let ortho_cam = state.get_ortho_camera(viewport_id);
        let view_distance = 50000.0;
        let mut cam = match viewport_id {
            ViewportId::Top => Camera::ortho_top(),
            ViewportId::Front => Camera::ortho_front(),
            ViewportId::Side => Camera::ortho_side(),
            ViewportId::Perspective => unreachable!(),
        };
        match viewport_id {
            ViewportId::Top => cam.position = Vec3::new(0.0, view_distance, 0.0),
            ViewportId::Front => cam.position = Vec3::new(0.0, 0.0, view_distance),
            ViewportId::Side => cam.position = Vec3::new(view_distance, 0.0, 0.0),
            ViewportId::Perspective => unreachable!(),
        }
        let ortho_proj = OrthoProjection {
            zoom: ortho_cam.zoom,
            center_x: ortho_cam.center.x,
            center_y: ortho_cam.center.y,
        };
        (cam, Some(ortho_proj))
    } else {
        (state.camera.clone(), None)
    };
    let ortho_ref = ortho.as_ref();

    // First, check if hovering near any bone's base or tip (screen space)
    const TIP_RADIUS: f32 = 12.0;  // Screen pixels for tip/base hover

    let mut closest_base: Option<(usize, f32)> = None;
    let mut closest_tip: Option<(usize, f32)> = None;

    for (idx, _bone) in skeleton.iter().enumerate() {
        let (base_pos, _) = state.get_bone_world_transform(idx);
        let tip_pos = state.get_bone_tip_position(idx);

        // Project base and tip to screen
        if let Some((base_sx, base_sy)) = world_to_screen_with_ortho(
            base_pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z,
            fb_width, fb_height, ortho_ref
        ) {
            let base_dist = ((fb_pos.0 - base_sx).powi(2) + (fb_pos.1 - base_sy).powi(2)).sqrt();
            if base_dist < TIP_RADIUS {
                if closest_base.is_none() || base_dist < closest_base.unwrap().1 {
                    closest_base = Some((idx, base_dist));
                }
            }
        }

        if let Some((tip_sx, tip_sy)) = world_to_screen_with_ortho(
            tip_pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z,
            fb_width, fb_height, ortho_ref
        ) {
            let tip_dist = ((fb_pos.0 - tip_sx).powi(2) + (fb_pos.1 - tip_sy).powi(2)).sqrt();
            if tip_dist < TIP_RADIUS {
                if closest_tip.is_none() || tip_dist < closest_tip.unwrap().1 {
                    closest_tip = Some((idx, tip_dist));
                }
            }
        }
    }

    // Tip takes priority over base (more precise selection)
    if let Some((tip_idx, tip_dist)) = closest_tip {
        if let Some((base_idx, base_dist)) = closest_base {
            // Both are close - pick the closer one
            if tip_dist <= base_dist {
                return (None, Some(tip_idx));
            } else {
                return (Some(base_idx), None);
            }
        }
        return (None, Some(tip_idx));
    }

    if let Some((base_idx, _)) = closest_base {
        return (Some(base_idx), None);
    }

    // If not hovering on base/tip, check bone body using ray intersection
    let ray = screen_to_ray(fb_pos.0, fb_pos.1, fb_width, fb_height, &camera);

    let mut closest_bone: Option<usize> = None;
    let mut closest_dist = f32::MAX;

    for (idx, bone) in skeleton.iter().enumerate() {
        let (base_pos, _) = state.get_bone_world_transform(idx);
        let tip_pos = state.get_bone_tip_position(idx);
        let pick_radius = (bone.length * 0.15).clamp(20.0, 200.0);

        if let Some(dist) = ray_bone_intersect(ray.origin, ray.direction, base_pos, tip_pos, pick_radius) {
            if dist < closest_dist {
                closest_dist = dist;
                closest_bone = Some(idx);
            }
        }
    }

    // Clicking on bone body selects the base (move whole bone)
    (closest_bone, None)
}

/// Calculate Euler rotation (X, Z) to point from default Y-up to a target direction
fn direction_to_rotation(dir: Vec3) -> Vec3 {
    let len = dir.len();
    if len < 0.001 {
        return Vec3::ZERO;
    }
    let d = dir * (1.0 / len);

    // X rotation: pitch (tilt forward/back)
    let rot_x = (-d.z).atan2((d.x * d.x + d.y * d.y).sqrt()).to_degrees();
    // Z rotation: yaw (turn left/right)
    let rot_z = d.x.atan2(d.y).to_degrees();

    Vec3::new(rot_x, 0.0, rot_z)
}

/// Handle skeleton interaction in viewport - bone tip dragging
fn handle_skeleton_interaction(
    ctx: &UiContext,
    state: &mut ModelerState,
    mouse_pos: (f32, f32),
    draw_x: f32, draw_y: f32,
    draw_w: f32, draw_h: f32,
    fb_width: usize, fb_height: usize,
    viewport_id: ViewportId,
) {
    // Convert screen to framebuffer coords
    let fb_x = (mouse_pos.0 - draw_x) / draw_w * fb_width as f32;
    let fb_y = (mouse_pos.1 - draw_y) / draw_h * fb_height as f32;
    let is_ortho = viewport_id != ViewportId::Perspective;

    // Get 3D position from screen coords
    let world_pos = if is_ortho {
        // Ortho mode: convert screen position directly to world position
        let ortho_cam = state.get_ortho_camera(viewport_id);
        let zoom = ortho_cam.zoom;
        let center = ortho_cam.center;

        // Screen center to world offset
        let half_w = fb_width as f32 / 2.0;
        let half_h = fb_height as f32 / 2.0;
        let world_x = (fb_x - half_w) / zoom + center.x;
        let world_y = -(fb_y - half_h) / zoom + center.y; // Y inverted

        // Get bone's third coordinate (the one perpendicular to this view)
        let bone_coord = state.selected_bone
            .map(|idx| state.get_bone_tip_position(idx))
            .unwrap_or(Vec3::ZERO);

        match viewport_id {
            ViewportId::Top => Vec3::new(world_x, bone_coord.y, world_y),    // XZ plane, keep Y
            ViewportId::Front => Vec3::new(world_x, world_y, bone_coord.z),  // XY plane, keep Z
            ViewportId::Side => Vec3::new(bone_coord.x, world_y, world_x),   // YZ plane, keep X
            ViewportId::Perspective => Vec3::ZERO,
        }
    } else {
        // Perspective mode: use ray casting onto a plane
        let ray = screen_to_ray(fb_x, fb_y, fb_width, fb_height, &state.camera);

        if let Some(bone_idx) = state.selected_bone {
            let (base_pos, _) = state.get_bone_world_transform(bone_idx);
            // Find intersection with plane at base_pos perpendicular to camera
            let plane_normal = state.camera.basis_z;
            let plane_d = base_pos.dot(plane_normal);
            let denom = ray.direction.dot(plane_normal);
            if denom.abs() > 0.001 {
                let t = (plane_d - ray.origin.dot(plane_normal)) / denom;
                if t > 0.0 {
                    ray.origin + ray.direction * t
                } else {
                    ray.origin + ray.direction * 500.0
                }
            } else {
                ray.origin + ray.direction * 500.0
            }
        } else {
            ray.origin + ray.direction * 500.0
        }
    };

    // Check if we should start dragging a bone tip
    // Don't start if gizmo is being used or already dragging
    if ctx.mouse.left_pressed && state.bone_creation.is_none()
        && state.gizmo_hovered_axis.is_none() && !state.drag_manager.is_dragging() {
        if let Some(bone_idx) = state.selected_bone {
            let tip_pos = state.get_bone_tip_position(bone_idx);
            let (base_pos, _) = state.get_bone_world_transform(bone_idx);

            // Check if click is near the tip (in screen space)
            let tip_screen = world_to_screen_with_ortho_depth(
                tip_pos,
                state.camera.position,
                state.camera.basis_x,
                state.camera.basis_y,
                state.camera.basis_z,
                fb_width,
                fb_height,
                state.raster_settings.ortho_projection.as_ref(),
            );

            if let Some((tip_sx, tip_sy, _)) = tip_screen {
                let dist = ((fb_x - tip_sx).powi(2) + (fb_y - tip_sy).powi(2)).sqrt();
                if dist < 20.0 {
                    // Compute offset so the tip doesn't snap to mouse position
                    // drag_offset = tip_pos - world_pos, so during update: new_tip = world_pos + drag_offset
                    let drag_offset = tip_pos - world_pos;
                    // Save undo before starting drag
                    state.save_undo_skeleton("Move Bone Tip");
                    // Start tip drag - use bone_creation state to track the drag
                    state.bone_creation = Some(super::state::BoneCreationState {
                        parent: Some(bone_idx), // Store which bone we're editing
                        start_pos: base_pos,
                        end_pos: tip_pos,
                        drag_offset,
                    });
                    state.set_status("Drag to adjust bone", 1.0);
                }
            }
        }
    }

    // Update drag
    if ctx.mouse.left_down {
        if let Some(ref mut creation) = state.bone_creation {
            if let Some(bone_idx) = creation.parent {
                // Apply drag_offset to prevent snapping to mouse position
                // new_tip = world_pos + drag_offset (where drag_offset = original_tip - initial_world_pos)
                let adjusted_pos = world_pos + creation.drag_offset;

                // Apply snapping to the tip position (Z key disables snap)
                let snap_disabled = is_key_down(KeyCode::Z);
                let snap_enabled = state.snap_settings.enabled && !snap_disabled;
                let snapped_pos = if snap_enabled {
                    state.snap_settings.snap_vec3(adjusted_pos)
                } else {
                    adjusted_pos
                };

                // Update the tip position
                creation.end_pos = snapped_pos;

                // Calculate new bone direction and length
                let bone_vec = creation.end_pos - creation.start_pos;
                let new_length = bone_vec.len().max(20.0); // Minimum length

                // direction_to_rotation gives WORLD rotation, but we need LOCAL rotation
                // For child bones, subtract parent's accumulated rotation
                let world_rotation = direction_to_rotation(bone_vec);
                let parent_rotation = state.skeleton().get(bone_idx)
                    .and_then(|b| b.parent)
                    .map(|parent_idx| {
                        let (_, parent_rot) = state.get_bone_world_transform(parent_idx);
                        parent_rot
                    })
                    .unwrap_or(Vec3::ZERO);
                let new_rotation = world_rotation - parent_rotation;

                // Apply to the bone
                let old_length = state.skeleton().get(bone_idx).map(|b| b.length).unwrap_or(0.0);
                if let Some(bones) = state.asset.skeleton_mut() {
                    if let Some(bone) = bones.get_mut(bone_idx) {
                        bone.length = new_length;
                        bone.local_rotation = new_rotation;
                        state.dirty = true;
                    }
                    // Smart mode: only update children that were at the tip
                    for bone in bones.iter_mut() {
                        if bone.parent == Some(bone_idx) {
                            let was_at_tip = (bone.local_position.y - old_length).abs() < 1.0;
                            if was_at_tip {
                                bone.local_position.y = new_length;
                            }
                        }
                    }
                }
            }
        }
    }

    // End drag
    if !ctx.mouse.left_down && state.bone_creation.is_some() {
        state.bone_creation = None;
        state.set_status("Bone adjusted", 1.0);
    }
}

/// Handle click on hovered element (replaces mode-based selection)
fn handle_hover_click(state: &mut ModelerState) {
    // Multi-select with Shift OR X key
    let multi_select = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift)
                    || is_key_down(KeyCode::X);

    // Handle bone TIP selection (click on tip = change direction/length)
    if let Some(bone_idx) = state.hovered_bone_tip {
        if multi_select {
            match &mut state.selection {
                ModelerSelection::BoneTips(tips) => {
                    if let Some(pos) = tips.iter().position(|&b| b == bone_idx) {
                        tips.remove(pos);
                        state.selected_bone = tips.first().copied();
                    } else {
                        tips.push(bone_idx);
                        state.selected_bone = Some(bone_idx);
                    }
                }
                _ => {
                    state.selection = ModelerSelection::BoneTips(vec![bone_idx]);
                    state.selected_bone = Some(bone_idx);
                }
            }
        } else {
            state.selection = ModelerSelection::BoneTips(vec![bone_idx]);
            state.selected_bone = Some(bone_idx);
        }

        let skeleton_comp_idx = state.asset.components.iter().position(|c| c.is_skeleton());
        if skeleton_comp_idx.is_some() {
            state.selected_component = skeleton_comp_idx;
        }

        let bone_name = state.skeleton().get(bone_idx)
            .map(|b| b.name.clone())
            .unwrap_or_default();
        state.set_status(&format!("Selected tip: {} (G to rotate/resize)", bone_name), 1.0);
        return;
    }

    // Handle bone BASE selection (click on base/body = move whole bone)
    if let Some(bone_idx) = state.hovered_bone {
        if multi_select {
            match &mut state.selection {
                ModelerSelection::Bones(bones) => {
                    if let Some(pos) = bones.iter().position(|&b| b == bone_idx) {
                        bones.remove(pos);
                        state.selected_bone = bones.first().copied();
                    } else {
                        bones.push(bone_idx);
                        state.selected_bone = Some(bone_idx);
                    }
                }
                _ => {
                    state.selection = ModelerSelection::Bones(vec![bone_idx]);
                    state.selected_bone = Some(bone_idx);
                }
            }
        } else {
            state.selection = ModelerSelection::Bones(vec![bone_idx]);
            state.selected_bone = Some(bone_idx);
        }

        let skeleton_comp_idx = state.asset.components.iter().position(|c| c.is_skeleton());
        if skeleton_comp_idx.is_some() {
            state.selected_component = skeleton_comp_idx;
        }

        let bone_name = state.skeleton().get(bone_idx)
            .map(|b| b.name.clone())
            .unwrap_or_default();
        state.set_status(&format!("Selected bone: {} (G to move)", bone_name), 1.0);
        return;
    }

    // Priority: vertex > edge > face
    if let Some(vert_idx) = state.hovered_vertex {
        if multi_select {
            // Toggle vertex in selection - save undo first
            state.save_selection_undo();
            match &mut state.selection {
                ModelerSelection::Vertices(verts) => {
                    if let Some(pos) = verts.iter().position(|&v| v == vert_idx) {
                        verts.remove(pos);
                    } else {
                        verts.push(vert_idx);
                    }
                }
                _ => {
                    state.selection = ModelerSelection::Vertices(vec![vert_idx]);
                }
            }
        } else {
            state.set_selection(ModelerSelection::Vertices(vec![vert_idx]));
        }
        state.select_mode = SelectMode::Vertex;
        return;
    }

    if let Some((v0, v1)) = state.hovered_edge {
        if multi_select {
            // Toggle edge in selection - save undo first
            state.save_selection_undo();
            match &mut state.selection {
                ModelerSelection::Edges(edges) => {
                    if let Some(pos) = edges.iter().position(|e| *e == (v0, v1) || *e == (v1, v0)) {
                        edges.remove(pos);
                    } else {
                        edges.push((v0, v1));
                    }
                }
                _ => {
                    state.selection = ModelerSelection::Edges(vec![(v0, v1)]);
                }
            }
        } else {
            state.set_selection(ModelerSelection::Edges(vec![(v0, v1)]));
        }
        state.select_mode = SelectMode::Edge;
        return;
    }

    if let Some(face_idx) = state.hovered_face {
        if multi_select {
            // Toggle face in selection - save undo first
            state.save_selection_undo();
            match &mut state.selection {
                ModelerSelection::Faces(faces) => {
                    if let Some(pos) = faces.iter().position(|&f| f == face_idx) {
                        faces.remove(pos);
                    } else {
                        faces.push(face_idx);
                    }
                }
                _ => {
                    state.selection = ModelerSelection::Faces(vec![face_idx]);
                }
            }
        } else {
            state.set_selection(ModelerSelection::Faces(vec![face_idx]));
        }
        state.select_mode = SelectMode::Face;
        return;
    }

    // Clicked on nothing - clear selection (unless holding X)
    if !is_key_down(KeyCode::X) {
        state.set_selection(ModelerSelection::None);
    }
}

// ============================================================================
// GIZMO - Transform selected elements by dragging axis handles
// ============================================================================

const GIZMO_HIT_RADIUS: f32 = 8.0;  // Hit radius for axis lines
const ROTATE_GIZMO_RADIUS: f32 = 50.0;  // Radius of rotate circles in screen pixels

/// Dispatcher: handle the appropriate gizmo based on current tool
fn handle_transform_gizmo(
    ctx: &UiContext,
    state: &mut ModelerState,
    mouse_pos: (f32, f32),
    inside_viewport: bool,
    draw_x: f32,
    draw_y: f32,
    draw_w: f32,
    draw_h: f32,
    fb_width: usize,
    fb_height: usize,
    viewport_id: ViewportId,
) {
    // Use new tool system - check which transform tool is active
    match state.tool_box.active_transform_tool() {
        Some(ModelerToolId::Move) => handle_move_gizmo(ctx, state, mouse_pos, inside_viewport, draw_x, draw_y, draw_w, draw_h, fb_width, fb_height, viewport_id),
        Some(ModelerToolId::Scale) => handle_scale_gizmo(ctx, state, mouse_pos, inside_viewport, draw_x, draw_y, draw_w, draw_h, fb_width, fb_height, viewport_id),
        Some(ModelerToolId::Rotate) => handle_rotate_gizmo(ctx, state, mouse_pos, inside_viewport, draw_x, draw_y, draw_w, draw_h, fb_width, fb_height, viewport_id),
        _ => {
            // Select/Extrude modes or no tool active - no gizmos
            state.gizmo_hovered_axis = None;
        }
    }
}

/// Shared gizmo setup: compute center, screen position, and axis endpoints
struct GizmoSetup {
    center: Vec3,
    center_screen: (f32, f32),
    world_length: f32,
    axis_screen_ends: [(Axis, (f32, f32), Color); 3],
}

fn setup_gizmo(
    state: &ModelerState,
    draw_x: f32,
    draw_y: f32,
    draw_w: f32,
    draw_h: f32,
    fb_width: usize,
    fb_height: usize,
) -> Option<GizmoSetup> {
    // Compute center based on selection type
    let center = if let Some(bone_indices) = state.selection.bones() {
        // For bone base selection, compute center from bone base positions
        if bone_indices.is_empty() {
            return None;
        }
        let skeleton = state.skeleton();
        let sum: Vec3 = bone_indices.iter()
            .filter_map(|&idx| {
                if idx < skeleton.len() {
                    let (base_pos, _) = state.get_bone_world_transform(idx);
                    Some(base_pos)
                } else {
                    None
                }
            })
            .fold(Vec3::ZERO, |acc, pos| acc + pos);
        let count = bone_indices.iter().filter(|&&idx| idx < skeleton.len()).count();
        if count == 0 {
            return None;
        }
        sum * (1.0 / count as f32)
    } else if let Some(tip_indices) = state.selection.bone_tips() {
        // For bone tip selection, compute center from bone tip positions
        if tip_indices.is_empty() {
            return None;
        }
        let skeleton = state.skeleton();
        let sum: Vec3 = tip_indices.iter()
            .filter_map(|&idx| {
                if idx < skeleton.len() {
                    Some(state.get_bone_tip_position(idx))
                } else {
                    None
                }
            })
            .fold(Vec3::ZERO, |acc, pos| acc + pos);
        let count = tip_indices.iter().filter(|&&idx| idx < skeleton.len()).count();
        if count == 0 {
            return None;
        }
        sum * (1.0 / count as f32)
    } else {
        // Use compute_selection_center which handles bone transforms for bound meshes
        state.compute_selection_center()?
    };
    let camera = &state.camera;
    let ortho = state.raster_settings.ortho_projection.as_ref();

    let center_screen = match world_to_screen_with_ortho(center, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height, ortho) {
        Some((sx, sy)) => (draw_x + sx / fb_width as f32 * draw_w, draw_y + sy / fb_height as f32 * draw_h),
        None => return None,
    };

    // Get orientation basis (respects Global/Local mode and bone transforms)
    let (basis_x, basis_y, basis_z) = state.compute_orientation_basis();
    let axis_dirs = [
        (Axis::X, basis_x, RED),
        (Axis::Y, basis_y, GREEN),
        (Axis::Z, basis_z, BLUE),
    ];

    // In ortho mode, use a fixed world-space size scaled by zoom
    // In perspective mode, scale by distance to camera
    let world_length = if let Some(ortho) = ortho {
        // Fixed size in screen pixels, converted to world units
        50.0 / ortho.zoom
    } else {
        let dist_to_camera = (center - camera.position).len();
        dist_to_camera * 0.1
    };

    let mut axis_screen_ends: [(Axis, (f32, f32), Color); 3] = [
        (Axis::X, (0.0, 0.0), RED),
        (Axis::Y, (0.0, 0.0), GREEN),
        (Axis::Z, (0.0, 0.0), BLUE),
    ];

    for (i, (axis, dir, color)) in axis_dirs.iter().enumerate() {
        let end_world = center + *dir * world_length;
        if let Some((sx, sy)) = world_to_screen_with_ortho(end_world, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height, ortho) {
            let screen_end = (draw_x + sx / fb_width as f32 * draw_w, draw_y + sy / fb_height as f32 * draw_h);
            axis_screen_ends[i] = (*axis, screen_end, *color);
        }
    }

    Some(GizmoSetup { center, center_screen, world_length, axis_screen_ends })
}

/// Get axis color with hover/drag state
fn get_axis_color(base_color: Color, is_hovered: bool, is_dragging: bool) -> Color {
    if is_dragging {
        YELLOW
    } else if is_hovered {
        Color::from_rgba(
            (base_color.r * 255.0) as u8,
            (base_color.g * 255.0) as u8,
            (base_color.b * 255.0) as u8,
            255,
        )
    } else {
        Color::from_rgba(
            (base_color.r * 200.0) as u8,
            (base_color.g * 200.0) as u8,
            (base_color.b * 200.0) as u8,
            200,
        )
    }
}

// ============================================================================
// MOVE GIZMO - Arrows pointing along each axis (now using DragManager)
// ============================================================================

fn handle_move_gizmo(
    ctx: &UiContext,
    state: &mut ModelerState,
    mouse_pos: (f32, f32),
    inside_viewport: bool,
    draw_x: f32,
    draw_y: f32,
    draw_w: f32,
    draw_h: f32,
    fb_width: usize,
    fb_height: usize,
    viewport_id: ViewportId,
) {
    let setup = match setup_gizmo(state, draw_x, draw_y, draw_w, draw_h, fb_width, fb_height) {
        Some(s) => s,
        None => {
            state.gizmo_hovered_axis = None;
            return;
        }
    };

    let is_ortho = viewport_id != ViewportId::Perspective;

    // Check if DragManager has an active move drag
    let is_dragging = state.drag_manager.is_dragging() && state.drag_manager.active.is_move();

    // Check if this viewport owns the drag
    let owns_drag = state.ortho_drag_viewport == Some(viewport_id) ||
                   (state.ortho_drag_viewport.is_none() && !is_ortho);

    // Skip if another viewport owns this drag
    if !owns_drag && is_dragging {
        return;
    }

    // Handle ongoing drag
    if is_dragging && owns_drag {
        if ctx.mouse.left_down {
            if is_ortho {
                // Ortho mode: use screen-to-world delta (simpler, more precise)
                let drag_zoom = state.ortho_drag_zoom;

                // Get mouse delta from drag start
                if let Some(drag_state) = &state.drag_manager.state {
                    let dx = mouse_pos.0 - drag_state.initial_mouse.0;
                    let dy = mouse_pos.1 - drag_state.initial_mouse.1;

                    // Convert screen delta to world delta based on viewport
                    let world_dx = dx / drag_zoom;
                    let world_dy = -dy / drag_zoom; // Y inverted

                    let mut delta = match viewport_id {
                        ViewportId::Top => Vec3::new(world_dx, 0.0, world_dy),    // XZ plane
                        ViewportId::Front => Vec3::new(world_dx, world_dy, 0.0),  // XY plane
                        ViewportId::Side => Vec3::new(0.0, world_dy, world_dx),   // ZY plane
                        ViewportId::Perspective => Vec3::ZERO,
                    };

                    // Apply axis constraint if present
                    if let super::drag::ActiveDrag::Move(tracker) = &state.drag_manager.active {
                        if let Some(axis) = &tracker.axis {
                            match axis {
                                crate::ui::drag_tracker::Axis::X => { delta.y = 0.0; delta.z = 0.0; }
                                crate::ui::drag_tracker::Axis::Y => { delta.x = 0.0; delta.z = 0.0; }
                                crate::ui::drag_tracker::Axis::Z => { delta.x = 0.0; delta.y = 0.0; }
                            }
                        }

                        // Apply delta to initial positions
                        let snap_disabled = is_key_down(KeyCode::Z);
                        let snap_enabled = state.snap_settings.enabled && !snap_disabled;
                        let snap_size = state.snap_settings.grid_size;

                        let updates: Vec<_> = tracker.initial_positions.iter()
                            .map(|(idx, start_pos)| (*idx, *start_pos + delta))
                            .collect();

                        if state.gizmo_bone_drag {
                            // Apply to bone bases
                            for (bone_idx, mut new_pos) in updates {
                                if snap_enabled {
                                    new_pos.x = (new_pos.x / snap_size).round() * snap_size;
                                    new_pos.y = (new_pos.y / snap_size).round() * snap_size;
                                    new_pos.z = (new_pos.z / snap_size).round() * snap_size;
                                }

                                // Check if this bone has a parent
                                let parent_idx = state.skeleton().get(bone_idx).and_then(|b| b.parent);

                                if let Some(parent_idx) = parent_idx {
                                    let shift_held = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);

                                    if shift_held {
                                        // SHIFT held: old behavior - move parent's tip
                                        let (parent_base, _) = state.get_bone_world_transform(parent_idx);
                                        let parent_vec = new_pos - parent_base;
                                        let new_parent_length = parent_vec.len().max(1.0);
                                        let parent_world_rot = direction_to_rotation(parent_vec);

                                        let grandparent_rot = state.skeleton().get(parent_idx)
                                            .and_then(|b| b.parent)
                                            .map(|gp_idx| state.get_bone_world_transform(gp_idx).1)
                                            .unwrap_or(Vec3::ZERO);
                                        let new_parent_local_rot = parent_world_rot - grandparent_rot;

                                        let old_parent_length = state.skeleton().get(parent_idx).map(|b| b.length).unwrap_or(0.0);
                                        if let Some(bones) = state.asset.skeleton_mut() {
                                            if let Some(parent) = bones.get_mut(parent_idx) {
                                                parent.local_rotation = new_parent_local_rot;
                                                parent.length = new_parent_length;
                                            }
                                            // Smart mode: only update children that were at the tip
                                            for bone in bones.iter_mut() {
                                                if bone.parent == Some(parent_idx) {
                                                    let was_at_tip = (bone.local_position.y - old_parent_length).abs() < 1.0;
                                                    if was_at_tip {
                                                        bone.local_position.y = new_parent_length;
                                                    }
                                                }
                                            }
                                        }
                                    } else {
                                        // Normal drag: slide attachment point along parent
                                        let (parent_base, _) = state.get_bone_world_transform(parent_idx);
                                        let parent_tip = state.get_bone_tip_position(parent_idx);
                                        let parent_axis = (parent_tip - parent_base).normalize();
                                        let parent_length = state.skeleton().get(parent_idx).map(|b| b.length).unwrap_or(200.0);

                                        // Project drag position onto parent axis
                                        let to_drag = new_pos - parent_base;
                                        let along_parent = to_drag.dot(parent_axis).clamp(0.0, parent_length);

                                        // Update child's local_position to slide along parent
                                        if let Some(bones) = state.asset.skeleton_mut() {
                                            if let Some(bone) = bones.get_mut(bone_idx) {
                                                bone.local_position.y = along_parent;
                                            }
                                        }
                                    }
                                } else {
                                    // ROOT bone: can move local_position freely
                                    if let Some(bones) = state.asset.skeleton_mut() {
                                        if let Some(bone) = bones.get_mut(bone_idx) {
                                            bone.local_position = new_pos;
                                        }
                                    }
                                }
                            }
                        } else if state.gizmo_bone_tip_drag {
                            // Apply to bone tips (changes rotation/length)
                            // new_pos is the new TIP world position
                            for (idx, mut new_tip_pos) in updates {
                                // Apply snapping to tip position
                                if snap_enabled {
                                    new_tip_pos.x = (new_tip_pos.x / snap_size).round() * snap_size;
                                    new_tip_pos.y = (new_tip_pos.y / snap_size).round() * snap_size;
                                    new_tip_pos.z = (new_tip_pos.z / snap_size).round() * snap_size;
                                }

                                // Get the bone's base position (fixed during tip drag)
                                let (base_pos, _) = state.get_bone_world_transform(idx);
                                let bone_vec = new_tip_pos - base_pos;
                                let new_length = bone_vec.len().max(1.0);

                                // Convert direction to world rotation, then to local rotation
                                let world_rotation = direction_to_rotation(bone_vec);
                                let parent_rotation = state.skeleton().get(idx)
                                    .and_then(|b| b.parent)
                                    .map(|parent_idx| {
                                        let (_, parent_rot) = state.get_bone_world_transform(parent_idx);
                                        parent_rot
                                    })
                                    .unwrap_or(Vec3::ZERO);
                                let new_rotation = world_rotation - parent_rotation;

                                let old_length = state.skeleton().get(idx).map(|b| b.length).unwrap_or(0.0);
                                if let Some(bones) = state.asset.skeleton_mut() {
                                    if let Some(bone) = bones.get_mut(idx) {
                                        bone.local_rotation = new_rotation;
                                        bone.length = new_length;
                                    }
                                    // Smart mode: only update children that were at the tip
                                    for bone in bones.iter_mut() {
                                        if bone.parent == Some(idx) {
                                            let was_at_tip = (bone.local_position.y - old_length).abs() < 1.0;
                                            if was_at_tip {
                                                bone.local_position.y = new_length;
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            // Apply to mesh vertices
                            let mirror_settings = state.current_mirror_settings();
                            if let Some(mesh) = state.mesh_mut() {
                                for (idx, mut new_pos) in updates {
                                    if snap_enabled {
                                        new_pos.x = (new_pos.x / snap_size).round() * snap_size;
                                        new_pos.y = (new_pos.y / snap_size).round() * snap_size;
                                        new_pos.z = (new_pos.z / snap_size).round() * snap_size;
                                    }
                                    // Constrain center vertices to mirror plane
                                    new_pos = mirror_settings.constrain_to_plane(new_pos);
                                    if let Some(vert) = mesh.vertices.get_mut(idx) {
                                        vert.pos = new_pos;
                                    }
                                }
                            }
                        }
                        state.dirty = true;
                    }
                }
            } else {
                // Perspective mode: use ray casting via DragManager
                let fb_mouse = (
                    (mouse_pos.0 - draw_x) / draw_w * fb_width as f32,
                    (mouse_pos.1 - draw_y) / draw_h * fb_height as f32,
                );

                let result = state.drag_manager.update(
                    fb_mouse,
                    &state.camera,
                    fb_width,
                    fb_height,
                );

                if let DragUpdateResult::Move { positions, .. } = result {
                    let snap_disabled = is_key_down(KeyCode::Z);
                    let snap_enabled = state.snap_settings.enabled && !snap_disabled;
                    let snap_settings = state.snap_settings.clone();

                    if state.gizmo_bone_drag {
                        // Apply to bone bases
                        for (bone_idx, new_pos) in positions {
                            let snapped = if snap_enabled {
                                snap_settings.snap_vec3(new_pos)
                            } else {
                                new_pos
                            };

                            // Check if this bone has a parent
                            let parent_idx = state.skeleton().get(bone_idx).and_then(|b| b.parent);

                            if let Some(parent_idx) = parent_idx {
                                let shift_held = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);

                                if shift_held {
                                    // SHIFT held: old behavior - move parent's tip
                                    let (parent_base, _) = state.get_bone_world_transform(parent_idx);
                                    let parent_vec = snapped - parent_base;
                                    let new_parent_length = parent_vec.len().max(1.0);
                                    let parent_world_rot = direction_to_rotation(parent_vec);

                                    let grandparent_rot = state.skeleton().get(parent_idx)
                                        .and_then(|b| b.parent)
                                        .map(|gp_idx| state.get_bone_world_transform(gp_idx).1)
                                        .unwrap_or(Vec3::ZERO);
                                    let new_parent_local_rot = parent_world_rot - grandparent_rot;

                                    let old_parent_length = state.skeleton().get(parent_idx).map(|b| b.length).unwrap_or(0.0);
                                    if let Some(bones) = state.asset.skeleton_mut() {
                                        if let Some(parent) = bones.get_mut(parent_idx) {
                                            parent.local_rotation = new_parent_local_rot;
                                            parent.length = new_parent_length;
                                        }
                                        // Smart mode: only update children that were at the tip
                                        for bone in bones.iter_mut() {
                                            if bone.parent == Some(parent_idx) {
                                                let was_at_tip = (bone.local_position.y - old_parent_length).abs() < 1.0;
                                                if was_at_tip {
                                                    bone.local_position.y = new_parent_length;
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    // Normal drag: slide attachment point along parent
                                    let (parent_base, _) = state.get_bone_world_transform(parent_idx);
                                    let parent_tip = state.get_bone_tip_position(parent_idx);
                                    let parent_axis = (parent_tip - parent_base).normalize();
                                    let parent_length = state.skeleton().get(parent_idx).map(|b| b.length).unwrap_or(200.0);

                                    // Project drag position onto parent axis
                                    let to_drag = snapped - parent_base;
                                    let along_parent = to_drag.dot(parent_axis).clamp(0.0, parent_length);

                                    // Update child's local_position to slide along parent
                                    if let Some(bones) = state.asset.skeleton_mut() {
                                        if let Some(bone) = bones.get_mut(bone_idx) {
                                            bone.local_position.y = along_parent;
                                        }
                                    }
                                }
                            } else {
                                // ROOT bone: can move local_position freely
                                if let Some(bones) = state.asset.skeleton_mut() {
                                    if let Some(bone) = bones.get_mut(bone_idx) {
                                        bone.local_position = snapped;
                                    }
                                }
                            }
                        }
                    } else if state.gizmo_bone_tip_drag {
                        // Apply to bone tips (changes rotation/length)
                        for (bone_idx, new_tip_pos) in positions {
                            // Get the bone's base position (fixed during tip drag)
                            let (base_pos, current_world_rot) = state.get_bone_world_transform(bone_idx);
                            let bone_vec = new_tip_pos - base_pos;
                            let new_length = bone_vec.len().max(1.0);

                            // Convert direction to world rotation, then to local rotation
                            let world_rotation = direction_to_rotation(bone_vec);
                            let parent_rotation = state.skeleton().get(bone_idx)
                                .and_then(|b| b.parent)
                                .map(|parent_idx| {
                                    let (_, parent_rot) = state.get_bone_world_transform(parent_idx);
                                    parent_rot
                                })
                                .unwrap_or(Vec3::ZERO);
                            let new_rotation = world_rotation - parent_rotation;

                            let old_length = state.skeleton().get(bone_idx).map(|b| b.length).unwrap_or(0.0);
                            if let Some(bones) = state.asset.skeleton_mut() {
                                if let Some(bone) = bones.get_mut(bone_idx) {
                                    bone.local_rotation = new_rotation;
                                    bone.length = new_length;
                                }
                                // Smart mode: only update children that were at the tip
                                for bone in bones.iter_mut() {
                                    if bone.parent == Some(bone_idx) {
                                        let was_at_tip = (bone.local_position.y - old_length).abs() < 1.0;
                                        if was_at_tip {
                                            bone.local_position.y = new_length;
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        // Apply to mesh vertices
                        let mirror_settings = state.current_mirror_settings();
                        if let Some(mesh) = state.mesh_mut() {
                            for (vert_idx, new_pos) in positions {
                                if let Some(vert) = mesh.vertices.get_mut(vert_idx) {
                                    let snapped = if snap_enabled {
                                        snap_settings.snap_vec3(new_pos)
                                    } else {
                                        new_pos
                                    };
                                    // Constrain center vertices to mirror plane
                                    vert.pos = mirror_settings.constrain_to_plane(snapped);
                                }
                            }
                        }
                    }
                    state.dirty = true;
                }
            }
        } else {
            // End drag - sync tool state
            state.tool_box.tools.move_tool.end_drag();
            state.drag_manager.end();
            state.ortho_drag_viewport = None;
            state.gizmo_bone_drag = false;
            state.gizmo_bone_tip_drag = false;
        }
    }

    // Detect axis hover (only when not dragging)
    state.gizmo_hovered_axis = None;
    if !is_dragging && inside_viewport {
        for (axis, end_screen, _) in &setup.axis_screen_ends {
            let dist = point_to_line_distance(
                mouse_pos.0, mouse_pos.1,
                setup.center_screen.0, setup.center_screen.1,
                end_screen.0, end_screen.1,
            );
            if dist < GIZMO_HIT_RADIUS {
                state.gizmo_hovered_axis = Some(*axis);
                break;
            }
        }
    }
    // Sync hover state to tool
    state.tool_box.tools.move_tool.set_hovered_axis(state.gizmo_hovered_axis.map(to_ui_axis));

    // Start drag on click
    if ctx.mouse.left_pressed && inside_viewport && state.gizmo_hovered_axis.is_some() && !is_dragging {
        let axis = state.gizmo_hovered_axis.unwrap();

        // Check if we're moving bones, bone tips, or vertices
        let (indices, initial_positions, is_bone_drag, is_bone_tip_drag) = if let Some(bone_indices) = state.selection.bones() {
            // Bone BASE selection - get bone indices and their local_position values
            let skeleton = state.skeleton();
            let positions: Vec<(usize, Vec3)> = bone_indices.iter()
                .filter_map(|&idx| skeleton.get(idx).map(|b| (idx, b.local_position)))
                .collect();
            let indices: Vec<usize> = positions.iter().map(|(idx, _)| *idx).collect();
            (indices, positions, true, false)
        } else if let Some(tip_indices) = state.selection.bone_tips() {
            // Bone TIP selection - get bone indices and their TIP world positions
            let positions: Vec<(usize, Vec3)> = tip_indices.iter()
                .map(|&idx| {
                    let tip_pos = state.get_bone_tip_position(idx);
                    (idx, tip_pos)
                })
                .collect();
            let indices: Vec<usize> = positions.iter().map(|(idx, _)| *idx).collect();
            (indices, positions, false, true)
        } else {
            // Vertex selection - get vertex indices and positions
            let mesh = state.mesh();
            let mut indices = state.selection.get_affected_vertex_indices(mesh);
            if state.vertex_linking {
                indices = mesh.expand_to_coincident(&indices, 0.001);
            }
            let positions: Vec<(usize, Vec3)> = indices.iter()
                .filter_map(|&idx| mesh.vertices.get(idx).map(|v| (idx, v.pos)))
                .collect();
            (indices, positions, false, false)
        };

        // Track whether this is a bone drag
        state.gizmo_bone_drag = is_bone_drag;
        state.gizmo_bone_tip_drag = is_bone_tip_drag;

        // Save undo state BEFORE starting the gizmo drag
        if is_bone_drag || is_bone_tip_drag {
            let undo_name = if is_bone_drag { "Move Bones" } else { "Move Bone Tips" };
            state.save_undo_skeleton(undo_name);
        } else {
            state.push_undo("Gizmo Move");
        }

        // Set up ortho-specific tracking
        if is_ortho {
            state.ortho_drag_viewport = Some(viewport_id);
            let ortho_cam = state.get_ortho_camera(viewport_id);
            state.ortho_drag_zoom = ortho_cam.zoom;
        }

        // Start drag with DragManager and sync tool state
        let ui_axis = to_ui_axis(axis);
        state.tool_box.tools.move_tool.start_drag(Some(ui_axis));

        // Get bone rotation for world-to-local delta transformation (vertex moves on bone-bound meshes)
        // This is needed in BOTH Global and Local modes because:
        // - Vertices are stored in bone-local space
        // - Drag delta is computed in world space (along world or local axes)
        // - Delta must be inverse-transformed to bone-local before applying
        let bone_rotation = if !is_bone_drag && !is_bone_tip_drag {
            state.selected_object()
                .and_then(|obj| obj.default_bone_index)
                .map(|bone_idx| state.get_bone_world_transform(bone_idx).1)
        } else {
            None
        };

        // Get axis direction from orientation basis (for Local mode)
        let (basis_x, basis_y, basis_z) = state.compute_orientation_basis();
        let axis_direction = match axis {
            Axis::X => basis_x,
            Axis::Y => basis_y,
            Axis::Z => basis_z,
        };

        if is_ortho {
            // For ortho, use screen coordinates (we calculate delta from screen, not ray casting)
            state.drag_manager.start_move_with_bone(
                setup.center,
                mouse_pos,  // Use screen coordinates directly
                Some(ui_axis),
                Some(axis_direction),
                indices,
                initial_positions,
                state.snap_settings.enabled,
                state.snap_settings.grid_size,
                bone_rotation,
            );
        } else {
            // For perspective, use framebuffer coordinates for ray casting
            let fb_mouse = (
                (mouse_pos.0 - draw_x) / draw_w * fb_width as f32,
                (mouse_pos.1 - draw_y) / draw_h * fb_height as f32,
            );
            state.drag_manager.start_move_3d_with_bone(
                setup.center,
                fb_mouse,
                Some(ui_axis),
                Some(axis_direction),
                indices,
                initial_positions,
                state.snap_settings.enabled,
                state.snap_settings.grid_size,
                &state.camera,
                fb_width,
                fb_height,
                bone_rotation,
            );
        }
    }

    // Draw move gizmo (arrows)
    for (axis, end_screen, base_color) in &setup.axis_screen_ends {
        let is_hovered = state.gizmo_hovered_axis == Some(*axis);
        let axis_dragging = is_dragging && state.drag_manager.current_axis() == Some(to_ui_axis(*axis));
        let color = get_axis_color(*base_color, is_hovered, axis_dragging);
        let thickness = if is_hovered || axis_dragging { 3.0 } else { 2.0 };

        // Draw axis line
        draw_line(setup.center_screen.0, setup.center_screen.1, end_screen.0, end_screen.1, thickness, color);

        // Draw arrowhead
        let dx = end_screen.0 - setup.center_screen.0;
        let dy = end_screen.1 - setup.center_screen.1;
        let len = (dx * dx + dy * dy).sqrt();
        if len > 5.0 {
            let nx = dx / len;
            let ny = dy / len;
            let px = -ny;
            let py = nx;
            let arrow_size = 6.0;
            let arrow_base_x = end_screen.0 - nx * arrow_size * 1.5;
            let arrow_base_y = end_screen.1 - ny * arrow_size * 1.5;
            draw_triangle(
                Vec2::new(end_screen.0, end_screen.1),
                Vec2::new(arrow_base_x + px * arrow_size, arrow_base_y + py * arrow_size),
                Vec2::new(arrow_base_x - px * arrow_size, arrow_base_y - py * arrow_size),
                color,
            );
        }
    }

    // Draw center circle
    let center_color = if is_dragging { YELLOW } else { WHITE };
    draw_circle(setup.center_screen.0, setup.center_screen.1, 4.0, center_color);
}

// ============================================================================
// SCALE GIZMO - Cubes at the end of each axis
// ============================================================================

fn handle_scale_gizmo(
    ctx: &UiContext,
    state: &mut ModelerState,
    mouse_pos: (f32, f32),
    inside_viewport: bool,
    draw_x: f32,
    draw_y: f32,
    draw_w: f32,
    draw_h: f32,
    fb_width: usize,
    fb_height: usize,
    _viewport_id: ViewportId,  // TODO: add ortho-specific scale handling
) {
    let setup = match setup_gizmo(state, draw_x, draw_y, draw_w, draw_h, fb_width, fb_height) {
        Some(s) => s,
        None => {
            state.gizmo_hovered_axis = None;
            return;
        }
    };

    // Check if DragManager has an active scale drag
    let is_dragging = state.drag_manager.is_dragging() && state.drag_manager.active.is_scale();

    // Handle ongoing drag with DragManager
    if is_dragging {
        if ctx.mouse.left_down {
            // Scale uses screen-space coordinates for distance-from-center calculation
            let result = state.drag_manager.update(
                mouse_pos,  // screen-space (same as center_screen)
                &state.camera,
                fb_width,
                fb_height,
            );

            if let DragUpdateResult::Scale { positions, .. } = result {
                let snap_disabled = is_key_down(KeyCode::Z);
                // Capture snap settings before borrowing mesh
                let snap_enabled = state.snap_settings.enabled && !snap_disabled;
                let snap_settings = state.snap_settings.clone();
                if let Some(mesh) = state.mesh_mut() {
                    for (vert_idx, new_pos) in positions {
                        if let Some(vert) = mesh.vertices.get_mut(vert_idx) {
                            vert.pos = if snap_enabled {
                                snap_settings.snap_vec3(new_pos)
                            } else {
                                new_pos
                            };
                        }
                    }
                }
                state.dirty = true;
            }
        } else {
            // End drag - sync tool state
            state.tool_box.tools.scale.end_drag();
            state.drag_manager.end();
        }
    }

    // Detect hover on cube handles and center (only when not dragging)
    state.gizmo_hovered_axis = None;
    let mut center_hovered = false;
    if !is_dragging && inside_viewport {
        // Check center circle first (uniform scale)
        let center_dx = mouse_pos.0 - setup.center_screen.0;
        let center_dy = mouse_pos.1 - setup.center_screen.1;
        let center_radius = 8.0; // Slightly larger than visual radius for easier clicking
        if center_dx * center_dx + center_dy * center_dy < center_radius * center_radius {
            center_hovered = true;
            // Don't set gizmo_hovered_axis - None means uniform scale
        } else {
            // Check axis cube handles
            let cube_size = 6.0;
            for (axis, end_screen, _) in &setup.axis_screen_ends {
                let dx = mouse_pos.0 - end_screen.0;
                let dy = mouse_pos.1 - end_screen.1;
                if dx.abs() < cube_size && dy.abs() < cube_size {
                    state.gizmo_hovered_axis = Some(*axis);
                    break;
                }
            }
        }
    }
    // Sync hover state to tool (None = uniform scale when center hovered)
    let tool_axis = if center_hovered { None } else { state.gizmo_hovered_axis.map(to_ui_axis) };
    state.tool_box.tools.scale.set_hovered_axis(tool_axis);

    // Start drag on click (axis handle OR center for uniform scale)
    let can_start_drag = state.gizmo_hovered_axis.is_some() || center_hovered;
    if ctx.mouse.left_pressed && inside_viewport && can_start_drag && !is_dragging {
        // Get vertex indices and initial positions
        let mesh = state.mesh();
        let mut indices = state.selection.get_affected_vertex_indices(mesh);
        if state.vertex_linking {
            indices = mesh.expand_to_coincident(&indices, 0.001);
        }

        let initial_positions: Vec<(usize, Vec3)> = indices.iter()
            .filter_map(|&idx| mesh.vertices.get(idx).map(|v| (idx, v.pos)))
            .collect();

        // Save undo state BEFORE starting the gizmo drag
        state.push_undo("Gizmo Scale");

        // Determine axis: None for uniform scale (center), Some(axis) for constrained
        let ui_axis = state.gizmo_hovered_axis.map(to_ui_axis);

        // Start drag with DragManager and sync tool state
        // Scale uses screen-space coordinates for distance-from-center calculation
        state.tool_box.tools.scale.start_drag(ui_axis);
        state.drag_manager.start_scale(
            setup.center,
            mouse_pos,           // screen-space mouse position
            ui_axis,
            indices,
            initial_positions,
            setup.center_screen, // screen-space center for distance calculation
        );
    }

    // Draw scale gizmo (lines with cubes)
    for (axis, end_screen, base_color) in &setup.axis_screen_ends {
        let is_hovered = state.gizmo_hovered_axis == Some(*axis);
        let axis_dragging = is_dragging && state.drag_manager.current_axis() == Some(to_ui_axis(*axis));
        let color = get_axis_color(*base_color, is_hovered, axis_dragging);
        let thickness = if is_hovered || axis_dragging { 3.0 } else { 2.0 };

        // Draw axis line
        draw_line(setup.center_screen.0, setup.center_screen.1, end_screen.0, end_screen.1, thickness, color);

        // Draw cube at end
        let cube_size = if is_hovered || axis_dragging { 5.0 } else { 4.0 };
        draw_rectangle(
            end_screen.0 - cube_size,
            end_screen.1 - cube_size,
            cube_size * 2.0,
            cube_size * 2.0,
            color,
        );
    }

    // Draw center circle (uniform scale handle)
    let uniform_dragging = is_dragging && state.drag_manager.current_axis().is_none();
    let center_color = if uniform_dragging {
        YELLOW
    } else if center_hovered {
        Color::from_rgba(255, 255, 255, 255) // Bright white when hovered
    } else {
        Color::from_rgba(200, 200, 200, 200) // Dimmer when not hovered
    };
    let center_size = if center_hovered || uniform_dragging { 6.0 } else { 4.0 };
    draw_circle(setup.center_screen.0, setup.center_screen.1, center_size, center_color);
}

// ============================================================================
// ROTATE GIZMO - Circles around each axis
// ============================================================================

fn handle_rotate_gizmo(
    ctx: &UiContext,
    state: &mut ModelerState,
    mouse_pos: (f32, f32),
    inside_viewport: bool,
    draw_x: f32,
    draw_y: f32,
    draw_w: f32,
    draw_h: f32,
    fb_width: usize,
    fb_height: usize,
    _viewport_id: ViewportId,  // TODO: add ortho-specific rotate handling
) {
    let setup = match setup_gizmo(state, draw_x, draw_y, draw_w, draw_h, fb_width, fb_height) {
        Some(s) => s,
        None => {
            state.gizmo_hovered_axis = None;
            return;
        }
    };

    // Copy camera data to avoid borrow conflicts
    let camera_position = state.camera.position;
    let camera_basis_x = state.camera.basis_x;
    let camera_basis_y = state.camera.basis_y;
    let camera_basis_z = state.camera.basis_z;
    let ortho = state.raster_settings.ortho_projection.clone();

    // Check if DragManager has an active rotate drag
    let is_dragging = state.drag_manager.is_dragging() && state.drag_manager.active.is_rotate();

    // Handle ongoing drag with DragManager
    if is_dragging {
        if ctx.mouse.left_down {
            // Pass RAW screen mouse - the drag manager converts it using stored viewport transform
            let result = state.drag_manager.update(
                mouse_pos,  // raw screen-space mouse (converted internally)
                &state.camera,
                fb_width,
                fb_height,
            );

            if let DragUpdateResult::Rotate { positions, .. } = result {
                let snap_disabled = is_key_down(KeyCode::Z);
                // Capture snap settings before borrowing mesh
                let snap_enabled = state.snap_settings.enabled && !snap_disabled;
                let snap_settings = state.snap_settings.clone();
                if let Some(mesh) = state.mesh_mut() {
                    for (vert_idx, new_pos) in positions {
                        if let Some(vert) = mesh.vertices.get_mut(vert_idx) {
                            vert.pos = if snap_enabled {
                                snap_settings.snap_vec3(new_pos)
                            } else {
                                new_pos
                            };
                        }
                    }
                }
                state.dirty = true;
            }
        } else {
            // End drag - sync tool state
            state.tool_box.tools.rotate.end_drag();
            state.drag_manager.end();
        }
    }

    // Detect hover on rotation circles (only when not dragging)
    state.gizmo_hovered_axis = None;
    if !is_dragging && inside_viewport {
        let axes_to_check = [
            (Axis::X, Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 1.0)),
            (Axis::Y, Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0)),
            (Axis::Z, Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)),
        ];

        let mut best_dist = f32::MAX;
        let mut best_axis: Option<Axis> = None;

        for (axis, perp1, perp2) in &axes_to_check {
            // Check visibility - skip if circle is edge-on
            let axis_dir = match axis {
                Axis::X => Vec3::new(1.0, 0.0, 0.0),
                Axis::Y => Vec3::new(0.0, 1.0, 0.0),
                Axis::Z => Vec3::new(0.0, 0.0, 1.0),
            };
            let dot = (axis_dir.x * camera_basis_z.x + axis_dir.y * camera_basis_z.y + axis_dir.z * camera_basis_z.z).abs();
            if dot > 0.95 { continue; } // Skip nearly edge-on circles

            // Sample points on the circle and find minimum distance to mouse
            let segments = 24;
            for i in 0..segments {
                let t = i as f32 / segments as f32 * std::f32::consts::TAU;
                let world_point = setup.center + *perp1 * (t.cos() * setup.world_length) + *perp2 * (t.sin() * setup.world_length);

                if let Some((sx, sy)) = world_to_screen_with_ortho(world_point, camera_position, camera_basis_x, camera_basis_y, camera_basis_z, fb_width, fb_height, ortho.as_ref()) {
                    let screen_pos = (draw_x + sx / fb_width as f32 * draw_w, draw_y + sy / fb_height as f32 * draw_h);
                    let dist = ((mouse_pos.0 - screen_pos.0).powi(2) + (mouse_pos.1 - screen_pos.1).powi(2)).sqrt();
                    if dist < best_dist {
                        best_dist = dist;
                        best_axis = Some(*axis);
                    }
                }
            }
        }

        if best_dist < GIZMO_HIT_RADIUS * 1.5 {
            state.gizmo_hovered_axis = best_axis;
        }
    }
    // Sync hover state to tool
    state.tool_box.tools.rotate.set_hovered_axis(state.gizmo_hovered_axis.map(to_ui_axis));

    // Start drag on click
    if ctx.mouse.left_pressed && inside_viewport && state.gizmo_hovered_axis.is_some() && !is_dragging {
        let axis = state.gizmo_hovered_axis.unwrap();

        // Get vertex indices and initial positions
        let mesh = state.mesh();
        let mut indices = state.selection.get_affected_vertex_indices(mesh);
        if state.vertex_linking {
            indices = mesh.expand_to_coincident(&indices, 0.001);
        }

        let initial_positions: Vec<(usize, Vec3)> = indices.iter()
            .filter_map(|&idx| mesh.vertices.get(idx).map(|v| (idx, v.pos)))
            .collect();

        // Calculate initial angle using ray-circle intersection (arc-following)
        // Convert screen mouse to framebuffer coordinates for initial angle calculation
        let fb_mouse = (
            (mouse_pos.0 - draw_x) / draw_w * fb_width as f32,
            (mouse_pos.1 - draw_y) / draw_h * fb_height as f32,
        );

        // Get reference vector for angle=0 (perpendicular to rotation axis)
        let ref_vector = match axis {
            Axis::X => Vec3::new(0.0, 1.0, 0.0),
            Axis::Y => Vec3::new(1.0, 0.0, 0.0),
            Axis::Z => Vec3::new(1.0, 0.0, 0.0),
        };
        let axis_vec = match axis {
            Axis::X => Vec3::new(1.0, 0.0, 0.0),
            Axis::Y => Vec3::new(0.0, 1.0, 0.0),
            Axis::Z => Vec3::new(0.0, 0.0, 1.0),
        };

        // Cast ray from mouse position and find angle on rotation circle
        let ray = screen_to_ray(fb_mouse.0, fb_mouse.1, fb_width, fb_height, &state.camera);
        let initial_angle = ray_circle_angle(&ray, setup.center, axis_vec, ref_vector)
            .unwrap_or(0.0);

        // Save undo state BEFORE starting the gizmo drag
        state.push_undo("Gizmo Rotate");

        // Start drag with DragManager and sync tool state
        let ui_axis = to_ui_axis(axis);
        state.tool_box.tools.rotate.start_drag(Some(ui_axis), initial_angle);
        state.drag_manager.start_rotate(
            setup.center,
            initial_angle,
            mouse_pos,           // raw screen-space mouse (converted internally using viewport transform)
            setup.center_screen, // screen-space center (fallback)
            ui_axis,
            indices,
            initial_positions,
            state.snap_settings.enabled,
            15.0, // Snap to 15-degree increments
            &state.camera,
            fb_width,
            fb_height,
            (draw_x, draw_y, draw_w, draw_h), // viewport transform for consistent coordinate conversion
        );
    }

    // Draw rotation circles
    let axes = [(Axis::X, RED), (Axis::Y, GREEN), (Axis::Z, BLUE)];

    for (axis, base_color) in &axes {
        let is_hovered = state.gizmo_hovered_axis == Some(*axis);
        let axis_dragging = is_dragging && state.drag_manager.current_axis() == Some(to_ui_axis(*axis));
        let color = get_axis_color(*base_color, is_hovered, axis_dragging);
        let thickness = if is_hovered || axis_dragging { 2.5 } else { 1.5 };

        // Draw arc representing rotation around this axis
        let segments = 32;
        let view_dir = camera_basis_z;

        // Determine visibility factor based on how perpendicular the axis is to view
        let axis_dir = match axis {
            Axis::X => Vec3::new(1.0, 0.0, 0.0),
            Axis::Y => Vec3::new(0.0, 1.0, 0.0),
            Axis::Z => Vec3::new(0.0, 0.0, 1.0),
        };
        let dot = (axis_dir.x * view_dir.x + axis_dir.y * view_dir.y + axis_dir.z * view_dir.z).abs();

        // Only draw if axis is reasonably perpendicular to view (circle would be visible)
        if dot < 0.9 {
            // Get perpendicular vectors for this axis
            let (perp1, perp2) = match axis {
                Axis::X => (Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 1.0)),
                Axis::Y => (Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0)),
                Axis::Z => (Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)),
            };

            let mut prev_screen: Option<(f32, f32)> = None;
            for i in 0..=segments {
                let t = i as f32 / segments as f32 * std::f32::consts::TAU;
                let world_point = setup.center + perp1 * (t.cos() * setup.world_length) + perp2 * (t.sin() * setup.world_length);

                if let Some((sx, sy)) = world_to_screen_with_ortho(world_point, camera_position, camera_basis_x, camera_basis_y, camera_basis_z, fb_width, fb_height, ortho.as_ref()) {
                    let screen_pos = (draw_x + sx / fb_width as f32 * draw_w, draw_y + sy / fb_height as f32 * draw_h);

                    if let Some(prev) = prev_screen {
                        draw_line(prev.0, prev.1, screen_pos.0, screen_pos.1, thickness, color);
                    }
                    prev_screen = Some(screen_pos);
                } else {
                    prev_screen = None;
                }
            }
        }
    }

    // Draw center circle
    let center_color = if is_dragging { YELLOW } else { WHITE };
    draw_circle(setup.center_screen.0, setup.center_screen.1, 4.0, center_color);
}

// =============================================================================
// Component Gizmos - Visual representations for non-mesh components
// =============================================================================

/// Draw visual representations for components (lights show as octahedrons, etc.)
fn draw_component_gizmos(
    state: &ModelerState,
    fb: &mut Framebuffer,
) {
    let camera = &state.camera;
    let ortho = state.raster_settings.ortho_projection.as_ref();

    // Draw all visible light components as octahedrons
    for (comp_idx, component) in state.asset.components.iter().enumerate() {
        if state.is_component_hidden(comp_idx) {
            continue;
        }

        match component {
            crate::asset::AssetComponent::Light { color, offset, .. } => {
                let light_pos = Vec3::new(offset[0], offset[1], offset[2]);
                let is_selected = state.selected_component == Some(comp_idx);

                // Slightly larger and white when selected
                let size = if is_selected { 120.0 } else { 80.0 };
                let gizmo_color = if is_selected {
                    RasterColor::new(255, 255, 255)
                } else {
                    RasterColor::new(color[0], color[1], color[2])
                };

                draw_filled_octahedron(fb, camera, ortho, light_pos, size, gizmo_color);
            }
            _ => {}
        }
    }
}

/// Handle move gizmo for component offset (Light, etc.)
/// Draws gizmo arrows and handles drag interaction
fn handle_component_move_gizmo(
    ctx: &UiContext,
    state: &mut ModelerState,
    draw_x: f32,
    draw_y: f32,
    draw_w: f32,
    draw_h: f32,
    fb_width: usize,
    fb_height: usize,
    viewport_id: ViewportId,
) {
    // Only show gizmo when Move tool is active and a component with offset is selected
    if state.tool_box.active_transform_tool() != Some(ModelerToolId::Move) {
        return;
    }

    let Some(comp_idx) = state.selected_component else { return };

    // Get the component's offset position
    let offset = match state.asset.components.get(comp_idx) {
        Some(crate::asset::AssetComponent::Light { offset, .. }) => *offset,
        _ => return, // No gizmo for non-offset components
    };

    let center = Vec3::new(offset[0], offset[1], offset[2]);
    let camera = &state.camera;
    let ortho = state.raster_settings.ortho_projection.as_ref();

    // Project center to screen
    let Some((cx, cy)) = world_to_screen_with_ortho(
        center, camera.position, camera.basis_x, camera.basis_y, camera.basis_z,
        fb_width, fb_height, ortho
    ) else { return };

    let center_screen = (
        draw_x + cx / fb_width as f32 * draw_w,
        draw_y + cy / fb_height as f32 * draw_h,
    );

    // Calculate gizmo size based on distance
    let world_length = if let Some(ortho) = ortho {
        50.0 / ortho.zoom
    } else {
        let dist_to_camera = (center - camera.position).len();
        dist_to_camera * 0.1
    };

    // Project axis endpoints
    let axis_data = [
        (Axis::X, Vec3::new(1.0, 0.0, 0.0), RED),
        (Axis::Y, Vec3::new(0.0, 1.0, 0.0), GREEN),
        (Axis::Z, Vec3::new(0.0, 0.0, 1.0), BLUE),
    ];

    let mut axis_screen_ends: Vec<(Axis, (f32, f32), Color)> = Vec::new();
    for (axis, dir, color) in &axis_data {
        let end_world = center + *dir * world_length;
        if let Some((sx, sy)) = world_to_screen_with_ortho(
            end_world, camera.position, camera.basis_x, camera.basis_y, camera.basis_z,
            fb_width, fb_height, ortho
        ) {
            let screen_end = (
                draw_x + sx / fb_width as f32 * draw_w,
                draw_y + sy / fb_height as f32 * draw_h,
            );
            axis_screen_ends.push((*axis, screen_end, *color));
        }
    }

    let mouse_pos = (ctx.mouse.x, ctx.mouse.y);
    let is_dragging = state.component_gizmo_drag_axis.is_some();

    // Check if this viewport owns the drag (or no drag is active)
    let owns_drag = state.component_gizmo_drag_viewport == Some(viewport_id) ||
                   state.component_gizmo_drag_viewport.is_none();

    // Handle ongoing drag - only in the viewport that owns it
    if is_dragging && ctx.mouse.left_down && state.component_gizmo_drag_viewport == Some(viewport_id) {
        if let (Some(drag_axis), Some(drag_start)) = (state.component_gizmo_drag_axis, state.component_gizmo_drag_start) {
            let screen_dx = mouse_pos.0 - drag_start.0;
            let screen_dy = mouse_pos.1 - drag_start.1;

            // Find the screen direction of the dragged axis
            let axis_screen_dir = axis_screen_ends.iter()
                .find(|(a, _, _)| *a == drag_axis)
                .map(|(_, end, _)| {
                    let dx = end.0 - center_screen.0;
                    let dy = end.1 - center_screen.1;
                    let len = (dx * dx + dy * dy).sqrt();
                    if len > 0.001 { (dx / len, dy / len) } else { (1.0, 0.0) }
                })
                .unwrap_or((1.0, 0.0));

            // Project mouse movement onto the screen direction of the axis
            // dot product gives how much the mouse moved along the axis direction
            let screen_movement = screen_dx * axis_screen_dir.0 + screen_dy * axis_screen_dir.1;

            // Convert screen movement to world units
            let zoom = if let Some(ortho) = ortho {
                ortho.zoom
            } else {
                let dist = (center - camera.position).len();
                500.0 / dist
            };
            let world_movement = screen_movement / zoom;

            // Apply movement along the world axis
            let delta = match drag_axis {
                Axis::X => Vec3::new(world_movement, 0.0, 0.0),
                Axis::Y => Vec3::new(0.0, world_movement, 0.0),
                Axis::Z => Vec3::new(0.0, 0.0, world_movement),
            };

            // Calculate new offset from start position + delta
            let start = state.component_gizmo_start_offset;
            let mut new_offset = [
                start[0] + delta.x,
                start[1] + delta.y,
                start[2] + delta.z,
            ];

            // Apply snap to grid
            let snap_disabled = is_key_down(KeyCode::Z);
            let snap_enabled = state.snap_settings.enabled && !snap_disabled;
            if snap_enabled {
                let snap_size = state.snap_settings.grid_size;
                new_offset[0] = (new_offset[0] / snap_size).round() * snap_size;
                new_offset[1] = (new_offset[1] / snap_size).round() * snap_size;
                new_offset[2] = (new_offset[2] / snap_size).round() * snap_size;
            }

            if let Some(crate::asset::AssetComponent::Light { offset, .. }) = state.asset.components.get_mut(comp_idx) {
                *offset = new_offset;
            }
        }
    }

    // End drag on mouse release (any viewport can detect this)
    if is_dragging && !ctx.mouse.left_down {
        state.component_gizmo_drag_axis = None;
        state.component_gizmo_drag_start = None;
        state.component_gizmo_drag_viewport = None;
    }

    // Check for hover/click on gizmo axes (only when not dragging or this viewport owns it)
    let hit_radius = 8.0;
    let mut hovered_axis: Option<Axis> = None;

    if !is_dragging && owns_drag {
        for (axis, end_pos, _) in &axis_screen_ends {
            let dist = point_to_line_distance(
                mouse_pos.0, mouse_pos.1,
                center_screen.0, center_screen.1,
                end_pos.0, end_pos.1
            );
            if dist < hit_radius {
                hovered_axis = Some(*axis);
                break;
            }
        }

        // Start drag on click
        if hovered_axis.is_some() && ctx.mouse.left_pressed {
            state.component_gizmo_drag_axis = hovered_axis;
            state.component_gizmo_drag_start = Some(mouse_pos);
            state.component_gizmo_start_offset = offset;
            state.component_gizmo_drag_viewport = Some(viewport_id);
        }
    }

    // Draw gizmo arrows
    for (axis, end_pos, base_color) in &axis_screen_ends {
        let is_this_hovered = hovered_axis == Some(*axis);
        let is_this_dragging = state.component_gizmo_drag_axis == Some(*axis);

        let color = if is_this_dragging {
            YELLOW
        } else if is_this_hovered {
            WHITE
        } else {
            *base_color
        };

        let thickness = if is_this_dragging || is_this_hovered { 3.0 } else { 2.0 };
        draw_line(center_screen.0, center_screen.1, end_pos.0, end_pos.1, thickness, color);

        // Draw arrowhead
        let dir = (end_pos.0 - center_screen.0, end_pos.1 - center_screen.1);
        let len = (dir.0 * dir.0 + dir.1 * dir.1).sqrt();
        if len > 0.0 {
            let norm = (dir.0 / len, dir.1 / len);
            let perp = (-norm.1, norm.0);
            let arrow_size = 8.0;
            let tip = *end_pos;
            let left = (tip.0 - norm.0 * arrow_size + perp.0 * arrow_size * 0.5,
                       tip.1 - norm.1 * arrow_size + perp.1 * arrow_size * 0.5);
            let right = (tip.0 - norm.0 * arrow_size - perp.0 * arrow_size * 0.5,
                        tip.1 - norm.1 * arrow_size - perp.1 * arrow_size * 0.5);
            draw_triangle(
                vec2(tip.0, tip.1),
                vec2(left.0, left.1),
                vec2(right.0, right.1),
                color,
            );
        }
    }

    // Draw center dot
    draw_circle(center_screen.0, center_screen.1, 4.0, WHITE);
}

/// Draw a filled octahedron in 3D (classic light gizmo)
fn draw_filled_octahedron(
    fb: &mut Framebuffer,
    camera: &Camera,
    ortho: Option<&OrthoProjection>,
    center: Vec3,
    size: f32,
    color: RasterColor,
) {
    // Octahedron has 6 vertices: top, bottom, and 4 around the middle
    let top = Vec3::new(center.x, center.y + size, center.z);
    let bottom = Vec3::new(center.x, center.y - size, center.z);
    let front = Vec3::new(center.x, center.y, center.z + size);
    let back = Vec3::new(center.x, center.y, center.z - size);
    let left = Vec3::new(center.x - size, center.y, center.z);
    let right = Vec3::new(center.x + size, center.y, center.z);

    // Project all vertices to screen space
    let project_vertex = |p: Vec3| -> Option<(i32, i32, f32)> {
        if let Some(proj) = world_to_screen_with_ortho_depth(
            p,
            camera.position,
            camera.basis_x,
            camera.basis_y,
            camera.basis_z,
            fb.width,
            fb.height,
            ortho,
        ) {
            Some((proj.0 as i32, proj.1 as i32, proj.2))
        } else {
            None
        }
    };

    let top_s = project_vertex(top);
    let bottom_s = project_vertex(bottom);
    let front_s = project_vertex(front);
    let back_s = project_vertex(back);
    let left_s = project_vertex(left);
    let right_s = project_vertex(right);

    // 8 triangular faces of the octahedron
    let faces = [
        (top_s, front_s, right_s),
        (top_s, right_s, back_s),
        (top_s, back_s, left_s),
        (top_s, left_s, front_s),
        (bottom_s, right_s, front_s),
        (bottom_s, back_s, right_s),
        (bottom_s, left_s, back_s),
        (bottom_s, front_s, left_s),
    ];

    for (v0, v1, v2) in faces {
        if let (Some(p0), Some(p1), Some(p2)) = (v0, v1, v2) {
            draw_filled_triangle_3d(fb, p0, p1, p2, color);
        }
    }

    // Draw edges for definition
    let edge_color = RasterColor::new(
        (color.r as u16 * 3 / 4) as u8,
        (color.g as u16 * 3 / 4) as u8,
        (color.b as u16 * 3 / 4) as u8,
    );

    // Draw edges using framebuffer's 3d line function
    let draw_edge = |fb: &mut Framebuffer, p0: Option<(i32, i32, f32)>, p1: Option<(i32, i32, f32)>| {
        if let (Some((x0, y0, z0)), Some((x1, y1, z1))) = (p0, p1) {
            fb.draw_line_3d(x0, y0, z0, x1, y1, z1, edge_color);
        }
    };

    draw_edge(fb, top_s, front_s);
    draw_edge(fb, top_s, back_s);
    draw_edge(fb, top_s, left_s);
    draw_edge(fb, top_s, right_s);
    draw_edge(fb, bottom_s, front_s);
    draw_edge(fb, bottom_s, back_s);
    draw_edge(fb, bottom_s, left_s);
    draw_edge(fb, bottom_s, right_s);
    draw_edge(fb, front_s, right_s);
    draw_edge(fb, right_s, back_s);
    draw_edge(fb, back_s, left_s);
    draw_edge(fb, left_s, front_s);
}

/// Draw a filled triangle (for gizmos)
fn draw_filled_triangle_3d(
    fb: &mut Framebuffer,
    p0: (i32, i32, f32),
    p1: (i32, i32, f32),
    p2: (i32, i32, f32),
    color: RasterColor,
) {
    // Sort vertices by y coordinate
    let mut pts = [(p0.0, p0.1), (p1.0, p1.1), (p2.0, p2.1)];
    pts.sort_by(|a, b| a.1.cmp(&b.1));
    let (x0, y0) = pts[0];
    let (x1, y1) = pts[1];
    let (x2, y2) = pts[2];

    if y2 == y0 { return; } // Degenerate triangle

    let total_height = (y2 - y0) as f32;

    for y in y0.max(0)..=y2.min(fb.height as i32 - 1) {
        let second_half = y > y1 || y1 == y0;
        let segment_height = if second_half {
            (y2 - y1) as f32
        } else {
            (y1 - y0) as f32
        };

        if segment_height == 0.0 { continue; }

        let alpha = (y - y0) as f32 / total_height;
        let beta = if second_half {
            (y - y1) as f32 / segment_height
        } else {
            (y - y0) as f32 / segment_height
        };

        let mut ax = x0 as f32 + (x2 - x0) as f32 * alpha;
        let mut bx = if second_half {
            x1 as f32 + (x2 - x1) as f32 * beta
        } else {
            x0 as f32 + (x1 - x0) as f32 * beta
        };

        if ax > bx {
            std::mem::swap(&mut ax, &mut bx);
        }

        let x_start = (ax as i32).max(0);
        let x_end = (bx as i32).min(fb.width as i32 - 1);

        for x in x_start..=x_end {
            let idx = (y as usize * fb.width + x as usize) * 4;
            if idx + 3 < fb.pixels.len() {
                fb.pixels[idx] = color.r;
                fb.pixels[idx + 1] = color.g;
                fb.pixels[idx + 2] = color.b;
                fb.pixels[idx + 3] = 255;
            }
        }
    }
}
