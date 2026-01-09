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
use crate::ui::{Rect, UiContext, drag_tracker::pick_plane};
use crate::rasterizer::{
    Framebuffer, Texture as RasterTexture, render_mesh, render_mesh_15, Color as RasterColor, Vec3,
    WIDTH, HEIGHT, WIDTH_HI, HEIGHT_HI,
    world_to_screen, world_to_screen_with_depth,
    point_to_segment_distance, point_in_triangle_2d,
    Light, RasterSettings,
};
use crate::world::{SECTOR_SIZE, SplitDirection};
use crate::input::{InputState, Action};
use super::{EditorState, EditorTool, Selection, SectorFace, CameraMode, CEILING_HEIGHT, CopiedFaceData};

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

/// Get the wall index from a wall face (vertical layer index)
fn get_wall_index(face: &SectorFace) -> Option<usize> {
    match face {
        SectorFace::WallNorth(i) | SectorFace::WallEast(i) |
        SectorFace::WallSouth(i) | SectorFace::WallWest(i) |
        SectorFace::WallNwSe(i) | SectorFace::WallNeSw(i) => Some(*i),
        _ => None,
    }
}

/// Get the wall direction type (0-5) for matching walls of the same orientation
fn get_wall_direction_type(face: &SectorFace) -> Option<usize> {
    match face {
        SectorFace::WallNorth(_) => Some(0),
        SectorFace::WallEast(_) => Some(1),
        SectorFace::WallSouth(_) => Some(2),
        SectorFace::WallWest(_) => Some(3),
        SectorFace::WallNwSe(_) => Some(4),
        SectorFace::WallNeSw(_) => Some(5),
        _ => None,
    }
}

/// Create a wall face with the given direction type and index
fn make_wall_face(direction_type: usize, index: usize) -> SectorFace {
    match direction_type {
        0 => SectorFace::WallNorth(index),
        1 => SectorFace::WallEast(index),
        2 => SectorFace::WallSouth(index),
        3 => SectorFace::WallWest(index),
        4 => SectorFace::WallNwSe(index),
        5 => SectorFace::WallNeSw(index),
        _ => SectorFace::WallNorth(index), // Fallback
    }
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
/// Now layer-aware: if start and end walls have different indices (vertical layers),
/// returns all walls in the index range [min_idx, max_idx] for each XZ position along the path.
fn find_wall_path(
    room: &crate::world::Room,
    start_x: usize, start_z: usize, start_face: &SectorFace,
    end_x: usize, end_z: usize, end_face: &SectorFace
) -> Option<Vec<(usize, usize, SectorFace)>> {
    use std::collections::{VecDeque, HashSet, HashMap};

    // Get start/end wall indices (vertical layer)
    let start_wall_idx = get_wall_index(start_face).unwrap_or(0);
    let end_wall_idx = get_wall_index(end_face).unwrap_or(0);
    let min_layer = start_wall_idx.min(end_wall_idx);
    let max_layer = start_wall_idx.max(end_wall_idx);

    // Get all walls in the room as (x, z, face, endpoints)
    // Use index 0 for path finding (we'll expand to all layers at the end)
    let mut all_walls: Vec<(usize, usize, SectorFace, ((i32, i32), (i32, i32)))> = Vec::new();
    // Also track how many walls exist at each (x, z, direction_type)
    let mut wall_counts: HashMap<(usize, usize, usize), usize> = HashMap::new();

    for gz in 0..room.depth {
        for gx in 0..room.width {
            if let Some(sector) = room.get_sector(gx, gz) {
                // Cardinal walls - only add index 0 for BFS, track count
                if !sector.walls_north.is_empty() {
                    let face = SectorFace::WallNorth(0);
                    all_walls.push((gx, gz, face, get_wall_endpoints(gx, gz, &face)));
                    wall_counts.insert((gx, gz, 0), sector.walls_north.len());
                }
                if !sector.walls_east.is_empty() {
                    let face = SectorFace::WallEast(0);
                    all_walls.push((gx, gz, face, get_wall_endpoints(gx, gz, &face)));
                    wall_counts.insert((gx, gz, 1), sector.walls_east.len());
                }
                if !sector.walls_south.is_empty() {
                    let face = SectorFace::WallSouth(0);
                    all_walls.push((gx, gz, face, get_wall_endpoints(gx, gz, &face)));
                    wall_counts.insert((gx, gz, 2), sector.walls_south.len());
                }
                if !sector.walls_west.is_empty() {
                    let face = SectorFace::WallWest(0);
                    all_walls.push((gx, gz, face, get_wall_endpoints(gx, gz, &face)));
                    wall_counts.insert((gx, gz, 3), sector.walls_west.len());
                }
                // Diagonal walls
                if !sector.walls_nwse.is_empty() {
                    let face = SectorFace::WallNwSe(0);
                    all_walls.push((gx, gz, face, get_wall_endpoints(gx, gz, &face)));
                    wall_counts.insert((gx, gz, 4), sector.walls_nwse.len());
                }
                if !sector.walls_nesw.is_empty() {
                    let face = SectorFace::WallNeSw(0);
                    all_walls.push((gx, gz, face, get_wall_endpoints(gx, gz, &face)));
                    wall_counts.insert((gx, gz, 5), sector.walls_nesw.len());
                }
            }
        }
    }

    // Get wall direction types for BFS lookup
    let start_dir_type = get_wall_direction_type(start_face)?;
    let end_dir_type = get_wall_direction_type(end_face)?;

    // Find indices of start and end walls (using index-0 faces)
    let start_idx = all_walls.iter().position(|(x, z, f, _)| {
        *x == start_x && *z == start_z && get_wall_direction_type(f) == Some(start_dir_type)
    })?;
    let end_idx = all_walls.iter().position(|(x, z, f, _)| {
        *x == end_x && *z == end_z && get_wall_direction_type(f) == Some(end_dir_type)
    })?;

    // Check if two walls share an endpoint (are connected)
    let walls_connected = |a: &((i32, i32), (i32, i32)), b: &((i32, i32), (i32, i32))| -> bool {
        a.0 == b.0 || a.0 == b.1 || a.1 == b.0 || a.1 == b.1
    };

    // BFS to find shortest path (by XZ connectivity)
    let mut visited: HashSet<usize> = HashSet::new();
    let mut parent: HashMap<usize, usize> = HashMap::new();
    let mut queue: VecDeque<usize> = VecDeque::new();

    visited.insert(start_idx);
    queue.push_back(start_idx);

    let mut path_indices: Option<Vec<usize>> = None;

    if start_idx == end_idx {
        // Same XZ position and direction
        path_indices = Some(vec![start_idx]);
    } else {
        while let Some(current) = queue.pop_front() {
            if current == end_idx {
                // Found path, reconstruct it
                let mut indices = Vec::new();
                let mut node = end_idx;
                while node != start_idx {
                    indices.push(node);
                    node = *parent.get(&node).unwrap();
                }
                indices.push(start_idx);
                indices.reverse();
                path_indices = Some(indices);
                break;
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
    }

    // Expand path to include all wall layers in range [min_layer, max_layer]
    let path_indices = path_indices?;
    let mut result: Vec<(usize, usize, SectorFace)> = Vec::new();

    for idx in path_indices {
        let (x, z, face, _) = &all_walls[idx];
        let dir_type = get_wall_direction_type(face).unwrap_or(0);
        let count = wall_counts.get(&(*x, *z, dir_type)).copied().unwrap_or(1);

        // Add all walls in the layer range that exist at this position
        for layer in min_layer..=max_layer {
            if layer < count {
                result.push((*x, *z, make_wall_face(dir_type, layer)));
            }
        }
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
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
    use super::state::EditorFrameTimings;
    let _vp_start = EditorFrameTimings::start();

    // === INPUT PHASE ===
    let input_start = EditorFrameTimings::start();

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

    // Rotate wall direction with R key (in DrawWall mode)
    // Cycles through all 6 directions: N -> E -> S -> W -> NW-SE -> NE-SW -> N
    if inside_viewport && is_key_pressed(KeyCode::R) && state.tool == EditorTool::DrawWall {
        state.wall_direction = state.wall_direction.rotate_cw();
        state.set_status(&format!("Wall direction: {}", state.wall_direction.name()), 1.0);
    }

    // Toggle high/low gap preference with F key (in DrawWall mode)
    if inside_viewport && is_key_pressed(KeyCode::F) && state.tool == EditorTool::DrawWall {
        state.wall_prefer_high = !state.wall_prefer_high;
        let gap = if state.wall_prefer_high { "High" } else { "Low" };
        state.set_status(&format!("Gap preference: {}", gap), 1.0);
    }

    // Tool shortcuts: 1=Select, 2=Floor, 3=Wall, 4=Ceiling, 5=Object
    if inside_viewport {
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

    // H/V flip for geometry clipboard
    if inside_viewport && is_key_pressed(KeyCode::H) {
        if let Some(gc) = &mut state.geometry_clipboard {
            gc.flip_h = !gc.flip_h;
            let status = if gc.flip_h { "Geometry: flipped horizontally" } else { "Geometry: flip H off" };
            state.set_status(status, 1.0);
        }
    }
    if inside_viewport && is_key_pressed(KeyCode::V) {
        if let Some(gc) = &mut state.geometry_clipboard {
            gc.flip_v = !gc.flip_v;
            let status = if gc.flip_v { "Geometry: flipped vertically" } else { "Geometry: flip V off" };
            state.set_status(status, 1.0);
        }
    }
    if inside_viewport && is_key_pressed(KeyCode::R) {
        if let Some(gc) = &mut state.geometry_clipboard {
            gc.rotation = (gc.rotation + 1) % 4;
            let degrees = gc.rotation as u16 * 90;
            state.set_status(&format!("Geometry: rotated {}°", degrees), 1.0);
        }
    }

    // Clear selection and geometry clipboard with Escape key
    if inside_viewport && is_key_pressed(KeyCode::Escape) && (state.selection != Selection::None || !state.multi_selection.is_empty() || state.geometry_clipboard.is_some()) {
        state.save_selection_undo();
        state.set_selection(Selection::None);
        state.clear_multi_selection();
        if state.geometry_clipboard.is_some() {
            state.geometry_clipboard = None;
            state.set_status("Paste cancelled", 0.5);
        } else {
            state.set_status("Selection cleared", 0.5);
        }
    }

    // Select all faces in room with Ctrl-A
    let ctrl_held = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl)
        || is_key_down(KeyCode::LeftSuper) || is_key_down(KeyCode::RightSuper);
    if inside_viewport && ctrl_held && is_key_pressed(KeyCode::A) {
        // Find which room to select from: current selection's room, or first visible room
        let room_idx = match &state.selection {
            Selection::SectorFace { room, .. } => Some(*room),
            Selection::Vertex { room, .. } => Some(*room),
            Selection::Object { room, .. } => Some(*room),
            Selection::Room(room) => Some(*room),
            Selection::Sector { room, .. } => Some(*room),
            Selection::Edge { room, .. } => Some(*room),
            Selection::Portal { room, .. } => Some(*room),
            Selection::None => {
                // Find first visible room
                (0..state.level.rooms.len()).find(|i| !state.hidden_rooms.contains(i))
            }
        };

        if let Some(room_idx) = room_idx {
            // Collect all faces first (immutable borrow of room)
            let faces: Vec<Selection> = if let Some(room) = state.level.rooms.get(room_idx) {
                let mut faces = Vec::new();
                for gz in 0..room.depth {
                    for gx in 0..room.width {
                        if let Some(sector) = room.get_sector(gx, gz) {
                            if sector.floor.is_some() {
                                faces.push(Selection::SectorFace { room: room_idx, x: gx, z: gz, face: SectorFace::Floor });
                            }
                            if sector.ceiling.is_some() {
                                faces.push(Selection::SectorFace { room: room_idx, x: gx, z: gz, face: SectorFace::Ceiling });
                            }
                            for i in 0..sector.walls_north.len() {
                                faces.push(Selection::SectorFace { room: room_idx, x: gx, z: gz, face: SectorFace::WallNorth(i) });
                            }
                            for i in 0..sector.walls_south.len() {
                                faces.push(Selection::SectorFace { room: room_idx, x: gx, z: gz, face: SectorFace::WallSouth(i) });
                            }
                            for i in 0..sector.walls_east.len() {
                                faces.push(Selection::SectorFace { room: room_idx, x: gx, z: gz, face: SectorFace::WallEast(i) });
                            }
                            for i in 0..sector.walls_west.len() {
                                faces.push(Selection::SectorFace { room: room_idx, x: gx, z: gz, face: SectorFace::WallWest(i) });
                            }
                        }
                    }
                }
                faces
            } else {
                Vec::new()
            };

            // Now mutate state
            if !faces.is_empty() {
                state.save_selection_undo();
                state.clear_multi_selection();

                let mut iter = faces.into_iter();
                if let Some(first) = iter.next() {
                    state.set_selection(first);
                    state.multi_selection.extend(iter);
                }

                let count = 1 + state.multi_selection.len();
                state.set_status(&format!("Selected {} faces", count), 1.0);
            }
        }
    }

    // Center camera on selection with Period key
    if inside_viewport && is_key_pressed(KeyCode::Period) {
        state.center_camera_on_selection();
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
                        room.compact();
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

    // Geometry paste preview: detect sector position when we have clipboard
    // This enables showing a wireframe preview of where geometry will be pasted
    // Uses signed coordinates to allow pasting outside current room bounds (room will expand)
    // Shows preview anywhere - not just on empty space (user can paste to overwrite existing geometry)
    let geometry_paste_preview: Option<(i32, i32)> = if inside_viewport
        && state.tool == EditorTool::Select
        && state.geometry_clipboard.is_some()
    {
        // Find sector under cursor using ray-plane intersection (no distance limit)
        if let Some((mouse_fb_x, mouse_fb_y)) = screen_to_fb(mouse_pos.0, mouse_pos.1) {
            let detection_y = 0.0;
            let room_y = state.level.rooms.get(state.current_room)
                .map(|r| r.position.y)
                .unwrap_or(0.0);

            if let Some(world_pos) = pick_plane(
                Vec3::new(0.0, room_y + detection_y, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::ZERO,
                (mouse_fb_x, mouse_fb_y),
                &state.camera_3d,
                fb_width, fb_height,
            ) {
                let snapped_x = (world_pos.x / SECTOR_SIZE).floor() * SECTOR_SIZE;
                let snapped_z = (world_pos.z / SECTOR_SIZE).floor() * SECTOR_SIZE;

                // Convert world coords to grid coords relative to room position
                // Allow negative values and values beyond room bounds (room will expand on paste)
                if let Some(room) = state.level.rooms.get(state.current_room) {
                    let gx = ((snapped_x - room.position.x) / SECTOR_SIZE).floor() as i32;
                    let gz = ((snapped_z - room.position.z) / SECTOR_SIZE).floor() as i32;
                    Some((gx, gz))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    // In drawing modes, find preview sector position
    if inside_viewport && (state.tool == EditorTool::DrawFloor || state.tool == EditorTool::DrawCeiling) {
        if let Some((mouse_fb_x, mouse_fb_y)) = screen_to_fb(mouse_pos.0, mouse_pos.1) {
            use super::{CEILING_HEIGHT, CLICK_HEIGHT};

            let is_floor = state.tool == EditorTool::DrawFloor;

            // For sector detection, always use floor level (0.0) so clicking on the floor
            // selects the sector where you want to place geometry.
            // This is more intuitive - you click on the floor to place a ceiling above it.
            let detection_y = 0.0;

            // Find sector position using ray-plane intersection (no distance limit)
            let (snapped_x, snapped_z) = if let Some((locked_x, locked_z)) = state.height_adjust_locked_pos {
                // Use locked position when in height adjust mode
                (locked_x, locked_z)
            } else {
                // Get room Y offset for the detection plane
                let room_y = state.level.rooms.get(state.current_room)
                    .map(|r| r.position.y)
                    .unwrap_or(0.0);

                // Use ray-plane intersection to find where mouse points on floor plane
                if let Some(world_pos) = pick_plane(
                    Vec3::new(0.0, room_y + detection_y, 0.0),
                    Vec3::new(0.0, 1.0, 0.0),  // Y-up normal
                    Vec3::ZERO,
                    (mouse_fb_x, mouse_fb_y),
                    &state.camera_3d,
                    fb.width, fb.height,
                ) {
                    // Snap to grid
                    let grid_x = (world_pos.x / SECTOR_SIZE).floor() * SECTOR_SIZE;
                    let grid_z = (world_pos.z / SECTOR_SIZE).floor() * SECTOR_SIZE;
                    (grid_x, grid_z)
                } else {
                    // Ray doesn't hit plane (looking away from floor)
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

    // In DrawWall mode with cardinal direction, find preview wall at sector under cursor
    // Direction is controlled by state.wall_direction (rotated with R key)
    // For diagonal directions, a separate block handles the preview (see below)
    if inside_viewport && state.tool == EditorTool::DrawWall && !state.wall_direction.is_diagonal() {
        if let Some((mouse_fb_x, mouse_fb_y)) = screen_to_fb(mouse_pos.0, mouse_pos.1) {
            use super::CEILING_HEIGHT;

            // Use room's effective vertical bounds for gap detection
            let (default_y_bottom, default_y_top) = state.level.rooms.get(state.current_room)
                .map(|r| r.effective_height_bounds())
                .unwrap_or((0.0, CEILING_HEIGHT));
            let dir = state.wall_direction;

            // Get room Y offset for the detection plane
            let room_y_offset = state.level.rooms.get(state.current_room)
                .map(|r| r.position.y)
                .unwrap_or(0.0);

            // Use ray-plane intersection to find sector (no distance limit)
            let mid_y = (default_y_bottom + default_y_top) / 2.0;
            let sector_pos = pick_plane(
                Vec3::new(0.0, room_y_offset + mid_y, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::ZERO,
                (mouse_fb_x, mouse_fb_y),
                &state.camera_3d,
                fb.width, fb.height,
            );

            if let Some(world_pos) = sector_pos {
                let grid_x = (world_pos.x / SECTOR_SIZE).floor() * SECTOR_SIZE;
                let grid_z = (world_pos.z / SECTOR_SIZE).floor() * SECTOR_SIZE;

                // Estimate mouse world Y for gap selection
                // Note: diagonal directions filtered out in if condition above
                let edge_x = match dir {
                    crate::world::Direction::North | crate::world::Direction::South => grid_x + SECTOR_SIZE / 2.0,
                    crate::world::Direction::East => grid_x + SECTOR_SIZE,
                    crate::world::Direction::West => grid_x,
                    crate::world::Direction::NwSe | crate::world::Direction::NeSw => unreachable!(),
                };
                let edge_z = match dir {
                    crate::world::Direction::North => grid_z,
                    crate::world::Direction::South => grid_z + SECTOR_SIZE,
                    crate::world::Direction::East | crate::world::Direction::West => grid_z + SECTOR_SIZE / 2.0,
                    crate::world::Direction::NwSe | crate::world::Direction::NeSw => unreachable!(),
                };

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

                // Calculate where the new wall should be placed
                // Use prefer_high setting to select gap (high Y = ceiling, low Y = floor)
                let gap_select_y = if state.wall_prefer_high {
                    Some(default_y_top - 1.0)  // Near ceiling
                } else {
                    Some(default_y_bottom + 1.0)  // Near floor
                };
                let wall_info = if let Some(room) = state.level.rooms.get(state.current_room) {
                    if let Some((gx, gz)) = room.world_to_grid(grid_x + SECTOR_SIZE * 0.5, grid_z + SECTOR_SIZE * 0.5) {
                        if let Some(sector) = room.get_sector(gx, gz) {
                            // Debug logging for wall gap detection (only when sector changes)
                            {
                                use std::sync::atomic::{AtomicI32, Ordering};
                                static LAST_GX: AtomicI32 = AtomicI32::new(-999);
                                static LAST_GZ: AtomicI32 = AtomicI32::new(-999);
                                let gx_i = gx as i32;
                                let gz_i = gz as i32;
                                if LAST_GX.load(Ordering::Relaxed) != gx_i || LAST_GZ.load(Ordering::Relaxed) != gz_i {
                                    LAST_GX.store(gx_i, Ordering::Relaxed);
                                    LAST_GZ.store(gz_i, Ordering::Relaxed);
                                    let floor_info = sector.floor.as_ref().map(|f| {
                                        let (l, r) = f.edge_heights(dir);
                                        format!("F[{:.0},{:.0}]", l, r)
                                    }).unwrap_or_else(|| "F[-]".to_string());
                                    let ceiling_info = sector.ceiling.as_ref().map(|c| {
                                        let (l, r) = c.edge_heights(dir);
                                        format!("C[{:.0},{:.0}]", l, r)
                                    }).unwrap_or_else(|| "C[-]".to_string());
                                    let walls = sector.walls(dir);
                                    let walls_info = if walls.is_empty() {
                                        "W[]".to_string()
                                    } else {
                                        let w_strs: Vec<String> = walls.iter().map(|w| {
                                            format!("[{:.0},{:.0},{:.0},{:.0}]", w.heights[0], w.heights[1], w.heights[2], w.heights[3])
                                        }).collect();
                                        format!("W{}", w_strs.join(","))
                                    };
                                }
                            }
                            let has_existing = !sector.walls(dir).is_empty();
                            match sector.next_wall_position(dir, default_y_bottom, default_y_top, gap_select_y) {
                                Some(corner_heights) => {
                                    let wall_state = if has_existing { 1u8 } else { 0u8 };
                                    Some((corner_heights, wall_state))
                                }
                                None => Some(([0.0, 0.0, 0.0, 0.0], 2u8)),
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
                }
            }
        }
    }

    // In DrawWall mode with diagonal direction, find preview diagonal edge
    if inside_viewport && state.tool == EditorTool::DrawWall && state.wall_direction.is_diagonal() {
        if let Some((mouse_fb_x, mouse_fb_y)) = screen_to_fb(mouse_pos.0, mouse_pos.1) {
            use super::CEILING_HEIGHT;

            // Use room's effective vertical bounds for gap detection
            let (default_y_bottom, default_y_top) = state.level.rooms.get(state.current_room)
                .map(|r| r.effective_height_bounds())
                .unwrap_or((0.0, CEILING_HEIGHT));
            let mid_y = (default_y_bottom + default_y_top) / 2.0;

            // Get room Y offset for the detection plane
            let room_y_offset = state.level.rooms.get(state.current_room)
                .map(|r| r.position.y)
                .unwrap_or(0.0);

            // Use ray-plane intersection to find sector (no distance limit)
            let sector_pos = pick_plane(
                Vec3::new(0.0, room_y_offset + mid_y, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::ZERO,
                (mouse_fb_x, mouse_fb_y),
                &state.camera_3d,
                fb.width, fb.height,
            );

            if let Some(world_pos) = sector_pos {
                let grid_x = (world_pos.x / SECTOR_SIZE).floor() * SECTOR_SIZE;
                let grid_z = (world_pos.z / SECTOR_SIZE).floor() * SECTOR_SIZE;
                let center_x = grid_x + SECTOR_SIZE / 2.0;
                let center_z = grid_z + SECTOR_SIZE / 2.0;

                // Use wall_direction to determine diagonal type (NwSe or NeSw)
                let is_nwse = state.wall_direction == crate::world::Direction::NwSe;

                // Use prefer_high setting to select gap (same as axis-aligned walls)
                let gap_select_y = if state.wall_prefer_high {
                    Some(default_y_top - 1.0)  // Near ceiling
                } else {
                    Some(default_y_bottom + 1.0)  // Near floor
                };

                // Use gap detection for diagonal walls
                let wall_info = if let Some(room) = state.level.rooms.get(state.current_room) {
                    if let Some((gx, gz)) = room.world_to_grid(center_x, center_z) {
                        if let Some(sector) = room.get_sector(gx, gz) {
                            // Debug logging for diagonal wall gap detection (only when sector changes)
                            {
                                use std::sync::atomic::{AtomicI32, Ordering};
                                static LAST_DIAG_GX: AtomicI32 = AtomicI32::new(-999);
                                static LAST_DIAG_GZ: AtomicI32 = AtomicI32::new(-999);
                                let gx_i = gx as i32;
                                let gz_i = gz as i32;
                                if LAST_DIAG_GX.load(Ordering::Relaxed) != gx_i || LAST_DIAG_GZ.load(Ordering::Relaxed) != gz_i {
                                    LAST_DIAG_GX.store(gx_i, Ordering::Relaxed);
                                    LAST_DIAG_GZ.store(gz_i, Ordering::Relaxed);
                                    let dir = if is_nwse { crate::world::Direction::NwSe } else { crate::world::Direction::NeSw };
                                    let floor_info = sector.floor.as_ref().map(|f| {
                                        let (l, r) = f.edge_heights(dir);
                                        format!("F[{:.0},{:.0}]", l, r)
                                    }).unwrap_or_else(|| "F[-]".to_string());
                                    let ceiling_info = sector.ceiling.as_ref().map(|c| {
                                        let (l, r) = c.edge_heights(dir);
                                        format!("C[{:.0},{:.0}]", l, r)
                                    }).unwrap_or_else(|| "C[-]".to_string());
                                    let walls = if is_nwse { &sector.walls_nwse } else { &sector.walls_nesw };
                                    let walls_info = if walls.is_empty() {
                                        "W[]".to_string()
                                    } else {
                                        let w_strs: Vec<String> = walls.iter().map(|w| {
                                            format!("[{:.0},{:.0},{:.0},{:.0}]", w.heights[0], w.heights[1], w.heights[2], w.heights[3])
                                        }).collect();
                                        format!("W{}", w_strs.join(","))
                                    };
                                }
                            }
                            let walls = if is_nwse { &sector.walls_nwse } else { &sector.walls_nesw };
                            let has_existing = !walls.is_empty();
                            match sector.next_diagonal_wall_position(is_nwse, default_y_bottom, default_y_top, gap_select_y) {
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
                        // Range selection: if current selection is a vertex on same face type,
                        // select all vertices along the line between them
                        let mut did_range_select = false;
                        if let Selection::Vertex { room: sel_room, x: sel_x, z: sel_z, face: sel_face, corner_idx: sel_corner } = &state.selection {
                            // Only range select on same room, same face type (floor/ceiling)
                            let same_face_type = matches!((&face, sel_face),
                                (SectorFace::Floor, SectorFace::Floor) | (SectorFace::Ceiling, SectorFace::Ceiling));
                            if *sel_room == room_idx && same_face_type {
                                // Determine the line direction based on which coordinates differ
                                // and which corner indices are involved
                                let x_min = (*sel_x).min(gx);
                                let x_max = (*sel_x).max(gx);
                                let z_min = (*sel_z).min(gz);
                                let z_max = (*sel_z).max(gz);

                                // Check if selecting along an edge line (same z for N/S edges, same x for E/W edges)
                                // For floor: corner 0=NW, 1=NE, 2=SE, 3=SW
                                // North edge vertices: 0 and 1 (both at z_min of their sectors)
                                // South edge vertices: 2 and 3 (both at z_max of their sectors)
                                // East edge vertices: 1 and 2 (both at x_max of their sectors)
                                // West edge vertices: 0 and 3 (both at x_min of their sectors)

                                let is_north_edge = (*sel_corner == 0 || *sel_corner == 1) && (corner_idx == 0 || corner_idx == 1);
                                let is_south_edge = (*sel_corner == 2 || *sel_corner == 3) && (corner_idx == 2 || corner_idx == 3);
                                let is_east_edge = (*sel_corner == 1 || *sel_corner == 2) && (corner_idx == 1 || corner_idx == 2);
                                let is_west_edge = (*sel_corner == 0 || *sel_corner == 3) && (corner_idx == 0 || corner_idx == 3);

                                if is_north_edge || is_south_edge {
                                    // First, add the original selection to multi-selection so it's not lost
                                    if !state.multi_selection.contains(&state.selection) {
                                        state.multi_selection.push(state.selection.clone());
                                    }
                                    // Select along X axis (north or south edge)
                                    let z_coord = *sel_z; // Use the original selection's Z
                                    for x in x_min..=x_max {
                                        // Add both corner vertices that form this edge
                                        for &ci in if is_north_edge { &[0usize, 1] } else { &[2usize, 3] } {
                                            let vert_sel = Selection::Vertex {
                                                room: room_idx, x, z: z_coord,
                                                face: face.clone(), corner_idx: ci,
                                            };
                                            if !state.multi_selection.contains(&vert_sel) {
                                                state.multi_selection.push(vert_sel);
                                            }
                                        }
                                    }
                                    did_range_select = true;
                                } else if is_east_edge || is_west_edge {
                                    // First, add the original selection to multi-selection so it's not lost
                                    if !state.multi_selection.contains(&state.selection) {
                                        state.multi_selection.push(state.selection.clone());
                                    }
                                    // Select along Z axis (east or west edge)
                                    let x_coord = *sel_x; // Use the original selection's X
                                    for z in z_min..=z_max {
                                        // Add both corner vertices that form this edge
                                        for &ci in if is_east_edge { &[1usize, 2] } else { &[0usize, 3] } {
                                            let vert_sel = Selection::Vertex {
                                                room: room_idx, x: x_coord, z,
                                                face: face.clone(), corner_idx: ci,
                                            };
                                            if !state.multi_selection.contains(&vert_sel) {
                                                state.multi_selection.push(vert_sel);
                                            }
                                        }
                                    }
                                    did_range_select = true;
                                }
                            }
                        }
                        if !did_range_select {
                            state.toggle_multi_selection(new_selection.clone());
                        }
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
                        // Range selection: if current selection is an edge on same face type,
                        // select all edges in between along the shared axis
                        let mut did_range_select = false;
                        if let Selection::Edge { room: sel_room, x: sel_x, z: sel_z, face_idx: sel_face_idx, edge_idx: sel_edge_idx, wall_face: sel_wall_face } = &state.selection {
                            // Range select requires same room, same face type, same edge orientation
                            if *sel_room == room_idx && *sel_face_idx == face_idx && *sel_edge_idx == edge_idx {
                                let mut edges_to_add: Vec<Selection> = Vec::new();

                                if face_idx < 2 {
                                    // Floor/ceiling edges:
                                    // edge_idx 0 (north) or 2 (south): varies along X axis
                                    // edge_idx 1 (east) or 3 (west): varies along Z axis
                                    if edge_idx == 0 || edge_idx == 2 {
                                        // North/South edges - range along X
                                        let x_min = (*sel_x).min(gx);
                                        let x_max = (*sel_x).max(gx);
                                        let z_coord = if edge_idx == 0 { (*sel_z).min(gz) } else { (*sel_z).max(gz) };
                                        for x in x_min..=x_max {
                                            edges_to_add.push(Selection::Edge {
                                                room: room_idx, x, z: z_coord,
                                                face_idx, edge_idx, wall_face: None,
                                            });
                                        }
                                    } else {
                                        // East/West edges - range along Z
                                        let z_min = (*sel_z).min(gz);
                                        let z_max = (*sel_z).max(gz);
                                        let x_coord = if edge_idx == 1 { (*sel_x).max(gx) } else { (*sel_x).min(gx) };
                                        for z in z_min..=z_max {
                                            edges_to_add.push(Selection::Edge {
                                                room: room_idx, x: x_coord, z,
                                                face_idx, edge_idx, wall_face: None,
                                            });
                                        }
                                    }
                                } else if edge_idx == 0 || edge_idx == 2 {
                                    // Wall top/bottom edges - trace wall contour from start to end
                                    // This follows connected walls regardless of orientation (N/S/E/W/diagonal)
                                    if let Some(room) = state.level.rooms.get(room_idx) {
                                        if let (Some(start_face), Some(end_face)) = (sel_wall_face.clone(), wall_face.clone()) {
                                            // Use existing find_wall_path function which handles all wall types
                                            if let Some(path) = find_wall_path(room, *sel_x, *sel_z, &start_face, gx, gz, &end_face) {
                                                for (px, pz, pwf) in path {
                                                    edges_to_add.push(Selection::Edge {
                                                        room: room_idx,
                                                        x: px,
                                                        z: pz,
                                                        face_idx,
                                                        edge_idx,
                                                        wall_face: Some(pwf),
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }

                                // Add all edges in range to multi-selection
                                let had_edges = !edges_to_add.is_empty();
                                // First, add the original selection to multi-selection so it's not lost
                                // when we change state.selection to the new clicked edge
                                if !state.multi_selection.contains(&state.selection) {
                                    state.multi_selection.push(state.selection.clone());
                                }
                                for edge_sel in edges_to_add {
                                    if !state.multi_selection.contains(&edge_sel) {
                                        state.multi_selection.push(edge_sel);
                                    }
                                }
                                did_range_select = had_edges;
                            }
                        }
                        if !did_range_select {
                            state.toggle_multi_selection(new_selection.clone());
                        }
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
                } else if let Some((anchor_gx, anchor_gz)) = geometry_paste_preview {
                    // Click to paste geometry from clipboard at preview location
                    if let Some(gc) = state.geometry_clipboard.clone() {
                        // Create a temporary selection at the anchor point for paste_geometry_selection
                        state.set_selection(Selection::Sector {
                            room: state.current_room,
                            x: anchor_gx.max(0) as usize,
                            z: anchor_gz.max(0) as usize,
                        });
                        // Trigger paste via the action system by setting a flag
                        // We'll handle the paste inline here since we have all the info
                        crate::editor::layout::paste_geometry_at(state, &gc, anchor_gx, anchor_gz);
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
                        // Regular click: Check if clicking on an already-selected face
                        let is_already_selected = state.selection == new_selection ||
                            state.multi_selection.contains(&new_selection);

                        if !is_already_selected {
                            // Clicking on unselected face: Deselect all, select just this one
                            if state.selection != new_selection || !state.multi_selection.is_empty() {
                                state.save_selection_undo();
                                state.clear_multi_selection();
                                state.set_selection(new_selection.clone());
                            }
                        }
                        // If already selected, keep current selection intact for dragging
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

                    // Check if Shift is held for drag mode selection
                    if shift_down {
                    // Y-axis drag mode: move faces up/down
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
                        // X/Z relocation mode: move faces in the horizontal plane
                        state.xz_drag_active = true;
                        state.xz_drag_initial_positions = faces_to_drag;
                        state.xz_drag_delta = (0, 0);

                        // Calculate average Y for horizontal drag plane
                        let avg_y = calculate_selection_center_y(state);
                        state.xz_drag_plane_y = avg_y;

                        // Get initial world position using pick_plane
                        if let Some((fb_x, fb_y)) = screen_to_fb(ctx.mouse.x, ctx.mouse.y) {
                            if let Some(world_pos) = pick_plane(
                                Vec3::new(0.0, avg_y, 0.0),
                                Vec3::new(0.0, 1.0, 0.0),  // Y-up normal
                                Vec3::ZERO,
                                (fb_x, fb_y),
                                &state.camera_3d,
                                fb.width, fb.height,
                            ) {
                                state.xz_drag_start_world = (world_pos.x, world_pos.z);
                            }
                        }
                    }
                } else {
                    // Clicked on nothing - start box select
                    if let Some((fb_x, fb_y)) = screen_to_fb(ctx.mouse.x, ctx.mouse.y) {
                        // If not shift-clicking, clear existing selection
                        if !shift_down {
                            if state.selection != Selection::None || !state.multi_selection.is_empty() {
                                state.save_selection_undo();
                                state.set_selection(Selection::None);
                                state.clear_multi_selection();
                            }
                        }
                        // Start box select
                        state.selection_rect_start = Some((fb_x, fb_y));
                        state.selection_rect_end = Some((fb_x, fb_y));
                        state.box_selecting = true;
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
            else if state.tool == EditorTool::DrawWall && !state.wall_direction.is_diagonal() {
                if let Some((grid_x, grid_z, dir, _corner_heights, wall_state, _mouse_y)) = preview_wall {
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
                            // Use wall_prefer_high to select gap (same as preview)
                            let gap_y = if state.wall_prefer_high {
                                Some(super::CEILING_HEIGHT - 1.0)  // Near ceiling
                            } else {
                                Some(1.0)  // Near floor
                            };
                            state.wall_drag_mouse_y = gap_y;
                        }
                    } else {
                        state.set_status("Edge is fully covered", 2.0);
                    }
                }
            }
            // DrawWall mode with diagonal direction - start drag for diagonal wall placement
            else if state.tool == EditorTool::DrawWall && state.wall_direction.is_diagonal() {
                if let Some((grid_x, grid_z, _is_nwse, _corner_heights)) = preview_diagonal_wall {
                    // Convert world coords to grid coords
                    if let Some(room) = state.level.rooms.get(state.current_room) {
                        let local_x = grid_x - room.position.x;
                        let local_z = grid_z - room.position.z;
                        let gx = (local_x / SECTOR_SIZE).floor() as i32;
                        let gz = (local_z / SECTOR_SIZE).floor() as i32;
                        // Use wall_direction which already has the diagonal type
                        state.wall_drag_start = Some((gx, gz, state.wall_direction));
                        state.wall_drag_current = Some((gx, gz, state.wall_direction));
                        // Use wall_prefer_high to select gap (same as axis-aligned walls)
                        let gap_y = if state.wall_prefer_high {
                            Some(super::CEILING_HEIGHT - 1.0)  // Near ceiling
                        } else {
                            Some(1.0)  // Near floor
                        };
                        state.wall_drag_mouse_y = gap_y;
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

        // Continue X/Z relocation drag
        if ctx.mouse.left_down && state.xz_drag_active {
            if let Some((fb_x, fb_y)) = screen_to_fb(ctx.mouse.x, ctx.mouse.y) {
                if let Some(world_pos) = pick_plane(
                    Vec3::new(0.0, state.xz_drag_plane_y, 0.0),
                    Vec3::new(0.0, 1.0, 0.0),
                    Vec3::ZERO,
                    (fb_x, fb_y),
                    &state.camera_3d,
                    fb.width, fb.height,
                ) {
                    let world_dx = world_pos.x - state.xz_drag_start_world.0;
                    let world_dz = world_pos.z - state.xz_drag_start_world.1;

                    // Convert to grid delta (snap to sectors)
                    let grid_dx = (world_dx / SECTOR_SIZE).round() as i32;
                    let grid_dz = (world_dz / SECTOR_SIZE).round() as i32;

                    // Save undo on first actual movement (both geometry and selection)
                    if !state.viewport_drag_started && (grid_dx != 0 || grid_dz != 0) {
                        state.save_selection_undo();
                        state.save_undo();
                        state.viewport_drag_started = true;
                    }

                    state.xz_drag_delta = (grid_dx, grid_dz);
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

        // Continue cardinal wall drag (update current grid position based on mouse, locked to start direction)
        // Diagonal wall drag is handled in a separate block below
        if ctx.mouse.left_down {
            if let Some((start_gx, start_gz, start_dir)) = state.wall_drag_start {
                if !start_dir.is_diagonal() {
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
                        use crate::world::Direction;
                        // Lock to the axis based on wall direction
                        // North/South walls extend along X axis (fixed Z)
                        // East/West walls extend along Z axis (fixed X)
                        let (final_gx, final_gz) = match start_dir {
                            Direction::North | Direction::South => (gx, start_gz),
                            Direction::East | Direction::West => (start_gx, gz),
                            Direction::NwSe | Direction::NeSw => unreachable!(), // Handled separately
                        };
                        state.wall_drag_current = Some((final_gx, final_gz, start_dir));
                    }
                }
            }
        }

        // Continue diagonal wall drag (update current grid position, locked to diagonal movement)
        if ctx.mouse.left_down {
            if let Some((start_gx, start_gz, start_dir)) = state.wall_drag_start {
                if start_dir.is_diagonal() {
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
                        use crate::world::Direction;
                        // Lock to diagonal movement: both X and Z must change together
                        // Use the axis with the larger delta to determine the diagonal length
                        let dx: i32 = mouse_gx - start_gx;
                        let dz: i32 = mouse_gz - start_gz;

                        // NW-SE diagonal: +X goes with +Z, -X goes with -Z
                        // NE-SW diagonal: +X goes with -Z, -X goes with +Z
                        let diag_len = dx.abs().max(dz.abs());

                        // Determine the diagonal direction based on which quadrant the mouse is in
                        let (final_gx, final_gz) = if start_dir == Direction::NwSe {
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

                        state.wall_drag_current = Some((final_gx, final_gz, start_dir));
                    }
                }
            }
        }

        // Update box select rectangle on drag
        if ctx.mouse.left_down && state.box_selecting {
            if let Some((fb_x, fb_y)) = screen_to_fb(ctx.mouse.x, ctx.mouse.y) {
                state.selection_rect_end = Some((fb_x, fb_y));
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
                            // Shift all objects to match new grid indices
                            for obj in &mut room.objects {
                                obj.sector_x += 1;
                            }
                        }

                        // Expand in negative Z direction
                        while min_gz + offset_z < 0 {
                            room.position.z -= SECTOR_SIZE;
                            for col in &mut room.sectors {
                                col.insert(0, None);
                            }
                            room.depth += 1;
                            offset_z += 1;
                            // Shift all objects to match new grid indices
                            for obj in &mut room.objects {
                                obj.sector_z += 1;
                            }
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

            // Handle wall drag completion (axis-aligned walls only)
            if let (Some((start_gx, start_gz, dir)), Some((end_gx, end_gz, _))) = (state.wall_drag_start, state.wall_drag_current) {
                if !dir.is_diagonal() {
                use crate::world::Direction;

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
                    Direction::NwSe | Direction::NeSw => unreachable!(),
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
                        Direction::NwSe | Direction::NeSw => unreachable!(),
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
                        // Shift all objects to match new grid indices
                        for obj in &mut room.objects {
                            obj.sector_x += 1;
                        }
                    }

                    // Expand in negative Z direction
                    while min_gz + offset_z < 0 {
                        room.position.z -= SECTOR_SIZE;
                        for col in &mut room.sectors {
                            col.insert(0, None);
                        }
                        room.depth += 1;
                        offset_z += 1;
                        // Shift all objects to match new grid indices
                        for obj in &mut room.objects {
                            obj.sector_z += 1;
                        }
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
                            Direction::NwSe | Direction::NeSw => unreachable!(),
                        };

                        let adjusted_gx = (gx + offset_x) as usize;
                        let adjusted_gz = (gz + offset_z) as usize;

                        // Check if there's a gap to fill (handles both empty edges and gaps between walls)
                        // Use room's effective vertical bounds for gap detection
                        // Use the stored mouse_y from drag start for consistent gap selection
                        room.ensure_sector(adjusted_gx, adjusted_gz);
                        let (fallback_bottom, fallback_top) = room.effective_height_bounds();
                        if let Some(sector) = room.get_sector(adjusted_gx, adjusted_gz) {
                            if let Some(heights) = sector.next_wall_position(dir, fallback_bottom, fallback_top, state.wall_drag_mouse_y) {
                                // Calculate wall center position in world space
                                let base_x = room.position.x + adjusted_gx as f32 * SECTOR_SIZE;
                                let base_z = room.position.z + adjusted_gz as f32 * SECTOR_SIZE;
                                let wall_center = match dir {
                                    Direction::North => macroquad::math::Vec3::new(base_x + SECTOR_SIZE / 2.0, 0.0, base_z),
                                    Direction::South => macroquad::math::Vec3::new(base_x + SECTOR_SIZE / 2.0, 0.0, base_z + SECTOR_SIZE),
                                    Direction::East => macroquad::math::Vec3::new(base_x + SECTOR_SIZE, 0.0, base_z + SECTOR_SIZE / 2.0),
                                    Direction::West => macroquad::math::Vec3::new(base_x, 0.0, base_z + SECTOR_SIZE / 2.0),
                                    Direction::NwSe | Direction::NeSw => unreachable!(),
                                };

                                // Wall normal (front-facing direction)
                                let wall_normal = match dir {
                                    Direction::North => macroquad::math::Vec3::new(0.0, 0.0, 1.0),
                                    Direction::South => macroquad::math::Vec3::new(0.0, 0.0, -1.0),
                                    Direction::East => macroquad::math::Vec3::new(-1.0, 0.0, 0.0),
                                    Direction::West => macroquad::math::Vec3::new(1.0, 0.0, 0.0),
                                    Direction::NwSe | Direction::NeSw => unreachable!(),
                                };

                                // Vector from wall to camera (XZ plane only)
                                let cam_pos = state.camera_3d.position;
                                let to_camera = macroquad::math::Vec3::new(
                                    cam_pos.x - wall_center.x,
                                    0.0,
                                    cam_pos.z - wall_center.z,
                                );

                                // If dot product is negative, camera is on the back side
                                let dot = wall_normal.dot(to_camera);
                                let normal_mode = if dot < 0.0 {
                                    crate::world::FaceNormalMode::Back
                                } else {
                                    crate::world::FaceNormalMode::Front
                                };

                                // There's a gap - add wall with computed heights
                                if let Some(sector_mut) = room.get_sector_mut(adjusted_gx, adjusted_gz) {
                                    let mut wall = crate::world::VerticalFace::new_sloped(
                                        heights[0], heights[1], heights[2], heights[3],
                                        texture.clone()
                                    );
                                    wall.normal_mode = normal_mode;
                                    sector_mut.walls_mut(dir).push(wall);
                                    placed_count += 1;
                                }
                            }
                        }
                    }
                    room.recalculate_bounds();
                }

                state.mark_portals_dirty();
                if placed_count > 0 {
                    state.set_status(&format!("Created {} {} walls", placed_count, dir.name().to_lowercase()), 2.0);
                }

                // Clear wall drag state
                state.wall_drag_start = None;
                state.wall_drag_current = None;
                state.wall_drag_mouse_y = None;
                } // end if !dir.is_diagonal()
            }

            // Handle diagonal wall drag completion
            if let (Some((start_gx, start_gz, start_dir)), Some((end_gx, end_gz, _))) = (state.wall_drag_start, state.wall_drag_current) {
                if start_dir.is_diagonal() {
                    use crate::world::{Direction, VerticalFace};

                    let is_nwse = start_dir == Direction::NwSe;

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
                            // Shift all objects to match new grid indices
                            for obj in &mut room.objects {
                                obj.sector_x += 1;
                            }
                        }

                        // Expand in negative Z direction
                        while min_gz + offset_z < 0 {
                            room.position.z -= SECTOR_SIZE;
                            for col in &mut room.sectors {
                                col.insert(0, None);
                            }
                            room.depth += 1;
                            offset_z += 1;
                            // Shift all objects to match new grid indices
                            for obj in &mut room.objects {
                                obj.sector_z += 1;
                            }
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
                            // Use room's effective vertical bounds for gap detection
                            room.ensure_sector(adjusted_gx, adjusted_gz);
                            let (fallback_bottom, fallback_top) = room.effective_height_bounds();
                            if let Some(sector) = room.get_sector(adjusted_gx, adjusted_gz) {
                                if let Some(heights) = sector.next_diagonal_wall_position(is_nwse, fallback_bottom, fallback_top, state.wall_drag_mouse_y) {
                                    // Calculate diagonal wall center position in world space
                                    let base_x = room.position.x + adjusted_gx as f32 * SECTOR_SIZE;
                                    let base_z = room.position.z + adjusted_gz as f32 * SECTOR_SIZE;
                                    let wall_center = macroquad::math::Vec3::new(
                                        base_x + SECTOR_SIZE / 2.0,
                                        0.0,
                                        base_z + SECTOR_SIZE / 2.0,
                                    );

                                    // Diagonal wall normal (perpendicular to the diagonal line)
                                    // NW-SE diagonal: normal faces (1, 0, -1) normalized
                                    // NE-SW diagonal: normal faces (-1, 0, -1) normalized
                                    let inv_sqrt2 = 1.0 / 2.0_f32.sqrt();
                                    let wall_normal = if is_nwse {
                                        macroquad::math::Vec3::new(inv_sqrt2, 0.0, -inv_sqrt2)
                                    } else {
                                        macroquad::math::Vec3::new(-inv_sqrt2, 0.0, -inv_sqrt2)
                                    };

                                    // Vector from wall to camera (XZ plane only)
                                    let cam_pos = state.camera_3d.position;
                                    let to_camera = macroquad::math::Vec3::new(
                                        cam_pos.x - wall_center.x,
                                        0.0,
                                        cam_pos.z - wall_center.z,
                                    );

                                    // If dot product is negative, camera is on the back side
                                    let dot = wall_normal.dot(to_camera);
                                    let normal_mode = if dot < 0.0 {
                                        crate::world::FaceNormalMode::Back
                                    } else {
                                        crate::world::FaceNormalMode::Front
                                    };

                                    // There's a gap - add wall with computed heights
                                    if let Some(sector_mut) = room.get_sector_mut(adjusted_gx, adjusted_gz) {
                                        let mut wall = VerticalFace::new_sloped(
                                            heights[0], heights[1], heights[2], heights[3],
                                            texture.clone()
                                        );
                                        wall.normal_mode = normal_mode;
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
                        state.set_status(&format!("Created {} {} diagonal walls", placed_count, start_dir.name()), 2.0);
                    }

                    // Clear wall drag state (shared with axis-aligned walls)
                    state.wall_drag_start = None;
                    state.wall_drag_current = None;
                    state.wall_drag_mouse_y = None;
                }
            }

            // Apply X/Z relocation
            if state.xz_drag_active {
                let (dx, dz) = state.xz_drag_delta;

                if state.viewport_drag_started && (dx != 0 || dz != 0) {
                    let faces = state.xz_drag_initial_positions.clone();
                    let (moved_count, total_dx, total_dz, trim_x, trim_z) = relocate_faces(state, &faces, dx, dz);
                    // Selection offset = movement delta + expansion offset - trim offset
                    // (trim shifts all coordinates back by trim amount)
                    let selection_dx = total_dx - trim_x as i32;
                    let selection_dz = total_dz - trim_z as i32;
                    update_selection_positions(state, &faces, selection_dx, selection_dz);
                    if moved_count > 0 {
                        state.set_status(&format!("Moved {} face(s)", moved_count), 2.0);
                    }
                }

                // Cleanup X/Z drag state
                state.xz_drag_active = false;
                state.xz_drag_initial_positions.clear();
                state.xz_drag_delta = (0, 0);
                state.viewport_drag_started = false;
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

            // Finalize box select
            if state.box_selecting {
                if let (Some((x0, y0)), Some((x1, y1))) = (state.selection_rect_start, state.selection_rect_end) {
                    let rect_min_x = x0.min(x1);
                    let rect_max_x = x0.max(x1);
                    let rect_min_y = y0.min(y1);
                    let rect_max_y = y0.max(y1);

                    // Only process if box has meaningful size (not just a click)
                    if (rect_max_x - rect_min_x) > 3.0 || (rect_max_y - rect_min_y) > 3.0 {
                        let collected = find_selections_in_rect(state, fb, rect_min_x, rect_min_y, rect_max_x, rect_max_y);
                        if !collected.is_empty() {
                            state.save_selection_undo();
                            for sel in collected {
                                state.add_to_multi_selection(sel);
                            }
                            // Set primary selection to first item if none set
                            if state.selection == Selection::None && !state.multi_selection.is_empty() {
                                state.selection = state.multi_selection[0].clone();
                            }
                            let count = state.multi_selection.len();
                            state.set_status(&format!("Selected {} items", count), 2.0);
                        }
                    }
                }

                // Clear box select state
                state.selection_rect_start = None;
                state.selection_rect_end = None;
                state.box_selecting = false;
            }
        }
    }

    // Update mouse position for next frame
    state.viewport_last_mouse = mouse_pos;

    // Update orbit target after selection changes (in orbit mode)
    if should_update_orbit_target {
        state.update_orbit_target();
        state.sync_camera_from_orbit();
    }

    let vp_input_ms = EditorFrameTimings::elapsed_ms(input_start);

    // === CLEAR PHASE ===
    let clear_start = EditorFrameTimings::start();

    // Clear framebuffer - use 3D skybox if configured
    if let Some(skybox) = &state.level.skybox {
        fb.clear(RasterColor::new(0, 0, 0));
        let time = macroquad::prelude::get_time() as f32;
        fb.render_skybox(skybox, &state.camera_3d, time);
    } else {
        fb.clear(RasterColor::new(30, 30, 40));
    }

    let vp_clear_ms = EditorFrameTimings::elapsed_ms(clear_start);

    // === GRID PHASE ===
    let grid_start = EditorFrameTimings::start();

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

    // Draw wall drag preview (axis-aligned walls only - diagonal handled separately)
    if let (Some((start_gx, start_gz, dir)), Some((end_gx, end_gz, _))) = (state.wall_drag_start, state.wall_drag_current) {
        if !dir.is_diagonal() {
        use crate::world::Direction;

        // Get room position
        let room_pos = state.level.rooms.get(state.current_room)
            .map(|r| r.position)
            .unwrap_or_default();

        // Calculate the line of walls based on direction
        let (iter_axis_start, iter_axis_end, fixed_axis) = match dir {
            Direction::North | Direction::South => (start_gx, end_gx, start_gz),
            Direction::East | Direction::West => (start_gz, end_gz, start_gx),
            Direction::NwSe | Direction::NeSw => unreachable!(),
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
                Direction::NwSe | Direction::NeSw => unreachable!(),
            };

            // Calculate world position for this grid cell
            let grid_x = room_pos.x + (gx as f32) * SECTOR_SIZE;
            let grid_z = room_pos.z + (gz as f32) * SECTOR_SIZE;

            // Check if there's a sector with existing walls - use gap heights if so
            // Use room's effective vertical bounds for gap detection
            let (corner_heights, is_gap_fill) = if gx >= 0 && gz >= 0 {
                if let Some(room) = state.level.rooms.get(state.current_room) {
                    let (fallback_bottom, fallback_top) = room.effective_height_bounds();
                    if let Some(sector) = room.get_sector(gx as usize, gz as usize) {
                        // Check if edge has walls and find the gap
                        // Use the stored mouse_y from drag start for consistent gap selection
                        let has_walls = !sector.walls(dir).is_empty();
                        if let Some(heights) = sector.next_wall_position(dir, fallback_bottom, fallback_top, state.wall_drag_mouse_y) {
                            (heights, has_walls)
                        } else {
                            // Fully covered - skip this segment
                            continue;
                        }
                    } else {
                        // No sector - use room's vertical bounds
                        ([fallback_bottom, fallback_bottom, fallback_top, fallback_top], false)
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
                Direction::NwSe | Direction::NeSw => unreachable!(),
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
            // Skip if wall is triangular (collapsed vertices make + look wrong)
            let is_triangular = (corner_heights[0] - corner_heights[3]).abs() < 1.0
                             || (corner_heights[1] - corner_heights[2]).abs() < 1.0;
            if is_gap_fill && !is_triangular {
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
        state.set_status(&format!("Drag to place {} {} walls", wall_count, dir.name().to_lowercase()), 0.1);
        } // end if !dir.is_diagonal()
    }

    // Draw diagonal wall drag preview
    if let (Some((start_gx, start_gz, start_dir)), Some((end_gx, end_gz, _))) = (state.wall_drag_start, state.wall_drag_current) {
        if start_dir.is_diagonal() {
            use crate::world::Direction;
            let is_nwse = start_dir == Direction::NwSe;

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

                // Use room's effective vertical bounds
                let (floor_rel, ceiling_rel) = state.level.rooms.get(state.current_room)
                    .map(|r| r.effective_height_bounds())
                    .unwrap_or((0.0, super::CEILING_HEIGHT));
                let floor_y = room_pos.y + floor_rel;
                let ceiling_y = room_pos.y + ceiling_rel;

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
            state.set_status(&format!("Drag to place {} {} walls", diag_count, start_dir.name()), 0.1);
        }
    }

    let vp_grid_ms = EditorFrameTimings::elapsed_ms(grid_start);

    // === LIGHTS PHASE ===
    let lights_start = EditorFrameTimings::start();

    // Build texture map from texture packs + user textures
    let mut texture_map: std::collections::HashMap<(String, String), usize> = std::collections::HashMap::new();
    let mut texture_idx = 0;
    for pack in &state.texture_packs {
        for tex in &pack.textures {
            texture_map.insert((pack.name.clone(), tex.name.clone()), texture_idx);
            texture_idx += 1;
        }
    }
    // Add user textures (using _USER pack name convention)
    for name in state.user_textures.names() {
        texture_map.insert((crate::world::USER_TEXTURE_PACK.to_string(), name.to_string()), texture_idx);
        texture_idx += 1;
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

    let vp_lights_ms = EditorFrameTimings::elapsed_ms(lights_start);

    // === TEXTURE CONVERSION PHASE ===
    let texconv_start = EditorFrameTimings::start();

    // Convert textures to RGB555 if enabled (lazy cache: invalidate when generation changes)
    let use_rgb555 = state.raster_settings.use_rgb555;
    if use_rgb555 && (state.textures_15_cache_generation != state.texture_generation || state.textures_15_cache.len() != textures.len()) {
        state.textures_15_cache = textures.iter().map(|t| t.to_15()).collect();
        state.textures_15_cache_generation = state.texture_generation;
    }

    let vp_texconv_ms = EditorFrameTimings::elapsed_ms(texconv_start);

    // === RENDER PHASE (meshgen + raster) ===
    let render_start = EditorFrameTimings::start();
    let mut vp_meshgen_ms = 0.0f32;
    let mut vp_raster_ms = 0.0f32;

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

        // Time mesh generation
        let meshgen_start = EditorFrameTimings::start();
        let (vertices, faces) = room.to_render_data_with_textures(&resolve_texture);
        vp_meshgen_ms += EditorFrameTimings::elapsed_ms(meshgen_start);

        // Build fog parameter from room settings
        let fog = if room.fog.enabled {
            let (r, g, b) = room.fog.color;
            let fog_color = RasterColor::new(
                (r * 255.0) as u8,
                (g * 255.0) as u8,
                (b * 255.0) as u8,
            );
            Some((room.fog.start, room.fog.falloff, fog_color))
        } else {
            None
        };

        // Time rasterization
        let raster_start = EditorFrameTimings::start();
        if use_rgb555 {
            render_mesh_15(fb, &vertices, &faces, &state.textures_15_cache, None, &state.camera_3d, &render_settings, fog);
        } else {
            render_mesh(fb, &vertices, &faces, textures, &state.camera_3d, &render_settings);
        }
        vp_raster_ms += EditorFrameTimings::elapsed_ms(raster_start);
    }

    let _render_total_ms = EditorFrameTimings::elapsed_ms(render_start);

    // === PREVIEW/SELECTION PHASE ===
    let preview_start = EditorFrameTimings::start();

    // Draw hovering floor grid centered on hovered tile when in floor placement mode
    // (Drawn after mesh so it renders on top)
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
    // (Drawn after mesh so it renders on top)
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

    // Draw subtle sector boundary highlight for wall placement
    // Only show if on the drag line (same check as wall preview)
    if let Some((grid_x, grid_z, dir, _, _, _)) = preview_wall {
        // Check if this sector is on the drag line (if dragging)
        let on_drag_line = if let Some((start_gx, start_gz, start_dir)) = state.wall_drag_start {
            use crate::world::Direction;
            if start_dir.is_diagonal() {
                false // Diagonal walls have separate boundary highlighting
            } else {
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
                    Direction::NwSe | Direction::NeSw => unreachable!(),
                }
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
        use crate::world::Direction;
        // Check if this sector is on the drag line (if dragging)
        let on_drag_line = if let Some((start_gx, start_gz, start_dir)) = state.wall_drag_start {
            if !start_dir.is_diagonal() {
                true // Not diagonal drag, always show
            } else {
                let start_is_nwse = start_dir == Direction::NwSe;
                // Convert preview world coords to grid coords
                let preview_gx = if let Some(room) = state.level.rooms.get(state.current_room) {
                    ((grid_x - room.position.x) / SECTOR_SIZE).floor() as i32
                } else { 0 };
                let preview_gz = if let Some(room) = state.level.rooms.get(state.current_room) {
                    ((grid_z - room.position.z) / SECTOR_SIZE).floor() as i32
                } else { 0 };
                // Diagonal is on the line if type matches AND it's on the simple diagonal line
                if let Some((end_gx, end_gz, _)) = state.wall_drag_current {
                    if is_nwse != start_is_nwse {
                        false // Wrong diagonal type
                    } else {
                        // Check if preview point is on the diagonal line from start to end
                        let sx: i32 = if start_gx < end_gx { 1 } else if start_gx > end_gx { -1 } else { 0 };
                        let sz: i32 = if start_gz < end_gz { 1 } else if start_gz > end_gz { -1 } else { 0 };
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
            if start_dir.is_diagonal() {
                false // Diagonal walls have separate preview
            } else {
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
                    Direction::NwSe | Direction::NeSw => unreachable!(),
                }
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
                Direction::NwSe | Direction::NeSw => unreachable!(),
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
                Direction::NwSe | Direction::NeSw => unreachable!(),
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
            // Skip if wall is triangular (collapsed vertices make + look wrong)
            let is_triangular = (corner_heights[0] - corner_heights[3]).abs() < 1.0
                             || (corner_heights[1] - corner_heights[2]).abs() < 1.0;
            if wall_state == 1 && !is_triangular {
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

    // Draw diagonal wall preview when in DrawWall mode with diagonal direction
    // Skip single diagonal preview if it's not on the drag line
    if let Some((grid_x, grid_z, is_nwse, corner_heights)) = preview_diagonal_wall {
        use crate::world::Direction;
        // Check if this preview is on the drag line (if dragging)
        let on_drag_line = if let Some((start_gx, start_gz, start_dir)) = state.wall_drag_start {
            if !start_dir.is_diagonal() {
                true // Not diagonal drag, always show
            } else {
                let start_is_nwse = start_dir == Direction::NwSe;
                // Convert preview world coords to grid coords
                let preview_gx = if let Some(room) = state.level.rooms.get(state.current_room) {
                    ((grid_x - room.position.x) / SECTOR_SIZE).floor() as i32
                } else { 0 };
                let preview_gz = if let Some(room) = state.level.rooms.get(state.current_room) {
                    ((grid_z - room.position.z) / SECTOR_SIZE).floor() as i32
                } else { 0 };
                // Diagonal is on the line if type matches AND it's on the simple diagonal line
                if let Some((end_gx, end_gz, _)) = state.wall_drag_current {
                    if is_nwse != start_is_nwse {
                        false // Wrong diagonal type
                    } else {
                        // Check if preview point is on the diagonal line from start to end
                        // Simple check: both X and Z step together at the same rate
                        let sx: i32 = if start_gx < end_gx { 1 } else if start_gx > end_gx { -1 } else { 0 };
                        let sz: i32 = if start_gz < end_gz { 1 } else if start_gz > end_gz { -1 } else { 0 };
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
                                // Triangle 1 corners (use heights)
                                let corners_1 = [
                                    Vec3::new(base_x, room_y + floor.heights[0], base_z),                    // NW = 0
                                    Vec3::new(base_x + SECTOR_SIZE, room_y + floor.heights[1], base_z),      // NE = 1
                                    Vec3::new(base_x + SECTOR_SIZE, room_y + floor.heights[2], base_z + SECTOR_SIZE), // SE = 2
                                    Vec3::new(base_x, room_y + floor.heights[3], base_z + SECTOR_SIZE),      // SW = 3
                                ];
                                // Triangle 2 corners (use heights_2 if unlinked)
                                let h2 = floor.get_heights_2();
                                let corners_2 = [
                                    Vec3::new(base_x, room_y + h2[0], base_z),                    // NW = 0
                                    Vec3::new(base_x + SECTOR_SIZE, room_y + h2[1], base_z),      // NE = 1
                                    Vec3::new(base_x + SECTOR_SIZE, room_y + h2[2], base_z + SECTOR_SIZE), // SE = 2
                                    Vec3::new(base_x, room_y + h2[3], base_z + SECTOR_SIZE),      // SW = 3
                                ];
                                // Draw triangle edges based on split direction
                                match floor.split_direction {
                                    SplitDirection::NwSe => {
                                        // Tri1: NW-NE-SE, Tri2: NW-SE-SW
                                        draw_3d_line(fb, corners_1[0], corners_1[1], &state.camera_3d, hover_color);
                                        draw_3d_line(fb, corners_1[1], corners_1[2], &state.camera_3d, hover_color);
                                        draw_3d_line(fb, corners_2[2], corners_2[3], &state.camera_3d, hover_color);
                                        draw_3d_line(fb, corners_2[3], corners_2[0], &state.camera_3d, hover_color);
                                        // Diagonal for both triangles
                                        draw_3d_line(fb, corners_1[0], corners_1[2], &state.camera_3d, hover_color);
                                        draw_3d_line(fb, corners_2[0], corners_2[2], &state.camera_3d, hover_color);
                                    }
                                    SplitDirection::NeSw => {
                                        // Tri1: NW-NE-SW, Tri2: NE-SE-SW
                                        draw_3d_line(fb, corners_1[0], corners_1[1], &state.camera_3d, hover_color);
                                        draw_3d_line(fb, corners_1[0], corners_1[3], &state.camera_3d, hover_color);
                                        draw_3d_line(fb, corners_2[1], corners_2[2], &state.camera_3d, hover_color);
                                        draw_3d_line(fb, corners_2[2], corners_2[3], &state.camera_3d, hover_color);
                                        // Diagonal for both triangles
                                        draw_3d_line(fb, corners_1[1], corners_1[3], &state.camera_3d, hover_color);
                                        draw_3d_line(fb, corners_2[1], corners_2[3], &state.camera_3d, hover_color);
                                    }
                                }
                            }
                        }
                        SectorFace::Ceiling => {
                            if let Some(ceiling) = &sector.ceiling {
                                // Triangle 1 corners (use heights)
                                let corners_1 = [
                                    Vec3::new(base_x, room_y + ceiling.heights[0], base_z),                    // NW = 0
                                    Vec3::new(base_x + SECTOR_SIZE, room_y + ceiling.heights[1], base_z),      // NE = 1
                                    Vec3::new(base_x + SECTOR_SIZE, room_y + ceiling.heights[2], base_z + SECTOR_SIZE), // SE = 2
                                    Vec3::new(base_x, room_y + ceiling.heights[3], base_z + SECTOR_SIZE),      // SW = 3
                                ];
                                // Triangle 2 corners (use heights_2 if unlinked)
                                let h2 = ceiling.get_heights_2();
                                let corners_2 = [
                                    Vec3::new(base_x, room_y + h2[0], base_z),                    // NW = 0
                                    Vec3::new(base_x + SECTOR_SIZE, room_y + h2[1], base_z),      // NE = 1
                                    Vec3::new(base_x + SECTOR_SIZE, room_y + h2[2], base_z + SECTOR_SIZE), // SE = 2
                                    Vec3::new(base_x, room_y + h2[3], base_z + SECTOR_SIZE),      // SW = 3
                                ];
                                // Draw triangle edges based on split direction
                                match ceiling.split_direction {
                                    SplitDirection::NwSe => {
                                        draw_3d_line(fb, corners_1[0], corners_1[1], &state.camera_3d, hover_color);
                                        draw_3d_line(fb, corners_1[1], corners_1[2], &state.camera_3d, hover_color);
                                        draw_3d_line(fb, corners_2[2], corners_2[3], &state.camera_3d, hover_color);
                                        draw_3d_line(fb, corners_2[3], corners_2[0], &state.camera_3d, hover_color);
                                        draw_3d_line(fb, corners_1[0], corners_1[2], &state.camera_3d, hover_color);
                                        draw_3d_line(fb, corners_2[0], corners_2[2], &state.camera_3d, hover_color);
                                    }
                                    SplitDirection::NeSw => {
                                        draw_3d_line(fb, corners_1[0], corners_1[1], &state.camera_3d, hover_color);
                                        draw_3d_line(fb, corners_1[0], corners_1[3], &state.camera_3d, hover_color);
                                        draw_3d_line(fb, corners_2[1], corners_2[2], &state.camera_3d, hover_color);
                                        draw_3d_line(fb, corners_2[2], corners_2[3], &state.camera_3d, hover_color);
                                        draw_3d_line(fb, corners_1[1], corners_1[3], &state.camera_3d, hover_color);
                                        draw_3d_line(fb, corners_2[1], corners_2[3], &state.camera_3d, hover_color);
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

    // Draw geometry paste preview wireframe when hovering over empty space with clipboard
    if let Some((anchor_gx, anchor_gz)) = geometry_paste_preview {
        if let Some(gc) = &state.geometry_clipboard {
            if let Some(room) = state.level.rooms.get(state.current_room) {
                let room_y = room.position.y;
                let preview_color = RasterColor::new(100, 255, 150); // Green wireframe for paste preview

                // Get bounds for flip transformation
                let (min_x, max_x, min_z, max_z) = gc.bounds();
                let width = max_x - min_x;
                let depth = max_z - min_z;

                for copied_face in &gc.faces {
                    // Apply rotation first, then flip transformations
                    let (rx, rz, rw, rd) = match gc.rotation % 4 {
                        1 => (depth - copied_face.rel_z, copied_face.rel_x, depth, width),
                        2 => (width - copied_face.rel_x, depth - copied_face.rel_z, width, depth),
                        3 => (copied_face.rel_z, width - copied_face.rel_x, depth, width),
                        _ => (copied_face.rel_x, copied_face.rel_z, width, depth),
                    };
                    let (rel_x, rel_z) = match (gc.flip_h, gc.flip_v) {
                        (true, true) => (rw - rx, rd - rz),
                        (true, false) => (rw - rx, rz),
                        (false, true) => (rx, rd - rz),
                        (false, false) => (rx, rz),
                    };

                    let target_gx = anchor_gx + rel_x;
                    let target_gz = anchor_gz + rel_z;

                    // Calculate world position - allow any grid position (room will expand on paste)
                    let base_x = room.position.x + (target_gx as f32) * SECTOR_SIZE;
                    let base_z = room.position.z + (target_gz as f32) * SECTOR_SIZE;

                    match &copied_face.face {
                        CopiedFaceData::Floor(f) => {
                            // Apply rotation then flips to heights for preview
                            let mut heights = match gc.rotation % 4 {
                                1 => [f.heights[3], f.heights[0], f.heights[1], f.heights[2]],
                                2 => [f.heights[2], f.heights[3], f.heights[0], f.heights[1]],
                                3 => [f.heights[1], f.heights[2], f.heights[3], f.heights[0]],
                                _ => f.heights,
                            };
                            if gc.flip_h {
                                heights = [heights[1], heights[0], heights[3], heights[2]];
                            }
                            if gc.flip_v {
                                heights = [heights[3], heights[2], heights[1], heights[0]];
                            }
                            let corners = [
                                Vec3::new(base_x, room_y + heights[0], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, room_y + heights[1], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, room_y + heights[2], base_z + SECTOR_SIZE),
                                Vec3::new(base_x, room_y + heights[3], base_z + SECTOR_SIZE),
                            ];
                            // Draw quad outline + diagonal
                            draw_3d_line(fb, corners[0], corners[1], &state.camera_3d, preview_color);
                            draw_3d_line(fb, corners[1], corners[2], &state.camera_3d, preview_color);
                            draw_3d_line(fb, corners[2], corners[3], &state.camera_3d, preview_color);
                            draw_3d_line(fb, corners[3], corners[0], &state.camera_3d, preview_color);
                            draw_3d_line(fb, corners[0], corners[2], &state.camera_3d, preview_color);
                        }
                        CopiedFaceData::Ceiling(f) => {
                            let mut heights = match gc.rotation % 4 {
                                1 => [f.heights[3], f.heights[0], f.heights[1], f.heights[2]],
                                2 => [f.heights[2], f.heights[3], f.heights[0], f.heights[1]],
                                3 => [f.heights[1], f.heights[2], f.heights[3], f.heights[0]],
                                _ => f.heights,
                            };
                            if gc.flip_h {
                                heights = [heights[1], heights[0], heights[3], heights[2]];
                            }
                            if gc.flip_v {
                                heights = [heights[3], heights[2], heights[1], heights[0]];
                            }
                            let corners = [
                                Vec3::new(base_x, room_y + heights[0], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, room_y + heights[1], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, room_y + heights[2], base_z + SECTOR_SIZE),
                                Vec3::new(base_x, room_y + heights[3], base_z + SECTOR_SIZE),
                            ];
                            draw_3d_line(fb, corners[0], corners[1], &state.camera_3d, preview_color);
                            draw_3d_line(fb, corners[1], corners[2], &state.camera_3d, preview_color);
                            draw_3d_line(fb, corners[2], corners[3], &state.camera_3d, preview_color);
                            draw_3d_line(fb, corners[3], corners[0], &state.camera_3d, preview_color);
                            draw_3d_line(fb, corners[0], corners[2], &state.camera_3d, preview_color);
                        }
                        CopiedFaceData::WallNorth(_, w) => {
                            // If flip_v, this becomes south wall visually
                            let wall_z = if gc.flip_v { base_z + SECTOR_SIZE } else { base_z };
                            let p0 = Vec3::new(base_x, room_y + w.heights[0], wall_z);
                            let p1 = Vec3::new(base_x + SECTOR_SIZE, room_y + w.heights[1], wall_z);
                            let p2 = Vec3::new(base_x + SECTOR_SIZE, room_y + w.heights[2], wall_z);
                            let p3 = Vec3::new(base_x, room_y + w.heights[3], wall_z);
                            draw_3d_line(fb, p0, p1, &state.camera_3d, preview_color);
                            draw_3d_line(fb, p1, p2, &state.camera_3d, preview_color);
                            draw_3d_line(fb, p2, p3, &state.camera_3d, preview_color);
                            draw_3d_line(fb, p3, p0, &state.camera_3d, preview_color);
                        }
                        CopiedFaceData::WallSouth(_, w) => {
                            let wall_z = if gc.flip_v { base_z } else { base_z + SECTOR_SIZE };
                            let p0 = Vec3::new(base_x + SECTOR_SIZE, room_y + w.heights[0], wall_z);
                            let p1 = Vec3::new(base_x, room_y + w.heights[1], wall_z);
                            let p2 = Vec3::new(base_x, room_y + w.heights[2], wall_z);
                            let p3 = Vec3::new(base_x + SECTOR_SIZE, room_y + w.heights[3], wall_z);
                            draw_3d_line(fb, p0, p1, &state.camera_3d, preview_color);
                            draw_3d_line(fb, p1, p2, &state.camera_3d, preview_color);
                            draw_3d_line(fb, p2, p3, &state.camera_3d, preview_color);
                            draw_3d_line(fb, p3, p0, &state.camera_3d, preview_color);
                        }
                        CopiedFaceData::WallEast(_, w) => {
                            let wall_x = if gc.flip_h { base_x } else { base_x + SECTOR_SIZE };
                            let p0 = Vec3::new(wall_x, room_y + w.heights[0], base_z);
                            let p1 = Vec3::new(wall_x, room_y + w.heights[1], base_z + SECTOR_SIZE);
                            let p2 = Vec3::new(wall_x, room_y + w.heights[2], base_z + SECTOR_SIZE);
                            let p3 = Vec3::new(wall_x, room_y + w.heights[3], base_z);
                            draw_3d_line(fb, p0, p1, &state.camera_3d, preview_color);
                            draw_3d_line(fb, p1, p2, &state.camera_3d, preview_color);
                            draw_3d_line(fb, p2, p3, &state.camera_3d, preview_color);
                            draw_3d_line(fb, p3, p0, &state.camera_3d, preview_color);
                        }
                        CopiedFaceData::WallWest(_, w) => {
                            let wall_x = if gc.flip_h { base_x + SECTOR_SIZE } else { base_x };
                            let p0 = Vec3::new(wall_x, room_y + w.heights[0], base_z + SECTOR_SIZE);
                            let p1 = Vec3::new(wall_x, room_y + w.heights[1], base_z);
                            let p2 = Vec3::new(wall_x, room_y + w.heights[2], base_z);
                            let p3 = Vec3::new(wall_x, room_y + w.heights[3], base_z + SECTOR_SIZE);
                            draw_3d_line(fb, p0, p1, &state.camera_3d, preview_color);
                            draw_3d_line(fb, p1, p2, &state.camera_3d, preview_color);
                            draw_3d_line(fb, p2, p3, &state.camera_3d, preview_color);
                            draw_3d_line(fb, p3, p0, &state.camera_3d, preview_color);
                        }
                        CopiedFaceData::WallNwSe(_, w) => {
                            // NW to SE diagonal - swap to NE-SW if both flips
                            let (c0, c1) = if gc.flip_h != gc.flip_v {
                                // Becomes NE-SW diagonal
                                ((base_x + SECTOR_SIZE, base_z), (base_x, base_z + SECTOR_SIZE))
                            } else {
                                // Stays NW-SE
                                ((base_x, base_z), (base_x + SECTOR_SIZE, base_z + SECTOR_SIZE))
                            };
                            let p0 = Vec3::new(c0.0, room_y + w.heights[0], c0.1);
                            let p1 = Vec3::new(c1.0, room_y + w.heights[1], c1.1);
                            let p2 = Vec3::new(c1.0, room_y + w.heights[2], c1.1);
                            let p3 = Vec3::new(c0.0, room_y + w.heights[3], c0.1);
                            draw_3d_line(fb, p0, p1, &state.camera_3d, preview_color);
                            draw_3d_line(fb, p1, p2, &state.camera_3d, preview_color);
                            draw_3d_line(fb, p2, p3, &state.camera_3d, preview_color);
                            draw_3d_line(fb, p3, p0, &state.camera_3d, preview_color);
                        }
                        CopiedFaceData::WallNeSw(_, w) => {
                            let (c0, c1) = if gc.flip_h != gc.flip_v {
                                // Becomes NW-SE diagonal
                                ((base_x, base_z), (base_x + SECTOR_SIZE, base_z + SECTOR_SIZE))
                            } else {
                                // Stays NE-SW
                                ((base_x + SECTOR_SIZE, base_z), (base_x, base_z + SECTOR_SIZE))
                            };
                            let p0 = Vec3::new(c0.0, room_y + w.heights[0], c0.1);
                            let p1 = Vec3::new(c1.0, room_y + w.heights[1], c1.1);
                            let p2 = Vec3::new(c1.0, room_y + w.heights[2], c1.1);
                            let p3 = Vec3::new(c0.0, room_y + w.heights[3], c0.1);
                            draw_3d_line(fb, p0, p1, &state.camera_3d, preview_color);
                            draw_3d_line(fb, p1, p2, &state.camera_3d, preview_color);
                            draw_3d_line(fb, p2, p3, &state.camera_3d, preview_color);
                            draw_3d_line(fb, p3, p0, &state.camera_3d, preview_color);
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
                                    // Triangle 1 corners (use heights)
                                    let corners_1 = [
                                        Vec3::new(base_x, room_y + floor.heights[0], base_z),                    // NW = 0
                                        Vec3::new(base_x + SECTOR_SIZE, room_y + floor.heights[1], base_z),      // NE = 1
                                        Vec3::new(base_x + SECTOR_SIZE, room_y + floor.heights[2], base_z + SECTOR_SIZE), // SE = 2
                                        Vec3::new(base_x, room_y + floor.heights[3], base_z + SECTOR_SIZE),      // SW = 3
                                    ];
                                    // Triangle 2 corners (use heights_2 if unlinked)
                                    let h2 = floor.get_heights_2();
                                    let corners_2 = [
                                        Vec3::new(base_x, room_y + h2[0], base_z),                    // NW = 0
                                        Vec3::new(base_x + SECTOR_SIZE, room_y + h2[1], base_z),      // NE = 1
                                        Vec3::new(base_x + SECTOR_SIZE, room_y + h2[2], base_z + SECTOR_SIZE), // SE = 2
                                        Vec3::new(base_x, room_y + h2[3], base_z + SECTOR_SIZE),      // SW = 3
                                    ];
                                    // Draw triangle edges based on split direction
                                    match floor.split_direction {
                                        SplitDirection::NwSe => {
                                            // Tri1: NW-NE-SE, Tri2: NW-SE-SW
                                            // Tri1 edges: NW-NE, NE-SE
                                            draw_3d_line(fb, corners_1[0], corners_1[1], &state.camera_3d, select_color);
                                            draw_3d_line(fb, corners_1[1], corners_1[2], &state.camera_3d, select_color);
                                            // Tri2 edges: SE-SW, SW-NW
                                            draw_3d_line(fb, corners_2[2], corners_2[3], &state.camera_3d, select_color);
                                            draw_3d_line(fb, corners_2[3], corners_2[0], &state.camera_3d, select_color);
                                            // Diagonal: draw for both triangles (Tri1 NW-SE, Tri2 NW-SE)
                                            draw_3d_line(fb, corners_1[0], corners_1[2], &state.camera_3d, select_color);
                                            draw_3d_line(fb, corners_2[0], corners_2[2], &state.camera_3d, select_color);
                                        }
                                        SplitDirection::NeSw => {
                                            // Tri1: NW-NE-SW, Tri2: NE-SE-SW
                                            // Tri1 edges: NW-NE, NW-SW
                                            draw_3d_line(fb, corners_1[0], corners_1[1], &state.camera_3d, select_color);
                                            draw_3d_line(fb, corners_1[0], corners_1[3], &state.camera_3d, select_color);
                                            // Tri2 edges: NE-SE, SE-SW
                                            draw_3d_line(fb, corners_2[1], corners_2[2], &state.camera_3d, select_color);
                                            draw_3d_line(fb, corners_2[2], corners_2[3], &state.camera_3d, select_color);
                                            // Diagonal: draw for both triangles (Tri1 NE-SW, Tri2 NE-SW)
                                            draw_3d_line(fb, corners_1[1], corners_1[3], &state.camera_3d, select_color);
                                            draw_3d_line(fb, corners_2[1], corners_2[3], &state.camera_3d, select_color);
                                        }
                                    }
                                }
                            }
                            SectorFace::Ceiling => {
                                if let Some(ceiling) = &sector.ceiling {
                                    // Triangle 1 corners (use heights)
                                    let corners_1 = [
                                        Vec3::new(base_x, room_y + ceiling.heights[0], base_z),                    // NW = 0
                                        Vec3::new(base_x + SECTOR_SIZE, room_y + ceiling.heights[1], base_z),      // NE = 1
                                        Vec3::new(base_x + SECTOR_SIZE, room_y + ceiling.heights[2], base_z + SECTOR_SIZE), // SE = 2
                                        Vec3::new(base_x, room_y + ceiling.heights[3], base_z + SECTOR_SIZE),      // SW = 3
                                    ];
                                    // Triangle 2 corners (use heights_2 if unlinked)
                                    let h2 = ceiling.get_heights_2();
                                    let corners_2 = [
                                        Vec3::new(base_x, room_y + h2[0], base_z),                    // NW = 0
                                        Vec3::new(base_x + SECTOR_SIZE, room_y + h2[1], base_z),      // NE = 1
                                        Vec3::new(base_x + SECTOR_SIZE, room_y + h2[2], base_z + SECTOR_SIZE), // SE = 2
                                        Vec3::new(base_x, room_y + h2[3], base_z + SECTOR_SIZE),      // SW = 3
                                    ];
                                    // Draw triangle edges based on split direction
                                    match ceiling.split_direction {
                                        SplitDirection::NwSe => {
                                            // Tri1: NW-NE-SE, Tri2: NW-SE-SW
                                            // Tri1 edges: NW-NE, NE-SE
                                            draw_3d_line(fb, corners_1[0], corners_1[1], &state.camera_3d, select_color);
                                            draw_3d_line(fb, corners_1[1], corners_1[2], &state.camera_3d, select_color);
                                            // Tri2 edges: SE-SW, SW-NW
                                            draw_3d_line(fb, corners_2[2], corners_2[3], &state.camera_3d, select_color);
                                            draw_3d_line(fb, corners_2[3], corners_2[0], &state.camera_3d, select_color);
                                            // Diagonal: draw for both triangles (Tri1 NW-SE, Tri2 NW-SE)
                                            draw_3d_line(fb, corners_1[0], corners_1[2], &state.camera_3d, select_color);
                                            draw_3d_line(fb, corners_2[0], corners_2[2], &state.camera_3d, select_color);
                                        }
                                        SplitDirection::NeSw => {
                                            // Tri1: NW-NE-SW, Tri2: NE-SE-SW
                                            // Tri1 edges: NW-NE, NW-SW
                                            draw_3d_line(fb, corners_1[0], corners_1[1], &state.camera_3d, select_color);
                                            draw_3d_line(fb, corners_1[0], corners_1[3], &state.camera_3d, select_color);
                                            // Tri2 edges: NE-SE, SE-SW
                                            draw_3d_line(fb, corners_2[1], corners_2[2], &state.camera_3d, select_color);
                                            draw_3d_line(fb, corners_2[2], corners_2[3], &state.camera_3d, select_color);
                                            // Diagonal: draw for both triangles (Tri1 NE-SW, Tri2 NE-SW)
                                            draw_3d_line(fb, corners_1[1], corners_1[3], &state.camera_3d, select_color);
                                            draw_3d_line(fb, corners_2[1], corners_2[3], &state.camera_3d, select_color);
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

                        // Draw floor outline if floor exists (both triangles if heights unlinked)
                        if let Some(floor) = &sector.floor {
                            let corners_1 = [
                                Vec3::new(base_x, room_y + floor.heights[0], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, room_y + floor.heights[1], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, room_y + floor.heights[2], base_z + SECTOR_SIZE),
                                Vec3::new(base_x, room_y + floor.heights[3], base_z + SECTOR_SIZE),
                            ];
                            let h2 = floor.get_heights_2();
                            let corners_2 = [
                                Vec3::new(base_x, room_y + h2[0], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, room_y + h2[1], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, room_y + h2[2], base_z + SECTOR_SIZE),
                                Vec3::new(base_x, room_y + h2[3], base_z + SECTOR_SIZE),
                            ];
                            // Draw outer edges for both triangles
                            match floor.split_direction {
                                SplitDirection::NwSe => {
                                    // Tri1: NW-NE-SE outer edges: NW-NE, NE-SE
                                    draw_3d_line(fb, corners_1[0], corners_1[1], &state.camera_3d, select_color);
                                    draw_3d_line(fb, corners_1[1], corners_1[2], &state.camera_3d, select_color);
                                    // Tri2: NW-SE-SW outer edges: SE-SW, SW-NW
                                    draw_3d_line(fb, corners_2[2], corners_2[3], &state.camera_3d, select_color);
                                    draw_3d_line(fb, corners_2[3], corners_2[0], &state.camera_3d, select_color);
                                }
                                SplitDirection::NeSw => {
                                    // Tri1: NW-NE-SW outer edges: NW-NE, NW-SW
                                    draw_3d_line(fb, corners_1[0], corners_1[1], &state.camera_3d, select_color);
                                    draw_3d_line(fb, corners_1[0], corners_1[3], &state.camera_3d, select_color);
                                    // Tri2: NE-SE-SW outer edges: NE-SE, SE-SW
                                    draw_3d_line(fb, corners_2[1], corners_2[2], &state.camera_3d, select_color);
                                    draw_3d_line(fb, corners_2[2], corners_2[3], &state.camera_3d, select_color);
                                }
                            }
                        }

                        // Draw ceiling outline if ceiling exists (both triangles if heights unlinked)
                        if let Some(ceiling) = &sector.ceiling {
                            let corners_1 = [
                                Vec3::new(base_x, room_y + ceiling.heights[0], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, room_y + ceiling.heights[1], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, room_y + ceiling.heights[2], base_z + SECTOR_SIZE),
                                Vec3::new(base_x, room_y + ceiling.heights[3], base_z + SECTOR_SIZE),
                            ];
                            let h2 = ceiling.get_heights_2();
                            let corners_2 = [
                                Vec3::new(base_x, room_y + h2[0], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, room_y + h2[1], base_z),
                                Vec3::new(base_x + SECTOR_SIZE, room_y + h2[2], base_z + SECTOR_SIZE),
                                Vec3::new(base_x, room_y + h2[3], base_z + SECTOR_SIZE),
                            ];
                            // Draw outer edges for both triangles
                            match ceiling.split_direction {
                                SplitDirection::NwSe => {
                                    draw_3d_line(fb, corners_1[0], corners_1[1], &state.camera_3d, select_color);
                                    draw_3d_line(fb, corners_1[1], corners_1[2], &state.camera_3d, select_color);
                                    draw_3d_line(fb, corners_2[2], corners_2[3], &state.camera_3d, select_color);
                                    draw_3d_line(fb, corners_2[3], corners_2[0], &state.camera_3d, select_color);
                                }
                                SplitDirection::NeSw => {
                                    draw_3d_line(fb, corners_1[0], corners_1[1], &state.camera_3d, select_color);
                                    draw_3d_line(fb, corners_1[0], corners_1[3], &state.camera_3d, select_color);
                                    draw_3d_line(fb, corners_2[1], corners_2[2], &state.camera_3d, select_color);
                                    draw_3d_line(fb, corners_2[2], corners_2[3], &state.camera_3d, select_color);
                                }
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

    // Draw box select preview (live highlighting during drag)
    if state.box_selecting {
        if let (Some((x0, y0)), Some((x1, y1))) = (state.selection_rect_start, state.selection_rect_end) {
            let rect_min_x = x0.min(x1);
            let rect_max_x = x0.max(x1);
            let rect_min_y = y0.min(y1);
            let rect_max_y = y0.max(y1);

            // Only calculate if box has meaningful size
            if (rect_max_x - rect_min_x) > 3.0 || (rect_max_y - rect_min_y) > 3.0 {
                let preview_items = find_selections_in_rect(state, fb, rect_min_x, rect_min_y, rect_max_x, rect_max_y);

                // Draw preview items with cyan color to distinguish from confirmed selection
                let preview_color = RasterColor::new(100, 220, 255); // Cyan for preview

                // Create a draw function for preview with different color
                let draw_preview_selection = |fb: &mut Framebuffer, selection: &Selection| {
                    match selection {
                        Selection::SectorFace { room, x, z, face } => {
                            if let Some(room_data) = state.level.rooms.get(*room) {
                                if let Some(sector) = room_data.get_sector(*x, *z) {
                                    let base_x = room_data.position.x + (*x as f32) * SECTOR_SIZE;
                                    let base_z = room_data.position.z + (*z as f32) * SECTOR_SIZE;
                                    let room_y = room_data.position.y;

                                    match face {
                                        SectorFace::Floor => {
                                            if let Some(floor) = &sector.floor {
                                                let corners = [
                                                    Vec3::new(base_x, room_y + floor.heights[0], base_z),
                                                    Vec3::new(base_x + SECTOR_SIZE, room_y + floor.heights[1], base_z),
                                                    Vec3::new(base_x + SECTOR_SIZE, room_y + floor.heights[2], base_z + SECTOR_SIZE),
                                                    Vec3::new(base_x, room_y + floor.heights[3], base_z + SECTOR_SIZE),
                                                ];
                                                draw_3d_line(fb, corners[0], corners[1], &state.camera_3d, preview_color);
                                                draw_3d_line(fb, corners[1], corners[2], &state.camera_3d, preview_color);
                                                draw_3d_line(fb, corners[2], corners[3], &state.camera_3d, preview_color);
                                                draw_3d_line(fb, corners[3], corners[0], &state.camera_3d, preview_color);
                                            }
                                        }
                                        SectorFace::Ceiling => {
                                            if let Some(ceiling) = &sector.ceiling {
                                                let corners = [
                                                    Vec3::new(base_x, room_y + ceiling.heights[0], base_z),
                                                    Vec3::new(base_x + SECTOR_SIZE, room_y + ceiling.heights[1], base_z),
                                                    Vec3::new(base_x + SECTOR_SIZE, room_y + ceiling.heights[2], base_z + SECTOR_SIZE),
                                                    Vec3::new(base_x, room_y + ceiling.heights[3], base_z + SECTOR_SIZE),
                                                ];
                                                draw_3d_line(fb, corners[0], corners[1], &state.camera_3d, preview_color);
                                                draw_3d_line(fb, corners[1], corners[2], &state.camera_3d, preview_color);
                                                draw_3d_line(fb, corners[2], corners[3], &state.camera_3d, preview_color);
                                                draw_3d_line(fb, corners[3], corners[0], &state.camera_3d, preview_color);
                                            }
                                        }
                                        face if is_wall_face(face) => {
                                            // Get wall corners based on direction
                                            let (x0, z0, x1, z1) = match face {
                                                SectorFace::WallNorth(_) => (base_x, base_z, base_x + SECTOR_SIZE, base_z),
                                                SectorFace::WallSouth(_) => (base_x, base_z + SECTOR_SIZE, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE),
                                                SectorFace::WallEast(_) => (base_x + SECTOR_SIZE, base_z, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE),
                                                SectorFace::WallWest(_) => (base_x, base_z, base_x, base_z + SECTOR_SIZE),
                                                SectorFace::WallNwSe(_) => (base_x, base_z, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE),
                                                SectorFace::WallNeSw(_) => (base_x + SECTOR_SIZE, base_z, base_x, base_z + SECTOR_SIZE),
                                                _ => return,
                                            };
                                            let walls = match face {
                                                SectorFace::WallNorth(i) => sector.walls_north.get(*i),
                                                SectorFace::WallEast(i) => sector.walls_east.get(*i),
                                                SectorFace::WallSouth(i) => sector.walls_south.get(*i),
                                                SectorFace::WallWest(i) => sector.walls_west.get(*i),
                                                SectorFace::WallNwSe(i) => sector.walls_nwse.get(*i),
                                                SectorFace::WallNeSw(i) => sector.walls_nesw.get(*i),
                                                _ => None,
                                            };
                                            if let Some(wall) = walls {
                                                let corners = [
                                                    Vec3::new(x0, room_y + wall.heights[0], z0),
                                                    Vec3::new(x1, room_y + wall.heights[1], z1),
                                                    Vec3::new(x1, room_y + wall.heights[2], z1),
                                                    Vec3::new(x0, room_y + wall.heights[3], z0),
                                                ];
                                                draw_3d_line(fb, corners[0], corners[1], &state.camera_3d, preview_color);
                                                draw_3d_line(fb, corners[1], corners[2], &state.camera_3d, preview_color);
                                                draw_3d_line(fb, corners[2], corners[3], &state.camera_3d, preview_color);
                                                draw_3d_line(fb, corners[3], corners[0], &state.camera_3d, preview_color);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        Selection::Object { room, index } => {
                            if let Some(room_data) = state.level.rooms.get(*room) {
                                if let Some(obj) = room_data.objects.get(*index) {
                                    let world_pos = obj.world_position(room_data);
                                    // Draw a small cross at object position
                                    let size = 64.0;
                                    draw_3d_line(fb, world_pos - Vec3::new(size, 0.0, 0.0), world_pos + Vec3::new(size, 0.0, 0.0), &state.camera_3d, preview_color);
                                    draw_3d_line(fb, world_pos - Vec3::new(0.0, size, 0.0), world_pos + Vec3::new(0.0, size, 0.0), &state.camera_3d, preview_color);
                                    draw_3d_line(fb, world_pos - Vec3::new(0.0, 0.0, size), world_pos + Vec3::new(0.0, 0.0, size), &state.camera_3d, preview_color);
                                }
                            }
                        }
                        _ => {}
                    }
                };

                for sel in &preview_items {
                    draw_preview_selection(fb, sel);
                }
            }
        }
    }

    // Draw X/Z relocation preview
    if state.xz_drag_active && state.viewport_drag_started {
        let (dx, dz) = state.xz_drag_delta;
        let preview_color = RasterColor::new(100, 220, 255); // Cyan for valid
        let blocked_color = RasterColor::new(255, 100, 100); // Red for blocked

        for &(room_idx, gx, gz, ref face) in &state.xz_drag_initial_positions {
            // Keep as signed for world position calculation (allows negative/out-of-bounds preview)
            let dst_gx_signed = gx as i32 + dx;
            let dst_gz_signed = gz as i32 + dz;

            // Check if destination is blocked (only for valid in-bounds positions)
            let is_blocked = if dst_gx_signed >= 0 && dst_gz_signed >= 0 {
                is_destination_occupied(state, room_idx, dst_gx_signed as usize, dst_gz_signed as usize, face, &state.xz_drag_initial_positions)
            } else {
                false // Out of bounds positions are not blocked (will expand room)
            };
            let color = if is_blocked { blocked_color } else { preview_color };

            if let Some(room) = state.level.rooms.get(room_idx) {
                let room_y = room.position.y;
                // Use signed coordinates for world position - allows preview outside current bounds
                let dst_base_x = room.position.x + (dst_gx_signed as f32) * SECTOR_SIZE;
                let dst_base_z = room.position.z + (dst_gz_signed as f32) * SECTOR_SIZE;

                // Get face heights from original position
                if let Some(sector) = room.get_sector(gx, gz) {
                    let corners: Option<[Vec3; 4]> = match face {
                        SectorFace::Floor => sector.floor.as_ref().map(|f| [
                            Vec3::new(dst_base_x, room_y + f.heights[0], dst_base_z),
                            Vec3::new(dst_base_x + SECTOR_SIZE, room_y + f.heights[1], dst_base_z),
                            Vec3::new(dst_base_x + SECTOR_SIZE, room_y + f.heights[2], dst_base_z + SECTOR_SIZE),
                            Vec3::new(dst_base_x, room_y + f.heights[3], dst_base_z + SECTOR_SIZE),
                        ]),
                        SectorFace::Ceiling => sector.ceiling.as_ref().map(|c| [
                            Vec3::new(dst_base_x, room_y + c.heights[0], dst_base_z),
                            Vec3::new(dst_base_x + SECTOR_SIZE, room_y + c.heights[1], dst_base_z),
                            Vec3::new(dst_base_x + SECTOR_SIZE, room_y + c.heights[2], dst_base_z + SECTOR_SIZE),
                            Vec3::new(dst_base_x, room_y + c.heights[3], dst_base_z + SECTOR_SIZE),
                        ]),
                        _ => {
                            // Wall faces
                            let (x0, z0, x1, z1) = match face {
                                SectorFace::WallNorth(_) => (dst_base_x, dst_base_z, dst_base_x + SECTOR_SIZE, dst_base_z),
                                SectorFace::WallEast(_) => (dst_base_x + SECTOR_SIZE, dst_base_z, dst_base_x + SECTOR_SIZE, dst_base_z + SECTOR_SIZE),
                                SectorFace::WallSouth(_) => (dst_base_x + SECTOR_SIZE, dst_base_z + SECTOR_SIZE, dst_base_x, dst_base_z + SECTOR_SIZE),
                                SectorFace::WallWest(_) => (dst_base_x, dst_base_z + SECTOR_SIZE, dst_base_x, dst_base_z),
                                SectorFace::WallNwSe(_) => (dst_base_x, dst_base_z, dst_base_x + SECTOR_SIZE, dst_base_z + SECTOR_SIZE),
                                SectorFace::WallNeSw(_) => (dst_base_x + SECTOR_SIZE, dst_base_z, dst_base_x, dst_base_z + SECTOR_SIZE),
                                _ => (0.0, 0.0, 0.0, 0.0),
                            };
                            let wall_heights = match face {
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
                        }
                    };

                    if let Some(c) = corners {
                        draw_3d_line(fb, c[0], c[1], &state.camera_3d, color);
                        draw_3d_line(fb, c[1], c[2], &state.camera_3d, color);
                        draw_3d_line(fb, c[2], c[3], &state.camera_3d, color);
                        draw_3d_line(fb, c[3], c[0], &state.camera_3d, color);
                    }
                }
            }
        }
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

    let vp_preview_ms = EditorFrameTimings::elapsed_ms(preview_start);

    // === UPLOAD PHASE ===
    let upload_start = EditorFrameTimings::start();

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

    let vp_upload_ms = EditorFrameTimings::elapsed_ms(upload_start);

    // Store viewport timings
    state.frame_timings.vp_input_ms = vp_input_ms;
    state.frame_timings.vp_clear_ms = vp_clear_ms;
    state.frame_timings.vp_grid_ms = vp_grid_ms;
    state.frame_timings.vp_lights_ms = vp_lights_ms;
    state.frame_timings.vp_texconv_ms = vp_texconv_ms;
    state.frame_timings.vp_meshgen_ms = vp_meshgen_ms;
    state.frame_timings.vp_raster_ms = vp_raster_ms;
    state.frame_timings.vp_preview_ms = vp_preview_ms;
    state.frame_timings.vp_selection_ms = 0.0; // Included in preview
    state.frame_timings.vp_upload_ms = vp_upload_ms;

    // Draw viewport border
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, Color::from_rgba(60, 60, 60, 255));

    // Draw box select rectangle (if active)
    if state.box_selecting {
        if let (Some((fb_x0, fb_y0)), Some((fb_x1, fb_y1))) = (state.selection_rect_start, state.selection_rect_end) {
            // Convert framebuffer coords back to screen coords
            let screen_x0 = fb_x0 / fb_width as f32 * draw_w + draw_x;
            let screen_y0 = fb_y0 / fb_height as f32 * draw_h + draw_y;
            let screen_x1 = fb_x1 / fb_width as f32 * draw_w + draw_x;
            let screen_y1 = fb_y1 / fb_height as f32 * draw_h + draw_y;

            let rect_x = screen_x0.min(screen_x1);
            let rect_y = screen_y0.min(screen_y1);
            let rect_w = (screen_x1 - screen_x0).abs();
            let rect_h = (screen_y1 - screen_y0).abs();

            // Only draw if it's a meaningful size
            if rect_w > 2.0 || rect_h > 2.0 {
                // Semi-transparent fill
                draw_rectangle(rect_x, rect_y, rect_w, rect_h, Color::from_rgba(100, 180, 255, 50));

                // Outline
                let outline_color = Color::from_rgba(100, 180, 255, 200);
                draw_line(rect_x, rect_y, rect_x + rect_w, rect_y, 1.0, outline_color);
                draw_line(rect_x + rect_w, rect_y, rect_x + rect_w, rect_y + rect_h, 1.0, outline_color);
                draw_line(rect_x + rect_w, rect_y + rect_h, rect_x, rect_y + rect_h, 1.0, outline_color);
                draw_line(rect_x, rect_y + rect_h, rect_x, rect_y, 1.0, outline_color);
            }
        }
    }

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

        // Clip line to screen bounds using Cohen-Sutherland algorithm
        // This prevents Bresenham from iterating over thousands of off-screen pixels
        let w = fb.width as f32;
        let h = fb.height as f32;

        let Some((cx0, cy0, cx1, cy1)) = clip_line_to_rect(x0f, y0f, x1f, y1f, 0.0, 0.0, w, h) else {
            return; // Line entirely outside viewport
        };

        // Convert to integers for Bresenham
        let mut x0 = cx0 as i32;
        let mut y0 = cy0 as i32;
        let x1 = cx1 as i32;
        let y1 = cy1 as i32;

        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;

        let wi = fb.width as i32;
        let hi = fb.height as i32;

        loop {
            // Safety check (should be redundant after clipping, but just in case)
            if x0 >= 0 && x0 < wi && y0 >= 0 && y0 < hi {
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

/// Cohen-Sutherland line clipping to a rectangle.
/// Returns None if line is entirely outside, otherwise returns clipped endpoints.
fn clip_line_to_rect(
    mut x0: f32, mut y0: f32,
    mut x1: f32, mut y1: f32,
    xmin: f32, ymin: f32,
    xmax: f32, ymax: f32,
) -> Option<(f32, f32, f32, f32)> {
    // Outcode bits
    const INSIDE: u8 = 0;
    const LEFT: u8 = 1;
    const RIGHT: u8 = 2;
    const BOTTOM: u8 = 4;
    const TOP: u8 = 8;

    let outcode = |x: f32, y: f32| -> u8 {
        let mut code = INSIDE;
        if x < xmin { code |= LEFT; }
        else if x >= xmax { code |= RIGHT; }
        if y < ymin { code |= TOP; }
        else if y >= ymax { code |= BOTTOM; }
        code
    };

    let mut code0 = outcode(x0, y0);
    let mut code1 = outcode(x1, y1);

    // Guard against infinite loops from floating-point edge cases
    for _ in 0..16 {
        if (code0 | code1) == 0 {
            // Both inside
            return Some((x0, y0, x1, y1));
        }
        if (code0 & code1) != 0 {
            // Both outside same edge - reject
            return None;
        }

        // Pick the point outside
        let code_out = if code0 != 0 { code0 } else { code1 };

        // Find intersection
        let (x, y);
        if (code_out & BOTTOM) != 0 {
            x = x0 + (x1 - x0) * (ymax - 1.0 - y0) / (y1 - y0);
            y = ymax - 1.0;
        } else if (code_out & TOP) != 0 {
            x = x0 + (x1 - x0) * (ymin - y0) / (y1 - y0);
            y = ymin;
        } else if (code_out & RIGHT) != 0 {
            y = y0 + (y1 - y0) * (xmax - 1.0 - x0) / (x1 - x0);
            x = xmax - 1.0;
        } else {
            // LEFT
            y = y0 + (y1 - y0) * (xmin - x0) / (x1 - x0);
            x = xmin;
        }

        if code_out == code0 {
            x0 = x;
            y0 = y;
            code0 = outcode(x0, y0);
        } else {
            x1 = x;
            y1 = y;
            code1 = outcode(x1, y1);
        }
    }

    // Failed to converge (floating-point edge case) - reject the line
    None
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

/// Calculate the average Y position of all selected faces (for X/Z drag plane)
fn calculate_selection_center_y(state: &EditorState) -> f32 {
    let mut total_y = 0.0;
    let mut count = 0;

    // Helper to get face center Y
    let get_face_y = |room_idx: usize, gx: usize, gz: usize, face: &SectorFace| -> Option<f32> {
        let room = state.level.rooms.get(room_idx)?;
        let sector = room.get_sector(gx, gz)?;
        let room_y = room.position.y;

        match face {
            SectorFace::Floor => {
                sector.floor.as_ref().map(|f| {
                    room_y + (f.heights[0] + f.heights[1] + f.heights[2] + f.heights[3]) / 4.0
                })
            }
            SectorFace::Ceiling => {
                sector.ceiling.as_ref().map(|c| {
                    room_y + (c.heights[0] + c.heights[1] + c.heights[2] + c.heights[3]) / 4.0
                })
            }
            SectorFace::WallNorth(i) => sector.walls_north.get(*i).map(|w| {
                room_y + (w.heights[0] + w.heights[1] + w.heights[2] + w.heights[3]) / 4.0
            }),
            SectorFace::WallEast(i) => sector.walls_east.get(*i).map(|w| {
                room_y + (w.heights[0] + w.heights[1] + w.heights[2] + w.heights[3]) / 4.0
            }),
            SectorFace::WallSouth(i) => sector.walls_south.get(*i).map(|w| {
                room_y + (w.heights[0] + w.heights[1] + w.heights[2] + w.heights[3]) / 4.0
            }),
            SectorFace::WallWest(i) => sector.walls_west.get(*i).map(|w| {
                room_y + (w.heights[0] + w.heights[1] + w.heights[2] + w.heights[3]) / 4.0
            }),
            SectorFace::WallNwSe(i) => sector.walls_nwse.get(*i).map(|w| {
                room_y + (w.heights[0] + w.heights[1] + w.heights[2] + w.heights[3]) / 4.0
            }),
            SectorFace::WallNeSw(i) => sector.walls_nesw.get(*i).map(|w| {
                room_y + (w.heights[0] + w.heights[1] + w.heights[2] + w.heights[3]) / 4.0
            }),
        }
    };

    // Check primary selection
    if let Selection::SectorFace { room, x, z, face } = &state.selection {
        if let Some(y) = get_face_y(*room, *x, *z, face) {
            total_y += y;
            count += 1;
        }
    }

    // Check multi-selection
    for sel in &state.multi_selection {
        if let Selection::SectorFace { room, x, z, face } = sel {
            if let Some(y) = get_face_y(*room, *x, *z, face) {
                total_y += y;
                count += 1;
            }
        }
    }

    if count > 0 {
        total_y / count as f32
    } else {
        0.0
    }
}

/// Face data extracted for relocation
#[derive(Clone)]
enum FaceData {
    Floor(crate::world::HorizontalFace),
    Ceiling(crate::world::HorizontalFace),
    WallNorth(crate::world::VerticalFace),
    WallEast(crate::world::VerticalFace),
    WallSouth(crate::world::VerticalFace),
    WallWest(crate::world::VerticalFace),
    WallNwSe(crate::world::VerticalFace),
    WallNeSw(crate::world::VerticalFace),
}

/// Relocate faces by dx/dz grid units. Returns (count moved, total_offset_x, total_offset_z).
/// The total offset includes room expansion offset + movement delta.
/// Returns (moved_count, total_dx, total_dz, trim_x, trim_z)
/// - moved_count: number of faces actually moved
/// - total_dx/dz: total offset including room expansion + movement delta
/// - trim_x/z: how many columns/rows were trimmed from the start after compacting
fn relocate_faces(
    state: &mut EditorState,
    faces: &[(usize, usize, usize, SectorFace)],
    dx: i32, dz: i32,
) -> (usize, i32, i32, usize, usize) {
    use std::collections::HashSet;

    if faces.is_empty() || (dx == 0 && dz == 0) {
        return (0, 0, 0, 0, 0);
    }

    // Phase 1: Calculate destination coordinates and check bounds
    // We need to potentially expand the room for negative destinations
    let mut min_dst_gx = i32::MAX;
    let mut min_dst_gz = i32::MAX;
    let mut max_dst_gx = i32::MIN;
    let mut max_dst_gz = i32::MIN;

    for &(_, gx, gz, _) in faces {
        let dst_gx = gx as i32 + dx;
        let dst_gz = gz as i32 + dz;
        min_dst_gx = min_dst_gx.min(dst_gx);
        min_dst_gz = min_dst_gz.min(dst_gz);
        max_dst_gx = max_dst_gx.max(dst_gx);
        max_dst_gz = max_dst_gz.max(dst_gz);
    }

    // Get the room index (assume all faces are in the same room for now)
    let room_idx = faces.first().map(|(r, _, _, _)| *r).unwrap_or(0);

    // Phase 2: Expand room if needed
    let mut offset_x = 0i32;
    let mut offset_z = 0i32;

    if let Some(room) = state.level.rooms.get_mut(room_idx) {
        // Expand in negative X direction
        while min_dst_gx + offset_x < 0 {
            room.position.x -= SECTOR_SIZE;
            room.sectors.insert(0, (0..room.depth).map(|_| None).collect());
            room.width += 1;
            offset_x += 1;
            // Shift all objects to match new grid indices
            for obj in &mut room.objects {
                obj.sector_x += 1;
            }
        }

        // Expand in negative Z direction
        while min_dst_gz + offset_z < 0 {
            room.position.z -= SECTOR_SIZE;
            for col in &mut room.sectors {
                col.insert(0, None);
            }
            room.depth += 1;
            offset_z += 1;
            // Shift all objects to match new grid indices
            for obj in &mut room.objects {
                obj.sector_z += 1;
            }
        }

        // Expand in positive X direction
        while (max_dst_gx + offset_x) as usize >= room.width {
            room.width += 1;
            room.sectors.push((0..room.depth).map(|_| None).collect());
        }

        // Expand in positive Z direction
        while (max_dst_gz + offset_z) as usize >= room.depth {
            room.depth += 1;
            for col in &mut room.sectors {
                col.push(None);
            }
        }
    }

    // Adjust face coordinates for the expansion offset
    let adjusted_faces: Vec<(usize, usize, usize, SectorFace)> = faces.iter().map(|&(r, gx, gz, ref face)| {
        (r, (gx as i32 + offset_x) as usize, (gz as i32 + offset_z) as usize, face.clone())
    }).collect();

    // Phase 3: Check which faces can actually move (destination not occupied by non-moving faces)
    let movable: Vec<(usize, usize, usize, SectorFace)> = adjusted_faces.iter().filter(|(room_idx, gx, gz, face)| {
        let dst_gx = (*gx as i32 + dx) as usize;
        let dst_gz = (*gz as i32 + dz) as usize;
        // Pass adjusted_faces as vacating positions - faces in this list will vacate, so don't count them as blocking
        !is_destination_occupied(state, *room_idx, dst_gx, dst_gz, face, &adjusted_faces)
    }).cloned().collect();

    if movable.is_empty() {
        return (0, offset_x + dx, offset_z + dz, 0, 0);
    }

    // Phase 4: Extract face data from sources
    let face_data: Vec<Option<FaceData>> = movable.iter().map(|(room_idx, gx, gz, face)| {
        extract_face_data(&state.level, *room_idx, *gx, *gz, face)
    }).collect();

    // Phase 5: Delete from sources
    for (room_idx, gx, gz, face) in &movable {
        delete_face(&mut state.level, *room_idx, *gx, *gz, face.clone());
    }

    // Phase 6: Create at destinations
    let mut moved_count = 0;
    for (i, (room_idx, gx, gz, _face)) in movable.iter().enumerate() {
        let dst_gx = (*gx as i32 + dx) as usize;
        let dst_gz = (*gz as i32 + dz) as usize;
        if let Some(data) = &face_data[i] {
            create_face(&mut state.level, *room_idx, dst_gx, dst_gz, data);
            moved_count += 1;
        }
    }

    // Phase 7: Cleanup
    let affected_rooms: HashSet<usize> = movable.iter().map(|(r, _, _, _)| *r).collect();
    let mut trim_x = 0usize;
    let mut trim_z = 0usize;
    for room_idx in affected_rooms {
        if let Some(room) = state.level.rooms.get_mut(room_idx) {
            let (tx, tz) = room.compact();
            // Accumulate trim offsets (in practice there's usually only one room)
            trim_x = trim_x.max(tx);
            trim_z = trim_z.max(tz);
        }
    }
    state.mark_portals_dirty();

    // Return count, total offset (room expansion + movement delta), and trim offset
    (moved_count, offset_x + dx, offset_z + dz, trim_x, trim_z)
}

/// Check if destination has a conflicting face
/// `vacating_positions` is the list of positions being moved - faces at these positions don't count as blocking
fn is_destination_occupied(
    state: &EditorState,
    room_idx: usize,
    gx: usize,
    gz: usize,
    face: &SectorFace,
    vacating_positions: &[(usize, usize, usize, SectorFace)],
) -> bool {
    if let Some(room) = state.level.rooms.get(room_idx) {
        if let Some(sector) = room.get_sector(gx, gz) {
            let has_face = match face {
                SectorFace::Floor => sector.floor.is_some(),
                SectorFace::Ceiling => sector.ceiling.is_some(),
                SectorFace::WallNorth(_) => !sector.walls_north.is_empty(),
                SectorFace::WallEast(_) => !sector.walls_east.is_empty(),
                SectorFace::WallSouth(_) => !sector.walls_south.is_empty(),
                SectorFace::WallWest(_) => !sector.walls_west.is_empty(),
                SectorFace::WallNwSe(_) => !sector.walls_nwse.is_empty(),
                SectorFace::WallNeSw(_) => !sector.walls_nesw.is_empty(),
            };

            if !has_face {
                return false;
            }

            // Check if the face at this position is being vacated (part of the selection being moved)
            let is_being_vacated = vacating_positions.iter().any(|(r, x, z, f)| {
                *r == room_idx && *x == gx && *z == gz && std::mem::discriminant(f) == std::mem::discriminant(face)
            });

            // Only blocked if there's a face AND it's not being vacated
            return !is_being_vacated;
        }
    }
    false
}

/// Extract face data from a sector
fn extract_face_data(level: &crate::world::Level, room_idx: usize, gx: usize, gz: usize, face: &SectorFace) -> Option<FaceData> {
    let room = level.rooms.get(room_idx)?;
    let sector = room.get_sector(gx, gz)?;

    match face {
        SectorFace::Floor => sector.floor.clone().map(FaceData::Floor),
        SectorFace::Ceiling => sector.ceiling.clone().map(FaceData::Ceiling),
        SectorFace::WallNorth(i) => sector.walls_north.get(*i).cloned().map(FaceData::WallNorth),
        SectorFace::WallEast(i) => sector.walls_east.get(*i).cloned().map(FaceData::WallEast),
        SectorFace::WallSouth(i) => sector.walls_south.get(*i).cloned().map(FaceData::WallSouth),
        SectorFace::WallWest(i) => sector.walls_west.get(*i).cloned().map(FaceData::WallWest),
        SectorFace::WallNwSe(i) => sector.walls_nwse.get(*i).cloned().map(FaceData::WallNwSe),
        SectorFace::WallNeSw(i) => sector.walls_nesw.get(*i).cloned().map(FaceData::WallNeSw),
    }
}

/// Create a face at a destination sector
fn create_face(level: &mut crate::world::Level, room_idx: usize, gx: usize, gz: usize, data: &FaceData) {
    if let Some(room) = level.rooms.get_mut(room_idx) {
        room.ensure_sector(gx, gz);
        if let Some(sector) = room.get_sector_mut(gx, gz) {
            match data {
                FaceData::Floor(f) => { sector.floor = Some(f.clone()); }
                FaceData::Ceiling(c) => { sector.ceiling = Some(c.clone()); }
                FaceData::WallNorth(w) => { sector.walls_north.push(w.clone()); }
                FaceData::WallEast(w) => { sector.walls_east.push(w.clone()); }
                FaceData::WallSouth(w) => { sector.walls_south.push(w.clone()); }
                FaceData::WallWest(w) => { sector.walls_west.push(w.clone()); }
                FaceData::WallNwSe(w) => { sector.walls_nwse.push(w.clone()); }
                FaceData::WallNeSw(w) => { sector.walls_nesw.push(w.clone()); }
            }
        }
    }
}

/// Update selection positions after relocation
fn update_selection_positions(
    state: &mut EditorState,
    original_faces: &[(usize, usize, usize, SectorFace)],
    dx: i32, dz: i32,
) {
    // Build a lookup of original positions that were moved
    let moved_positions: std::collections::HashSet<(usize, usize, usize)> = original_faces.iter()
        .map(|(r, x, z, _)| (*r, *x, *z))
        .collect();

    // Update primary selection
    if let Selection::SectorFace { room, x, z, face } = &state.selection {
        if moved_positions.contains(&(*room, *x, *z)) {
            let new_x = (*x as i32 + dx) as usize;
            let new_z = (*z as i32 + dz) as usize;
            // For walls, the index may have changed - use index 0 for the new wall
            let new_face = match face {
                SectorFace::WallNorth(_) => SectorFace::WallNorth(0),
                SectorFace::WallEast(_) => SectorFace::WallEast(0),
                SectorFace::WallSouth(_) => SectorFace::WallSouth(0),
                SectorFace::WallWest(_) => SectorFace::WallWest(0),
                SectorFace::WallNwSe(_) => SectorFace::WallNwSe(0),
                SectorFace::WallNeSw(_) => SectorFace::WallNeSw(0),
                _ => *face,
            };
            state.selection = Selection::SectorFace { room: *room, x: new_x, z: new_z, face: new_face };
        }
    }

    // Update multi-selection
    for sel in &mut state.multi_selection {
        if let Selection::SectorFace { room, x, z, face } = sel {
            if moved_positions.contains(&(*room, *x, *z)) {
                let new_x = (*x as i32 + dx) as usize;
                let new_z = (*z as i32 + dz) as usize;
                let new_face = match face {
                    SectorFace::WallNorth(_) => SectorFace::WallNorth(0),
                    SectorFace::WallEast(_) => SectorFace::WallEast(0),
                    SectorFace::WallSouth(_) => SectorFace::WallSouth(0),
                    SectorFace::WallWest(_) => SectorFace::WallWest(0),
                    SectorFace::WallNwSe(_) => SectorFace::WallNwSe(0),
                    SectorFace::WallNeSw(_) => SectorFace::WallNeSw(0),
                    _ => *face,
                };
                *sel = Selection::SectorFace { room: *room, x: new_x, z: new_z, face: new_face };
            }
        }
    }
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

/// Find all faces and objects within a screen-space rectangle (for box select)
/// Returns a Vec of selections without modifying state
fn find_selections_in_rect(
    state: &EditorState,
    fb: &Framebuffer,
    rect_min_x: f32, rect_min_y: f32,
    rect_max_x: f32, rect_max_y: f32,
) -> Vec<Selection> {
    let room_idx = state.current_room;
    let Some(room) = state.level.rooms.get(room_idx) else { return Vec::new() };

    let cam = &state.camera_3d;
    let mut collected: Vec<Selection> = Vec::new();

    // Check all sectors for faces
    for x in 0..room.width {
        for z in 0..room.depth {
            let Some(sector) = room.get_sector(x, z) else { continue };

            // World position of sector corners
            let base_x = room.position.x + (x as f32) * SECTOR_SIZE;
            let base_z = room.position.z + (z as f32) * SECTOR_SIZE;

            // Check floor
            if let Some(floor) = &sector.floor {
                if face_center_in_rect(room.position.y, base_x, base_z, floor.heights, cam, fb, rect_min_x, rect_min_y, rect_max_x, rect_max_y) {
                    collected.push(Selection::SectorFace { room: room_idx, x, z, face: SectorFace::Floor });
                }
            }

            // Check ceiling
            if let Some(ceiling) = &sector.ceiling {
                if face_center_in_rect(room.position.y, base_x, base_z, ceiling.heights, cam, fb, rect_min_x, rect_min_y, rect_max_x, rect_max_y) {
                    collected.push(Selection::SectorFace { room: room_idx, x, z, face: SectorFace::Ceiling });
                }
            }

            // Check cardinal walls
            for (i, wall) in sector.walls_north.iter().enumerate() {
                if wall_center_in_rect(room.position.y, base_x, base_z, crate::world::Direction::North, wall.heights, cam, fb, rect_min_x, rect_min_y, rect_max_x, rect_max_y) {
                    collected.push(Selection::SectorFace { room: room_idx, x, z, face: SectorFace::WallNorth(i) });
                }
            }
            for (i, wall) in sector.walls_east.iter().enumerate() {
                if wall_center_in_rect(room.position.y, base_x, base_z, crate::world::Direction::East, wall.heights, cam, fb, rect_min_x, rect_min_y, rect_max_x, rect_max_y) {
                    collected.push(Selection::SectorFace { room: room_idx, x, z, face: SectorFace::WallEast(i) });
                }
            }
            for (i, wall) in sector.walls_south.iter().enumerate() {
                if wall_center_in_rect(room.position.y, base_x, base_z, crate::world::Direction::South, wall.heights, cam, fb, rect_min_x, rect_min_y, rect_max_x, rect_max_y) {
                    collected.push(Selection::SectorFace { room: room_idx, x, z, face: SectorFace::WallSouth(i) });
                }
            }
            for (i, wall) in sector.walls_west.iter().enumerate() {
                if wall_center_in_rect(room.position.y, base_x, base_z, crate::world::Direction::West, wall.heights, cam, fb, rect_min_x, rect_min_y, rect_max_x, rect_max_y) {
                    collected.push(Selection::SectorFace { room: room_idx, x, z, face: SectorFace::WallWest(i) });
                }
            }

            // Check diagonal walls
            for (i, wall) in sector.walls_nwse.iter().enumerate() {
                if wall_center_in_rect(room.position.y, base_x, base_z, crate::world::Direction::NwSe, wall.heights, cam, fb, rect_min_x, rect_min_y, rect_max_x, rect_max_y) {
                    collected.push(Selection::SectorFace { room: room_idx, x, z, face: SectorFace::WallNwSe(i) });
                }
            }
            for (i, wall) in sector.walls_nesw.iter().enumerate() {
                if wall_center_in_rect(room.position.y, base_x, base_z, crate::world::Direction::NeSw, wall.heights, cam, fb, rect_min_x, rect_min_y, rect_max_x, rect_max_y) {
                    collected.push(Selection::SectorFace { room: room_idx, x, z, face: SectorFace::WallNeSw(i) });
                }
            }
        }
    }

    // Check objects
    for (i, obj) in room.objects.iter().enumerate() {
        let world_pos = obj.world_position(room);
        if let Some((sx, sy)) = world_to_screen(world_pos, cam.position, cam.basis_x, cam.basis_y, cam.basis_z, fb.width, fb.height) {
            if sx >= rect_min_x && sx <= rect_max_x && sy >= rect_min_y && sy <= rect_max_y {
                collected.push(Selection::Object { room: room_idx, index: i });
            }
        }
    }

    collected
}

/// Check if the center of a floor/ceiling face projects into the screen rectangle
fn face_center_in_rect(
    room_y: f32,
    base_x: f32, base_z: f32,
    heights: [f32; 4],
    cam: &crate::rasterizer::Camera, fb: &Framebuffer,
    rect_min_x: f32, rect_min_y: f32,
    rect_max_x: f32, rect_max_y: f32,
) -> bool {
    // Calculate center position (average of all 4 corners)
    let avg_height = (heights[0] + heights[1] + heights[2] + heights[3]) / 4.0;
    let center = Vec3::new(
        base_x + SECTOR_SIZE / 2.0,
        room_y + avg_height,
        base_z + SECTOR_SIZE / 2.0,
    );

    if let Some((sx, sy)) = world_to_screen(center, cam.position, cam.basis_x, cam.basis_y, cam.basis_z, fb.width, fb.height) {
        sx >= rect_min_x && sx <= rect_max_x && sy >= rect_min_y && sy <= rect_max_y
    } else {
        false
    }
}

/// Check if the center of a wall face projects into the screen rectangle
fn wall_center_in_rect(
    room_y: f32,
    base_x: f32, base_z: f32,
    direction: crate::world::Direction,
    heights: [f32; 4],
    cam: &crate::rasterizer::Camera, fb: &Framebuffer,
    rect_min_x: f32, rect_min_y: f32,
    rect_max_x: f32, rect_max_y: f32,
) -> bool {
    use crate::world::Direction;

    // Get wall edge positions based on direction
    let (x0, z0, x1, z1) = match direction {
        Direction::North => (base_x, base_z, base_x + SECTOR_SIZE, base_z),
        Direction::South => (base_x, base_z + SECTOR_SIZE, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE),
        Direction::East => (base_x + SECTOR_SIZE, base_z, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE),
        Direction::West => (base_x, base_z, base_x, base_z + SECTOR_SIZE),
        Direction::NwSe => (base_x, base_z, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE),
        Direction::NeSw => (base_x + SECTOR_SIZE, base_z, base_x, base_z + SECTOR_SIZE),
    };

    // Calculate center position
    let avg_height = (heights[0] + heights[1] + heights[2] + heights[3]) / 4.0;
    let center = Vec3::new(
        (x0 + x1) / 2.0,
        room_y + avg_height,
        (z0 + z1) / 2.0,
    );

    if let Some((sx, sy)) = world_to_screen(center, cam.position, cam.basis_x, cam.basis_y, cam.basis_z, fb.width, fb.height) {
        sx >= rect_min_x && sx <= rect_max_x && sy >= rect_min_y && sy <= rect_max_y
    } else {
        false
    }
}
