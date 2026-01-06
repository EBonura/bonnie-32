//! 3D Viewport - Software rendered preview
//!
//! Sector-based geometry system - selection works on faces within sectors.
//!
//! This module is split into logical sections:
//! - Camera input handling
//! - Hover detection (vertex > edge > face priority)
//! - Click/drag handling for selection and placement
//! - Grid and preview drawing
//! - Geometry rendering
//! - Overlay drawing (selection highlights, etc.)

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext};
use crate::rasterizer::{
    Framebuffer, Texture as RasterTexture, render_mesh, render_mesh_15, Color as RasterColor, Vec3,
    WIDTH, HEIGHT, WIDTH_HI, HEIGHT_HI,
    world_to_screen, world_to_screen_with_depth,
    point_to_segment_distance, point_in_triangle_2d,
    Light, RasterSettings,
};
use crate::world::{SECTOR_SIZE, SplitDirection};
use crate::input::{InputState, Action};
use super::{EditorState, EditorTool, Selection, SectorFace, CameraMode, CEILING_HEIGHT};

/// Calculate distance from point (px, py) to line segment from (x1, y1) to (x2, y2)
fn point_to_line_dist(px: f32, py: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 0.0001 {
        // Degenerate line (point)
        return ((px - x1).powi(2) + (py - y1).powi(2)).sqrt();
    }
    // Project point onto line, clamped to segment
    let t = ((px - x1) * dx + (py - y1) * dy) / len_sq;
    let t = t.clamp(0.0, 1.0);
    let proj_x = x1 + t * dx;
    let proj_y = y1 + t * dy;
    ((px - proj_x).powi(2) + (py - proj_y).powi(2)).sqrt()
}

/// Check if a SectorFace is a wall type (cardinal or diagonal)
fn is_wall_face(face: &SectorFace) -> bool {
    matches!(face,
        SectorFace::WallNorth(_) | SectorFace::WallEast(_) |
        SectorFace::WallSouth(_) | SectorFace::WallWest(_) |
        SectorFace::WallNwSe(_) | SectorFace::WallNeSw(_)
    )
}

/// Get the two grid corner endpoints for a wall face
/// Returns ((x1, z1), (x2, z2)) in grid coordinates (fractional, where corners are at 0 and 1 within sector)
fn get_wall_endpoints(gx: usize, gz: usize, face: &SectorFace) -> ((i32, i32), (i32, i32)) {
    // Grid corners for sector (gx, gz):
    // NW = (gx, gz), NE = (gx+1, gz), SE = (gx+1, gz+1), SW = (gx, gz+1)
    let gx = gx as i32;
    let gz = gz as i32;
    match face {
        SectorFace::WallNorth(_) => ((gx, gz), (gx + 1, gz)),       // NW to NE
        SectorFace::WallEast(_) => ((gx + 1, gz), (gx + 1, gz + 1)), // NE to SE
        SectorFace::WallSouth(_) => ((gx + 1, gz + 1), (gx, gz + 1)), // SE to SW
        SectorFace::WallWest(_) => ((gx, gz + 1), (gx, gz)),         // SW to NW
        SectorFace::WallNwSe(_) => ((gx, gz), (gx + 1, gz + 1)),     // NW to SE
        SectorFace::WallNeSw(_) => ((gx + 1, gz), (gx, gz + 1)),     // NE to SW
        _ => ((0, 0), (0, 0)), // Not a wall
    }
}

/// Find a connected path of walls/diagonals between two wall faces using BFS
/// Returns the path including start and end, or None if not connected
fn find_wall_path(
    room: &crate::world::Room,
    start_x: usize, start_z: usize, start_face: &SectorFace,
    end_x: usize, end_z: usize, end_face: &SectorFace
) -> Option<Vec<(usize, usize, SectorFace)>> {
    use std::collections::{VecDeque, HashSet, HashMap};

    // Get all walls in the room as (x, z, face, endpoints)
    let mut all_walls: Vec<(usize, usize, SectorFace, ((i32, i32), (i32, i32)))> = Vec::new();

    for gz in 0..room.depth {
        for gx in 0..room.width {
            if let Some(sector) = room.get_sector(gx, gz) {
                // Cardinal walls
                for (i, _) in sector.walls_north.iter().enumerate() {
                    let face = SectorFace::WallNorth(i);
                    all_walls.push((gx, gz, face, get_wall_endpoints(gx, gz, &face)));
                }
                for (i, _) in sector.walls_east.iter().enumerate() {
                    let face = SectorFace::WallEast(i);
                    all_walls.push((gx, gz, face, get_wall_endpoints(gx, gz, &face)));
                }
                for (i, _) in sector.walls_south.iter().enumerate() {
                    let face = SectorFace::WallSouth(i);
                    all_walls.push((gx, gz, face, get_wall_endpoints(gx, gz, &face)));
                }
                for (i, _) in sector.walls_west.iter().enumerate() {
                    let face = SectorFace::WallWest(i);
                    all_walls.push((gx, gz, face, get_wall_endpoints(gx, gz, &face)));
                }
                // Diagonal walls
                for (i, _) in sector.walls_nwse.iter().enumerate() {
                    let face = SectorFace::WallNwSe(i);
                    all_walls.push((gx, gz, face, get_wall_endpoints(gx, gz, &face)));
                }
                for (i, _) in sector.walls_nesw.iter().enumerate() {
                    let face = SectorFace::WallNeSw(i);
                    all_walls.push((gx, gz, face, get_wall_endpoints(gx, gz, &face)));
                }
            }
        }
    }

    // Find indices of start and end walls
    let start_idx = all_walls.iter().position(|(x, z, f, _)| *x == start_x && *z == start_z && *f == *start_face)?;
    let end_idx = all_walls.iter().position(|(x, z, f, _)| *x == end_x && *z == end_z && *f == *end_face)?;

    if start_idx == end_idx {
        // Same wall
        return Some(vec![(start_x, start_z, *start_face)]);
    }

    // Check if two walls share an endpoint (are connected)
    let walls_connected = |a: &((i32, i32), (i32, i32)), b: &((i32, i32), (i32, i32))| -> bool {
        a.0 == b.0 || a.0 == b.1 || a.1 == b.0 || a.1 == b.1
    };

    // BFS to find shortest path
    let mut visited: HashSet<usize> = HashSet::new();
    let mut parent: HashMap<usize, usize> = HashMap::new();
    let mut queue: VecDeque<usize> = VecDeque::new();

    visited.insert(start_idx);
    queue.push_back(start_idx);

    while let Some(current) = queue.pop_front() {
        if current == end_idx {
            // Found path, reconstruct it
            let mut path = Vec::new();
            let mut node = end_idx;
            while node != start_idx {
                let (x, z, f, _) = &all_walls[node];
                path.push((*x, *z, *f));
                node = *parent.get(&node).unwrap();
            }
            let (x, z, f, _) = &all_walls[start_idx];
            path.push((*x, *z, *f));
            path.reverse();
            return Some(path);
        }

        let current_endpoints = &all_walls[current].3;

        // Find all connected walls
        for (i, (_, _, _, endpoints)) in all_walls.iter().enumerate() {
            if !visited.contains(&i) && walls_connected(current_endpoints, endpoints) {
                visited.insert(i);
                parent.insert(i, current);
                queue.push_back(i);
            }
        }
    }

    // No path found
    None
}

/// Draw the 3D viewport using the software rasterizer
pub fn draw_viewport_3d(
    ctx: &mut UiContext,
    rect: Rect,
    state: &mut EditorState,
    textures: &[RasterTexture],
    fb: &mut Framebuffer,
    input: &InputState,
    icon_font: Option<&Font>,
) {
    // Resize framebuffer based on resolution setting
    let (target_w, target_h) = if state.raster_settings.stretch_to_fill {
        // Keep horizontal resolution fixed, scale vertical to match viewport aspect ratio
        let base_w = if state.raster_settings.low_resolution { WIDTH } else { WIDTH_HI };
        let viewport_aspect = rect.h / rect.w;
        let scaled_h = (base_w as f32 * viewport_aspect) as usize;
        (base_w, scaled_h.max(1))
    } else if state.raster_settings.low_resolution {
        (WIDTH, HEIGHT)
    } else {
        (WIDTH_HI, HEIGHT_HI)
    };
    fb.resize(target_w, target_h);

    let mouse_pos = (ctx.mouse.x, ctx.mouse.y);
    let inside_viewport = ctx.mouse.inside(&rect);

    // Pre-calculate viewport scaling (used multiple times)
    let fb_width = fb.width;
    let fb_height = fb.height;
    let (draw_w, draw_h, draw_x, draw_y) = if state.raster_settings.stretch_to_fill {
        // Framebuffer matches viewport, no scaling needed
        (rect.w, rect.h, rect.x, rect.y)
    } else {
        // Maintain aspect ratio (4:3 for PS1)
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

    // Camera controls - depend on camera mode
    let should_update_orbit_target = handle_camera_input(ctx, state, inside_viewport, mouse_pos, input);

    // Toggle link coincident vertices mode with L key
    if inside_viewport && is_key_pressed(KeyCode::L) {
        state.link_coincident_vertices = !state.link_coincident_vertices;
        let mode = if state.link_coincident_vertices { "Linked" } else { "Independent" };
        state.set_status(&format!("Vertex mode: {}", mode), 2.0);
    }

    // Clear selection with Escape key
    if inside_viewport && is_key_pressed(KeyCode::Escape) && (state.selection != Selection::None || !state.multi_selection.is_empty()) {
        state.save_selection_undo();
        state.set_selection(Selection::None);
        state.clear_multi_selection();
        state.set_status("Selection cleared", 0.5);
    }

    // Delete selected elements with Delete or Backspace key (supports multi-selection)
    if inside_viewport && (is_key_pressed(KeyCode::Delete) || is_key_pressed(KeyCode::Backspace)) {
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
            sorted_objects.sort_by(|a, b| b.1.cmp(&a.1)); // Sort by index descending

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
            // Filter to only SectorFace selections
            let face_selections: Vec<_> = all_selections.into_iter()
                .filter_map(|s| match s {
                    Selection::SectorFace { room, x, z, face } => Some((room, x, z, face)),
                    _ => None,
                })
                .collect();

            if !face_selections.is_empty() {
                state.save_undo();
                let mut deleted_count = 0;
                let mut affected_rooms = std::collections::HashSet::new();

                for (room_idx, gx, gz, face) in face_selections {
                    let deleted = delete_face(&mut state.level, room_idx, gx, gz, face);
                    if deleted {
                        deleted_count += 1;
                        affected_rooms.insert(room_idx);
                    }
                }

                // Cleanup affected rooms
                for room_idx in affected_rooms {
                    if let Some(room) = state.level.rooms.get_mut(room_idx) {
                        room.cleanup_empty_sectors();
                        room.trim_empty_edges();
                        room.recalculate_bounds();
                    }
                }

                if deleted_count > 0 {
                    state.set_selection(Selection::None);
                    state.clear_multi_selection();
                    state.mark_portals_dirty();
                    let msg = if deleted_count == 1 { "Deleted 1 face".to_string() } else { format!("Deleted {} faces", deleted_count) };
                    state.set_status(&msg, 2.0);
                }
            }
        }
    }

    // Collect all vertex positions for the current room (for drawing and selection)
    let all_vertices = collect_room_vertices(state);

    // Find hovered elements using 2D screen-space projection
    // Priority: vertex > edge > face
    let mut preview_sector: Option<(f32, f32, f32, bool)> = None; // (x, z, target_y, is_occupied)
    // Wall preview: (x, z, direction, corner_heights [bl, br, tr, tl], state, mouse_y_room_relative)
    // state: 0 = new wall, 1 = filling gap, 2 = fully covered
    let mut preview_wall: Option<(f32, f32, crate::world::Direction, [f32; 4], u8, Option<f32>)> = None;
    // Diagonal wall preview: (x, z, is_nwse, corner_heights [corner1_bot, corner2_bot, corner2_top, corner1_top])
    let mut preview_diagonal_wall: Option<(f32, f32, bool, [f32; 4])> = None;

    let hover = if inside_viewport && !ctx.mouse.right_down &&
        (state.tool == EditorTool::Select || state.tool == EditorTool::PlaceObject) {
        screen_to_fb(mouse_pos.0, mouse_pos.1)
            .map(|mouse_fb| find_hovered_elements(state, mouse_fb, fb_width, fb_height, &all_vertices))
            .unwrap_or_default()
    } else {
        HoverResult::default()
    };

    let hovered_vertex = hover.vertex;
    let hovered_edge = hover.edge;
    let hovered_face = hover.face;
    let hovered_object = hover.object;

    // In drawing modes, find preview sector position
    if inside_viewport && (state.tool == EditorTool::DrawFloor || state.tool == EditorTool::DrawCeiling) {
        if let Some((mouse_fb_x, mouse_fb_y)) = screen_to_fb(mouse_pos.0, mouse_pos.1) {
            use super::{CEILING_HEIGHT, CLICK_HEIGHT};

            let is_floor = state.tool == EditorTool::DrawFloor;

            // For sector detection, always use floor level (0.0) so clicking on the floor
            // selects the sector where you want to place geometry.
            // This is more intuitive - you click on the floor to place a ceiling above it.
            let detection_y = 0.0;

            // Find closest sector to mouse cursor (only when not in height adjust mode)
            let (snapped_x, snapped_z) = if let Some((locked_x, locked_z)) = state.height_adjust_locked_pos {
                // Use locked position when in height adjust mode
                (locked_x, locked_z)
            } else {
                // Find closest grid position to mouse
                let search_radius = 20;
                let cam_x = state.camera_3d.position.x;
                let cam_z = state.camera_3d.position.z;
                let start_x = ((cam_x / SECTOR_SIZE).floor() as i32 - search_radius) as f32 * SECTOR_SIZE;
                let start_z = ((cam_z / SECTOR_SIZE).floor() as i32 - search_radius) as f32 * SECTOR_SIZE;

                let mut closest: Option<(f32, f32, f32)> = None;
                for ix in 0..(search_radius * 2) {
                    for iz in 0..(search_radius * 2) {
                        let grid_x = start_x + (ix as f32 * SECTOR_SIZE);
                        let grid_z = start_z + (iz as f32 * SECTOR_SIZE);
                        let test_pos = Vec3::new(grid_x + SECTOR_SIZE / 2.0, detection_y, grid_z + SECTOR_SIZE / 2.0);

                        if let Some((sx, sy)) = world_to_screen(test_pos, state.camera_3d.position,
                            state.camera_3d.basis_x, state.camera_3d.basis_y, state.camera_3d.basis_z,
                            fb.width, fb.height)
                        {
                            let dist = ((mouse_fb_x - sx).powi(2) + (mouse_fb_y - sy).powi(2)).sqrt();
                            if closest.map_or(true, |(_, _, best_dist)| dist < best_dist) {
                                closest = Some((grid_x, grid_z, dist));
                            }
                        }
                    }
                }

                if let Some((x, z, dist)) = closest {
                    if dist < 100.0 {
                        (x, z)
                    } else {
                        // Too far from any grid position
                        (f32::NAN, f32::NAN)
                    }
                } else {
                    (f32::NAN, f32::NAN)
                }
            };

            // Handle Shift+drag for height adjustment
            let shift_down = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);

            if shift_down && !state.height_adjust_mode && !snapped_x.is_nan() {
                // Just started holding shift - enter height adjust mode and lock position
                state.height_adjust_mode = true;
                state.height_adjust_start_mouse_y = mouse_pos.1;
                state.height_adjust_start_y = state.placement_target_y;
                state.height_adjust_locked_pos = Some((snapped_x, snapped_z));
            } else if !shift_down && state.height_adjust_mode {
                // Released shift - exit height adjust mode and unlock position
                state.height_adjust_mode = false;
                state.height_adjust_locked_pos = None;
            }

            // Adjust height while shift is held
            if state.height_adjust_mode {
                let mouse_delta = state.height_adjust_start_mouse_y - mouse_pos.1;
                let y_sensitivity = 5.0;
                let raw_delta = mouse_delta * y_sensitivity;
                // Snap to CLICK_HEIGHT increments
                let snapped_delta = (raw_delta / CLICK_HEIGHT).round() * CLICK_HEIGHT;
                state.placement_target_y = state.height_adjust_start_y + snapped_delta;
                // Show height in status bar
                let clicks = (state.placement_target_y / CLICK_HEIGHT) as i32;
                state.set_status(&format!("Height: {:.0} ({} clicks)", state.placement_target_y, clicks), 0.5);
            }

            // Set preview sector if we have a valid position
            if !snapped_x.is_nan() {
                // Check if sector is occupied using new sector API
                let occupied = if let Some(room) = state.level.rooms.get(state.current_room) {
                    // Convert world coords to grid coords
                    if let Some((gx, gz)) = room.world_to_grid(snapped_x + SECTOR_SIZE * 0.5, snapped_z + SECTOR_SIZE * 0.5) {
                        if let Some(sector) = room.get_sector(gx, gz) {
                            if is_floor { sector.floor.is_some() } else { sector.ceiling.is_some() }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

                // Use current target_y for preview (may have been updated by height adjust)
                let preview_y = state.placement_target_y;
                let final_y = if preview_y == 0.0 && !state.height_adjust_mode {
                    if is_floor { 0.0 } else { CEILING_HEIGHT }
                } else {
                    preview_y
                };

                preview_sector = Some((snapped_x, snapped_z, final_y, occupied));
            }
        }
    }

    // In DrawWall mode, find preview wall edge
    if inside_viewport && state.tool == EditorTool::DrawWall {
        if let Some((mouse_fb_x, mouse_fb_y)) = screen_to_fb(mouse_pos.0, mouse_pos.1) {
            use super::CEILING_HEIGHT;
            use crate::world::Direction;

            // Find the closest sector edge to the mouse cursor
            let search_radius = 20;
            let cam_x = state.camera_3d.position.x;
            let cam_z = state.camera_3d.position.z;
            let start_x = ((cam_x / SECTOR_SIZE).floor() as i32 - search_radius) as f32 * SECTOR_SIZE;
            let start_z = ((cam_z / SECTOR_SIZE).floor() as i32 - search_radius) as f32 * SECTOR_SIZE;

            // Default wall height (floor to ceiling or 0 to CEILING_HEIGHT)
            let (default_y_bottom, default_y_top) = (0.0, CEILING_HEIGHT);

            // Find closest edge - each sector has 4 edges
            // We collect candidates and prioritize edges with existing walls
            let mut edge_candidates: Vec<(f32, f32, Direction, f32, bool)> = Vec::new(); // (grid_x, grid_z, direction, screen_dist, has_walls)

            for ix in 0..(search_radius * 2) {
                for iz in 0..(search_radius * 2) {
                    let grid_x = start_x + (ix as f32 * SECTOR_SIZE);
                    let grid_z = start_z + (iz as f32 * SECTOR_SIZE);

                    // Mid-height for edge center detection
                    let mid_y = (default_y_bottom + default_y_top) / 2.0;

                    // Check all 4 edges of this sector
                    let edges = [
                        // North edge (-Z): from NW to NE corner
                        (Direction::North, Vec3::new(grid_x + SECTOR_SIZE / 2.0, mid_y, grid_z)),
                        // East edge (+X): from NE to SE corner
                        (Direction::East, Vec3::new(grid_x + SECTOR_SIZE, mid_y, grid_z + SECTOR_SIZE / 2.0)),
                        // South edge (+Z): from SE to SW corner
                        (Direction::South, Vec3::new(grid_x + SECTOR_SIZE / 2.0, mid_y, grid_z + SECTOR_SIZE)),
                        // West edge (-X): from SW to NW corner
                        (Direction::West, Vec3::new(grid_x, mid_y, grid_z + SECTOR_SIZE / 2.0)),
                    ];

                    for (edge_dir, center) in edges {
                        if let Some((sx, sy)) = world_to_screen(center, state.camera_3d.position,
                            state.camera_3d.basis_x, state.camera_3d.basis_y, state.camera_3d.basis_z,
                            fb.width, fb.height)
                        {
                            let dist = ((mouse_fb_x - sx).powi(2) + (mouse_fb_y - sy).powi(2)).sqrt();
                            // Only consider edges within a reasonable distance
                            if dist < 120.0 {
                                // Walls face inward based on direction:
                                // - North wall (at z=grid_z) faces +Z
                                // - South wall (at z=grid_z+SECTOR_SIZE) faces -Z
                                // - East wall (at x=grid_x+SECTOR_SIZE) faces -X
                                // - West wall (at x=grid_x) faces +X
                                //
                                // To make wall face camera, we may need to place on adjacent sector
                                let cam = state.camera_3d.position;
                                let (final_grid_x, final_grid_z, dir) = match edge_dir {
                                    Direction::North => {
                                        // Edge at z=grid_z, wall faces +Z (south)
                                        // If camera is north of edge (cam.z < center.z), place as South wall
                                        // on the sector to the north (grid_z - SECTOR_SIZE)
                                        if cam.z < center.z {
                                            (grid_x, grid_z - SECTOR_SIZE, Direction::South)
                                        } else {
                                            (grid_x, grid_z, Direction::North)
                                        }
                                    }
                                    Direction::South => {
                                        // Edge at z=grid_z+SECTOR_SIZE, wall faces -Z (north)
                                        // If camera is south of edge (cam.z > center.z), place as North wall
                                        // on the sector to the south (grid_z + SECTOR_SIZE)
                                        if cam.z > center.z {
                                            (grid_x, grid_z + SECTOR_SIZE, Direction::North)
                                        } else {
                                            (grid_x, grid_z, Direction::South)
                                        }
                                    }
                                    Direction::East => {
                                        // Edge at x=grid_x+SECTOR_SIZE, wall faces -X (west)
                                        // If camera is east of edge (cam.x > center.x), place as West wall
                                        // on the sector to the east (grid_x + SECTOR_SIZE)
                                        if cam.x > center.x {
                                            (grid_x + SECTOR_SIZE, grid_z, Direction::West)
                                        } else {
                                            (grid_x, grid_z, Direction::East)
                                        }
                                    }
                                    Direction::West => {
                                        // Edge at x=grid_x, wall faces +X (east)
                                        // If camera is west of edge (cam.x < center.x), place as East wall
                                        // on the sector to the west (grid_x - SECTOR_SIZE)
                                        if cam.x < center.x {
                                            (grid_x - SECTOR_SIZE, grid_z, Direction::East)
                                        } else {
                                            (grid_x, grid_z, Direction::West)
                                        }
                                    }
                                };

                                // Check if this edge has existing walls
                                let has_walls = if let Some(room) = state.level.rooms.get(state.current_room) {
                                    if let Some((gx, gz)) = room.world_to_grid(final_grid_x + SECTOR_SIZE * 0.5, final_grid_z + SECTOR_SIZE * 0.5) {
                                        if let Some(sector) = room.get_sector(gx, gz) {
                                            !sector.walls(dir).is_empty()
                                        } else {
                                            false
                                        }
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                };

                                edge_candidates.push((final_grid_x, final_grid_z, dir, dist, has_walls));
                            }
                        }
                    }
                }
            }

            // Select edge: prioritize edges with existing walls, but only if reasonably close
            let closest_edge = {
                // Find closest edge overall
                let closest_any = edge_candidates.iter()
                    .min_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal));

                // Find closest edge with walls
                let closest_wall = edge_candidates.iter()
                    .filter(|(_, _, _, _, has_walls)| *has_walls)
                    .min_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal));

                // Prioritize wall edge only if:
                // 1. It exists and is within 60px (reasonable hovering distance)
                // 2. AND the closest non-wall edge isn't dramatically closer (within 30px difference)
                match (closest_wall, closest_any) {
                    (Some(&(wgx, wgz, wdir, wdist, _)), Some(&(_, _, _, any_dist, _))) => {
                        // If wall edge is close enough AND not much farther than closest edge
                        if wdist < 60.0 && (wdist - any_dist) < 30.0 {
                            Some((wgx, wgz, wdir, wdist))
                        } else {
                            closest_any.map(|&(gx, gz, dir, dist, _)| (gx, gz, dir, dist))
                        }
                    }
                    (None, Some(&(gx, gz, dir, dist, _))) => Some((gx, gz, dir, dist)),
                    _ => None,
                }
            };

            if let Some((grid_x, grid_z, dir, dist)) = closest_edge {
                if dist < 80.0 {
                    // Estimate mouse world Y by projecting floor and ceiling points and interpolating
                    // Use the wall edge center position for X/Z
                    let edge_x = match dir {
                        Direction::North | Direction::South => grid_x + SECTOR_SIZE / 2.0,
                        Direction::East => grid_x + SECTOR_SIZE,
                        Direction::West => grid_x,
                    };
                    let edge_z = match dir {
                        Direction::North => grid_z,
                        Direction::South => grid_z + SECTOR_SIZE,
                        Direction::East | Direction::West => grid_z + SECTOR_SIZE / 2.0,
                    };

                    // Project floor and ceiling Y at this edge to screen
                    let room_y = state.level.rooms.get(state.current_room)
                        .map(|r| r.position.y)
                        .unwrap_or(0.0);
                    let floor_world = Vec3::new(edge_x, room_y + default_y_bottom, edge_z);
                    let ceiling_world = Vec3::new(edge_x, room_y + default_y_top, edge_z);

                    let mouse_y_room_relative = if let (Some((_, floor_sy)), Some((_, ceiling_sy))) = (
                        world_to_screen(floor_world, state.camera_3d.position,
                            state.camera_3d.basis_x, state.camera_3d.basis_y, state.camera_3d.basis_z,
                            fb.width, fb.height),
                        world_to_screen(ceiling_world, state.camera_3d.position,
                            state.camera_3d.basis_x, state.camera_3d.basis_y, state.camera_3d.basis_z,
                            fb.width, fb.height),
                    ) {
                        // Interpolate: screen Y goes from ceiling (top) to floor (bottom)
                        // ceiling_sy < floor_sy in screen space (Y increases downward)
                        if (floor_sy - ceiling_sy).abs() > 1.0 {
                            let t = (mouse_fb_y - ceiling_sy) / (floor_sy - ceiling_sy);
                            let t_clamped = t.clamp(0.0, 1.0);
                            // Interpolate from ceiling to floor in room-relative Y
                            Some(default_y_top + t_clamped * (default_y_bottom - default_y_top))
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    // Calculate where the new wall should be placed
                    // Uses floor/ceiling heights and finds gaps between existing walls
                    let wall_info = if let Some(room) = state.level.rooms.get(state.current_room) {
                        if let Some((gx, gz)) = room.world_to_grid(grid_x + SECTOR_SIZE * 0.5, grid_z + SECTOR_SIZE * 0.5) {
                            if let Some(sector) = room.get_sector(gx, gz) {
                                let has_existing = !sector.walls(dir).is_empty();
                                match sector.next_wall_position(dir, default_y_bottom, default_y_top, mouse_y_room_relative) {
                                    Some(corner_heights) => {
                                        // 0 = new wall, 1 = filling gap
                                        let state = if has_existing { 1u8 } else { 0u8 };
                                        Some((corner_heights, state))
                                    }
                                    None => {
                                        // Edge is fully covered
                                        Some(([0.0, 0.0, 0.0, 0.0], 2u8))
                                    }
                                }
                            } else {
                                Some(([default_y_bottom, default_y_bottom, default_y_top, default_y_top], 0u8))
                            }
                        } else {
                            Some(([default_y_bottom, default_y_bottom, default_y_top, default_y_top], 0u8))
                        }
                    } else {
                        Some(([default_y_bottom, default_y_bottom, default_y_top, default_y_top], 0u8))
                    };

                    if let Some((corner_heights, wall_state)) = wall_info {
                        preview_wall = Some((grid_x, grid_z, dir, corner_heights, wall_state, mouse_y_room_relative));

                        // Debug logging
                        let dir_name = match dir {
                            Direction::North => "N",
                            Direction::South => "S",
                            Direction::East => "E",
                            Direction::West => "W",
                        };
                        let state_name = match wall_state {
                            0 => "new",
                            1 => "gap",
                            2 => "full",
                            _ => "?",
                        };
                        eprintln!("WALL: mouse_fb=({:.0},{:.0}) dist={:.0} edge={:?} dir={} mouse_y_rel={:?} heights=[{:.0},{:.0},{:.0},{:.0}] state={}",
                            mouse_fb_x, mouse_fb_y, dist, (grid_x, grid_z), dir_name,
                            mouse_y_room_relative.map(|y| y as i32),
                            corner_heights[0], corner_heights[1], corner_heights[2], corner_heights[3],
                            state_name);
                    }
                }
            }
        }
    }

    // In DrawDiagonalWall mode, find preview diagonal edge
    if inside_viewport && state.tool == EditorTool::DrawDiagonalWall {
        if let Some((mouse_fb_x, mouse_fb_y)) = screen_to_fb(mouse_pos.0, mouse_pos.1) {
            use super::CEILING_HEIGHT;

            // Find the closest sector center first
            let search_radius = 20;
            let cam = &state.camera_3d.position;
            let start_x = ((cam.x / SECTOR_SIZE).floor() as i32 - search_radius) as f32 * SECTOR_SIZE;
            let start_z = ((cam.z / SECTOR_SIZE).floor() as i32 - search_radius) as f32 * SECTOR_SIZE;

            let (default_y_bottom, default_y_top) = (0.0, CEILING_HEIGHT);
            let mid_y = (default_y_bottom + default_y_top) / 2.0;

            // Find closest sector center
            let mut closest_sector: Option<(f32, f32, f32)> = None; // (grid_x, grid_z, screen_dist)

            for ix in 0..(search_radius * 2) {
                for iz in 0..(search_radius * 2) {
                    let grid_x = start_x + (ix as f32 * SECTOR_SIZE);
                    let grid_z = start_z + (iz as f32 * SECTOR_SIZE);
                    let center = Vec3::new(grid_x + SECTOR_SIZE / 2.0, mid_y, grid_z + SECTOR_SIZE / 2.0);

                    if let Some((sx, sy)) = world_to_screen(center, state.camera_3d.position,
                        state.camera_3d.basis_x, state.camera_3d.basis_y, state.camera_3d.basis_z,
                        fb.width, fb.height)
                    {
                        let dist = ((mouse_fb_x - sx).powi(2) + (mouse_fb_y - sy).powi(2)).sqrt();
                        if closest_sector.map_or(true, |(_, _, best)| dist < best) {
                            closest_sector = Some((grid_x, grid_z, dist));
                        }
                    }
                }
            }

            if let Some((grid_x, grid_z, dist)) = closest_sector {
                if dist < 120.0 {
                    let center_x = grid_x + SECTOR_SIZE / 2.0;
                    let center_z = grid_z + SECTOR_SIZE / 2.0;

                    // Choose diagonal type based on which diagonal line the mouse is closer to
                    let is_nwse = if let Some(_) = world_to_screen(
                        Vec3::new(center_x, mid_y, center_z), state.camera_3d.position,
                        state.camera_3d.basis_x, state.camera_3d.basis_y, state.camera_3d.basis_z,
                        fb.width, fb.height)
                    {
                        // Project NW corner and SE corner to screen
                        let nw = Vec3::new(grid_x, mid_y, grid_z);
                        let se = Vec3::new(grid_x + SECTOR_SIZE, mid_y, grid_z + SECTOR_SIZE);
                        let ne = Vec3::new(grid_x + SECTOR_SIZE, mid_y, grid_z);
                        let sw = Vec3::new(grid_x, mid_y, grid_z + SECTOR_SIZE);

                        if let (Some((nw_sx, nw_sy)), Some((se_sx, se_sy)),
                                Some((ne_sx, ne_sy)), Some((sw_sx, sw_sy))) = (
                            world_to_screen(nw, state.camera_3d.position, state.camera_3d.basis_x,
                                state.camera_3d.basis_y, state.camera_3d.basis_z, fb.width, fb.height),
                            world_to_screen(se, state.camera_3d.position, state.camera_3d.basis_x,
                                state.camera_3d.basis_y, state.camera_3d.basis_z, fb.width, fb.height),
                            world_to_screen(ne, state.camera_3d.position, state.camera_3d.basis_x,
                                state.camera_3d.basis_y, state.camera_3d.basis_z, fb.width, fb.height),
                            world_to_screen(sw, state.camera_3d.position, state.camera_3d.basis_x,
                                state.camera_3d.basis_y, state.camera_3d.basis_z, fb.width, fb.height),
                        ) {
                            // Calculate distance from mouse to each diagonal line
                            let dist_nwse = point_to_line_dist(mouse_fb_x, mouse_fb_y, nw_sx, nw_sy, se_sx, se_sy);
                            let dist_nesw = point_to_line_dist(mouse_fb_x, mouse_fb_y, ne_sx, ne_sy, sw_sx, sw_sy);
                            dist_nwse < dist_nesw
                        } else {
                            true // Default to NW-SE
                        }
                    } else {
                        true
                    };

                    // Compute mouse Y in room-relative space for gap selection
                    let room_y = state.level.rooms.get(state.current_room)
                        .map(|r| r.position.y)
                        .unwrap_or(0.0);
                    let floor_world = Vec3::new(center_x, room_y + default_y_bottom, center_z);
                    let ceiling_world = Vec3::new(center_x, room_y + default_y_top, center_z);

                    let mouse_y_room_relative = if let (Some((_, floor_sy)), Some((_, ceiling_sy))) = (
                        world_to_screen(floor_world, state.camera_3d.position,
                            state.camera_3d.basis_x, state.camera_3d.basis_y, state.camera_3d.basis_z,
                            fb.width, fb.height),
                        world_to_screen(ceiling_world, state.camera_3d.position,
                            state.camera_3d.basis_x, state.camera_3d.basis_y, state.camera_3d.basis_z,
                            fb.width, fb.height),
                    ) {
                        if (floor_sy - ceiling_sy).abs() > 1.0 {
                            let t = (mouse_fb_y - ceiling_sy) / (floor_sy - ceiling_sy);
                            let t_clamped = t.clamp(0.0, 1.0);
                            Some(default_y_top + t_clamped * (default_y_bottom - default_y_top))
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    // Use gap detection for diagonal walls
                    let wall_info = if let Some(room) = state.level.rooms.get(state.current_room) {
                        if let Some((gx, gz)) = room.world_to_grid(center_x, center_z) {
                            if let Some(sector) = room.get_sector(gx, gz) {
                                let walls = if is_nwse { &sector.walls_nwse } else { &sector.walls_nesw };
                                let has_existing = !walls.is_empty();
                                match sector.next_diagonal_wall_position(is_nwse, default_y_bottom, default_y_top, mouse_y_room_relative) {
                                    Some(heights) => Some((heights, if has_existing { 1u8 } else { 0u8 })),
                                    None => Some(([0.0, 0.0, 0.0, 0.0], 2u8)), // Fully covered
                                }
                            } else {
                                Some(([default_y_bottom, default_y_bottom, default_y_top, default_y_top], 0u8))
                            }
                        } else {
                            Some(([default_y_bottom, default_y_bottom, default_y_top, default_y_top], 0u8))
                        }
                    } else {
                        Some(([default_y_bottom, default_y_bottom, default_y_top, default_y_top], 0u8))
                    };

                    if let Some((corner_heights, wall_state)) = wall_info {
                        if wall_state < 2 { // Not fully covered
                            preview_diagonal_wall = Some((grid_x, grid_z, is_nwse, corner_heights));
                        }
                    }
                }
            }
        }
    }

    // Handle clicks and dragging in 3D viewport
    if inside_viewport && !ctx.mouse.right_down {
        // Detect modifier keys for selection
        let shift_down = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
        let ctrl_down = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl);

        // Start dragging or select on left press
        if ctx.mouse.left_pressed {
            if state.tool == EditorTool::Select {
                // Priority: vertex > edge > face
                if let Some((room_idx, gx, gz, corner_idx, face, _)) = hovered_vertex {
                    // Create vertex selection (allows proper vertex multi-selection)
                    let new_selection = Selection::Vertex { room: room_idx, x: gx, z: gz, face: face.clone(), corner_idx };
                    if shift_down {
                        state.toggle_multi_selection(new_selection.clone());
                    } else {
                        state.clear_multi_selection();
                    }
                    // Save selection for undo
                    state.save_selection_undo();
                    state.selection = new_selection;

                    // Scroll texture palette to show this face's texture
                    let tex_to_scroll = state.level.rooms.get(room_idx).and_then(|room| {
                        room.get_sector(gx, gz).and_then(|sector| {
                            match &face {
                                SectorFace::Floor => sector.floor.as_ref().map(|f| f.texture.clone()),
                                SectorFace::Ceiling => sector.ceiling.as_ref().map(|c| c.texture.clone()),
                                SectorFace::WallNorth(i) => sector.walls_north.get(*i).map(|w| w.texture.clone()),
                                SectorFace::WallEast(i) => sector.walls_east.get(*i).map(|w| w.texture.clone()),
                                SectorFace::WallSouth(i) => sector.walls_south.get(*i).map(|w| w.texture.clone()),
                                SectorFace::WallWest(i) => sector.walls_west.get(*i).map(|w| w.texture.clone()),
                                SectorFace::WallNwSe(i) => sector.walls_nwse.get(*i).map(|w| w.texture.clone()),
                                SectorFace::WallNeSw(i) => sector.walls_nesw.get(*i).map(|w| w.texture.clone()),
                            }
                        })
                    });
                    if let Some(tex) = tex_to_scroll {
                        state.scroll_to_texture(&tex);
                    }

                    // Also update selected_vertex_indices for legacy compatibility
                    state.selected_vertex_indices.clear();
                    state.selected_vertex_indices.push(corner_idx);

                    // Start dragging vertex (and all multi-selected vertices)
                    state.dragging_sector_vertices.clear();
                    state.drag_initial_heights.clear();
                    state.viewport_drag_started = false;

                    // Helper to add a vertex to drag list with its initial height
                    let mut add_vertex_to_drag = |ri: usize, vgx: usize, vgz: usize, vface: SectorFace, ci: usize, level: &crate::world::Level| {
                        let key = (ri, vgx, vgz, vface.clone(), ci);
                        if state.dragging_sector_vertices.contains(&key) {
                            return;
                        }
                        if let Some(room) = level.rooms.get(ri) {
                            if let Some(sector) = room.get_sector(vgx, vgz) {
                                let height = match &vface {
                                    SectorFace::Floor => sector.floor.as_ref().map(|f| f.heights[ci]),
                                    SectorFace::Ceiling => sector.ceiling.as_ref().map(|c| c.heights[ci]),
                                    SectorFace::WallNorth(i) => sector.walls_north.get(*i).map(|w| w.heights[ci]),
                                    SectorFace::WallEast(i) => sector.walls_east.get(*i).map(|w| w.heights[ci]),
                                    SectorFace::WallSouth(i) => sector.walls_south.get(*i).map(|w| w.heights[ci]),
                                    SectorFace::WallWest(i) => sector.walls_west.get(*i).map(|w| w.heights[ci]),
                                    SectorFace::WallNwSe(i) => sector.walls_nwse.get(*i).map(|w| w.heights[ci]),
                                    SectorFace::WallNeSw(i) => sector.walls_nesw.get(*i).map(|w| w.heights[ci]),
                                };
                                if let Some(h) = height {
                                    state.dragging_sector_vertices.push(key);
                                    state.drag_initial_heights.push(h);
                                }
                            }
                        }
                    };

                    // Add clicked vertex
                    add_vertex_to_drag(room_idx, gx, gz, face.clone(), corner_idx, &state.level);

                    // Add all multi-selected vertices
                    for sel in &state.multi_selection {
                        if let Selection::Vertex { room, x, z, face: f, corner_idx: ci } = sel {
                            add_vertex_to_drag(*room, *x, *z, f.clone(), *ci, &state.level);
                        }
                    }

                    // Set initial drag plane Y
                    if !state.drag_initial_heights.is_empty() {
                        state.viewport_drag_plane_y = state.drag_initial_heights.iter().sum::<f32>()
                            / state.drag_initial_heights.len() as f32;
                    }

                    // If linking mode is on, find coincident vertices across ALL rooms
                    if state.link_coincident_vertices {
                        let all_room_vertices = collect_all_room_vertices(state);
                        const EPSILON: f32 = 0.1;

                        // Collect world positions of all currently dragged vertices
                        let dragged_positions: Vec<Vec3> = state.dragging_sector_vertices.iter()
                            .filter_map(|(ri, vgx, vgz, vface, ci)| {
                                all_room_vertices.iter()
                                    .find(|(_, r, x, z, c, f)| *r == *ri && *x == *vgx && *z == *vgz && *c == *ci && f == vface)
                                    .map(|(pos, _, _, _, _, _)| *pos)
                            })
                            .collect();

                        // Find all coincident vertices
                        for (pos, ri, vgx, vgz, ci, vface) in &all_room_vertices {
                            for dragged_pos in &dragged_positions {
                                if (pos.x - dragged_pos.x).abs() < EPSILON &&
                                   (pos.y - dragged_pos.y).abs() < EPSILON &&
                                   (pos.z - dragged_pos.z).abs() < EPSILON {
                                    let key = (*ri, *vgx, *vgz, *vface, *ci);
                                    if !state.dragging_sector_vertices.contains(&key) {
                                        state.dragging_sector_vertices.push(key);
                                        let linked_room_y = state.level.rooms.get(*ri).map(|r| r.position.y).unwrap_or(0.0);
                                        state.drag_initial_heights.push(pos.y - linked_room_y);
                                    }
                                    break;
                                }
                            }
                        }

                        // Update drag plane Y with all vertices
                        if !state.drag_initial_heights.is_empty() {
                            state.viewport_drag_plane_y = state.drag_initial_heights.iter().sum::<f32>()
                                / state.drag_initial_heights.len() as f32;
                        }
                    }
                } else if let Some((room_idx, gx, gz, face_idx, edge_idx, wall_face, _)) = hovered_edge {
                    // Start dragging edge (both vertices)
                    state.dragging_sector_vertices.clear();
                    state.drag_initial_heights.clear();
                    state.viewport_drag_started = false;

                    // Use Selection::Edge for proper edge multi-selection
                    let new_selection = Selection::Edge {
                        room: room_idx,
                        x: gx,
                        z: gz,
                        face_idx,
                        edge_idx,
                        wall_face: wall_face.clone(),
                    };

                    if shift_down {
                        state.toggle_multi_selection(new_selection.clone());
                    } else {
                        let clicking_selected = state.selection == new_selection ||
                            state.multi_selection.contains(&new_selection);
                        if !clicking_selected {
                            state.clear_multi_selection();
                        }
                    }
                    // Save selection for undo
                    state.save_selection_undo();
                    state.selection = new_selection;

                    // Scroll texture palette to show this face's texture
                    let face_for_texture = match face_idx {
                        0 => SectorFace::Floor,
                        1 => SectorFace::Ceiling,
                        _ => wall_face.clone().unwrap_or(SectorFace::Floor),
                    };
                    let tex_to_scroll = state.level.rooms.get(room_idx).and_then(|room| {
                        room.get_sector(gx, gz).and_then(|sector| {
                            match &face_for_texture {
                                SectorFace::Floor => sector.floor.as_ref().map(|f| f.texture.clone()),
                                SectorFace::Ceiling => sector.ceiling.as_ref().map(|c| c.texture.clone()),
                                SectorFace::WallNorth(i) => sector.walls_north.get(*i).map(|w| w.texture.clone()),
                                SectorFace::WallEast(i) => sector.walls_east.get(*i).map(|w| w.texture.clone()),
                                SectorFace::WallSouth(i) => sector.walls_south.get(*i).map(|w| w.texture.clone()),
                                SectorFace::WallWest(i) => sector.walls_west.get(*i).map(|w| w.texture.clone()),
                                SectorFace::WallNwSe(i) => sector.walls_nwse.get(*i).map(|w| w.texture.clone()),
                                SectorFace::WallNeSw(i) => sector.walls_nesw.get(*i).map(|w| w.texture.clone()),
                            }
                        })
                    });
                    if let Some(tex) = tex_to_scroll {
                        state.scroll_to_texture(&tex);
                    }

                    // Pre-select the two vertices that make up this edge for color editing
                    // For floor/ceiling: edges are [0:NW-NE, 1:NE-SE, 2:SE-SW, 3:SW-NW]
                    // For walls: edges are [0:BL-BR, 1:BR-TR, 2:TR-TL, 3:TL-BL]
                    let (v1, v2) = match edge_idx {
                        0 => (0, 1),
                        1 => (1, 2),
                        2 => (2, 3),
                        3 => (3, 0),
                        _ => (0, 1),
                    };
                    state.selected_vertex_indices.clear();
                    state.selected_vertex_indices.push(v1);
                    state.selected_vertex_indices.push(v2);

                    // Collect all edges to drag: start with clicked edge + multi-selected edges
                    let mut edges_to_drag: Vec<(usize, usize, usize, usize, usize, Option<SectorFace>)> = Vec::new();

                    // Add the clicked edge
                    edges_to_drag.push((room_idx, gx, gz, face_idx, edge_idx, wall_face.clone()));

                    // Add all multi-selected edges
                    for sel in &state.multi_selection {
                        if let Selection::Edge { room, x, z, face_idx: fi, edge_idx: ei, wall_face: wf } = sel {
                            let key = (*room, *x, *z, *fi, *ei, wf.clone());
                            if !edges_to_drag.contains(&key) {
                                edges_to_drag.push(key);
                            }
                        }
                    }

                    // Add vertices for all edges to drag
                    for (r_idx, gx, gz, face_idx, edge_idx, wf) in &edges_to_drag {
                        if let Some(room) = state.level.rooms.get(*r_idx) {
                            if let Some(sector) = room.get_sector(*gx, *gz) {
                                // Handle floor/ceiling edges
                                if *face_idx == 0 || *face_idx == 1 {
                                    let face = if *face_idx == 0 { SectorFace::Floor } else { SectorFace::Ceiling };
                                    let corner0 = *edge_idx;
                                    let corner1 = (*edge_idx + 1) % 4;

                                    let heights = match face {
                                        SectorFace::Floor => sector.floor.as_ref().map(|f| f.heights),
                                        SectorFace::Ceiling => sector.ceiling.as_ref().map(|c| c.heights),
                                        _ => None,
                                    };
                                    if let Some(h) = heights {
                                        // Add both edge vertices
                                        let key0 = (*r_idx, *gx, *gz, face, corner0);
                                        if !state.dragging_sector_vertices.contains(&key0) {
                                            state.dragging_sector_vertices.push(key0);
                                            state.drag_initial_heights.push(h[corner0]);
                                        }
                                        let key1 = (*r_idx, *gx, *gz, face, corner1);
                                        if !state.dragging_sector_vertices.contains(&key1) {
                                            state.dragging_sector_vertices.push(key1);
                                            state.drag_initial_heights.push(h[corner1]);
                                        }

                                        // If linking, find coincident vertices for the edge across ALL rooms
                                        if state.link_coincident_vertices {
                                            let base_x = room.position.x + (*gx as f32) * SECTOR_SIZE;
                                            let base_z = room.position.z + (*gz as f32) * SECTOR_SIZE;
                                            let room_y = room.position.y; // Y offset for world-space

                                            let edge_positions = [
                                                match corner0 {
                                                    0 => Vec3::new(base_x, room_y + h[0], base_z),
                                                    1 => Vec3::new(base_x + SECTOR_SIZE, room_y + h[1], base_z),
                                                    2 => Vec3::new(base_x + SECTOR_SIZE, room_y + h[2], base_z + SECTOR_SIZE),
                                                    3 => Vec3::new(base_x, room_y + h[3], base_z + SECTOR_SIZE),
                                                    _ => unreachable!(),
                                                },
                                                match corner1 {
                                                    0 => Vec3::new(base_x, room_y + h[0], base_z),
                                                    1 => Vec3::new(base_x + SECTOR_SIZE, room_y + h[1], base_z),
                                                    2 => Vec3::new(base_x + SECTOR_SIZE, room_y + h[2], base_z + SECTOR_SIZE),
                                                    3 => Vec3::new(base_x, room_y + h[3], base_z + SECTOR_SIZE),
                                                    _ => unreachable!(),
                                                },
                                            ];

                                            const EPSILON: f32 = 0.1;
                                            let all_room_vertices = collect_all_room_vertices(state);
                                            for (pos, ri, vgx, vgz, ci, vface) in &all_room_vertices {
                                                for ep in &edge_positions {
                                                    if (pos.x - ep.x).abs() < EPSILON &&
                                                       (pos.y - ep.y).abs() < EPSILON &&
                                                       (pos.z - ep.z).abs() < EPSILON {
                                                        let key = (*ri, *vgx, *vgz, *vface, *ci);
                                                        if !state.dragging_sector_vertices.contains(&key) {
                                                            state.dragging_sector_vertices.push(key);
                                                            // Store room-relative height (pos.y is world-space)
                                                            let linked_room_y = state.level.rooms.get(*ri).map(|r| r.position.y).unwrap_or(0.0);
                                                            state.drag_initial_heights.push(pos.y - linked_room_y);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                // Handle wall edges (face_idx == 2)
                                else if *face_idx == 2 {
                                    if let Some(wall_face) = wf {
                                        let corner0 = *edge_idx;
                                        let corner1 = (*edge_idx + 1) % 4;

                                        // Get wall heights based on wall direction
                                        let wall_heights = match wall_face {
                                            SectorFace::WallNorth(i) => sector.walls_north.get(*i).map(|w| w.heights),
                                            SectorFace::WallEast(i) => sector.walls_east.get(*i).map(|w| w.heights),
                                            SectorFace::WallSouth(i) => sector.walls_south.get(*i).map(|w| w.heights),
                                            SectorFace::WallWest(i) => sector.walls_west.get(*i).map(|w| w.heights),
                                            SectorFace::WallNwSe(i) => sector.walls_nwse.get(*i).map(|w| w.heights),
                                            SectorFace::WallNeSw(i) => sector.walls_nesw.get(*i).map(|w| w.heights),
                                            _ => None,
                                        };

                                        if let Some(h) = wall_heights {
                                            // Add both edge vertices
                                            let key0 = (*r_idx, *gx, *gz, *wall_face, corner0);
                                            if !state.dragging_sector_vertices.contains(&key0) {
                                                state.dragging_sector_vertices.push(key0);
                                                state.drag_initial_heights.push(h[corner0]);
                                            }
                                            let key1 = (*r_idx, *gx, *gz, *wall_face, corner1);
                                            if !state.dragging_sector_vertices.contains(&key1) {
                                                state.dragging_sector_vertices.push(key1);
                                                state.drag_initial_heights.push(h[corner1]);
                                            }

                                            // If linking, find coincident vertices for wall edges across ALL rooms
                                            if state.link_coincident_vertices {
                                                let base_x = room.position.x + (*gx as f32) * SECTOR_SIZE;
                                                let base_z = room.position.z + (*gz as f32) * SECTOR_SIZE;
                                                let room_y = room.position.y;

                                                // Wall corner positions depend on wall direction
                                                // Walls: [0]=bottom-left, [1]=bottom-right, [2]=top-right, [3]=top-left
                                                let (x0, z0, x1, z1) = match wall_face {
                                                    SectorFace::WallNorth(_) => (base_x, base_z, base_x + SECTOR_SIZE, base_z),
                                                    SectorFace::WallEast(_) => (base_x + SECTOR_SIZE, base_z, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE),
                                                    SectorFace::WallSouth(_) => (base_x + SECTOR_SIZE, base_z + SECTOR_SIZE, base_x, base_z + SECTOR_SIZE),
                                                    SectorFace::WallWest(_) => (base_x, base_z + SECTOR_SIZE, base_x, base_z),
                                                    SectorFace::WallNwSe(_) => (base_x, base_z, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE),
                                                    SectorFace::WallNeSw(_) => (base_x + SECTOR_SIZE, base_z, base_x, base_z + SECTOR_SIZE),
                                                    _ => (base_x, base_z, base_x, base_z),
                                                };

                                                let edge_positions = [
                                                    match corner0 {
                                                        0 => Vec3::new(x0, room_y + h[0], z0), // bottom-left
                                                        1 => Vec3::new(x1, room_y + h[1], z1), // bottom-right
                                                        2 => Vec3::new(x1, room_y + h[2], z1), // top-right
                                                        3 => Vec3::new(x0, room_y + h[3], z0), // top-left
                                                        _ => unreachable!(),
                                                    },
                                                    match corner1 {
                                                        0 => Vec3::new(x0, room_y + h[0], z0),
                                                        1 => Vec3::new(x1, room_y + h[1], z1),
                                                        2 => Vec3::new(x1, room_y + h[2], z1),
                                                        3 => Vec3::new(x0, room_y + h[3], z0),
                                                        _ => unreachable!(),
                                                    },
                                                ];

                                                const EPSILON: f32 = 0.1;
                                                let all_room_vertices = collect_all_room_vertices(state);
                                                for (pos, ri, vgx, vgz, ci, vface) in &all_room_vertices {
                                                    for ep in &edge_positions {
                                                        if (pos.x - ep.x).abs() < EPSILON &&
                                                           (pos.y - ep.y).abs() < EPSILON &&
                                                           (pos.z - ep.z).abs() < EPSILON {
                                                            let key = (*ri, *vgx, *vgz, *vface, *ci);
                                                            if !state.dragging_sector_vertices.contains(&key) {
                                                                state.dragging_sector_vertices.push(key);
                                                                let linked_room_y = state.level.rooms.get(*ri).map(|r| r.position.y).unwrap_or(0.0);
                                                                state.drag_initial_heights.push(pos.y - linked_room_y);
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Set drag plane to average height of ALL vertices (including linked ones)
                    // This prevents jumping when linked vertices have different room-relative heights
                    if !state.drag_initial_heights.is_empty() {
                        state.viewport_drag_plane_y = state.drag_initial_heights.iter().sum::<f32>()
                            / state.drag_initial_heights.len() as f32;
                    }
                } else if let Some((obj_room_idx, obj_idx, _)) = hovered_object {
                    // Object selection/dragging - checked before lights and faces
                    let is_already_selected = matches!(&state.selection,
                        Selection::Object { room, index } if *room == obj_room_idx && *index == obj_idx);

                    if is_already_selected {
                        // Start dragging the object (Y-axis)
                        if let Some(room) = state.level.rooms.get(obj_room_idx) {
                            if let Some(obj) = room.objects.get(obj_idx) {
                                let world_pos = obj.world_position(room);
                                state.dragging_object = Some((obj_room_idx, obj_idx));
                                state.dragging_object_initial_y = world_pos.y;
                                state.dragging_object_plane_y = world_pos.y;
                            }
                        }
                    } else {
                        // Select object
                        state.save_selection_undo();
                        state.set_selection(Selection::Object { room: obj_room_idx, index: obj_idx });
                        state.set_status("Object selected", 1.0);
                    }
                } else if let Some((room_idx, gx, gz, face)) = hovered_face {
                    // Start dragging face (all 4 vertices)
                    state.dragging_sector_vertices.clear();
                    state.drag_initial_heights.clear();
                    state.viewport_drag_started = false;

                    // Handle selection first (Shift = toggle multi-select)
                    let new_selection = Selection::SectorFace { room: room_idx, x: gx, z: gz, face };

                    if shift_down {
                        // Shift-click: rectangular selection for floors and ceilings
                        state.save_selection_undo();

                        // Check if clicking on a floor or ceiling
                        let is_floor = matches!(face, SectorFace::Floor);
                        let is_ceiling = matches!(face, SectorFace::Ceiling);

                        if is_floor || is_ceiling {
                            // Collect all currently selected faces of the same type in the same room
                            let mut selected_faces: Vec<(usize, usize)> = Vec::new();

                            // Check primary selection
                            if let Selection::SectorFace { room, x, z, face: sel_face } = &state.selection {
                                let matches_type = (is_floor && matches!(sel_face, SectorFace::Floor))
                                    || (is_ceiling && matches!(sel_face, SectorFace::Ceiling));
                                if *room == room_idx && matches_type {
                                    selected_faces.push((*x, *z));
                                }
                            }

                            // Check multi-selection
                            for sel in &state.multi_selection {
                                if let Selection::SectorFace { room, x, z, face: sel_face } = sel {
                                    let matches_type = (is_floor && matches!(sel_face, SectorFace::Floor))
                                        || (is_ceiling && matches!(sel_face, SectorFace::Ceiling));
                                    if *room == room_idx && matches_type && !selected_faces.contains(&(*x, *z)) {
                                        selected_faces.push((*x, *z));
                                    }
                                }
                            }

                            if !selected_faces.is_empty() {
                                // Calculate bounding rectangle including the new click
                                let mut min_x = gx;
                                let mut max_x = gx;
                                let mut min_z = gz;
                                let mut max_z = gz;

                                for (sx, sz) in &selected_faces {
                                    min_x = min_x.min(*sx);
                                    max_x = max_x.max(*sx);
                                    min_z = min_z.min(*sz);
                                    max_z = max_z.max(*sz);
                                }

                                // Collect all faces of this type in the rectangle that actually exist
                                let mut faces_to_select: Vec<(usize, usize)> = Vec::new();
                                if let Some(room) = state.level.rooms.get(room_idx) {
                                    for rx in min_x..=max_x {
                                        for rz in min_z..=max_z {
                                            if let Some(sector) = room.get_sector(rx, rz) {
                                                let has_face = if is_floor {
                                                    sector.floor.is_some()
                                                } else {
                                                    sector.ceiling.is_some()
                                                };
                                                if has_face {
                                                    faces_to_select.push((rx, rz));
                                                }
                                            }
                                        }
                                    }
                                }

                                // Clear existing selection and apply new selections
                                state.clear_multi_selection();
                                let target_face = if is_floor { SectorFace::Floor } else { SectorFace::Ceiling };
                                for (i, (rx, rz)) in faces_to_select.into_iter().enumerate() {
                                    let sel = Selection::SectorFace {
                                        room: room_idx,
                                        x: rx,
                                        z: rz,
                                        face: target_face
                                    };
                                    if i == 0 {
                                        state.set_selection(sel);
                                    } else {
                                        state.multi_selection.push(sel);
                                    }
                                }
                            } else {
                                // No existing selection of this type, just select this face
                                state.toggle_multi_selection(new_selection.clone());
                                state.set_selection(new_selection.clone());
                            }
                        } else {
                            // Check if clicking on any wall type (cardinal or diagonal)
                            let is_wall = matches!(face,
                                SectorFace::WallNorth(_) | SectorFace::WallEast(_) |
                                SectorFace::WallSouth(_) | SectorFace::WallWest(_) |
                                SectorFace::WallNwSe(_) | SectorFace::WallNeSw(_)
                            );

                            if is_wall {
                                // Find any existing wall/diagonal selection in the same room
                                let mut existing_wall: Option<(usize, usize, SectorFace)> = None;

                                // Check primary selection
                                if let Selection::SectorFace { room, x, z, face: sel_face } = &state.selection {
                                    if *room == room_idx && is_wall_face(sel_face) {
                                        existing_wall = Some((*x, *z, *sel_face));
                                    }
                                }

                                // Check multi-selection if no primary wall found
                                if existing_wall.is_none() {
                                    for sel in &state.multi_selection {
                                        if let Selection::SectorFace { room, x, z, face: sel_face } = sel {
                                            if *room == room_idx && is_wall_face(sel_face) {
                                                existing_wall = Some((*x, *z, *sel_face));
                                                break;
                                            }
                                        }
                                    }
                                }

                                if let Some((start_x, start_z, start_face)) = existing_wall {
                                    // Use BFS to find connected path
                                    let path = if let Some(room) = state.level.rooms.get(room_idx) {
                                        find_wall_path(room, start_x, start_z, &start_face, gx, gz, &face)
                                    } else {
                                        None
                                    };

                                    if let Some(walls_on_path) = path {
                                        // Clear existing selection and select all walls on path
                                        state.clear_multi_selection();
                                        for (i, (wx, wz, wface)) in walls_on_path.into_iter().enumerate() {
                                            let sel = Selection::SectorFace {
                                                room: room_idx,
                                                x: wx,
                                                z: wz,
                                                face: wface
                                            };
                                            if i == 0 {
                                                state.set_selection(sel);
                                            } else {
                                                state.multi_selection.push(sel);
                                            }
                                        }
                                    } else {
                                        // No connected path found, just toggle this wall
                                        state.toggle_multi_selection(new_selection.clone());
                                        state.set_selection(new_selection.clone());
                                    }
                                } else {
                                    // No existing wall selection, just select this wall
                                    state.toggle_multi_selection(new_selection.clone());
                                    state.set_selection(new_selection.clone());
                                }
                            } else {
                                // Not a floor, ceiling, or wall - use standard toggle behavior
                                state.toggle_multi_selection(new_selection.clone());
                                state.set_selection(new_selection.clone());
                            }
                        }
                    } else if ctrl_down {
                        // Ctrl+click: Toggle individual item in multi-selection
                        state.save_selection_undo();

                        // Check if item was already selected (will be removed)
                        let was_selected = state.selection == new_selection ||
                            state.multi_selection.contains(&new_selection);

                        state.toggle_multi_selection(new_selection.clone());

                        if was_selected {
                            // Item was removed - pick a new primary selection from remaining
                            if state.selection == new_selection {
                                if let Some(first) = state.multi_selection.first().cloned() {
                                    state.selection = first;
                                } else {
                                    state.selection = Selection::None;
                                }
                            }
                            // If primary selection is still valid, keep it
                        } else {
                            // Item was added - make it the primary selection
                            state.set_selection(new_selection.clone());
                        }
                    } else {
                        // Regular click: Deselect all, select just this one
                        // Only save undo if something actually changes
                        if state.selection != new_selection || !state.multi_selection.is_empty() {
                            state.save_selection_undo();
                            state.clear_multi_selection();
                            state.set_selection(new_selection.clone());
                        }
                    }

                    // Scroll texture palette to show this face's texture
                    let tex_to_scroll = state.level.rooms.get(room_idx).and_then(|room| {
                        room.get_sector(gx, gz).and_then(|sector| {
                            match &face {
                                SectorFace::Floor => sector.floor.as_ref().map(|f| f.texture.clone()),
                                SectorFace::Ceiling => sector.ceiling.as_ref().map(|c| c.texture.clone()),
                                SectorFace::WallNorth(i) => sector.walls_north.get(*i).map(|w| w.texture.clone()),
                                SectorFace::WallEast(i) => sector.walls_east.get(*i).map(|w| w.texture.clone()),
                                SectorFace::WallSouth(i) => sector.walls_south.get(*i).map(|w| w.texture.clone()),
                                SectorFace::WallWest(i) => sector.walls_west.get(*i).map(|w| w.texture.clone()),
                                SectorFace::WallNwSe(i) => sector.walls_nwse.get(*i).map(|w| w.texture.clone()),
                                SectorFace::WallNeSw(i) => sector.walls_nesw.get(*i).map(|w| w.texture.clone()),
                            }
                        })
                    });
                    if let Some(tex) = tex_to_scroll {
                        state.scroll_to_texture(&tex);
                    }

                    // Collect all faces to drag: primary selection + multi-selection
                    let mut faces_to_drag: Vec<(usize, usize, usize, SectorFace)> = Vec::new();

                    // Add primary selection if it's a face
                    if let Selection::SectorFace { room, x, z, face } = &state.selection {
                        faces_to_drag.push((*room, *x, *z, *face));
                    }

                    // Add all multi-selected faces
                    for sel in &state.multi_selection {
                        if let Selection::SectorFace { room, x, z, face } = sel {
                            let key = (*room, *x, *z, *face);
                            if !faces_to_drag.contains(&key) {
                                faces_to_drag.push(key);
                            }
                        }
                    }

                    // Add vertices for all faces to drag
                    for (r_idx, gx, gz, face) in &faces_to_drag {
                        if let Some(room) = state.level.rooms.get(*r_idx) {
                            if let Some(sector) = room.get_sector(*gx, *gz) {
                                // Handle floor/ceiling
                                let heights = match face {
                                    SectorFace::Floor => sector.floor.as_ref().map(|f| f.heights),
                                    SectorFace::Ceiling => sector.ceiling.as_ref().map(|c| c.heights),
                                    _ => None,
                                };

                                if let Some(h) = heights {
                                    for corner in 0..4 {
                                        let key = (*r_idx, *gx, *gz, *face, corner);
                                        if !state.dragging_sector_vertices.contains(&key) {
                                            state.dragging_sector_vertices.push(key);
                                            state.drag_initial_heights.push(h[corner]);
                                        }
                                    }

                                    // If linking, find coincident vertices across ALL rooms
                                    if state.link_coincident_vertices {
                                        let base_x = room.position.x + (*gx as f32) * SECTOR_SIZE;
                                        let base_z = room.position.z + (*gz as f32) * SECTOR_SIZE;
                                        let room_y = room.position.y; // Y offset for world-space
                                        let face_positions = [
                                            Vec3::new(base_x, room_y + h[0], base_z),
                                            Vec3::new(base_x + SECTOR_SIZE, room_y + h[1], base_z),
                                            Vec3::new(base_x + SECTOR_SIZE, room_y + h[2], base_z + SECTOR_SIZE),
                                            Vec3::new(base_x, room_y + h[3], base_z + SECTOR_SIZE),
                                        ];

                                        const EPSILON: f32 = 0.1;
                                        let all_room_vertices = collect_all_room_vertices(state);
                                        for (pos, ri, vgx, vgz, ci, vface) in &all_room_vertices {
                                            for fp in &face_positions {
                                                if (pos.x - fp.x).abs() < EPSILON &&
                                                   (pos.y - fp.y).abs() < EPSILON &&
                                                   (pos.z - fp.z).abs() < EPSILON {
                                                    let key = (*ri, *vgx, *vgz, *vface, *ci);
                                                    if !state.dragging_sector_vertices.contains(&key) {
                                                        state.dragging_sector_vertices.push(key);
                                                        // Store room-relative height (pos.y is world-space)
                                                        let linked_room_y = state.level.rooms.get(*ri).map(|r| r.position.y).unwrap_or(0.0);
                                                        state.drag_initial_heights.push(pos.y - linked_room_y);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // Handle wall dragging
                                match face {
                                    SectorFace::WallNorth(i) | SectorFace::WallEast(i) |
                                    SectorFace::WallSouth(i) | SectorFace::WallWest(i) |
                                    SectorFace::WallNwSe(i) | SectorFace::WallNeSw(i) => {
                                        let walls = match face {
                                            SectorFace::WallNorth(_) => &sector.walls_north,
                                            SectorFace::WallEast(_) => &sector.walls_east,
                                            SectorFace::WallSouth(_) => &sector.walls_south,
                                            SectorFace::WallWest(_) => &sector.walls_west,
                                            SectorFace::WallNwSe(_) => &sector.walls_nwse,
                                            SectorFace::WallNeSw(_) => &sector.walls_nesw,
                                            _ => unreachable!(),
                                        };
                                        if let Some(wall) = walls.get(*i) {
                                            for corner in 0..4 {
                                                let key = (*r_idx, *gx, *gz, *face, corner);
                                                if !state.dragging_sector_vertices.contains(&key) {
                                                    state.dragging_sector_vertices.push(key);
                                                    state.drag_initial_heights.push(wall.heights[corner]);
                                                }
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }

                    // Set drag plane to average height of ALL vertices (including linked ones)
                    // This prevents jumping when linked vertices have different room-relative heights
                    if !state.drag_initial_heights.is_empty() {
                        state.viewport_drag_plane_y = state.drag_initial_heights.iter().sum::<f32>()
                            / state.drag_initial_heights.len() as f32;
                    }
                } else {
                    // Clicked on nothing - clear selection (unless Shift is held)
                    if !shift_down {
                        // Only save undo if something will actually change
                        if state.selection != Selection::None || !state.multi_selection.is_empty() {
                            state.save_selection_undo();
                            state.set_selection(Selection::None);
                            state.clear_multi_selection();
                        }
                    }
                }
            }
            // Drawing modes - start drag for floor/ceiling placement
            else if state.tool == EditorTool::DrawFloor || state.tool == EditorTool::DrawCeiling {
                if let Some((snapped_x, snapped_z, _target_y, _occupied)) = preview_sector {
                    // Convert world coords to grid coords for drag start
                    if let Some(room) = state.level.rooms.get(state.current_room) {
                        let local_x = snapped_x - room.position.x;
                        let local_z = snapped_z - room.position.z;
                        let gx = (local_x / SECTOR_SIZE).floor() as i32;
                        let gz = (local_z / SECTOR_SIZE).floor() as i32;
                        state.placement_drag_start = Some((gx, gz));
                        state.placement_drag_current = Some((gx, gz));
                    }
                }
            }
            // DrawWall mode - start drag for wall placement
            else if state.tool == EditorTool::DrawWall {
                if let Some((grid_x, grid_z, dir, _corner_heights, wall_state, mouse_y)) = preview_wall {
                    // Only start drag if not fully covered
                    if wall_state != 2 {
                        // Convert world coords to grid coords
                        if let Some(room) = state.level.rooms.get(state.current_room) {
                            let local_x = grid_x - room.position.x;
                            let local_z = grid_z - room.position.z;
                            let gx = (local_x / SECTOR_SIZE).floor() as i32;
                            let gz = (local_z / SECTOR_SIZE).floor() as i32;
                            state.wall_drag_start = Some((gx, gz, dir));
                            state.wall_drag_current = Some((gx, gz, dir));
                            // Capture mouse Y for consistent gap selection during drag
                            state.wall_drag_mouse_y = mouse_y;
                        }
                    } else {
                        state.set_status("Edge is fully covered", 2.0);
                    }
                }
            }
            // DrawDiagonalWall mode - start drag for diagonal wall placement
            else if state.tool == EditorTool::DrawDiagonalWall {
                if let Some((grid_x, grid_z, is_nwse, _corner_heights)) = preview_diagonal_wall {
                    // Convert world coords to grid coords
                    if let Some(room) = state.level.rooms.get(state.current_room) {
                        let local_x = grid_x - room.position.x;
                        let local_z = grid_z - room.position.z;
                        let gx = (local_x / SECTOR_SIZE).floor() as i32;
                        let gz = (local_z / SECTOR_SIZE).floor() as i32;
                        state.diagonal_drag_start = Some((gx, gz, is_nwse));
                        state.diagonal_drag_current = Some((gx, gz, is_nwse));
                    }
                }
            }
            // PlaceObject mode - select existing objects in 3D (placement is in 2D grid view)
            else if state.tool == EditorTool::PlaceObject {
                if let Some((obj_room_idx, obj_idx, _)) = hovered_object {
                    state.save_selection_undo();
                    state.set_selection(Selection::Object { room: obj_room_idx, index: obj_idx });
                    state.set_status("Object selected", 1.0);
                } else {
                    state.set_status("Use 2D grid view to place objects", 2.0);
                }
            }
        }

        // Continue dragging (Y-axis only - TRLE constraint)
        if ctx.mouse.left_down && !state.dragging_sector_vertices.is_empty() {
            use super::CLICK_HEIGHT;

            // Calculate Y delta from mouse movement (inverted: mouse up = positive Y)
            let mouse_delta_y = state.viewport_last_mouse.1 - mouse_pos.1;

            // Only save undo when actual movement happens, not on click
            if !state.viewport_drag_started && mouse_delta_y.abs() > 0.5 {
                state.save_undo();
                state.viewport_drag_started = true;
            }
            let y_sensitivity = 5.0;
            let y_delta = mouse_delta_y * y_sensitivity;

            // Accumulate delta
            state.viewport_drag_plane_y += y_delta;

            // Calculate delta from initial average
            let initial_avg: f32 = state.drag_initial_heights.iter().sum::<f32>()
                / state.drag_initial_heights.len().max(1) as f32;
            let delta_from_initial = state.viewport_drag_plane_y - initial_avg;

            // Apply delta to each vertex
            for (i, &(room_idx, gx, gz, face, corner_idx)) in state.dragging_sector_vertices.clone().iter().enumerate() {
                if let Some(initial_h) = state.drag_initial_heights.get(i) {
                    let new_h = initial_h + delta_from_initial;
                    let snapped_h = (new_h / CLICK_HEIGHT).round() * CLICK_HEIGHT;

                    if let Some(room) = state.level.rooms.get_mut(room_idx) {
                        if let Some(sector) = room.get_sector_mut(gx, gz) {
                            match face {
                                SectorFace::Floor => {
                                    if let Some(floor) = &mut sector.floor {
                                        floor.heights[corner_idx] = snapped_h;
                                    }
                                }
                                SectorFace::Ceiling => {
                                    if let Some(ceiling) = &mut sector.ceiling {
                                        ceiling.heights[corner_idx] = snapped_h;
                                    }
                                }
                                SectorFace::WallNorth(wi) | SectorFace::WallEast(wi) |
                                SectorFace::WallSouth(wi) | SectorFace::WallWest(wi) |
                                SectorFace::WallNwSe(wi) | SectorFace::WallNeSw(wi) => {
                                    let walls = match face {
                                        SectorFace::WallNorth(_) => &mut sector.walls_north,
                                        SectorFace::WallEast(_) => &mut sector.walls_east,
                                        SectorFace::WallSouth(_) => &mut sector.walls_south,
                                        SectorFace::WallWest(_) => &mut sector.walls_west,
                                        SectorFace::WallNwSe(_) => &mut sector.walls_nwse,
                                        SectorFace::WallNeSw(_) => &mut sector.walls_nesw,
                                        _ => unreachable!(),
                                    };
                                    if let Some(wall) = walls.get_mut(wi) {
                                        // Update individual corner height
                                        wall.heights[corner_idx] = snapped_h;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Recalculate bounds while dragging so wireframe updates in real-time
            if let Some(room) = state.level.rooms.get_mut(state.current_room) {
                room.recalculate_bounds();
            }
        }

        // Continue dragging object (Y-axis only)
        if ctx.mouse.left_down && state.dragging_object.is_some() {
            use super::CLICK_HEIGHT;

            if !state.viewport_drag_started {
                state.save_undo();
                state.viewport_drag_started = true;
            }

            // Calculate Y delta from mouse movement (inverted: mouse up = positive Y)
            let mouse_delta_y = state.viewport_last_mouse.1 - mouse_pos.1;
            let y_sensitivity = 5.0;
            let y_delta = mouse_delta_y * y_sensitivity;

            // Accumulate delta
            state.dragging_object_plane_y += y_delta;

            // Calculate snapped height
            let delta_from_initial = state.dragging_object_plane_y - state.dragging_object_initial_y;
            let new_y = state.dragging_object_initial_y + delta_from_initial;
            let snapped_y = (new_y / CLICK_HEIGHT).round() * CLICK_HEIGHT;

            // Apply to object's height offset
            if let Some((obj_room_idx, obj_idx)) = state.dragging_object {
                // We need sector info first, then we can update the object
                let sector_floor_y = state.level.rooms.get(obj_room_idx)
                    .and_then(|room| {
                        room.objects.get(obj_idx).and_then(|obj| {
                            room.get_sector(obj.sector_x, obj.sector_z)
                                .and_then(|s| s.floor.as_ref())
                                .map(|f| f.avg_height())
                                .or(Some(room.position.y))
                        })
                    });

                if let Some(floor_y) = sector_floor_y {
                    if let Some(obj) = state.level.get_object_mut(obj_room_idx, obj_idx) {
                        obj.height = snapped_y - floor_y;
                    }
                }
            }
        }

        // Continue placement drag (update current grid position)
        if ctx.mouse.left_down && state.placement_drag_start.is_some() {
            if let Some((snapped_x, snapped_z, _, _)) = preview_sector {
                if let Some(room) = state.level.rooms.get(state.current_room) {
                    let local_x = snapped_x - room.position.x;
                    let local_z = snapped_z - room.position.z;
                    let gx = (local_x / SECTOR_SIZE).floor() as i32;
                    let gz = (local_z / SECTOR_SIZE).floor() as i32;
                    state.placement_drag_current = Some((gx, gz));
                }
            }
        }

        // Continue wall drag (update current grid position based on mouse, locked to start direction)
        if ctx.mouse.left_down {
            if let Some((start_gx, start_gz, start_dir)) = state.wall_drag_start {
                use crate::world::Direction;

                // Get mouse grid position from preview_wall or preview_sector
                let mouse_grid_pos = if let Some((grid_x, grid_z, _, _, _, _)) = preview_wall {
                    if let Some(room) = state.level.rooms.get(state.current_room) {
                        let local_x = grid_x - room.position.x;
                        let local_z = grid_z - room.position.z;
                        Some(((local_x / SECTOR_SIZE).floor() as i32, (local_z / SECTOR_SIZE).floor() as i32))
                    } else { None }
                } else if let Some((snapped_x, snapped_z, _, _)) = preview_sector {
                    if let Some(room) = state.level.rooms.get(state.current_room) {
                        let local_x = snapped_x - room.position.x;
                        let local_z = snapped_z - room.position.z;
                        Some(((local_x / SECTOR_SIZE).floor() as i32, (local_z / SECTOR_SIZE).floor() as i32))
                    } else { None }
                } else { None };

                if let Some((gx, gz)) = mouse_grid_pos {
                    // Lock to the axis based on wall direction
                    // North/South walls extend along X axis (fixed Z)
                    // East/West walls extend along Z axis (fixed X)
                    let (final_gx, final_gz) = match start_dir {
                        Direction::North | Direction::South => (gx, start_gz),
                        Direction::East | Direction::West => (start_gx, gz),
                    };
                    state.wall_drag_current = Some((final_gx, final_gz, start_dir));
                }
            }
        }

        // Continue diagonal wall drag (update current grid position, locked to diagonal movement)
        if ctx.mouse.left_down {
            if let Some((start_gx, start_gz, start_is_nwse)) = state.diagonal_drag_start {
                // Get mouse grid position from preview_diagonal_wall or preview_sector
                let mouse_grid_pos = if let Some((grid_x, grid_z, _, _)) = preview_diagonal_wall {
                    if let Some(room) = state.level.rooms.get(state.current_room) {
                        let local_x = grid_x - room.position.x;
                        let local_z = grid_z - room.position.z;
                        Some(((local_x / SECTOR_SIZE).floor() as i32, (local_z / SECTOR_SIZE).floor() as i32))
                    } else { None }
                } else if let Some((snapped_x, snapped_z, _, _)) = preview_sector {
                    if let Some(room) = state.level.rooms.get(state.current_room) {
                        let local_x = snapped_x - room.position.x;
                        let local_z = snapped_z - room.position.z;
                        Some(((local_x / SECTOR_SIZE).floor() as i32, (local_z / SECTOR_SIZE).floor() as i32))
                    } else { None }
                } else { None };

                if let Some((mouse_gx, mouse_gz)) = mouse_grid_pos {
                    // Lock to diagonal movement: both X and Z must change together
                    // Use the axis with the larger delta to determine the diagonal length
                    let dx = mouse_gx - start_gx;
                    let dz = mouse_gz - start_gz;

                    // NW-SE diagonal: +X goes with +Z, -X goes with -Z
                    // NE-SW diagonal: +X goes with -Z, -X goes with +Z
                    let diag_len = dx.abs().max(dz.abs());

                    // Determine the diagonal direction based on which quadrant the mouse is in
                    let (final_gx, final_gz) = if start_is_nwse {
                        // NW-SE: X and Z move in same direction
                        // Use the primary movement direction (larger delta)
                        if dx.abs() >= dz.abs() {
                            // X is primary
                            let sign = if dx >= 0 { 1 } else { -1 };
                            (start_gx + sign * diag_len, start_gz + sign * diag_len)
                        } else {
                            // Z is primary
                            let sign = if dz >= 0 { 1 } else { -1 };
                            (start_gx + sign * diag_len, start_gz + sign * diag_len)
                        }
                    } else {
                        // NE-SW: X and Z move in opposite directions
                        if dx.abs() >= dz.abs() {
                            // X is primary
                            let sign = if dx >= 0 { 1 } else { -1 };
                            (start_gx + sign * diag_len, start_gz - sign * diag_len)
                        } else {
                            // Z is primary
                            let sign = if dz >= 0 { 1 } else { -1 };
                            (start_gx - sign * diag_len, start_gz + sign * diag_len)
                        }
                    };

                    state.diagonal_drag_current = Some((final_gx, final_gz, start_is_nwse));
                }
            }
        }

        // End dragging on release
        if ctx.mouse.left_released {
            // Handle placement drag completion (floor/ceiling)
            if let (Some((start_gx, start_gz)), Some((end_gx, end_gz))) = (state.placement_drag_start, state.placement_drag_current) {
                if state.tool == EditorTool::DrawFloor || state.tool == EditorTool::DrawCeiling {
                    let is_floor = state.tool == EditorTool::DrawFloor;

                    // Calculate rectangle bounds
                    let min_gx = start_gx.min(end_gx);
                    let max_gx = start_gx.max(end_gx);
                    let min_gz = start_gz.min(end_gz);
                    let max_gz = start_gz.max(end_gz);

                    // Get target Y and texture
                    let target_y = if state.placement_target_y == 0.0 && !state.height_adjust_mode {
                        if is_floor { 0.0 } else { super::CEILING_HEIGHT }
                    } else {
                        state.placement_target_y
                    };
                    let texture = state.selected_texture.clone();

                    state.save_undo();

                    // Place all sectors in the rectangle
                    let mut placed_count = 0;
                    if let Some(room) = state.level.rooms.get_mut(state.current_room) {
                        // First, expand the room grid to accommodate all sectors
                        // Handle negative coordinates by expanding the grid
                        let mut offset_x = 0i32;
                        let mut offset_z = 0i32;

                        // Expand in negative X direction
                        while min_gx + offset_x < 0 {
                            room.position.x -= SECTOR_SIZE;
                            room.sectors.insert(0, (0..room.depth).map(|_| None).collect());
                            room.width += 1;
                            offset_x += 1;
                        }

                        // Expand in negative Z direction
                        while min_gz + offset_z < 0 {
                            room.position.z -= SECTOR_SIZE;
                            for col in &mut room.sectors {
                                col.insert(0, None);
                            }
                            room.depth += 1;
                            offset_z += 1;
                        }

                        // Expand in positive X direction
                        while (max_gx + offset_x) as usize >= room.width {
                            room.width += 1;
                            room.sectors.push((0..room.depth).map(|_| None).collect());
                        }

                        // Expand in positive Z direction
                        while (max_gz + offset_z) as usize >= room.depth {
                            room.depth += 1;
                            for col in &mut room.sectors {
                                col.push(None);
                            }
                        }

                        // Now place all sectors (with adjusted coordinates)
                        for gx in min_gx..=max_gx {
                            for gz in min_gz..=max_gz {
                                let adjusted_gx = (gx + offset_x) as usize;
                                let adjusted_gz = (gz + offset_z) as usize;

                                // Check if already occupied
                                let occupied = room.get_sector(adjusted_gx, adjusted_gz)
                                    .map(|s| if is_floor { s.floor.is_some() } else { s.ceiling.is_some() })
                                    .unwrap_or(false);

                                if !occupied {
                                    if is_floor {
                                        room.set_floor(adjusted_gx, adjusted_gz, target_y, texture.clone());
                                    } else {
                                        room.set_ceiling(adjusted_gx, adjusted_gz, target_y, texture.clone());
                                    }
                                    placed_count += 1;
                                }
                            }
                        }
                        room.recalculate_bounds();
                    }

                    state.mark_portals_dirty();
                    if placed_count > 0 {
                        let type_name = if is_floor { "floor" } else { "ceiling" };
                        state.set_status(&format!("Created {} {} sectors", placed_count, type_name), 2.0);
                    }
                }

                // Clear drag state
                state.placement_drag_start = None;
                state.placement_drag_current = None;
            }

            // Handle wall drag completion
            if let (Some((start_gx, start_gz, dir)), Some((end_gx, end_gz, _))) = (state.wall_drag_start, state.wall_drag_current) {
                use crate::world::Direction;
                use super::CEILING_HEIGHT;

                // Calculate the line of walls to place based on direction
                let (iter_axis_start, iter_axis_end, fixed_axis) = match dir {
                    Direction::North | Direction::South => {
                        // Horizontal wall - iterate along X axis
                        (start_gx, end_gx, start_gz)
                    }
                    Direction::East | Direction::West => {
                        // Vertical wall - iterate along Z axis
                        (start_gz, end_gz, start_gx)
                    }
                };

                let min_iter = iter_axis_start.min(iter_axis_end);
                let max_iter = iter_axis_start.max(iter_axis_end);

                state.save_undo();
                let mut placed_count = 0;

                if let Some(room) = state.level.rooms.get_mut(state.current_room) {
                    let texture = state.selected_texture.clone();

                    // Calculate min/max grid coordinates for all walls
                    let (min_gx, max_gx, min_gz, max_gz) = match dir {
                        Direction::North | Direction::South => {
                            (min_iter, max_iter, fixed_axis, fixed_axis)
                        }
                        Direction::East | Direction::West => {
                            (fixed_axis, fixed_axis, min_iter, max_iter)
                        }
                    };

                    // Expand the room grid to accommodate all walls (like floor/ceiling does)
                    let mut offset_x = 0i32;
                    let mut offset_z = 0i32;

                    // Expand in negative X direction
                    while min_gx + offset_x < 0 {
                        room.position.x -= SECTOR_SIZE;
                        room.sectors.insert(0, (0..room.depth).map(|_| None).collect());
                        room.width += 1;
                        offset_x += 1;
                    }

                    // Expand in negative Z direction
                    while min_gz + offset_z < 0 {
                        room.position.z -= SECTOR_SIZE;
                        for col in &mut room.sectors {
                            col.insert(0, None);
                        }
                        room.depth += 1;
                        offset_z += 1;
                    }

                    // Expand in positive X direction
                    while (max_gx + offset_x) as usize >= room.width {
                        room.width += 1;
                        room.sectors.push((0..room.depth).map(|_| None).collect());
                    }

                    // Expand in positive Z direction
                    while (max_gz + offset_z) as usize >= room.depth {
                        room.depth += 1;
                        for col in &mut room.sectors {
                            col.push(None);
                        }
                    }

                    // Now place all walls (with adjusted coordinates)
                    for i in min_iter..=max_iter {
                        let (gx, gz) = match dir {
                            Direction::North | Direction::South => (i, fixed_axis),
                            Direction::East | Direction::West => (fixed_axis, i),
                        };

                        let adjusted_gx = (gx + offset_x) as usize;
                        let adjusted_gz = (gz + offset_z) as usize;

                        // Check if there's a gap to fill (handles both empty edges and gaps between walls)
                        // Use the stored mouse_y from drag start for consistent gap selection
                        room.ensure_sector(adjusted_gx, adjusted_gz);
                        if let Some(sector) = room.get_sector(adjusted_gx, adjusted_gz) {
                            if let Some(heights) = sector.next_wall_position(dir, 0.0, CEILING_HEIGHT, state.wall_drag_mouse_y) {
                                // There's a gap - add wall with computed heights
                                if let Some(sector_mut) = room.get_sector_mut(adjusted_gx, adjusted_gz) {
                                    sector_mut.walls_mut(dir).push(
                                        crate::world::VerticalFace::new_sloped(
                                            heights[0], heights[1], heights[2], heights[3],
                                            texture.clone()
                                        )
                                    );
                                    placed_count += 1;
                                }
                            }
                        }
                    }
                    room.recalculate_bounds();
                }

                state.mark_portals_dirty();
                if placed_count > 0 {
                    let dir_name = match dir {
                        Direction::North => "north",
                        Direction::East => "east",
                        Direction::South => "south",
                        Direction::West => "west",
                    };
                    state.set_status(&format!("Created {} {} walls", placed_count, dir_name), 2.0);
                }

                // Clear wall drag state
                state.wall_drag_start = None;
                state.wall_drag_current = None;
                state.wall_drag_mouse_y = None;
            }

            // Handle diagonal wall drag completion
            if let (Some((start_gx, start_gz, is_nwse)), Some((end_gx, end_gz, _))) = (state.diagonal_drag_start, state.diagonal_drag_current) {
                use crate::world::VerticalFace;
                use super::CEILING_HEIGHT;

                // Calculate the diagonal line of walls to place
                // Diagonals follow a diagonal line from start to end
                let min_gx = start_gx.min(end_gx);
                let max_gx = start_gx.max(end_gx);
                let min_gz = start_gz.min(end_gz);
                let max_gz = start_gz.max(end_gz);

                state.save_undo();
                let mut placed_count = 0;

                if let Some(room) = state.level.rooms.get_mut(state.current_room) {
                    let texture = state.selected_texture.clone();

                    // Expand the room grid to accommodate all diagonals (like floor/ceiling does)
                    let mut offset_x = 0i32;
                    let mut offset_z = 0i32;

                    // Expand in negative X direction
                    while min_gx + offset_x < 0 {
                        room.position.x -= SECTOR_SIZE;
                        room.sectors.insert(0, (0..room.depth).map(|_| None).collect());
                        room.width += 1;
                        offset_x += 1;
                    }

                    // Expand in negative Z direction
                    while min_gz + offset_z < 0 {
                        room.position.z -= SECTOR_SIZE;
                        for col in &mut room.sectors {
                            col.insert(0, None);
                        }
                        room.depth += 1;
                        offset_z += 1;
                    }

                    // Expand in positive X direction
                    while (max_gx + offset_x) as usize >= room.width {
                        room.width += 1;
                        room.sectors.push((0..room.depth).map(|_| None).collect());
                    }

                    // Expand in positive Z direction
                    while (max_gz + offset_z) as usize >= room.depth {
                        room.depth += 1;
                        for col in &mut room.sectors {
                            col.push(None);
                        }
                    }

                    // Place diagonal walls along a true diagonal LINE
                    // Both X and Z step together at the same rate (45-degree grid diagonal)
                    let sx = if start_gx < end_gx { 1 } else if start_gx > end_gx { -1 } else { 0 };
                    let sz = if start_gz < end_gz { 1 } else if start_gz > end_gz { -1 } else { 0 };
                    let steps = (end_gx - start_gx).abs().max((end_gz - start_gz).abs());

                    for i in 0..=steps {
                        let gx = start_gx + sx * i;
                        let gz = start_gz + sz * i;

                        let adjusted_gx = (gx + offset_x) as usize;
                        let adjusted_gz = (gz + offset_z) as usize;

                        // Check if there's a gap to fill (handles both empty diagonals and gaps)
                        room.ensure_sector(adjusted_gx, adjusted_gz);
                        if let Some(sector) = room.get_sector(adjusted_gx, adjusted_gz) {
                            if let Some(heights) = sector.next_diagonal_wall_position(is_nwse, 0.0, CEILING_HEIGHT, None) {
                                // There's a gap - add wall with computed heights
                                if let Some(sector_mut) = room.get_sector_mut(adjusted_gx, adjusted_gz) {
                                    let wall = VerticalFace::new_sloped(
                                        heights[0], heights[1], heights[2], heights[3],
                                        texture.clone()
                                    );
                                    if is_nwse {
                                        sector_mut.walls_nwse.push(wall);
                                    } else {
                                        sector_mut.walls_nesw.push(wall);
                                    }
                                    placed_count += 1;
                                }
                            }
                        }
                    }
                    room.recalculate_bounds();
                }

                state.mark_portals_dirty();
                if placed_count > 0 {
                    let type_name = if is_nwse { "NW-SE" } else { "NE-SW" };
                    state.set_status(&format!("Created {} {} diagonal walls", placed_count, type_name), 2.0);
                }

                // Clear diagonal drag state
                state.diagonal_drag_start = None;
                state.diagonal_drag_current = None;
            }

            // If we actually dragged geometry, recalculate room bounds
            if state.viewport_drag_started {
                if let Some(room) = state.level.rooms.get_mut(state.current_room) {
                    room.recalculate_bounds();
                }
                state.mark_portals_dirty();
            }
            state.dragging_sector_vertices.clear();
            state.drag_initial_heights.clear();
            state.dragging_object = None;
            state.viewport_drag_started = false;
        }
    }

    // Update mouse position for next frame
    state.viewport_last_mouse = mouse_pos;

    // Update orbit target after selection changes (in orbit mode)
    if should_update_orbit_target {
        state.update_orbit_target();
        state.sync_camera_from_orbit();
    }

    // Clear framebuffer - use 3D skybox if configured
    if let Some(skybox) = &state.level.skybox {
        fb.clear(RasterColor::new(0, 0, 0));
        let time = macroquad::prelude::get_time() as f32;
        fb.render_skybox(skybox, &state.camera_3d, time);
    } else {
        fb.clear(RasterColor::new(30, 30, 40));
    }

    // Draw main floor grid (large, fixed extent)
    if state.show_grid {
        let grid_color = RasterColor::new(50, 50, 60);
        let grid_size = state.grid_size;
        let grid_extent = 10240.0; // Cover approximately 10 sectors in each direction

        // Calculate grid Y position based on lowest floor point in selected room
        let grid_y = if let Some(room) = state.level.rooms.get(state.current_room) {
            // Find lowest floor height in the room
            let mut lowest = f32::INFINITY;
            for row in &room.sectors {
                for sector_opt in row {
                    if let Some(sector) = sector_opt {
                        if let Some(floor) = &sector.floor {
                            for &h in &floor.heights {
                                if h < lowest {
                                    lowest = h;
                                }
                            }
                        }
                    }
                }
            }
            // If no floors found, use 0.0 relative to room
            let floor_offset = if lowest.is_finite() { lowest } else { 0.0 };
            room.position.y + floor_offset
        } else {
            0.0
        };

        // Draw grid lines - use shorter segments for better clipping behavior
        let segment_length: f32 = SECTOR_SIZE;

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

        // Draw origin axes (slightly brighter)
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

    // Draw hovering floor grid centered on hovered tile when in floor placement mode
    if let Some((snapped_x, snapped_z, _, _)) = preview_sector {
        if state.tool == EditorTool::DrawFloor {
            let inner_color = RasterColor::new(80, 180, 160); // Teal (bright)
            let outer_color = RasterColor::new(40, 90, 80);   // Teal (dim)

            // Get room Y offset for correct world-space positioning
            let grid_y = state.level.rooms.get(state.current_room)
                .map(|r| r.position.y)
                .unwrap_or(0.0);

            // Center of the hovered sector (snap to grid)
            let center_x = (snapped_x / SECTOR_SIZE).floor() * SECTOR_SIZE + SECTOR_SIZE * 0.5;
            let center_z = (snapped_z / SECTOR_SIZE).floor() * SECTOR_SIZE + SECTOR_SIZE * 0.5;

            let inner_half = SECTOR_SIZE * 1.5; // Inner 3x3
            let outer_half = SECTOR_SIZE * 2.5; // Outer 5x5

            // Draw grid lines - 6 lines in each direction for 5x5 grid
            for i in 0..=5 {
                let offset = -outer_half + (i as f32 * SECTOR_SIZE);
                let dist_from_center = offset.abs();

                let color = if dist_from_center <= inner_half {
                    inner_color
                } else {
                    outer_color
                };

                let z = center_z + offset;
                draw_3d_line(
                    fb,
                    Vec3::new(center_x - outer_half, grid_y, z),
                    Vec3::new(center_x + outer_half, grid_y, z),
                    &state.camera_3d,
                    color,
                );

                let x = center_x + offset;
                draw_3d_line(
                    fb,
                    Vec3::new(x, grid_y, center_z - outer_half),
                    Vec3::new(x, grid_y, center_z + outer_half),
                    &state.camera_3d,
                    color,
                );
            }

            // Draw vertex indicators at the 4 corners of the hovered sector
            let sector_x = (snapped_x / SECTOR_SIZE).floor() * SECTOR_SIZE;
            let sector_z = (snapped_z / SECTOR_SIZE).floor() * SECTOR_SIZE;
            let vertex_color = RasterColor::new(255, 255, 255); // White
            draw_3d_point(fb, Vec3::new(sector_x, grid_y, sector_z), &state.camera_3d, 3, vertex_color);
            draw_3d_point(fb, Vec3::new(sector_x + SECTOR_SIZE, grid_y, sector_z), &state.camera_3d, 3, vertex_color);
            draw_3d_point(fb, Vec3::new(sector_x + SECTOR_SIZE, grid_y, sector_z + SECTOR_SIZE), &state.camera_3d, 3, vertex_color);
            draw_3d_point(fb, Vec3::new(sector_x, grid_y, sector_z + SECTOR_SIZE), &state.camera_3d, 3, vertex_color);
        }
    }

    // Draw hovering ceiling grid centered on hovered tile when in ceiling placement mode
    if let Some((snapped_x, snapped_z, _, _)) = preview_sector {
        if state.tool == EditorTool::DrawCeiling {
            use super::CEILING_HEIGHT;

            let inner_color = RasterColor::new(140, 100, 180); // Purple (bright)
            let outer_color = RasterColor::new(70, 50, 90);    // Purple (dim)

            // Get room Y offset for correct world-space positioning
            let room_y = state.level.rooms.get(state.current_room)
                .map(|r| r.position.y)
                .unwrap_or(0.0);
            let ceiling_y = room_y + CEILING_HEIGHT;

            let center_x = (snapped_x / SECTOR_SIZE).floor() * SECTOR_SIZE + SECTOR_SIZE * 0.5;
            let center_z = (snapped_z / SECTOR_SIZE).floor() * SECTOR_SIZE + SECTOR_SIZE * 0.5;

            let inner_half = SECTOR_SIZE * 1.5;
            let outer_half = SECTOR_SIZE * 2.5;

            for i in 0..=5 {
                let offset = -outer_half + (i as f32 * SECTOR_SIZE);
                let dist_from_center = offset.abs();

                let color = if dist_from_center <= inner_half {
                    inner_color
                } else {
                    outer_color
                };

                let z = center_z + offset;
                draw_3d_line(
                    fb,
                    Vec3::new(center_x - outer_half, ceiling_y, z),
                    Vec3::new(center_x + outer_half, ceiling_y, z),
                    &state.camera_3d,
                    color,
                );

                let x = center_x + offset;
                draw_3d_line(
                    fb,
                    Vec3::new(x, ceiling_y, center_z - outer_half),
                    Vec3::new(x, ceiling_y, center_z + outer_half),
                    &state.camera_3d,
                    color,
                );
            }

            // Draw vertex indicators at the 4 corners of the hovered sector
            let sector_x = (snapped_x / SECTOR_SIZE).floor() * SECTOR_SIZE;
            let sector_z = (snapped_z / SECTOR_SIZE).floor() * SECTOR_SIZE;
            let vertex_color = RasterColor::new(255, 255, 255); // White
            draw_3d_point(fb, Vec3::new(sector_x, ceiling_y, sector_z), &state.camera_3d, 3, vertex_color);
            draw_3d_point(fb, Vec3::new(sector_x + SECTOR_SIZE, ceiling_y, sector_z), &state.camera_3d, 3, vertex_color);
            draw_3d_point(fb, Vec3::new(sector_x + SECTOR_SIZE, ceiling_y, sector_z + SECTOR_SIZE), &state.camera_3d, 3, vertex_color);
            draw_3d_point(fb, Vec3::new(sector_x, ceiling_y, sector_z + SECTOR_SIZE), &state.camera_3d, 3, vertex_color);
        }
    }

    // Draw drag rectangle preview for floor/ceiling placement
    if let (Some((start_gx, start_gz)), Some((end_gx, end_gz))) = (state.placement_drag_start, state.placement_drag_current) {
        if state.tool == EditorTool::DrawFloor || state.tool == EditorTool::DrawCeiling {
            let is_floor = state.tool == EditorTool::DrawFloor;

            // Get room position and calculate Y height
            let room_pos = state.level.rooms.get(state.current_room)
                .map(|r| r.position)
                .unwrap_or_default();

            let target_y = if state.placement_target_y == 0.0 && !state.height_adjust_mode {
                if is_floor { 0.0 } else { super::CEILING_HEIGHT }
            } else {
                state.placement_target_y
            };
            let grid_y = room_pos.y + target_y;

            // Calculate rectangle bounds in world space
            let min_gx = start_gx.min(end_gx);
            let max_gx = start_gx.max(end_gx);
            let min_gz = start_gz.min(end_gz);
            let max_gz = start_gz.max(end_gz);

            let world_min_x = room_pos.x + (min_gx as f32) * SECTOR_SIZE;
            let world_max_x = room_pos.x + ((max_gx + 1) as f32) * SECTOR_SIZE;
            let world_min_z = room_pos.z + (min_gz as f32) * SECTOR_SIZE;
            let world_max_z = room_pos.z + ((max_gz + 1) as f32) * SECTOR_SIZE;

            // Choose color based on floor vs ceiling
            let rect_color = if is_floor {
                RasterColor::new(100, 255, 200) // Bright teal for floor
            } else {
                RasterColor::new(180, 140, 255) // Bright purple for ceiling
            };

            // Draw rectangle outline
            draw_3d_line(fb, Vec3::new(world_min_x, grid_y, world_min_z), Vec3::new(world_max_x, grid_y, world_min_z), &state.camera_3d, rect_color);
            draw_3d_line(fb, Vec3::new(world_max_x, grid_y, world_min_z), Vec3::new(world_max_x, grid_y, world_max_z), &state.camera_3d, rect_color);
            draw_3d_line(fb, Vec3::new(world_max_x, grid_y, world_max_z), Vec3::new(world_min_x, grid_y, world_max_z), &state.camera_3d, rect_color);
            draw_3d_line(fb, Vec3::new(world_min_x, grid_y, world_max_z), Vec3::new(world_min_x, grid_y, world_min_z), &state.camera_3d, rect_color);

            // Draw internal grid lines
            for gx in min_gx..=max_gx {
                let x = room_pos.x + ((gx + 1) as f32) * SECTOR_SIZE;
                if gx < max_gx {
                    draw_3d_line(fb, Vec3::new(x, grid_y, world_min_z), Vec3::new(x, grid_y, world_max_z), &state.camera_3d, rect_color);
                }
            }
            for gz in min_gz..=max_gz {
                let z = room_pos.z + ((gz + 1) as f32) * SECTOR_SIZE;
                if gz < max_gz {
                    draw_3d_line(fb, Vec3::new(world_min_x, grid_y, z), Vec3::new(world_max_x, grid_y, z), &state.camera_3d, rect_color);
                }
            }

            // Draw vertex indicators at the 4 corners of the rectangle
            let vertex_color = RasterColor::new(255, 255, 255); // White
            draw_3d_point(fb, Vec3::new(world_min_x, grid_y, world_min_z), &state.camera_3d, 4, vertex_color);
            draw_3d_point(fb, Vec3::new(world_max_x, grid_y, world_min_z), &state.camera_3d, 4, vertex_color);
            draw_3d_point(fb, Vec3::new(world_max_x, grid_y, world_max_z), &state.camera_3d, 4, vertex_color);
            draw_3d_point(fb, Vec3::new(world_min_x, grid_y, world_max_z), &state.camera_3d, 4, vertex_color);

            // Show sector count in status
            let width = (max_gx - min_gx + 1) as usize;
            let depth = (max_gz - min_gz + 1) as usize;
            let type_name = if is_floor { "floor" } else { "ceiling" };
            state.set_status(&format!("Drag to place {} ({}x{} = {} sectors)", type_name, width, depth, width * depth), 0.1);
        }
    }

    // Draw wall drag preview
    if let (Some((start_gx, start_gz, dir)), Some((end_gx, end_gz, _))) = (state.wall_drag_start, state.wall_drag_current) {
        use crate::world::Direction;

        // Get room position
        let room_pos = state.level.rooms.get(state.current_room)
            .map(|r| r.position)
            .unwrap_or_default();

        // Calculate the line of walls based on direction
        let (iter_axis_start, iter_axis_end, fixed_axis) = match dir {
            Direction::North | Direction::South => (start_gx, end_gx, start_gz),
            Direction::East | Direction::West => (start_gz, end_gz, start_gx),
        };

        let min_iter = iter_axis_start.min(iter_axis_end);
        let max_iter = iter_axis_start.max(iter_axis_end);

        let new_wall_color = RasterColor::new(80, 200, 180); // Teal - new wall
        let gap_fill_color = RasterColor::new(255, 180, 80); // Orange - filling gap
        let vertex_color = RasterColor::new(255, 255, 255); // White

        for i in min_iter..=max_iter {
            let (gx, gz) = match dir {
                Direction::North | Direction::South => (i, fixed_axis),
                Direction::East | Direction::West => (fixed_axis, i),
            };

            // Calculate world position for this grid cell
            let grid_x = room_pos.x + (gx as f32) * SECTOR_SIZE;
            let grid_z = room_pos.z + (gz as f32) * SECTOR_SIZE;

            // Check if there's a sector with existing walls - use gap heights if so
            let (corner_heights, is_gap_fill) = if gx >= 0 && gz >= 0 {
                if let Some(room) = state.level.rooms.get(state.current_room) {
                    if let Some(sector) = room.get_sector(gx as usize, gz as usize) {
                        // Check if edge has walls and find the gap
                        // Use the stored mouse_y from drag start for consistent gap selection
                        let has_walls = !sector.walls(dir).is_empty();
                        if let Some(heights) = sector.next_wall_position(dir, 0.0, super::CEILING_HEIGHT, state.wall_drag_mouse_y) {
                            (heights, has_walls)
                        } else {
                            // Fully covered - skip this segment
                            continue;
                        }
                    } else {
                        // No sector - use default heights
                        ([0.0, 0.0, super::CEILING_HEIGHT, super::CEILING_HEIGHT], false)
                    }
                } else {
                    ([0.0, 0.0, super::CEILING_HEIGHT, super::CEILING_HEIGHT], false)
                }
            } else {
                // Negative coordinates - use default heights
                ([0.0, 0.0, super::CEILING_HEIGHT, super::CEILING_HEIGHT], false)
            };

            let wall_color = if is_gap_fill { gap_fill_color } else { new_wall_color };

            // Wall corners based on direction, using computed corner heights
            // corner_heights: [bottom-left, bottom-right, top-right, top-left]
            let (p0, p1, p2, p3) = match dir {
                Direction::North => (
                    Vec3::new(grid_x, room_pos.y + corner_heights[0], grid_z),
                    Vec3::new(grid_x + SECTOR_SIZE, room_pos.y + corner_heights[1], grid_z),
                    Vec3::new(grid_x + SECTOR_SIZE, room_pos.y + corner_heights[2], grid_z),
                    Vec3::new(grid_x, room_pos.y + corner_heights[3], grid_z),
                ),
                Direction::East => (
                    Vec3::new(grid_x + SECTOR_SIZE, room_pos.y + corner_heights[0], grid_z),
                    Vec3::new(grid_x + SECTOR_SIZE, room_pos.y + corner_heights[1], grid_z + SECTOR_SIZE),
                    Vec3::new(grid_x + SECTOR_SIZE, room_pos.y + corner_heights[2], grid_z + SECTOR_SIZE),
                    Vec3::new(grid_x + SECTOR_SIZE, room_pos.y + corner_heights[3], grid_z),
                ),
                Direction::South => (
                    Vec3::new(grid_x + SECTOR_SIZE, room_pos.y + corner_heights[0], grid_z + SECTOR_SIZE),
                    Vec3::new(grid_x, room_pos.y + corner_heights[1], grid_z + SECTOR_SIZE),
                    Vec3::new(grid_x, room_pos.y + corner_heights[2], grid_z + SECTOR_SIZE),
                    Vec3::new(grid_x + SECTOR_SIZE, room_pos.y + corner_heights[3], grid_z + SECTOR_SIZE),
                ),
                Direction::West => (
                    Vec3::new(grid_x, room_pos.y + corner_heights[0], grid_z + SECTOR_SIZE),
                    Vec3::new(grid_x, room_pos.y + corner_heights[1], grid_z),
                    Vec3::new(grid_x, room_pos.y + corner_heights[2], grid_z),
                    Vec3::new(grid_x, room_pos.y + corner_heights[3], grid_z + SECTOR_SIZE),
                ),
            };

            // Draw wall outline
            draw_3d_line(fb, p0, p1, &state.camera_3d, wall_color);
            draw_3d_line(fb, p1, p2, &state.camera_3d, wall_color);
            draw_3d_line(fb, p2, p3, &state.camera_3d, wall_color);
            draw_3d_line(fb, p3, p0, &state.camera_3d, wall_color);

            // Draw vertex indicators
            draw_3d_point(fb, p0, &state.camera_3d, 3, vertex_color);
            draw_3d_point(fb, p1, &state.camera_3d, 3, vertex_color);
            draw_3d_point(fb, p2, &state.camera_3d, 3, vertex_color);
            draw_3d_point(fb, p3, &state.camera_3d, 3, vertex_color);

            // Draw + through it if filling gap to indicate addition
            if is_gap_fill {
                // Vertical line (center)
                let mid_x = (p0.x + p1.x) / 2.0;
                let mid_z = (p0.z + p1.z) / 2.0;
                let center_bottom = Vec3::new(mid_x, room_pos.y + (corner_heights[0] + corner_heights[1]) / 2.0, mid_z);
                let center_top = Vec3::new(mid_x, room_pos.y + (corner_heights[2] + corner_heights[3]) / 2.0, mid_z);
                draw_3d_line(fb, center_bottom, center_top, &state.camera_3d, wall_color);
                // Horizontal line (middle height)
                let mid_y = room_pos.y + (corner_heights[0] + corner_heights[1] + corner_heights[2] + corner_heights[3]) / 4.0;
                let left = Vec3::new(p0.x, mid_y, p0.z);
                let right = Vec3::new(p1.x, mid_y, p1.z);
                draw_3d_line(fb, left, right, &state.camera_3d, wall_color);
            }
        }

        // Show wall count in status
        let wall_count = (max_iter - min_iter + 1) as usize;
        let dir_name = match dir {
            Direction::North => "north",
            Direction::East => "east",
            Direction::South => "south",
            Direction::West => "west",
        };
        state.set_status(&format!("Drag to place {} {} walls", wall_count, dir_name), 0.1);
    }

    // Draw diagonal wall drag preview
    if let (Some((start_gx, start_gz, is_nwse)), Some((end_gx, end_gz, _))) = (state.diagonal_drag_start, state.diagonal_drag_current) {
        // Get room position
        let room_pos = state.level.rooms.get(state.current_room)
            .map(|r| r.position)
            .unwrap_or_default();

        let diag_color = RasterColor::new(80, 220, 220); // Cyan
        let vertex_color = RasterColor::new(255, 255, 255); // White

        // Simple diagonal line: both X and Z step together at the same rate
        let sx = if start_gx < end_gx { 1 } else if start_gx > end_gx { -1 } else { 0 };
        let sz = if start_gz < end_gz { 1 } else if start_gz > end_gz { -1 } else { 0 };
        let steps = (end_gx - start_gx).abs().max((end_gz - start_gz).abs());

        for i in 0..=steps {
            let gx = start_gx + sx * i;
            let gz = start_gz + sz * i;

            // Calculate world position for this grid cell
            let grid_x = room_pos.x + (gx as f32) * SECTOR_SIZE;
            let grid_z = room_pos.z + (gz as f32) * SECTOR_SIZE;

            // Use default heights (0 to CEILING_HEIGHT)
            let floor_y = room_pos.y;
            let ceiling_y = room_pos.y + super::CEILING_HEIGHT;

            // Diagonal wall corners
            let (p0, p1, p2, p3) = if is_nwse {
                // NW-SE diagonal: from NW corner to SE corner
                (
                    Vec3::new(grid_x, floor_y, grid_z),                               // NW bottom
                    Vec3::new(grid_x + SECTOR_SIZE, floor_y, grid_z + SECTOR_SIZE),   // SE bottom
                    Vec3::new(grid_x + SECTOR_SIZE, ceiling_y, grid_z + SECTOR_SIZE), // SE top
                    Vec3::new(grid_x, ceiling_y, grid_z),                             // NW top
                )
            } else {
                // NE-SW diagonal: from NE corner to SW corner
                (
                    Vec3::new(grid_x + SECTOR_SIZE, floor_y, grid_z),                 // NE bottom
                    Vec3::new(grid_x, floor_y, grid_z + SECTOR_SIZE),                 // SW bottom
                    Vec3::new(grid_x, ceiling_y, grid_z + SECTOR_SIZE),               // SW top
                    Vec3::new(grid_x + SECTOR_SIZE, ceiling_y, grid_z),               // NE top
                )
            };

            // Draw diagonal wall outline
            draw_3d_line(fb, p0, p1, &state.camera_3d, diag_color);
            draw_3d_line(fb, p1, p2, &state.camera_3d, diag_color);
            draw_3d_line(fb, p2, p3, &state.camera_3d, diag_color);
            draw_3d_line(fb, p3, p0, &state.camera_3d, diag_color);
            // Cross pattern to indicate diagonal
            draw_3d_line(fb, p0, p2, &state.camera_3d, diag_color);

            // Draw vertex indicators
            draw_3d_point(fb, p0, &state.camera_3d, 3, vertex_color);
            draw_3d_point(fb, p1, &state.camera_3d, 3, vertex_color);
            draw_3d_point(fb, p2, &state.camera_3d, 3, vertex_color);
            draw_3d_point(fb, p3, &state.camera_3d, 3, vertex_color);
        }

        let diag_count = steps + 1;

        // Show diagonal count in status
        let type_name = if is_nwse { "NW-SE" } else { "NE-SW" };
        state.set_status(&format!("Drag to place {} {} diagonal walls", diag_count, type_name), 0.1);
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

    // Collect all lights from room objects
    let lights: Vec<Light> = if state.raster_settings.shading != crate::rasterizer::ShadingMode::None {
        state.level.rooms.iter()
            .flat_map(|room| {
                room.objects.iter()
                    .filter(|obj| obj.enabled)
                    .filter_map(|obj| {
                        if let crate::world::ObjectType::Light { color, intensity, radius } = &obj.object_type {
                            let world_pos = obj.world_position(room);
                            let mut light = Light::point(world_pos, *radius, *intensity);
                            light.color = *color;
                            Some(light)
                        } else {
                            None
                        }
                    })
            })
            .collect()
    } else {
        Vec::new()
    };

    // Convert textures to RGB555 if enabled
    let use_rgb555 = state.raster_settings.use_rgb555;
    let textures_15: Vec<_> = if use_rgb555 {
        textures.iter().map(|t| t.to_15()).collect()
    } else {
        Vec::new()
    };

    // Render each room with its own ambient setting (skip hidden ones)
    for (room_idx, room) in state.level.rooms.iter().enumerate() {
        // Skip hidden rooms
        if state.hidden_rooms.contains(&room_idx) {
            continue;
        }
        let render_settings = RasterSettings {
            lights: lights.clone(),
            ambient: room.ambient,
            ..state.raster_settings.clone()
        };
        let (vertices, faces) = room.to_render_data_with_textures(&resolve_texture);

        if use_rgb555 {
            render_mesh_15(fb, &vertices, &faces, &textures_15, None, &state.camera_3d, &render_settings);
        } else {
            render_mesh(fb, &vertices, &faces, textures, &state.camera_3d, &render_settings);
        }
    }

    // Draw subtle sector boundary highlight for wall placement
    // Only show if on the drag line (same check as wall preview)
    if let Some((grid_x, grid_z, dir, _, _, _)) = preview_wall {
        // Check if this sector is on the drag line (if dragging)
        let on_drag_line = if let Some((start_gx, start_gz, start_dir)) = state.wall_drag_start {
            use crate::world::Direction;
            // Convert preview world coords to grid coords
            let preview_gx = if let Some(room) = state.level.rooms.get(state.current_room) {
                ((grid_x - room.position.x) / SECTOR_SIZE).floor() as i32
            } else { 0 };
            let preview_gz = if let Some(room) = state.level.rooms.get(state.current_room) {
                ((grid_z - room.position.z) / SECTOR_SIZE).floor() as i32
            } else { 0 };
            // Sector is on the line if direction matches AND the fixed axis matches
            dir == start_dir && match start_dir {
                Direction::North | Direction::South => preview_gz == start_gz,
                Direction::East | Direction::West => preview_gx == start_gx,
            }
        } else {
            true // Not dragging, always show
        };

        if on_drag_line {
            let room_y = state.level.rooms.get(state.current_room)
                .map(|r| r.position.y)
                .unwrap_or(0.0);

            let floor_y = room_y;
            let ceiling_y = room_y + super::CEILING_HEIGHT;

            // Sector corners
            let corners_floor = [
                Vec3::new(grid_x, floor_y, grid_z),
                Vec3::new(grid_x + SECTOR_SIZE, floor_y, grid_z),
                Vec3::new(grid_x + SECTOR_SIZE, floor_y, grid_z + SECTOR_SIZE),
                Vec3::new(grid_x, floor_y, grid_z + SECTOR_SIZE),
            ];
            let corners_ceiling = [
                Vec3::new(grid_x, ceiling_y, grid_z),
                Vec3::new(grid_x + SECTOR_SIZE, ceiling_y, grid_z),
                Vec3::new(grid_x + SECTOR_SIZE, ceiling_y, grid_z + SECTOR_SIZE),
                Vec3::new(grid_x, ceiling_y, grid_z + SECTOR_SIZE),
            ];

            let dim_color = RasterColor::new(50, 120, 110); // Dim teal

            // Draw vertical boundary lines
            for i in 0..4 {
                draw_3d_line(fb, corners_floor[i], corners_ceiling[i], &state.camera_3d, dim_color);
            }
            // Draw floor boundary
            for i in 0..4 {
                draw_3d_line(fb, corners_floor[i], corners_floor[(i + 1) % 4], &state.camera_3d, dim_color);
            }
            // Draw ceiling boundary
            for i in 0..4 {
                draw_3d_line(fb, corners_ceiling[i], corners_ceiling[(i + 1) % 4], &state.camera_3d, dim_color);
            }
        }
    }

    // Draw subtle sector boundary highlight for diagonal wall placement
    // Only show if on the drag line (same check as diagonal preview)
    if let Some((grid_x, grid_z, is_nwse, _)) = preview_diagonal_wall {
        // Check if this sector is on the drag line (if dragging)
        let on_drag_line = if let Some((start_gx, start_gz, start_is_nwse)) = state.diagonal_drag_start {
            // Convert preview world coords to grid coords
            let preview_gx = if let Some(room) = state.level.rooms.get(state.current_room) {
                ((grid_x - room.position.x) / SECTOR_SIZE).floor() as i32
            } else { 0 };
            let preview_gz = if let Some(room) = state.level.rooms.get(state.current_room) {
                ((grid_z - room.position.z) / SECTOR_SIZE).floor() as i32
            } else { 0 };
            // Diagonal is on the line if type matches AND it's on the simple diagonal line
            if let Some((end_gx, end_gz, _)) = state.diagonal_drag_current {
                if is_nwse != start_is_nwse {
                    false // Wrong diagonal type
                } else {
                    // Check if preview point is on the diagonal line from start to end
                    let sx = if start_gx < end_gx { 1 } else if start_gx > end_gx { -1 } else { 0 };
                    let sz = if start_gz < end_gz { 1 } else if start_gz > end_gz { -1 } else { 0 };
                    let steps = (end_gx - start_gx).abs().max((end_gz - start_gz).abs());
                    let mut found = false;
                    for i in 0..=steps {
                        let gx = start_gx + sx * i;
                        let gz = start_gz + sz * i;
                        if gx == preview_gx && gz == preview_gz {
                            found = true;
                            break;
                        }
                    }
                    found
                }
            } else {
                is_nwse == start_is_nwse
            }
        } else {
            true // Not dragging, always show
        };

        if on_drag_line {
            let room_y = state.level.rooms.get(state.current_room)
                .map(|r| r.position.y)
                .unwrap_or(0.0);

            let floor_y = room_y;
            let ceiling_y = room_y + super::CEILING_HEIGHT;

            // Sector corners
            let corners_floor = [
                Vec3::new(grid_x, floor_y, grid_z),
                Vec3::new(grid_x + SECTOR_SIZE, floor_y, grid_z),
                Vec3::new(grid_x + SECTOR_SIZE, floor_y, grid_z + SECTOR_SIZE),
                Vec3::new(grid_x, floor_y, grid_z + SECTOR_SIZE),
            ];
            let corners_ceiling = [
                Vec3::new(grid_x, ceiling_y, grid_z),
                Vec3::new(grid_x + SECTOR_SIZE, ceiling_y, grid_z),
                Vec3::new(grid_x + SECTOR_SIZE, ceiling_y, grid_z + SECTOR_SIZE),
                Vec3::new(grid_x, ceiling_y, grid_z + SECTOR_SIZE),
            ];

            let dim_color = RasterColor::new(50, 130, 130); // Dim cyan

            // Draw vertical boundary lines
            for i in 0..4 {
                draw_3d_line(fb, corners_floor[i], corners_ceiling[i], &state.camera_3d, dim_color);
            }
            // Draw floor boundary
            for i in 0..4 {
                draw_3d_line(fb, corners_floor[i], corners_floor[(i + 1) % 4], &state.camera_3d, dim_color);
            }
            // Draw ceiling boundary
            for i in 0..4 {
                draw_3d_line(fb, corners_ceiling[i], corners_ceiling[(i + 1) % 4], &state.camera_3d, dim_color);
            }
        }
    }

    // Draw wall preview when in DrawWall mode (after geometry so depth testing works)
    // wall_state: 0 = new, 1 = filling gap, 2 = fully covered
    // corner_heights: [bottom-left, bottom-right, top-right, top-left] (room-relative)
    // Skip single wall preview if it's not on the drag line
    if let Some((grid_x, grid_z, dir, corner_heights, wall_state, _mouse_y)) = preview_wall {
        // Check if this preview is on the drag line (if dragging)
        let on_drag_line = if let Some((start_gx, start_gz, start_dir)) = state.wall_drag_start {
            use crate::world::Direction;
            // Convert preview world coords to grid coords
            let preview_gx = if let Some(room) = state.level.rooms.get(state.current_room) {
                ((grid_x - room.position.x) / SECTOR_SIZE).floor() as i32
            } else { 0 };
            let preview_gz = if let Some(room) = state.level.rooms.get(state.current_room) {
                ((grid_z - room.position.z) / SECTOR_SIZE).floor() as i32
            } else { 0 };
            // Wall is on the line if direction matches AND the fixed axis matches
            dir == start_dir && match start_dir {
                Direction::North | Direction::South => preview_gz == start_gz,
                Direction::East | Direction::West => preview_gx == start_gx,
            }
        } else {
            true // Not dragging, always show preview
        };
        if on_drag_line {
        use crate::world::Direction;

        // Get room Y offset for world-space rendering
        let room_y = state.level.rooms.get(state.current_room)
            .map(|r| r.position.y)
            .unwrap_or(0.0);

        // For fully covered edges, show an X at edge center (no rectangle)
        if wall_state == 2 {
            // Just show a red X at the edge center to indicate fully covered
            let mid_y = room_y + CEILING_HEIGHT / 2.0;
            let (center, offset) = match dir {
                Direction::North => (Vec3::new(grid_x + SECTOR_SIZE / 2.0, mid_y, grid_z), Vec3::new(100.0, 100.0, 0.0)),
                Direction::East => (Vec3::new(grid_x + SECTOR_SIZE, mid_y, grid_z + SECTOR_SIZE / 2.0), Vec3::new(0.0, 100.0, 100.0)),
                Direction::South => (Vec3::new(grid_x + SECTOR_SIZE / 2.0, mid_y, grid_z + SECTOR_SIZE), Vec3::new(100.0, 100.0, 0.0)),
                Direction::West => (Vec3::new(grid_x, mid_y, grid_z + SECTOR_SIZE / 2.0), Vec3::new(0.0, 100.0, 100.0)),
            };
            let color = RasterColor::new(200, 80, 80); // Red
            draw_3d_line_depth(fb, center - offset, center + offset, &state.camera_3d, color);
            let offset2 = Vec3::new(offset.x, -offset.y, offset.z);
            draw_3d_line_depth(fb, center - offset2, center + offset2, &state.camera_3d, color);
        } else {
            // Wall corners with sloped heights based on direction
            // corner_heights are room-relative, add room_y for world-space
            let (p0, p1, p2, p3) = match dir {
                Direction::North => (
                    Vec3::new(grid_x, room_y + corner_heights[0], grid_z),                     // BL
                    Vec3::new(grid_x + SECTOR_SIZE, room_y + corner_heights[1], grid_z),       // BR
                    Vec3::new(grid_x + SECTOR_SIZE, room_y + corner_heights[2], grid_z),       // TR
                    Vec3::new(grid_x, room_y + corner_heights[3], grid_z),                     // TL
                ),
                Direction::East => (
                    Vec3::new(grid_x + SECTOR_SIZE, room_y + corner_heights[0], grid_z),                     // BL
                    Vec3::new(grid_x + SECTOR_SIZE, room_y + corner_heights[1], grid_z + SECTOR_SIZE),       // BR
                    Vec3::new(grid_x + SECTOR_SIZE, room_y + corner_heights[2], grid_z + SECTOR_SIZE),       // TR
                    Vec3::new(grid_x + SECTOR_SIZE, room_y + corner_heights[3], grid_z),                     // TL
                ),
                Direction::South => (
                    Vec3::new(grid_x + SECTOR_SIZE, room_y + corner_heights[0], grid_z + SECTOR_SIZE),       // BL
                    Vec3::new(grid_x, room_y + corner_heights[1], grid_z + SECTOR_SIZE),                     // BR
                    Vec3::new(grid_x, room_y + corner_heights[2], grid_z + SECTOR_SIZE),                     // TR
                    Vec3::new(grid_x + SECTOR_SIZE, room_y + corner_heights[3], grid_z + SECTOR_SIZE),       // TL
                ),
                Direction::West => (
                    Vec3::new(grid_x, room_y + corner_heights[0], grid_z + SECTOR_SIZE),       // BL
                    Vec3::new(grid_x, room_y + corner_heights[1], grid_z),                     // BR
                    Vec3::new(grid_x, room_y + corner_heights[2], grid_z),                     // TR
                    Vec3::new(grid_x, room_y + corner_heights[3], grid_z + SECTOR_SIZE),       // TL
                ),
            };

            // Color: teal for new wall, orange for filling gap
            let color = if wall_state == 1 {
                RasterColor::new(255, 180, 80) // Orange - filling gap
            } else {
                RasterColor::new(80, 200, 180) // Teal - new
            };

            // Draw wall outline (quad with potentially sloped edges) with depth testing and thickness
            draw_3d_thick_line_depth(fb, p0, p1, &state.camera_3d, color, 3);  // bottom edge
            draw_3d_thick_line_depth(fb, p1, p2, &state.camera_3d, color, 3);  // right edge
            draw_3d_thick_line_depth(fb, p2, p3, &state.camera_3d, color, 3);  // top edge
            draw_3d_thick_line_depth(fb, p3, p0, &state.camera_3d, color, 3);  // left edge

            // Draw vertex indicators at the 4 corners of the wall preview
            let vertex_color = RasterColor::new(255, 255, 255); // White
            draw_3d_point(fb, p0, &state.camera_3d, 3, vertex_color);
            draw_3d_point(fb, p1, &state.camera_3d, 3, vertex_color);
            draw_3d_point(fb, p2, &state.camera_3d, 3, vertex_color);
            draw_3d_point(fb, p3, &state.camera_3d, 3, vertex_color);

            // Draw + through it if filling gap to indicate addition
            if wall_state == 1 {
                // Vertical line (center)
                let mid_x = (p0.x + p1.x) / 2.0;
                let mid_z = (p0.z + p1.z) / 2.0;
                let center_bottom = Vec3::new(mid_x, room_y + (corner_heights[0] + corner_heights[1]) / 2.0, mid_z);
                let center_top = Vec3::new(mid_x, room_y + (corner_heights[2] + corner_heights[3]) / 2.0, mid_z);
                draw_3d_line_depth(fb, center_bottom, center_top, &state.camera_3d, color);
                // Horizontal line (middle height)
                let mid_y = room_y + (corner_heights[0] + corner_heights[1] + corner_heights[2] + corner_heights[3]) / 4.0;
                let left = Vec3::new(p0.x, mid_y, p0.z);
                let right = Vec3::new(p1.x, mid_y, p1.z);
                draw_3d_line_depth(fb, left, right, &state.camera_3d, color);
            }
        }
        } // end if on_drag_line
    }

    // Draw diagonal wall preview when in DrawDiagonalWall mode
    // Skip single diagonal preview if it's not on the drag line
    if let Some((grid_x, grid_z, is_nwse, corner_heights)) = preview_diagonal_wall {
        // Check if this preview is on the drag line (if dragging)
        let on_drag_line = if let Some((start_gx, start_gz, start_is_nwse)) = state.diagonal_drag_start {
            // Convert preview world coords to grid coords
            let preview_gx = if let Some(room) = state.level.rooms.get(state.current_room) {
                ((grid_x - room.position.x) / SECTOR_SIZE).floor() as i32
            } else { 0 };
            let preview_gz = if let Some(room) = state.level.rooms.get(state.current_room) {
                ((grid_z - room.position.z) / SECTOR_SIZE).floor() as i32
            } else { 0 };
            // Diagonal is on the line if type matches AND it's on the simple diagonal line
            if let Some((end_gx, end_gz, _)) = state.diagonal_drag_current {
                if is_nwse != start_is_nwse {
                    false // Wrong diagonal type
                } else {
                    // Check if preview point is on the diagonal line from start to end
                    // Simple check: both X and Z step together at the same rate
                    let sx = if start_gx < end_gx { 1 } else if start_gx > end_gx { -1 } else { 0 };
                    let sz = if start_gz < end_gz { 1 } else if start_gz > end_gz { -1 } else { 0 };
                    let steps = (end_gx - start_gx).abs().max((end_gz - start_gz).abs());
                    let mut found = false;
                    for i in 0..=steps {
                        let gx = start_gx + sx * i;
                        let gz = start_gz + sz * i;
                        if gx == preview_gx && gz == preview_gz {
                            found = true;
                            break;
                        }
                    }
                    found
                }
            } else {
                is_nwse == start_is_nwse
            }
        } else {
            true // Not dragging, always show preview
        };
        if on_drag_line {
        // Get room Y offset for world-space rendering
        let room_y = state.level.rooms.get(state.current_room)
            .map(|r| r.position.y)
            .unwrap_or(0.0);

        // Diagonal wall corners
        // corner_heights: [corner1_bot, corner2_bot, corner2_top, corner1_top]
        let (p0, p1, p2, p3) = if is_nwse {
            // NW-SE diagonal: from NW corner to SE corner
            (
                Vec3::new(grid_x, room_y + corner_heights[0], grid_z),                               // NW bottom
                Vec3::new(grid_x + SECTOR_SIZE, room_y + corner_heights[1], grid_z + SECTOR_SIZE),   // SE bottom
                Vec3::new(grid_x + SECTOR_SIZE, room_y + corner_heights[2], grid_z + SECTOR_SIZE),   // SE top
                Vec3::new(grid_x, room_y + corner_heights[3], grid_z),                               // NW top
            )
        } else {
            // NE-SW diagonal: from NE corner to SW corner
            (
                Vec3::new(grid_x + SECTOR_SIZE, room_y + corner_heights[0], grid_z),                 // NE bottom
                Vec3::new(grid_x, room_y + corner_heights[1], grid_z + SECTOR_SIZE),                 // SW bottom
                Vec3::new(grid_x, room_y + corner_heights[2], grid_z + SECTOR_SIZE),                 // SW top
                Vec3::new(grid_x + SECTOR_SIZE, room_y + corner_heights[3], grid_z),                 // NE top
            )
        };

        // Cyan color for diagonal wall preview
        let color = RasterColor::new(80, 220, 220);

        // Draw diagonal wall outline (quad) with depth testing and thickness
        draw_3d_thick_line_depth(fb, p0, p1, &state.camera_3d, color, 3);  // bottom diagonal edge
        draw_3d_thick_line_depth(fb, p1, p2, &state.camera_3d, color, 3);  // right edge
        draw_3d_thick_line_depth(fb, p2, p3, &state.camera_3d, color, 3);  // top diagonal edge
        draw_3d_thick_line_depth(fb, p3, p0, &state.camera_3d, color, 3);  // left edge
        // Cross pattern to indicate diagonal
        draw_3d_line_depth(fb, p0, p2, &state.camera_3d, color);  // diagonal (thin - just indicator)

        // Draw vertex indicators at the 4 corners of the diagonal wall preview
        let vertex_color = RasterColor::new(255, 255, 255); // White
        draw_3d_point(fb, p0, &state.camera_3d, 3, vertex_color);
        draw_3d_point(fb, p1, &state.camera_3d, 3, vertex_color);
        draw_3d_point(fb, p2, &state.camera_3d, 3, vertex_color);
        draw_3d_point(fb, p3, &state.camera_3d, 3, vertex_color);
        }
    }

    // Draw room boundary wireframes for all rooms
    if state.show_room_bounds {
        for (room_idx, room) in state.level.rooms.iter().enumerate() {
            // Skip hidden rooms
            if state.hidden_rooms.contains(&room_idx) {
                continue;
            }

            let is_current = room_idx == state.current_room;
            // Current room: bright blue, other rooms: dim gray
            let room_color = if is_current {
                RasterColor::new(80, 120, 200) // Blue for current room
            } else {
                RasterColor::new(60, 60, 80) // Dim gray for other rooms
            };

            // Room grid extents in world space
            let min_x = room.position.x;
            let min_z = room.position.z;
            let max_x = room.position.x + (room.width as f32) * SECTOR_SIZE;
            let max_z = room.position.z + (room.depth as f32) * SECTOR_SIZE;

            // Use Y range from room's actual geometry bounds
            // bounds are room-relative, so add room.position.y
            let min_y = room.position.y + room.bounds.min.y;
            let max_y = room.position.y + room.bounds.max.y;

            // Skip rooms with no geometry (empty bounds)
            if min_y > max_y || min_x > max_x || min_z > max_z {
                continue;
            }

            // 8 corners of the room bounding box
            let corners = [
                Vec3::new(min_x, min_y, min_z), // 0: front-bottom-left
                Vec3::new(max_x, min_y, min_z), // 1: front-bottom-right
                Vec3::new(max_x, min_y, max_z), // 2: back-bottom-right
                Vec3::new(min_x, min_y, max_z), // 3: back-bottom-left
                Vec3::new(min_x, max_y, min_z), // 4: front-top-left
                Vec3::new(max_x, max_y, min_z), // 5: front-top-right
                Vec3::new(max_x, max_y, max_z), // 6: back-top-right
                Vec3::new(min_x, max_y, max_z), // 7: back-top-left
            ];

            // Project corners to screen with depth for z-tested line drawing
            let screen_corners: Vec<Option<(i32, i32, f32)>> = corners.iter()
                .map(|c| world_to_screen_with_depth(*c, state.camera_3d.position,
                    state.camera_3d.basis_x, state.camera_3d.basis_y, state.camera_3d.basis_z,
                    fb.width, fb.height)
                    .map(|(x, y, z)| (x as i32, y as i32, z)))
                .collect();

            // Draw 12 edges of the bounding box with depth testing
            let edges = [
                // Bottom face
                (0, 1), (1, 2), (2, 3), (3, 0),
                // Top face
                (4, 5), (5, 6), (6, 7), (7, 4),
                // Vertical edges
                (0, 4), (1, 5), (2, 6), (3, 7),
            ];

            for (i, j) in edges {
                if let (Some((x0, y0, z0)), Some((x1, y1, z1))) = (screen_corners[i], screen_corners[j]) {
                    // Use overlay mode to draw on co-planar geometry surfaces
                    fb.draw_line_3d_overlay(x0, y0, z0, x1, y1, z1, room_color);
                }
            }

            // Draw portal outlines for this room
            for portal in &room.portals {
                // Check if this is a horizontal portal (normal pointing up or down)
                let is_horizontal = portal.normal.y.abs() > 0.9;

                // Different colors: magenta for horizontal (floor/ceiling), cyan for vertical (wall)
                let portal_color = if is_horizontal {
                    RasterColor::new(255, 100, 255) // Magenta for horizontal portals
                } else {
                    RasterColor::new(100, 255, 255) // Cyan for wall portals
                };

                // Portal vertices are room-relative, convert to world space
                let world_verts: [Vec3; 4] = [
                    Vec3::new(portal.vertices[0].x + room.position.x, portal.vertices[0].y + room.position.y, portal.vertices[0].z + room.position.z),
                    Vec3::new(portal.vertices[1].x + room.position.x, portal.vertices[1].y + room.position.y, portal.vertices[1].z + room.position.z),
                    Vec3::new(portal.vertices[2].x + room.position.x, portal.vertices[2].y + room.position.y, portal.vertices[2].z + room.position.z),
                    Vec3::new(portal.vertices[3].x + room.position.x, portal.vertices[3].y + room.position.y, portal.vertices[3].z + room.position.z),
                ];

                // Project to screen
                let screen_verts: Vec<Option<(i32, i32, f32)>> = world_verts.iter()
                    .map(|v| world_to_screen_with_depth(*v, state.camera_3d.position,
                        state.camera_3d.basis_x, state.camera_3d.basis_y, state.camera_3d.basis_z,
                        fb.width, fb.height)
                        .map(|(x, y, z)| (x as i32, y as i32, z)))
                    .collect();

                // Draw portal quad outline with overlay mode
                let portal_edges = [(0, 1), (1, 2), (2, 3), (3, 0)];
                for (i, j) in portal_edges {
                    if let (Some((x0, y0, z0)), Some((x1, y1, z1))) = (screen_verts[i], screen_verts[j]) {
                        fb.draw_line_3d_overlay(x0, y0, z0, x1, y1, z1, portal_color);
                    }
                }
            }
        }
    }

    // Draw LevelObject gizmos (spawns, object-based lights, triggers, etc.)
    for (room_idx, room) in state.level.rooms.iter().enumerate() {
        for (obj_idx, obj) in room.objects.iter().enumerate() {
            let world_pos = obj.world_position(room);

            // Project to screen
            if let Some((fb_x, fb_y)) = world_to_screen(
                world_pos,
                state.camera_3d.position,
                state.camera_3d.basis_x,
                state.camera_3d.basis_y,
                state.camera_3d.basis_z,
                fb.width,
                fb.height,
            ) {
                use crate::world::{ObjectType, SpawnPointType};

                // Check if this object is selected
                let is_selected = matches!(&state.selection, Selection::Object { room: r, index } if *r == room_idx && *index == obj_idx);

                // Get color and radius based on object type (letter unused in 3D view)
                let (color, _letter, draw_radius) = match &obj.object_type {
                    ObjectType::Spawn(spawn_type) => {
                        let (r, g, b, ch) = match spawn_type {
                            SpawnPointType::PlayerStart => (100, 255, 100, 'P'),
                            SpawnPointType::Checkpoint => (100, 200, 255, 'C'),
                            SpawnPointType::Enemy => (255, 100, 100, 'E'),
                            SpawnPointType::Item => (255, 200, 100, 'I'),
                        };
                        (RasterColor::new(r, g, b), ch, None)
                    }
                    ObjectType::Light { color: light_color, radius, .. } => {
                        let c = if obj.enabled {
                            RasterColor::new(light_color.r, light_color.g, light_color.b)
                        } else {
                            RasterColor::new(80, 80, 80)
                        };
                        (c, 'L', Some(*radius))
                    }
                    ObjectType::Prop(_) => (RasterColor::new(180, 130, 255), 'M', None),
                    ObjectType::Trigger { .. } => (RasterColor::new(255, 100, 200), 'T', None),
                    ObjectType::Particle { .. } => (RasterColor::new(255, 180, 100), '*', None),
                    ObjectType::Audio { .. } => (RasterColor::new(100, 200, 255), '~', None),
                };

                // For lights, draw 3D filled octahedron gizmo
                if let Some(_radius) = draw_radius {
                    let octa_size = if is_selected { 80.0 } else { 50.0 };
                    let octa_color = if is_selected {
                        RasterColor::new(255, 255, 255) // White when selected
                    } else {
                        color
                    };
                    draw_filled_octahedron(fb, &state.camera_3d, world_pos, octa_size, octa_color);
                } else if matches!(&obj.object_type, ObjectType::Spawn(SpawnPointType::PlayerStart)) {
                    // PlayerStart: draw collision cylinder wireframe only (no dot)
                    let settings = &state.level.player_settings;
                    let cylinder_color = if is_selected {
                        RasterColor::new(100, 255, 100) // Green when selected
                    } else {
                        RasterColor::new(100, 100, 100) // Grey when not selected
                    };
                    draw_wireframe_cylinder(
                        fb,
                        &state.camera_3d,
                        world_pos,
                        settings.radius,
                        settings.height,
                        12, // segments
                        cylinder_color,
                    );

                    // Draw camera position indicator
                    let cam_pos = Vec3::new(
                        world_pos.x,
                        world_pos.y + settings.camera_height,
                        world_pos.z - settings.camera_distance,
                    );
                    let cam_color = if is_selected {
                        RasterColor::new(255, 255, 100) // Yellow when selected
                    } else {
                        RasterColor::new(120, 120, 80) // Dark yellow/grey when not
                    };
                    // Draw small wireframe sphere for camera
                    draw_wireframe_sphere(fb, &state.camera_3d, cam_pos, 30.0, 6, cam_color);
                    // Draw line from player head to camera
                    let head_pos = Vec3::new(world_pos.x, world_pos.y + settings.height, world_pos.z);
                    draw_wireframe_line(fb, &state.camera_3d, head_pos, cam_pos, cam_color);
                } else {
                    // Other non-light objects: use 2D circles
                    let base_radius = if is_selected { 8 } else { 5 };

                    // Selection highlight (white circle behind)
                    if is_selected {
                        fb.draw_circle(fb_x as i32, fb_y as i32, base_radius + 3, RasterColor::new(255, 255, 255));
                    }

                    // Main circle
                    fb.draw_circle(fb_x as i32, fb_y as i32, base_radius, color);
                }
            }
        }
    }

    // Draw vertex overlays directly into framebuffer (only in Select mode)
    if state.tool == EditorTool::Select {
        for (world_pos, room_idx, gx, gz, corner_idx, face) in &all_vertices {
            if let Some((fb_x, fb_y)) = world_to_screen(
                *world_pos,
                state.camera_3d.position,
                state.camera_3d.basis_x,
                state.camera_3d.basis_y,
                state.camera_3d.basis_z,
                fb.width,
                fb.height,
            ) {
                // Check if this specific vertex is hovered (match room, sector coords, corner index, and face)
                let is_hovered = hovered_vertex.map_or(false, |(hr, hgx, hgz, hci, hface, _)|
                    hr == *room_idx && hgx == *gx && hgz == *gz && hci == *corner_idx && hface == *face);

                // Check if this vertex is selected (primary selection)
                let is_primary_selected = matches!(&state.selection,
                    Selection::Vertex { room, x, z, face: f, corner_idx: ci }
                    if *room == *room_idx && *x == *gx && *z == *gz && f == face && *ci == *corner_idx
                );

                // Check if this vertex is in multi-selection
                let is_multi_selected = state.multi_selection.iter().any(|sel| {
                    matches!(sel,
                        Selection::Vertex { room, x, z, face: f, corner_idx: ci }
                        if *room == *room_idx && *x == *gx && *z == *gz && f == face && *ci == *corner_idx
                    )
                });

                // Determine color based on state
                let color = if is_primary_selected || is_multi_selected {
                    RasterColor::new(100, 255, 100) // Green for selected
                } else if is_hovered {
                    RasterColor::new(255, 200, 150) // Orange when hovered
                } else {
                    continue; // Skip unselected, unhovered vertices
                };

                let radius = if is_primary_selected { 5 } else if is_multi_selected { 4 } else { 4 };
                fb.draw_circle(fb_x as i32, fb_y as i32, radius, color);
            }
        }
    }

    // Draw selected edges (primary and multi-selection)
    // Helper closure to draw an edge
    let draw_edge_highlight = |fb: &mut Framebuffer, state: &EditorState, room_idx: usize, gx: usize, gz: usize, face_idx: usize, edge_idx: usize, wall_face_opt: &Option<SectorFace>, color: RasterColor| {
        if let Some(room) = state.level.rooms.get(room_idx) {
            if let Some(sector) = room.get_sector(gx, gz) {
                let base_x = room.position.x + (gx as f32) * SECTOR_SIZE;
                let base_z = room.position.z + (gz as f32) * SECTOR_SIZE;
                let room_y = room.position.y;

                let corners: Option<[Vec3; 4]> = match face_idx {
                    0 => sector.floor.as_ref().map(|f| [
                        Vec3::new(base_x, room_y + f.heights[0], base_z),
                        Vec3::new(base_x + SECTOR_SIZE, room_y + f.heights[1], base_z),
                        Vec3::new(base_x + SECTOR_SIZE, room_y + f.heights[2], base_z + SECTOR_SIZE),
                        Vec3::new(base_x, room_y + f.heights[3], base_z + SECTOR_SIZE),
                    ]),
                    1 => sector.ceiling.as_ref().map(|c| [
                        Vec3::new(base_x, room_y + c.heights[0], base_z),
                        Vec3::new(base_x + SECTOR_SIZE, room_y + c.heights[1], base_z),
                        Vec3::new(base_x + SECTOR_SIZE, room_y + c.heights[2], base_z + SECTOR_SIZE),
                        Vec3::new(base_x, room_y + c.heights[3], base_z + SECTOR_SIZE),
                    ]),
                    2 => {
                        if let Some(wf) = wall_face_opt {
                            let (x0, z0, x1, z1) = match wf {
                                SectorFace::WallNorth(_) => (base_x, base_z, base_x + SECTOR_SIZE, base_z),
                                SectorFace::WallEast(_) => (base_x + SECTOR_SIZE, base_z, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE),
                                SectorFace::WallSouth(_) => (base_x + SECTOR_SIZE, base_z + SECTOR_SIZE, base_x, base_z + SECTOR_SIZE),
                                SectorFace::WallWest(_) => (base_x, base_z + SECTOR_SIZE, base_x, base_z),
                                SectorFace::WallNwSe(_) => (base_x, base_z, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE),
                                SectorFace::WallNeSw(_) => (base_x + SECTOR_SIZE, base_z, base_x, base_z + SECTOR_SIZE),
                                _ => (0.0, 0.0, 0.0, 0.0),
                            };
                            let wall_heights = match wf {
                                SectorFace::WallNorth(i) => sector.walls_north.get(*i).map(|w| w.heights),
                                SectorFace::WallEast(i) => sector.walls_east.get(*i).map(|w| w.heights),
                                SectorFace::WallSouth(i) => sector.walls_south.get(*i).map(|w| w.heights),
                                SectorFace::WallWest(i) => sector.walls_west.get(*i).map(|w| w.heights),
                                SectorFace::WallNwSe(i) => sector.walls_nwse.get(*i).map(|w| w.heights),
                                SectorFace::WallNeSw(i) => sector.walls_nesw.get(*i).map(|w| w.heights),
                                _ => None,
                            };
                            wall_heights.map(|h| [
                                Vec3::new(x0, room_y + h[0], z0),
                                Vec3::new(x1, room_y + h[1], z1),
                                Vec3::new(x1, room_y + h[2], z1),
                                Vec3::new(x0, room_y + h[3], z0),
                            ])
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                if let Some(corners) = corners {
                    let v0 = corners[edge_idx];
                    let v1 = corners[(edge_idx + 1) % 4];

                    if let (Some((sx0, sy0)), Some((sx1, sy1))) = (
                        world_to_screen(v0, state.camera_3d.position, state.camera_3d.basis_x,
                            state.camera_3d.basis_y, state.camera_3d.basis_z, fb.width, fb.height),
                        world_to_screen(v1, state.camera_3d.position, state.camera_3d.basis_x,
                            state.camera_3d.basis_y, state.camera_3d.basis_z, fb.width, fb.height)
                    ) {
                        fb.draw_thick_line(sx0 as i32, sy0 as i32, sx1 as i32, sy1 as i32, 3, color);
                    }
                }
            }
        }
    };

    // Draw primary selected edge
    if let Selection::Edge { room, x, z, face_idx, edge_idx, wall_face } = &state.selection {
        let selected_color = RasterColor::new(100, 255, 100); // Green for selected
        draw_edge_highlight(fb, state, *room, *x, *z, *face_idx, *edge_idx, wall_face, selected_color);
    }

    // Draw multi-selected edges
    for sel in &state.multi_selection {
        if let Selection::Edge { room, x, z, face_idx, edge_idx, wall_face } = sel {
            let selected_color = RasterColor::new(100, 255, 100); // Green for selected
            draw_edge_highlight(fb, state, *room, *x, *z, *face_idx, *edge_idx, wall_face, selected_color);
        }
    }

    // Draw hovered edge highlight directly into framebuffer
    if let Some((room_idx, gx, gz, face_idx, edge_idx, wall_face_opt, _)) = hovered_edge {
        if let Some(room) = state.level.rooms.get(room_idx) {
            if let Some(sector) = room.get_sector(gx, gz) {
                let base_x = room.position.x + (gx as f32) * SECTOR_SIZE;
                let base_z = room.position.z + (gz as f32) * SECTOR_SIZE;

                let edge_color = RasterColor::new(255, 200, 100); // Orange for edge hover

                let room_y = room.position.y; // Y offset for world-space

                // Get edge vertices based on face_idx
                let corners: Option<[Vec3; 4]> = match face_idx {
                    0 => sector.floor.as_ref().map(|f| [
                        Vec3::new(base_x, room_y + f.heights[0], base_z),
                        Vec3::new(base_x + SECTOR_SIZE, room_y + f.heights[1], base_z),
                        Vec3::new(base_x + SECTOR_SIZE, room_y + f.heights[2], base_z + SECTOR_SIZE),
                        Vec3::new(base_x, room_y + f.heights[3], base_z + SECTOR_SIZE),
                    ]),
                    1 => sector.ceiling.as_ref().map(|c| [
                        Vec3::new(base_x, room_y + c.heights[0], base_z),
                        Vec3::new(base_x + SECTOR_SIZE, room_y + c.heights[1], base_z),
                        Vec3::new(base_x + SECTOR_SIZE, room_y + c.heights[2], base_z + SECTOR_SIZE),
                        Vec3::new(base_x, room_y + c.heights[3], base_z + SECTOR_SIZE),
                    ]),
                    2 => {
                        // Wall edge - get corners from the specific wall
                        if let Some(wf) = &wall_face_opt {
                            let (x0, z0, x1, z1) = match wf {
                                SectorFace::WallNorth(_) => (base_x, base_z, base_x + SECTOR_SIZE, base_z),
                                SectorFace::WallEast(_) => (base_x + SECTOR_SIZE, base_z, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE),
                                SectorFace::WallSouth(_) => (base_x + SECTOR_SIZE, base_z + SECTOR_SIZE, base_x, base_z + SECTOR_SIZE),
                                SectorFace::WallWest(_) => (base_x, base_z + SECTOR_SIZE, base_x, base_z),
                                SectorFace::WallNwSe(_) => (base_x, base_z, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE),
                                SectorFace::WallNeSw(_) => (base_x + SECTOR_SIZE, base_z, base_x, base_z + SECTOR_SIZE),
                                _ => (0.0, 0.0, 0.0, 0.0),
                            };
                            let wall_heights = match wf {
                                SectorFace::WallNorth(i) => sector.walls_north.get(*i).map(|w| w.heights),
                                SectorFace::WallEast(i) => sector.walls_east.get(*i).map(|w| w.heights),
                                SectorFace::WallSouth(i) => sector.walls_south.get(*i).map(|w| w.heights),
                                SectorFace::WallWest(i) => sector.walls_west.get(*i).map(|w| w.heights),
                                SectorFace::WallNwSe(i) => sector.walls_nwse.get(*i).map(|w| w.heights),
                                SectorFace::WallNeSw(i) => sector.walls_nesw.get(*i).map(|w| w.heights),
                                _ => None,
                            };
                            wall_heights.map(|h| [
                                Vec3::new(x0, room_y + h[0], z0),
                                Vec3::new(x1, room_y + h[1], z1),
                                Vec3::new(x1, room_y + h[2], z1),
                                Vec3::new(x0, room_y + h[3], z0),
                            ])
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                if let Some(corners) = corners {
                    let v0 = corners[edge_idx];
                    let v1 = corners[(edge_idx + 1) % 4];

                    if let (Some((sx0, sy0)), Some((sx1, sy1))) = (
                        world_to_screen(v0, state.camera_3d.position, state.camera_3d.basis_x,
                            state.camera_3d.basis_y, state.camera_3d.basis_z, fb.width, fb.height),
                        world_to_screen(v1, state.camera_3d.position, state.camera_3d.basis_x,
                            state.camera_3d.basis_y, state.camera_3d.basis_z, fb.width, fb.height)
                    ) {
                        fb.draw_thick_line(sx0 as i32, sy0 as i32, sx1 as i32, sy1 as i32, 3, edge_color);
                    }
                }
            }
        }
    }

    // Draw hover highlight for hovered face (in Select mode)
    if let Some((room_idx, gx, gz, face)) = hovered_face {
        // Don't draw hover if this face is already selected
        let is_selected = state.selection.includes_face(room_idx, gx, gz, face);
        if !is_selected {
            if let Some(room) = state.level.rooms.get(room_idx) {
                if let Some(sector) = room.get_sector(gx, gz) {
                    let base_x = room.position.x + (gx as f32) * SECTOR_SIZE;
                    let base_z = room.position.z + (gz as f32) * SECTOR_SIZE;
                    let room_y = room.position.y; // Y offset for world-space

                    let hover_color = RasterColor::new(150, 200, 255); // Light blue for hover

                    match face {
                        SectorFace::Floor => {
                            if let Some(floor) = &sector.floor {
                                let corners = [
                                    Vec3::new(base_x, room_y + floor.heights[0], base_z),                    // NW = 0
                                    Vec3::new(base_x + SECTOR_SIZE, room_y + floor.heights[1], base_z),      // NE = 1
                                    Vec3::new(base_x + SECTOR_SIZE, room_y + floor.heights[2], base_z + SECTOR_SIZE), // SE = 2
                                    Vec3::new(base_x, room_y + floor.heights[3], base_z + SECTOR_SIZE),      // SW = 3
                                ];
                                // Draw all 4 edges
                                for i in 0..4 {
                                    draw_3d_line(fb, corners[i], corners[(i + 1) % 4], &state.camera_3d, hover_color);
                                }
                                // Draw diagonal based on split direction
                                match floor.split_direction {
                                    SplitDirection::NwSe => {
                                        draw_3d_line(fb, corners[0], corners[2], &state.camera_3d, hover_color);
                                    }
                                    SplitDirection::NeSw => {
                                        draw_3d_line(fb, corners[1], corners[3], &state.camera_3d, hover_color);
                                    }
                                }
                            }
                        }
                        SectorFace::Ceiling => {
                            if let Some(ceiling) = &sector.ceiling {
                                let corners = [
                                    Vec3::new(base_x, room_y + ceiling.heights[0], base_z),                    // NW = 0
                                    Vec3::new(base_x + SECTOR_SIZE, room_y + ceiling.heights[1], base_z),      // NE = 1
                                    Vec3::new(base_x + SECTOR_SIZE, room_y + ceiling.heights[2], base_z + SECTOR_SIZE), // SE = 2
                                    Vec3::new(base_x, room_y + ceiling.heights[3], base_z + SECTOR_SIZE),      // SW = 3
                                ];
                                // Draw all 4 edges
                                for i in 0..4 {
                                    draw_3d_line(fb, corners[i], corners[(i + 1) % 4], &state.camera_3d, hover_color);
                                }
                                // Draw diagonal based on split direction
                                match ceiling.split_direction {
                                    SplitDirection::NwSe => {
                                        draw_3d_line(fb, corners[0], corners[2], &state.camera_3d, hover_color);
                                    }
                                    SplitDirection::NeSw => {
                                        draw_3d_line(fb, corners[1], corners[3], &state.camera_3d, hover_color);
                                    }
                                }
                            }
                        }
                        SectorFace::WallNorth(i) => {
                            if let Some(wall) = sector.walls_north.get(i) {
                                let p0 = Vec3::new(base_x, room_y + wall.heights[0], base_z);
                                let p1 = Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[1], base_z);
                                let p2 = Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[2], base_z);
                                let p3 = Vec3::new(base_x, room_y + wall.heights[3], base_z);
                                draw_3d_line(fb, p0, p1, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p1, p2, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p2, p3, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p3, p0, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p0, p2, &state.camera_3d, hover_color);
                            }
                        }
                        SectorFace::WallEast(i) => {
                            if let Some(wall) = sector.walls_east.get(i) {
                                let p0 = Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[0], base_z);
                                let p1 = Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[1], base_z + SECTOR_SIZE);
                                let p2 = Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[2], base_z + SECTOR_SIZE);
                                let p3 = Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[3], base_z);
                                draw_3d_line(fb, p0, p1, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p1, p2, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p2, p3, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p3, p0, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p0, p2, &state.camera_3d, hover_color);
                            }
                        }
                        SectorFace::WallSouth(i) => {
                            if let Some(wall) = sector.walls_south.get(i) {
                                let p0 = Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[0], base_z + SECTOR_SIZE);
                                let p1 = Vec3::new(base_x, room_y + wall.heights[1], base_z + SECTOR_SIZE);
                                let p2 = Vec3::new(base_x, room_y + wall.heights[2], base_z + SECTOR_SIZE);
                                let p3 = Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[3], base_z + SECTOR_SIZE);
                                draw_3d_line(fb, p0, p1, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p1, p2, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p2, p3, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p3, p0, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p0, p2, &state.camera_3d, hover_color);
                            }
                        }
                        SectorFace::WallWest(i) => {
                            if let Some(wall) = sector.walls_west.get(i) {
                                let p0 = Vec3::new(base_x, room_y + wall.heights[0], base_z + SECTOR_SIZE);
                                let p1 = Vec3::new(base_x, room_y + wall.heights[1], base_z);
                                let p2 = Vec3::new(base_x, room_y + wall.heights[2], base_z);
                                let p3 = Vec3::new(base_x, room_y + wall.heights[3], base_z + SECTOR_SIZE);
                                draw_3d_line(fb, p0, p1, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p1, p2, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p2, p3, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p3, p0, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p0, p2, &state.camera_3d, hover_color);
                            }
                        }
                        SectorFace::WallNwSe(i) => {
                            // Diagonal wall from NW corner (base_x, base_z) to SE corner (base_x+SIZE, base_z+SIZE)
                            if let Some(wall) = sector.walls_nwse.get(i) {
                                let p0 = Vec3::new(base_x, room_y + wall.heights[0], base_z);                    // NW bottom
                                let p1 = Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[1], base_z + SECTOR_SIZE); // SE bottom
                                let p2 = Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[2], base_z + SECTOR_SIZE); // SE top
                                let p3 = Vec3::new(base_x, room_y + wall.heights[3], base_z);                    // NW top
                                draw_3d_line(fb, p0, p1, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p1, p2, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p2, p3, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p3, p0, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p0, p2, &state.camera_3d, hover_color);
                            }
                        }
                        SectorFace::WallNeSw(i) => {
                            // Diagonal wall from NE corner (base_x+SIZE, base_z) to SW corner (base_x, base_z+SIZE)
                            if let Some(wall) = sector.walls_nesw.get(i) {
                                let p0 = Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[0], base_z);      // NE bottom
                                let p1 = Vec3::new(base_x, room_y + wall.heights[1], base_z + SECTOR_SIZE);      // SW bottom
                                let p2 = Vec3::new(base_x, room_y + wall.heights[2], base_z + SECTOR_SIZE);      // SW top
                                let p3 = Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[3], base_z);      // NE top
                                draw_3d_line(fb, p0, p1, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p1, p2, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p2, p3, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p3, p0, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p0, p2, &state.camera_3d, hover_color);
                            }
                        }
                    }
                }
            }
        }
    }

    // Draw selection highlights for primary selection and all multi-selections
    let select_color = RasterColor::new(255, 200, 80); // Yellow/orange

    // Helper closure to draw selection highlight for a single Selection
    let draw_selection = |fb: &mut Framebuffer, selection: &Selection| {
        match selection {
            Selection::SectorFace { room, x, z, face } => {
                if let Some(room_data) = state.level.rooms.get(*room) {
                    if let Some(sector) = room_data.get_sector(*x, *z) {
                        let base_x = room_data.position.x + (*x as f32) * SECTOR_SIZE;
                        let base_z = room_data.position.z + (*z as f32) * SECTOR_SIZE;
                        let room_y = room_data.position.y; // Y offset for world-space

                        match face {
                            SectorFace::Floor => {
                                if let Some(floor) = &sector.floor {
                                    let corners = [
                                        Vec3::new(base_x, room_y + floor.heights[0], base_z),                    // NW = 0
                                        Vec3::new(base_x + SECTOR_SIZE, room_y + floor.heights[1], base_z),      // NE = 1
                                        Vec3::new(base_x + SECTOR_SIZE, room_y + floor.heights[2], base_z + SECTOR_SIZE), // SE = 2
                                        Vec3::new(base_x, room_y + floor.heights[3], base_z + SECTOR_SIZE),      // SW = 3
                                    ];
                                    // Draw all 4 edges
                                    for i in 0..4 {
                                        draw_3d_line(fb, corners[i], corners[(i + 1) % 4], &state.camera_3d, select_color);
                                    }
                                    // Draw diagonal based on split direction
                                    match floor.split_direction {
                                        SplitDirection::NwSe => {
                                            draw_3d_line(fb, corners[0], corners[2], &state.camera_3d, select_color);
                                        }
                                        SplitDirection::NeSw => {
                                            draw_3d_line(fb, corners[1], corners[3], &state.camera_3d, select_color);
                                        }
                                    }
                                }
                            }
                            SectorFace::Ceiling => {
                                if let Some(ceiling) = &sector.ceiling {
                                    let corners = [
                                        Vec3::new(base_x, room_y + ceiling.heights[0], base_z),                    // NW = 0
                                        Vec3::new(base_x + SECTOR_SIZE, room_y + ceiling.heights[1], base_z),      // NE = 1
                                        Vec3::new(base_x + SECTOR_SIZE, room_y + ceiling.heights[2], base_z + SECTOR_SIZE), // SE = 2
                                        Vec3::new(base_x, room_y + ceiling.heights[3], base_z + SECTOR_SIZE),      // SW = 3
                                    ];
                                    // Draw all 4 edges
                                    for i in 0..4 {
                                        draw_3d_line(fb, corners[i], corners[(i + 1) % 4], &state.camera_3d, select_color);
                                    }
                                    // Draw diagonal based on split direction
                                    match ceiling.split_direction {
                                        SplitDirection::NwSe => {
                                            draw_3d_line(fb, corners[0], corners[2], &state.camera_3d, select_color);
                                        }
                                        SplitDirection::NeSw => {
                                            draw_3d_line(fb, corners[1], corners[3], &state.camera_3d, select_color);
                                        }
                                    }
                                }
                            }
                            SectorFace::WallNorth(i) => {
                                if let Some(wall) = sector.walls_north.get(*i) {
                                    let p0 = Vec3::new(base_x, room_y + wall.heights[0], base_z);
                                    let p1 = Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[1], base_z);
                                    let p2 = Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[2], base_z);
                                    let p3 = Vec3::new(base_x, room_y + wall.heights[3], base_z);
                                    draw_3d_line(fb, p0, p1, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p1, p2, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p2, p3, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p3, p0, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p0, p2, &state.camera_3d, select_color);
                                }
                            }
                            SectorFace::WallEast(i) => {
                                if let Some(wall) = sector.walls_east.get(*i) {
                                    let p0 = Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[0], base_z);
                                    let p1 = Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[1], base_z + SECTOR_SIZE);
                                    let p2 = Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[2], base_z + SECTOR_SIZE);
                                    let p3 = Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[3], base_z);
                                    draw_3d_line(fb, p0, p1, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p1, p2, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p2, p3, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p3, p0, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p0, p2, &state.camera_3d, select_color);
                                }
                            }
                            SectorFace::WallSouth(i) => {
                                if let Some(wall) = sector.walls_south.get(*i) {
                                    let p0 = Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[0], base_z + SECTOR_SIZE);
                                    let p1 = Vec3::new(base_x, room_y + wall.heights[1], base_z + SECTOR_SIZE);
                                    let p2 = Vec3::new(base_x, room_y + wall.heights[2], base_z + SECTOR_SIZE);
                                    let p3 = Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[3], base_z + SECTOR_SIZE);
                                    draw_3d_line(fb, p0, p1, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p1, p2, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p2, p3, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p3, p0, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p0, p2, &state.camera_3d, select_color);
                                }
                            }
                            SectorFace::WallWest(i) => {
                                if let Some(wall) = sector.walls_west.get(*i) {
                                    let p0 = Vec3::new(base_x, room_y + wall.heights[0], base_z + SECTOR_SIZE);
                                    let p1 = Vec3::new(base_x, room_y + wall.heights[1], base_z);
                                    let p2 = Vec3::new(base_x, room_y + wall.heights[2], base_z);
                                    let p3 = Vec3::new(base_x, room_y + wall.heights[3], base_z + SECTOR_SIZE);
                                    draw_3d_line(fb, p0, p1, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p1, p2, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p2, p3, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p3, p0, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p0, p2, &state.camera_3d, select_color);
                                }
                            }
                            SectorFace::WallNwSe(i) => {
                                if let Some(wall) = sector.walls_nwse.get(*i) {
                                    let p0 = Vec3::new(base_x, room_y + wall.heights[0], base_z);
                                    let p1 = Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[1], base_z + SECTOR_SIZE);
                                    let p2 = Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[2], base_z + SECTOR_SIZE);
                                    let p3 = Vec3::new(base_x, room_y + wall.heights[3], base_z);
                                    draw_3d_line(fb, p0, p1, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p1, p2, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p2, p3, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p3, p0, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p0, p2, &state.camera_3d, select_color);
                                }
                            }
                            SectorFace::WallNeSw(i) => {
                                if let Some(wall) = sector.walls_nesw.get(*i) {
                                    let p0 = Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[0], base_z);
                                    let p1 = Vec3::new(base_x, room_y + wall.heights[1], base_z + SECTOR_SIZE);
                                    let p2 = Vec3::new(base_x, room_y + wall.heights[2], base_z + SECTOR_SIZE);
                                    let p3 = Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[3], base_z);
                                    draw_3d_line(fb, p0, p1, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p1, p2, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p2, p3, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p3, p0, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p0, p2, &state.camera_3d, select_color);
                                }
                            }
                        }
                    }
                }
            }
            Selection::Sector { room, x, z } => {
                // Sector-level selection (from 2D grid view) - highlight all faces
                if let Some(room_data) = state.level.rooms.get(*room) {
                    if let Some(sector) = room_data.get_sector(*x, *z) {
                        let base_x = room_data.position.x + (*x as f32) * SECTOR_SIZE;
                        let base_z = room_data.position.z + (*z as f32) * SECTOR_SIZE;
                        let room_y = room_data.position.y; // Y offset for world-space

                        // Draw floor outline if floor exists
                        if let Some(floor) = &sector.floor {
                            let corners = [
                                Vec3::new(base_x, room_y + floor.heights[0], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, room_y + floor.heights[1], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, room_y + floor.heights[2], base_z + SECTOR_SIZE),
                                Vec3::new(base_x, room_y + floor.heights[3], base_z + SECTOR_SIZE),
                            ];
                            for i in 0..4 {
                                draw_3d_line(fb, corners[i], corners[(i + 1) % 4], &state.camera_3d, select_color);
                            }
                        }

                        // Draw ceiling outline if ceiling exists
                        if let Some(ceiling) = &sector.ceiling {
                            let corners = [
                                Vec3::new(base_x, room_y + ceiling.heights[0], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, room_y + ceiling.heights[1], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, room_y + ceiling.heights[2], base_z + SECTOR_SIZE),
                                Vec3::new(base_x, room_y + ceiling.heights[3], base_z + SECTOR_SIZE),
                            ];
                            for i in 0..4 {
                                draw_3d_line(fb, corners[i], corners[(i + 1) % 4], &state.camera_3d, select_color);
                            }
                        }

                        // Draw vertical edges at corners
                        if sector.floor.is_some() || sector.ceiling.is_some() {
                            let floor_y = sector.floor.as_ref().map(|f| f.heights[0]).unwrap_or(0.0);
                            let ceiling_y = sector.ceiling.as_ref().map(|c| c.heights[0]).unwrap_or(1024.0);

                            let corner_positions = [
                                (base_x, base_z),
                                (base_x + SECTOR_SIZE, base_z),
                                (base_x + SECTOR_SIZE, base_z + SECTOR_SIZE),
                                (base_x, base_z + SECTOR_SIZE),
                            ];

                            for (i, &(cx, cz)) in corner_positions.iter().enumerate() {
                                let fy = sector.floor.as_ref().map(|f| f.heights[i]).unwrap_or(floor_y);
                                let cy = sector.ceiling.as_ref().map(|c| c.heights[i]).unwrap_or(ceiling_y);
                                draw_3d_line(
                                    fb,
                                    Vec3::new(cx, room_y + fy, cz),
                                    Vec3::new(cx, room_y + cy, cz),
                                    &state.camera_3d,
                                    select_color,
                                );
                            }
                        }

                        // Draw wall outlines
                        let wall_sets = [
                            (&sector.walls_north, base_x, base_z, base_x + SECTOR_SIZE, base_z),
                            (&sector.walls_east, base_x + SECTOR_SIZE, base_z, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE),
                            (&sector.walls_south, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE, base_x, base_z + SECTOR_SIZE),
                            (&sector.walls_west, base_x, base_z + SECTOR_SIZE, base_x, base_z),
                        ];

                        for (walls, x0, z0, x1, z1) in wall_sets {
                            for wall in walls {
                                let p0 = Vec3::new(x0, room_y + wall.heights[0], z0);
                                let p1 = Vec3::new(x1, room_y + wall.heights[1], z1);
                                let p2 = Vec3::new(x1, room_y + wall.heights[2], z1);
                                let p3 = Vec3::new(x0, room_y + wall.heights[3], z0);
                                draw_3d_line(fb, p0, p1, &state.camera_3d, select_color);
                                draw_3d_line(fb, p1, p2, &state.camera_3d, select_color);
                                draw_3d_line(fb, p2, p3, &state.camera_3d, select_color);
                                draw_3d_line(fb, p3, p0, &state.camera_3d, select_color);
                            }
                        }
                    }
                }
            }
            Selection::Edge { room, x, z, face_idx, edge_idx, wall_face } => {
                if let Some(room_data) = state.level.rooms.get(*room) {
                    if let Some(sector) = room_data.get_sector(*x, *z) {
                        let base_x = room_data.position.x + (*x as f32) * SECTOR_SIZE;
                        let base_z = room_data.position.z + (*z as f32) * SECTOR_SIZE;
                        let room_y = room_data.position.y; // Y offset for world-space

                        let corners: Option<[Vec3; 4]> = if *face_idx == 0 {
                            sector.floor.as_ref().map(|f| [
                                Vec3::new(base_x, room_y + f.heights[0], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, room_y + f.heights[1], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, room_y + f.heights[2], base_z + SECTOR_SIZE),
                                Vec3::new(base_x, room_y + f.heights[3], base_z + SECTOR_SIZE),
                            ])
                        } else if *face_idx == 1 {
                            sector.ceiling.as_ref().map(|c| [
                                Vec3::new(base_x, room_y + c.heights[0], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, room_y + c.heights[1], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, room_y + c.heights[2], base_z + SECTOR_SIZE),
                                Vec3::new(base_x, room_y + c.heights[3], base_z + SECTOR_SIZE),
                            ])
                        } else if *face_idx == 2 {
                            // Wall edge
                            if let Some(wf) = wall_face {
                                let (x0, z0, x1, z1) = match wf {
                                    SectorFace::WallNorth(_) => (base_x, base_z, base_x + SECTOR_SIZE, base_z),
                                    SectorFace::WallEast(_) => (base_x + SECTOR_SIZE, base_z, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE),
                                    SectorFace::WallSouth(_) => (base_x + SECTOR_SIZE, base_z + SECTOR_SIZE, base_x, base_z + SECTOR_SIZE),
                                    SectorFace::WallWest(_) => (base_x, base_z + SECTOR_SIZE, base_x, base_z),
                                    SectorFace::WallNwSe(_) => (base_x, base_z, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE),
                                    SectorFace::WallNeSw(_) => (base_x + SECTOR_SIZE, base_z, base_x, base_z + SECTOR_SIZE),
                                    _ => (0.0, 0.0, 0.0, 0.0),
                                };
                                let wall_heights = match wf {
                                    SectorFace::WallNorth(i) => sector.walls_north.get(*i).map(|w| w.heights),
                                    SectorFace::WallEast(i) => sector.walls_east.get(*i).map(|w| w.heights),
                                    SectorFace::WallSouth(i) => sector.walls_south.get(*i).map(|w| w.heights),
                                    SectorFace::WallWest(i) => sector.walls_west.get(*i).map(|w| w.heights),
                                    SectorFace::WallNwSe(i) => sector.walls_nwse.get(*i).map(|w| w.heights),
                                    SectorFace::WallNeSw(i) => sector.walls_nesw.get(*i).map(|w| w.heights),
                                    _ => None,
                                };
                                wall_heights.map(|h| [
                                    Vec3::new(x0, room_y + h[0], z0),
                                    Vec3::new(x1, room_y + h[1], z1),
                                    Vec3::new(x1, room_y + h[2], z1),
                                    Vec3::new(x0, room_y + h[3], z0),
                                ])
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        if let Some(c) = corners {
                            let corner0 = *edge_idx;
                            let corner1 = (*edge_idx + 1) % 4;
                            draw_3d_line(fb, c[corner0], c[corner1], &state.camera_3d, select_color);
                        }
                    }
                }
            }
            _ => {}
        }
    };

    // Draw primary selection
    draw_selection(fb, &state.selection);

    // Draw all multi-selections
    for sel in &state.multi_selection {
        draw_selection(fb, sel);
    }

    // Draw floor/ceiling placement preview wireframe with vertical sector boundaries
    if let Some((snapped_x, snapped_z, target_y, occupied)) = preview_sector {
        use super::CEILING_HEIGHT;

        // Get room Y offset for correct world-space positioning
        let room_y = state.level.rooms.get(state.current_room)
            .map(|r| r.position.y)
            .unwrap_or(0.0);

        let floor_y = room_y;
        let ceiling_y = room_y + CEILING_HEIGHT;

        // target_y is room-relative, add room_y for world-space position
        let world_target_y = room_y + target_y;
        let corners = [
            Vec3::new(snapped_x, world_target_y, snapped_z),
            Vec3::new(snapped_x, world_target_y, snapped_z + SECTOR_SIZE),
            Vec3::new(snapped_x + SECTOR_SIZE, world_target_y, snapped_z + SECTOR_SIZE),
            Vec3::new(snapped_x + SECTOR_SIZE, world_target_y, snapped_z),
        ];

        let floor_corners = [
            Vec3::new(snapped_x, floor_y, snapped_z),
            Vec3::new(snapped_x, floor_y, snapped_z + SECTOR_SIZE),
            Vec3::new(snapped_x + SECTOR_SIZE, floor_y, snapped_z + SECTOR_SIZE),
            Vec3::new(snapped_x + SECTOR_SIZE, floor_y, snapped_z),
        ];

        let ceiling_corners = [
            Vec3::new(snapped_x, ceiling_y, snapped_z),
            Vec3::new(snapped_x, ceiling_y, snapped_z + SECTOR_SIZE),
            Vec3::new(snapped_x + SECTOR_SIZE, ceiling_y, snapped_z + SECTOR_SIZE),
            Vec3::new(snapped_x + SECTOR_SIZE, ceiling_y, snapped_z),
        ];

        let mut screen_corners = Vec::new();
        let mut screen_floor = Vec::new();
        let mut screen_ceiling = Vec::new();

        for corner in &corners {
            if let Some((sx, sy)) = world_to_screen(*corner, state.camera_3d.position,
                state.camera_3d.basis_x, state.camera_3d.basis_y, state.camera_3d.basis_z,
                fb.width, fb.height)
            {
                screen_corners.push((sx as i32, sy as i32));
            }
        }

        for corner in &floor_corners {
            if let Some((sx, sy)) = world_to_screen(*corner, state.camera_3d.position,
                state.camera_3d.basis_x, state.camera_3d.basis_y, state.camera_3d.basis_z,
                fb.width, fb.height)
            {
                screen_floor.push((sx as i32, sy as i32));
            }
        }

        for corner in &ceiling_corners {
            if let Some((sx, sy)) = world_to_screen(*corner, state.camera_3d.position,
                state.camera_3d.basis_x, state.camera_3d.basis_y, state.camera_3d.basis_z,
                fb.width, fb.height)
            {
                screen_ceiling.push((sx as i32, sy as i32));
            }
        }

        // Green for valid placement, red for occupied
        let color = if occupied {
            RasterColor::new(255, 80, 80)
        } else {
            RasterColor::new(80, 255, 80)
        };
        let dim_color = if occupied {
            RasterColor::new(180, 60, 60)
        } else {
            RasterColor::new(60, 180, 60)
        };

        // Draw vertical boundary lines (floor to ceiling at each corner)
        if screen_floor.len() == 4 && screen_ceiling.len() == 4 {
            for i in 0..4 {
                let (fx, fy) = screen_floor[i];
                let (cx, cy) = screen_ceiling[i];
                fb.draw_line(fx, fy, cx, cy, dim_color);
            }

            for i in 0..4 {
                let (x0, y0) = screen_floor[i];
                let (x1, y1) = screen_floor[(i + 1) % 4];
                fb.draw_line(x0, y0, x1, y1, dim_color);
            }

            for i in 0..4 {
                let (x0, y0) = screen_ceiling[i];
                let (x1, y1) = screen_ceiling[(i + 1) % 4];
                fb.draw_line(x0, y0, x1, y1, dim_color);
            }
        }

        // Draw placement preview (the actual tile being placed - brighter)
        if screen_corners.len() == 4 {
            for i in 0..4 {
                let (x0, y0) = screen_corners[i];
                let (x1, y1) = screen_corners[(i + 1) % 4];
                fb.draw_thick_line(x0, y0, x1, y1, 2, color);
            }

            for (x, y) in &screen_corners {
                fb.draw_circle(*x, *y, 3, color);
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

    // Draw camera info (position and rotation) - top left
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
        rect.y + 14.0,
        14.0,
        Color::from_rgba(200, 200, 200, 255),
    );

    // Center 3D camera on current room button - top right
    let btn_size = 24.0;
    let btn_rect = crate::ui::Rect::new(
        rect.right() - btn_size - 4.0,
        rect.y + 4.0,
        btn_size,
        btn_size,
    );
    if crate::ui::icon_button(ctx, btn_rect, crate::ui::icon::SQUARE_SQUARE, icon_font, "Center 3D camera on current room") {
        state.center_3d_on_current_room();
    }
}

/// Delete a single face from a sector, returns true if something was deleted
fn delete_face(level: &mut crate::world::Level, room_idx: usize, gx: usize, gz: usize, face: SectorFace) -> bool {
    let Some(room) = level.rooms.get_mut(room_idx) else { return false };
    let Some(sector) = room.get_sector_mut(gx, gz) else { return false };

    match face {
        SectorFace::Floor => {
            if sector.floor.is_some() { sector.floor = None; true } else { false }
        }
        SectorFace::Ceiling => {
            if sector.ceiling.is_some() { sector.ceiling = None; true } else { false }
        }
        SectorFace::WallNorth(i) => {
            if i < sector.walls_north.len() { sector.walls_north.remove(i); true } else { false }
        }
        SectorFace::WallEast(i) => {
            if i < sector.walls_east.len() { sector.walls_east.remove(i); true } else { false }
        }
        SectorFace::WallSouth(i) => {
            if i < sector.walls_south.len() { sector.walls_south.remove(i); true } else { false }
        }
        SectorFace::WallWest(i) => {
            if i < sector.walls_west.len() { sector.walls_west.remove(i); true } else { false }
        }
        SectorFace::WallNwSe(i) => {
            if i < sector.walls_nwse.len() { sector.walls_nwse.remove(i); true } else { false }
        }
        SectorFace::WallNeSw(i) => {
            if i < sector.walls_nesw.len() { sector.walls_nesw.remove(i); true } else { false }
        }
    }
}

/// Draw a 3D line into the framebuffer using Bresenham's algorithm (no depth testing)
fn draw_3d_line(
    fb: &mut Framebuffer,
    p0: Vec3,
    p1: Vec3,
    camera: &crate::rasterizer::Camera,
    color: RasterColor,
) {
    draw_3d_line_impl(fb, p0, p1, camera, color, false);
}

/// Draw a 3D line with depth testing (overlay mode - draws on co-planar surfaces)
fn draw_3d_line_depth(
    fb: &mut Framebuffer,
    p0: Vec3,
    p1: Vec3,
    camera: &crate::rasterizer::Camera,
    color: RasterColor,
) {
    draw_3d_line_impl(fb, p0, p1, camera, color, true);
}

/// Draw a thick 3D line with depth testing (for preview highlights)
fn draw_3d_thick_line_depth(
    fb: &mut Framebuffer,
    p0: Vec3,
    p1: Vec3,
    camera: &crate::rasterizer::Camera,
    color: RasterColor,
    thickness: i32,
) {
    if thickness <= 1 {
        draw_3d_line_impl(fb, p0, p1, camera, color, true);
        return;
    }

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
        let t = (NEAR_PLANE - z0) / (z1 - z0);
        let new_p0 = p0 + (p1 - p0) * t;
        (new_p0, p1)
    } else if z1 <= NEAR_PLANE {
        let t = (NEAR_PLANE - z0) / (z1 - z0);
        let new_p1 = p0 + (p1 - p0) * t;
        (p0, new_p1)
    } else {
        (p0, p1)
    };

    // Project to screen
    let s0 = world_to_screen_with_depth(clipped_p0, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height);
    let s1 = world_to_screen_with_depth(clipped_p1, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height);

    let (Some((x0f, y0f, depth0)), Some((x1f, y1f, depth1))) = (s0, s1) else {
        return;
    };

    let x0 = x0f as i32;
    let y0 = y0f as i32;
    let x1 = x1f as i32;
    let y1 = y1f as i32;

    // Calculate perpendicular offset for thickness
    let dx = (x1 - x0) as f32;
    let dy = (y1 - y0) as f32;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.001 {
        return;
    }

    let half = thickness as f32 * 0.5;
    let px = -dy / len * half;
    let py = dx / len * half;

    // Draw multiple parallel lines for thickness
    for i in 0..thickness {
        let offset = i as f32 - half + 0.5;
        let ox = (px * offset / half) as i32;
        let oy = (py * offset / half) as i32;
        fb.draw_line_3d_overlay(x0 + ox, y0 + oy, depth0, x1 + ox, y1 + oy, depth1, color);
    }
}

fn draw_3d_line_impl(
    fb: &mut Framebuffer,
    p0: Vec3,
    p1: Vec3,
    camera: &crate::rasterizer::Camera,
    color: RasterColor,
    use_depth: bool,
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
        let t = (NEAR_PLANE - z0) / (z1 - z0);
        let new_p0 = p0 + (p1 - p0) * t;
        (new_p0, p1)
    } else if z1 <= NEAR_PLANE {
        let t = (NEAR_PLANE - z0) / (z1 - z0);
        let new_p1 = p0 + (p1 - p0) * t;
        (p0, new_p1)
    } else {
        (p0, p1)
    };

    if use_depth {
        // Use depth-tested line drawing
        let s0 = world_to_screen_with_depth(clipped_p0, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height);
        let s1 = world_to_screen_with_depth(clipped_p1, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height);

        let (Some((x0f, y0f, depth0)), Some((x1f, y1f, depth1))) = (s0, s1) else {
            return;
        };

        fb.draw_line_3d_overlay(x0f as i32, y0f as i32, depth0, x1f as i32, y1f as i32, depth1, color);
    } else {
        // No depth testing - draw on top of everything
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
}

/// Draw a 3D point (filled circle) at a world position
fn draw_3d_point(
    fb: &mut Framebuffer,
    pos: Vec3,
    camera: &crate::rasterizer::Camera,
    radius: i32,
    color: RasterColor,
) {
    if let Some((sx, sy)) = world_to_screen(
        pos,
        camera.position,
        camera.basis_x,
        camera.basis_y,
        camera.basis_z,
        fb.width,
        fb.height,
    ) {
        fb.draw_circle(sx as i32, sy as i32, radius, color);
    }
}

/// Draw a wireframe cylinder in the 3D view (for player collision visualization)
fn draw_wireframe_cylinder(
    fb: &mut Framebuffer,
    camera: &crate::rasterizer::Camera,
    center: Vec3,
    radius: f32,
    height: f32,
    segments: usize,
    color: RasterColor,
) {
    use std::f32::consts::PI;

    // Generate circle points at bottom and top
    let mut bottom_points: Vec<Vec3> = Vec::with_capacity(segments);
    let mut top_points: Vec<Vec3> = Vec::with_capacity(segments);

    for i in 0..segments {
        let angle = (i as f32 / segments as f32) * 2.0 * PI;
        let x = center.x + radius * angle.cos();
        let z = center.z + radius * angle.sin();

        bottom_points.push(Vec3::new(x, center.y, z));
        top_points.push(Vec3::new(x, center.y + height, z));
    }

    // Draw bottom circle
    for i in 0..segments {
        let next = (i + 1) % segments;
        draw_3d_line(fb, bottom_points[i], bottom_points[next], camera, color);
    }

    // Draw top circle
    for i in 0..segments {
        let next = (i + 1) % segments;
        draw_3d_line(fb, top_points[i], top_points[next], camera, color);
    }

    // Draw vertical lines connecting top and bottom (every other segment for cleaner look)
    let skip = if segments > 8 { 2 } else { 1 };
    for i in (0..segments).step_by(skip) {
        draw_3d_line(fb, bottom_points[i], top_points[i], camera, color);
    }
}

/// Draw a wireframe sphere in 3D (for camera gizmo)
fn draw_wireframe_sphere(
    fb: &mut Framebuffer,
    camera: &crate::rasterizer::Camera,
    center: Vec3,
    radius: f32,
    segments: usize,
    color: RasterColor,
) {
    use std::f32::consts::PI;

    // Draw 3 orthogonal circles (XY, XZ, YZ planes)
    // XZ plane (horizontal circle)
    let mut prev_xz = Vec3::new(center.x + radius, center.y, center.z);
    for i in 1..=segments {
        let angle = (i as f32 / segments as f32) * 2.0 * PI;
        let curr = Vec3::new(center.x + radius * angle.cos(), center.y, center.z + radius * angle.sin());
        draw_3d_line(fb, prev_xz, curr, camera, color);
        prev_xz = curr;
    }

    // XY plane (vertical circle facing Z)
    let mut prev_xy = Vec3::new(center.x + radius, center.y, center.z);
    for i in 1..=segments {
        let angle = (i as f32 / segments as f32) * 2.0 * PI;
        let curr = Vec3::new(center.x + radius * angle.cos(), center.y + radius * angle.sin(), center.z);
        draw_3d_line(fb, prev_xy, curr, camera, color);
        prev_xy = curr;
    }

    // YZ plane (vertical circle facing X)
    let mut prev_yz = Vec3::new(center.x, center.y + radius, center.z);
    for i in 1..=segments {
        let angle = (i as f32 / segments as f32) * 2.0 * PI;
        let curr = Vec3::new(center.x, center.y + radius * angle.cos(), center.z + radius * angle.sin());
        draw_3d_line(fb, prev_yz, curr, camera, color);
        prev_yz = curr;
    }
}

/// Draw a wireframe line in 3D between two points
fn draw_wireframe_line(
    fb: &mut Framebuffer,
    camera: &crate::rasterizer::Camera,
    start: Vec3,
    end: Vec3,
    color: RasterColor,
) {
    draw_3d_line(fb, start, end, camera, color);
}

/// Draw a filled octahedron in 3D (classic light gizmo)
fn draw_filled_octahedron(
    fb: &mut Framebuffer,
    camera: &crate::rasterizer::Camera,
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
        let rel = p - camera.position;
        let cam = crate::rasterizer::perspective_transform(rel, camera.basis_x, camera.basis_y, camera.basis_z);
        if cam.z < 0.1 { return None; }
        let proj = crate::rasterizer::project(cam, fb.width, fb.height);
        Some((proj.x as i32, proj.y as i32, cam.z))
    };

    let top_s = project_vertex(top);
    let bottom_s = project_vertex(bottom);
    let front_s = project_vertex(front);
    let back_s = project_vertex(back);
    let left_s = project_vertex(left);
    let right_s = project_vertex(right);

    // 8 triangular faces of the octahedron
    // Top pyramid: top-front-right, top-right-back, top-back-left, top-left-front
    // Bottom pyramid: bottom-right-front, bottom-back-right, bottom-left-back, bottom-front-left
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
    draw_3d_line(fb, top, front, camera, edge_color);
    draw_3d_line(fb, top, back, camera, edge_color);
    draw_3d_line(fb, top, left, camera, edge_color);
    draw_3d_line(fb, top, right, camera, edge_color);
    draw_3d_line(fb, bottom, front, camera, edge_color);
    draw_3d_line(fb, bottom, back, camera, edge_color);
    draw_3d_line(fb, bottom, left, camera, edge_color);
    draw_3d_line(fb, bottom, right, camera, edge_color);
    draw_3d_line(fb, front, right, camera, edge_color);
    draw_3d_line(fb, right, back, camera, edge_color);
    draw_3d_line(fb, back, left, camera, edge_color);
    draw_3d_line(fb, left, front, camera, edge_color);
}

/// Draw a filled triangle (for gizmos, renders on top without z-test)
fn draw_filled_triangle_3d(
    fb: &mut Framebuffer,
    p0: (i32, i32, f32),
    p1: (i32, i32, f32),
    p2: (i32, i32, f32),
    color: RasterColor,
) {
    // Sort vertices by y coordinate (ignore z, we don't z-test gizmos)
    let mut pts = [(p0.0, p0.1), (p1.0, p1.1), (p2.0, p2.1)];
    pts.sort_by(|a, b| a.1.cmp(&b.1));
    let (x0, y0) = pts[0];
    let (x1, y1) = pts[1];
    let (x2, y2) = pts[2];

    if y2 == y0 { return; } // Degenerate triangle

    // Scanline fill
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

        // Interpolate x along edges
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

        // Draw horizontal scanline
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

// =============================================================================
// Camera Input Handling
// =============================================================================

/// Handle camera input for both Free and Orbit modes.
/// Returns true if orbit target should be updated after selection changes.
fn handle_camera_input(
    ctx: &UiContext,
    state: &mut EditorState,
    inside_viewport: bool,
    mouse_pos: (f32, f32),
    input: &InputState,
) -> bool {
    let shift_held = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
    let delta = get_frame_time();

    // Gamepad input
    let left_stick = input.left_stick();
    let right_stick = input.right_stick();
    let has_gamepad_input = left_stick.length() > 0.0 || right_stick.length() > 0.0
        || input.action_down(Action::FlyUp) || input.action_down(Action::FlyDown);

    match state.camera_mode {
        CameraMode::Free => {
            // Free camera: right-drag to look around, WASD to move
            if ctx.mouse.right_down && inside_viewport && state.dragging_sector_vertices.is_empty() {
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

            // Gamepad right stick: look around
            if right_stick.length() > 0.0 {
                let look_sensitivity = 2.5;
                state.camera_3d.rotate(
                    right_stick.y * look_sensitivity * delta,
                    -right_stick.x * look_sensitivity * delta,
                );
            }

            // Keyboard camera movement (WASD + Q/E) - only when viewport focused and not dragging
            // Hold Shift for faster movement
            let base_speed = 100.0; // Scaled for TRLE units (1024 per sector)
            let move_speed = if shift_held { base_speed * 4.0 } else { base_speed };
            if (inside_viewport || state.viewport_mouse_captured || has_gamepad_input) && state.dragging_sector_vertices.is_empty() {
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

                // Gamepad left stick: move forward/back, strafe left/right
                let gamepad_speed = 1500.0 * delta; // Frame-rate independent
                if left_stick.length() > 0.1 {
                    state.camera_3d.position = state.camera_3d.position + state.camera_3d.basis_z * left_stick.y * gamepad_speed;
                    state.camera_3d.position = state.camera_3d.position + state.camera_3d.basis_x * left_stick.x * gamepad_speed;
                }

                // Gamepad vertical movement: LB up, LT down
                if input.action_down(Action::FlyUp) {
                    state.camera_3d.position = state.camera_3d.position + state.camera_3d.basis_y * gamepad_speed;
                }
                if input.action_down(Action::FlyDown) {
                    state.camera_3d.position = state.camera_3d.position - state.camera_3d.basis_y * gamepad_speed;
                }
            }

            // Mouse wheel: move forward/backward (like W/S)
            if inside_viewport && ctx.mouse.scroll != 0.0 {
                let scroll_speed = if shift_held { 400.0 } else { 100.0 };
                let direction = if ctx.mouse.scroll > 0.0 { 1.0 } else { -1.0 };
                state.camera_3d.position = state.camera_3d.position + state.camera_3d.basis_z * direction * scroll_speed;
            }
        }

        CameraMode::Orbit => {
            // Orbit camera: right-drag rotates around target (or pans with Shift)
            if ctx.mouse.right_down && (inside_viewport || state.viewport_mouse_captured) && state.dragging_sector_vertices.is_empty() {
                if state.viewport_mouse_captured {
                    let dx = mouse_pos.0 - state.viewport_last_mouse.0;
                    let dy = mouse_pos.1 - state.viewport_last_mouse.1;

                    if shift_held {
                        // Shift+Right drag: pan the orbit target
                        let pan_speed = state.orbit_distance * 0.002;
                        state.orbit_target = state.orbit_target - state.camera_3d.basis_x * dx * pan_speed;
                        state.orbit_target = state.orbit_target + state.camera_3d.basis_y * dy * pan_speed;
                        state.last_orbit_target = state.orbit_target;
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

            // Gamepad right stick: orbit rotation
            if right_stick.length() > 0.0 {
                state.orbit_azimuth -= right_stick.x * 2.0 * delta;
                state.orbit_elevation = (state.orbit_elevation + right_stick.y * 1.5 * delta)
                    .clamp(-1.4, 1.4);
                state.sync_camera_from_orbit();
            }

            // Gamepad left stick: pan the orbit target
            if left_stick.length() > 0.1 {
                let pan_speed = state.orbit_distance * 0.002 * 60.0 * delta; // 60 = normalize for typical framerate
                state.orbit_target = state.orbit_target - state.camera_3d.basis_x * left_stick.x * pan_speed;
                state.orbit_target = state.orbit_target + state.camera_3d.basis_z * left_stick.y * pan_speed;
                state.last_orbit_target = state.orbit_target;
                state.sync_camera_from_orbit();
            }

            // Gamepad LB/LT: zoom in/out
            if input.action_down(Action::FlyUp) {
                state.orbit_distance = (state.orbit_distance * (1.0 - 1.5 * delta)).clamp(100.0, 20000.0);
                state.sync_camera_from_orbit();
            }
            if input.action_down(Action::FlyDown) {
                state.orbit_distance = (state.orbit_distance * (1.0 + 1.5 * delta)).clamp(100.0, 20000.0);
                state.sync_camera_from_orbit();
            }

            // Mouse wheel: zoom in/out (change orbit distance)
            if inside_viewport && ctx.mouse.scroll != 0.0 {
                let zoom_factor = if ctx.mouse.scroll > 0.0 { 0.9 } else { 1.1 };
                state.orbit_distance = (state.orbit_distance * zoom_factor).clamp(100.0, 20000.0);
                state.sync_camera_from_orbit();
            }
        }
    }

    // Return whether orbit target should be updated
    state.camera_mode == CameraMode::Orbit && ctx.mouse.left_pressed && inside_viewport
}

// =============================================================================
// Hover Detection Types and Helpers
// =============================================================================

/// Result of hover detection in the viewport
pub struct HoverResult {
    /// Hovered vertex: (room_idx, gx, gz, corner_idx, face, screen_dist)
    pub vertex: Option<(usize, usize, usize, usize, SectorFace, f32)>,
    /// Hovered edge: (room_idx, gx, gz, face_idx, edge_idx, wall_face, dist)
    pub edge: Option<(usize, usize, usize, usize, usize, Option<SectorFace>, f32)>,
    /// Hovered face: (room_idx, gx, gz, face)
    pub face: Option<(usize, usize, usize, SectorFace)>,
    /// Hovered object: (room_idx, object_idx, screen_dist)
    pub object: Option<(usize, usize, f32)>,
}

impl Default for HoverResult {
    fn default() -> Self {
        Self {
            vertex: None,
            edge: None,
            face: None,
            object: None,
        }
    }
}

/// Collected vertex data for hover detection and rendering
/// (world_pos, room_idx, gx, gz, corner_idx, face_type)
pub type VertexData = Vec<(Vec3, usize, usize, usize, usize, SectorFace)>;

/// Collect vertices from a single room
fn collect_single_room_vertices(room: &crate::world::Room, room_idx: usize) -> VertexData {
    let mut vertices = Vec::new();
    let room_y = room.position.y; // Y offset for world-space

    for (gx, gz, sector) in room.iter_sectors() {
        let base_x = room.position.x + (gx as f32) * SECTOR_SIZE;
        let base_z = room.position.z + (gz as f32) * SECTOR_SIZE;

        // Floor vertices
        if let Some(floor) = &sector.floor {
            vertices.push((Vec3::new(base_x, room_y + floor.heights[0], base_z), room_idx, gx, gz, 0, SectorFace::Floor));
            vertices.push((Vec3::new(base_x + SECTOR_SIZE, room_y + floor.heights[1], base_z), room_idx, gx, gz, 1, SectorFace::Floor));
            vertices.push((Vec3::new(base_x + SECTOR_SIZE, room_y + floor.heights[2], base_z + SECTOR_SIZE), room_idx, gx, gz, 2, SectorFace::Floor));
            vertices.push((Vec3::new(base_x, room_y + floor.heights[3], base_z + SECTOR_SIZE), room_idx, gx, gz, 3, SectorFace::Floor));
        }

        // Ceiling vertices
        if let Some(ceiling) = &sector.ceiling {
            vertices.push((Vec3::new(base_x, room_y + ceiling.heights[0], base_z), room_idx, gx, gz, 0, SectorFace::Ceiling));
            vertices.push((Vec3::new(base_x + SECTOR_SIZE, room_y + ceiling.heights[1], base_z), room_idx, gx, gz, 1, SectorFace::Ceiling));
            vertices.push((Vec3::new(base_x + SECTOR_SIZE, room_y + ceiling.heights[2], base_z + SECTOR_SIZE), room_idx, gx, gz, 2, SectorFace::Ceiling));
            vertices.push((Vec3::new(base_x, room_y + ceiling.heights[3], base_z + SECTOR_SIZE), room_idx, gx, gz, 3, SectorFace::Ceiling));
        }

        // Wall vertices
        let wall_configs: [(&Vec<crate::world::VerticalFace>, f32, f32, f32, f32, fn(usize) -> SectorFace); 4] = [
            (&sector.walls_north, base_x, base_z, base_x + SECTOR_SIZE, base_z, |i| SectorFace::WallNorth(i)),
            (&sector.walls_east, base_x + SECTOR_SIZE, base_z, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE, |i| SectorFace::WallEast(i)),
            (&sector.walls_south, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE, base_x, base_z + SECTOR_SIZE, |i| SectorFace::WallSouth(i)),
            (&sector.walls_west, base_x, base_z + SECTOR_SIZE, base_x, base_z, |i| SectorFace::WallWest(i)),
        ];

        for (walls, x0, z0, x1, z1, make_face) in wall_configs {
            for (i, wall) in walls.iter().enumerate() {
                // 4 corners of wall: bottom-left, bottom-right, top-right, top-left
                vertices.push((Vec3::new(x0, room_y + wall.heights[0], z0), room_idx, gx, gz, 0, make_face(i)));
                vertices.push((Vec3::new(x1, room_y + wall.heights[1], z1), room_idx, gx, gz, 1, make_face(i)));
                vertices.push((Vec3::new(x1, room_y + wall.heights[2], z1), room_idx, gx, gz, 2, make_face(i)));
                vertices.push((Vec3::new(x0, room_y + wall.heights[3], z0), room_idx, gx, gz, 3, make_face(i)));
            }
        }

        // Diagonal wall vertices (NW-SE)
        for (i, wall) in sector.walls_nwse.iter().enumerate() {
            // NW-SE wall: from NW corner (base_x, base_z) to SE corner (base_x+SIZE, base_z+SIZE)
            vertices.push((Vec3::new(base_x, room_y + wall.heights[0], base_z), room_idx, gx, gz, 0, SectorFace::WallNwSe(i)));
            vertices.push((Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[1], base_z + SECTOR_SIZE), room_idx, gx, gz, 1, SectorFace::WallNwSe(i)));
            vertices.push((Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[2], base_z + SECTOR_SIZE), room_idx, gx, gz, 2, SectorFace::WallNwSe(i)));
            vertices.push((Vec3::new(base_x, room_y + wall.heights[3], base_z), room_idx, gx, gz, 3, SectorFace::WallNwSe(i)));
        }

        // Diagonal wall vertices (NE-SW)
        for (i, wall) in sector.walls_nesw.iter().enumerate() {
            // NE-SW wall: from NE corner (base_x+SIZE, base_z) to SW corner (base_x, base_z+SIZE)
            vertices.push((Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[0], base_z), room_idx, gx, gz, 0, SectorFace::WallNeSw(i)));
            vertices.push((Vec3::new(base_x, room_y + wall.heights[1], base_z + SECTOR_SIZE), room_idx, gx, gz, 1, SectorFace::WallNeSw(i)));
            vertices.push((Vec3::new(base_x, room_y + wall.heights[2], base_z + SECTOR_SIZE), room_idx, gx, gz, 2, SectorFace::WallNeSw(i)));
            vertices.push((Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[3], base_z), room_idx, gx, gz, 3, SectorFace::WallNeSw(i)));
        }
    }

    vertices
}

/// Collect all vertex positions for the current room (for hover detection)
fn collect_room_vertices(state: &EditorState) -> VertexData {
    if let Some(room) = state.level.rooms.get(state.current_room) {
        collect_single_room_vertices(room, state.current_room)
    } else {
        Vec::new()
    }
}

/// Collect vertex positions from ALL rooms (for cross-room linking)
fn collect_all_room_vertices(state: &EditorState) -> VertexData {
    let mut all_vertices = Vec::new();
    for (room_idx, room) in state.level.rooms.iter().enumerate() {
        all_vertices.extend(collect_single_room_vertices(room, room_idx));
    }
    all_vertices
}

/// Find hovered elements in Select mode using depth-based selection.
/// The element closest to the camera wins, regardless of type (vertex/edge/face).
fn find_hovered_elements(
    state: &EditorState,
    mouse_fb: (f32, f32),
    fb_width: usize,
    fb_height: usize,
    all_vertices: &VertexData,
) -> HoverResult {
    let mut result = HoverResult::default();
    let (mouse_fb_x, mouse_fb_y) = mouse_fb;

    const VERTEX_THRESHOLD: f32 = 6.0;
    const EDGE_THRESHOLD: f32 = 4.0;

    // Track which type wins: 0=vertex, 1=edge, 2=face
    let mut best_type: usize = 0;

    // Temporary storage for best vertex/edge with depth
    let mut best_vertex: Option<(usize, usize, usize, usize, SectorFace, f32, f32)> = None; // + depth
    let mut best_edge: Option<(usize, usize, usize, usize, usize, Option<SectorFace>, f32, f32)> = None; // + depth
    let mut best_face: Option<(usize, usize, usize, SectorFace, f32)> = None;

    // Check vertices with depth
    for (world_pos, room_idx, gx, gz, corner_idx, face) in all_vertices {
        if let Some((sx, sy, depth)) = world_to_screen_with_depth(
            *world_pos,
            state.camera_3d.position,
            state.camera_3d.basis_x,
            state.camera_3d.basis_y,
            state.camera_3d.basis_z,
            fb_width,
            fb_height,
        ) {
            let screen_dist = ((mouse_fb_x - sx).powi(2) + (mouse_fb_y - sy).powi(2)).sqrt();
            if screen_dist < VERTEX_THRESHOLD {
                // Check if this is closer than current best vertex
                if best_vertex.map_or(true, |(_, _, _, _, _, _, best_d)| depth < best_d) {
                    best_vertex = Some((*room_idx, *gx, *gz, *corner_idx, *face, screen_dist, depth));
                }
            }
        }
    }

    // Check edges with depth
    if let Some(room) = state.level.rooms.get(state.current_room) {
        let room_y = room.position.y;
        for (gx, gz, sector) in room.iter_sectors() {
            let base_x = room.position.x + (gx as f32) * SECTOR_SIZE;
            let base_z = room.position.z + (gz as f32) * SECTOR_SIZE;

            // Helper to check an edge and update best_edge if closer
            let mut check_edge = |v0: Vec3, v1: Vec3, face_idx: usize, edge_idx: usize, wall_face: Option<SectorFace>| {
                if let (Some((sx0, sy0, d0)), Some((sx1, sy1, d1))) = (
                    world_to_screen_with_depth(v0, state.camera_3d.position, state.camera_3d.basis_x,
                        state.camera_3d.basis_y, state.camera_3d.basis_z, fb_width, fb_height),
                    world_to_screen_with_depth(v1, state.camera_3d.position, state.camera_3d.basis_x,
                        state.camera_3d.basis_y, state.camera_3d.basis_z, fb_width, fb_height)
                ) {
                    let screen_dist = point_to_segment_distance(mouse_fb_x, mouse_fb_y, sx0, sy0, sx1, sy1);
                    if screen_dist < EDGE_THRESHOLD {
                        // Interpolate depth along the edge based on closest point
                        let edge_depth = interpolate_edge_depth(
                            mouse_fb_x, mouse_fb_y, sx0, sy0, d0, sx1, sy1, d1
                        );
                        if best_edge.map_or(true, |(_, _, _, _, _, _, _, best_d)| edge_depth < best_d) {
                            best_edge = Some((state.current_room, gx, gz, face_idx, edge_idx, wall_face, screen_dist, edge_depth));
                        }
                    }
                }
            };

            // Check floor edges
            if let Some(floor) = &sector.floor {
                let corners = [
                    Vec3::new(base_x, room_y + floor.heights[0], base_z),
                    Vec3::new(base_x + SECTOR_SIZE, room_y + floor.heights[1], base_z),
                    Vec3::new(base_x + SECTOR_SIZE, room_y + floor.heights[2], base_z + SECTOR_SIZE),
                    Vec3::new(base_x, room_y + floor.heights[3], base_z + SECTOR_SIZE),
                ];
                for edge_idx in 0..4 {
                    check_edge(corners[edge_idx], corners[(edge_idx + 1) % 4], 0, edge_idx, None);
                }
            }

            // Check ceiling edges
            if let Some(ceiling) = &sector.ceiling {
                let corners = [
                    Vec3::new(base_x, room_y + ceiling.heights[0], base_z),
                    Vec3::new(base_x + SECTOR_SIZE, room_y + ceiling.heights[1], base_z),
                    Vec3::new(base_x + SECTOR_SIZE, room_y + ceiling.heights[2], base_z + SECTOR_SIZE),
                    Vec3::new(base_x, room_y + ceiling.heights[3], base_z + SECTOR_SIZE),
                ];
                for edge_idx in 0..4 {
                    check_edge(corners[edge_idx], corners[(edge_idx + 1) % 4], 1, edge_idx, None);
                }
            }

            // Check wall edges
            let wall_configs: [(&Vec<crate::world::VerticalFace>, f32, f32, f32, f32, fn(usize) -> SectorFace); 4] = [
                (&sector.walls_north, base_x, base_z, base_x + SECTOR_SIZE, base_z, |i| SectorFace::WallNorth(i)),
                (&sector.walls_east, base_x + SECTOR_SIZE, base_z, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE, |i| SectorFace::WallEast(i)),
                (&sector.walls_south, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE, base_x, base_z + SECTOR_SIZE, |i| SectorFace::WallSouth(i)),
                (&sector.walls_west, base_x, base_z + SECTOR_SIZE, base_x, base_z, |i| SectorFace::WallWest(i)),
            ];

            for (walls, x0, z0, x1, z1, make_face) in wall_configs {
                for (i, wall) in walls.iter().enumerate() {
                    let wall_corners = [
                        Vec3::new(x0, room_y + wall.heights[0], z0),
                        Vec3::new(x1, room_y + wall.heights[1], z1),
                        Vec3::new(x1, room_y + wall.heights[2], z1),
                        Vec3::new(x0, room_y + wall.heights[3], z0),
                    ];
                    for edge_idx in 0..4 {
                        check_edge(wall_corners[edge_idx], wall_corners[(edge_idx + 1) % 4], 2, edge_idx, Some(make_face(i)));
                    }
                }
            }

            // Check diagonal wall edges
            // NW-SE walls: from NW corner (base_x, base_z) to SE corner (base_x+SIZE, base_z+SIZE)
            for (i, wall) in sector.walls_nwse.iter().enumerate() {
                let wall_corners = [
                    Vec3::new(base_x, room_y + wall.heights[0], base_z),                               // NW bottom
                    Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[1], base_z + SECTOR_SIZE),   // SE bottom
                    Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[2], base_z + SECTOR_SIZE),   // SE top
                    Vec3::new(base_x, room_y + wall.heights[3], base_z),                               // NW top
                ];
                for edge_idx in 0..4 {
                    check_edge(wall_corners[edge_idx], wall_corners[(edge_idx + 1) % 4], 2, edge_idx, Some(SectorFace::WallNwSe(i)));
                }
            }

            // NE-SW walls: from NE corner (base_x+SIZE, base_z) to SW corner (base_x, base_z+SIZE)
            for (i, wall) in sector.walls_nesw.iter().enumerate() {
                let wall_corners = [
                    Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[0], base_z),                 // NE bottom
                    Vec3::new(base_x, room_y + wall.heights[1], base_z + SECTOR_SIZE),                 // SW bottom
                    Vec3::new(base_x, room_y + wall.heights[2], base_z + SECTOR_SIZE),                 // SW top
                    Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[3], base_z),                 // NE top
                ];
                for edge_idx in 0..4 {
                    check_edge(wall_corners[edge_idx], wall_corners[(edge_idx + 1) % 4], 2, edge_idx, Some(SectorFace::WallNeSw(i)));
                }
            }
        }
    }

    // Check faces (already uses depth)
    if let Some(room) = state.level.rooms.get(state.current_room) {
        let room_y = room.position.y;
        for (gx, gz, sector) in room.iter_sectors() {
            let base_x = room.position.x + (gx as f32) * SECTOR_SIZE;
            let base_z = room.position.z + (gz as f32) * SECTOR_SIZE;

            // Check floor
            if let Some(floor) = &sector.floor {
                let corners = [
                    Vec3::new(base_x, room_y + floor.heights[0], base_z),
                    Vec3::new(base_x + SECTOR_SIZE, room_y + floor.heights[1], base_z),
                    Vec3::new(base_x + SECTOR_SIZE, room_y + floor.heights[2], base_z + SECTOR_SIZE),
                    Vec3::new(base_x, room_y + floor.heights[3], base_z + SECTOR_SIZE),
                ];

                if let Some(depth) = check_quad_hit_with_depth(
                    mouse_fb_x, mouse_fb_y, &corners, &state.camera_3d, fb_width, fb_height
                ) {
                    if best_face.map_or(true, |(_, _, _, _, best_depth)| depth < best_depth) {
                        best_face = Some((state.current_room, gx, gz, SectorFace::Floor, depth));
                    }
                }
            }

            // Check ceiling
            if let Some(ceiling) = &sector.ceiling {
                let corners = [
                    Vec3::new(base_x, room_y + ceiling.heights[0], base_z),
                    Vec3::new(base_x + SECTOR_SIZE, room_y + ceiling.heights[1], base_z),
                    Vec3::new(base_x + SECTOR_SIZE, room_y + ceiling.heights[2], base_z + SECTOR_SIZE),
                    Vec3::new(base_x, room_y + ceiling.heights[3], base_z + SECTOR_SIZE),
                ];

                if let Some(depth) = check_quad_hit_with_depth(
                    mouse_fb_x, mouse_fb_y, &corners, &state.camera_3d, fb_width, fb_height
                ) {
                    if best_face.map_or(true, |(_, _, _, _, best_depth)| depth < best_depth) {
                        best_face = Some((state.current_room, gx, gz, SectorFace::Ceiling, depth));
                    }
                }
            }

            // Check walls
            let wall_configs: [(&Vec<crate::world::VerticalFace>, f32, f32, f32, f32, fn(usize) -> SectorFace); 4] = [
                (&sector.walls_north, base_x, base_z, base_x + SECTOR_SIZE, base_z, |i| SectorFace::WallNorth(i)),
                (&sector.walls_east, base_x + SECTOR_SIZE, base_z, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE, |i| SectorFace::WallEast(i)),
                (&sector.walls_south, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE, base_x, base_z + SECTOR_SIZE, |i| SectorFace::WallSouth(i)),
                (&sector.walls_west, base_x, base_z + SECTOR_SIZE, base_x, base_z, |i| SectorFace::WallWest(i)),
            ];

            for (walls, x0, z0, x1, z1, make_face) in wall_configs {
                for (i, wall) in walls.iter().enumerate() {
                    let wall_corners = [
                        Vec3::new(x0, room_y + wall.heights[0], z0),
                        Vec3::new(x1, room_y + wall.heights[1], z1),
                        Vec3::new(x1, room_y + wall.heights[2], z1),
                        Vec3::new(x0, room_y + wall.heights[3], z0),
                    ];

                    if let Some(depth) = check_quad_hit_with_depth(
                        mouse_fb_x, mouse_fb_y, &wall_corners, &state.camera_3d, fb_width, fb_height
                    ) {
                        if best_face.map_or(true, |(_, _, _, _, best_depth)| depth < best_depth) {
                            best_face = Some((state.current_room, gx, gz, make_face(i), depth));
                        }
                    }
                }
            }

            // Check diagonal walls (NW-SE)
            for (i, wall) in sector.walls_nwse.iter().enumerate() {
                let wall_corners = [
                    Vec3::new(base_x, room_y + wall.heights[0], base_z),                               // NW bottom
                    Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[1], base_z + SECTOR_SIZE),   // SE bottom
                    Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[2], base_z + SECTOR_SIZE),   // SE top
                    Vec3::new(base_x, room_y + wall.heights[3], base_z),                               // NW top
                ];

                if let Some(depth) = check_quad_hit_with_depth(
                    mouse_fb_x, mouse_fb_y, &wall_corners, &state.camera_3d, fb_width, fb_height
                ) {
                    if best_face.map_or(true, |(_, _, _, _, best_depth)| depth < best_depth) {
                        best_face = Some((state.current_room, gx, gz, SectorFace::WallNwSe(i), depth));
                    }
                }
            }

            // Check diagonal walls (NE-SW)
            for (i, wall) in sector.walls_nesw.iter().enumerate() {
                let wall_corners = [
                    Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[0], base_z),                 // NE bottom
                    Vec3::new(base_x, room_y + wall.heights[1], base_z + SECTOR_SIZE),                 // SW bottom
                    Vec3::new(base_x, room_y + wall.heights[2], base_z + SECTOR_SIZE),                 // SW top
                    Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[3], base_z),                 // NE top
                ];

                if let Some(depth) = check_quad_hit_with_depth(
                    mouse_fb_x, mouse_fb_y, &wall_corners, &state.camera_3d, fb_width, fb_height
                ) {
                    if best_face.map_or(true, |(_, _, _, _, best_depth)| depth < best_depth) {
                        best_face = Some((state.current_room, gx, gz, SectorFace::WallNeSw(i), depth));
                    }
                }
            }
        }
    }

    // Depth tolerance as percentage - when depths are within this ratio of each other,
    // use priority ordering (vertex > edge > face) instead of strict depth comparison.
    // 1% means depths within 1% are considered "same depth".
    const DEPTH_TOLERANCE_PERCENT: f32 = 0.01;

    // Collect candidates with their depths and priorities (lower type = higher priority)
    let mut candidates: Vec<(f32, usize)> = Vec::new(); // (depth, type) where type: 0=vertex, 1=edge, 2=face

    if let Some((_, _, _, _, _, _, vertex_depth)) = best_vertex {
        candidates.push((vertex_depth, 0));
    }
    if let Some((_, _, _, _, _, _, _, edge_depth)) = best_edge {
        candidates.push((edge_depth, 1));
    }
    if let Some((_, _, _, _, face_depth)) = best_face {
        candidates.push((face_depth, 2));
    }

    // Find the winner: closest depth, but when depths are within tolerance, lower type wins
    if !candidates.is_empty() {
        // Sort by depth first
        candidates.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

        // The closest candidate
        let (closest_depth, _) = candidates[0];
        let tolerance = closest_depth * DEPTH_TOLERANCE_PERCENT;

        // Among all candidates within tolerance of the closest, pick the one with lowest type (highest priority)
        best_type = candidates.iter()
            .filter(|(d, _)| (*d - closest_depth).abs() < tolerance)
            .map(|(_, t)| *t)
            .min()
            .unwrap_or(candidates[0].1);
    }

    // Set result based on which type won
    match best_type {
        0 => {
            if let Some((room_idx, gx, gz, corner_idx, face, screen_dist, _)) = best_vertex {
                result.vertex = Some((room_idx, gx, gz, corner_idx, face, screen_dist));
            }
        }
        1 => {
            if let Some((room_idx, gx, gz, face_idx, edge_idx, wall_face, screen_dist, _)) = best_edge {
                result.edge = Some((room_idx, gx, gz, face_idx, edge_idx, wall_face, screen_dist));
            }
        }
        2 => {
            if let Some((room_idx, gx, gz, face, _)) = best_face {
                result.face = Some((room_idx, gx, gz, face));
            }
        }
        _ => {}
    }

    // Check objects (level objects like spawns, lights, triggers, etc.)
    // Objects use screen distance for now (they're point markers)
    const OBJECT_THRESHOLD: f32 = 12.0;
    for (room_idx, room) in state.level.rooms.iter().enumerate() {
        for (obj_idx, obj) in room.objects.iter().enumerate() {
            let world_pos = obj.world_position(room);
            if let Some((sx, sy)) = world_to_screen(
                world_pos,
                state.camera_3d.position,
                state.camera_3d.basis_x,
                state.camera_3d.basis_y,
                state.camera_3d.basis_z,
                fb_width,
                fb_height,
            ) {
                let dist = ((mouse_fb_x - sx).powi(2) + (mouse_fb_y - sy).powi(2)).sqrt();
                if dist < OBJECT_THRESHOLD {
                    if result.object.map_or(true, |(_, _, best_dist)| dist < best_dist) {
                        result.object = Some((room_idx, obj_idx, dist));
                    }
                }
            }
        }
    }

    result
}

/// Interpolate depth along an edge based on the closest point to the mouse.
fn interpolate_edge_depth(
    mx: f32, my: f32,           // Mouse position
    x0: f32, y0: f32, d0: f32,  // Edge start (screen x, y, depth)
    x1: f32, y1: f32, d1: f32,  // Edge end (screen x, y, depth)
) -> f32 {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len_sq = dx * dx + dy * dy;

    if len_sq < 0.0001 {
        // Degenerate edge, return average depth
        return (d0 + d1) * 0.5;
    }

    // Project mouse onto edge line, clamped to [0, 1]
    let t = ((mx - x0) * dx + (my - y0) * dy) / len_sq;
    let t = t.clamp(0.0, 1.0);

    // Interpolate depth
    d0 + t * (d1 - d0)
}

/// Check if a point is inside a quad (4 corners) and return depth at that point if hit.
/// Uses world_to_screen_with_depth to get screen coords + depth for each corner,
/// then interpolates depth using barycentric coordinates.
fn check_quad_hit_with_depth(
    mouse_x: f32,
    mouse_y: f32,
    corners: &[Vec3; 4],
    camera: &crate::rasterizer::Camera,
    fb_width: usize,
    fb_height: usize,
) -> Option<f32> {
    use crate::rasterizer::world_to_screen_with_depth;

    // Get screen coords + depth for all 4 corners
    let projected: Vec<Option<(f32, f32, f32)>> = corners.iter().map(|&c| {
        world_to_screen_with_depth(c, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb_width, fb_height)
    }).collect();

    // Need all 4 corners visible
    let (sx0, sy0, d0) = projected[0]?;
    let (sx1, sy1, d1) = projected[1]?;
    let (sx2, sy2, d2) = projected[2]?;
    let (sx3, sy3, d3) = projected[3]?;

    // Check first triangle (0, 1, 2)
    if point_in_triangle_2d(mouse_x, mouse_y, sx0, sy0, sx1, sy1, sx2, sy2) {
        // Compute barycentric coords and interpolate depth
        let depth = interpolate_depth_in_triangle(
            mouse_x, mouse_y,
            sx0, sy0, d0,
            sx1, sy1, d1,
            sx2, sy2, d2,
        );
        return Some(depth);
    }

    // Check second triangle (0, 2, 3)
    if point_in_triangle_2d(mouse_x, mouse_y, sx0, sy0, sx2, sy2, sx3, sy3) {
        let depth = interpolate_depth_in_triangle(
            mouse_x, mouse_y,
            sx0, sy0, d0,
            sx2, sy2, d2,
            sx3, sy3, d3,
        );
        return Some(depth);
    }

    None
}

/// Interpolate depth at a point inside a triangle using barycentric coordinates
/// Uses signed area method for robustness
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
    // w0 = area(P, v1, v2) / area(v0, v1, v2)  -> weight for v0
    // w1 = area(v0, P, v2) / area(v0, v1, v2)  -> weight for v1
    // w2 = area(v0, v1, P) / area(v0, v1, v2)  -> weight for v2
    let w0 = ((x1 - px) * (y2 - py) - (x2 - px) * (y1 - py)) / area;
    let w1 = ((x2 - px) * (y0 - py) - (x0 - px) * (y2 - py)) / area;
    let w2 = 1.0 - w0 - w1;

    // Interpolate depth
    w0 * d0 + w1 * d1 + w2 * d2
}
