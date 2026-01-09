//! 2D Grid View - Top-down room editing
//!
//! Sector-based geometry system - selection and editing works on sectors.

use macroquad::prelude::*;
use crate::rasterizer::Vec3;
use crate::ui::{Rect, UiContext};
use crate::world::{Direction, SplitDirection, SECTOR_SIZE};
use super::{EditorState, EditorTool, Selection, GridViewMode, CEILING_HEIGHT, CLICK_HEIGHT};

/// Determine which edge of a sector the mouse is closest to (in Top view mode)
/// Returns the direction of the closest edge based on position within the sector
fn closest_edge_top_view(local_x: f32, local_z: f32) -> Direction {
    // Get position within the sector (0.0 to 1.0)
    let fx = (local_x / SECTOR_SIZE).fract();
    let fz = (local_z / SECTOR_SIZE).fract();

    // Handle negative fractions
    let fx = if fx < 0.0 { fx + 1.0 } else { fx };
    let fz = if fz < 0.0 { fz + 1.0 } else { fz };

    // Calculate distances to each edge
    let dist_north = fz;           // Distance to -Z edge (top in screen coords)
    let dist_south = 1.0 - fz;     // Distance to +Z edge (bottom in screen coords)
    let dist_west = fx;            // Distance to -X edge (left)
    let dist_east = 1.0 - fx;      // Distance to +X edge (right)

    // Find minimum distance
    let min_dist = dist_north.min(dist_south).min(dist_west).min(dist_east);

    if min_dist == dist_north {
        Direction::North
    } else if min_dist == dist_south {
        Direction::South
    } else if min_dist == dist_west {
        Direction::West
    } else {
        Direction::East
    }
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
            let zoom_factor = 1.0 + ctx.mouse.scroll * 0.008;
            state.grid_zoom = (state.grid_zoom * zoom_factor).clamp(0.002, 2.0);
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

    // Get the current view mode
    let view_mode = state.grid_view_mode;

    // World to screen conversion - depends on view mode
    // Returns (screen_x, screen_y) from (world_a, world_b) where a,b depend on view mode
    let world_to_screen = |wa: f32, wb: f32| -> (f32, f32) {
        let sx = center_x + wa * scale;
        let sy = center_y - wb * scale; // Negated for screen Y (up is negative)
        (sx, sy)
    };

    // Screen to world conversion
    let screen_to_world = |sx: f32, sy: f32| -> (f32, f32) {
        let wa = (sx - center_x) / scale;
        let wb = -(sy - center_y) / scale;
        (wa, wb)
    };

    // Helper to convert full 3D world position to the 2D plane based on view mode
    let world_pos_to_plane = |x: f32, y: f32, z: f32| -> (f32, f32) {
        match view_mode {
            GridViewMode::Top => (x, z),      // X-Z plane
            GridViewMode::Front => (x, y),    // X-Y plane
            GridViewMode::Side => (z, y),     // Z-Y plane
        }
    };

    // Helper to convert 2D plane position back to world offset
    // Returns (dx, dy, dz) offset to apply
    let plane_to_world_offset = |da: f32, db: f32| -> (f32, f32, f32) {
        match view_mode {
            GridViewMode::Top => (da, 0.0, db),    // X-Z plane
            GridViewMode::Front => (da, db, 0.0),  // X-Y plane
            GridViewMode::Side => (0.0, db, da),   // Z-Y plane
        }
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

    // Find hovered sector and edge (only for current room)
    let mut hovered_sector: Option<(usize, usize)> = None;
    let mut hovered_edge: Option<Direction> = None;
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
                    // Determine closest edge (only relevant for wall mode in Top view)
                    if view_mode == GridViewMode::Top {
                        hovered_edge = Some(closest_edge_top_view(local_x, local_z));
                    }
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

        // Skip hidden rooms
        if state.hidden_rooms.contains(&room_idx) {
            continue;
        }

        // Draw non-current room sectors with dimmed colors
        for (gx, gz, sector) in r.iter_sectors() {
            let base_x = r.position.x + (gx as f32) * SECTOR_SIZE;
            let base_z = r.position.z + (gz as f32) * SECTOR_SIZE;

            // Get floor and ceiling heights for side views
            let floor_y = r.position.y + sector.floor.as_ref().map(|f| f.avg_height()).unwrap_or(0.0);
            let ceil_y = r.position.y + sector.ceiling.as_ref().map(|c| c.avg_height()).unwrap_or(CEILING_HEIGHT);

            // Calculate screen corners based on view mode
            let (sx0, sy0, sx1, sy1, sx2, sy2, sx3, sy3) = match view_mode {
                GridViewMode::Top => {
                    let (s0, t0) = world_to_screen(base_x, base_z);
                    let (s1, t1) = world_to_screen(base_x + SECTOR_SIZE, base_z);
                    let (s2, t2) = world_to_screen(base_x + SECTOR_SIZE, base_z + SECTOR_SIZE);
                    let (s3, t3) = world_to_screen(base_x, base_z + SECTOR_SIZE);
                    (s0, t0, s1, t1, s2, t2, s3, t3)
                }
                GridViewMode::Front => {
                    let (s0, t0) = world_to_screen(base_x, floor_y);
                    let (s1, t1) = world_to_screen(base_x + SECTOR_SIZE, floor_y);
                    let (s2, t2) = world_to_screen(base_x + SECTOR_SIZE, ceil_y);
                    let (s3, t3) = world_to_screen(base_x, ceil_y);
                    (s0, t0, s1, t1, s2, t2, s3, t3)
                }
                GridViewMode::Side => {
                    let (s0, t0) = world_to_screen(base_z, floor_y);
                    let (s1, t1) = world_to_screen(base_z + SECTOR_SIZE, floor_y);
                    let (s2, t2) = world_to_screen(base_z + SECTOR_SIZE, ceil_y);
                    let (s3, t3) = world_to_screen(base_z, ceil_y);
                    (s0, t0, s1, t1, s2, t2, s3, t3)
                }
            };

            // Dimmed colors for non-current rooms
            let has_floor = sector.floor.is_some();
            let has_ceiling = sector.ceiling.is_some();
            let has_walls = !sector.walls_north.is_empty() || !sector.walls_east.is_empty()
                || !sector.walls_south.is_empty() || !sector.walls_west.is_empty();

            // Skip empty sectors in non-current rooms
            if !has_floor && !has_ceiling && !has_walls {
                continue;
            }

            let fill_color = if has_floor && has_ceiling {
                Color::from_rgba(40, 60, 55, 60) // Dim full sector
            } else if has_floor {
                Color::from_rgba(40, 55, 60, 60) // Dim floor only
            } else if has_ceiling {
                Color::from_rgba(55, 40, 60, 60) // Dim ceiling only
            } else {
                Color::from_rgba(50, 50, 50, 40) // Walls only
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

        // Get floor and ceiling heights for side views
        let floor_y = room.position.y + sector.floor.as_ref().map(|f| f.avg_height()).unwrap_or(0.0);
        let ceil_y = room.position.y + sector.ceiling.as_ref().map(|c| c.avg_height()).unwrap_or(CEILING_HEIGHT);

        // Calculate screen corners based on view mode
        let (sx0, sy0, sx1, sy1, sx2, sy2, sx3, sy3) = match view_mode {
            GridViewMode::Top => {
                // X-Z footprint (corners: NW, NE, SE, SW)
                let (s0, t0) = world_to_screen(base_x, base_z);
                let (s1, t1) = world_to_screen(base_x + SECTOR_SIZE, base_z);
                let (s2, t2) = world_to_screen(base_x + SECTOR_SIZE, base_z + SECTOR_SIZE);
                let (s3, t3) = world_to_screen(base_x, base_z + SECTOR_SIZE);
                (s0, t0, s1, t1, s2, t2, s3, t3)
            }
            GridViewMode::Front => {
                // X-Y rectangle (bottom-left, bottom-right, top-right, top-left)
                let (s0, t0) = world_to_screen(base_x, floor_y);
                let (s1, t1) = world_to_screen(base_x + SECTOR_SIZE, floor_y);
                let (s2, t2) = world_to_screen(base_x + SECTOR_SIZE, ceil_y);
                let (s3, t3) = world_to_screen(base_x, ceil_y);
                (s0, t0, s1, t1, s2, t2, s3, t3)
            }
            GridViewMode::Side => {
                // Z-Y rectangle (bottom-left, bottom-right, top-right, top-left)
                let (s0, t0) = world_to_screen(base_z, floor_y);
                let (s1, t1) = world_to_screen(base_z + SECTOR_SIZE, floor_y);
                let (s2, t2) = world_to_screen(base_z + SECTOR_SIZE, ceil_y);
                let (s3, t3) = world_to_screen(base_z, ceil_y);
                (s0, t0, s1, t1, s2, t2, s3, t3)
            }
        };

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
        let has_geometry = has_floor || has_ceiling || has_walls;

        // Skip rendering empty sectors unless they're being interacted with
        if !has_geometry && !is_selected && !is_multi_selected && !is_hovered {
            continue;
        }

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
            Color::from_rgba(80, 80, 80, 60) // Empty sector (only shown when selected/hovered)
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

        // Draw diagonal split indicator (only in Top view mode for now)
        // World coordinates: sx0=NW, sx1=NE, sx2=SE, sx3=SW (based on base_x, base_z mapping)
        // Note: On screen these appear flipped because screen Y is inverted
        if view_mode == GridViewMode::Top {
            let diag_color = Color::from_rgba(255, 180, 100, 200);

            // Check floor split direction - draw the diagonal line
            if let Some(floor) = &sector.floor {
                match floor.split_direction {
                    SplitDirection::NwSe => {
                        // NW-SE diagonal: from sx0 (NW) to sx2 (SE)
                        draw_line(sx0, sy0, sx2, sy2, 2.0, diag_color);
                    }
                    SplitDirection::NeSw => {
                        // NE-SW diagonal: from sx1 (NE) to sx3 (SW)
                        draw_line(sx1, sy1, sx3, sy3, 2.0, diag_color);
                    }
                }
            }

            // Check ceiling split direction (draw with different color if different from floor)
            if let Some(ceil) = &sector.ceiling {
                let floor_split = sector.floor.as_ref().map(|f| f.split_direction).unwrap_or(SplitDirection::NwSe);
                // Only draw if different from floor (avoid duplicate lines)
                if ceil.split_direction != floor_split {
                    let ceil_diag_color = Color::from_rgba(180, 100, 255, 200);
                    match ceil.split_direction {
                        SplitDirection::NwSe => {
                            draw_line(sx0, sy0, sx2, sy2, 2.0, ceil_diag_color);
                        }
                        SplitDirection::NeSw => {
                            draw_line(sx1, sy1, sx3, sy3, 2.0, ceil_diag_color);
                        }
                    }
                }
            }
        }

        // Draw sector edges
        let is_highlighted = is_selected || is_multi_selected || is_hovered;
        let edge_color = if is_highlighted {
            Color::from_rgba(200, 200, 220, 255)
        } else {
            Color::from_rgba(100, 100, 110, 255)
        };
        let edge_thickness = if is_highlighted { 2.0 } else { 1.0 };
        draw_line(sx0, sy0, sx1, sy1, edge_thickness, edge_color);
        draw_line(sx1, sy1, sx2, sy2, edge_thickness, edge_color);
        draw_line(sx2, sy2, sx3, sy3, edge_thickness, edge_color);
        draw_line(sx3, sy3, sx0, sy0, edge_thickness, edge_color);

        // Draw vertex indicators for highlighted sectors
        if is_highlighted {
            let vertex_color = Color::from_rgba(255, 255, 255, 200);
            let vertex_radius = 3.0;
            draw_circle(sx0, sy0, vertex_radius, vertex_color);
            draw_circle(sx1, sy1, vertex_radius, vertex_color);
            draw_circle(sx2, sy2, vertex_radius, vertex_color);
            draw_circle(sx3, sy3, vertex_radius, vertex_color);
        }

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

        // Draw diagonal wall indicators
        let diag_wall_color = Color::from_rgba(220, 180, 120, 255);
        if !sector.walls_nwse.is_empty() {
            // NW-SE diagonal: from NW corner (sx0) to SE corner (sx2)
            draw_line(sx0, sy0, sx2, sy2, 3.0, diag_wall_color);
        }
        if !sector.walls_nesw.is_empty() {
            // NE-SW diagonal: from NE corner (sx1) to SW corner (sx3)
            draw_line(sx1, sy1, sx3, sy3, 3.0, diag_wall_color);
        }
    }

    // Draw wall edge highlight when in wall mode with hovered sector (Top view only)
    if view_mode == GridViewMode::Top && state.tool == super::EditorTool::DrawWall {
        if let (Some((gx, gz)), Some(edge_dir)) = (hovered_sector, hovered_edge) {
            let base_x = room.position.x + (gx as f32) * SECTOR_SIZE;
            let base_z = room.position.z + (gz as f32) * SECTOR_SIZE;

            // Get screen coords for sector corners
            let (sx0, sy0) = world_to_screen(base_x, base_z);                          // NW
            let (sx1, sy1) = world_to_screen(base_x + SECTOR_SIZE, base_z);            // NE
            let (sx2, sy2) = world_to_screen(base_x + SECTOR_SIZE, base_z + SECTOR_SIZE); // SE
            let (sx3, sy3) = world_to_screen(base_x, base_z + SECTOR_SIZE);            // SW

            // Determine which edge to highlight based on direction
            let (edge_start, edge_end) = match edge_dir {
                Direction::North => ((sx0, sy0), (sx1, sy1)), // NW to NE
                Direction::East  => ((sx1, sy1), (sx2, sy2)), // NE to SE
                Direction::South => ((sx2, sy2), (sx3, sy3)), // SE to SW
                Direction::West  => ((sx3, sy3), (sx0, sy0)), // SW to NW
                Direction::NwSe  => ((sx0, sy0), (sx2, sy2)), // NW to SE diagonal
                Direction::NeSw  => ((sx1, sy1), (sx3, sy3)), // NE to SW diagonal
            };

            // Draw highlighted edge (bright cyan, thick line)
            let edge_color = Color::from_rgba(100, 255, 255, 255);
            draw_line(edge_start.0, edge_start.1, edge_end.0, edge_end.1, 4.0, edge_color);

            // Draw vertex indicators at edge endpoints
            draw_circle(edge_start.0, edge_start.1, 5.0, edge_color);
            draw_circle(edge_end.0, edge_end.1, 5.0, edge_color);
        }
    }

    // Draw portals (view-mode-aware)
    for portal in &room.portals {
        // Portal vertices are room-relative, convert to world space
        let v0 = Vec3::new(
            portal.vertices[0].x + room.position.x,
            portal.vertices[0].y + room.position.y,
            portal.vertices[0].z + room.position.z,
        );
        let v1 = Vec3::new(
            portal.vertices[1].x + room.position.x,
            portal.vertices[1].y + room.position.y,
            portal.vertices[1].z + room.position.z,
        );
        let v2 = Vec3::new(
            portal.vertices[2].x + room.position.x,
            portal.vertices[2].y + room.position.y,
            portal.vertices[2].z + room.position.z,
        );
        let v3 = Vec3::new(
            portal.vertices[3].x + room.position.x,
            portal.vertices[3].y + room.position.y,
            portal.vertices[3].z + room.position.z,
        );

        // Check if this is a horizontal portal (normal pointing up or down)
        let is_horizontal = portal.normal.y.abs() > 0.9;

        // Convert to screen coordinates based on view mode
        let (sx0, sy0) = {
            let (a, b) = world_pos_to_plane(v0.x, v0.y, v0.z);
            world_to_screen(a, b)
        };
        let (sx1, sy1) = {
            let (a, b) = world_pos_to_plane(v1.x, v1.y, v1.z);
            world_to_screen(a, b)
        };
        let (sx2, sy2) = {
            let (a, b) = world_pos_to_plane(v2.x, v2.y, v2.z);
            world_to_screen(a, b)
        };
        let (sx3, sy3) = {
            let (a, b) = world_pos_to_plane(v3.x, v3.y, v3.z);
            world_to_screen(a, b)
        };

        // In top view, horizontal portals appear as filled quads
        // In side views, horizontal portals appear as horizontal lines
        // In top view, vertical portals may appear as lines (edges)
        // In side views, vertical portals appear as filled quads
        let should_fill = match view_mode {
            GridViewMode::Top => is_horizontal,      // Horizontal portals fill, vertical appear as lines
            GridViewMode::Front | GridViewMode::Side => !is_horizontal, // Vertical portals fill
        };

        let fill_color = Color::from_rgba(200, 50, 200, 80);
        let outline_color = Color::from_rgba(255, 100, 255, 255);

        if should_fill {
            // Draw filled quad
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
        }

        // Portal outline (always draw)
        draw_line(sx0, sy0, sx1, sy1, 2.0, outline_color);
        draw_line(sx1, sy1, sx2, sy2, 2.0, outline_color);
        draw_line(sx2, sy2, sx3, sy3, 2.0, outline_color);
        draw_line(sx3, sy3, sx0, sy0, 2.0, outline_color);
    }

    // Draw level objects (spawns, lights, triggers, etc.) for current room and detect hover
    let mut hovered_object: Option<usize> = None;
    for (obj_idx, obj) in room.objects.iter().enumerate() {
        // Calculate world position (center of sector)
        let world_x = room.position.x + (obj.sector_x as f32 + 0.5) * SECTOR_SIZE;
        let world_y = room.position.y + obj.height;
        let world_z = room.position.z + (obj.sector_z as f32 + 0.5) * SECTOR_SIZE;
        // Convert to 2D plane based on view mode
        let (plane_a, plane_b) = world_pos_to_plane(world_x, world_y, world_z);
        let (sx, sy) = world_to_screen(plane_a, plane_b);

        // Check if this object is selected
        let is_selected = matches!(&state.selection, super::Selection::Object { room: r, index } if *r == current_room_idx && *index == obj_idx);

        // Check if mouse is hovering over this object
        let center_radius = if is_selected { 10.0 } else { 7.0 };
        let dist_to_mouse = ((mouse_pos.0 - sx).powi(2) + (mouse_pos.1 - sy).powi(2)).sqrt();
        if inside && dist_to_mouse < center_radius + 4.0 {
            hovered_object = Some(obj_idx);
        }

        // Color based on object type
        let (fill_color, outline_color, icon_char) = match &obj.object_type {
            crate::world::ObjectType::Spawn(crate::world::SpawnPointType::PlayerStart) =>
                (Color::from_rgba(50, 200, 50, 200), Color::from_rgba(100, 255, 100, 255), 'P'),
            crate::world::ObjectType::Spawn(crate::world::SpawnPointType::Checkpoint) =>
                (Color::from_rgba(50, 50, 200, 200), Color::from_rgba(100, 100, 255, 255), 'C'),
            crate::world::ObjectType::Spawn(crate::world::SpawnPointType::Enemy) =>
                (Color::from_rgba(200, 50, 50, 200), Color::from_rgba(255, 100, 100, 255), 'E'),
            crate::world::ObjectType::Spawn(crate::world::SpawnPointType::Item) =>
                (Color::from_rgba(200, 200, 50, 200), Color::from_rgba(255, 255, 100, 255), 'I'),
            crate::world::ObjectType::Light { .. } =>
                (Color::from_rgba(255, 200, 50, 200), Color::from_rgba(255, 255, 150, 255), 'L'),
            crate::world::ObjectType::Prop(_) =>
                (Color::from_rgba(150, 100, 200, 200), Color::from_rgba(200, 150, 255, 255), 'M'),
            crate::world::ObjectType::Trigger { .. } =>
                (Color::from_rgba(200, 100, 50, 200), Color::from_rgba(255, 150, 100, 255), 'T'),
            crate::world::ObjectType::Particle { .. } =>
                (Color::from_rgba(255, 150, 200, 200), Color::from_rgba(255, 200, 230, 255), '*'),
            crate::world::ObjectType::Audio { .. } =>
                (Color::from_rgba(100, 200, 200, 200), Color::from_rgba(150, 255, 255, 255), '~'),
        };

        // Draw object marker
        if obj.enabled {
            draw_circle(sx, sy, center_radius, fill_color);
            draw_circle_lines(sx, sy, center_radius, 1.5, outline_color);

            // Draw facing direction arrow for spawns
            if matches!(obj.object_type, crate::world::ObjectType::Spawn(_)) {
                let arrow_len = center_radius + 6.0;
                let angle = obj.facing;
                // In our coordinate system: facing 0 = +Z = screen down
                let dx = angle.sin() * arrow_len;
                let dy = angle.cos() * arrow_len;
                draw_line(sx, sy, sx + dx, sy + dy, 2.0, outline_color);
                // Arrow head
                let head_len = 4.0;
                let head_angle1 = angle + 2.5;
                let head_angle2 = angle - 2.5;
                draw_line(sx + dx, sy + dy,
                    sx + dx - head_angle1.sin() * head_len,
                    sy + dy - head_angle1.cos() * head_len, 2.0, outline_color);
                draw_line(sx + dx, sy + dy,
                    sx + dx - head_angle2.sin() * head_len,
                    sy + dy - head_angle2.cos() * head_len, 2.0, outline_color);
            }

            // Draw icon letter
            let letter = icon_char.to_string();
            let letter_dims = measure_text(&letter, None, 12, 1.0);
            draw_text(&letter, sx - letter_dims.width / 2.0, sy + 4.0, 12.0, WHITE);
        } else {
            // Disabled objects shown as hollow
            draw_circle_lines(sx, sy, center_radius, 2.0, Color::from_rgba(100, 100, 100, 200));
        }

        // Selection/hover highlight
        if is_selected {
            draw_circle_lines(sx, sy, center_radius + 4.0, 2.0, WHITE);
        } else if hovered_object == Some(obj_idx) {
            draw_circle_lines(sx, sy, center_radius + 4.0, 1.0, Color::from_rgba(255, 255, 200, 180));
        }
    }

    // Draw room center markers for all rooms (handle for moving the room)
    let mut hovered_room_origin: Option<usize> = None;
    for (room_idx, r) in state.level.rooms.iter().enumerate() {
        let is_current = room_idx == current_room_idx;
        let is_hidden = state.hidden_rooms.contains(&room_idx);

        // Skip hidden rooms (but always show current room)
        if is_hidden && !is_current {
            continue;
        }

        // Calculate room center (not origin corner) - depends on view mode
        let center_x = r.position.x + (r.width as f32 * SECTOR_SIZE) / 2.0;
        let center_z = r.position.z + (r.depth as f32 * SECTOR_SIZE) / 2.0;
        let center_y = r.position.y + (r.bounds.max.y + r.bounds.min.y) / 2.0;

        let (ox, oy) = match view_mode {
            GridViewMode::Top => world_to_screen(center_x, center_z),
            GridViewMode::Front => world_to_screen(center_x, center_y),
            GridViewMode::Side => world_to_screen(center_z, center_y),
        };
        if ox >= rect.x - 10.0 && ox <= rect.right() + 10.0 && oy >= rect.y - 10.0 && oy <= rect.bottom() + 10.0 {
            let dist_to_mouse = ((mouse_pos.0 - ox).powi(2) + (mouse_pos.1 - oy).powi(2)).sqrt();
            let is_hovered = inside && dist_to_mouse < 12.0;

            if is_hovered {
                hovered_room_origin = Some(room_idx);
            }

            // Draw center handle (dimmed if hidden)
            let color = if is_hovered {
                Color::from_rgba(255, 255, 150, 255) // Bright yellow when hovered
            } else if is_hidden {
                Color::from_rgba(100, 60, 60, 150) // Very dim for hidden current room
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
        let (offset_a, offset_b) = state.grid_sector_drag_offset;
        if let Some(r) = state.level.rooms.get(current_room_idx) {
            // Ghost at new center position - offset applies to the current view plane
            let center_x = r.position.x + (r.width as f32 * SECTOR_SIZE) / 2.0;
            let center_z = r.position.z + (r.depth as f32 * SECTOR_SIZE) / 2.0;
            let center_y = r.position.y + (r.bounds.max.y + r.bounds.min.y) / 2.0;

            let (ox, oy) = match view_mode {
                GridViewMode::Top => world_to_screen(center_x + offset_a, center_z + offset_b),
                GridViewMode::Front => world_to_screen(center_x + offset_a, center_y + offset_b),
                GridViewMode::Side => world_to_screen(center_z + offset_a, center_y + offset_b),
            };

            // Ghost center crosshair
            draw_circle(ox, oy, 8.0, Color::from_rgba(100, 255, 100, 200));
            draw_line(ox - 14.0, oy, ox + 14.0, oy, 2.0, Color::from_rgba(100, 255, 100, 200));
            draw_line(ox, oy - 14.0, ox, oy + 14.0, 2.0, Color::from_rgba(100, 255, 100, 200));
        }
    }

    // Draw ghost preview when dragging an object
    if let Some((drag_room_idx, obj_idx)) = state.grid_dragging_object {
        if state.grid_sector_drag_start.is_some() {
            let (offset_a, offset_b) = state.grid_sector_drag_offset;
            // Convert plane offset to world offset
            let (world_dx, world_dy, world_dz) = plane_to_world_offset(offset_a, offset_b);
            // Snap to sector grid for horizontal, click height for vertical
            let snapped_dx = (world_dx / SECTOR_SIZE).round() * SECTOR_SIZE;
            let snapped_dz = (world_dz / SECTOR_SIZE).round() * SECTOR_SIZE;
            let snapped_dy = (world_dy / CLICK_HEIGHT).round() * CLICK_HEIGHT;

            if let Some(drag_room) = state.level.rooms.get(drag_room_idx) {
                if let Some(obj) = drag_room.objects.get(obj_idx) {
                    // Get current world position
                    let current_pos = obj.world_position(drag_room);
                    // Calculate ghost position in world space
                    let new_world_x = current_pos.x + snapped_dx;
                    let new_world_y = current_pos.y + snapped_dy;
                    let new_world_z = current_pos.z + snapped_dz;
                    // Convert to screen via plane coordinates
                    let (plane_a, plane_b) = world_pos_to_plane(new_world_x, new_world_y, new_world_z);
                    let (gx, gy) = world_to_screen(plane_a, plane_b);

                    // Draw ghost object (semi-transparent)
                    use crate::world::ObjectType;
                    let (color, letter) = match &obj.object_type {
                        ObjectType::Spawn(spawn_type) => {
                            use crate::world::SpawnPointType;
                            match spawn_type {
                                SpawnPointType::PlayerStart => (Color::from_rgba(100, 255, 100, 150), 'P'),
                                SpawnPointType::Checkpoint => (Color::from_rgba(100, 200, 255, 150), 'C'),
                                SpawnPointType::Enemy => (Color::from_rgba(255, 100, 100, 150), 'E'),
                                SpawnPointType::Item => (Color::from_rgba(255, 200, 100, 150), 'I'),
                            }
                        }
                        ObjectType::Light { .. } => (Color::from_rgba(255, 255, 100, 150), 'L'),
                        ObjectType::Prop { .. } => (Color::from_rgba(180, 130, 255, 150), 'M'),
                        ObjectType::Trigger { .. } => (Color::from_rgba(255, 100, 200, 150), 'T'),
                        ObjectType::Particle { .. } => (Color::from_rgba(255, 180, 100, 150), '*'),
                        ObjectType::Audio { .. } => (Color::from_rgba(100, 200, 255, 150), '~'),
                    };

                    draw_circle(gx, gy, 10.0, color);
                    draw_circle_lines(gx, gy, 13.0, 2.0, Color::from_rgba(255, 255, 255, 200));

                    // Draw letter
                    let text = letter.to_string();
                    let font_size = 14.0;
                    let text_dims = measure_text(&text, None, font_size as u16, 1.0);
                    draw_text(
                        &text,
                        gx - text_dims.width * 0.5,
                        gy + text_dims.height * 0.3,
                        font_size,
                        Color::from_rgba(255, 255, 255, 200),
                    );
                }
            }
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
            let (offset_a, offset_b) = state.grid_sector_drag_offset;

            // Check if we're dragging an object
            if let Some((drag_room_idx, obj_idx)) = state.grid_dragging_object {
                // Convert plane offset to world offset
                let (world_dx, world_dy, world_dz) = plane_to_world_offset(offset_a, offset_b);

                // Snap to sector grid for horizontal movement
                let snapped_dx = (world_dx / SECTOR_SIZE).round() * SECTOR_SIZE;
                let snapped_dz = (world_dz / SECTOR_SIZE).round() * SECTOR_SIZE;
                // Snap to click height for vertical movement
                let snapped_dy = (world_dy / CLICK_HEIGHT).round() * CLICK_HEIGHT;

                // Convert to sector delta
                let sector_dx = (snapped_dx / SECTOR_SIZE).round() as i32;
                let sector_dz = (snapped_dz / SECTOR_SIZE).round() as i32;

                let has_horizontal_movement = sector_dx != 0 || sector_dz != 0;
                let has_vertical_movement = snapped_dy.abs() >= CLICK_HEIGHT * 0.5;

                if has_horizontal_movement || has_vertical_movement {
                    state.save_undo();

                    let mut final_sector_x = 0;
                    let mut final_sector_z = 0;
                    let mut final_height = 0.0;

                    if let Some(obj) = state.level.get_object_mut(drag_room_idx, obj_idx) {
                        // Update sector coordinates (horizontal movement)
                        if has_horizontal_movement {
                            let new_sector_x = (obj.sector_x as i32 + sector_dx).max(0) as usize;
                            let new_sector_z = (obj.sector_z as i32 + sector_dz).max(0) as usize;
                            obj.sector_x = new_sector_x;
                            obj.sector_z = new_sector_z;
                        }

                        // Update height (vertical movement in Front/Side views)
                        if has_vertical_movement {
                            obj.height += snapped_dy;
                        }

                        // Store final values for status message
                        final_sector_x = obj.sector_x;
                        final_sector_z = obj.sector_z;
                        final_height = obj.height;
                    }

                    // Status message (after mutable borrow ends)
                    if has_horizontal_movement && has_vertical_movement {
                        state.set_status(
                            &format!("Moved object to sector ({}, {}) at height {:.0}", final_sector_x, final_sector_z, final_height),
                            2.0
                        );
                    } else if has_horizontal_movement {
                        state.set_status(
                            &format!("Moved object to sector ({}, {})", final_sector_x, final_sector_z),
                            2.0
                        );
                    } else {
                        state.set_status(
                            &format!("Changed object height to {:.0}", final_height),
                            2.0
                        );
                    }
                }

                // Clear drag state
                state.grid_dragging_object = None;
                state.grid_sector_drag_offset = (0.0, 0.0);
                state.grid_sector_drag_start = None;
            }
            // Handle sector/room dragging
            else {
                // offset_a/offset_b are screen-plane offsets
                // Convert to world offsets using view mode
                let (world_dx, world_dy, world_dz) = plane_to_world_offset(offset_a, offset_b);

                // Snap offsets to appropriate grid
                let snapped_dx = (world_dx / SECTOR_SIZE).round() * SECTOR_SIZE;
                let snapped_dy = (world_dy / CLICK_HEIGHT).round() * CLICK_HEIGHT;
                let snapped_dz = (world_dz / SECTOR_SIZE).round() * SECTOR_SIZE;

                // Only apply if there's actual movement (check all axes)
                let has_movement = snapped_dx.abs() >= SECTOR_SIZE * 0.5
                    || snapped_dz.abs() >= SECTOR_SIZE * 0.5
                    || snapped_dy.abs() >= CLICK_HEIGHT * 0.5;

                if has_movement {
                    state.save_undo();

                    if state.grid_dragging_room_origin {
                    // Move entire room position
                    if let Some(room) = state.level.rooms.get_mut(current_room_idx) {
                        room.position.x += snapped_dx;
                        room.position.y += snapped_dy;
                        room.position.z += snapped_dz;
                    }
                    if let Some(room) = state.level.rooms.get(current_room_idx) {
                        state.set_status(&format!("Moved room to ({:.0}, {:.0}, {:.0})", room.position.x, room.position.y, room.position.z), 2.0);
                    }
                    state.mark_portals_dirty();
                } else {
                    // Move selected sectors within the room grid (X-Z only, no Y movement for sectors)
                    let grid_dx = (snapped_dx / SECTOR_SIZE).round() as i32;
                    let grid_dz = (snapped_dz / SECTOR_SIZE).round() as i32;

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

                        room.compact();
                        state.set_status(&format!("Moved {} sector(s)", state.grid_dragging_sectors.len()), 2.0);
                        state.mark_portals_dirty();
                    }
                }
                }

                // Clear drag state (for sectors/rooms)
                state.grid_dragging_sectors.clear();
                state.grid_sector_drag_offset = (0.0, 0.0);
                state.grid_sector_drag_start = None;
                state.grid_dragging_room_origin = false;
            }
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
                        // Save selection state BEFORE any modifications
                        state.save_selection_undo();

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
                    // Check if clicking on an object first
                    if let Some(obj_idx) = hovered_object {
                        // Check if this object is already selected (start drag)
                        let is_already_selected = matches!(&state.selection,
                            Selection::Object { room: r, index } if *r == current_room_idx && *index == obj_idx);

                        if is_already_selected {
                            // Start dragging the object
                            let (wx, wz) = screen_to_world(mouse_pos.0, mouse_pos.1);
                            state.grid_dragging_object = Some((current_room_idx, obj_idx));
                            state.grid_sector_drag_start = Some((wx, wz));
                            state.grid_sector_drag_offset = (0.0, 0.0);
                        } else {
                            // Select the object
                            state.save_selection_undo();
                            state.clear_multi_selection();
                            state.set_selection(Selection::Object { room: current_room_idx, index: obj_idx });
                        }
                    }
                    // Check if clicking on room origin
                    else if let Some(origin_room_idx) = hovered_room_origin {
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
                                // Shift-click always changes something (toggle)
                                state.save_selection_undo();
                                state.toggle_multi_selection(new_selection.clone());
                                state.set_selection(new_selection);
                            } else if state.selection != new_selection || !state.multi_selection.is_empty() {
                                // Only save undo if selection will change
                                state.save_selection_undo();
                                state.clear_multi_selection();
                                state.set_selection(new_selection);
                            }
                        }
                    } else {
                        // Clicked on empty space - start selection rectangle
                        if !shift_down {
                            // Only save undo if something will change
                            if state.selection != Selection::None || !state.multi_selection.is_empty() {
                                state.save_selection_undo();
                                state.set_selection(Selection::None);
                                state.clear_multi_selection();
                            }
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

                    // Calculate grid coords as signed integers
                    let local_x = ((snapped_x - room.position.x) / SECTOR_SIZE).floor() as i32;
                    let local_z = ((snapped_z - room.position.z) / SECTOR_SIZE).floor() as i32;

                    // Check if sector already has a floor (only if in current bounds)
                    let has_floor = if local_x >= 0 && local_z >= 0 {
                        room.get_sector(local_x as usize, local_z as usize)
                            .map(|s| s.floor.is_some())
                            .unwrap_or(false)
                    } else {
                        false
                    };

                    if has_floor {
                        state.set_status("Sector already has a floor", 2.0);
                    } else {
                        state.save_undo();

                        if let Some(room) = state.level.rooms.get_mut(current_room_idx) {
                            // Expand room in negative X direction if needed
                            if local_x < 0 {
                                let shift = (-local_x) as usize;
                                // Shift room position
                                room.position.x -= shift as f32 * SECTOR_SIZE;
                                // Insert empty columns at the beginning
                                let mut new_sectors = Vec::with_capacity(room.width + shift);
                                for _ in 0..shift {
                                    new_sectors.push((0..room.depth).map(|_| None).collect());
                                }
                                new_sectors.append(&mut room.sectors);
                                room.sectors = new_sectors;
                                room.width += shift;
                            }

                            // Expand room in negative Z direction if needed
                            if local_z < 0 {
                                let shift = (-local_z) as usize;
                                // Shift room position
                                room.position.z -= shift as f32 * SECTOR_SIZE;
                                // Insert empty rows at the beginning of each column
                                for col in &mut room.sectors {
                                    let mut new_col = (0..shift).map(|_| None).collect::<Vec<_>>();
                                    new_col.append(col);
                                    *col = new_col;
                                }
                                room.depth += shift;
                            }

                            // Recalculate grid coords after potential shift
                            let gx = ((snapped_x - room.position.x) / SECTOR_SIZE).floor() as usize;
                            let gz = ((snapped_z - room.position.z) / SECTOR_SIZE).floor() as usize;

                            // Expand room grid in positive direction if needed
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

                            // Floor at 0.0 = room-relative base (will render at room.position.y in world space)
                            room.set_floor(gx, gz, 0.0, state.selected_texture.clone());
                            room.recalculate_bounds();
                            state.mark_portals_dirty();
                            state.set_status("Created floor sector", 2.0);
                        }
                    }
                }

                EditorTool::DrawCeiling => {
                    let (wx, wz) = screen_to_world(mouse_pos.0, mouse_pos.1);
                    let snapped_x = (wx / SECTOR_SIZE).floor() * SECTOR_SIZE;
                    let snapped_z = (wz / SECTOR_SIZE).floor() * SECTOR_SIZE;

                    // Calculate grid coords as signed integers
                    let local_x = ((snapped_x - room.position.x) / SECTOR_SIZE).floor() as i32;
                    let local_z = ((snapped_z - room.position.z) / SECTOR_SIZE).floor() as i32;

                    // Check if sector already has a ceiling (only if in current bounds)
                    let has_ceiling = if local_x >= 0 && local_z >= 0 {
                        room.get_sector(local_x as usize, local_z as usize)
                            .map(|s| s.ceiling.is_some())
                            .unwrap_or(false)
                    } else {
                        false
                    };

                    if has_ceiling {
                        state.set_status("Sector already has a ceiling", 2.0);
                    } else {
                        state.save_undo();

                        if let Some(room) = state.level.rooms.get_mut(current_room_idx) {
                            // Expand room in negative X direction if needed
                            if local_x < 0 {
                                let shift = (-local_x) as usize;
                                room.position.x -= shift as f32 * SECTOR_SIZE;
                                let mut new_sectors = Vec::with_capacity(room.width + shift);
                                for _ in 0..shift {
                                    new_sectors.push((0..room.depth).map(|_| None).collect());
                                }
                                new_sectors.append(&mut room.sectors);
                                room.sectors = new_sectors;
                                room.width += shift;
                            }

                            // Expand room in negative Z direction if needed
                            if local_z < 0 {
                                let shift = (-local_z) as usize;
                                room.position.z -= shift as f32 * SECTOR_SIZE;
                                for col in &mut room.sectors {
                                    let mut new_col = (0..shift).map(|_| None).collect::<Vec<_>>();
                                    new_col.append(col);
                                    *col = new_col;
                                }
                                room.depth += shift;
                            }

                            // Recalculate grid coords after potential shift
                            let gx = ((snapped_x - room.position.x) / SECTOR_SIZE).floor() as usize;
                            let gz = ((snapped_z - room.position.z) / SECTOR_SIZE).floor() as usize;

                            // Expand room grid in positive direction if needed
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

                            // Ceiling at CEILING_HEIGHT = room-relative (will render at room.position.y + CEILING_HEIGHT)
                            room.set_ceiling(gx, gz, CEILING_HEIGHT, state.selected_texture.clone());
                            room.recalculate_bounds();
                            state.mark_portals_dirty();
                            state.set_status("Created ceiling sector", 2.0);
                        }
                    }
                }

                EditorTool::DrawWall => {
                    // Diagonal walls must be placed in 3D viewport
                    if state.wall_direction.is_diagonal() {
                        state.set_status("Diagonal walls: use 3D viewport (R to change direction)", 2.0);
                    } else if view_mode != GridViewMode::Top {
                        // Wall placement only works in Top view mode
                        state.set_status("Wall tool: switch to Top view", 2.0);
                    } else if let (Some((gx, gz)), Some(edge_dir)) = (hovered_sector, hovered_edge) {
                        // Check if wall already exists on this edge
                        let has_wall = state.level.rooms.get(current_room_idx).map(|r| {
                            r.get_sector(gx, gz).map(|s| {
                                match edge_dir {
                                    Direction::North => !s.walls_north.is_empty(),
                                    Direction::East => !s.walls_east.is_empty(),
                                    Direction::South => !s.walls_south.is_empty(),
                                    Direction::West => !s.walls_west.is_empty(),
                                    Direction::NwSe | Direction::NeSw => false, // Can't detect in 2D
                                }
                            }).unwrap_or(false)
                        }).unwrap_or(false);

                        if has_wall {
                            state.set_status("Wall already exists on this edge", 1.5);
                        } else {
                            state.save_undo();
                            if let Some(room) = state.level.rooms.get_mut(current_room_idx) {
                                room.add_wall(gx, gz, edge_dir, 0.0, CEILING_HEIGHT, state.selected_texture.clone());
                                room.recalculate_bounds();
                                state.mark_portals_dirty();
                                state.set_status(&format!("Created {} wall", edge_dir.name().to_lowercase()), 1.5);
                            }
                        }
                    } else {
                        state.set_status("Hover over a sector edge to place wall", 2.0);
                    }
                }

                EditorTool::PlaceObject => {
                    let (wx, wz) = screen_to_world(mouse_pos.0, mouse_pos.1);
                    let snapped_x = (wx / SECTOR_SIZE).floor() * SECTOR_SIZE;
                    let snapped_z = (wz / SECTOR_SIZE).floor() * SECTOR_SIZE;

                    // Gather sector data first (immutable borrow)
                    let sector_data = state.level.rooms.get(current_room_idx).and_then(|room| {
                        let local_x = snapped_x - room.position.x;
                        let local_z = snapped_z - room.position.z;
                        let gx = (local_x / SECTOR_SIZE).floor() as i32;
                        let gz = (local_z / SECTOR_SIZE).floor() as i32;

                        if gx >= 0 && gz >= 0 {
                            let gx = gx as usize;
                            let gz = gz as usize;
                            // Just check if sector exists
                            if room.get_sector(gx, gz).is_some() {
                                Some((gx, gz))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    });

                    if let Some((gx, gz)) = sector_data {
                        // Check if we can place this object type
                        let can_place = state.level.can_add_object(
                            current_room_idx, gx, gz, &state.selected_object_type
                        );

                        match can_place {
                            Ok(()) => {
                                state.save_undo();
                                let new_object = crate::world::LevelObject::new(
                                    gx, gz,
                                    state.selected_object_type.clone()
                                );
                                let obj_name = state.selected_object_type.display_name();
                                if let Ok(idx) = state.level.add_object(current_room_idx, new_object) {
                                    state.set_selection(super::Selection::Object { room: current_room_idx, index: idx });
                                    state.set_status(&format!("{} placed", obj_name), 1.0);
                                }
                            }
                            Err(msg) => {
                                state.set_status(msg, 2.0);
                            }
                        }
                    } else {
                        state.set_status("Click on a sector to place object", 2.0);
                    }
                }
            }
        }
    }

    // Handle Delete/Backspace key for deletion in 2D view (objects and sectors)
    if inside && (is_key_pressed(KeyCode::Delete) || is_key_pressed(KeyCode::Backspace)) {
        // Collect all selections (primary + multi)
        let mut all_selections: Vec<Selection> = vec![state.selection.clone()];
        all_selections.extend(state.multi_selection.clone());

        // Check for object selections first
        let object_selections: Vec<_> = all_selections.iter()
            .filter_map(|s| match s {
                Selection::Object { room, index } => Some((*room, *index)),
                _ => None,
            })
            .collect();

        if !object_selections.is_empty() {
            state.save_undo();
            // Delete objects in reverse order to preserve indices
            let mut sorted_objects = object_selections;
            sorted_objects.sort_by(|a, b| b.1.cmp(&a.1));

            let mut deleted_count = 0;
            for (room_idx, obj_idx) in sorted_objects {
                if state.level.remove_object(room_idx, obj_idx).is_some() {
                    deleted_count += 1;
                }
            }

            if deleted_count > 0 {
                state.set_selection(Selection::None);
                state.clear_multi_selection();
                let msg = if deleted_count == 1 { "Deleted 1 object".to_string() } else { format!("Deleted {} objects", deleted_count) };
                state.set_status(&msg, 2.0);
            }
        } else {
            // Check for sector selections (from 2D drag select)
            let sector_selections: Vec<_> = all_selections.iter()
                .filter_map(|s| match s {
                    Selection::Sector { room, x, z } => Some((*room, *x, *z)),
                    _ => None,
                })
                .collect();

            if !sector_selections.is_empty() {
                state.save_undo();
                let mut deleted_count = 0;
                let mut affected_rooms = std::collections::HashSet::new();

                for (room_idx, gx, gz) in &sector_selections {
                    if let Some(room) = state.level.rooms.get_mut(*room_idx) {
                        if let Some(sector) = room.get_sector_mut(*gx, *gz) {
                            // Clear all geometry in sector
                            let had_geometry = sector.floor.is_some()
                                || sector.ceiling.is_some()
                                || !sector.walls_north.is_empty()
                                || !sector.walls_east.is_empty()
                                || !sector.walls_south.is_empty()
                                || !sector.walls_west.is_empty()
                                || !sector.walls_nwse.is_empty()
                                || !sector.walls_nesw.is_empty();

                            if had_geometry {
                                sector.floor = None;
                                sector.ceiling = None;
                                sector.walls_north.clear();
                                sector.walls_east.clear();
                                sector.walls_south.clear();
                                sector.walls_west.clear();
                                sector.walls_nwse.clear();
                                sector.walls_nesw.clear();
                                deleted_count += 1;
                                affected_rooms.insert(*room_idx);
                            }
                        }
                    }
                }

                // Cleanup affected rooms
                for room_idx in affected_rooms {
                    if let Some(room) = state.level.rooms.get_mut(room_idx) {
                        room.compact();
                    }
                }

                if deleted_count > 0 {
                    state.set_selection(Selection::None);
                    state.clear_multi_selection();
                    state.mark_portals_dirty();
                    let msg = if deleted_count == 1 { "Deleted 1 sector".to_string() } else { format!("Deleted {} sectors", deleted_count) };
                    state.set_status(&msg, 2.0);
                }
            }
        }
    }

    // Tool shortcuts: 1=Select, 2=Floor, 3=Wall, 4=Ceiling, 5=Object
    if inside {
        if is_key_pressed(KeyCode::Key1) {
            state.tool = EditorTool::Select;
        } else if is_key_pressed(KeyCode::Key2) {
            state.tool = EditorTool::DrawFloor;
        } else if is_key_pressed(KeyCode::Key3) {
            state.tool = EditorTool::DrawWall;
        } else if is_key_pressed(KeyCode::Key4) {
            state.tool = EditorTool::DrawCeiling;
        } else if is_key_pressed(KeyCode::Key5) {
            state.tool = EditorTool::PlaceObject;
        }
    }

    // Disable scissor rectangle
    unsafe {
        get_internal_gl().quad_gl.scissor(None);
    }
}
