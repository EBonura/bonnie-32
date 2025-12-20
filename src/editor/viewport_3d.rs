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
    Framebuffer, Texture as RasterTexture, render_mesh, Color as RasterColor, Vec3,
    WIDTH, HEIGHT, WIDTH_HI, HEIGHT_HI,
    world_to_screen, world_to_screen_with_depth,
    point_to_segment_distance, point_in_triangle_2d,
    Light, RasterSettings,
};
use crate::world::SECTOR_SIZE;
use super::{EditorState, EditorTool, Selection, SectorFace, CameraMode};

/// Draw the 3D viewport using the software rasterizer
pub fn draw_viewport_3d(
    ctx: &mut UiContext,
    rect: Rect,
    state: &mut EditorState,
    textures: &[RasterTexture],
    fb: &mut Framebuffer,
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
    let should_update_orbit_target = handle_camera_input(ctx, state, inside_viewport, mouse_pos);

    // Toggle link coincident vertices mode with L key
    if inside_viewport && is_key_pressed(KeyCode::L) {
        state.link_coincident_vertices = !state.link_coincident_vertices;
        let mode = if state.link_coincident_vertices { "Linked" } else { "Independent" };
        state.set_status(&format!("Vertex mode: {}", mode), 2.0);
    }

    // Clear selection with Escape key
    if inside_viewport && is_key_pressed(KeyCode::Escape) && state.selection != Selection::None {
        state.selection = Selection::None;
        state.set_status("Selection cleared", 0.5);
    }

    // Delete selected face with Delete or Backspace key
    if inside_viewport && (is_key_pressed(KeyCode::Delete) || is_key_pressed(KeyCode::Backspace)) {
        if let Selection::SectorFace { room, x, z, face } = &state.selection {
            let (room_idx, gx, gz, face) = (*room, *x, *z, *face);

            // Check if there's something to delete before calling save_undo
            let has_face = match face {
                SectorFace::Floor => state.level.rooms.get(room_idx)
                    .and_then(|r| r.get_sector(gx, gz))
                    .map(|s| s.floor.is_some())
                    .unwrap_or(false),
                SectorFace::Ceiling => state.level.rooms.get(room_idx)
                    .and_then(|r| r.get_sector(gx, gz))
                    .map(|s| s.ceiling.is_some())
                    .unwrap_or(false),
                SectorFace::WallNorth(i) => state.level.rooms.get(room_idx)
                    .and_then(|r| r.get_sector(gx, gz))
                    .map(|s| s.walls_north.get(i).is_some())
                    .unwrap_or(false),
                SectorFace::WallEast(i) => state.level.rooms.get(room_idx)
                    .and_then(|r| r.get_sector(gx, gz))
                    .map(|s| s.walls_east.get(i).is_some())
                    .unwrap_or(false),
                SectorFace::WallSouth(i) => state.level.rooms.get(room_idx)
                    .and_then(|r| r.get_sector(gx, gz))
                    .map(|s| s.walls_south.get(i).is_some())
                    .unwrap_or(false),
                SectorFace::WallWest(i) => state.level.rooms.get(room_idx)
                    .and_then(|r| r.get_sector(gx, gz))
                    .map(|s| s.walls_west.get(i).is_some())
                    .unwrap_or(false),
            };

            if has_face {
                state.save_undo();

                let deleted = match face {
                    SectorFace::Floor => {
                        if let Some(room) = state.level.rooms.get_mut(room_idx) {
                            if let Some(sector) = room.get_sector_mut(gx, gz) {
                                sector.floor = None;
                            }
                            room.recalculate_bounds();
                            Some("floor")
                        } else { None }
                    }
                    SectorFace::Ceiling => {
                        if let Some(room) = state.level.rooms.get_mut(room_idx) {
                            if let Some(sector) = room.get_sector_mut(gx, gz) {
                                sector.ceiling = None;
                            }
                            room.recalculate_bounds();
                            Some("ceiling")
                        } else { None }
                    }
                    SectorFace::WallNorth(i) => {
                        if let Some(room) = state.level.rooms.get_mut(room_idx) {
                            if let Some(sector) = room.get_sector_mut(gx, gz) {
                                if i < sector.walls_north.len() {
                                    sector.walls_north.remove(i);
                                }
                            }
                            room.recalculate_bounds();
                            Some("north wall")
                        } else { None }
                    }
                    SectorFace::WallEast(i) => {
                        if let Some(room) = state.level.rooms.get_mut(room_idx) {
                            if let Some(sector) = room.get_sector_mut(gx, gz) {
                                if i < sector.walls_east.len() {
                                    sector.walls_east.remove(i);
                                }
                            }
                            room.recalculate_bounds();
                            Some("east wall")
                        } else { None }
                    }
                    SectorFace::WallSouth(i) => {
                        if let Some(room) = state.level.rooms.get_mut(room_idx) {
                            if let Some(sector) = room.get_sector_mut(gx, gz) {
                                if i < sector.walls_south.len() {
                                    sector.walls_south.remove(i);
                                }
                            }
                            room.recalculate_bounds();
                            Some("south wall")
                        } else { None }
                    }
                    SectorFace::WallWest(i) => {
                        if let Some(room) = state.level.rooms.get_mut(room_idx) {
                            if let Some(sector) = room.get_sector_mut(gx, gz) {
                                if i < sector.walls_west.len() {
                                    sector.walls_west.remove(i);
                                }
                            }
                            room.recalculate_bounds();
                            Some("west wall")
                        } else { None }
                    }
                };

                if let Some(type_name) = deleted {
                    state.set_selection(Selection::None);
                    state.mark_portals_dirty();
                    state.set_status(&format!("Deleted {}", type_name), 2.0);
                }
            }
        }
    }

    // Collect all vertex positions for the current room (for drawing and selection)
    let all_vertices = collect_room_vertices(state);

    // Find hovered elements using 2D screen-space projection
    // Priority: vertex > edge > face
    let mut preview_sector: Option<(f32, f32, f32, bool)> = None; // (x, z, target_y, is_occupied)
    let mut preview_wall: Option<(f32, f32, crate::world::Direction, f32, f32, bool)> = None; // (x, z, direction, y_bottom, y_top, is_occupied)

    let hover = if inside_viewport && !ctx.mouse.right_down &&
        (state.tool == EditorTool::Select || state.tool == EditorTool::PlaceLight) {
        screen_to_fb(mouse_pos.0, mouse_pos.1)
            .map(|mouse_fb| find_hovered_elements(state, mouse_fb, fb_width, fb_height, &all_vertices))
            .unwrap_or_default()
    } else {
        HoverResult::default()
    };

    let hovered_vertex = hover.vertex;
    let hovered_edge = hover.edge;
    let hovered_face = hover.face;
    let hovered_light = hover.light;

    // In drawing modes, find preview sector position
    if inside_viewport && (state.tool == EditorTool::DrawFloor || state.tool == EditorTool::DrawCeiling) {
        if let Some((mouse_fb_x, mouse_fb_y)) = screen_to_fb(mouse_pos.0, mouse_pos.1) {
            use super::{CEILING_HEIGHT, CLICK_HEIGHT};

            let is_floor = state.tool == EditorTool::DrawFloor;

            // Use placement_target_y, but initialize to sensible default if zero
            let target_y = if state.placement_target_y == 0.0 && !state.height_adjust_mode {
                // Default: floor at 0, ceiling at CEILING_HEIGHT
                if is_floor { 0.0 } else { CEILING_HEIGHT }
            } else {
                state.placement_target_y
            };

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
                        let test_pos = Vec3::new(grid_x + SECTOR_SIZE / 2.0, target_y, grid_z + SECTOR_SIZE / 2.0);

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
            let mut closest_edge: Option<(f32, f32, Direction, f32)> = None; // (grid_x, grid_z, direction, screen_dist)

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
                            if closest_edge.map_or(true, |(_, _, _, best_dist)| dist < best_dist) {
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
                                closest_edge = Some((final_grid_x, final_grid_z, dir, dist));
                            }
                        }
                    }
                }
            }

            if let Some((grid_x, grid_z, dir, dist)) = closest_edge {
                if dist < 80.0 {
                    // Check if this edge already has a wall
                    let occupied = if let Some(room) = state.level.rooms.get(state.current_room) {
                        if let Some((gx, gz)) = room.world_to_grid(grid_x + SECTOR_SIZE * 0.5, grid_z + SECTOR_SIZE * 0.5) {
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

                    preview_wall = Some((grid_x, grid_z, dir, default_y_bottom, default_y_top, occupied));
                }
            }
        }
    }

    // Handle clicks and dragging in 3D viewport
    if inside_viewport && !ctx.mouse.right_down {
        // Detect Shift key for multi-select
        let shift_down = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);

        // Start dragging or select on left press
        if ctx.mouse.left_pressed {
            if state.tool == EditorTool::Select {
                // Priority: vertex > edge > face
                if let Some((room_idx, gx, gz, corner_idx, face, _)) = hovered_vertex {
                    // Set selection to the face this vertex belongs to (shows properties panel)
                    let new_selection = Selection::SectorFace { room: room_idx, x: gx, z: gz, face: face.clone() };
                    if shift_down {
                        state.toggle_multi_selection(new_selection.clone());
                    } else {
                        state.clear_multi_selection();
                    }
                    // Use direct assignment to preserve vertex index selection
                    state.selection = new_selection;

                    // Select this vertex index for color editing (keeps selection, just changes vertex)
                    state.selected_vertex_indices.clear();
                    state.selected_vertex_indices.push(corner_idx);

                    // Start dragging vertex
                    state.dragging_sector_vertices.clear();
                    state.drag_initial_heights.clear();
                    state.viewport_drag_started = false;

                    // Store the vertex to drag
                    state.dragging_sector_vertices.push((room_idx, gx, gz, face, corner_idx));

                    // Get initial height
                    if let Some(room) = state.level.rooms.get(room_idx) {
                        if let Some(sector) = room.get_sector(gx, gz) {
                            let height = match face {
                                SectorFace::Floor => sector.floor.as_ref().map(|f| f.heights[corner_idx]),
                                SectorFace::Ceiling => sector.ceiling.as_ref().map(|c| c.heights[corner_idx]),
                                SectorFace::WallNorth(i) => sector.walls_north.get(i).map(|w| w.heights[corner_idx]),
                                SectorFace::WallEast(i) => sector.walls_east.get(i).map(|w| w.heights[corner_idx]),
                                SectorFace::WallSouth(i) => sector.walls_south.get(i).map(|w| w.heights[corner_idx]),
                                SectorFace::WallWest(i) => sector.walls_west.get(i).map(|w| w.heights[corner_idx]),
                            };
                            if let Some(h) = height {
                                state.drag_initial_heights.push(h);
                                state.viewport_drag_plane_y = h;
                            }
                        }
                    }

                    // If linking mode is on, find coincident vertices
                    if state.link_coincident_vertices {
                        // Get position of clicked vertex
                        if let Some((world_pos, _, _, _, _, _)) = all_vertices.iter()
                            .find(|(_, ri, vgx, vgz, ci, f)| *ri == room_idx && *vgx == gx && *vgz == gz && *ci == corner_idx && *f == face)
                        {
                            const EPSILON: f32 = 0.1;
                            // Find all vertices at same position
                            for (pos, ri, vgx, vgz, ci, vface) in &all_vertices {
                                if (pos.x - world_pos.x).abs() < EPSILON &&
                                   (pos.y - world_pos.y).abs() < EPSILON &&
                                   (pos.z - world_pos.z).abs() < EPSILON {
                                    let key = (*ri, *vgx, *vgz, *vface, *ci);
                                    if !state.dragging_sector_vertices.contains(&key) {
                                        state.dragging_sector_vertices.push(key);
                                        state.drag_initial_heights.push(pos.y);
                                    }
                                }
                            }
                        }
                    }
                } else if let Some((room_idx, gx, gz, face_idx, edge_idx, wall_face, _)) = hovered_edge {
                    // Start dragging edge (both vertices)
                    state.dragging_sector_vertices.clear();
                    state.drag_initial_heights.clear();
                    state.viewport_drag_started = false;

                    // Convert edge to face selection so properties panel shows face with edge vertices selected
                    // face_idx: 0=floor, 1=ceiling, 2=wall
                    let face_for_selection = match face_idx {
                        0 => SectorFace::Floor,
                        1 => SectorFace::Ceiling,
                        _ => wall_face.clone().unwrap_or(SectorFace::Floor),
                    };
                    let new_selection = Selection::SectorFace {
                        room: room_idx,
                        x: gx,
                        z: gz,
                        face: face_for_selection,
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
                    // Use direct assignment to preserve vertex selection
                    state.selection = new_selection;

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

                    // Collect all edges to drag: start with the clicked edge
                    let mut edges_to_drag: Vec<(usize, usize, usize, usize, usize, Option<SectorFace>)> = Vec::new(); // (room, gx, gz, face_idx, edge_idx, wall_face)

                    // Add the clicked edge
                    edges_to_drag.push((room_idx, gx, gz, face_idx, edge_idx, wall_face.clone()));

                    // Add vertices for all edges to drag
                    let mut avg_height = 0.0;
                    let mut height_count = 0;

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
                                            avg_height += h[corner0];
                                            height_count += 1;
                                        }
                                        let key1 = (*r_idx, *gx, *gz, face, corner1);
                                        if !state.dragging_sector_vertices.contains(&key1) {
                                            state.dragging_sector_vertices.push(key1);
                                            state.drag_initial_heights.push(h[corner1]);
                                            avg_height += h[corner1];
                                            height_count += 1;
                                        }

                                        // If linking, find coincident vertices for the edge
                                        if state.link_coincident_vertices {
                                            let base_x = room.position.x + (*gx as f32) * SECTOR_SIZE;
                                            let base_z = room.position.z + (*gz as f32) * SECTOR_SIZE;

                                            let edge_positions = [
                                                match corner0 {
                                                    0 => Vec3::new(base_x, h[0], base_z),
                                                    1 => Vec3::new(base_x + SECTOR_SIZE, h[1], base_z),
                                                    2 => Vec3::new(base_x + SECTOR_SIZE, h[2], base_z + SECTOR_SIZE),
                                                    3 => Vec3::new(base_x, h[3], base_z + SECTOR_SIZE),
                                                    _ => unreachable!(),
                                                },
                                                match corner1 {
                                                    0 => Vec3::new(base_x, h[0], base_z),
                                                    1 => Vec3::new(base_x + SECTOR_SIZE, h[1], base_z),
                                                    2 => Vec3::new(base_x + SECTOR_SIZE, h[2], base_z + SECTOR_SIZE),
                                                    3 => Vec3::new(base_x, h[3], base_z + SECTOR_SIZE),
                                                    _ => unreachable!(),
                                                },
                                            ];

                                            const EPSILON: f32 = 0.1;
                                            for (pos, ri, vgx, vgz, ci, vface) in &all_vertices {
                                                for ep in &edge_positions {
                                                    if (pos.x - ep.x).abs() < EPSILON &&
                                                       (pos.y - ep.y).abs() < EPSILON &&
                                                       (pos.z - ep.z).abs() < EPSILON {
                                                        let key = (*ri, *vgx, *vgz, *vface, *ci);
                                                        if !state.dragging_sector_vertices.contains(&key) {
                                                            state.dragging_sector_vertices.push(key);
                                                            state.drag_initial_heights.push(pos.y);
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
                                            _ => None,
                                        };

                                        if let Some(h) = wall_heights {
                                            // Add both edge vertices
                                            let key0 = (*r_idx, *gx, *gz, *wall_face, corner0);
                                            if !state.dragging_sector_vertices.contains(&key0) {
                                                state.dragging_sector_vertices.push(key0);
                                                state.drag_initial_heights.push(h[corner0]);
                                                avg_height += h[corner0];
                                                height_count += 1;
                                            }
                                            let key1 = (*r_idx, *gx, *gz, *wall_face, corner1);
                                            if !state.dragging_sector_vertices.contains(&key1) {
                                                state.dragging_sector_vertices.push(key1);
                                                state.drag_initial_heights.push(h[corner1]);
                                                avg_height += h[corner1];
                                                height_count += 1;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Set drag plane to average height
                    if height_count > 0 {
                        state.viewport_drag_plane_y = avg_height / height_count as f32;
                    }
                } else if let Some((room_idx, light_idx, _)) = hovered_light {
                    // Light selection/dragging - checked before faces so lights are clickable
                    let is_already_selected = matches!(&state.selection,
                        Selection::Light { room, light } if *room == room_idx && *light == light_idx);

                    if is_already_selected {
                        // Start dragging the light (Y-axis)
                        if let Some(room) = state.level.rooms.get(room_idx) {
                            if let Some(light) = room.lights.get(light_idx) {
                                state.dragging_light = Some((room_idx, light_idx));
                                state.dragging_light_initial_y = light.position.y;
                                state.dragging_light_plane_y = light.position.y;
                            }
                        }
                    } else {
                        // Select light
                        state.selection = Selection::Light { room: room_idx, light: light_idx };
                        state.set_status("Light selected", 1.0);
                    }
                } else if let Some((room_idx, gx, gz, face)) = hovered_face {
                    // Start dragging face (all 4 vertices)
                    state.dragging_sector_vertices.clear();
                    state.drag_initial_heights.clear();
                    state.viewport_drag_started = false;

                    // Handle selection first (Shift = toggle multi-select)
                    let new_selection = Selection::SectorFace { room: room_idx, x: gx, z: gz, face };
                    if shift_down {
                        state.toggle_multi_selection(new_selection.clone());
                        state.set_selection(new_selection.clone());
                    } else {
                        // If clicking on something already selected, keep multi-selection for dragging
                        let clicking_selected = state.selection == new_selection ||
                            state.multi_selection.contains(&new_selection);
                        if !clicking_selected {
                            state.clear_multi_selection();
                        }
                        state.set_selection(new_selection.clone());
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
                    let mut avg_height = 0.0;
                    let mut height_count = 0;

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
                                            avg_height += h[corner];
                                            height_count += 1;
                                        }
                                    }

                                    // If linking, find coincident vertices
                                    if state.link_coincident_vertices {
                                        let base_x = room.position.x + (*gx as f32) * SECTOR_SIZE;
                                        let base_z = room.position.z + (*gz as f32) * SECTOR_SIZE;
                                        let face_positions = [
                                            Vec3::new(base_x, h[0], base_z),
                                            Vec3::new(base_x + SECTOR_SIZE, h[1], base_z),
                                            Vec3::new(base_x + SECTOR_SIZE, h[2], base_z + SECTOR_SIZE),
                                            Vec3::new(base_x, h[3], base_z + SECTOR_SIZE),
                                        ];

                                        const EPSILON: f32 = 0.1;
                                        for (pos, ri, vgx, vgz, ci, vface) in &all_vertices {
                                            for fp in &face_positions {
                                                if (pos.x - fp.x).abs() < EPSILON &&
                                                   (pos.y - fp.y).abs() < EPSILON &&
                                                   (pos.z - fp.z).abs() < EPSILON {
                                                    let key = (*ri, *vgx, *vgz, *vface, *ci);
                                                    if !state.dragging_sector_vertices.contains(&key) {
                                                        state.dragging_sector_vertices.push(key);
                                                        state.drag_initial_heights.push(pos.y);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // Handle wall dragging
                                match face {
                                    SectorFace::WallNorth(i) | SectorFace::WallEast(i) |
                                    SectorFace::WallSouth(i) | SectorFace::WallWest(i) => {
                                        let walls = match face {
                                            SectorFace::WallNorth(_) => &sector.walls_north,
                                            SectorFace::WallEast(_) => &sector.walls_east,
                                            SectorFace::WallSouth(_) => &sector.walls_south,
                                            SectorFace::WallWest(_) => &sector.walls_west,
                                            _ => unreachable!(),
                                        };
                                        if let Some(wall) = walls.get(*i) {
                                            for corner in 0..4 {
                                                let key = (*r_idx, *gx, *gz, *face, corner);
                                                if !state.dragging_sector_vertices.contains(&key) {
                                                    state.dragging_sector_vertices.push(key);
                                                    state.drag_initial_heights.push(wall.heights[corner]);
                                                    avg_height += wall.heights[corner];
                                                    height_count += 1;
                                                }
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }

                    // Set drag plane to average height of all dragged vertices
                    if height_count > 0 {
                        state.viewport_drag_plane_y = avg_height / height_count as f32;
                    }
                } else {
                    // Clicked on nothing - clear selection (unless Shift is held)
                    if !shift_down {
                        state.set_selection(Selection::None);
                        state.clear_multi_selection();
                    }
                }
            }
            // Drawing modes - place floor/ceiling
            else if state.tool == EditorTool::DrawFloor || state.tool == EditorTool::DrawCeiling {
                if let Some((snapped_x, snapped_z, target_y, occupied)) = preview_sector {
                    let is_floor = state.tool == EditorTool::DrawFloor;

                    if occupied {
                        let type_name = if is_floor { "floor" } else { "ceiling" };
                        state.set_status(&format!("Sector already has a {}", type_name), 2.0);
                    } else {
                        state.save_undo();

                        // Get texture and room position before borrowing mutably
                        let texture = state.selected_texture.clone();
                        let room_pos = state.level.rooms.get(state.current_room)
                            .map(|r| r.position)
                            .unwrap_or_default();

                        if let Some(room) = state.level.rooms.get_mut(state.current_room) {
                            // Convert world coords to local coords (can be negative)
                            let local_x = snapped_x - room_pos.x;
                            let local_z = snapped_z - room_pos.z;

                            // Calculate grid coords, handling negative values
                            let mut gx = (local_x / SECTOR_SIZE).floor() as i32;
                            let mut gz = (local_z / SECTOR_SIZE).floor() as i32;

                            // Expand grid in negative X direction if needed
                            while gx < 0 {
                                // Shift room position by one sector in -X
                                room.position.x -= SECTOR_SIZE;
                                // Insert new column at front
                                room.sectors.insert(0, (0..room.depth).map(|_| None).collect());
                                room.width += 1;
                                gx += 1; // Grid index shifts up
                            }

                            // Expand grid in negative Z direction if needed
                            while gz < 0 {
                                // Shift room position by one sector in -Z
                                room.position.z -= SECTOR_SIZE;
                                // Insert new row at front of each column
                                for col in &mut room.sectors {
                                    col.insert(0, None);
                                }
                                room.depth += 1;
                                gz += 1; // Grid index shifts up
                            }

                            // Now gx and gz are guaranteed >= 0, convert to usize
                            let gx = gx as usize;
                            let gz = gz as usize;

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

                            if is_floor {
                                room.set_floor(gx, gz, target_y, texture);
                            } else {
                                room.set_ceiling(gx, gz, target_y, texture);
                            }
                            room.recalculate_bounds();
                        }

                        state.mark_portals_dirty();
                        let status = if is_floor { "Created floor sector" } else { "Created ceiling sector" };
                        state.set_status(status, 2.0);
                    }
                }
            }
            // DrawWall mode - place wall on sector edge
            else if state.tool == EditorTool::DrawWall {
                if let Some((grid_x, grid_z, dir, y_bottom, y_top, occupied)) = preview_wall {
                    use crate::world::{Direction, VerticalFace};

                    if occupied {
                        state.set_status("Edge already has a wall", 2.0);
                    } else {
                        state.save_undo();

                        let texture = state.selected_texture.clone();
                        let room_pos = state.level.rooms.get(state.current_room)
                            .map(|r| r.position)
                            .unwrap_or_default();

                        if let Some(room) = state.level.rooms.get_mut(state.current_room) {
                            // Convert world coords to local coords (can be negative)
                            let local_x = grid_x - room_pos.x;
                            let local_z = grid_z - room_pos.z;

                            // Calculate grid coords, handling negative values
                            let mut gx = (local_x / SECTOR_SIZE).floor() as i32;
                            let mut gz = (local_z / SECTOR_SIZE).floor() as i32;

                            // Expand grid in negative X direction if needed
                            while gx < 0 {
                                room.position.x -= SECTOR_SIZE;
                                room.sectors.insert(0, (0..room.depth).map(|_| None).collect());
                                room.width += 1;
                                gx += 1;
                            }

                            // Expand grid in negative Z direction if needed
                            while gz < 0 {
                                room.position.z -= SECTOR_SIZE;
                                for col in &mut room.sectors {
                                    col.insert(0, None);
                                }
                                room.depth += 1;
                                gz += 1;
                            }

                            // Expand in positive direction if needed
                            let gx = gx as usize;
                            let gz = gz as usize;
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

                            // Create the wall
                            let wall = VerticalFace::new(y_bottom, y_top, texture);

                            // Ensure sector exists and add wall
                            let sector = room.ensure_sector(gx, gz);
                            match dir {
                                Direction::North => sector.walls_north.push(wall),
                                Direction::East => sector.walls_east.push(wall),
                                Direction::South => sector.walls_south.push(wall),
                                Direction::West => sector.walls_west.push(wall),
                            }
                            room.recalculate_bounds();
                        }

                        state.mark_portals_dirty();
                        let dir_name = match dir {
                            Direction::North => "north",
                            Direction::East => "east",
                            Direction::South => "south",
                            Direction::West => "west",
                        };
                        state.set_status(&format!("Created {} wall", dir_name), 2.0);
                    }
                }
            }
            // PlaceLight mode - select existing lights (placement is in 2D grid view)
            else if state.tool == EditorTool::PlaceLight {
                if let Some((room_idx, light_idx, _)) = hovered_light {
                    state.selection = Selection::Light { room: room_idx, light: light_idx };
                    state.set_status("Light selected", 1.0);
                } else {
                    state.set_status("Use 2D grid view to place lights", 2.0);
                }
            }
        }

        // Continue dragging (Y-axis only - TRLE constraint)
        if ctx.mouse.left_down && !state.dragging_sector_vertices.is_empty() {
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
                                SectorFace::WallSouth(wi) | SectorFace::WallWest(wi) => {
                                    let walls = match face {
                                        SectorFace::WallNorth(_) => &mut sector.walls_north,
                                        SectorFace::WallEast(_) => &mut sector.walls_east,
                                        SectorFace::WallSouth(_) => &mut sector.walls_south,
                                        SectorFace::WallWest(_) => &mut sector.walls_west,
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

        // Continue dragging light (Y-axis only)
        if ctx.mouse.left_down && state.dragging_light.is_some() {
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
            state.dragging_light_plane_y += y_delta;

            // Calculate snapped height
            let delta_from_initial = state.dragging_light_plane_y - state.dragging_light_initial_y;
            let new_y = state.dragging_light_initial_y + delta_from_initial;
            let snapped_y = (new_y / CLICK_HEIGHT).round() * CLICK_HEIGHT;

            // Apply to light
            if let Some((room_idx, light_idx)) = state.dragging_light {
                if let Some(room) = state.level.rooms.get_mut(room_idx) {
                    if let Some(light) = room.lights.get_mut(light_idx) {
                        light.position.y = snapped_y;
                    }
                }
            }
        }

        // End dragging on release
        if ctx.mouse.left_released {
            // If we actually dragged geometry, recalculate room bounds
            if state.viewport_drag_started {
                if let Some(room) = state.level.rooms.get_mut(state.current_room) {
                    room.recalculate_bounds();
                }
                state.mark_portals_dirty();
            }
            state.dragging_sector_vertices.clear();
            state.drag_initial_heights.clear();
            state.dragging_light = None;
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

    // Clear framebuffer
    fb.clear(RasterColor::new(30, 30, 40));

    // Draw main floor grid (large, fixed extent)
    if state.show_grid {
        let grid_color = RasterColor::new(50, 50, 60);
        let grid_size = state.grid_size;
        let grid_extent = 10240.0; // Cover approximately 10 sectors in each direction

        // Calculate grid Y position based on lowest point in all rooms
        let grid_y = 0.0;

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

            let grid_y = 0.0;

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
        }
    }

    // Draw hovering ceiling grid centered on hovered tile when in ceiling placement mode
    if let Some((snapped_x, snapped_z, _, _)) = preview_sector {
        if state.tool == EditorTool::DrawCeiling {
            use super::CEILING_HEIGHT;

            let inner_color = RasterColor::new(140, 100, 180); // Purple (bright)
            let outer_color = RasterColor::new(70, 50, 90);    // Purple (dim)

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
                    Vec3::new(center_x - outer_half, CEILING_HEIGHT, z),
                    Vec3::new(center_x + outer_half, CEILING_HEIGHT, z),
                    &state.camera_3d,
                    color,
                );

                let x = center_x + offset;
                draw_3d_line(
                    fb,
                    Vec3::new(x, CEILING_HEIGHT, center_z - outer_half),
                    Vec3::new(x, CEILING_HEIGHT, center_z + outer_half),
                    &state.camera_3d,
                    color,
                );
            }
        }
    }

    // Draw wall preview when in DrawWall mode
    if let Some((grid_x, grid_z, dir, y_bottom, y_top, occupied)) = preview_wall {
        use crate::world::Direction;

        // Wall edge corners based on direction
        let (p0, p1) = match dir {
            Direction::North => (
                Vec3::new(grid_x, y_bottom, grid_z),
                Vec3::new(grid_x + SECTOR_SIZE, y_bottom, grid_z),
            ),
            Direction::East => (
                Vec3::new(grid_x + SECTOR_SIZE, y_bottom, grid_z),
                Vec3::new(grid_x + SECTOR_SIZE, y_bottom, grid_z + SECTOR_SIZE),
            ),
            Direction::South => (
                Vec3::new(grid_x + SECTOR_SIZE, y_bottom, grid_z + SECTOR_SIZE),
                Vec3::new(grid_x, y_bottom, grid_z + SECTOR_SIZE),
            ),
            Direction::West => (
                Vec3::new(grid_x, y_bottom, grid_z + SECTOR_SIZE),
                Vec3::new(grid_x, y_bottom, grid_z),
            ),
        };

        let p2 = Vec3::new(p1.x, y_top, p1.z);
        let p3 = Vec3::new(p0.x, y_top, p0.z);

        // Color: teal for valid, red for occupied
        let color = if occupied {
            RasterColor::new(200, 80, 80)
        } else {
            RasterColor::new(80, 200, 180)
        };

        // Draw wall outline (rectangle)
        draw_3d_line(fb, p0, p1, &state.camera_3d, color);
        draw_3d_line(fb, p1, p2, &state.camera_3d, color);
        draw_3d_line(fb, p2, p3, &state.camera_3d, color);
        draw_3d_line(fb, p3, p0, &state.camera_3d, color);

        // Draw X through it if occupied
        if occupied {
            draw_3d_line(fb, p0, p2, &state.camera_3d, color);
            draw_3d_line(fb, p1, p3, &state.camera_3d, color);
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

    // Build lighting settings: when Gouraud is ON, use room lights; when OFF, no shading
    let render_settings = if state.raster_settings.shading != crate::rasterizer::ShadingMode::None {
        // Collect all enabled lights from all rooms
        let mut lights = Vec::new();
        for room in &state.level.rooms {
            for room_light in &room.lights {
                if room_light.enabled {
                    // Convert room-local position to world position
                    let world_pos = Vec3::new(
                        room.position.x + room_light.position.x,
                        room.position.y + room_light.position.y,
                        room.position.z + room_light.position.z,
                    );
                    let mut light = Light::point(world_pos, room_light.radius, room_light.intensity);
                    light.color = room_light.color;
                    lights.push(light);
                }
            }
        }
        // Use room ambient from current room (or default)
        let ambient = state.level.rooms.get(state.current_room)
            .map(|r| r.ambient)
            .unwrap_or(0.5);
        // Create modified settings with room lights
        RasterSettings {
            lights,
            ambient,
            ..state.raster_settings.clone()
        }
    } else {
        state.raster_settings.clone()
    };

    // Render all rooms (skip hidden ones)
    for (room_idx, room) in state.level.rooms.iter().enumerate() {
        // Skip hidden rooms
        if state.hidden_rooms.contains(&room_idx) {
            continue;
        }
        let (vertices, faces) = room.to_render_data_with_textures(&resolve_texture);

        render_mesh(fb, &vertices, &faces, textures, &state.camera_3d, &render_settings);
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
                    fb.draw_line_3d(x0, y0, z0, x1, y1, z1, room_color);
                }
            }

            // Draw portal outlines for this room
            let portal_color = RasterColor::new(255, 100, 255); // Magenta
            for portal in &room.portals {
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

                // Draw portal quad outline
                let portal_edges = [(0, 1), (1, 2), (2, 3), (3, 0)];
                for (i, j) in portal_edges {
                    if let (Some((x0, y0, z0)), Some((x1, y1, z1))) = (screen_verts[i], screen_verts[j]) {
                        fb.draw_line_3d(x0, y0, z0, x1, y1, z1, portal_color);
                    }
                }
            }

            // Draw light gizmos for this room
            for (light_idx, light) in room.lights.iter().enumerate() {
                // Light position in world space
                let world_pos = Vec3::new(
                    room.position.x + light.position.x,
                    room.position.y + light.position.y,
                    room.position.z + light.position.z,
                );

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
                    // Light color
                    let light_color = if light.enabled {
                        RasterColor::new(light.color.r, light.color.g, light.color.b)
                    } else {
                        RasterColor::new(80, 80, 80) // Dim gray for disabled
                    };

                    // Check if this light is selected
                    let is_selected = matches!(&state.selection, Selection::Light { room: r, light: l }
                        if *r == room_idx && *l == light_idx);

                    // Draw light gizmo
                    let radius = if is_selected { 8 } else { 5 };

                    // Selection highlight (white circle behind)
                    if is_selected {
                        fb.draw_circle(fb_x as i32, fb_y as i32, radius + 3, RasterColor::new(255, 255, 255));
                    }

                    fb.draw_circle(fb_x as i32, fb_y as i32, radius, light_color);

                    // Sun rays for enabled lights
                    if light.enabled {
                        let ray_len = radius as i32 + 4;
                        let offsets = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
                        for (dx, dy) in offsets {
                            let x0 = fb_x as i32 + dx * (radius as i32 + 1);
                            let y0 = fb_y as i32 + dy * (radius as i32 + 1);
                            let x1 = fb_x as i32 + dx * ray_len;
                            let y1 = fb_y as i32 + dy * ray_len;
                            fb.draw_line_3d(x0, y0, 0.0, x1, y1, 0.0, light_color);
                        }
                    }
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

                // Only draw vertex dot when hovered (no gray dots for unselected vertices)
                if is_hovered {
                    let color = RasterColor::new(255, 200, 150); // Orange when hovered
                    fb.draw_circle(fb_x as i32, fb_y as i32, 4, color);
                }
            }
        }
    }

    // Draw hovered edge highlight directly into framebuffer
    if let Some((room_idx, gx, gz, face_idx, edge_idx, wall_face_opt, _)) = hovered_edge {
        if let Some(room) = state.level.rooms.get(room_idx) {
            if let Some(sector) = room.get_sector(gx, gz) {
                let base_x = room.position.x + (gx as f32) * SECTOR_SIZE;
                let base_z = room.position.z + (gz as f32) * SECTOR_SIZE;

                let edge_color = RasterColor::new(255, 200, 100); // Orange for edge hover

                // Get edge vertices based on face_idx
                let corners: Option<[Vec3; 4]> = match face_idx {
                    0 => sector.floor.as_ref().map(|f| [
                        Vec3::new(base_x, f.heights[0], base_z),
                        Vec3::new(base_x + SECTOR_SIZE, f.heights[1], base_z),
                        Vec3::new(base_x + SECTOR_SIZE, f.heights[2], base_z + SECTOR_SIZE),
                        Vec3::new(base_x, f.heights[3], base_z + SECTOR_SIZE),
                    ]),
                    1 => sector.ceiling.as_ref().map(|c| [
                        Vec3::new(base_x, c.heights[0], base_z),
                        Vec3::new(base_x + SECTOR_SIZE, c.heights[1], base_z),
                        Vec3::new(base_x + SECTOR_SIZE, c.heights[2], base_z + SECTOR_SIZE),
                        Vec3::new(base_x, c.heights[3], base_z + SECTOR_SIZE),
                    ]),
                    2 => {
                        // Wall edge - get corners from the specific wall
                        if let Some(wf) = &wall_face_opt {
                            let (x0, z0, x1, z1) = match wf {
                                SectorFace::WallNorth(_) => (base_x, base_z, base_x + SECTOR_SIZE, base_z),
                                SectorFace::WallEast(_) => (base_x + SECTOR_SIZE, base_z, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE),
                                SectorFace::WallSouth(_) => (base_x + SECTOR_SIZE, base_z + SECTOR_SIZE, base_x, base_z + SECTOR_SIZE),
                                SectorFace::WallWest(_) => (base_x, base_z + SECTOR_SIZE, base_x, base_z),
                                _ => (0.0, 0.0, 0.0, 0.0),
                            };
                            let wall_heights = match wf {
                                SectorFace::WallNorth(i) => sector.walls_north.get(*i).map(|w| w.heights),
                                SectorFace::WallEast(i) => sector.walls_east.get(*i).map(|w| w.heights),
                                SectorFace::WallSouth(i) => sector.walls_south.get(*i).map(|w| w.heights),
                                SectorFace::WallWest(i) => sector.walls_west.get(*i).map(|w| w.heights),
                                _ => None,
                            };
                            wall_heights.map(|h| [
                                Vec3::new(x0, h[0], z0),
                                Vec3::new(x1, h[1], z1),
                                Vec3::new(x1, h[2], z1),
                                Vec3::new(x0, h[3], z0),
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

                    let hover_color = RasterColor::new(150, 200, 255); // Light blue for hover

                    match face {
                        SectorFace::Floor => {
                            if let Some(floor) = &sector.floor {
                                let corners = [
                                    Vec3::new(base_x, floor.heights[0], base_z),
                                    Vec3::new(base_x + SECTOR_SIZE, floor.heights[1], base_z),
                                    Vec3::new(base_x + SECTOR_SIZE, floor.heights[2], base_z + SECTOR_SIZE),
                                    Vec3::new(base_x, floor.heights[3], base_z + SECTOR_SIZE),
                                ];
                                for i in 0..4 {
                                    draw_3d_line(fb, corners[i], corners[(i + 1) % 4], &state.camera_3d, hover_color);
                                }
                                // Draw diagonal to show it's a face
                                draw_3d_line(fb, corners[0], corners[2], &state.camera_3d, hover_color);
                            }
                        }
                        SectorFace::Ceiling => {
                            if let Some(ceiling) = &sector.ceiling {
                                let corners = [
                                    Vec3::new(base_x, ceiling.heights[0], base_z),
                                    Vec3::new(base_x + SECTOR_SIZE, ceiling.heights[1], base_z),
                                    Vec3::new(base_x + SECTOR_SIZE, ceiling.heights[2], base_z + SECTOR_SIZE),
                                    Vec3::new(base_x, ceiling.heights[3], base_z + SECTOR_SIZE),
                                ];
                                for i in 0..4 {
                                    draw_3d_line(fb, corners[i], corners[(i + 1) % 4], &state.camera_3d, hover_color);
                                }
                                draw_3d_line(fb, corners[0], corners[2], &state.camera_3d, hover_color);
                            }
                        }
                        SectorFace::WallNorth(i) => {
                            if let Some(wall) = sector.walls_north.get(i) {
                                let p0 = Vec3::new(base_x, wall.heights[0], base_z);
                                let p1 = Vec3::new(base_x + SECTOR_SIZE, wall.heights[1], base_z);
                                let p2 = Vec3::new(base_x + SECTOR_SIZE, wall.heights[2], base_z);
                                let p3 = Vec3::new(base_x, wall.heights[3], base_z);
                                draw_3d_line(fb, p0, p1, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p1, p2, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p2, p3, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p3, p0, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p0, p2, &state.camera_3d, hover_color);
                            }
                        }
                        SectorFace::WallEast(i) => {
                            if let Some(wall) = sector.walls_east.get(i) {
                                let p0 = Vec3::new(base_x + SECTOR_SIZE, wall.heights[0], base_z);
                                let p1 = Vec3::new(base_x + SECTOR_SIZE, wall.heights[1], base_z + SECTOR_SIZE);
                                let p2 = Vec3::new(base_x + SECTOR_SIZE, wall.heights[2], base_z + SECTOR_SIZE);
                                let p3 = Vec3::new(base_x + SECTOR_SIZE, wall.heights[3], base_z);
                                draw_3d_line(fb, p0, p1, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p1, p2, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p2, p3, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p3, p0, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p0, p2, &state.camera_3d, hover_color);
                            }
                        }
                        SectorFace::WallSouth(i) => {
                            if let Some(wall) = sector.walls_south.get(i) {
                                let p0 = Vec3::new(base_x + SECTOR_SIZE, wall.heights[0], base_z + SECTOR_SIZE);
                                let p1 = Vec3::new(base_x, wall.heights[1], base_z + SECTOR_SIZE);
                                let p2 = Vec3::new(base_x, wall.heights[2], base_z + SECTOR_SIZE);
                                let p3 = Vec3::new(base_x + SECTOR_SIZE, wall.heights[3], base_z + SECTOR_SIZE);
                                draw_3d_line(fb, p0, p1, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p1, p2, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p2, p3, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p3, p0, &state.camera_3d, hover_color);
                                draw_3d_line(fb, p0, p2, &state.camera_3d, hover_color);
                            }
                        }
                        SectorFace::WallWest(i) => {
                            if let Some(wall) = sector.walls_west.get(i) {
                                let p0 = Vec3::new(base_x, wall.heights[0], base_z + SECTOR_SIZE);
                                let p1 = Vec3::new(base_x, wall.heights[1], base_z);
                                let p2 = Vec3::new(base_x, wall.heights[2], base_z);
                                let p3 = Vec3::new(base_x, wall.heights[3], base_z + SECTOR_SIZE);
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

                        match face {
                            SectorFace::Floor => {
                                if let Some(floor) = &sector.floor {
                                    let corners = [
                                        Vec3::new(base_x, floor.heights[0], base_z),
                                        Vec3::new(base_x + SECTOR_SIZE, floor.heights[1], base_z),
                                        Vec3::new(base_x + SECTOR_SIZE, floor.heights[2], base_z + SECTOR_SIZE),
                                        Vec3::new(base_x, floor.heights[3], base_z + SECTOR_SIZE),
                                    ];
                                    for i in 0..4 {
                                        draw_3d_line(fb, corners[i], corners[(i + 1) % 4], &state.camera_3d, select_color);
                                    }
                                    draw_3d_line(fb, corners[0], corners[2], &state.camera_3d, select_color);
                                }
                            }
                            SectorFace::Ceiling => {
                                if let Some(ceiling) = &sector.ceiling {
                                    let corners = [
                                        Vec3::new(base_x, ceiling.heights[0], base_z),
                                        Vec3::new(base_x + SECTOR_SIZE, ceiling.heights[1], base_z),
                                        Vec3::new(base_x + SECTOR_SIZE, ceiling.heights[2], base_z + SECTOR_SIZE),
                                        Vec3::new(base_x, ceiling.heights[3], base_z + SECTOR_SIZE),
                                    ];
                                    for i in 0..4 {
                                        draw_3d_line(fb, corners[i], corners[(i + 1) % 4], &state.camera_3d, select_color);
                                    }
                                    draw_3d_line(fb, corners[0], corners[2], &state.camera_3d, select_color);
                                }
                            }
                            SectorFace::WallNorth(i) => {
                                if let Some(wall) = sector.walls_north.get(*i) {
                                    let p0 = Vec3::new(base_x, wall.heights[0], base_z);
                                    let p1 = Vec3::new(base_x + SECTOR_SIZE, wall.heights[1], base_z);
                                    let p2 = Vec3::new(base_x + SECTOR_SIZE, wall.heights[2], base_z);
                                    let p3 = Vec3::new(base_x, wall.heights[3], base_z);
                                    draw_3d_line(fb, p0, p1, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p1, p2, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p2, p3, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p3, p0, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p0, p2, &state.camera_3d, select_color);
                                }
                            }
                            SectorFace::WallEast(i) => {
                                if let Some(wall) = sector.walls_east.get(*i) {
                                    let p0 = Vec3::new(base_x + SECTOR_SIZE, wall.heights[0], base_z);
                                    let p1 = Vec3::new(base_x + SECTOR_SIZE, wall.heights[1], base_z + SECTOR_SIZE);
                                    let p2 = Vec3::new(base_x + SECTOR_SIZE, wall.heights[2], base_z + SECTOR_SIZE);
                                    let p3 = Vec3::new(base_x + SECTOR_SIZE, wall.heights[3], base_z);
                                    draw_3d_line(fb, p0, p1, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p1, p2, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p2, p3, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p3, p0, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p0, p2, &state.camera_3d, select_color);
                                }
                            }
                            SectorFace::WallSouth(i) => {
                                if let Some(wall) = sector.walls_south.get(*i) {
                                    let p0 = Vec3::new(base_x + SECTOR_SIZE, wall.heights[0], base_z + SECTOR_SIZE);
                                    let p1 = Vec3::new(base_x, wall.heights[1], base_z + SECTOR_SIZE);
                                    let p2 = Vec3::new(base_x, wall.heights[2], base_z + SECTOR_SIZE);
                                    let p3 = Vec3::new(base_x + SECTOR_SIZE, wall.heights[3], base_z + SECTOR_SIZE);
                                    draw_3d_line(fb, p0, p1, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p1, p2, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p2, p3, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p3, p0, &state.camera_3d, select_color);
                                    draw_3d_line(fb, p0, p2, &state.camera_3d, select_color);
                                }
                            }
                            SectorFace::WallWest(i) => {
                                if let Some(wall) = sector.walls_west.get(*i) {
                                    let p0 = Vec3::new(base_x, wall.heights[0], base_z + SECTOR_SIZE);
                                    let p1 = Vec3::new(base_x, wall.heights[1], base_z);
                                    let p2 = Vec3::new(base_x, wall.heights[2], base_z);
                                    let p3 = Vec3::new(base_x, wall.heights[3], base_z + SECTOR_SIZE);
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

                        // Draw floor outline if floor exists
                        if let Some(floor) = &sector.floor {
                            let corners = [
                                Vec3::new(base_x, floor.heights[0], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, floor.heights[1], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, floor.heights[2], base_z + SECTOR_SIZE),
                                Vec3::new(base_x, floor.heights[3], base_z + SECTOR_SIZE),
                            ];
                            for i in 0..4 {
                                draw_3d_line(fb, corners[i], corners[(i + 1) % 4], &state.camera_3d, select_color);
                            }
                        }

                        // Draw ceiling outline if ceiling exists
                        if let Some(ceiling) = &sector.ceiling {
                            let corners = [
                                Vec3::new(base_x, ceiling.heights[0], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, ceiling.heights[1], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, ceiling.heights[2], base_z + SECTOR_SIZE),
                                Vec3::new(base_x, ceiling.heights[3], base_z + SECTOR_SIZE),
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
                                    Vec3::new(cx, fy, cz),
                                    Vec3::new(cx, cy, cz),
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
                                let p0 = Vec3::new(x0, wall.heights[0], z0);
                                let p1 = Vec3::new(x1, wall.heights[1], z1);
                                let p2 = Vec3::new(x1, wall.heights[2], z1);
                                let p3 = Vec3::new(x0, wall.heights[3], z0);
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

                        let corners: Option<[Vec3; 4]> = if *face_idx == 0 {
                            sector.floor.as_ref().map(|f| [
                                Vec3::new(base_x, f.heights[0], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, f.heights[1], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, f.heights[2], base_z + SECTOR_SIZE),
                                Vec3::new(base_x, f.heights[3], base_z + SECTOR_SIZE),
                            ])
                        } else if *face_idx == 1 {
                            sector.ceiling.as_ref().map(|c| [
                                Vec3::new(base_x, c.heights[0], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, c.heights[1], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, c.heights[2], base_z + SECTOR_SIZE),
                                Vec3::new(base_x, c.heights[3], base_z + SECTOR_SIZE),
                            ])
                        } else if *face_idx == 2 {
                            // Wall edge
                            if let Some(wf) = wall_face {
                                let (x0, z0, x1, z1) = match wf {
                                    SectorFace::WallNorth(_) => (base_x, base_z, base_x + SECTOR_SIZE, base_z),
                                    SectorFace::WallEast(_) => (base_x + SECTOR_SIZE, base_z, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE),
                                    SectorFace::WallSouth(_) => (base_x + SECTOR_SIZE, base_z + SECTOR_SIZE, base_x, base_z + SECTOR_SIZE),
                                    SectorFace::WallWest(_) => (base_x, base_z + SECTOR_SIZE, base_x, base_z),
                                    _ => (0.0, 0.0, 0.0, 0.0),
                                };
                                let wall_heights = match wf {
                                    SectorFace::WallNorth(i) => sector.walls_north.get(*i).map(|w| w.heights),
                                    SectorFace::WallEast(i) => sector.walls_east.get(*i).map(|w| w.heights),
                                    SectorFace::WallSouth(i) => sector.walls_south.get(*i).map(|w| w.heights),
                                    SectorFace::WallWest(i) => sector.walls_west.get(*i).map(|w| w.heights),
                                    _ => None,
                                };
                                wall_heights.map(|h| [
                                    Vec3::new(x0, h[0], z0),
                                    Vec3::new(x1, h[1], z1),
                                    Vec3::new(x1, h[2], z1),
                                    Vec3::new(x0, h[3], z0),
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

        let floor_y = 0.0;
        let ceiling_y = CEILING_HEIGHT;

        let corners = [
            Vec3::new(snapped_x, target_y, snapped_z),
            Vec3::new(snapped_x, target_y, snapped_z + SECTOR_SIZE),
            Vec3::new(snapped_x + SECTOR_SIZE, target_y, snapped_z + SECTOR_SIZE),
            Vec3::new(snapped_x + SECTOR_SIZE, target_y, snapped_z),
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
) -> bool {
    let shift_held = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);

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

            // Keyboard camera movement (WASD + Q/E) - only when viewport focused and not dragging
            // Hold Shift for faster movement
            let base_speed = 100.0; // Scaled for TRLE units (1024 per sector)
            let move_speed = if shift_held { base_speed * 4.0 } else { base_speed };
            if (inside_viewport || state.viewport_mouse_captured) && state.dragging_sector_vertices.is_empty() {
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

            // Mouse wheel: zoom in/out (change orbit distance)
            if inside_viewport {
                let scroll = mouse_wheel().1;
                if scroll != 0.0 {
                    let zoom_factor = if scroll > 0.0 { 0.9 } else { 1.1 };
                    state.orbit_distance = (state.orbit_distance * zoom_factor).clamp(100.0, 20000.0);
                    state.sync_camera_from_orbit();
                }
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
    /// Hovered light: (room_idx, light_idx, screen_dist)
    pub light: Option<(usize, usize, f32)>,
}

impl Default for HoverResult {
    fn default() -> Self {
        Self {
            vertex: None,
            edge: None,
            face: None,
            light: None,
        }
    }
}

/// Collected vertex data for hover detection and rendering
/// (world_pos, room_idx, gx, gz, corner_idx, face_type)
pub type VertexData = Vec<(Vec3, usize, usize, usize, usize, SectorFace)>;

/// Collect all vertex positions for the current room
fn collect_room_vertices(state: &EditorState) -> VertexData {
    let mut all_vertices = Vec::new();

    if let Some(room) = state.level.rooms.get(state.current_room) {
        for (gx, gz, sector) in room.iter_sectors() {
            let base_x = room.position.x + (gx as f32) * SECTOR_SIZE;
            let base_z = room.position.z + (gz as f32) * SECTOR_SIZE;

            // Floor vertices
            if let Some(floor) = &sector.floor {
                all_vertices.push((Vec3::new(base_x, floor.heights[0], base_z), state.current_room, gx, gz, 0, SectorFace::Floor));
                all_vertices.push((Vec3::new(base_x + SECTOR_SIZE, floor.heights[1], base_z), state.current_room, gx, gz, 1, SectorFace::Floor));
                all_vertices.push((Vec3::new(base_x + SECTOR_SIZE, floor.heights[2], base_z + SECTOR_SIZE), state.current_room, gx, gz, 2, SectorFace::Floor));
                all_vertices.push((Vec3::new(base_x, floor.heights[3], base_z + SECTOR_SIZE), state.current_room, gx, gz, 3, SectorFace::Floor));
            }

            // Ceiling vertices
            if let Some(ceiling) = &sector.ceiling {
                all_vertices.push((Vec3::new(base_x, ceiling.heights[0], base_z), state.current_room, gx, gz, 0, SectorFace::Ceiling));
                all_vertices.push((Vec3::new(base_x + SECTOR_SIZE, ceiling.heights[1], base_z), state.current_room, gx, gz, 1, SectorFace::Ceiling));
                all_vertices.push((Vec3::new(base_x + SECTOR_SIZE, ceiling.heights[2], base_z + SECTOR_SIZE), state.current_room, gx, gz, 2, SectorFace::Ceiling));
                all_vertices.push((Vec3::new(base_x, ceiling.heights[3], base_z + SECTOR_SIZE), state.current_room, gx, gz, 3, SectorFace::Ceiling));
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
                    all_vertices.push((Vec3::new(x0, wall.heights[0], z0), state.current_room, gx, gz, 0, make_face(i)));
                    all_vertices.push((Vec3::new(x1, wall.heights[1], z1), state.current_room, gx, gz, 1, make_face(i)));
                    all_vertices.push((Vec3::new(x1, wall.heights[2], z1), state.current_room, gx, gz, 2, make_face(i)));
                    all_vertices.push((Vec3::new(x0, wall.heights[3], z0), state.current_room, gx, gz, 3, make_face(i)));
                }
            }
        }
    }

    all_vertices
}

/// Find hovered elements in Select mode (vertex > edge > face priority)
fn find_hovered_elements(
    state: &EditorState,
    mouse_fb: (f32, f32),
    fb_width: usize,
    fb_height: usize,
    all_vertices: &VertexData,
) -> HoverResult {
    let mut result = HoverResult::default();
    let (mouse_fb_x, mouse_fb_y) = mouse_fb;

    const VERTEX_THRESHOLD: f32 = 10.0;
    const EDGE_THRESHOLD: f32 = 8.0;

    // Check vertices first (highest priority)
    for (world_pos, room_idx, gx, gz, corner_idx, face) in all_vertices {
        if let Some((sx, sy)) = world_to_screen(
            *world_pos,
            state.camera_3d.position,
            state.camera_3d.basis_x,
            state.camera_3d.basis_y,
            state.camera_3d.basis_z,
            fb_width,
            fb_height,
        ) {
            let dist = ((mouse_fb_x - sx).powi(2) + (mouse_fb_y - sy).powi(2)).sqrt();
            if dist < VERTEX_THRESHOLD {
                if result.vertex.map_or(true, |(_, _, _, _, _, best_dist)| dist < best_dist) {
                    result.vertex = Some((*room_idx, *gx, *gz, *corner_idx, *face, dist));
                }
            }
        }
    }

    // Check edges if no vertex hovered
    if result.vertex.is_none() {
        if let Some(room) = state.level.rooms.get(state.current_room) {
            for (gx, gz, sector) in room.iter_sectors() {
                let base_x = room.position.x + (gx as f32) * SECTOR_SIZE;
                let base_z = room.position.z + (gz as f32) * SECTOR_SIZE;

                // Check floor edges
                if let Some(floor) = &sector.floor {
                    let corners = [
                        Vec3::new(base_x, floor.heights[0], base_z),
                        Vec3::new(base_x + SECTOR_SIZE, floor.heights[1], base_z),
                        Vec3::new(base_x + SECTOR_SIZE, floor.heights[2], base_z + SECTOR_SIZE),
                        Vec3::new(base_x, floor.heights[3], base_z + SECTOR_SIZE),
                    ];
                    for edge_idx in 0..4 {
                        let v0 = corners[edge_idx];
                        let v1 = corners[(edge_idx + 1) % 4];
                        if let (Some((sx0, sy0)), Some((sx1, sy1))) = (
                            world_to_screen(v0, state.camera_3d.position, state.camera_3d.basis_x,
                                state.camera_3d.basis_y, state.camera_3d.basis_z, fb_width, fb_height),
                            world_to_screen(v1, state.camera_3d.position, state.camera_3d.basis_x,
                                state.camera_3d.basis_y, state.camera_3d.basis_z, fb_width, fb_height)
                        ) {
                            let dist = point_to_segment_distance(mouse_fb_x, mouse_fb_y, sx0, sy0, sx1, sy1);
                            if dist < EDGE_THRESHOLD {
                                if result.edge.map_or(true, |(_, _, _, _, _, _, best_dist)| dist < best_dist) {
                                    result.edge = Some((state.current_room, gx, gz, 0, edge_idx, None, dist));
                                }
                            }
                        }
                    }
                }

                // Check ceiling edges
                if let Some(ceiling) = &sector.ceiling {
                    let corners = [
                        Vec3::new(base_x, ceiling.heights[0], base_z),
                        Vec3::new(base_x + SECTOR_SIZE, ceiling.heights[1], base_z),
                        Vec3::new(base_x + SECTOR_SIZE, ceiling.heights[2], base_z + SECTOR_SIZE),
                        Vec3::new(base_x, ceiling.heights[3], base_z + SECTOR_SIZE),
                    ];
                    for edge_idx in 0..4 {
                        let v0 = corners[edge_idx];
                        let v1 = corners[(edge_idx + 1) % 4];
                        if let (Some((sx0, sy0)), Some((sx1, sy1))) = (
                            world_to_screen(v0, state.camera_3d.position, state.camera_3d.basis_x,
                                state.camera_3d.basis_y, state.camera_3d.basis_z, fb_width, fb_height),
                            world_to_screen(v1, state.camera_3d.position, state.camera_3d.basis_x,
                                state.camera_3d.basis_y, state.camera_3d.basis_z, fb_width, fb_height)
                        ) {
                            let dist = point_to_segment_distance(mouse_fb_x, mouse_fb_y, sx0, sy0, sx1, sy1);
                            if dist < EDGE_THRESHOLD {
                                if result.edge.map_or(true, |(_, _, _, _, _, _, best_dist)| dist < best_dist) {
                                    result.edge = Some((state.current_room, gx, gz, 1, edge_idx, None, dist));
                                }
                            }
                        }
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
                            Vec3::new(x0, wall.heights[0], z0),
                            Vec3::new(x1, wall.heights[1], z1),
                            Vec3::new(x1, wall.heights[2], z1),
                            Vec3::new(x0, wall.heights[3], z0),
                        ];
                        for edge_idx in 0..4 {
                            let v0 = wall_corners[edge_idx];
                            let v1 = wall_corners[(edge_idx + 1) % 4];
                            if let (Some((sx0, sy0)), Some((sx1, sy1))) = (
                                world_to_screen(v0, state.camera_3d.position, state.camera_3d.basis_x,
                                    state.camera_3d.basis_y, state.camera_3d.basis_z, fb_width, fb_height),
                                world_to_screen(v1, state.camera_3d.position, state.camera_3d.basis_x,
                                    state.camera_3d.basis_y, state.camera_3d.basis_z, fb_width, fb_height)
                            ) {
                                let dist = point_to_segment_distance(mouse_fb_x, mouse_fb_y, sx0, sy0, sx1, sy1);
                                if dist < EDGE_THRESHOLD {
                                    if result.edge.map_or(true, |(_, _, _, _, _, _, best_dist)| dist < best_dist) {
                                        let wall_face = make_face(i);
                                        result.edge = Some((state.current_room, gx, gz, 2, edge_idx, Some(wall_face), dist));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Check faces if no vertex or edge hovered
    if result.vertex.is_none() && result.edge.is_none() {
        if let Some(room) = state.level.rooms.get(state.current_room) {
            'face_loop: for (gx, gz, sector) in room.iter_sectors() {
                let base_x = room.position.x + (gx as f32) * SECTOR_SIZE;
                let base_z = room.position.z + (gz as f32) * SECTOR_SIZE;

                // Check floor
                if let Some(floor) = &sector.floor {
                    let corners = [
                        Vec3::new(base_x, floor.heights[0], base_z),
                        Vec3::new(base_x + SECTOR_SIZE, floor.heights[1], base_z),
                        Vec3::new(base_x + SECTOR_SIZE, floor.heights[2], base_z + SECTOR_SIZE),
                        Vec3::new(base_x, floor.heights[3], base_z + SECTOR_SIZE),
                    ];

                    if let (Some((sx0, sy0)), Some((sx1, sy1)), Some((sx2, sy2)), Some((sx3, sy3))) = (
                        world_to_screen(corners[0], state.camera_3d.position, state.camera_3d.basis_x,
                            state.camera_3d.basis_y, state.camera_3d.basis_z, fb_width, fb_height),
                        world_to_screen(corners[1], state.camera_3d.position, state.camera_3d.basis_x,
                            state.camera_3d.basis_y, state.camera_3d.basis_z, fb_width, fb_height),
                        world_to_screen(corners[2], state.camera_3d.position, state.camera_3d.basis_x,
                            state.camera_3d.basis_y, state.camera_3d.basis_z, fb_width, fb_height),
                        world_to_screen(corners[3], state.camera_3d.position, state.camera_3d.basis_x,
                            state.camera_3d.basis_y, state.camera_3d.basis_z, fb_width, fb_height),
                    ) {
                        if point_in_triangle_2d(mouse_fb_x, mouse_fb_y, sx0, sy0, sx1, sy1, sx2, sy2) ||
                           point_in_triangle_2d(mouse_fb_x, mouse_fb_y, sx0, sy0, sx2, sy2, sx3, sy3) {
                            result.face = Some((state.current_room, gx, gz, SectorFace::Floor));
                            break 'face_loop;
                        }
                    }
                }

                // Check ceiling
                if let Some(ceiling) = &sector.ceiling {
                    let corners = [
                        Vec3::new(base_x, ceiling.heights[0], base_z),
                        Vec3::new(base_x + SECTOR_SIZE, ceiling.heights[1], base_z),
                        Vec3::new(base_x + SECTOR_SIZE, ceiling.heights[2], base_z + SECTOR_SIZE),
                        Vec3::new(base_x, ceiling.heights[3], base_z + SECTOR_SIZE),
                    ];

                    if let (Some((sx0, sy0)), Some((sx1, sy1)), Some((sx2, sy2)), Some((sx3, sy3))) = (
                        world_to_screen(corners[0], state.camera_3d.position, state.camera_3d.basis_x,
                            state.camera_3d.basis_y, state.camera_3d.basis_z, fb_width, fb_height),
                        world_to_screen(corners[1], state.camera_3d.position, state.camera_3d.basis_x,
                            state.camera_3d.basis_y, state.camera_3d.basis_z, fb_width, fb_height),
                        world_to_screen(corners[2], state.camera_3d.position, state.camera_3d.basis_x,
                            state.camera_3d.basis_y, state.camera_3d.basis_z, fb_width, fb_height),
                        world_to_screen(corners[3], state.camera_3d.position, state.camera_3d.basis_x,
                            state.camera_3d.basis_y, state.camera_3d.basis_z, fb_width, fb_height),
                    ) {
                        if point_in_triangle_2d(mouse_fb_x, mouse_fb_y, sx0, sy0, sx1, sy1, sx2, sy2) ||
                           point_in_triangle_2d(mouse_fb_x, mouse_fb_y, sx0, sy0, sx2, sy2, sx3, sy3) {
                            result.face = Some((state.current_room, gx, gz, SectorFace::Ceiling));
                            break 'face_loop;
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
                            Vec3::new(x0, wall.heights[0], z0),
                            Vec3::new(x1, wall.heights[1], z1),
                            Vec3::new(x1, wall.heights[2], z1),
                            Vec3::new(x0, wall.heights[3], z0),
                        ];

                        if let (Some((sx0, sy0)), Some((sx1, sy1)), Some((sx2, sy2)), Some((sx3, sy3))) = (
                            world_to_screen(wall_corners[0], state.camera_3d.position, state.camera_3d.basis_x,
                                state.camera_3d.basis_y, state.camera_3d.basis_z, fb_width, fb_height),
                            world_to_screen(wall_corners[1], state.camera_3d.position, state.camera_3d.basis_x,
                                state.camera_3d.basis_y, state.camera_3d.basis_z, fb_width, fb_height),
                            world_to_screen(wall_corners[2], state.camera_3d.position, state.camera_3d.basis_x,
                                state.camera_3d.basis_y, state.camera_3d.basis_z, fb_width, fb_height),
                            world_to_screen(wall_corners[3], state.camera_3d.position, state.camera_3d.basis_x,
                                state.camera_3d.basis_y, state.camera_3d.basis_z, fb_width, fb_height),
                        ) {
                            if point_in_triangle_2d(mouse_fb_x, mouse_fb_y, sx0, sy0, sx1, sy1, sx2, sy2) ||
                               point_in_triangle_2d(mouse_fb_x, mouse_fb_y, sx0, sy0, sx2, sy2, sx3, sy3) {
                                result.face = Some((state.current_room, gx, gz, make_face(i)));
                                break 'face_loop;
                            }
                        }
                    }
                }
            }
        }
    }

    // Check lights (for all rooms, not just current)
    const LIGHT_THRESHOLD: f32 = 12.0;
    for (room_idx, room) in state.level.rooms.iter().enumerate() {
        for (light_idx, light) in room.lights.iter().enumerate() {
            let world_pos = Vec3::new(
                room.position.x + light.position.x,
                room.position.y + light.position.y,
                room.position.z + light.position.z,
            );
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
                if dist < LIGHT_THRESHOLD {
                    if result.light.map_or(true, |(_, _, best_dist)| dist < best_dist) {
                        result.light = Some((room_idx, light_idx, dist));
                    }
                }
            }
        }
    }

    result
}
