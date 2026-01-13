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
};
use super::state::{ModelerState, ModelerSelection, SelectMode, Axis, ModalTransform, CameraMode, ViewportId};
use super::drag::{DragUpdateResult, ActiveDrag};
use super::tools::ModelerToolId;

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
    let mesh = state.mesh();

    match &state.selection {
        ModelerSelection::Vertices(verts) => {
            for &idx in verts {
                if let Some(vert) = mesh.vertices.get(idx) {
                    positions.push(vert.pos);
                }
            }
        }
        ModelerSelection::Edges(edges) => {
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
            _ => {}
        }
    }

    // Second pass: apply movements (mutable)
    let mirror_settings = state.mirror_settings;
    if let Some(mesh) = state.mesh_mut() {
        let mut already_moved = std::collections::HashSet::new();
        for (idx, old_pos, new_pos) in &movements {
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
    let mirror_settings = state.mirror_settings;
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

    // Cancel on right click (Escape is handled through ActionRegistry in handle_actions())
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

                        // Start free move drag (axis = None for screen-space movement)
                        state.drag_manager.start_move(
                            center,
                            drag_start_mouse,
                            None,      // No axis = free movement
                            indices,
                            initial_positions,
                            state.snap_settings.enabled,
                            state.snap_settings.grid_size,
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

            // Save undo state before starting transform
            state.push_undo(mode.label());

            // Start the appropriate DragManager drag and sync tool state
            match mode {
                ModalTransform::Grab => {
                    state.tool_box.tools.move_tool.start_drag(None);
                    state.drag_manager.start_move(
                        center,
                        mouse_pos,
                        None, // No axis constraint initially
                        indices,
                        initial_positions,
                        state.snap_settings.enabled,
                        state.snap_settings.grid_size,
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
                    state.tool_box.tools.rotate.start_drag(Some(UiAxis::Y), 0.0);
                    // For rotation, initial angle is 0
                    state.drag_manager.start_rotate(
                        center,
                        0.0, // initial angle
                        mouse_pos,
                        mouse_pos, // Use mouse_pos as center for screen-space rotation
                        UiAxis::Y, // Default to Y axis rotation
                        indices,
                        initial_positions,
                        state.snap_settings.enabled,
                        15.0, // 15-degree snap increments
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
    if state.mirror_settings.enabled {
        let mirror_color = RasterColor::new(255, 100, 100); // Pinkish-red for mirror plane
        let extent = crate::world::SECTOR_SIZE * 5.0; // Large enough to be visible

        // Draw cross lines along the mirror plane
        match state.mirror_settings.axis {
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

    // Render all visible objects
    // Use RGB555 or RGB888 based on settings
    let use_rgb555 = state.raster_settings.use_rgb555;

    // Log once per render frame (use frame counter if available, or static counter)
    static mut FRAME_COUNTER: u32 = 0;
    let frame_num = unsafe { FRAME_COUNTER += 1; FRAME_COUNTER };

    // Only log every 60 frames to reduce spam
    let should_log = frame_num % 60 == 0;

    // Fallback CLUT for objects with no assigned CLUT
    let fallback_clut = state.project.clut_pool.first_id()
        .and_then(|id| state.project.clut_pool.get(id));

    for (obj_idx, obj) in state.project.objects.iter().enumerate() {
        // Skip hidden objects
        if !obj.visible {
            continue;
        }

        // Get this object's CLUT from the shared pool (per-object atlas.default_clut)
        let obj_clut = if obj.atlas.default_clut.is_valid() {
            state.project.clut_pool.get(obj.atlas.default_clut).or(fallback_clut)
        } else {
            fallback_clut
        };

        if should_log {
            // Log first 8 indices to see if atlas data differs between objects
            let indices_preview: Vec<u8> = obj.atlas.indices.iter().take(8).copied().collect();
            eprintln!("[DEBUG render] obj[{}] '{}': atlas {}x{} depth={:?} clut={:?} indices[0..8]={:?}",
                obj_idx, obj.name, obj.atlas.width, obj.atlas.height, obj.atlas.depth,
                obj.atlas.default_clut, indices_preview);
        }

        // Convert this object's atlas to rasterizer texture using its own CLUT
        let atlas_texture = obj_clut.map(|c| obj.atlas.to_raster_texture(c, &format!("atlas_{}", obj_idx)));
        let atlas_texture_15 = if use_rgb555 {
            obj_clut.map(|c| obj.atlas.to_texture15(c, &format!("atlas_{}", obj_idx)))
        } else {
            None
        };

        // Use project mesh directly (mesh() accessor returns selected object's mesh)
        let mesh = &obj.mesh;

        // Dim non-selected objects slightly
        let base_color = if state.project.selected_object == Some(obj_idx) {
            180u8
        } else {
            140u8
        };

        let vertices: Vec<RasterVertex> = mesh.vertices.iter().map(|v| {
            RasterVertex {
                pos: v.pos,
                normal: v.normal,
                uv: v.uv,
                color: RasterColor::new(base_color, base_color, base_color),
                bone_index: None,
            }
        }).collect();

        // Triangulate n-gon faces for rendering (all use texture 0 - the atlas)
        let mut faces: Vec<RasterFace> = Vec::new();
        for edit_face in &mesh.faces {
            for [v0, v1, v2] in edit_face.triangulate() {
                faces.push(RasterFace {
                    v0,
                    v1,
                    v2,
                    texture_id: Some(0),
                    black_transparent: edit_face.black_transparent,
                    blend_mode: edit_face.blend_mode,
                });
            }
        }

        if !vertices.is_empty() && !faces.is_empty() {
            // Collect per-face blend modes
            let blend_modes: Vec<crate::rasterizer::BlendMode> = faces.iter()
                .map(|f| f.blend_mode)
                .collect();

            if use_rgb555 {
                // RGB555 rendering path
                // Per-face blend modes + per-pixel STP bit (PS1-authentic)
                if let Some(ref tex15) = atlas_texture_15 {
                    let textures_15 = [tex15.clone()];
                    render_mesh_15(
                        fb,
                        &vertices,
                        &faces,
                        &textures_15,
                        Some(&blend_modes),
                        &state.camera,
                        &state.raster_settings,
                        None,
                    );
                }
            } else {
                // RGB888 rendering path (original)
                if let Some(ref tex) = atlas_texture {
                    let textures = [tex.clone()];
                    render_mesh(
                        fb,
                        &vertices,
                        &faces,
                        &textures,
                        &state.camera,
                        &state.raster_settings,
                    );
                }
            }

            // Render mirrored geometry if mirror mode is enabled
            if state.mirror_settings.enabled {
                // Create mirrored vertices
                let mirrored_vertices: Vec<RasterVertex> = vertices.iter().map(|v| {
                    RasterVertex {
                        pos: state.mirror_settings.mirror_position(v.pos),
                        normal: state.mirror_settings.mirror_normal(v.normal),
                        uv: v.uv,
                        // Slightly dim the mirrored side to indicate it's generated
                        color: RasterColor::new(
                            (base_color as f32 * 0.85) as u8,
                            (base_color as f32 * 0.85) as u8,
                            (base_color as f32 * 0.85) as u8,
                        ),
                        bone_index: None,
                    }
                }).collect();

                // Create mirrored faces with reversed winding order
                let mirrored_faces: Vec<RasterFace> = faces.iter().map(|f| {
                    RasterFace {
                        v0: f.v0,
                        v1: f.v2,  // Swap v1 and v2 to reverse winding
                        v2: f.v1,
                        texture_id: f.texture_id,
                        black_transparent: f.black_transparent,
                        blend_mode: f.blend_mode,
                    }
                }).collect();

                if use_rgb555 {
                    if let Some(ref tex15) = atlas_texture_15 {
                        let textures_15 = [tex15.clone()];
                        render_mesh_15(
                            fb,
                            &mirrored_vertices,
                            &mirrored_faces,
                            &textures_15,
                            Some(&blend_modes),
                            &state.camera,
                            &state.raster_settings,
                            None,
                        );
                    }
                } else {
                    if let Some(ref tex) = atlas_texture {
                        let textures = [tex.clone()];
                        render_mesh(
                            fb,
                            &mirrored_vertices,
                            &mirrored_faces,
                            &textures,
                            &state.camera,
                            &state.raster_settings,
                        );
                    }
                }
            }
        }
    }

    // Draw selection overlays
    draw_mesh_selection_overlays(state, fb);

    // Draw box selection preview (highlight elements that would be selected)
    if state.box_select_viewport == Some(viewport_id) {
        if let ActiveDrag::BoxSelect(tracker) = &state.drag_manager.active {
            let (min_x, min_y, max_x, max_y) = tracker.bounds();
            // Convert screen coords to framebuffer coords
            let fb_x0 = (min_x - draw_x) / draw_w * fb_width as f32;
            let fb_y0 = (min_y - draw_y) / draw_h * fb_height as f32;
            let fb_x1 = (max_x - draw_x) / draw_w * fb_width as f32;
            let fb_y1 = (max_y - draw_y) / draw_h * fb_height as f32;
            draw_box_selection_preview(state, fb, fb_x0, fb_y0, fb_x1, fb_y1, viewport_id);
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

    // Draw and handle transform gizmo (on top of the framebuffer)
    handle_transform_gizmo(ctx, state, mouse_pos, inside_viewport, draw_x, draw_y, draw_w, draw_h, fb_width, fb_height, viewport_id);

    // Update hover state every frame (like world editor) - but not when gizmo is active
    // Only update for the active viewport to prevent viewports from overwriting each other's hover state
    let is_active_viewport = state.active_viewport == viewport_id;
    if is_active_viewport && !state.drag_manager.is_dragging() && state.gizmo_hovered_axis.is_none() {
        update_hover_state(state, mouse_pos, draw_x, draw_y, draw_w, draw_h, fb_width, fb_height);
    }

    // Handle box selection (left-drag without hitting an element or gizmo)
    // Uses DragManager for tracking
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
                // Semi-transparent fill
                draw_rectangle(clip_min_x, clip_min_y, clip_max_x - clip_min_x, clip_max_y - clip_min_y, Color::from_rgba(100, 150, 255, 50));
                // Border
                draw_rectangle_lines(clip_min_x, clip_min_y, clip_max_x - clip_min_x, clip_max_y - clip_min_y, 1.0, Color::from_rgba(100, 150, 255, 200));
            }
        }
    }

    // Handle single-click selection using hover system (like world editor)
    // Only if not clicking on gizmo
    if inside_viewport && ctx.mouse.left_pressed
        && state.modal_transform == ModalTransform::None
        && state.gizmo_hovered_axis.is_none()
        && !state.drag_manager.is_dragging()
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
    // Don't start box select during modal transforms or other drags
    if state.modal_transform != ModalTransform::None {
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
                // Use face center for box selection (average of all vertices)
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

/// Draw selection and hover overlays for mesh editing (like world editor)
fn draw_mesh_selection_overlays(state: &ModelerState, fb: &mut Framebuffer) {
    let mesh = state.mesh();
    let camera = &state.camera;
    let ortho = state.raster_settings.ortho_projection.as_ref();

    let hover_color = RasterColor::new(255, 200, 150);   // Orange for hover
    let select_color = RasterColor::new(100, 180, 255);  // Blue for selection
    let edge_overlay_color = RasterColor::new(80, 80, 80);  // Gray for edge overlay

    // =========================================================================
    // Draw all edges with semi-transparent overlay (always visible in solid mode)
    // Skip if in pure wireframe mode (edges already shown via backface_wireframe)
    // Uses depth testing so edges respect z-order
    // =========================================================================
    if !state.raster_settings.wireframe_overlay {
        for face in &mesh.faces {
            for (v0_idx, v1_idx) in face.edges() {
                if let (Some(v0), Some(v1)) = (mesh.vertices.get(v0_idx), mesh.vertices.get(v1_idx)) {
                    if let (Some((sx0, sy0, z0)), Some((sx1, sy1, z1))) = (
                        world_to_screen_with_ortho_depth(v0.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height, ortho),
                        world_to_screen_with_ortho_depth(v1.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height, ortho),
                    ) {
                        fb.draw_line_3d_alpha(sx0 as i32, sy0 as i32, z0, sx1 as i32, sy1 as i32, z1, edge_overlay_color, 191);
                    }
                }
            }
        }

        // Draw vertex dots with semi-transparent overlay
        let vertex_overlay_color = RasterColor::new(40, 40, 50);
        for vert in &mesh.vertices {
            if let Some((sx, sy)) = world_to_screen_with_ortho(
                vert.pos,
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

    // =========================================================================
    // Draw hovered vertex (if any) - orange dot
    // =========================================================================
    if let Some(hovered_idx) = state.hovered_vertex {
        if let Some(vert) = mesh.vertices.get(hovered_idx) {
            if let Some((sx, sy)) = world_to_screen_with_ortho(
                vert.pos,
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
        if let (Some(v0), Some(v1)) = (mesh.vertices.get(v0_idx), mesh.vertices.get(v1_idx)) {
            if let (Some((sx0, sy0)), Some((sx1, sy1))) = (
                world_to_screen_with_ortho(v0.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height, ortho),
                world_to_screen_with_ortho(v1.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height, ortho),
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
                .filter_map(|&vi| mesh.vertices.get(vi))
                .filter_map(|v| world_to_screen_with_ortho(v.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height, ortho))
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
            if let Some(vert) = mesh.vertices.get(idx) {
                if let Some((sx, sy)) = world_to_screen_with_ortho(
                    vert.pos,
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
            if let (Some(v0), Some(v1)) = (mesh.vertices.get(*v0_idx), mesh.vertices.get(*v1_idx)) {
                if let (Some((sx0, sy0)), Some((sx1, sy1))) = (
                    world_to_screen_with_ortho(v0.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height, ortho),
                    world_to_screen_with_ortho(v1.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height, ortho),
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
                    .filter_map(|&vi| mesh.vertices.get(vi).map(|v| v.pos))
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

    match state.select_mode {
        SelectMode::Vertex => {
            // Highlight vertices inside the box
            for vert in &mesh.vertices {
                if let Some((sx, sy)) = world_to_screen_with_ortho(
                    vert.pos,
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
        SelectMode::Edge => {
            // Highlight edges where both vertices are inside the box
            let mut drawn_edges = std::collections::HashSet::new();
            for face in &mesh.faces {
                for (v0_idx, v1_idx) in face.edges() {
                    let edge = (v0_idx.min(v1_idx), v0_idx.max(v1_idx));
                    if drawn_edges.contains(&edge) {
                        continue;
                    }

                    if let (Some(v0), Some(v1)) = (mesh.vertices.get(v0_idx), mesh.vertices.get(v1_idx)) {
                        if let (Some((sx0, sy0)), Some((sx1, sy1))) = (
                            world_to_screen_with_ortho(v0.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height, ortho),
                            world_to_screen_with_ortho(v1.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height, ortho),
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
                let verts: Vec<_> = face.vertices.iter()
                    .filter_map(|&vi| mesh.vertices.get(vi))
                    .collect();
                if !verts.is_empty() {
                    let center = verts.iter().map(|v| v.pos).fold(Vec3::ZERO, |acc, p| acc + p) * (1.0 / verts.len() as f32);
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
                            let screen_positions: Vec<_> = verts.iter()
                                .filter_map(|v| world_to_screen_with_ortho(v.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height, ortho))
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
    let (mouse_fb_x, mouse_fb_y) = mouse_fb;
    let camera = &state.camera;
    let mesh = state.mesh();
    let ortho = state.raster_settings.ortho_projection.as_ref();

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
            if let (Some(v0), Some(v1), Some(v2)) = (
                mesh.vertices.get(face.vertices[0]),
                mesh.vertices.get(face.vertices[1]),
                mesh.vertices.get(face.vertices[2]),
            ) {
                // Use screen-space signed area for backface culling (same as rasterizer)
                if let (Some((sx0, sy0)), Some((sx1, sy1)), Some((sx2, sy2))) = (
                    world_to_screen_with_ortho(v0.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height, ortho),
                    world_to_screen_with_ortho(v1.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height, ortho),
                    world_to_screen_with_ortho(v2.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height, ortho),
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

    // Check vertices first (highest priority) - only if on front-facing face (unless X-ray mode)
    for (idx, vert) in mesh.vertices.iter().enumerate() {
        if !state.xray_mode && !vertex_on_front_face[idx] {
            continue; // Skip vertices only on backfaces (X-ray allows selecting through)
        }
        // Skip vertices on the non-editable side when mirror is enabled
        if !state.mirror_settings.is_editable_side(vert.pos) {
            continue;
        }
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
            let dist = ((mouse_fb_x - sx).powi(2) + (mouse_fb_y - sy).powi(2)).sqrt();
            if dist < VERTEX_THRESHOLD {
                if hovered_vertex.map_or(true, |(_, best_dist)| dist < best_dist) {
                    hovered_vertex = Some((idx, dist));
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

                // Skip edges only on backfaces (unless X-ray mode)
                if !state.xray_mode && !edge_on_front_face.contains(&edge) {
                    continue;
                }

                if let (Some(v0), Some(v1)) = (mesh.vertices.get(v0_idx), mesh.vertices.get(v1_idx)) {
                    // Skip edges with any vertex on non-editable side when mirror is enabled
                    if !state.mirror_settings.is_editable_side(v0.pos) || !state.mirror_settings.is_editable_side(v1.pos) {
                        continue;
                    }
                    if let (Some((sx0, sy0)), Some((sx1, sy1))) = (
                        world_to_screen_with_ortho(v0.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height, ortho),
                        world_to_screen_with_ortho(v1.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height, ortho),
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
            let face_on_editable_side = face.vertices.iter().all(|&vi| {
                mesh.vertices.get(vi)
                    .map(|v| state.mirror_settings.is_editable_side(v.pos))
                    .unwrap_or(false)
            });
            if !face_on_editable_side {
                continue;
            }
            // Triangulate the n-gon face and check each triangle
            for [i0, i1, i2] in face.triangulate() {
                if let (Some(v0), Some(v1), Some(v2)) = (
                    mesh.vertices.get(i0),
                    mesh.vertices.get(i1),
                    mesh.vertices.get(i2),
                ) {
                    // Use screen-space signed area for backface culling (same as rasterizer)
                    if let (Some((sx0, sy0)), Some((sx1, sy1)), Some((sx2, sy2))) = (
                        world_to_screen_with_ortho(v0.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height, ortho),
                        world_to_screen_with_ortho(v1.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height, ortho),
                        world_to_screen_with_ortho(v2.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height, ortho),
                    ) {
                        // 2D screen-space signed area (PS1-style) - positive = front-facing
                        let signed_area = (sx1 - sx0) * (sy2 - sy0) - (sx2 - sx0) * (sy1 - sy0);
                        if !state.xray_mode && signed_area <= 0.0 {
                            continue; // Backface - skip (X-ray allows selecting through)
                        }

                        // Check if mouse is inside the triangle
                        if point_in_triangle_2d(mouse_fb_x, mouse_fb_y, sx0, sy0, sx1, sy1, sx2, sy2) {
                            // Calculate depth at mouse position for Z-ordering
                            let depth = interpolate_depth_in_triangle(
                                mouse_fb_x, mouse_fb_y,
                                sx0, sy0, (v0.pos - camera.position).dot(camera.basis_z),
                                sx1, sy1, (v1.pos - camera.position).dot(camera.basis_z),
                                sx2, sy2, (v2.pos - camera.position).dot(camera.basis_z),
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
) {
    // Don't update hover during transforms or box select
    if state.modal_transform != ModalTransform::None || state.drag_manager.is_dragging() {
        state.hovered_vertex = None;
        state.hovered_edge = None;
        state.hovered_face = None;
        return;
    }

    // Check if mouse is inside viewport
    let inside = mouse_pos.0 >= draw_x && mouse_pos.0 < draw_x + draw_w
              && mouse_pos.1 >= draw_y && mouse_pos.1 < draw_y + draw_h;

    if !inside {
        state.hovered_vertex = None;
        state.hovered_edge = None;
        state.hovered_face = None;
        return;
    }

    // Convert screen to framebuffer coords
    let fb_x = (mouse_pos.0 - draw_x) / draw_w * fb_width as f32;
    let fb_y = (mouse_pos.1 - draw_y) / draw_h * fb_height as f32;

    let (vert, edge, face) = find_hovered_element(state, (fb_x, fb_y), fb_width, fb_height);
    state.hovered_vertex = vert;
    state.hovered_edge = edge;
    state.hovered_face = face;
}

/// Handle click on hovered element (replaces mode-based selection)
fn handle_hover_click(state: &mut ModelerState) {
    // Multi-select with Shift OR X key
    let multi_select = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift)
                    || is_key_down(KeyCode::X);

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
    let center = state.selection.compute_center(state.mesh())?;
    let camera = &state.camera;
    let ortho = state.raster_settings.ortho_projection.as_ref();

    let center_screen = match world_to_screen_with_ortho(center, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height, ortho) {
        Some((sx, sy)) => (draw_x + sx / fb_width as f32 * draw_w, draw_y + sy / fb_height as f32 * draw_h),
        None => return None,
    };

    let axis_dirs = [
        (Axis::X, Vec3::new(1.0, 0.0, 0.0), RED),
        (Axis::Y, Vec3::new(0.0, 1.0, 0.0), GREEN),
        (Axis::Z, Vec3::new(0.0, 0.0, 1.0), BLUE),
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

                        let mirror_settings = state.mirror_settings;
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
                    let mirror_settings = state.mirror_settings;
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
                    state.dirty = true;
                }
            }
        } else {
            // End drag - sync tool state
            state.tool_box.tools.move_tool.end_drag();
            state.drag_manager.end();
            state.ortho_drag_viewport = None;
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
        state.push_undo("Gizmo Move");

        // Set up ortho-specific tracking
        if is_ortho {
            state.ortho_drag_viewport = Some(viewport_id);
            let ortho_cam = state.get_ortho_camera(viewport_id);
            state.ortho_drag_zoom = ortho_cam.zoom;
        }

        // Start drag with DragManager and sync tool state
        let ui_axis = to_ui_axis(axis);
        state.tool_box.tools.move_tool.start_drag(Some(ui_axis));

        if is_ortho {
            // For ortho, use screen coordinates (we calculate delta from screen, not ray casting)
            state.drag_manager.start_move(
                setup.center,
                mouse_pos,  // Use screen coordinates directly
                Some(ui_axis),
                indices,
                initial_positions,
                state.snap_settings.enabled,
                state.snap_settings.grid_size,
            );
        } else {
            // For perspective, use framebuffer coordinates for ray casting
            let fb_mouse = (
                (mouse_pos.0 - draw_x) / draw_w * fb_width as f32,
                (mouse_pos.1 - draw_y) / draw_h * fb_height as f32,
            );
            state.drag_manager.start_move_3d(
                setup.center,
                fb_mouse,
                Some(ui_axis),
                indices,
                initial_positions,
                state.snap_settings.enabled,
                state.snap_settings.grid_size,
                &state.camera,
                fb_width,
                fb_height,
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
            // Rotation uses screen-space coordinates for angle calculation
            let result = state.drag_manager.update(
                mouse_pos,  // screen-space mouse position
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

        // Calculate initial angle (for screen-space rotation)
        let start_vec = (
            mouse_pos.0 - setup.center_screen.0,
            mouse_pos.1 - setup.center_screen.1,
        );
        let initial_angle = start_vec.1.atan2(start_vec.0);

        // Save undo state BEFORE starting the gizmo drag
        state.push_undo("Gizmo Rotate");

        // Start drag with DragManager and sync tool state
        let ui_axis = to_ui_axis(axis);
        state.tool_box.tools.rotate.start_drag(Some(ui_axis), initial_angle);
        state.drag_manager.start_rotate(
            setup.center,
            initial_angle,
            mouse_pos,           // screen-space mouse
            setup.center_screen, // screen-space center for angle calculation
            ui_axis,
            indices,
            initial_positions,
            state.snap_settings.enabled,
            15.0, // Snap to 15-degree increments
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
