//! 3D Viewport - Software rendered preview

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext};
use crate::rasterizer::{
    Framebuffer, Texture as RasterTexture, RasterSettings, render_mesh, Color as RasterColor, Vec3,
    perspective_transform,
};
use super::{EditorState, Selection, CLICK_HEIGHT};

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

/// Calculate distance from point to line segment in 2D screen space
fn point_to_segment_distance(
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

/// Test if point is inside 2D triangle using barycentric coordinates
fn point_in_triangle_2d(
    px: f32, py: f32,      // Point
    x1: f32, y1: f32,      // Triangle v1
    x2: f32, y2: f32,      // Triangle v2
    x3: f32, y3: f32,      // Triangle v3
) -> bool {
    let area = 0.5 * (-y2 * x3 + y1 * (-x2 + x3) + x1 * (y2 - y3) + x2 * y3);
    let s = (y1 * x3 - x1 * y3 + (y3 - y1) * px + (x1 - x3) * py) / (2.0 * area);
    let t = (x1 * y2 - y1 * x2 + (y1 - y2) * px + (x2 - x1) * py) / (2.0 * area);
    s >= 0.0 && t >= 0.0 && (1.0 - s - t) >= 0.0
}

/// Draw the 3D viewport using the software rasterizer
pub fn draw_viewport_3d(
    ctx: &mut UiContext,
    rect: Rect,
    state: &mut EditorState,
    textures: &[RasterTexture],
    fb: &mut Framebuffer,
    settings: &RasterSettings,
) {
    let mouse_pos = (ctx.mouse.x, ctx.mouse.y);
    let inside_viewport = ctx.mouse.inside(&rect);

    // Pre-calculate viewport scaling (used multiple times)
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

    // Helper to convert framebuffer to screen coordinates
    let fb_to_screen = |fb_x: f32, fb_y: f32| -> (f32, f32) {
        let screen_x = draw_x + (fb_x / fb_width as f32) * draw_w;
        let screen_y = draw_y + (fb_y / fb_height as f32) * draw_h;
        (screen_x, screen_y)
    };

    // Camera rotation with right mouse button (same as game mode)
    // Only rotate camera when not dragging a vertex
    if ctx.mouse.right_down && inside_viewport && state.viewport_dragging_vertices.is_empty() {
        if state.viewport_mouse_captured {
            // Inverted to match Y-down coordinate system
            let dx = (mouse_pos.1 - state.viewport_last_mouse.1) * 0.005;
            let dy = -(mouse_pos.0 - state.viewport_last_mouse.0) * 0.005;
            state.camera_3d.rotate(dx, dy);
        }
        state.viewport_mouse_captured = true;
    } else if !ctx.mouse.right_down {
        state.viewport_mouse_captured = false;
    }

    // Keyboard camera movement (WASD + Q/E) - only when viewport focused and not dragging
    let move_speed = 100.0; // Scaled for TRLE units (1024 per sector)
    if (inside_viewport || state.viewport_mouse_captured) && state.viewport_dragging_vertices.is_empty() {
        if is_key_down(KeyCode::W) {
            state.camera_3d.position = state.camera_3d.position + state.camera_3d.basis_z * move_speed;
        }
        if is_key_down(KeyCode::S) {
            state.camera_3d.position = state.camera_3d.position - state.camera_3d.basis_z * move_speed;
        }
        if is_key_down(KeyCode::A) {
            state.camera_3d.position = state.camera_3d.position - state.camera_3d.basis_x * move_speed;
        }
        if is_key_down(KeyCode::D) {
            state.camera_3d.position = state.camera_3d.position + state.camera_3d.basis_x * move_speed;
        }
        if is_key_down(KeyCode::Q) {
            state.camera_3d.position = state.camera_3d.position - state.camera_3d.basis_y * move_speed;
        }
        if is_key_down(KeyCode::E) {
            state.camera_3d.position = state.camera_3d.position + state.camera_3d.basis_y * move_speed;
        }
    }

    // Toggle link coincident vertices mode with L key
    if inside_viewport && is_key_pressed(KeyCode::L) {
        state.link_coincident_vertices = !state.link_coincident_vertices;
        let mode = if state.link_coincident_vertices { "Linked" } else { "Independent" };
        state.set_status(&format!("Vertex mode: {}", mode), 2.0);
    }

    // Find vertex under mouse cursor (for picking/dragging)
    let mut hovered_vertex: Option<(usize, usize, f32)> = None; // (room_idx, vertex_idx, screen_dist)
    if inside_viewport && !ctx.mouse.right_down {
        if let Some((fb_x, fb_y)) = screen_to_fb(mouse_pos.0, mouse_pos.1) {
            for (room_idx, room) in state.level.rooms.iter().enumerate() {
                for (vert_idx, vert) in room.vertices.iter().enumerate() {
                    let world_pos = *vert + room.position;
                    if let Some((sx, sy)) = world_to_screen(
                        world_pos,
                        state.camera_3d.position,
                        state.camera_3d.basis_x,
                        state.camera_3d.basis_y,
                        state.camera_3d.basis_z,
                        fb.width,
                        fb.height,
                    ) {
                        let dist = ((fb_x - sx).powi(2) + (fb_y - sy).powi(2)).sqrt();
                        if dist < 10.0 {
                            // Check if this is closer than current best
                            if hovered_vertex.map_or(true, |(_, _, best_dist)| dist < best_dist) {
                                hovered_vertex = Some((room_idx, vert_idx, dist));
                            }
                        }
                    }
                }
            }
        }
    }

    // Find edge under mouse cursor (lower priority than vertex)
    let mut hovered_edge: Option<(usize, usize, usize, f32)> = None; // (room_idx, v0_idx, v1_idx, distance)
    if inside_viewport && !ctx.mouse.right_down && hovered_vertex.is_none() {
        if let Some((mouse_fb_x, mouse_fb_y)) = screen_to_fb(mouse_pos.0, mouse_pos.1) {
            const EDGE_THRESHOLD: f32 = 8.0; // Pixels

            for (room_idx, room) in state.level.rooms.iter().enumerate() {
                for face in &room.faces {
                    // Get number of edges (3 for triangle, 4 for quad)
                    let num_edges = if face.is_triangle { 3 } else { 4 };

                    for i in 0..num_edges {
                        let v0_idx = face.indices[i];
                        let v1_idx = face.indices[(i + 1) % num_edges];

                        let v0 = room.vertices[v0_idx] + room.position;
                        let v1 = room.vertices[v1_idx] + room.position;

                        // Project both vertices to screen
                        if let (Some((sx0, sy0)), Some((sx1, sy1))) = (
                            world_to_screen(v0, state.camera_3d.position, state.camera_3d.basis_x,
                                state.camera_3d.basis_y, state.camera_3d.basis_z, fb.width, fb.height),
                            world_to_screen(v1, state.camera_3d.position, state.camera_3d.basis_x,
                                state.camera_3d.basis_y, state.camera_3d.basis_z, fb.width, fb.height)
                        ) {
                            let dist = point_to_segment_distance(mouse_fb_x, mouse_fb_y, sx0, sy0, sx1, sy1);
                            if dist < EDGE_THRESHOLD {
                                if hovered_edge.map_or(true, |(_, _, _, best_dist)| dist < best_dist) {
                                    hovered_edge = Some((room_idx, v0_idx, v1_idx, dist));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Find face under mouse cursor (lowest priority)
    let mut hovered_face: Option<(usize, usize)> = None; // (room_idx, face_idx)
    if inside_viewport && !ctx.mouse.right_down && hovered_vertex.is_none() && hovered_edge.is_none() {
        if let Some((mouse_fb_x, mouse_fb_y)) = screen_to_fb(mouse_pos.0, mouse_pos.1) {
            for (room_idx, room) in state.level.rooms.iter().enumerate() {
                for (face_idx, face) in room.faces.iter().enumerate() {
                    // Get world positions
                    let v0 = room.vertices[face.indices[0]] + room.position;
                    let v1 = room.vertices[face.indices[1]] + room.position;
                    let v2 = room.vertices[face.indices[2]] + room.position;
                    let v3 = room.vertices[face.indices[3]] + room.position;

                    // Transform to camera space for backface culling
                    let rel0 = v0 - state.camera_3d.position;
                    let rel1 = v1 - state.camera_3d.position;
                    let rel2 = v2 - state.camera_3d.position;

                    let cv0 = perspective_transform(rel0, state.camera_3d.basis_x, state.camera_3d.basis_y, state.camera_3d.basis_z);
                    let cv1 = perspective_transform(rel1, state.camera_3d.basis_x, state.camera_3d.basis_y, state.camera_3d.basis_z);
                    let cv2 = perspective_transform(rel2, state.camera_3d.basis_x, state.camera_3d.basis_y, state.camera_3d.basis_z);

                    // Calculate face normal in camera space
                    let edge1 = cv1 - cv0;
                    let edge2 = cv2 - cv0;
                    let normal = edge1.cross(edge2);

                    // Backface culling - skip faces pointing away from camera
                    // In our coordinate system, +Z is forward (camera looks down +Z axis)
                    // Skip if normal.z > 0 (face pointing away)
                    if normal.z > 0.0 {
                        continue;
                    }

                    // Project vertices to screen
                    if let (Some((sx0, sy0)), Some((sx1, sy1)), Some((sx2, sy2)), Some((sx3, sy3))) = (
                        world_to_screen(v0, state.camera_3d.position, state.camera_3d.basis_x,
                            state.camera_3d.basis_y, state.camera_3d.basis_z, fb.width, fb.height),
                        world_to_screen(v1, state.camera_3d.position, state.camera_3d.basis_x,
                            state.camera_3d.basis_y, state.camera_3d.basis_z, fb.width, fb.height),
                        world_to_screen(v2, state.camera_3d.position, state.camera_3d.basis_x,
                            state.camera_3d.basis_y, state.camera_3d.basis_z, fb.width, fb.height),
                        world_to_screen(v3, state.camera_3d.position, state.camera_3d.basis_x,
                            state.camera_3d.basis_y, state.camera_3d.basis_z, fb.width, fb.height)
                    ) {
                        // Test first triangle (v0, v1, v2)
                        if point_in_triangle_2d(mouse_fb_x, mouse_fb_y, sx0, sy0, sx1, sy1, sx2, sy2) {
                            hovered_face = Some((room_idx, face_idx));
                            break;
                        }

                        // Test second triangle (v0, v2, v3) if quad
                        if !face.is_triangle {
                            if point_in_triangle_2d(mouse_fb_x, mouse_fb_y, sx0, sy0, sx2, sy2, sx3, sy3) {
                                hovered_face = Some((room_idx, face_idx));
                                break;
                            }
                        }
                    }
                }

                if hovered_face.is_some() {
                    break;
                }
            }
        }
    }

    // Handle selection and dragging based on what's hovered
    if inside_viewport && !ctx.mouse.right_down {
        // Start dragging or select on left press
        if ctx.mouse.left_pressed {
            use crate::editor::EditorTool;

            // Only handle selection/dragging in Select mode
            // Drawing tools don't work in 3D view (use 2D grid view instead)
            if state.tool == EditorTool::Select {
                if let Some((room_idx, vert_idx, _)) = hovered_vertex {
                // Select and start dragging this vertex
                state.selection = Selection::Vertex { room: room_idx, vertex: vert_idx };

                // Find all vertices to drag based on link mode
                if let Some(room) = state.level.rooms.get(room_idx) {
                    if let Some(clicked_vert) = room.vertices.get(vert_idx) {
                        let clicked_pos = clicked_vert;

                        if state.link_coincident_vertices {
                            // Find ALL vertices at the same position (coincident vertices)
                            state.viewport_dragging_vertices = room.vertices.iter()
                                .enumerate()
                                .filter(|(_, v)| {
                                    // Check if vertex is at same position (small epsilon for floating point)
                                    const EPSILON: f32 = 0.001;
                                    (v.x - clicked_pos.x).abs() < EPSILON &&
                                    (v.y - clicked_pos.y).abs() < EPSILON &&
                                    (v.z - clicked_pos.z).abs() < EPSILON
                                })
                                .map(|(idx, _)| (room_idx, idx))
                                .collect();

                            // Store initial Y positions for all coincident vertices
                            state.viewport_drag_initial_y = state.viewport_dragging_vertices.iter()
                                .filter_map(|&(_, idx)| room.vertices.get(idx))
                                .map(|v| v.y)
                                .collect();
                        } else {
                            // Independent mode - only drag the clicked vertex
                            state.viewport_dragging_vertices = vec![(room_idx, vert_idx)];
                            state.viewport_drag_initial_y = vec![clicked_pos.y];
                        }

                        // Debug output
                        println!("Clicked vertex {} at ({}, {}, {})", vert_idx, clicked_pos.x, clicked_pos.y, clicked_pos.z);
                        println!("Link mode: {}, Dragging {} vertices: {:?}",
                            state.link_coincident_vertices,
                            state.viewport_dragging_vertices.len(),
                            state.viewport_dragging_vertices
                        );

                        state.viewport_drag_started = false;
                        state.viewport_drag_plane_y = clicked_pos.y; // Reference point for delta
                    }
                }
            } else if let Some((room_idx, v0, v1, _)) = hovered_edge {
                // Select edge and start dragging both vertices
                state.selection = Selection::Edge { room: room_idx, v0, v1 };
                state.viewport_dragging_vertices = vec![(room_idx, v0), (room_idx, v1)];
                state.viewport_drag_started = false;

                // Store initial Y positions of both vertices
                if let Some(room) = state.level.rooms.get(room_idx) {
                    if let (Some(v0_pos), Some(v1_pos)) = (room.vertices.get(v0), room.vertices.get(v1)) {
                        state.viewport_drag_plane_y = (v0_pos.y + v1_pos.y) / 2.0; // Average as reference
                        state.viewport_drag_initial_y = vec![v0_pos.y, v1_pos.y];
                    }
                }
            } else if let Some((room_idx, face_idx)) = hovered_face {
                // Select face
                state.selection = Selection::Face { room: room_idx, face: face_idx };

                // Auto-select the face's texture in the texture palette
                if let Some(room) = state.level.rooms.get(room_idx) {
                    if let Some(face) = room.faces.get(face_idx) {
                        state.selected_texture = face.texture.clone();
                    }
                }

                // Allow dragging for all face types now that we preserve relative heights
                if let Some(room) = state.level.rooms.get(room_idx) {
                    if let Some(face) = room.faces.get(face_idx) {
                        // Collect all face vertices
                        let num_verts = if face.is_triangle { 3 } else { 4 };
                        state.viewport_dragging_vertices = (0..num_verts)
                            .map(|i| (room_idx, face.indices[i]))
                            .collect();

                        // Store initial Y positions of all vertices
                        state.viewport_drag_initial_y = (0..num_verts)
                            .filter_map(|i| room.vertices.get(face.indices[i]))
                            .map(|v| v.y)
                            .collect();

                        // Store average Y as reference point for delta
                        let avg_y: f32 = state.viewport_drag_initial_y.iter().sum::<f32>()
                            / state.viewport_drag_initial_y.len() as f32;
                        state.viewport_drag_plane_y = avg_y;
                        state.viewport_drag_started = false;
                    }
                }
            }
            } // end of if state.tool == EditorTool::Select
            else if state.tool == EditorTool::DrawFloor || state.tool == EditorTool::DrawCeiling {
                // Use grid search method to find where to place floor/ceiling
                // This mirrors the vertex hover detection approach for accuracy
                if let Some((mouse_fb_x, mouse_fb_y)) = screen_to_fb(mouse_pos.0, mouse_pos.1) {
                    // Determine target plane height
                    let target_y = if state.tool == EditorTool::DrawFloor {
                        0.0
                    } else {
                        1024.0 // Ceiling height
                    };

                    use super::SECTOR_SIZE;

                    // Search grid around camera position to find closest sector to mouse
                    let search_radius = 20; // Number of sectors to search in each direction
                    let mut closest_sector: Option<(f32, f32, f32)> = None; // (x, z, screen_distance)

                    // Calculate starting search position (snap camera position to grid)
                    let cam_x = state.camera_3d.position.x;
                    let cam_z = state.camera_3d.position.z;
                    let start_x = ((cam_x / SECTOR_SIZE).floor() as i32 - search_radius) as f32 * SECTOR_SIZE;
                    let start_z = ((cam_z / SECTOR_SIZE).floor() as i32 - search_radius) as f32 * SECTOR_SIZE;

                    // Sample grid positions and find which projects closest to mouse
                    for ix in 0..(search_radius * 2) {
                        for iz in 0..(search_radius * 2) {
                            let grid_x = start_x + (ix as f32 * SECTOR_SIZE);
                            let grid_z = start_z + (iz as f32 * SECTOR_SIZE);

                            // Test the sector's center point
                            let test_pos = Vec3::new(
                                grid_x + SECTOR_SIZE / 2.0,
                                target_y,
                                grid_z + SECTOR_SIZE / 2.0,
                            );

                            // Project to screen using the same function as vertex hover
                            if let Some((sx, sy)) = world_to_screen(
                                test_pos,
                                state.camera_3d.position,
                                state.camera_3d.basis_x,
                                state.camera_3d.basis_y,
                                state.camera_3d.basis_z,
                                fb.width,
                                fb.height,
                            ) {
                                // Calculate screen distance to mouse
                                let dist = ((mouse_fb_x - sx).powi(2) + (mouse_fb_y - sy).powi(2)).sqrt();

                                // Update closest if this is better
                                if closest_sector.map_or(true, |(_, _, best_dist)| dist < best_dist) {
                                    closest_sector = Some((grid_x, grid_z, dist));
                                }
                            }
                        }
                    }

                    // Place sector at closest grid position if found within reasonable distance
                    if let Some((snapped_x, snapped_z, dist)) = closest_sector {
                        // Only place if mouse is reasonably close (within ~50 pixels)
                        if dist < 100.0 {
                            state.save_undo();
                            if let Some(room) = state.level.rooms.get_mut(state.current_room) {
                                use crate::world::FaceType;

                                if state.tool == EditorTool::DrawFloor {
                                    // Add floor sector vertices
                                    let v0 = room.add_vertex(snapped_x, target_y, snapped_z);
                                    let v1 = room.add_vertex(snapped_x, target_y, snapped_z + SECTOR_SIZE);
                                    let v2 = room.add_vertex(snapped_x + SECTOR_SIZE, target_y, snapped_z + SECTOR_SIZE);
                                    let v3 = room.add_vertex(snapped_x + SECTOR_SIZE, target_y, snapped_z);

                                    room.add_quad_textured(v0, v1, v2, v3, state.selected_texture.clone(), FaceType::Floor);
                                    room.recalculate_bounds();
                                    state.set_status("Created floor sector", 2.0);
                                } else {
                                    // Add ceiling sector vertices (reversed winding)
                                    let v0 = room.add_vertex(snapped_x, target_y, snapped_z);
                                    let v1 = room.add_vertex(snapped_x + SECTOR_SIZE, target_y, snapped_z);
                                    let v2 = room.add_vertex(snapped_x + SECTOR_SIZE, target_y, snapped_z + SECTOR_SIZE);
                                    let v3 = room.add_vertex(snapped_x, target_y, snapped_z + SECTOR_SIZE);

                                    room.add_quad_textured(v0, v1, v2, v3, state.selected_texture.clone(), FaceType::Ceiling);
                                    room.recalculate_bounds();
                                    state.set_status("Created ceiling sector", 2.0);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Continue dragging (Y-axis only in 3D view - TRLE constraint)
        if ctx.mouse.left_down {
            if !state.viewport_dragging_vertices.is_empty() {
                // In 3D view, vertices can ONLY move vertically (Y-axis)
                // X/Z positions are locked to sector grid and edited in 2D view only

                if !state.viewport_drag_started {
                    state.save_undo();
                    state.viewport_drag_started = true;
                }

                // Calculate Y delta from mouse movement (inverted: mouse up = positive Y)
                let mouse_delta_y = state.viewport_last_mouse.1 - mouse_pos.1;
                let y_sensitivity = 5.0; // Increased sensitivity for better feel
                let y_delta = mouse_delta_y * y_sensitivity;

                // Accumulate delta into the unsnapped drag plane Y (reference point)
                state.viewport_drag_plane_y += y_delta;

                // Calculate the delta from the initial average position
                let delta_from_initial = state.viewport_drag_plane_y -
                    (state.viewport_drag_initial_y.iter().sum::<f32>() / state.viewport_drag_initial_y.len() as f32);

                // Apply delta to each vertex's initial position, preserving relative heights
                for (i, &(room_idx, vert_idx)) in state.viewport_dragging_vertices.iter().enumerate() {
                    if let Some(initial_y) = state.viewport_drag_initial_y.get(i) {
                        if let Some(room) = state.level.rooms.get_mut(room_idx) {
                            if let Some(v) = room.vertices.get_mut(vert_idx) {
                                // Apply delta to initial position and snap
                                let new_y = initial_y + delta_from_initial;
                                v.y = (new_y / CLICK_HEIGHT).round() * CLICK_HEIGHT;
                            }
                        }
                    }
                }
            }
        }

        // End dragging on release
        if ctx.mouse.left_released {
            // Snap the final position to CLICK_HEIGHT grid for all vertices
            for &(room_idx, vert_idx) in &state.viewport_dragging_vertices {
                if let Some(room) = state.level.rooms.get_mut(room_idx) {
                    if let Some(v) = room.vertices.get_mut(vert_idx) {
                        v.y = (v.y / CLICK_HEIGHT).round() * CLICK_HEIGHT;
                    }
                }
            }

            state.viewport_dragging_vertices.clear();
            state.viewport_drag_initial_y.clear();
            state.viewport_drag_started = false;
        }
    }

    // Update mouse position for next frame (after all mouse interaction logic)
    state.viewport_last_mouse = mouse_pos;

    // Clear framebuffer
    fb.clear(RasterColor::new(30, 30, 40));

    // Draw grid on floor if enabled
    if state.show_grid {
        let grid_color = RasterColor::new(50, 50, 60);
        let grid_size = state.grid_size;
        let grid_extent = 10240.0; // Cover approximately 10 sectors in each direction

        // Calculate grid Y position based on lowest vertex in all rooms
        let mut grid_y = 0.0;
        if !state.level.rooms.is_empty() {
            let mut min_y = f32::MAX;
            for room in &state.level.rooms {
                for vert in &room.vertices {
                    let world_y = vert.y + room.position.y;
                    min_y = min_y.min(world_y);
                }
            }
            if min_y != f32::MAX {
                grid_y = min_y;
            }
        }

        // Draw grid lines - use shorter segments for better clipping behavior
        let segment_length: f32 = 1024.0; // One sector per segment

        // X-parallel lines (varying X, fixed Z)
        let mut z: f32 = -grid_extent;
        while z <= grid_extent {
            let mut x: f32 = -grid_extent;
            while x < grid_extent {
                let x_end = (x + segment_length).min(grid_extent);
                draw_3d_line(
                    fb,
                    Vec3::new(x, grid_y, z),
                    Vec3::new(x_end, grid_y, z),
                    &state.camera_3d,
                    grid_color,
                );
                x += segment_length;
            }
            z += grid_size;
        }

        // Z-parallel lines (fixed X, varying Z)
        let mut x: f32 = -grid_extent;
        while x <= grid_extent {
            let mut z: f32 = -grid_extent;
            while z < grid_extent {
                let z_end = (z + segment_length).min(grid_extent);
                draw_3d_line(
                    fb,
                    Vec3::new(x, grid_y, z),
                    Vec3::new(x, grid_y, z_end),
                    &state.camera_3d,
                    grid_color,
                );
                z += segment_length;
            }
            x += grid_size;
        }

        // Draw origin axes (slightly brighter) - also segmented
        let mut x = -grid_extent;
        while x < grid_extent {
            let x_end = (x + segment_length).min(grid_extent);
            draw_3d_line(fb, Vec3::new(x, grid_y, 0.0), Vec3::new(x_end, grid_y, 0.0), &state.camera_3d, RasterColor::new(100, 60, 60));
            x += segment_length;
        }
        let mut z = -grid_extent;
        while z < grid_extent {
            let z_end = (z + segment_length).min(grid_extent);
            draw_3d_line(fb, Vec3::new(0.0, grid_y, z), Vec3::new(0.0, grid_y, z_end), &state.camera_3d, RasterColor::new(60, 60, 100));
            z += segment_length;
        }
    }

    // Build texture map from texture packs
    let mut texture_map: std::collections::HashMap<(String, String), usize> = std::collections::HashMap::new();
    let mut texture_idx = 0;
    for pack in &state.texture_packs {
        for tex in &pack.textures {
            texture_map.insert((pack.name.clone(), tex.name.clone()), texture_idx);
            texture_idx += 1;
        }
    }

    // Texture resolver closure
    let resolve_texture = |tex_ref: &crate::world::TextureRef| -> Option<usize> {
        if !tex_ref.is_valid() {
            return Some(0); // Fallback to first texture
        }
        texture_map.get(&(tex_ref.pack.clone(), tex_ref.name.clone())).copied()
    };

    // Render all rooms
    for room in &state.level.rooms {
        let (vertices, faces) = room.to_render_data_with_textures(&resolve_texture);
        render_mesh(fb, &vertices, &faces, textures, &state.camera_3d, settings);
    }

    // Collect vertex screen positions for overlay drawing
    let mut vertex_overlays: Vec<(f32, f32, bool, bool)> = Vec::new(); // (screen_x, screen_y, is_selected, is_hovered)

    for (room_idx, room) in state.level.rooms.iter().enumerate() {
        for (vert_idx, vert) in room.vertices.iter().enumerate() {
            let world_pos = *vert + room.position;
            if let Some((fb_sx, fb_sy)) = world_to_screen(
                world_pos,
                state.camera_3d.position,
                state.camera_3d.basis_x,
                state.camera_3d.basis_y,
                state.camera_3d.basis_z,
                fb.width,
                fb.height,
            ) {
                let is_selected = matches!(state.selection, Selection::Vertex { room: r, vertex: v } if r == room_idx && v == vert_idx);
                let is_hovered = hovered_vertex.map_or(false, |(ri, vi, _)| ri == room_idx && vi == vert_idx);
                let is_dragging = state.viewport_dragging_vertices.contains(&(room_idx, vert_idx));

                vertex_overlays.push((fb_sx, fb_sy, is_selected || is_dragging, is_hovered));
            }
        }
    }

    // Store selected face edges for overlay
    let mut selected_face_screen_verts: Option<Vec<(f32, f32)>> = None;

    if let Selection::Face { room: room_idx, face: face_idx } = state.selection {
        if let Some(room) = state.level.rooms.get(room_idx) {
            if let Some(face) = room.faces.get(face_idx) {
                let v0 = room.vertices[face.indices[0]] + room.position;
                let v1 = room.vertices[face.indices[1]] + room.position;
                let v2 = room.vertices[face.indices[2]] + room.position;
                let v3 = room.vertices[face.indices[3]] + room.position;

                let world_verts = if face.is_triangle {
                    vec![v0, v1, v2]
                } else {
                    vec![v0, v1, v2, v3]
                };

                let mut screen_verts = Vec::new();
                let mut all_visible = true;

                for v in &world_verts {
                    if let Some((sx, sy)) = world_to_screen(
                        *v,
                        state.camera_3d.position,
                        state.camera_3d.basis_x,
                        state.camera_3d.basis_y,
                        state.camera_3d.basis_z,
                        fb.width,
                        fb.height,
                    ) {
                        screen_verts.push((sx, sy));
                    } else {
                        all_visible = false;
                        break;
                    }
                }

                if all_visible && screen_verts.len() >= 3 {
                    selected_face_screen_verts = Some(screen_verts);
                }
            }
        }
    }

    // Convert framebuffer to texture and draw to viewport
    let texture = Texture2D::from_rgba8(fb.width as u16, fb.height as u16, &fb.pixels);
    texture.set_filter(FilterMode::Nearest);

    draw_texture_ex(
        &texture,
        draw_x,
        draw_y,
        WHITE,
        DrawTextureParams {
            dest_size: Some(Vec2::new(draw_w, draw_h)),
            ..Default::default()
        },
    );

    // Draw viewport border
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, Color::from_rgba(60, 60, 60, 255));

    // Draw selected face outline as 2D overlay
    if let Some(screen_verts) = selected_face_screen_verts {
        let highlight_color = Color::from_rgba(255, 200, 50, 255);
        let n = screen_verts.len();

        for i in 0..n {
            let (x1, y1) = fb_to_screen(screen_verts[i].0, screen_verts[i].1);
            let (x2, y2) = fb_to_screen(screen_verts[(i + 1) % n].0, screen_verts[(i + 1) % n].1);
            draw_line(x1, y1, x2, y2, 2.0, highlight_color);
        }

        for (fx, fy) in &screen_verts {
            let (sx, sy) = fb_to_screen(*fx, *fy);
            draw_circle(sx, sy, 4.0, highlight_color);
        }
    }

    // Draw hovered edge highlight
    if let Some((room_idx, v0_idx, v1_idx, _)) = hovered_edge {
        if let Some(room) = state.level.rooms.get(room_idx) {
            let v0 = room.vertices[v0_idx] + room.position;
            let v1 = room.vertices[v1_idx] + room.position;

            if let (Some((sx0, sy0)), Some((sx1, sy1))) = (
                world_to_screen(v0, state.camera_3d.position, state.camera_3d.basis_x,
                    state.camera_3d.basis_y, state.camera_3d.basis_z, fb.width, fb.height),
                world_to_screen(v1, state.camera_3d.position, state.camera_3d.basis_x,
                    state.camera_3d.basis_y, state.camera_3d.basis_z, fb.width, fb.height)
            ) {
                let (screen_x0, screen_y0) = fb_to_screen(sx0, sy0);
                let (screen_x1, screen_y1) = fb_to_screen(sx1, sy1);
                draw_line(screen_x0, screen_y0, screen_x1, screen_y1, 3.0, Color::from_rgba(255, 200, 100, 255));
            }
        }
    }

    // Draw hovered face highlight
    if let Some((room_idx, face_idx)) = hovered_face {
        if let Some(room) = state.level.rooms.get(room_idx) {
            if let Some(face) = room.faces.get(face_idx) {
                let num_verts = if face.is_triangle { 3 } else { 4 };
                let mut screen_verts = Vec::new();

                for i in 0..num_verts {
                    let v = room.vertices[face.indices[i]] + room.position;
                    if let Some((sx, sy)) = world_to_screen(v, state.camera_3d.position,
                        state.camera_3d.basis_x, state.camera_3d.basis_y, state.camera_3d.basis_z,
                        fb.width, fb.height)
                    {
                        let (screen_x, screen_y) = fb_to_screen(sx, sy);
                        screen_verts.push((screen_x, screen_y));
                    }
                }

                // Draw semi-transparent overlay
                if screen_verts.len() >= 3 {
                    // Draw edges
                    for i in 0..screen_verts.len() {
                        let (x0, y0) = screen_verts[i];
                        let (x1, y1) = screen_verts[(i + 1) % screen_verts.len()];
                        draw_line(x0, y0, x1, y1, 2.0, Color::from_rgba(150, 200, 255, 200));
                    }
                }
            }
        }
    }

    // Draw vertex overlays
    for (fb_x, fb_y, is_selected, is_hovered) in vertex_overlays {
        let (sx, sy) = fb_to_screen(fb_x, fb_y);

        // Skip if outside viewport
        if sx < rect.x || sx > rect.right() || sy < rect.y || sy > rect.bottom() {
            continue;
        }

        let color = if is_selected {
            Color::from_rgba(100, 255, 100, 255) // Green when selected/dragging
        } else if is_hovered {
            Color::from_rgba(255, 200, 150, 255) // Orange when hovered
        } else {
            Color::from_rgba(200, 200, 220, 200) // Default (slightly transparent)
        };

        let radius = if is_selected || is_hovered { 5.0 } else { 3.0 };
        draw_circle(sx, sy, radius, color);
    }

    // Draw camera info (position and rotation)
    draw_text(
        &format!(
            "Cam: ({:.0}, {:.0}, {:.0}) | Rot: ({:.2}, {:.2})",
            state.camera_3d.position.x,
            state.camera_3d.position.y,
            state.camera_3d.position.z,
            state.camera_3d.rotation_x,
            state.camera_3d.rotation_y
        ),
        rect.x + 5.0,
        rect.bottom() - 5.0,
        14.0,
        Color::from_rgba(200, 200, 200, 255),
    );
}

/// Draw a 3D line into the framebuffer using Bresenham's algorithm
fn draw_3d_line(
    fb: &mut Framebuffer,
    p0: Vec3,
    p1: Vec3,
    camera: &crate::rasterizer::Camera,
    color: RasterColor,
) {
    const NEAR_PLANE: f32 = 0.1;

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
        // p0 is behind, clip it
        let t = (NEAR_PLANE - z0) / (z1 - z0);
        let new_p0 = p0 + (p1 - p0) * t;
        (new_p0, p1)
    } else if z1 <= NEAR_PLANE {
        // p1 is behind, clip it
        let t = (NEAR_PLANE - z0) / (z1 - z0);
        let new_p1 = p0 + (p1 - p0) * t;
        (p0, new_p1)
    } else {
        // Both in front
        (p0, p1)
    };

    // Project clipped endpoints
    let s0 = world_to_screen(clipped_p0, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height);
    let s1 = world_to_screen(clipped_p1, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height);

    let (Some((x0f, y0f)), Some((x1f, y1f))) = (s0, s1) else {
        return;
    };

    // Convert to integers for Bresenham
    let mut x0 = x0f as i32;
    let mut y0 = y0f as i32;
    let x1 = x1f as i32;
    let y1 = y1f as i32;

    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    let w = fb.width as i32;
    let h = fb.height as i32;

    loop {
        if x0 >= 0 && x0 < w && y0 >= 0 && y0 < h {
            fb.set_pixel(x0 as usize, y0 as usize, color);
        }

        if x0 == x1 && y0 == y1 {
            break;
        }

        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}
