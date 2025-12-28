//! 3D Viewport for the modeler - renders meshes using the software rasterizer
//!
//! Simplified PicoCAD-style viewport with mesh editing only.

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext, Axis as UiAxis};
use crate::rasterizer::{
    Framebuffer, render_mesh, Color as RasterColor, Vec3,
    Vertex as RasterVertex, Face as RasterFace, WIDTH, HEIGHT,
    world_to_screen,
};
use super::state::{ModelerState, ModelerSelection, SelectMode, Axis, ModalTransform};
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
    let mesh = &state.mesh;

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
                    if let Some(v0) = mesh.vertices.get(face.v0) {
                        positions.push(v0.pos);
                    }
                    if let Some(v1) = mesh.vertices.get(face.v1) {
                        positions.push(v1.pos);
                    }
                    if let Some(v2) = mesh.vertices.get(face.v2) {
                        positions.push(v2.pos);
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

    match &selection {
        ModelerSelection::Vertices(verts) => {
            for &vert_idx in verts {
                if let Some(vert) = state.mesh.vertices.get(vert_idx) {
                    if let Some(&new_pos) = positions.get(pos_idx) {
                        movements.push((vert_idx, vert.pos, new_pos));
                    }
                    pos_idx += 1;
                }
            }
        }
        ModelerSelection::Edges(edges) => {
            for (v0, v1) in edges {
                if let Some(vert) = state.mesh.vertices.get(*v0) {
                    if let Some(&new_pos) = positions.get(pos_idx) {
                        movements.push((*v0, vert.pos, new_pos));
                    }
                    pos_idx += 1;
                }
                if let Some(vert) = state.mesh.vertices.get(*v1) {
                    if let Some(&new_pos) = positions.get(pos_idx) {
                        movements.push((*v1, vert.pos, new_pos));
                    }
                    pos_idx += 1;
                }
            }
        }
        ModelerSelection::Faces(faces) => {
            for &face_idx in faces {
                if let Some(face) = state.mesh.faces.get(face_idx).cloned() {
                    if let Some(vert) = state.mesh.vertices.get(face.v0) {
                        if let Some(&new_pos) = positions.get(pos_idx) {
                            movements.push((face.v0, vert.pos, new_pos));
                        }
                        pos_idx += 1;
                    }
                    if let Some(vert) = state.mesh.vertices.get(face.v1) {
                        if let Some(&new_pos) = positions.get(pos_idx) {
                            movements.push((face.v1, vert.pos, new_pos));
                        }
                        pos_idx += 1;
                    }
                    if let Some(vert) = state.mesh.vertices.get(face.v2) {
                        if let Some(&new_pos) = positions.get(pos_idx) {
                            movements.push((face.v2, vert.pos, new_pos));
                        }
                        pos_idx += 1;
                    }
                }
            }
        }
        _ => {}
    }

    // Apply movements, expanding to coincident vertices if linking enabled
    let mut already_moved = std::collections::HashSet::new();
    for (idx, old_pos, new_pos) in &movements {
        let delta = *new_pos - *old_pos;

        if linking {
            // Find all coincident vertices and move them by the same delta
            let coincident = state.mesh.find_coincident_vertices(*idx, LINK_EPSILON);
            for ci in coincident {
                if !already_moved.contains(&ci) {
                    if let Some(vert) = state.mesh.vertices.get_mut(ci) {
                        vert.pos = vert.pos + delta;
                    }
                    already_moved.insert(ci);
                }
            }
        } else {
            // Just move the single vertex
            if !already_moved.contains(idx) {
                if let Some(vert) = state.mesh.vertices.get_mut(*idx) {
                    vert.pos = *new_pos;
                }
                already_moved.insert(*idx);
            }
        }
    }

    // Sync geometry changes to project so UV editor stays in sync
    state.sync_mesh_to_project();
}

/// Handle modal transforms (G=Grab, S=Scale, R=Rotate) using DragManager
fn handle_modal_transform(state: &mut ModelerState, mouse_pos: (f32, f32)) {
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
    match result {
        DragUpdateResult::Move { positions, .. } => {
            for (vert_idx, new_pos) in positions {
                if let Some(vert) = state.mesh.vertices.get_mut(vert_idx) {
                    vert.pos = new_pos;
                }
            }
            state.dirty = true;
        }
        DragUpdateResult::Scale { positions, .. } => {
            for (vert_idx, new_pos) in positions {
                if let Some(vert) = state.mesh.vertices.get_mut(vert_idx) {
                    vert.pos = new_pos;
                }
            }
            state.dirty = true;
        }
        DragUpdateResult::Rotate { positions, .. } => {
            for (vert_idx, new_pos) in positions {
                if let Some(vert) = state.mesh.vertices.get_mut(vert_idx) {
                    vert.pos = new_pos;
                }
            }
            state.dirty = true;
        }
        _ => {}
    }

    // Confirm on left click
    if is_mouse_button_pressed(MouseButton::Left) {
        if let Some(_result) = state.drag_manager.end() {
            state.sync_mesh_to_project();
        }
        state.modal_transform = ModalTransform::None;
        state.dirty = true;
        state.set_status("Transform applied", 1.0);
    }

    // Cancel on right click (Escape is handled through ActionRegistry in handle_actions())
    if is_mouse_button_pressed(MouseButton::Right) {
        if let Some(original_positions) = state.drag_manager.cancel() {
            for (vert_idx, original_pos) in original_positions {
                if let Some(vert) = state.mesh.vertices.get_mut(vert_idx) {
                    vert.pos = original_pos;
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

    if is_free_moving {
        if ctx.mouse.left_down {
            // Update the drag with current mouse position
            let result = state.drag_manager.update(
                mouse_pos,
                &state.camera,
                fb_width,
                fb_height,
            );

            if let DragUpdateResult::Move { positions, .. } = result {
                let snap_disabled = is_key_down(KeyCode::Z);
                for (vert_idx, new_pos) in positions {
                    if let Some(vert) = state.mesh.vertices.get_mut(vert_idx) {
                        vert.pos = if state.snap_settings.enabled && !snap_disabled {
                            state.snap_settings.snap_vec3(new_pos)
                        } else {
                            new_pos
                        };
                    }
                }
                state.dirty = true;
            }
        } else {
            // End drag on mouse release
            if let Some(_result) = state.drag_manager.end() {
                state.sync_mesh_to_project();
            }
            state.set_status("Moved", 0.5);
        }

        // Cancel drag on right-click
        if is_mouse_button_pressed(MouseButton::Right) {
            if let Some(original_positions) = state.drag_manager.cancel() {
                for (vert_idx, original_pos) in original_positions {
                    if let Some(vert) = state.mesh.vertices.get_mut(vert_idx) {
                        vert.pos = original_pos;
                    }
                }
            }
            state.set_status("Move cancelled", 0.5);
        }
    } else if !state.drag_manager.is_dragging() {
        // Not in any drag - check for free move start
        // Store potential start position (similar to box select)
        if is_mouse_button_pressed(MouseButton::Left) && inside_viewport {
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
                    let mut indices = state.selection.get_affected_vertex_indices(&state.mesh);
                    if state.vertex_linking {
                        indices = state.mesh.expand_to_coincident(&indices, 0.001);
                    }

                    let initial_positions: Vec<(usize, Vec3)> = indices.iter()
                        .filter_map(|&idx| state.mesh.vertices.get(idx).map(|v| (idx, v.pos)))
                        .collect();

                    if !initial_positions.is_empty() {
                        // Calculate center
                        let sum: Vec3 = initial_positions.iter().map(|(_, p)| *p).fold(Vec3::ZERO, |acc, p| acc + p);
                        let center = sum * (1.0 / initial_positions.len() as f32);

                        // Save undo state before starting
                        state.push_undo("Drag move");

                        // Start free move drag (axis = None for screen-space movement)
                        state.drag_manager.start_move(
                            center,
                            start_pos, // Use the position where drag started
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

/// Draw the 3D modeler viewport
pub fn draw_modeler_viewport(
    ctx: &mut UiContext,
    rect: Rect,
    state: &mut ModelerState,
    fb: &mut Framebuffer,
) {
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

    // Orbit camera controls
    let shift_held = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);

    // Right mouse drag: rotate around target (or pan if Shift held)
    if ctx.mouse.right_down && (inside_viewport || state.viewport_mouse_captured) {
        if state.viewport_mouse_captured {
            let dx = mouse_pos.0 - state.viewport_last_mouse.0;
            let dy = mouse_pos.1 - state.viewport_last_mouse.1;

            if shift_held {
                let pan_speed = state.orbit_distance * 0.002;
                state.orbit_target = state.orbit_target - state.camera.basis_x * dx * pan_speed;
                state.orbit_target = state.orbit_target - state.camera.basis_y * dy * pan_speed;
            } else {
                state.orbit_azimuth += dx * 0.005;
                state.orbit_elevation = (state.orbit_elevation + dy * 0.005).clamp(-1.4, 1.4);
            }
            state.sync_camera_from_orbit();
        }
        state.viewport_mouse_captured = true;
    } else if !ctx.mouse.right_down {
        state.viewport_mouse_captured = false;
    }

    // Mouse wheel: zoom
    if inside_viewport {
        let scroll = ctx.mouse.scroll;
        if scroll != 0.0 {
            let zoom_factor = if scroll > 0.0 { 0.98 } else { 1.02 };
            state.orbit_distance = (state.orbit_distance * zoom_factor).clamp(50.0, 2000.0);
            state.sync_camera_from_orbit();
        }
    }

    state.viewport_last_mouse = mouse_pos;

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
        let mut indices = state.selection.get_affected_vertex_indices(&state.mesh);
        if state.vertex_linking {
            indices = state.mesh.expand_to_coincident(&indices, 0.001);
        }

        let initial_positions: Vec<(usize, Vec3)> = indices.iter()
            .filter_map(|&idx| state.mesh.vertices.get(idx).map(|v| (idx, v.pos)))
            .collect();

        if !initial_positions.is_empty() {
            // Calculate center
            let sum: Vec3 = initial_positions.iter().map(|(_, p)| *p).fold(Vec3::ZERO, |acc, p| acc + p);
            let center = sum * (1.0 / initial_positions.len() as f32);

            // Save undo state before starting transform
            state.push_undo(mode.label());

            // Start the appropriate DragManager drag
            match mode {
                ModalTransform::Grab => {
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
                    state.drag_manager.start_scale(
                        center,
                        mouse_pos,
                        None, // No axis constraint initially
                        indices,
                        initial_positions,
                    );
                }
                ModalTransform::Rotate => {
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

    handle_modal_transform(state, mouse_pos);

    // Handle left-click drag to move selection (if not in modal transform)
    handle_drag_move(ctx, state, mouse_pos, inside_viewport, fb_width, fb_height);

    // Clear and render
    fb.clear(RasterColor::new(30, 30, 35));

    // Draw grid
    draw_grid(fb, &state.camera, 0.0, 50.0, 10);

    // Render mesh with texture atlas
    let mesh = &state.mesh;
    let vertices: Vec<RasterVertex> = mesh.vertices.iter().map(|v| {
        RasterVertex {
            pos: v.pos,
            normal: v.normal,
            uv: v.uv,
            color: RasterColor::new(180, 180, 180),
            bone_index: None,
        }
    }).collect();

    // All faces use texture 0 (the atlas)
    let faces: Vec<RasterFace> = mesh.faces.iter().map(|f| {
        RasterFace {
            v0: f.v0,
            v1: f.v1,
            v2: f.v2,
            texture_id: Some(0),
        }
    }).collect();

    if !vertices.is_empty() && !faces.is_empty() {
        // Convert atlas to rasterizer texture
        let atlas_texture = state.project.atlas.to_raster_texture();
        let textures = [atlas_texture];

        render_mesh(
            fb,
            &vertices,
            &faces,
            &textures,
            &state.camera,
            &state.raster_settings,
        );
    }

    // Draw selection overlays
    draw_mesh_selection_overlays(state, fb);

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
    handle_transform_gizmo(ctx, state, mouse_pos, inside_viewport, draw_x, draw_y, draw_w, draw_h, fb_width, fb_height);

    // Update hover state every frame (like world editor) - but not when gizmo is active
    if !state.drag_manager.is_dragging() && state.gizmo_hovered_axis.is_none() {
        update_hover_state(state, mouse_pos, draw_x, draw_y, draw_w, draw_h, fb_width, fb_height);
    }

    // Handle box selection (left-drag without hitting an element or gizmo)
    // Uses DragManager for tracking
    if state.gizmo_hovered_axis.is_none() {
        handle_box_selection(ctx, state, mouse_pos, inside_viewport, draw_x, draw_y, draw_w, draw_h, fb_width, fb_height);
    }

    // Draw box selection overlay if DragManager has active box select
    if let ActiveDrag::BoxSelect(tracker) = &state.drag_manager.active {
        let (min_x, min_y, max_x, max_y) = tracker.bounds();

        // Semi-transparent fill
        draw_rectangle(min_x, min_y, max_x - min_x, max_y - min_y, Color::from_rgba(100, 150, 255, 50));
        // Border
        draw_rectangle_lines(min_x, min_y, max_x - min_x, max_y - min_y, 1.0, Color::from_rgba(100, 150, 255, 200));
    }

    // Handle single-click selection using hover system (like world editor)
    // Only if not clicking on gizmo
    if inside_viewport && is_mouse_button_pressed(MouseButton::Left)
        && state.modal_transform == ModalTransform::None
        && state.gizmo_hovered_axis.is_none()
        && !state.drag_manager.is_dragging()
    {
        handle_hover_click(state);
    }
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
) {
    // Don't start box select during modal transforms or other drags
    if state.modal_transform != ModalTransform::None {
        return;
    }

    // Check if we're already in a box select drag
    let is_box_selecting = state.drag_manager.active.is_box_select();

    if is_box_selecting {
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

                apply_box_selection(state, fb_x0, fb_y0, fb_x1, fb_y1, fb_width, fb_height);
            }

            // End the drag
            state.drag_manager.end();
        }
    } else if !state.drag_manager.is_dragging() {
        // Not in any drag - check for box select start
        // We need to detect drag start vs click, so we track potential start position
        if is_mouse_button_pressed(MouseButton::Left) && inside_viewport {
            // Store potential start position (will become box select if dragged far enough)
            state.box_select_pending_start = Some(mouse_pos);
        }

        // Check if we should convert pending start to actual box select
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
                }
            } else {
                // Mouse released without dragging - clear pending
                state.box_select_pending_start = None;
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
) {
    let camera = &state.camera;
    let mesh = &state.mesh;

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
                if let Some((sx, sy)) = world_to_screen(
                    vert.pos,
                    camera.position,
                    camera.basis_x,
                    camera.basis_y,
                    camera.basis_z,
                    fb_width,
                    fb_height,
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
                state.selection = ModelerSelection::Vertices(selected);
                state.set_status(&format!("Selected {} vertex(es)", count), 0.5);
            } else if !add_to_selection {
                state.selection = ModelerSelection::None;
            }
        }
        SelectMode::Face => {
            let mut selected = if add_to_selection {
                if let ModelerSelection::Faces(f) = &state.selection { f.clone() } else { Vec::new() }
            } else {
                Vec::new()
            };

            for (idx, face) in mesh.faces.iter().enumerate() {
                // Use face center for box selection
                if let (Some(v0), Some(v1), Some(v2)) = (
                    mesh.vertices.get(face.v0),
                    mesh.vertices.get(face.v1),
                    mesh.vertices.get(face.v2),
                ) {
                    let center = (v0.pos + v1.pos + v2.pos) * (1.0 / 3.0);
                    if let Some((sx, sy)) = world_to_screen(
                        center,
                        camera.position,
                        camera.basis_x,
                        camera.basis_y,
                        camera.basis_z,
                        fb_width,
                        fb_height,
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
                state.selection = ModelerSelection::Faces(selected);
                state.set_status(&format!("Selected {} face(s)", count), 0.5);
            } else if !add_to_selection {
                state.selection = ModelerSelection::None;
            }
        }
        _ => {}
    }
}

/// Draw grid on the floor plane
fn draw_grid(fb: &mut Framebuffer, camera: &crate::rasterizer::Camera, y: f32, spacing: f32, count: i32) {
    let grid_color = RasterColor::new(50, 50, 55);
    let axis_color = RasterColor::new(80, 80, 85);

    for i in -count..=count {
        let offset = i as f32 * spacing;
        let color = if i == 0 { axis_color } else { grid_color };

        // Lines along X
        draw_3d_line(fb, camera,
            Vec3::new(-count as f32 * spacing, y, offset),
            Vec3::new(count as f32 * spacing, y, offset),
            color);

        // Lines along Z
        draw_3d_line(fb, camera,
            Vec3::new(offset, y, -count as f32 * spacing),
            Vec3::new(offset, y, count as f32 * spacing),
            color);
    }
}

/// Draw a 3D line using framebuffer coordinates
fn draw_3d_line(fb: &mut Framebuffer, camera: &crate::rasterizer::Camera, start: Vec3, end: Vec3, color: RasterColor) {
    let start_screen = world_to_screen(
        start,
        camera.position,
        camera.basis_x,
        camera.basis_y,
        camera.basis_z,
        fb.width,
        fb.height,
    );
    let end_screen = world_to_screen(
        end,
        camera.position,
        camera.basis_x,
        camera.basis_y,
        camera.basis_z,
        fb.width,
        fb.height,
    );

    if let (Some((x0, y0)), Some((x1, y1))) = (start_screen, end_screen) {
        fb.draw_line(x0 as i32, y0 as i32, x1 as i32, y1 as i32, color);
    }
}

/// Draw selection and hover overlays for mesh editing (like world editor)
fn draw_mesh_selection_overlays(state: &ModelerState, fb: &mut Framebuffer) {
    let mesh = &state.mesh;
    let camera = &state.camera;

    let hover_color = RasterColor::new(255, 200, 150);   // Orange for hover
    let select_color = RasterColor::new(100, 180, 255);  // Blue for selection
    let vertex_normal = RasterColor::new(180, 180, 190); // Gray for unselected vertices

    // =========================================================================
    // Draw hovered vertex (if any) - orange dot
    // =========================================================================
    if let Some(hovered_idx) = state.hovered_vertex {
        if let Some(vert) = mesh.vertices.get(hovered_idx) {
            if let Some((sx, sy)) = world_to_screen(
                vert.pos,
                camera.position,
                camera.basis_x,
                camera.basis_y,
                camera.basis_z,
                fb.width,
                fb.height,
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
                world_to_screen(v0.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height),
                world_to_screen(v1.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height),
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
            if let (Some(v0), Some(v1), Some(v2)) = (
                mesh.vertices.get(face.v0),
                mesh.vertices.get(face.v1),
                mesh.vertices.get(face.v2),
            ) {
                // Draw face edges
                let positions = [v0.pos, v1.pos, v2.pos];
                let screen_positions: Vec<_> = positions.iter().filter_map(|p|
                    world_to_screen(*p, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height)
                ).collect();

                if screen_positions.len() == 3 {
                    for i in 0..3 {
                        let (sx0, sy0) = screen_positions[i];
                        let (sx1, sy1) = screen_positions[(i + 1) % 3];
                        fb.draw_line(sx0 as i32, sy0 as i32, sx1 as i32, sy1 as i32, hover_color);
                    }
                    // Draw diagonal to indicate it's a face
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
                if let Some((sx, sy)) = world_to_screen(
                    vert.pos,
                    camera.position,
                    camera.basis_x,
                    camera.basis_y,
                    camera.basis_z,
                    fb.width,
                    fb.height,
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
                    world_to_screen(v0.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height),
                    world_to_screen(v1.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height),
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
                if let (Some(v0), Some(v1), Some(v2)) = (
                    mesh.vertices.get(face.v0),
                    mesh.vertices.get(face.v1),
                    mesh.vertices.get(face.v2),
                ) {
                    let positions = [v0.pos, v1.pos, v2.pos];
                    let screen_positions: Vec<_> = positions.iter().filter_map(|p|
                        world_to_screen(*p, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height)
                    ).collect();

                    if screen_positions.len() == 3 {
                        for i in 0..3 {
                            let (sx0, sy0) = screen_positions[i];
                            let (sx1, sy1) = screen_positions[(i + 1) % 3];
                            fb.draw_line(sx0 as i32, sy0 as i32, sx1 as i32, sy1 as i32, select_color);
                            // Thicker line
                            fb.draw_line(sx0 as i32 + 1, sy0 as i32, sx1 as i32 + 1, sy1 as i32, select_color);
                        }
                        // Draw center dot
                        let center = (v0.pos + v1.pos + v2.pos) * (1.0 / 3.0);
                        if let Some((cx, cy)) = world_to_screen(
                            center,
                            camera.position,
                            camera.basis_x,
                            camera.basis_y,
                            camera.basis_z,
                            fb.width,
                            fb.height,
                        ) {
                            fb.draw_circle(cx as i32, cy as i32, 4, select_color);
                        }
                    }
                }
            }
        }
    }

    // =========================================================================
    // Draw all vertices as small dots (only when nothing is hovered/selected, or when in vertex selection)
    // This provides visual feedback of where vertices are
    // =========================================================================
    let show_all_verts = state.hovered_vertex.is_some() ||
                         matches!(&state.selection, ModelerSelection::Vertices(_)) ||
                         state.select_mode == SelectMode::Vertex;
    if show_all_verts {
        for (idx, vert) in mesh.vertices.iter().enumerate() {
            // Skip if this vertex is already highlighted as hovered or selected
            let is_hovered = state.hovered_vertex == Some(idx);
            let is_selected = matches!(&state.selection, ModelerSelection::Vertices(v) if v.contains(&idx));
            if is_hovered || is_selected {
                continue;
            }

            if let Some((sx, sy)) = world_to_screen(
                vert.pos,
                camera.position,
                camera.basis_x,
                camera.basis_y,
                camera.basis_z,
                fb.width,
                fb.height,
            ) {
                fb.draw_circle(sx as i32, sy as i32, 2, vertex_normal);
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
    let mesh = &state.mesh;

    match state.select_mode {
        SelectMode::Vertex => {
            // Find closest vertex
            let mut best_idx = None;
            let mut best_dist = 20.0_f32; // Max distance in pixels

            for (idx, vert) in mesh.vertices.iter().enumerate() {
                if let Some((sx, sy)) = world_to_screen(
                    vert.pos,
                    camera.position,
                    camera.basis_x,
                    camera.basis_y,
                    camera.basis_z,
                    fb_width,
                    fb_height,
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
                    // Toggle selection
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
                    state.selection = ModelerSelection::Vertices(vec![idx]);
                }
            } else if !is_key_down(KeyCode::X) {
                // Only clear selection if not holding X (multi-select mode)
                state.selection = ModelerSelection::None;
            }
        }
        SelectMode::Face => {
            // Find clicked face by checking face centers
            let mut best_idx = None;
            let mut best_dist = 30.0_f32;

            for (idx, face) in mesh.faces.iter().enumerate() {
                if let (Some(v0), Some(v1), Some(v2)) = (
                    mesh.vertices.get(face.v0),
                    mesh.vertices.get(face.v1),
                    mesh.vertices.get(face.v2),
                ) {
                    let center = (v0.pos + v1.pos + v2.pos) * (1.0 / 3.0);
                    if let Some((sx, sy)) = world_to_screen(
                        center,
                        camera.position,
                        camera.basis_x,
                        camera.basis_y,
                        camera.basis_z,
                        fb_width,
                        fb_height,
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
                    state.selection = ModelerSelection::Faces(vec![idx]);
                }
            } else if !is_key_down(KeyCode::X) {
                // Only clear selection if not holding X (multi-select mode)
                state.selection = ModelerSelection::None;
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
    let mesh = &state.mesh;

    const VERTEX_THRESHOLD: f32 = 12.0;
    const EDGE_THRESHOLD: f32 = 8.0;
    const FACE_THRESHOLD: f32 = 25.0;

    let mut hovered_vertex: Option<(usize, f32)> = None; // (index, distance)
    let mut hovered_edge: Option<((usize, usize), f32)> = None;
    let mut hovered_face: Option<(usize, f32)> = None;

    // Precompute which vertices are on front-facing faces (for backface culling)
    let mut vertex_on_front_face = vec![false; mesh.vertices.len()];
    let mut edge_on_front_face = std::collections::HashSet::<(usize, usize)>::new();

    for face in &mesh.faces {
        if let (Some(v0), Some(v1), Some(v2)) = (
            mesh.vertices.get(face.v0),
            mesh.vertices.get(face.v1),
            mesh.vertices.get(face.v2),
        ) {
            // Use screen-space signed area for backface culling (same as rasterizer)
            if let (Some((sx0, sy0)), Some((sx1, sy1)), Some((sx2, sy2))) = (
                world_to_screen(v0.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height),
                world_to_screen(v1.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height),
                world_to_screen(v2.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height),
            ) {
                // 2D screen-space signed area (PS1-style) - positive = front-facing
                let signed_area = (sx1 - sx0) * (sy2 - sy0) - (sx2 - sx0) * (sy1 - sy0);

                if signed_area > 0.0 {
                    // Front-facing: mark vertices and edges
                    vertex_on_front_face[face.v0] = true;
                    vertex_on_front_face[face.v1] = true;
                    vertex_on_front_face[face.v2] = true;

                    // Normalize edge order for consistency
                    let e0 = (face.v0.min(face.v1), face.v0.max(face.v1));
                    let e1 = (face.v1.min(face.v2), face.v1.max(face.v2));
                    let e2 = (face.v2.min(face.v0), face.v2.max(face.v0));
                    edge_on_front_face.insert(e0);
                    edge_on_front_face.insert(e1);
                    edge_on_front_face.insert(e2);
                }
            }
        }
    }

    // Check vertices first (highest priority) - only if on front-facing face
    for (idx, vert) in mesh.vertices.iter().enumerate() {
        if !vertex_on_front_face[idx] {
            continue; // Skip vertices only on backfaces
        }
        if let Some((sx, sy)) = world_to_screen(
            vert.pos,
            camera.position,
            camera.basis_x,
            camera.basis_y,
            camera.basis_z,
            fb_width,
            fb_height,
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
        // Collect edges from faces
        for face in &mesh.faces {
            let edges = [(face.v0, face.v1), (face.v1, face.v2), (face.v2, face.v0)];
            for (v0_idx, v1_idx) in edges {
                // Normalize edge order for consistency
                let edge = if v0_idx < v1_idx { (v0_idx, v1_idx) } else { (v1_idx, v0_idx) };

                // Skip edges only on backfaces
                if !edge_on_front_face.contains(&edge) {
                    continue;
                }

                if let (Some(v0), Some(v1)) = (mesh.vertices.get(v0_idx), mesh.vertices.get(v1_idx)) {
                    if let (Some((sx0, sy0)), Some((sx1, sy1))) = (
                        world_to_screen(v0.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height),
                        world_to_screen(v1.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height),
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

    // If no vertex or edge hovered, check faces
    if hovered_vertex.is_none() && hovered_edge.is_none() {
        for (idx, face) in mesh.faces.iter().enumerate() {
            if let (Some(v0), Some(v1), Some(v2)) = (
                mesh.vertices.get(face.v0),
                mesh.vertices.get(face.v1),
                mesh.vertices.get(face.v2),
            ) {
                // Use screen-space signed area for backface culling (same as rasterizer)
                if let (Some((sx0, sy0)), Some((sx1, sy1)), Some((sx2, sy2))) = (
                    world_to_screen(v0.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height),
                    world_to_screen(v1.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height),
                    world_to_screen(v2.pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height),
                ) {
                    // 2D screen-space signed area (PS1-style) - positive = front-facing
                    let signed_area = (sx1 - sx0) * (sy2 - sy0) - (sx2 - sx0) * (sy1 - sy0);
                    if signed_area <= 0.0 {
                        continue; // Backface - skip
                    }

                    let center_sx = (sx0 + sx1 + sx2) / 3.0;
                    let center_sy = (sy0 + sy1 + sy2) / 3.0;
                    let dist = ((mouse_fb_x - center_sx).powi(2) + (mouse_fb_y - center_sy).powi(2)).sqrt();
                    if dist < FACE_THRESHOLD {
                        if hovered_face.map_or(true, |(_, best_dist)| dist < best_dist) {
                            hovered_face = Some((idx, dist));
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
            // Toggle vertex in selection
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
            state.selection = ModelerSelection::Vertices(vec![vert_idx]);
        }
        state.select_mode = SelectMode::Vertex;
        return;
    }

    if let Some((v0, v1)) = state.hovered_edge {
        if multi_select {
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
            state.selection = ModelerSelection::Edges(vec![(v0, v1)]);
        }
        state.select_mode = SelectMode::Edge;
        return;
    }

    if let Some(face_idx) = state.hovered_face {
        if multi_select {
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
            state.selection = ModelerSelection::Faces(vec![face_idx]);
        }
        state.select_mode = SelectMode::Face;
        return;
    }

    // Clicked on nothing - clear selection (unless holding X)
    if !is_key_down(KeyCode::X) {
        state.selection = ModelerSelection::None;
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
) {
    // Use new tool system - check which transform tool is active
    match state.tool_box.active_transform_tool() {
        Some(ModelerToolId::Move) => handle_move_gizmo(ctx, state, mouse_pos, inside_viewport, draw_x, draw_y, draw_w, draw_h, fb_width, fb_height),
        Some(ModelerToolId::Scale) => handle_scale_gizmo(ctx, state, mouse_pos, inside_viewport, draw_x, draw_y, draw_w, draw_h, fb_width, fb_height),
        Some(ModelerToolId::Rotate) => handle_rotate_gizmo(ctx, state, mouse_pos, inside_viewport, draw_x, draw_y, draw_w, draw_h, fb_width, fb_height),
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
    let center = state.selection.compute_center(&state.mesh)?;
    let camera = &state.camera;

    let center_screen = match world_to_screen(center, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height) {
        Some((sx, sy)) => (draw_x + sx / fb_width as f32 * draw_w, draw_y + sy / fb_height as f32 * draw_h),
        None => return None,
    };

    let axis_dirs = [
        (Axis::X, Vec3::new(1.0, 0.0, 0.0), RED),
        (Axis::Y, Vec3::new(0.0, 1.0, 0.0), GREEN),
        (Axis::Z, Vec3::new(0.0, 0.0, 1.0), BLUE),
    ];

    let dist_to_camera = (center - camera.position).len();
    let world_length = dist_to_camera * 0.1;

    let mut axis_screen_ends: [(Axis, (f32, f32), Color); 3] = [
        (Axis::X, (0.0, 0.0), RED),
        (Axis::Y, (0.0, 0.0), GREEN),
        (Axis::Z, (0.0, 0.0), BLUE),
    ];

    for (i, (axis, dir, color)) in axis_dirs.iter().enumerate() {
        let end_world = center + *dir * world_length;
        if let Some((sx, sy)) = world_to_screen(end_world, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height) {
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
) {
    let setup = match setup_gizmo(state, draw_x, draw_y, draw_w, draw_h, fb_width, fb_height) {
        Some(s) => s,
        None => {
            state.gizmo_hovered_axis = None;
            return;
        }
    };

    // Check if DragManager has an active move drag
    let is_dragging = state.drag_manager.is_dragging() && state.drag_manager.active.is_move();

    // Handle ongoing drag with DragManager
    if is_dragging {
        if ctx.mouse.left_down {
            // Convert screen coords to framebuffer coords for ray casting
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
                for (vert_idx, new_pos) in positions {
                    if let Some(vert) = state.mesh.vertices.get_mut(vert_idx) {
                        vert.pos = if state.snap_settings.enabled && !snap_disabled {
                            state.snap_settings.snap_vec3(new_pos)
                        } else {
                            new_pos
                        };
                    }
                }
                state.dirty = true;
            }
        } else {
            // End drag - sync tool state
            state.tool_box.tools.move_tool.end_drag();
            if let Some(_result) = state.drag_manager.end() {
                state.push_undo("Gizmo Move");
                state.sync_mesh_to_project();
            }
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
    if is_mouse_button_pressed(MouseButton::Left) && inside_viewport && state.gizmo_hovered_axis.is_some() && !is_dragging {
        let axis = state.gizmo_hovered_axis.unwrap();

        // Get vertex indices and initial positions
        let mut indices = state.selection.get_affected_vertex_indices(&state.mesh);
        if state.vertex_linking {
            indices = state.mesh.expand_to_coincident(&indices, 0.001);
        }

        let initial_positions: Vec<(usize, Vec3)> = indices.iter()
            .filter_map(|&idx| state.mesh.vertices.get(idx).map(|v| (idx, v.pos)))
            .collect();

        // Convert screen coords to framebuffer coords
        let fb_mouse = (
            (mouse_pos.0 - draw_x) / draw_w * fb_width as f32,
            (mouse_pos.1 - draw_y) / draw_h * fb_height as f32,
        );

        // Start drag with DragManager and sync tool state
        let ui_axis = to_ui_axis(axis);
        state.tool_box.tools.move_tool.start_drag(Some(ui_axis));
        state.drag_manager.start_move(
            setup.center,
            fb_mouse,
            Some(ui_axis),
            indices,
            initial_positions,
            state.snap_settings.enabled,
            state.snap_settings.grid_size,
        );
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

            if let DragUpdateResult::Scale { positions, .. } = result {
                let snap_disabled = is_key_down(KeyCode::Z);
                for (vert_idx, new_pos) in positions {
                    if let Some(vert) = state.mesh.vertices.get_mut(vert_idx) {
                        vert.pos = if state.snap_settings.enabled && !snap_disabled {
                            state.snap_settings.snap_vec3(new_pos)
                        } else {
                            new_pos
                        };
                    }
                }
                state.dirty = true;
            }
        } else {
            // End drag - sync tool state
            state.tool_box.tools.scale.end_drag();
            if let Some(_result) = state.drag_manager.end() {
                state.push_undo("Gizmo Scale");
                state.sync_mesh_to_project();
            }
        }
    }

    // Detect hover on cube handles (only when not dragging)
    state.gizmo_hovered_axis = None;
    if !is_dragging && inside_viewport {
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
    // Sync hover state to tool
    state.tool_box.tools.scale.set_hovered_axis(state.gizmo_hovered_axis.map(to_ui_axis));

    // Start drag on click
    if is_mouse_button_pressed(MouseButton::Left) && inside_viewport && state.gizmo_hovered_axis.is_some() && !is_dragging {
        let axis = state.gizmo_hovered_axis.unwrap();

        // Get vertex indices and initial positions
        let mut indices = state.selection.get_affected_vertex_indices(&state.mesh);
        if state.vertex_linking {
            indices = state.mesh.expand_to_coincident(&indices, 0.001);
        }

        let initial_positions: Vec<(usize, Vec3)> = indices.iter()
            .filter_map(|&idx| state.mesh.vertices.get(idx).map(|v| (idx, v.pos)))
            .collect();

        let fb_mouse = (
            (mouse_pos.0 - draw_x) / draw_w * fb_width as f32,
            (mouse_pos.1 - draw_y) / draw_h * fb_height as f32,
        );

        // Start drag with DragManager and sync tool state
        let ui_axis = to_ui_axis(axis);
        state.tool_box.tools.scale.start_drag(Some(ui_axis));
        state.drag_manager.start_scale(
            setup.center,
            fb_mouse,
            Some(ui_axis),
            indices,
            initial_positions,
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

    // Draw center circle
    let center_color = if is_dragging { YELLOW } else { WHITE };
    draw_circle(setup.center_screen.0, setup.center_screen.1, 4.0, center_color);
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
                for (vert_idx, new_pos) in positions {
                    if let Some(vert) = state.mesh.vertices.get_mut(vert_idx) {
                        vert.pos = if state.snap_settings.enabled && !snap_disabled {
                            state.snap_settings.snap_vec3(new_pos)
                        } else {
                            new_pos
                        };
                    }
                }
                state.dirty = true;
            }
        } else {
            // End drag - sync tool state
            state.tool_box.tools.rotate.end_drag();
            if let Some(_result) = state.drag_manager.end() {
                state.push_undo("Gizmo Rotate");
                state.sync_mesh_to_project();
            }
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

                if let Some((sx, sy)) = world_to_screen(world_point, camera_position, camera_basis_x, camera_basis_y, camera_basis_z, fb_width, fb_height) {
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
    if is_mouse_button_pressed(MouseButton::Left) && inside_viewport && state.gizmo_hovered_axis.is_some() && !is_dragging {
        let axis = state.gizmo_hovered_axis.unwrap();

        // Get vertex indices and initial positions
        let mut indices = state.selection.get_affected_vertex_indices(&state.mesh);
        if state.vertex_linking {
            indices = state.mesh.expand_to_coincident(&indices, 0.001);
        }

        let initial_positions: Vec<(usize, Vec3)> = indices.iter()
            .filter_map(|&idx| state.mesh.vertices.get(idx).map(|v| (idx, v.pos)))
            .collect();

        // Calculate initial angle (for screen-space rotation)
        let start_vec = (
            mouse_pos.0 - setup.center_screen.0,
            mouse_pos.1 - setup.center_screen.1,
        );
        let initial_angle = start_vec.1.atan2(start_vec.0);

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

                if let Some((sx, sy)) = world_to_screen(world_point, camera_position, camera_basis_x, camera_basis_y, camera_basis_z, fb_width, fb_height) {
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
