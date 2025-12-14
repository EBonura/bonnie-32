//! 2D Grid View - Top-down room editing
//!
//! Sector-based geometry system - selection and editing works on sectors.

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext};
use crate::world::SECTOR_SIZE;
use super::{EditorState, Selection, CEILING_HEIGHT};

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

    // Clone room for read-only access
    let room = match state.level.rooms.get(state.current_room) {
        Some(r) => r.clone(),
        None => {
            draw_text("No room", rect.x + 10.0, rect.y + 20.0, 14.0, Color::from_rgba(100, 100, 100, 255));
            return;
        }
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

    // Screen to world conversion
    let screen_to_world = |sx: f32, sy: f32| -> (f32, f32) {
        let wx = (sx - center_x) / scale;
        let wz = -(sy - center_y) / scale;
        (wx, wz)
    };

    // Enable scissor rectangle to clip drawing to viewport bounds
    let dpi = screen_dpi_scale();
    gl_use_default_material();
    unsafe {
        get_internal_gl().quad_gl.scissor(
            Some((
                (rect.x * dpi) as i32,
                (rect.y * dpi) as i32,
                (rect.w * dpi) as i32,
                (rect.h * dpi) as i32
            ))
        );
    }

    // Draw grid lines
    if state.show_grid {
        let grid_color = Color::from_rgba(40, 40, 45, 255);
        let grid_step = state.grid_size;

        // Calculate visible grid range
        let min_wx = (rect.x - center_x) / scale;
        let max_wx = (rect.right() - center_x) / scale;
        let min_wz = -(rect.bottom() - center_y) / scale;
        let max_wz = -(rect.y - center_y) / scale;

        // Vertical lines
        let start_x = (min_wx / grid_step).floor() * grid_step;
        let mut x = start_x;
        while x <= max_wx {
            let (sx, _) = world_to_screen(x, 0.0);
            if sx >= rect.x && sx <= rect.right() {
                let line_color = if (x / grid_step).abs() < 0.01 {
                    Color::from_rgba(80, 40, 40, 255)
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
                    Color::from_rgba(40, 80, 40, 255)
                } else {
                    grid_color
                };
                draw_line(rect.x, sy, rect.right(), sy, 1.0, line_color);
            }
            z += grid_step;
        }
    }

    // Store room index
    let current_room_idx = state.current_room;

    // Find hovered sector
    let mut hovered_sector: Option<(usize, usize)> = None;
    if inside {
        let (wx, wz) = screen_to_world(mouse_pos.0, mouse_pos.1);
        // Convert to grid coords relative to room position
        let local_x = wx - room.position.x;
        let local_z = wz - room.position.z;
        if local_x >= 0.0 && local_z >= 0.0 {
            let gx = (local_x / SECTOR_SIZE) as usize;
            let gz = (local_z / SECTOR_SIZE) as usize;
            if gx < room.width && gz < room.depth {
                if room.get_sector(gx, gz).is_some() {
                    hovered_sector = Some((gx, gz));
                }
            }
        }
    }

    // Draw sectors
    for (gx, gz, sector) in room.iter_sectors() {
        let base_x = room.position.x + (gx as f32) * SECTOR_SIZE;
        let base_z = room.position.z + (gz as f32) * SECTOR_SIZE;

        let (sx0, sy0) = world_to_screen(base_x, base_z);
        let (sx1, sy1) = world_to_screen(base_x + SECTOR_SIZE, base_z);
        let (sx2, sy2) = world_to_screen(base_x + SECTOR_SIZE, base_z + SECTOR_SIZE);
        let (sx3, sy3) = world_to_screen(base_x, base_z + SECTOR_SIZE);

        let is_hovered = hovered_sector == Some((gx, gz));
        let is_selected = matches!(state.selection, Selection::Sector { x, z, .. } if x == gx && z == gz);
        let is_multi_selected = state.multi_selection.iter().any(|sel| {
            matches!(sel, Selection::Sector { x, z, .. } if *x == gx && *z == gz)
        });

        // Determine fill color based on sector contents
        let has_floor = sector.floor.is_some();
        let has_ceiling = sector.ceiling.is_some();
        let has_walls = !sector.walls_north.is_empty() || !sector.walls_east.is_empty()
            || !sector.walls_south.is_empty() || !sector.walls_west.is_empty();

        let fill_color = if is_selected || is_multi_selected {
            Color::from_rgba(255, 200, 100, 150)
        } else if is_hovered {
            Color::from_rgba(150, 200, 255, 120)
        } else if has_floor && has_ceiling {
            Color::from_rgba(60, 120, 100, 100) // Full sector
        } else if has_floor {
            Color::from_rgba(60, 100, 120, 100) // Floor only
        } else if has_ceiling {
            Color::from_rgba(100, 60, 120, 100) // Ceiling only
        } else {
            Color::from_rgba(80, 80, 80, 60) // Empty sector
        };

        // Draw sector fill
        draw_triangle(
            Vec2::new(sx0, sy0),
            Vec2::new(sx1, sy1),
            Vec2::new(sx2, sy2),
            fill_color,
        );
        draw_triangle(
            Vec2::new(sx0, sy0),
            Vec2::new(sx2, sy2),
            Vec2::new(sx3, sy3),
            fill_color,
        );

        // Draw sector edges
        let edge_color = if is_selected || is_multi_selected || is_hovered {
            Color::from_rgba(200, 200, 220, 255)
        } else {
            Color::from_rgba(100, 100, 110, 255)
        };
        draw_line(sx0, sy0, sx1, sy1, 1.0, edge_color);
        draw_line(sx1, sy1, sx2, sy2, 1.0, edge_color);
        draw_line(sx2, sy2, sx3, sy3, 1.0, edge_color);
        draw_line(sx3, sy3, sx0, sy0, 1.0, edge_color);

        // Draw wall indicators on edges that have walls
        let wall_color = Color::from_rgba(200, 150, 100, 255);
        if !sector.walls_north.is_empty() {
            draw_line(sx0, sy0, sx1, sy1, 3.0, wall_color);
        }
        if !sector.walls_east.is_empty() {
            draw_line(sx1, sy1, sx2, sy2, 3.0, wall_color);
        }
        if !sector.walls_south.is_empty() {
            draw_line(sx2, sy2, sx3, sy3, 3.0, wall_color);
        }
        if !sector.walls_west.is_empty() {
            draw_line(sx3, sy3, sx0, sy0, 3.0, wall_color);
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

    // Draw room origin marker
    let (ox, oy) = world_to_screen(0.0, 0.0);
    if ox >= rect.x && ox <= rect.right() && oy >= rect.y && oy <= rect.bottom() {
        draw_circle(ox, oy, 5.0, Color::from_rgba(255, 100, 100, 255));
    }

    // Handle selection and interaction
    if inside && !state.grid_panning {
        if ctx.mouse.left_pressed {
            use super::EditorTool;

            // Detect Shift key for multi-select
            let shift_down = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);

            match state.tool {
                EditorTool::Select => {
                    if let Some((gx, gz)) = hovered_sector {
                        let new_selection = Selection::Sector { room: current_room_idx, x: gx, z: gz };
                        if shift_down {
                            state.toggle_multi_selection(new_selection.clone());
                            state.set_selection(new_selection);
                        } else {
                            state.clear_multi_selection();
                            state.set_selection(new_selection);
                        }
                    } else {
                        // Clicked on nothing - clear selection (unless Shift is held)
                        if !shift_down {
                            state.set_selection(Selection::None);
                            state.clear_multi_selection();
                        }
                    }
                }

                EditorTool::DrawFloor => {
                    let (wx, wz) = screen_to_world(mouse_pos.0, mouse_pos.1);
                    let snapped_x = (wx / SECTOR_SIZE).floor() * SECTOR_SIZE;
                    let snapped_z = (wz / SECTOR_SIZE).floor() * SECTOR_SIZE;

                    // Check if sector already has a floor
                    let gx = ((snapped_x - room.position.x) / SECTOR_SIZE) as usize;
                    let gz = ((snapped_z - room.position.z) / SECTOR_SIZE) as usize;

                    let has_floor = room.get_sector(gx, gz)
                        .map(|s| s.floor.is_some())
                        .unwrap_or(false);

                    if has_floor {
                        state.set_status("Sector already has a floor", 2.0);
                    } else {
                        state.save_undo();

                        if let Some(room) = state.level.rooms.get_mut(current_room_idx) {
                            // Expand room grid if needed
                            while gx >= room.width {
                                room.width += 1;
                                room.sectors.push((0..room.depth).map(|_| None).collect());
                            }
                            while gz >= room.depth {
                                room.depth += 1;
                                for col in &mut room.sectors {
                                    col.push(None);
                                }
                            }

                            room.set_floor(gx, gz, 0.0, state.selected_texture.clone());
                            room.recalculate_bounds();
                            state.set_status("Created floor sector", 2.0);
                        }
                    }
                }

                EditorTool::DrawCeiling => {
                    let (wx, wz) = screen_to_world(mouse_pos.0, mouse_pos.1);
                    let snapped_x = (wx / SECTOR_SIZE).floor() * SECTOR_SIZE;
                    let snapped_z = (wz / SECTOR_SIZE).floor() * SECTOR_SIZE;

                    let gx = ((snapped_x - room.position.x) / SECTOR_SIZE) as usize;
                    let gz = ((snapped_z - room.position.z) / SECTOR_SIZE) as usize;

                    let has_ceiling = room.get_sector(gx, gz)
                        .map(|s| s.ceiling.is_some())
                        .unwrap_or(false);

                    if has_ceiling {
                        state.set_status("Sector already has a ceiling", 2.0);
                    } else {
                        state.save_undo();

                        if let Some(room) = state.level.rooms.get_mut(current_room_idx) {
                            // Expand room grid if needed
                            while gx >= room.width {
                                room.width += 1;
                                room.sectors.push((0..room.depth).map(|_| None).collect());
                            }
                            while gz >= room.depth {
                                room.depth += 1;
                                for col in &mut room.sectors {
                                    col.push(None);
                                }
                            }

                            room.set_ceiling(gx, gz, CEILING_HEIGHT, state.selected_texture.clone());
                            room.recalculate_bounds();
                            state.set_status("Created ceiling sector", 2.0);
                        }
                    }
                }

                EditorTool::DrawWall => {
                    state.set_status("Wall tool: not yet implemented", 3.0);
                }

                _ => {}
            }
        }
    }

    // Disable scissor rectangle
    unsafe {
        get_internal_gl().quad_gl.scissor(None);
    }
}
