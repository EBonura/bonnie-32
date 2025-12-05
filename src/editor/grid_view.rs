//! 2D Grid View - Top-down room editing

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext};
use super::{EditorState, Selection, SECTOR_SIZE};

/// Point-in-triangle test using barycentric coordinates
fn point_in_triangle(px: f32, py: f32, v0: (f32, f32), v1: (f32, f32), v2: (f32, f32)) -> bool {
    let area = 0.5 * (-v1.1 * v2.0 + v0.1 * (-v1.0 + v2.0) + v0.0 * (v1.1 - v2.1) + v1.0 * v2.1);
    let s = (v0.1 * v2.0 - v0.0 * v2.1 + (v2.1 - v0.1) * px + (v0.0 - v2.0) * py) / (2.0 * area);
    let t = (v0.0 * v1.1 - v0.1 * v1.0 + (v0.1 - v1.1) * px + (v1.0 - v0.0) * py) / (2.0 * area);
    s >= 0.0 && t >= 0.0 && (1.0 - s - t) >= 0.0
}

/// Point-in-quad test (two triangles)
fn point_in_quad(px: f32, py: f32, v0: (f32, f32), v1: (f32, f32), v2: (f32, f32), v3: (f32, f32)) -> bool {
    point_in_triangle(px, py, v0, v1, v2) || point_in_triangle(px, py, v0, v2, v3)
}

/// Draw the 2D grid view (top-down view of current room)
pub fn draw_grid_view(ctx: &mut UiContext, rect: Rect, state: &mut EditorState) {
    // Background
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(20, 20, 25, 255));

    let mouse_pos = (ctx.mouse.x, ctx.mouse.y);
    let inside = ctx.mouse.inside(&rect);

    // Handle pan and zoom
    if inside {
        // Zoom with scroll wheel
        if ctx.mouse.scroll != 0.0 {
            let zoom_factor = 1.0 + ctx.mouse.scroll * 0.02;
            // Adjusted zoom limits for TRLE scale (1024-unit sectors)
            // Allow more zoom out to see multiple sectors at once
            state.grid_zoom = (state.grid_zoom * zoom_factor).clamp(0.01, 2.0);
        }

        // Pan with right mouse button
        if ctx.mouse.right_down {
            if state.grid_panning {
                let dx = mouse_pos.0 - state.grid_last_mouse.0;
                let dy = mouse_pos.1 - state.grid_last_mouse.1;
                state.grid_offset_x += dx;
                state.grid_offset_y += dy;
            }
            state.grid_panning = true;
        } else {
            state.grid_panning = false;
        }
    } else {
        state.grid_panning = false;
    }
    state.grid_last_mouse = mouse_pos;

    let Some(room) = state.current_room() else {
        draw_text("No room", rect.x + 10.0, rect.y + 20.0, 14.0, Color::from_rgba(100, 100, 100, 255));
        return;
    };

    // Calculate view transform
    let center_x = rect.x + rect.w * 0.5 + state.grid_offset_x;
    let center_y = rect.y + rect.h * 0.5 + state.grid_offset_y;
    let scale = state.grid_zoom;

    // World to screen conversion (X-Z plane, viewing from top)
    let world_to_screen = |wx: f32, wz: f32| -> (f32, f32) {
        let sx = center_x + wx * scale;
        let sy = center_y - wz * scale; // Negated Z for top-down view
        (sx, sy)
    };

    // Draw grid lines
    if state.show_grid {
        let grid_color = Color::from_rgba(40, 40, 45, 255);
        let grid_step = state.grid_size;

        // Calculate visible grid range (negated Z for top-down view)
        let min_wx = (rect.x - center_x) / scale;
        let max_wx = (rect.right() - center_x) / scale;
        let min_wz = -(rect.bottom() - center_y) / scale; // Swapped and negated
        let max_wz = -(rect.y - center_y) / scale;

        // Vertical lines
        let start_x = (min_wx / grid_step).floor() * grid_step;
        let mut x = start_x;
        while x <= max_wx {
            let (sx, _) = world_to_screen(x, 0.0);
            if sx >= rect.x && sx <= rect.right() {
                let line_color = if (x / grid_step).abs() < 0.01 {
                    Color::from_rgba(80, 40, 40, 255) // Origin line (red-ish)
                } else {
                    grid_color
                };
                draw_line(sx, rect.y, sx, rect.bottom(), 1.0, line_color);
            }
            x += grid_step;
        }

        // Horizontal lines
        let start_z = (min_wz / grid_step).floor() * grid_step;
        let mut z = start_z;
        while z <= max_wz {
            let (_, sy) = world_to_screen(0.0, z);
            if sy >= rect.y && sy <= rect.bottom() {
                let line_color = if (z / grid_step).abs() < 0.01 {
                    Color::from_rgba(40, 80, 40, 255) // Origin line (green-ish)
                } else {
                    grid_color
                };
                draw_line(rect.x, sy, rect.right(), sy, 1.0, line_color);
            }
            z += grid_step;
        }
    }

    // Track hovered face for later (need to compute before drawing for highlight)
    let mut hovered_face_preview: Option<usize> = None;
    if inside {
        for (face_idx, face) in room.faces.iter().enumerate() {
            let v0 = room.vertices[face.indices[0]];
            let v1 = room.vertices[face.indices[1]];
            let v2 = room.vertices[face.indices[2]];
            let v3 = room.vertices[face.indices[3]];

            let s0 = world_to_screen(v0.x, v0.z);
            let s1 = world_to_screen(v1.x, v1.z);
            let s2 = world_to_screen(v2.x, v2.z);
            let s3 = world_to_screen(v3.x, v3.z);

            let in_face = if face.is_triangle {
                point_in_triangle(mouse_pos.0, mouse_pos.1, s0, s1, s2)
            } else {
                point_in_quad(mouse_pos.0, mouse_pos.1, s0, s1, s2, s3)
            };

            if in_face {
                hovered_face_preview = Some(face_idx);
                break;
            }
        }
    }

    // Draw room geometry (X-Z projection)
    // First pass: draw faces as filled polygons
    for (face_idx, face) in room.faces.iter().enumerate() {
        let v0 = room.vertices[face.indices[0]];
        let v1 = room.vertices[face.indices[1]];
        let v2 = room.vertices[face.indices[2]];
        let v3 = room.vertices[face.indices[3]];

        let (sx0, sy0) = world_to_screen(v0.x, v0.z);
        let (sx1, sy1) = world_to_screen(v1.x, v1.z);
        let (sx2, sy2) = world_to_screen(v2.x, v2.z);
        let (sx3, sy3) = world_to_screen(v3.x, v3.z);

        // Check if this face is selected or hovered
        let is_selected = matches!(state.selection, Selection::Face { face, .. } if face == face_idx);
        let is_hovered = hovered_face_preview == Some(face_idx);

        // Determine face type by normal (approximate from Y component)
        let edge1 = (v1.x - v0.x, v1.y - v0.y, v1.z - v0.z);
        let edge2 = (v2.x - v0.x, v2.y - v0.y, v2.z - v0.z);
        let normal_y = edge1.0 * edge2.2 - edge1.2 * edge2.0;

        let fill_color = if is_selected {
            Color::from_rgba(255, 200, 100, 150) // Yellow-orange for selected
        } else if is_hovered {
            Color::from_rgba(150, 200, 255, 120) // Light blue for hover
        } else if normal_y.abs() > 0.5 {
            Color::from_rgba(60, 120, 120, 100) // Cyan-ish for floor/ceiling
        } else {
            Color::from_rgba(100, 80, 60, 80) // Brown-ish for walls
        };

        // Draw as two triangles
        draw_triangle(
            Vec2::new(sx0, sy0),
            Vec2::new(sx1, sy1),
            Vec2::new(sx2, sy2),
            fill_color,
        );
        if !face.is_triangle {
            draw_triangle(
                Vec2::new(sx0, sy0),
                Vec2::new(sx2, sy2),
                Vec2::new(sx3, sy3),
                fill_color,
            );
        }
    }

    // Second pass: draw edges
    for face in &room.faces {
        let indices = if face.is_triangle {
            vec![0, 1, 2, 0]
        } else {
            vec![0, 1, 2, 3, 0]
        };

        for i in 0..indices.len() - 1 {
            let v0 = room.vertices[face.indices[indices[i]]];
            let v1 = room.vertices[face.indices[indices[i + 1]]];

            let (sx0, sy0) = world_to_screen(v0.x, v0.z);
            let (sx1, sy1) = world_to_screen(v1.x, v1.z);

            draw_line(sx0, sy0, sx1, sy1, 1.0, Color::from_rgba(150, 150, 160, 255));
        }
    }

    // Draw portals
    for portal in &room.portals {
        let v0 = portal.vertices[0];
        let v1 = portal.vertices[1];
        let v2 = portal.vertices[2];
        let v3 = portal.vertices[3];

        let (sx0, sy0) = world_to_screen(v0.x, v0.z);
        let (sx1, sy1) = world_to_screen(v1.x, v1.z);
        let (sx2, sy2) = world_to_screen(v2.x, v2.z);
        let (sx3, sy3) = world_to_screen(v3.x, v3.z);

        // Portal fill (magenta)
        draw_triangle(
            Vec2::new(sx0, sy0),
            Vec2::new(sx1, sy1),
            Vec2::new(sx2, sy2),
            Color::from_rgba(200, 50, 200, 80),
        );
        draw_triangle(
            Vec2::new(sx0, sy0),
            Vec2::new(sx2, sy2),
            Vec2::new(sx3, sy3),
            Color::from_rgba(200, 50, 200, 80),
        );

        // Portal outline
        draw_line(sx0, sy0, sx1, sy1, 2.0, Color::from_rgba(255, 100, 255, 255));
        draw_line(sx1, sy1, sx2, sy2, 2.0, Color::from_rgba(255, 100, 255, 255));
        draw_line(sx2, sy2, sx3, sy3, 2.0, Color::from_rgba(255, 100, 255, 255));
        draw_line(sx3, sy3, sx0, sy0, 2.0, Color::from_rgba(255, 100, 255, 255));
    }

    // Find vertex under mouse cursor (for selection/dragging)
    let mut hovered_vertex: Option<usize> = None;
    for (i, v) in room.vertices.iter().enumerate() {
        let (sx, sy) = world_to_screen(v.x, v.z);
        let dist = ((mouse_pos.0 - sx).powi(2) + (mouse_pos.1 - sy).powi(2)).sqrt();
        if dist < 8.0 {
            hovered_vertex = Some(i);
            break;
        }
    }

    // Find face under mouse cursor (only if no vertex is hovered)
    let mut hovered_face: Option<usize> = None;
    if hovered_vertex.is_none() && inside {
        for (face_idx, face) in room.faces.iter().enumerate() {
            let v0 = room.vertices[face.indices[0]];
            let v1 = room.vertices[face.indices[1]];
            let v2 = room.vertices[face.indices[2]];
            let v3 = room.vertices[face.indices[3]];

            let s0 = world_to_screen(v0.x, v0.z);
            let s1 = world_to_screen(v1.x, v1.z);
            let s2 = world_to_screen(v2.x, v2.z);
            let s3 = world_to_screen(v3.x, v3.z);

            let in_face = if face.is_triangle {
                point_in_triangle(mouse_pos.0, mouse_pos.1, s0, s1, s2)
            } else {
                point_in_quad(mouse_pos.0, mouse_pos.1, s0, s1, s2, s3)
            };

            if in_face {
                hovered_face = Some(face_idx);
                break;
            }
        }
    }

    // Store room index for later mutation
    let current_room_idx = state.current_room;

    // Draw vertices
    for (i, v) in room.vertices.iter().enumerate() {
        let (sx, sy) = world_to_screen(v.x, v.z);

        // Skip if outside view
        if sx < rect.x - 5.0 || sx > rect.right() + 5.0 || sy < rect.y - 5.0 || sy > rect.bottom() + 5.0 {
            continue;
        }

        let is_selected = matches!(state.selection, Selection::Vertex { vertex, .. } if vertex == i);
        let is_hovered = hovered_vertex == Some(i);
        let is_dragging = state.grid_dragging_vertex == Some(i);

        let color = if is_dragging {
            Color::from_rgba(100, 255, 100, 255) // Green while dragging
        } else if is_selected {
            Color::from_rgba(255, 255, 100, 255) // Yellow when selected
        } else if is_hovered {
            Color::from_rgba(255, 200, 150, 255) // Orange when hovered
        } else {
            Color::from_rgba(200, 200, 220, 255) // Default
        };

        let radius = if is_hovered || is_selected || is_dragging { 5.0 } else { 3.0 };
        draw_circle(sx, sy, radius, color);
    }

    // Draw room origin marker
    let (ox, oy) = world_to_screen(0.0, 0.0);
    if ox >= rect.x && ox <= rect.right() && oy >= rect.y && oy <= rect.bottom() {
        draw_circle(ox, oy, 5.0, Color::from_rgba(255, 100, 100, 255));
    }

    // Handle selection and interaction (only with left mouse, and not panning)
    if inside && !state.grid_panning {
        // Start action on left press
        if ctx.mouse.left_pressed {
            if let Some(vi) = hovered_vertex {
                // Vertex has priority - select and start dragging
                state.selection = Selection::Vertex { room: current_room_idx, vertex: vi };
                state.grid_dragging_vertex = Some(vi);
                state.grid_drag_started = false;
            } else if let Some(fi) = hovered_face {
                // Face clicked - select it and apply texture
                state.selection = Selection::Face { room: current_room_idx, face: fi };

                // Apply selected texture to face
                let texture_id = state.selected_texture;
                state.save_undo();
                if let Some(room) = state.level.rooms.get_mut(current_room_idx) {
                    if let Some(face) = room.faces.get_mut(fi) {
                        face.texture_id = texture_id;
                    }
                }
            } else {
                // Clicked empty space - deselect
                state.selection = Selection::None;
            }
        }

        // Continue dragging
        if ctx.mouse.left_down {
            if let Some(vi) = state.grid_dragging_vertex {
                // Save undo state on first actual movement
                if !state.grid_drag_started {
                    let dx = mouse_pos.0 - state.grid_last_mouse.0;
                    let dy = mouse_pos.1 - state.grid_last_mouse.1;
                    if dx.abs() > 1.0 || dy.abs() > 1.0 {
                        state.save_undo();
                        state.grid_drag_started = true;
                    }
                }

                // Move vertex to mouse position (convert screen to world)
                let wx = (mouse_pos.0 - center_x) / scale;
                let wz = -(mouse_pos.1 - center_y) / scale; // Negated for top-down view

                // Snap to TRLE sector grid (1024 units)
                let snapped_x = (wx / SECTOR_SIZE).round() * SECTOR_SIZE;
                let snapped_z = (wz / SECTOR_SIZE).round() * SECTOR_SIZE;

                // Update vertex position
                if let Some(room) = state.level.rooms.get_mut(current_room_idx) {
                    if let Some(v) = room.vertices.get_mut(vi) {
                        v.x = snapped_x;
                        v.z = snapped_z;
                    }
                }
            }
        }

        // End dragging on release
        if ctx.mouse.left_released {
            state.grid_dragging_vertex = None;
            state.grid_drag_started = false;
        }
    } else {
        // Mouse left the rect or started panning - cancel drag
        if !ctx.mouse.left_down {
            state.grid_dragging_vertex = None;
            state.grid_drag_started = false;
        }
    }
}
