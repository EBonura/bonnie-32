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

    // Find hovered sector (only for current room)
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

    // Draw sectors for ALL rooms (non-current rooms first, then current room on top)
    for (room_idx, r) in state.level.rooms.iter().enumerate() {
        let is_current_room = room_idx == current_room_idx;

        // Skip current room in first pass (draw it last so it's on top)
        if is_current_room {
            continue;
        }

        // Draw non-current room sectors with dimmed colors
        for (gx, gz, sector) in r.iter_sectors() {
            let base_x = r.position.x + (gx as f32) * SECTOR_SIZE;
            let base_z = r.position.z + (gz as f32) * SECTOR_SIZE;

            let (sx0, sy0) = world_to_screen(base_x, base_z);
            let (sx1, sy1) = world_to_screen(base_x + SECTOR_SIZE, base_z);
            let (sx2, sy2) = world_to_screen(base_x + SECTOR_SIZE, base_z + SECTOR_SIZE);
            let (sx3, sy3) = world_to_screen(base_x, base_z + SECTOR_SIZE);

            // Dimmed colors for non-current rooms
            let has_floor = sector.floor.is_some();
            let has_ceiling = sector.ceiling.is_some();

            let fill_color = if has_floor && has_ceiling {
                Color::from_rgba(40, 60, 55, 60) // Dim full sector
            } else if has_floor {
                Color::from_rgba(40, 55, 60, 60) // Dim floor only
            } else if has_ceiling {
                Color::from_rgba(55, 40, 60, 60) // Dim ceiling only
            } else {
                Color::from_rgba(50, 50, 50, 40) // Dim empty sector
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

            // Draw sector edges (dimmed)
            let edge_color = Color::from_rgba(60, 60, 65, 180);
            draw_line(sx0, sy0, sx1, sy1, 1.0, edge_color);
            draw_line(sx1, sy1, sx2, sy2, 1.0, edge_color);
            draw_line(sx2, sy2, sx3, sy3, 1.0, edge_color);
            draw_line(sx3, sy3, sx0, sy0, 1.0, edge_color);

            // Draw wall indicators (dimmed)
            let wall_color = Color::from_rgba(120, 90, 60, 180);
            if !sector.walls_north.is_empty() {
                draw_line(sx0, sy0, sx1, sy1, 2.0, wall_color);
            }
            if !sector.walls_east.is_empty() {
                draw_line(sx1, sy1, sx2, sy2, 2.0, wall_color);
            }
            if !sector.walls_south.is_empty() {
                draw_line(sx2, sy2, sx3, sy3, 2.0, wall_color);
            }
            if !sector.walls_west.is_empty() {
                draw_line(sx3, sy3, sx0, sy0, 2.0, wall_color);
            }
        }
    }

    // Draw current room sectors (on top, with full colors)
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
        let _has_walls = !sector.walls_north.is_empty() || !sector.walls_east.is_empty()
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

    // Draw room center markers for all rooms (handle for moving the room)
    let mut hovered_room_origin: Option<usize> = None;
    for (room_idx, r) in state.level.rooms.iter().enumerate() {
        // Calculate room center (not origin corner)
        let center_x = r.position.x + (r.width as f32 * SECTOR_SIZE) / 2.0;
        let center_z = r.position.z + (r.depth as f32 * SECTOR_SIZE) / 2.0;
        let (ox, oy) = world_to_screen(center_x, center_z);
        if ox >= rect.x - 10.0 && ox <= rect.right() + 10.0 && oy >= rect.y - 10.0 && oy <= rect.bottom() + 10.0 {
            let is_current = room_idx == current_room_idx;
            let dist_to_mouse = ((mouse_pos.0 - ox).powi(2) + (mouse_pos.1 - oy).powi(2)).sqrt();
            let is_hovered = inside && dist_to_mouse < 12.0;

            if is_hovered {
                hovered_room_origin = Some(room_idx);
            }

            // Draw center handle
            let color = if is_hovered {
                Color::from_rgba(255, 255, 150, 255) // Bright yellow when hovered
            } else if is_current {
                Color::from_rgba(255, 100, 100, 255) // Red for current room
            } else {
                Color::from_rgba(150, 80, 80, 255) // Dim for other rooms
            };

            // Draw crosshair for room center
            draw_circle(ox, oy, if is_hovered { 8.0 } else { 6.0 }, color);
            draw_line(ox - 12.0, oy, ox + 12.0, oy, 2.0, color);
            draw_line(ox, oy - 12.0, ox, oy + 12.0, 2.0, color);

            // Label with room index
            if is_current || is_hovered {
                draw_text(&format!("R{}", room_idx), ox + 14.0, oy - 4.0, 14.0, color);
            }
        }
    }

    // Draw ghost preview when dragging sectors
    if !state.grid_dragging_sectors.is_empty() && state.grid_sector_drag_start.is_some() {
        let (offset_x, offset_z) = state.grid_sector_drag_offset;

        for &(room_idx, gx, gz) in &state.grid_dragging_sectors {
            if let Some(r) = state.level.rooms.get(room_idx) {
                let base_x = r.position.x + (gx as f32) * SECTOR_SIZE + offset_x;
                let base_z = r.position.z + (gz as f32) * SECTOR_SIZE + offset_z;

                let (sx0, sy0) = world_to_screen(base_x, base_z);
                let (sx1, sy1) = world_to_screen(base_x + SECTOR_SIZE, base_z);
                let (sx2, sy2) = world_to_screen(base_x + SECTOR_SIZE, base_z + SECTOR_SIZE);
                let (sx3, sy3) = world_to_screen(base_x, base_z + SECTOR_SIZE);

                // Ghost fill
                draw_triangle(
                    Vec2::new(sx0, sy0),
                    Vec2::new(sx1, sy1),
                    Vec2::new(sx2, sy2),
                    Color::from_rgba(100, 200, 255, 100),
                );
                draw_triangle(
                    Vec2::new(sx0, sy0),
                    Vec2::new(sx2, sy2),
                    Vec2::new(sx3, sy3),
                    Color::from_rgba(100, 200, 255, 100),
                );

                // Ghost outline
                draw_line(sx0, sy0, sx1, sy1, 2.0, Color::from_rgba(100, 200, 255, 200));
                draw_line(sx1, sy1, sx2, sy2, 2.0, Color::from_rgba(100, 200, 255, 200));
                draw_line(sx2, sy2, sx3, sy3, 2.0, Color::from_rgba(100, 200, 255, 200));
                draw_line(sx3, sy3, sx0, sy0, 2.0, Color::from_rgba(100, 200, 255, 200));
            }
        }
    }

    // Draw ghost preview when dragging room center handle
    if state.grid_dragging_room_origin && state.grid_sector_drag_start.is_some() {
        let (offset_x, offset_z) = state.grid_sector_drag_offset;
        if let Some(r) = state.level.rooms.get(current_room_idx) {
            // Ghost at new center position
            let new_center_x = r.position.x + (r.width as f32 * SECTOR_SIZE) / 2.0 + offset_x;
            let new_center_z = r.position.z + (r.depth as f32 * SECTOR_SIZE) / 2.0 + offset_z;
            let (ox, oy) = world_to_screen(new_center_x, new_center_z);

            // Ghost center crosshair
            draw_circle(ox, oy, 8.0, Color::from_rgba(100, 255, 100, 200));
            draw_line(ox - 14.0, oy, ox + 14.0, oy, 2.0, Color::from_rgba(100, 255, 100, 200));
            draw_line(ox, oy - 14.0, ox, oy + 14.0, 2.0, Color::from_rgba(100, 255, 100, 200));
        }
    }

    // Draw selection rectangle
    if let (Some((sx0, sy0)), Some((sx1, sy1))) = (state.selection_rect_start, state.selection_rect_end) {
        let rect_x = sx0.min(sx1);
        let rect_y = sy0.min(sy1);
        let rect_w = (sx1 - sx0).abs();
        let rect_h = (sy1 - sy0).abs();

        // Only draw if it's a meaningful size
        if rect_w > 2.0 || rect_h > 2.0 {
            // Semi-transparent fill
            draw_rectangle(rect_x, rect_y, rect_w, rect_h, Color::from_rgba(100, 180, 255, 50));

            // Dashed outline effect (solid lines)
            let outline_color = Color::from_rgba(100, 180, 255, 200);
            draw_line(rect_x, rect_y, rect_x + rect_w, rect_y, 1.0, outline_color);
            draw_line(rect_x + rect_w, rect_y, rect_x + rect_w, rect_y + rect_h, 1.0, outline_color);
            draw_line(rect_x + rect_w, rect_y + rect_h, rect_x, rect_y + rect_h, 1.0, outline_color);
            draw_line(rect_x, rect_y + rect_h, rect_x, rect_y, 1.0, outline_color);
        }
    }

    // Handle selection and interaction
    if inside && !state.grid_panning {
        // Handle drag updates (when left button is held)
        if ctx.mouse.left_down && state.grid_sector_drag_start.is_some() {
            let (wx, wz) = screen_to_world(mouse_pos.0, mouse_pos.1);
            let (start_x, start_z) = state.grid_sector_drag_start.unwrap();
            state.grid_sector_drag_offset = (wx - start_x, wz - start_z);
        }

        // Handle selection rectangle drag update
        if ctx.mouse.left_down && state.selection_rect_start.is_some() {
            state.selection_rect_end = Some((mouse_pos.0, mouse_pos.1));
        }

        // Handle drag release
        if ctx.mouse.left_released && state.grid_sector_drag_start.is_some() {
            let (offset_x, offset_z) = state.grid_sector_drag_offset;

            // Snap offset to sector grid
            let snapped_offset_x = (offset_x / SECTOR_SIZE).round() * SECTOR_SIZE;
            let snapped_offset_z = (offset_z / SECTOR_SIZE).round() * SECTOR_SIZE;

            // Only apply if there's actual movement
            if snapped_offset_x.abs() >= SECTOR_SIZE * 0.5 || snapped_offset_z.abs() >= SECTOR_SIZE * 0.5 {
                state.save_undo();

                if state.grid_dragging_room_origin {
                    // Move entire room position
                    if let Some(room) = state.level.rooms.get_mut(current_room_idx) {
                        room.position.x += snapped_offset_x;
                        room.position.z += snapped_offset_z;
                    }
                    if let Some(room) = state.level.rooms.get(current_room_idx) {
                        state.set_status(&format!("Moved room to ({:.0}, {:.0})", room.position.x, room.position.z), 2.0);
                    }
                } else {
                    // Move selected sectors within the room grid
                    let grid_dx = (snapped_offset_x / SECTOR_SIZE).round() as i32;
                    let grid_dz = (snapped_offset_z / SECTOR_SIZE).round() as i32;

                    if let Some(room) = state.level.rooms.get_mut(current_room_idx) {
                        // Collect sectors to move
                        let sectors_to_move: Vec<_> = state.grid_dragging_sectors.iter()
                            .filter(|(r, _, _)| *r == current_room_idx)
                            .filter_map(|&(_, gx, gz)| {
                                room.sectors.get(gx).and_then(|col| col.get(gz)).cloned().flatten()
                                    .map(|sector| (gx, gz, sector))
                            })
                            .collect();

                        // Calculate how much we need to expand in negative direction
                        let mut min_new_gx: i32 = 0;
                        let mut min_new_gz: i32 = 0;
                        for (gx, gz, _) in &sectors_to_move {
                            let new_gx = *gx as i32 + grid_dx;
                            let new_gz = *gz as i32 + grid_dz;
                            min_new_gx = min_new_gx.min(new_gx);
                            min_new_gz = min_new_gz.min(new_gz);
                        }

                        // If we need to expand in negative direction, shift everything
                        let shift_x = if min_new_gx < 0 { -min_new_gx as usize } else { 0 };
                        let shift_z = if min_new_gz < 0 { -min_new_gz as usize } else { 0 };

                        if shift_x > 0 || shift_z > 0 {
                            // Expand grid to accommodate negative positions
                            // First, expand depth (add rows to each column)
                            if shift_z > 0 {
                                for col in &mut room.sectors {
                                    let mut new_col = vec![None; shift_z];
                                    new_col.append(col);
                                    *col = new_col;
                                }
                                room.depth += shift_z;
                            }

                            // Then, expand width (add new columns at front)
                            if shift_x > 0 {
                                let mut new_sectors = vec![vec![None; room.depth]; shift_x];
                                new_sectors.append(&mut room.sectors);
                                room.sectors = new_sectors;
                                room.width += shift_x;
                            }

                            // Adjust room origin to compensate for grid shift
                            room.position.x -= (shift_x as f32) * SECTOR_SIZE;
                            room.position.z -= (shift_z as f32) * SECTOR_SIZE;
                        }

                        // Clear old positions (adjusted for shift)
                        for &(_, gx, gz) in &state.grid_dragging_sectors {
                            let adj_gx = gx + shift_x;
                            let adj_gz = gz + shift_z;
                            if let Some(col) = room.sectors.get_mut(adj_gx) {
                                if let Some(cell) = col.get_mut(adj_gz) {
                                    *cell = None;
                                }
                            }
                        }

                        // Place at new positions (adjusted for shift)
                        for (old_gx, old_gz, sector) in sectors_to_move {
                            let new_gx = (old_gx as i32 + grid_dx + shift_x as i32) as usize;
                            let new_gz = (old_gz as i32 + grid_dz + shift_z as i32) as usize;

                            // Expand room grid if needed (for positive direction)
                            while new_gx >= room.width {
                                room.width += 1;
                                room.sectors.push(vec![None; room.depth]);
                            }
                            while new_gz >= room.depth {
                                room.depth += 1;
                                for col in &mut room.sectors {
                                    col.push(None);
                                }
                            }

                            room.sectors[new_gx][new_gz] = Some(sector);
                        }

                        room.trim_empty_edges();
                        room.recalculate_bounds();
                        state.set_status(&format!("Moved {} sector(s)", state.grid_dragging_sectors.len()), 2.0);
                    }
                }
            }

            // Clear drag state
            state.grid_dragging_sectors.clear();
            state.grid_sector_drag_offset = (0.0, 0.0);
            state.grid_sector_drag_start = None;
            state.grid_dragging_room_origin = false;
        }

        // Handle selection rectangle release
        if ctx.mouse.left_released && state.selection_rect_start.is_some() {
            if let (Some((sx0, sy0)), Some((sx1, sy1))) = (state.selection_rect_start, state.selection_rect_end) {
                // Convert screen rect to world rect
                let (wx0, wz0) = screen_to_world(sx0.min(sx1), sy0.max(sy1)); // Note: Y is inverted
                let (wx1, wz1) = screen_to_world(sx0.max(sx1), sy0.min(sy1));

                // Only select if rect is big enough (not just a click)
                let screen_dist = ((sx1 - sx0).powi(2) + (sy1 - sy0).powi(2)).sqrt();
                if screen_dist > 5.0 {
                    let shift_down = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);

                    // Find all sectors within the world rect
                    let mut selected_sectors = Vec::new();
                    for (gx, gz, _sector) in room.iter_sectors() {
                        let sector_x = room.position.x + (gx as f32) * SECTOR_SIZE;
                        let sector_z = room.position.z + (gz as f32) * SECTOR_SIZE;
                        let sector_center_x = sector_x + SECTOR_SIZE * 0.5;
                        let sector_center_z = sector_z + SECTOR_SIZE * 0.5;

                        // Check if sector center is within selection rect
                        if sector_center_x >= wx0 && sector_center_x <= wx1 &&
                           sector_center_z >= wz0 && sector_center_z <= wz1 {
                            selected_sectors.push((gx, gz));
                        }
                    }

                    if !selected_sectors.is_empty() {
                        if !shift_down {
                            state.clear_multi_selection();
                        }

                        // Add all selected sectors to multi-selection
                        for (gx, gz) in &selected_sectors {
                            let sel = Selection::Sector { room: current_room_idx, x: *gx, z: *gz };
                            state.add_to_multi_selection(sel);
                        }

                        // Set primary selection to first sector
                        if let Some(&(gx, gz)) = selected_sectors.first() {
                            state.set_selection(Selection::Sector { room: current_room_idx, x: gx, z: gz });
                        }

                        state.set_status(&format!("Selected {} sector(s)", selected_sectors.len()), 2.0);
                    }
                }
            }

            // Clear selection rect state
            state.selection_rect_start = None;
            state.selection_rect_end = None;
        }

        if ctx.mouse.left_pressed {
            use super::EditorTool;

            // Detect Shift key for multi-select
            let shift_down = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);

            match state.tool {
                EditorTool::Select => {
                    // Check if clicking on room origin first
                    if let Some(origin_room_idx) = hovered_room_origin {
                        // Start dragging room origin
                        state.current_room = origin_room_idx;
                        state.grid_dragging_room_origin = true;
                        let (wx, wz) = screen_to_world(mouse_pos.0, mouse_pos.1);
                        state.grid_sector_drag_start = Some((wx, wz));
                        state.grid_sector_drag_offset = (0.0, 0.0);
                    } else if let Some((gx, gz)) = hovered_sector {
                        // Check if clicking on an already-selected sector (start drag)
                        let is_already_selected = matches!(state.selection, Selection::Sector { room, x, z }
                            if room == current_room_idx && x == gx && z == gz)
                            || state.multi_selection.iter().any(|sel|
                                matches!(sel, Selection::Sector { room, x, z }
                                    if *room == current_room_idx && *x == gx && *z == gz));

                        if is_already_selected && !shift_down {
                            // Start dragging selected sectors
                            state.grid_dragging_sectors.clear();

                            // Add primary selection
                            if let Selection::Sector { room, x, z } = state.selection {
                                state.grid_dragging_sectors.push((room, x, z));
                            }

                            // Add multi-selection
                            for sel in &state.multi_selection {
                                if let Selection::Sector { room, x, z } = sel {
                                    if !state.grid_dragging_sectors.contains(&(*room, *x, *z)) {
                                        state.grid_dragging_sectors.push((*room, *x, *z));
                                    }
                                }
                            }

                            let (wx, wz) = screen_to_world(mouse_pos.0, mouse_pos.1);
                            state.grid_sector_drag_start = Some((wx, wz));
                            state.grid_sector_drag_offset = (0.0, 0.0);
                        } else {
                            // Normal selection
                            let new_selection = Selection::Sector { room: current_room_idx, x: gx, z: gz };
                            if shift_down {
                                state.toggle_multi_selection(new_selection.clone());
                                state.set_selection(new_selection);
                            } else {
                                state.clear_multi_selection();
                                state.set_selection(new_selection);
                            }
                        }
                    } else {
                        // Clicked on empty space - start selection rectangle
                        if !shift_down {
                            state.set_selection(Selection::None);
                            state.clear_multi_selection();
                        }
                        // Start selection rect (in screen coords)
                        state.selection_rect_start = Some((mouse_pos.0, mouse_pos.1));
                        state.selection_rect_end = Some((mouse_pos.0, mouse_pos.1));
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
