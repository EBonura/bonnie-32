//! 2D Grid View - Top-down room editing

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext};
use super::EditorState;

/// Draw the 2D grid view (top-down view of current room)
pub fn draw_grid_view(ctx: &mut UiContext, rect: Rect, state: &mut EditorState) {
    // Clip to panel bounds
    // Note: macroquad doesn't have scissor rects built-in, so we'll just draw within bounds

    // Background
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(20, 20, 25, 255));

    // Handle pan and zoom
    if ctx.mouse.inside(&rect) {
        // Zoom with scroll
        if ctx.mouse.scroll != 0.0 {
            let zoom_factor = 1.0 + ctx.mouse.scroll * 0.1;
            state.grid_zoom = (state.grid_zoom * zoom_factor).clamp(5.0, 100.0);
        }

        // Pan with middle mouse or right mouse
        if ctx.mouse.right_down {
            // Would need delta tracking for proper pan
        }
    }

    let Some(room) = state.current_room() else {
        draw_text("No room", rect.x + 10.0, rect.y + 20.0, 14.0, Color::from_rgba(100, 100, 100, 255));
        return;
    };

    // Calculate view transform
    let center_x = rect.x + rect.w * 0.5 + state.grid_offset_x;
    let center_y = rect.y + rect.h * 0.5 + state.grid_offset_y;
    let scale = state.grid_zoom;

    // World to screen conversion (X-Z plane, Y is up)
    let world_to_screen = |wx: f32, wz: f32| -> (f32, f32) {
        let sx = center_x + wx * scale;
        let sy = center_y + wz * scale; // Z maps to screen Y
        (sx, sy)
    };

    // Draw grid lines
    if state.show_grid {
        let grid_color = Color::from_rgba(40, 40, 45, 255);
        let grid_step = state.grid_size;

        // Calculate visible grid range
        let min_wx = (rect.x - center_x) / scale;
        let max_wx = (rect.right() - center_x) / scale;
        let min_wz = (rect.y - center_y) / scale;
        let max_wz = (rect.bottom() - center_y) / scale;

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

        // Determine face type by normal (approximate from Y component)
        // Floor faces have normal pointing up (negative Y in our system)
        let edge1 = (v1.x - v0.x, v1.y - v0.y, v1.z - v0.z);
        let edge2 = (v2.x - v0.x, v2.y - v0.y, v2.z - v0.z);
        let normal_y = edge1.0 * edge2.2 - edge1.2 * edge2.0; // Cross product Y component

        let fill_color = if normal_y.abs() > 0.5 {
            // Floor/ceiling (horizontal face)
            Color::from_rgba(60, 120, 120, 100) // Cyan-ish
        } else {
            // Wall (vertical face)
            Color::from_rgba(100, 80, 60, 80) // Brown-ish
        };

        // Draw as two triangles (simple fill)
        // Note: macroquad doesn't have polygon fill, so we'll use triangles
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

        // Highlight selected face
        if let super::Selection::Face { room: _, face: sel_face } = state.selection {
            if sel_face == face_idx {
                draw_triangle(
                    Vec2::new(sx0, sy0),
                    Vec2::new(sx1, sy1),
                    Vec2::new(sx2, sy2),
                    Color::from_rgba(255, 200, 100, 100),
                );
            }
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

    // Draw vertices
    for (i, v) in room.vertices.iter().enumerate() {
        let (sx, sy) = world_to_screen(v.x, v.z);

        // Skip if outside view
        if sx < rect.x - 5.0 || sx > rect.right() + 5.0 || sy < rect.y - 5.0 || sy > rect.bottom() + 5.0 {
            continue;
        }

        let is_selected = matches!(state.selection, super::Selection::Vertex { vertex, .. } if vertex == i);
        let color = if is_selected {
            Color::from_rgba(255, 255, 100, 255)
        } else {
            Color::from_rgba(200, 200, 220, 255)
        };

        draw_circle(sx, sy, 3.0, color);
    }

    // Draw room origin marker
    let (ox, oy) = world_to_screen(0.0, 0.0);
    if ox >= rect.x && ox <= rect.right() && oy >= rect.y && oy <= rect.bottom() {
        draw_circle(ox, oy, 5.0, Color::from_rgba(255, 100, 100, 255));
    }
}
