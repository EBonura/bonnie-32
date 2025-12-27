//! 3D Viewport for the modeler - renders meshes using the software rasterizer
//!
//! Simplified PicoCAD-style viewport with mesh editing only.

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext};
use crate::rasterizer::{
    Framebuffer, render_mesh, Color as RasterColor, Vec3,
    Vertex as RasterVertex, Face as RasterFace, WIDTH, HEIGHT,
    world_to_screen,
};
use super::state::{ModelerState, ModelerSelection, SelectMode, Axis, ModalTransform};

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

/// Handle modal transforms (G=Grab, S=Scale, R=Rotate)
fn handle_modal_transform(state: &mut ModelerState, mouse_pos: (f32, f32)) {
    if state.modal_transform == ModalTransform::None {
        return;
    }

    // Handle axis constraints (X/Y/Z keys)
    if is_key_pressed(KeyCode::X) {
        state.axis_lock = Some(Axis::X);
        state.set_status("X axis", 0.5);
    } else if is_key_pressed(KeyCode::Y) {
        state.axis_lock = Some(Axis::Y);
        state.set_status("Y axis", 0.5);
    } else if is_key_pressed(KeyCode::Z) {
        state.axis_lock = Some(Axis::Z);
        state.set_status("Z axis", 0.5);
    }

    // Calculate mouse delta
    let dx = mouse_pos.0 - state.modal_transform_start_mouse.0;
    let dy = mouse_pos.1 - state.modal_transform_start_mouse.1;

    // Calculate new positions based on transform type
    let new_positions: Vec<Vec3> = match state.modal_transform {
        ModalTransform::Grab => {
            let scale = 0.5; // Sensitivity
            let offset = match state.axis_lock {
                Some(Axis::X) => Vec3::new(dx * scale, 0.0, 0.0),
                Some(Axis::Y) => Vec3::new(0.0, -dy * scale, 0.0),
                Some(Axis::Z) => Vec3::new(0.0, 0.0, dx * scale),
                None => Vec3::new(dx * scale, -dy * scale, 0.0),
            };
            state.modal_transform_start_positions.iter().map(|&p| p + offset).collect()
        }
        ModalTransform::Scale => {
            let scale = 1.0 + (dx + dy) * 0.005;
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
            let angle = (dx + dy) * 0.01;
            state.modal_transform_start_positions.iter().map(|&start_pos| {
                let offset = start_pos - state.modal_transform_center;
                let cos_a = angle.cos();
                let sin_a = angle.sin();
                let rotated_offset = match state.axis_lock {
                    Some(Axis::X) => Vec3::new(
                        offset.x,
                        offset.y * cos_a - offset.z * sin_a,
                        offset.y * sin_a + offset.z * cos_a,
                    ),
                    Some(Axis::Y) | None => Vec3::new(
                        offset.x * cos_a + offset.z * sin_a,
                        offset.y,
                        -offset.x * sin_a + offset.z * cos_a,
                    ),
                    Some(Axis::Z) => Vec3::new(
                        offset.x * cos_a - offset.y * sin_a,
                        offset.x * sin_a + offset.y * cos_a,
                        offset.z,
                    ),
                };
                state.modal_transform_center + rotated_offset
            }).collect()
        }
        ModalTransform::None => return,
    };

    // Apply positions (real-time preview)
    apply_selected_positions(state, &new_positions);

    // Confirm on left click
    if is_mouse_button_pressed(MouseButton::Left) {
        state.modal_transform = ModalTransform::None;
        state.dirty = true;
        state.set_status("Transform applied", 1.0);
    }

    // Cancel on ESC or right click
    if is_key_pressed(KeyCode::Escape) || is_mouse_button_pressed(MouseButton::Right) {
        apply_selected_positions(state, &state.modal_transform_start_positions.clone());
        state.modal_transform = ModalTransform::None;
        state.set_status("Transform cancelled", 1.0);
    }
}

/// Handle left-drag to move selection in the viewport
fn handle_drag_move(
    ctx: &UiContext,
    state: &mut ModelerState,
    mouse_pos: (f32, f32),
    draw_x: f32,
    draw_y: f32,
    draw_w: f32,
    draw_h: f32,
    _fb_width: usize,
    _fb_height: usize,
) {
    // Don't interfere with modal transforms
    if state.modal_transform != ModalTransform::None {
        return;
    }

    // Only active when we have a selection
    if state.selection.is_empty() {
        state.transform_active = false;
        return;
    }

    let inside_viewport = mouse_pos.0 >= draw_x && mouse_pos.0 < draw_x + draw_w
                       && mouse_pos.1 >= draw_y && mouse_pos.1 < draw_y + draw_h;

    // Start drag on left-button down with selection (but not on initial click - that's for selection)
    // We detect if they're dragging after having selected
    if ctx.mouse.left_down && inside_viewport && !state.transform_active {
        // Only start drag if mouse has moved significantly from last position (to distinguish from click-to-select)
        let dx = (mouse_pos.0 - state.viewport_last_mouse.0).abs();
        let dy = (mouse_pos.1 - state.viewport_last_mouse.1).abs();
        if dx > 3.0 || dy > 3.0 {
            // Start drag
            state.transform_active = true;
            state.transform_start_mouse = state.viewport_last_mouse; // Use position before drag started
            state.transform_start_positions = get_selected_positions(state);

            // Save undo state at drag start
            state.push_undo("Drag move");

            state.set_status("Drag to move (hold Shift for fine)", 3.0);
        }
    }

    // During drag - move selection based on mouse delta
    if state.transform_active && ctx.mouse.left_down {
        let dx = mouse_pos.0 - state.transform_start_mouse.0;
        let dy = mouse_pos.1 - state.transform_start_mouse.1;

        // Sensitivity - finer with Shift
        let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
        let sensitivity = if shift { 0.1 } else { 0.5 };

        // Move in screen-space mapped to world
        // This is approximate - proper implementation would use camera ray projection
        let move_x = dx * sensitivity;
        let move_y = -dy * sensitivity; // Screen Y is flipped

        // Calculate new positions
        let new_positions: Vec<Vec3> = state.transform_start_positions.iter().map(|&start_pos| {
            // Move in camera's local XY plane
            let cam_right = state.camera.basis_x;
            let cam_up = state.camera.basis_y;
            start_pos + cam_right * move_x + cam_up * move_y
        }).collect();

        // Apply snapping if enabled and not holding Z
        let snap_disabled = is_key_down(KeyCode::Z);
        let final_positions: Vec<Vec3> = if state.snap_settings.enabled && !snap_disabled {
            new_positions.iter().map(|&p| state.snap_settings.snap_vec3(p)).collect()
        } else {
            new_positions
        };

        apply_selected_positions(state, &final_positions);
    }

    // End drag on mouse release
    if !ctx.mouse.left_down && state.transform_active {
        state.transform_active = false;
        state.dirty = true;
        state.set_status("Moved", 0.5);
    }

    // Cancel drag on ESC or right-click
    if state.transform_active && (is_key_pressed(KeyCode::Escape) || is_mouse_button_pressed(MouseButton::Right)) {
        apply_selected_positions(state, &state.transform_start_positions.clone());
        state.transform_active = false;
        state.set_status("Move cancelled", 0.5);
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

    // Modal transforms: G = Grab, S = Scale, R = Rotate
    let has_selection = !state.selection.is_empty();
    let no_transform_active = !state.transform_active && state.modal_transform == ModalTransform::None;

    if inside_viewport && has_selection && no_transform_active {
        let start_modal = |mode: ModalTransform, state: &mut ModelerState, mouse: (f32, f32)| {
            let positions = get_selected_positions(state);
            if positions.is_empty() {
                return;
            }
            // Save undo state before starting transform
            state.push_undo(mode.label());
            let center = positions.iter().fold(Vec3::ZERO, |acc, p| acc + *p) * (1.0 / positions.len() as f32);
            state.modal_transform = mode;
            state.modal_transform_start_mouse = mouse;
            state.modal_transform_start_positions = positions;
            state.modal_transform_center = center;
            state.axis_lock = None;
            state.set_status(&format!("{} - X/Y/Z to constrain, click to confirm", mode.label()), 5.0);
        };

        if is_key_pressed(KeyCode::G) {
            start_modal(ModalTransform::Grab, state, mouse_pos);
        } else if is_key_pressed(KeyCode::S) {
            start_modal(ModalTransform::Scale, state, mouse_pos);
        } else if is_key_pressed(KeyCode::R) {
            start_modal(ModalTransform::Rotate, state, mouse_pos);
        }
    }

    handle_modal_transform(state, mouse_pos);

    // Handle left-click drag to move selection (if not in modal transform)
    handle_drag_move(ctx, state, mouse_pos, draw_x, draw_y, draw_w, draw_h, fb_width, fb_height);

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

    // Draw and handle move gizmo (on top of the framebuffer)
    handle_gizmo(ctx, state, mouse_pos, inside_viewport, draw_x, draw_y, draw_w, draw_h, fb_width, fb_height);

    // Update hover state every frame (like world editor) - but not when dragging gizmo
    if !state.gizmo_dragging {
        update_hover_state(state, mouse_pos, draw_x, draw_y, draw_w, draw_h, fb_width, fb_height);
    }

    // Handle box selection (left-drag without hitting an element)
    if !state.gizmo_dragging {
        handle_box_selection(ctx, state, mouse_pos, draw_x, draw_y, draw_w, draw_h, fb_width, fb_height);
    }

    // Draw box selection overlay if active
    if state.box_select_active {
        let (x0, y0) = state.box_select_start;
        let (x1, y1) = mouse_pos;
        let min_x = x0.min(x1);
        let min_y = y0.min(y1);
        let max_x = x0.max(x1);
        let max_y = y0.max(y1);

        // Semi-transparent fill
        draw_rectangle(min_x, min_y, max_x - min_x, max_y - min_y, Color::from_rgba(100, 150, 255, 50));
        // Border
        draw_rectangle_lines(min_x, min_y, max_x - min_x, max_y - min_y, 1.0, Color::from_rgba(100, 150, 255, 200));
    }

    // Handle single-click selection using hover system (like world editor)
    // Only if not clicking on gizmo
    if inside_viewport && is_mouse_button_pressed(MouseButton::Left)
        && state.modal_transform == ModalTransform::None
        && !state.box_select_active
        && state.gizmo_hovered_axis.is_none()
        && !state.gizmo_dragging
    {
        handle_hover_click(state);
    }
}

/// Handle box/rectangle selection
fn handle_box_selection(
    ctx: &UiContext,
    state: &mut ModelerState,
    mouse_pos: (f32, f32),
    draw_x: f32,
    draw_y: f32,
    draw_w: f32,
    draw_h: f32,
    fb_width: usize,
    fb_height: usize,
) {
    let inside_viewport = mouse_pos.0 >= draw_x && mouse_pos.0 < draw_x + draw_w
                       && mouse_pos.1 >= draw_y && mouse_pos.1 < draw_y + draw_h;

    // Don't start box select during modal transforms or drag moves
    if state.modal_transform != ModalTransform::None || state.transform_active {
        return;
    }

    // Start box selection on left mouse down (we detect if it becomes a drag vs click)
    if is_mouse_button_pressed(MouseButton::Left) && inside_viewport && !state.box_select_active {
        state.box_select_start = mouse_pos;
    }

    // Detect if we're now in box selection mode (mouse moved enough from start)
    if ctx.mouse.left_down && inside_viewport && !state.box_select_active {
        let dx = (mouse_pos.0 - state.box_select_start.0).abs();
        let dy = (mouse_pos.1 - state.box_select_start.1).abs();
        // Only become box select if moved at least 5 pixels
        if dx > 5.0 || dy > 5.0 {
            state.box_select_active = true;
        }
    }

    // On mouse release with active box selection, select all elements in the box
    if !ctx.mouse.left_down && state.box_select_active {
        let (x0, y0) = state.box_select_start;
        let (x1, y1) = mouse_pos;

        // Convert screen coords to framebuffer coords
        let fb_x0 = (x0.min(x1) - draw_x) / draw_w * fb_width as f32;
        let fb_y0 = (y0.min(y1) - draw_y) / draw_h * fb_height as f32;
        let fb_x1 = (x0.max(x1) - draw_x) / draw_w * fb_width as f32;
        let fb_y1 = (y0.max(y1) - draw_y) / draw_h * fb_height as f32;

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

        state.box_select_active = false;
    }

    // Cancel box selection on ESC
    if state.box_select_active && is_key_pressed(KeyCode::Escape) {
        state.box_select_active = false;
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
    if state.modal_transform != ModalTransform::None || state.transform_active || state.box_select_active {
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
// GIZMO - Move selected elements by dragging axis handles
// ============================================================================

const GIZMO_SIZE: f32 = 60.0;  // Length of gizmo arms in screen pixels
const GIZMO_HIT_RADIUS: f32 = 8.0;  // Hit radius for axis lines

/// Handle the move gizmo: draw it, detect hover/click on axes, process dragging
fn handle_gizmo(
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
    // Only show gizmo if something is selected
    let center = match state.selection.compute_center(&state.mesh) {
        Some(c) => c,
        None => {
            state.gizmo_hovered_axis = None;
            return;
        }
    };

    let camera = &state.camera;

    // Project gizmo center to screen
    let center_screen = match world_to_screen(center, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height) {
        Some((sx, sy)) => {
            // Convert framebuffer coords to screen coords
            (draw_x + sx / fb_width as f32 * draw_w, draw_y + sy / fb_height as f32 * draw_h)
        }
        None => return,
    };

    // Project axis endpoints
    let axis_dirs = [
        (Axis::X, Vec3::new(1.0, 0.0, 0.0), RED),
        (Axis::Y, Vec3::new(0.0, 1.0, 0.0), GREEN),
        (Axis::Z, Vec3::new(0.0, 0.0, 1.0), BLUE),
    ];

    // Calculate axis length in world space based on distance from camera
    let dist_to_camera = (center - camera.position).len();
    let world_length = dist_to_camera * 0.1;  // Scale gizmo with distance

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

    // Handle ongoing drag
    if state.gizmo_dragging {
        if ctx.mouse.left_down {
            // Continue dragging - move vertices along the axis
            if let Some(axis) = state.gizmo_drag_axis {
                let mouse_delta = (mouse_pos.0 - state.gizmo_drag_start_mouse.0, mouse_pos.1 - state.gizmo_drag_start_mouse.1);

                // Get axis direction in world space
                let axis_dir = match axis {
                    Axis::X => Vec3::new(1.0, 0.0, 0.0),
                    Axis::Y => Vec3::new(0.0, 1.0, 0.0),
                    Axis::Z => Vec3::new(0.0, 0.0, 1.0),
                };

                // Project axis to screen to determine movement direction
                let axis_end_world = state.gizmo_drag_center + axis_dir * world_length;
                if let (Some((cx, cy)), Some((ex, ey))) = (
                    world_to_screen(state.gizmo_drag_center, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height),
                    world_to_screen(axis_end_world, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height),
                ) {
                    // Convert to screen coords
                    let cx_screen = cx / fb_width as f32 * draw_w;
                    let cy_screen = cy / fb_height as f32 * draw_h;
                    let ex_screen = ex / fb_width as f32 * draw_w;
                    let ey_screen = ey / fb_height as f32 * draw_h;

                    let axis_screen_dir = (ex_screen - cx_screen, ey_screen - cy_screen);
                    let axis_screen_len = (axis_screen_dir.0.powi(2) + axis_screen_dir.1.powi(2)).sqrt();

                    if axis_screen_len > 0.1 {
                        // Project mouse delta onto axis direction
                        let axis_norm = (axis_screen_dir.0 / axis_screen_len, axis_screen_dir.1 / axis_screen_len);
                        let projected_delta = mouse_delta.0 * axis_norm.0 + mouse_delta.1 * axis_norm.1;

                        // Convert screen delta to world delta
                        // Scale based on how much screen space the world_length occupies
                        let world_per_pixel = world_length / axis_screen_len;
                        let world_delta = projected_delta * world_per_pixel;

                        // Apply movement to all affected vertices
                        for (vert_idx, original_pos) in &state.gizmo_drag_start_positions {
                            if let Some(vert) = state.mesh.vertices.get_mut(*vert_idx) {
                                vert.pos = *original_pos + axis_dir * world_delta;

                                // Apply grid snapping if enabled
                                if !is_key_down(KeyCode::Z) && state.snap_settings.enabled {
                                    let snap = state.snap_settings.grid_size;
                                    vert.pos.x = (vert.pos.x / snap).round() * snap;
                                    vert.pos.y = (vert.pos.y / snap).round() * snap;
                                    vert.pos.z = (vert.pos.z / snap).round() * snap;
                                }
                            }
                        }
                        state.dirty = true;
                    }
                }
            }
        } else {
            // Mouse released - end drag
            if !state.gizmo_drag_start_positions.is_empty() {
                state.push_undo("Gizmo Move");
                state.sync_mesh_to_project(); // Keep project in sync
            }
            state.gizmo_dragging = false;
            state.gizmo_drag_axis = None;
            state.gizmo_drag_start_positions.clear();
        }
    }

    // Detect axis hover
    state.gizmo_hovered_axis = None;
    if !state.gizmo_dragging && inside_viewport {
        for (axis, end_screen, _) in &axis_screen_ends {
            let dist = point_to_line_distance(
                mouse_pos.0,
                mouse_pos.1,
                center_screen.0,
                center_screen.1,
                end_screen.0,
                end_screen.1,
            );
            if dist < GIZMO_HIT_RADIUS {
                state.gizmo_hovered_axis = Some(*axis);
                break;
            }
        }
    }

    // Start drag on click
    if is_mouse_button_pressed(MouseButton::Left) && inside_viewport && state.gizmo_hovered_axis.is_some() && !state.gizmo_dragging {
        let axis = state.gizmo_hovered_axis.unwrap();
        state.gizmo_dragging = true;
        state.gizmo_drag_axis = Some(axis);
        state.gizmo_drag_start_mouse = mouse_pos;
        state.gizmo_drag_center = center;

        // Store original positions of all affected vertices
        let mut indices = state.selection.get_affected_vertex_indices(&state.mesh);

        // If vertex linking is enabled, expand to include coincident vertices
        if state.vertex_linking {
            indices = state.mesh.expand_to_coincident(&indices, 0.001);
        }

        state.gizmo_drag_start_positions = indices.iter()
            .filter_map(|&idx| state.mesh.vertices.get(idx).map(|v| (idx, v.pos)))
            .collect();
    }

    // Draw the gizmo axes
    for (axis, end_screen, base_color) in &axis_screen_ends {
        let is_hovered = state.gizmo_hovered_axis == Some(*axis);
        let is_dragging = state.gizmo_dragging && state.gizmo_drag_axis == Some(*axis);

        let color = if is_dragging {
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
        };

        let thickness = if is_hovered || is_dragging { 3.0 } else { 2.0 };

        // Draw axis line
        draw_line(center_screen.0, center_screen.1, end_screen.0, end_screen.1, thickness, color);

        // Draw arrowhead
        let dx = end_screen.0 - center_screen.0;
        let dy = end_screen.1 - center_screen.1;
        let len = (dx * dx + dy * dy).sqrt();
        if len > 5.0 {
            let nx = dx / len;
            let ny = dy / len;
            // Perpendicular
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
    let center_color = if state.gizmo_dragging { YELLOW } else { WHITE };
    draw_circle(center_screen.0, center_screen.1, 4.0, center_color);
}
