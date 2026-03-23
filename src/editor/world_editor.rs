//! World editor — 3D viewport (primary) + tools, ported from v1 editor/viewport_3d.rs.
//! Primary view: 3D orbit camera with face picking (right-drag to orbit, scroll to zoom).
//! Secondary view: 2D top-down grid (toggled via toolbar icon).

use egui::{Color32, Painter, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use crate::rasterizer::{Camera, Color, RasterSettings, ShadingMode, Vec3};
use crate::rasterizer::constants::{WIDTH, HEIGHT};
use crate::scene::{
    AssetInstance, Direction, Level, Room, Sector, HorizontalFace, SectorFace,
    VerticalFace, TextureRef, SECTOR_SIZE,
};
use crate::app::{AppState, EditorAction, Selection};
use super::level_edit::LevelEditState;
use super::icons::{icon, icon_button};
use super::radial_menu::{RadialItem, WheelOut, WheelSession};
use super::theme;
use super::viewport::ViewportPanel;

// ---------------------------------------------------------------------------
// Tool & view state
// ---------------------------------------------------------------------------

/// Clipboard for face texture copy-paste (Ctrl+C / Ctrl+V in Select mode).
#[derive(Clone)]
pub struct FaceClipboard {
    pub texture: TextureRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorTool {
    Select,
    DrawFloor,
    DrawWall,
    DrawCeiling,
    PlaceObject,
    Erase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GridViewMode {
    #[default]
    Top,   // X-Z plane (looking down Y axis)
    Front, // X-Y plane
    Side,  // Z-Y plane
}

pub struct WorldEditorPanel {
    pub tool: EditorTool,
    pub current_room: usize,

    // ---- 3D orbit camera (primary view) ----
    pub camera: Camera,
    orbit_azimuth: f32,
    orbit_elevation: f32,
    orbit_distance: f32,
    orbit_target: Vec3,

    // 3D hover / selection  (room_idx, gx, gz, face)
    pub hovered_face: Option<(usize, usize, usize, SectorFace)>,
    pub selected_face: Option<(usize, usize, usize, SectorFace)>,

    // ---- View toggle ----
    pub show_3d_view: bool,

    // ---- 2D grid view (secondary) ----
    pub view_mode: GridViewMode,
    pub grid_zoom: f64,
    pub grid_offset: Vec2,
    pub show_grid: bool,
    pub grid_size: f32,

    // 2D selection (kept for grid view and tool fallback)
    pub hovered_sector: Option<(usize, usize)>,
    pub selected_sector: Option<(usize, usize)>,
    pub hovered_edge: Option<Direction>,

    // ---- Shared state ----
    pub hidden_rooms: std::collections::HashSet<usize>,
    pub floor_height: f32,
    pub ceiling_height: f32,
    pub selected_texture: TextureRef,

    /// Set to true after any tool mutates the level; cleared by Editor after rebuilding the cache.
    pub needs_viewport_rebuild: bool,

    /// Wall direction for DrawWall tool (cycled with R key)
    pub wall_direction: Direction,
    /// Hovered grid position via ray-plane intersection (works in empty space)
    hovered_placement_sector: Option<(i32, i32)>,
    /// True while a drag-placement is in progress (prevents duplicate undo pushes)
    drag_placing: bool,
    /// Timed status message for the status bar (text + expiry instant)
    status_message: Option<(String, std::time::Instant)>,
    /// Draw bounding box wireframe around each room
    pub show_room_bounds: bool,

    /// PS1 render settings — toggled via the render bar above the viewport
    pub raster_settings: RasterSettings,

    /// Face clipboard for Ctrl+C / Ctrl+V texture copy-paste
    pub face_clipboard: Option<FaceClipboard>,

    /// Active tool wheel session (opened with E key).
    tool_wheel: Option<WheelSession<EditorTool>>,
}

impl WorldEditorPanel {
    pub fn new() -> Self {
        let mut panel = Self {
            tool: EditorTool::DrawFloor,
            current_room: 0,

            camera: Camera::new(),
            orbit_azimuth: 0.8,
            orbit_elevation: 0.4,
            orbit_distance: 4000.0,
            orbit_target: Vec3::new(2.0 * SECTOR_SIZE, 0.0, 2.0 * SECTOR_SIZE),

            hovered_face: None,
            selected_face: None,

            show_3d_view: true,

            view_mode: GridViewMode::Top,
            grid_zoom: 0.1,
            grid_offset: Vec2::ZERO,
            show_grid: true,
            grid_size: SECTOR_SIZE,

            hovered_sector: None,
            selected_sector: None,
            hovered_edge: None,

            hidden_rooms: std::collections::HashSet::new(),
            floor_height: 0.0,
            ceiling_height: 3.0,
            selected_texture: TextureRef::new("_DEFAULT", "checkerboard"),

            needs_viewport_rebuild: false,

            wall_direction: Direction::North,
            hovered_placement_sector: None,
            drag_placing: false,
            status_message: None,
            show_room_bounds: true,

            raster_settings: RasterSettings::default(),
            face_clipboard: None,
            tool_wheel: None,
        };
        panel.sync_camera_from_orbit();
        panel
    }

    fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some((msg.into(), std::time::Instant::now()));
    }

    // -----------------------------------------------------------------------
    // Orbit camera sync (ported from v1/src/editor/state.rs)
    // -----------------------------------------------------------------------

    /// Reposition the orbit camera to center on the given level's first room.
    /// Used for freshly-created levels where editor_layout hasn't been saved yet.
    pub fn center_on_level(&mut self, level: &Level) {
        if let Some(room) = level.rooms.first() {
            let cx = room.position.x + room.width as f32 * SECTOR_SIZE / 2.0;
            let cz = room.position.z + room.depth as f32 * SECTOR_SIZE / 2.0;
            self.orbit_target = Vec3::new(cx, 0.0, cz);
            self.sync_camera_from_orbit();
        }
    }

    /// Center the orbit camera on a specific room (called when switching rooms).
    pub fn center_on_room(&mut self, room_idx: usize, level: &Level) {
        if let Some(room) = level.rooms.get(room_idx) {
            let cx = room.position.x + room.width as f32 * SECTOR_SIZE / 2.0;
            let cz = room.position.z + room.depth as f32 * SECTOR_SIZE / 2.0;
            // Preserve Y so the view height doesn't jump
            self.orbit_target = Vec3::new(cx, self.orbit_target.y, cz);
            self.sync_camera_from_orbit();
        }
    }

    /// Center the orbit camera on the selected face's centroid.
    pub fn center_camera_on_selection(&mut self, level: Option<&Level>) {
        let Some((room_idx, gx, gz, face)) = self.selected_face else { return };
        let Some(level) = level else { return };
        let Some(room) = level.rooms.get(room_idx) else { return };
        if let Some(corners) = face_corners(room, gx, gz, face) {
            let center = Vec3::new(
                (corners[0].x + corners[1].x + corners[2].x + corners[3].x) / 4.0,
                (corners[0].y + corners[1].y + corners[2].y + corners[3].y) / 4.0,
                (corners[0].z + corners[1].z + corners[2].z + corners[3].z) / 4.0,
            );
            self.orbit_target = center;
            self.sync_camera_from_orbit();
        }
    }

    /// Restore camera and grid state from the level's persisted editor layout.
    /// Called when opening an existing level so the view resumes where the user left off.
    pub fn restore_editor_layout(&mut self, level: &Level) {
        let l = &level.editor_layout;
        self.orbit_target = Vec3::new(l.orbit_target_x, l.orbit_target_y, l.orbit_target_z);
        self.orbit_distance = l.orbit_distance;
        self.orbit_azimuth = l.orbit_azimuth;
        self.orbit_elevation = l.orbit_elevation;
        self.grid_offset = Vec2::new(l.grid_offset_x, l.grid_offset_y);
        self.grid_zoom = l.grid_zoom as f64;
        self.sync_camera_from_orbit();
    }

    /// Write the current camera and grid state back into the level so it survives save/load.
    /// Call this immediately before serialising the level.
    pub fn save_editor_layout(&self, level: &mut Level) {
        let l = &mut level.editor_layout;
        l.orbit_target_x = self.orbit_target.x;
        l.orbit_target_y = self.orbit_target.y;
        l.orbit_target_z = self.orbit_target.z;
        l.orbit_distance = self.orbit_distance;
        l.orbit_azimuth = self.orbit_azimuth;
        l.orbit_elevation = self.orbit_elevation;
        l.grid_offset_x = self.grid_offset.x;
        l.grid_offset_y = self.grid_offset.y;
        l.grid_zoom = self.grid_zoom as f32;
    }

    pub fn sync_camera_from_orbit(&mut self) {
        let pitch = self.orbit_elevation;
        let yaw = self.orbit_azimuth;

        let forward = Vec3::new(
            pitch.cos() * yaw.sin(),
            -pitch.sin(),
            pitch.cos() * yaw.cos(),
        );

        self.camera.position = self.orbit_target - forward * self.orbit_distance;
        self.camera.rotation_x = pitch;
        self.camera.rotation_y = yaw;
        self.camera.update_basis();
    }

    // -----------------------------------------------------------------------
    // 3D render — writes to viewport framebuffer
    // Called from Editor::render_3d() in WorldEditor mode.
    // -----------------------------------------------------------------------
    pub fn render_frame(&mut self, viewport: &mut ViewportPanel, level: Option<&Level>) {
        use crate::rasterizer::render_mesh;
        use crate::rasterizer::draw::{draw_floor_grid, draw_3d_line_clipped};

        viewport.framebuffer.clear(Color::new(30, 30, 50));

        // Position grid at the room's lowest floor so it aligns with the geometry
        let grid_y = level.and_then(|l| l.rooms.get(self.current_room)).map(|room| {
            let room_y = room.position.y;
            room.iter_sectors()
                .filter_map(|(_, _, s)| s.floor.as_ref().map(|f| room_y + f.avg_height()))
                .fold(f32::INFINITY, f32::min)
                .min(room_y)  // clamp to room origin if no floors
        }).unwrap_or(0.0).min(1e30); // avoid INFINITY if no level

        draw_floor_grid(
            &mut viewport.framebuffer,
            &self.camera,
            if grid_y.is_finite() { grid_y } else { 0.0 },
            SECTOR_SIZE,
            8.0 * SECTOR_SIZE,
            Color::new(45, 45, 55),
            Color::new(100, 40, 40),
            Color::new(40, 40, 100),
        );

        if let Some(cached) = &viewport.cached_render {
            render_mesh(
                &mut viewport.framebuffer,
                &cached.vertices,
                &cached.faces,
                &cached.textures,
                &self.camera,
                &self.raster_settings,
            );
        }

        // Wireframes drawn after geometry so they appear on top
        if let Some(level) = level {
            if let Some((room_idx, gx, gz, face)) = self.hovered_face {
                if let Some(room) = level.rooms.get(room_idx) {
                    if let Some(corners) = face_corners(room, gx, gz, face) {
                        let teal = Color::new(0, 200, 180);
                        draw_3d_line_clipped(&mut viewport.framebuffer, &self.camera, corners[0], corners[1], teal);
                        draw_3d_line_clipped(&mut viewport.framebuffer, &self.camera, corners[1], corners[2], teal);
                        draw_3d_line_clipped(&mut viewport.framebuffer, &self.camera, corners[2], corners[3], teal);
                        draw_3d_line_clipped(&mut viewport.framebuffer, &self.camera, corners[3], corners[0], teal);
                        // Cross diagonal in Select mode (V1 visual cue)
                        if self.tool == EditorTool::Select {
                            draw_3d_line_clipped(&mut viewport.framebuffer, &self.camera, corners[0], corners[2], teal);
                        }
                    }
                }
            }
            if let Some((room_idx, gx, gz, face)) = self.selected_face {
                if let Some(room) = level.rooms.get(room_idx) {
                    if let Some(corners) = face_corners(room, gx, gz, face) {
                        let yellow = Color::new(255, 200, 80);
                        draw_3d_line_clipped(&mut viewport.framebuffer, &self.camera, corners[0], corners[1], yellow);
                        draw_3d_line_clipped(&mut viewport.framebuffer, &self.camera, corners[1], corners[2], yellow);
                        draw_3d_line_clipped(&mut viewport.framebuffer, &self.camera, corners[2], corners[3], yellow);
                        draw_3d_line_clipped(&mut viewport.framebuffer, &self.camera, corners[3], corners[0], yellow);
                    }
                }
            }

            // Placement preview — show where the active draw tool will place geometry
            if let Some((pi_gx, pi_gz)) = self.hovered_placement_sector {
                if let Some(room) = level.rooms.get(self.current_room) {
                    let room_y = room.position.y;
                    let bx = room.position.x + pi_gx as f32 * SECTOR_SIZE;
                    let bz = room.position.z + pi_gz as f32 * SECTOR_SIZE;

                    match self.tool {
                        EditorTool::DrawFloor => {
                            let y = room_y + self.floor_height;
                            let c = [
                                Vec3::new(bx,              y, bz),
                                Vec3::new(bx + SECTOR_SIZE, y, bz),
                                Vec3::new(bx + SECTOR_SIZE, y, bz + SECTOR_SIZE),
                                Vec3::new(bx,              y, bz + SECTOR_SIZE),
                            ];
                            let col = Color::new(0, 200, 160); // teal
                            draw_3d_line_clipped(&mut viewport.framebuffer, &self.camera, c[0], c[1], col);
                            draw_3d_line_clipped(&mut viewport.framebuffer, &self.camera, c[1], c[2], col);
                            draw_3d_line_clipped(&mut viewport.framebuffer, &self.camera, c[2], c[3], col);
                            draw_3d_line_clipped(&mut viewport.framebuffer, &self.camera, c[3], c[0], col);
                        }
                        EditorTool::DrawCeiling => {
                            let y = room_y + self.ceiling_height;
                            let c = [
                                Vec3::new(bx,              y, bz),
                                Vec3::new(bx + SECTOR_SIZE, y, bz),
                                Vec3::new(bx + SECTOR_SIZE, y, bz + SECTOR_SIZE),
                                Vec3::new(bx,              y, bz + SECTOR_SIZE),
                            ];
                            let col = Color::new(140, 80, 220); // purple
                            draw_3d_line_clipped(&mut viewport.framebuffer, &self.camera, c[0], c[1], col);
                            draw_3d_line_clipped(&mut viewport.framebuffer, &self.camera, c[1], c[2], col);
                            draw_3d_line_clipped(&mut viewport.framebuffer, &self.camera, c[2], c[3], col);
                            draw_3d_line_clipped(&mut viewport.framebuffer, &self.camera, c[3], c[0], col);
                        }
                        EditorTool::DrawWall => {
                            // Preview the wall edge on the current wall_direction
                            let y_bot = room_y + self.floor_height;
                            let y_top = room_y + self.ceiling_height;
                            let c = wall_direction_corners(bx, bz, y_bot, y_top, self.wall_direction);
                            let col = Color::new(220, 180, 60); // amber
                            draw_3d_line_clipped(&mut viewport.framebuffer, &self.camera, c[0], c[1], col);
                            draw_3d_line_clipped(&mut viewport.framebuffer, &self.camera, c[1], c[2], col);
                            draw_3d_line_clipped(&mut viewport.framebuffer, &self.camera, c[2], c[3], col);
                            draw_3d_line_clipped(&mut viewport.framebuffer, &self.camera, c[3], c[0], col);
                        }
                        _ => {}
                    }
                }
            }

            // Room bounding box wireframes
            if self.show_room_bounds {
                for (room_idx, room) in level.rooms.iter().enumerate() {
                    if self.hidden_rooms.contains(&room_idx) { continue; }
                    let is_current = room_idx == self.current_room;
                    let box_color = if is_current {
                        Color::new(80, 120, 200)
                    } else {
                        Color::new(55, 55, 70)
                    };
                    let min_x = room.position.x;
                    let min_z = room.position.z;
                    let max_x = room.position.x + room.width as f32 * SECTOR_SIZE;
                    let max_z = room.position.z + room.depth as f32 * SECTOR_SIZE;
                    let (mut min_y, mut max_y) = (f32::INFINITY, f32::NEG_INFINITY);
                    for (_, _, s) in room.iter_sectors() {
                        if let Some(f) = &s.floor {
                            min_y = min_y.min(room.position.y + f.avg_height());
                        }
                        if let Some(c) = &s.ceiling {
                            max_y = max_y.max(room.position.y + c.avg_height());
                        }
                    }
                    if !min_y.is_finite() { min_y = room.position.y; }
                    if !max_y.is_finite() { max_y = room.position.y + self.ceiling_height; }
                    let c = [
                        Vec3::new(min_x, min_y, min_z),
                        Vec3::new(max_x, min_y, min_z),
                        Vec3::new(max_x, min_y, max_z),
                        Vec3::new(min_x, min_y, max_z),
                        Vec3::new(min_x, max_y, min_z),
                        Vec3::new(max_x, max_y, min_z),
                        Vec3::new(max_x, max_y, max_z),
                        Vec3::new(min_x, max_y, max_z),
                    ];
                    let edges = [(0,1),(1,2),(2,3),(3,0),(4,5),(5,6),(6,7),(7,4),(0,4),(1,5),(2,6),(3,7)];
                    for (i, j) in edges {
                        draw_3d_line_clipped(&mut viewport.framebuffer, &self.camera, c[i], c[j], box_color);
                    }
                }
            }

            // Asset instance gizmos — colored circles in the framebuffer
            {
                use crate::rasterizer::world_to_screen;
                for (room_idx, room) in level.rooms.iter().enumerate() {
                    if self.hidden_rooms.contains(&room_idx) { continue; }
                    for (obj_idx, obj) in room.objects.iter().enumerate() {
                        let world_pos = obj.world_position(room);
                        if let Some((fb_x, fb_y)) = world_to_screen(
                            world_pos,
                            self.camera.position,
                            self.camera.basis_x,
                            self.camera.basis_y,
                            self.camera.basis_z,
                            WIDTH,
                            HEIGHT,
                        ) {
                            let is_selected = self.selected_face.is_none()
                                && self.selected_sector == Some((obj.sector_x, obj.sector_z));
                            let radius = if is_selected { 6 } else { 4 };
                            // Color by index seed (no asset type info yet in v2)
                            let hue_idx = obj_idx % 6;
                            let obj_color = match hue_idx {
                                0 => Color::new(180, 120, 255), // purple (generic)
                                1 => Color::new(255, 255, 100), // yellow (lights)
                                2 => Color::new(100, 255, 100), // green (spawn)
                                3 => Color::new(255, 100, 100), // red (enemy)
                                4 => Color::new(100, 200, 255), // cyan (trigger)
                                _ => Color::new(200, 200, 200), // white (other)
                            };
                            if is_selected {
                                viewport.framebuffer.draw_circle(fb_x as i32, fb_y as i32, radius + 2, Color::new(255, 255, 255));
                            }
                            viewport.framebuffer.draw_circle(fb_x as i32, fb_y as i32, radius, obj_color);
                        }
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Top: PS1 render settings bar
    // -----------------------------------------------------------------------
    fn draw_render_bar(&mut self, ctx: &egui::Context) {
        use super::icons::icon_text;
        egui::TopBottomPanel::top("we_render_bar")
            .resizable(false)
            .exact_height(28.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let s = &mut self.raster_settings;
                    let sz = theme::ICON_SIZE_SM;

                    // Wireframe overlay
                    let active = s.wireframe_overlay;
                    let resp = ui.add(egui::Button::new(icon_text(icon::GRID, sz))
                        .frame(active)
                        .selected(active))
                        .on_hover_text("Wireframe overlay");
                    if resp.clicked() { s.wireframe_overlay = !s.wireframe_overlay; }

                    ui.separator();

                    // Backface: cycles cull → cull+wire → show all
                    let (bc_icon, bc_tip) = if !s.backface_cull {
                        (icon::EYE, "Backface: show all (click to cull)")
                    } else if s.backface_wireframe {
                        (icon::SCAN, "Backface: wireframe (click to hide)")
                    } else {
                        (icon::EYE_OFF, "Backface: culled (click to wire)")
                    };
                    if ui.add(egui::Button::new(icon_text(bc_icon, sz)).frame(false))
                        .on_hover_text(bc_tip).clicked()
                    {
                        if !s.backface_cull {
                            // show → cull+wire
                            s.backface_cull = true; s.backface_wireframe = true;
                        } else if s.backface_wireframe {
                            // cull+wire → cull only
                            s.backface_wireframe = false;
                        } else {
                            // cull only → show all
                            s.backface_cull = false;
                        }
                    }

                    ui.separator();

                    // Affine textures
                    let active = s.affine_textures;
                    if ui.add(egui::Button::new(icon_text(icon::WAVES, sz))
                        .frame(active).selected(active))
                        .on_hover_text("Affine textures (PS1 warping)").clicked()
                    { s.affine_textures = !s.affine_textures; }

                    // Fixed-point math
                    let active = s.use_fixed_point;
                    if ui.add(egui::Button::new(icon_text(icon::HASH, sz))
                        .frame(active).selected(active))
                        .on_hover_text("Fixed-point math (PS1 jitter)").clicked()
                    { s.use_fixed_point = !s.use_fixed_point; }

                    // Gouraud / Flat shading
                    let gouraud = s.shading == ShadingMode::Gouraud;
                    if ui.add(egui::Button::new(icon_text(icon::SUN, sz))
                        .frame(gouraud).selected(gouraud))
                        .on_hover_text(if gouraud { "Gouraud shading (click for flat)" } else { "Flat shading (click for Gouraud)" }).clicked()
                    {
                        s.shading = if gouraud { ShadingMode::Flat } else { ShadingMode::Gouraud };
                    }

                    ui.separator();

                    // Low resolution
                    let active = s.low_resolution;
                    if ui.add(egui::Button::new(icon_text(icon::MONITOR, sz))
                        .frame(active).selected(active))
                        .on_hover_text("Low resolution (PS1 pixelated)").clicked()
                    { s.low_resolution = !s.low_resolution; }

                    // Dithering
                    let active = s.dithering;
                    if ui.add(egui::Button::new(icon_text(icon::BLEND, sz))
                        .frame(active).selected(active))
                        .on_hover_text("Dithering (ordered 4x4 Bayer)").clicked()
                    { s.dithering = !s.dithering; }

                    // Aspect ratio / stretch
                    let active = s.stretch_to_fill;
                    if ui.add(egui::Button::new(icon_text(icon::PROPORTIONS, sz))
                        .frame(active).selected(active))
                        .on_hover_text("Stretch to fill (off = 4:3 letterbox)").clicked()
                    { s.stretch_to_fill = !s.stretch_to_fill; }

                    ui.separator();

                    // Z-buffer
                    let active = s.use_zbuffer;
                    if ui.add(egui::Button::new(icon_text(icon::ARROW_DOWN_UP, sz))
                        .frame(active).selected(active))
                        .on_hover_text("Z-buffer depth test").clicked()
                    { s.use_zbuffer = !s.use_zbuffer; }

                    // RGB555
                    let active = s.use_rgb555;
                    if ui.add(egui::Button::new(icon_text(icon::PALETTE, sz))
                        .frame(active).selected(active))
                        .on_hover_text("RGB555 color mode (15-bit PS1)").clicked()
                    { s.use_rgb555 = !s.use_rgb555; }

                    ui.separator();

                    // 3D / 2D view toggle
                    let is_3d = self.show_3d_view;
                    if ui.add(egui::Button::new(icon_text(icon::ROTATE_3D, sz))
                        .frame(is_3d).selected(is_3d))
                        .on_hover_text("Toggle 3D / 2D view").clicked()
                    { self.show_3d_view = !self.show_3d_view; }

                    // Room bounds (3D mode only)
                    if self.show_3d_view {
                        let active = self.show_room_bounds;
                        if ui.add(egui::Button::new(icon_text(icon::BOX, sz))
                            .frame(active).selected(active))
                            .on_hover_text("Room bounds (B)").clicked()
                        { self.show_room_bounds = !self.show_room_bounds; }
                    }

                    // Grid toggle (2D mode only)
                    if !self.show_3d_view {
                        let active = self.show_grid;
                        if ui.add(egui::Button::new(icon_text(icon::GRID, sz))
                            .frame(active).selected(active))
                            .on_hover_text("Toggle grid").clicked()
                        { self.show_grid = !self.show_grid; }
                    }
                });
            });
    }

    // -----------------------------------------------------------------------
    // Left: room list panel
    // -----------------------------------------------------------------------
    fn draw_room_panel(&mut self, ctx: &egui::Context, level: &mut LevelEditState, state: &mut AppState) {
        egui::SidePanel::left("we_rooms")
            .resizable(true)
            .default_width(160.0)
            .min_width(100.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("Rooms");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_button(ui, icon::PLUS, theme::ICON_SIZE_SM, "Add room") {
                            state.request_action(EditorAction::AddRoom);
                        }
                    });
                });
                ui.separator();

                let room_count = level.current_level.as_ref().map(|l| l.rooms.len()).unwrap_or(0);
                if room_count == 0 {
                    ui.weak("No level loaded.\nUse Level > New Level.");
                    return;
                }

                for i in 0..room_count {
                    let hidden = self.hidden_rooms.contains(&i);
                    let current = self.current_room == i;

                    ui.horizontal(|ui| {
                        let eye_ic = if hidden { icon::EYE_OFF } else { icon::EYE };
                        if icon_button(ui, eye_ic, theme::ICON_SIZE_SM, "Toggle visibility") {
                            if hidden {
                                self.hidden_rooms.remove(&i);
                            } else {
                                self.hidden_rooms.insert(i);
                            }
                        }
                        let label = egui::RichText::new(format!("Room {}", i))
                            .color(if current { theme::ACCENT_HOVER } else { theme::TEXT });
                        if ui.selectable_label(current, label).clicked() {
                            self.current_room = i;
                            state.select(Selection::Room(i));
                            if let Some(level) = level.current_level.as_ref() {
                                self.center_on_room(i, level);
                            }
                        }
                    });
                }

                // 2D view mode selector (only shown in 2D mode)
                if !self.show_3d_view {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.label(egui::RichText::new("2D View").small().color(theme::TEXT_DIM));
                    ui.horizontal(|ui| {
                        for (mode, label) in [
                            (GridViewMode::Top,   "Top"),
                            (GridViewMode::Front, "Frt"),
                            (GridViewMode::Side,  "Sid"),
                        ] {
                            if ui.selectable_label(self.view_mode == mode, label).clicked() {
                                self.view_mode = mode;
                            }
                        }
                    });
                }
            });
    }

    // -----------------------------------------------------------------------
    // Right: face / sector properties panel
    // -----------------------------------------------------------------------
    fn draw_properties_panel(&mut self, ctx: &egui::Context, level: &mut LevelEditState, state: &mut AppState) {
        egui::SidePanel::right("we_props")
            .resizable(true)
            .default_width(220.0)
            .show(ctx, |ui| {
                ui.strong("Properties");
                ui.separator();

                // Brush settings
                ui.label(egui::RichText::new("Brush").small().color(theme::TEXT_DIM));
                ui.horizontal(|ui| {
                    ui.label("Floor Y:");
                    ui.add(egui::DragValue::new(&mut self.floor_height).speed(0.1));
                });
                ui.horizontal(|ui| {
                    ui.label("Ceil  Y:");
                    ui.add(egui::DragValue::new(&mut self.ceiling_height).speed(0.1));
                });
                ui.horizontal(|ui| {
                    ui.label("Pack: ");
                    ui.add(egui::TextEdit::singleline(&mut self.selected_texture.pack)
                        .desired_width(80.0)
                        .hint_text("_DEFAULT"));
                });
                ui.horizontal(|ui| {
                    ui.label("Tex:  ");
                    ui.add(egui::TextEdit::singleline(&mut self.selected_texture.name)
                        .desired_width(80.0)
                        .hint_text("checkerboard"));
                });
                // Wall direction control (only shown when DrawWall is active)
                if self.tool == EditorTool::DrawWall {
                    ui.horizontal(|ui| {
                        ui.label("Dir:  ");
                        ui.label(egui::RichText::new(self.wall_direction.name()).color(theme::ACCENT));
                        if ui.small_button("R").on_hover_text("Rotate direction (R key)").clicked() {
                            self.wall_direction = self.wall_direction.rotate_cw();
                        }
                    });
                }
                ui.separator();

                if self.show_3d_view {
                    self.draw_3d_face_properties(ui, level, state);
                } else {
                    self.draw_2d_sector_properties(ui, level, state);
                }

                // Objects list
                ui.label(egui::RichText::new("Objects").small().color(theme::TEXT_DIM));
                let room_idx = self.current_room;
                let obj_count = level.current_level.as_ref()
                    .and_then(|l| l.rooms.get(room_idx))
                    .map(|r| r.objects.len())
                    .unwrap_or(0);

                let mut remove_obj: Option<usize> = None;
                egui::ScrollArea::vertical()
                    .id_salt("we_objects")
                    .max_height(160.0)
                    .show(ui, |ui| {
                        for i in 0..obj_count {
                            if let Some(obj) = level.current_level.as_ref()
                                .and_then(|l| l.rooms.get(room_idx))
                                .and_then(|r| r.objects.get(i))
                            {
                                let label = if obj.name.is_empty() {
                                    format!("[{}] Sector ({},{})", i, obj.sector_x, obj.sector_z)
                                } else {
                                    format!("[{}] {}", i, obj.name)
                                };
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(&label).small());
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if icon_button(ui, icon::TRASH, theme::ICON_SIZE_SM, "Remove object") {
                                            remove_obj = Some(i);
                                        }
                                    });
                                });
                            }
                        }
                    });

                if let Some(idx) = remove_obj {
                    level.push_level_undo();
                    if let Some(room) = level.current_level.as_mut()
                        .and_then(|l| l.rooms.get_mut(room_idx))
                    {
                        if idx < room.objects.len() {
                            room.objects.remove(idx);
                            self.needs_viewport_rebuild = true;
                        }
                    }
                }

                ui.separator();
                if ui.button("Save Level").clicked() {
                    state.request_action(EditorAction::SaveLevel);
                }
            });
    }

    /// Properties panel content for 3D selected face
    fn draw_3d_face_properties(&mut self, ui: &mut egui::Ui, level: &mut LevelEditState, _state: &mut AppState) {
        if let Some((room_idx, gx, gz, face)) = self.selected_face {
            ui.label(egui::RichText::new(format!("Sector ({}, {})", gx, gz)).strong());
            ui.label(format!("Room: {}  Face: {:?}", room_idx, face));

            if let Some(room) = level.current_level.as_ref().and_then(|l| l.rooms.get(room_idx)) {
                if let Some(sector) = room.get_sector(gx, gz) {
                    match face {
                        SectorFace::Floor => {
                            if let Some(f) = &sector.floor {
                                ui.label(format!("Floor Y: {:.2}", f.avg_height()));
                            }
                        }
                        SectorFace::Ceiling => {
                            if let Some(c) = &sector.ceiling {
                                ui.label(format!("Ceil Y: {:.2}", c.avg_height()));
                            }
                        }
                        wall_face => {
                            let (dir, widx) = match wall_face {
                                SectorFace::WallNorth(i) => (Direction::North, i),
                                SectorFace::WallEast(i)  => (Direction::East,  i),
                                SectorFace::WallSouth(i) => (Direction::South, i),
                                SectorFace::WallWest(i)  => (Direction::West,  i),
                                SectorFace::WallNwSe(i)  => (Direction::NwSe,  i),
                                SectorFace::WallNeSw(i)  => (Direction::NeSw,  i),
                                _ => unreachable!(),
                            };
                            ui.label(format!("{} wall #{}", dir.name(), widx));
                            if let Some(w) = sector.walls(dir).get(widx) {
                                ui.label(format!("Bot Y: {:.0}  Top Y: {:.0}", w.y_bottom(), w.y_top()));
                            }
                        }
                    }
                }
            }

            ui.separator();
            ui.label(egui::RichText::new("Edit face").small().color(theme::TEXT_DIM));

            match face {
                SectorFace::Floor => {
                    let cur_fy = level.current_level.as_ref()
                        .and_then(|l| l.rooms.get(room_idx))
                        .and_then(|r| r.get_sector(gx, gz))
                        .and_then(|s| s.floor.as_ref())
                        .map(|f| f.avg_height());

                    if let Some(mut fy) = cur_fy {
                        let before = fy;
                        let drag_resp = ui.horizontal(|ui| {
                            ui.label("Floor Y:");
                            ui.add(egui::DragValue::new(&mut fy).speed(0.05))
                        }).inner;
                        if drag_resp.drag_started() {
                            level.push_level_undo();
                        }
                        if (fy - before).abs() > 1e-5 {
                            if let Some(s) = level.current_level.as_mut()
                                .and_then(|l| l.rooms.get_mut(room_idx))
                                .and_then(|r| r.sectors.get_mut(gx))
                                .and_then(|col| col.get_mut(gz))
                                .and_then(|s| s.as_mut())
                            {
                                if let Some(floor) = &mut s.floor { floor.heights = [fy; 4]; }
                                self.needs_viewport_rebuild = true;
                            }
                        }
                    }
                    // Texture fields
                    let tex = level.current_level.as_ref()
                        .and_then(|l| l.rooms.get(room_idx))
                        .and_then(|r| r.get_sector(gx, gz))
                        .and_then(|s| s.floor.as_ref())
                        .map(|f| f.texture.clone());
                    if let Some(mut t) = tex {
                        let before = t.clone();
                        ui.horizontal(|ui| {
                            ui.label("Pack:");
                            ui.add(egui::TextEdit::singleline(&mut t.pack).desired_width(80.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Tex: ");
                            ui.add(egui::TextEdit::singleline(&mut t.name).desired_width(80.0));
                        });
                        if t.pack != before.pack || t.name != before.name {
                            level.push_level_undo();
                            if let Some(s) = level.current_level.as_mut()
                                .and_then(|l| l.rooms.get_mut(room_idx))
                                .and_then(|r| r.sectors.get_mut(gx))
                                .and_then(|col| col.get_mut(gz))
                                .and_then(|s| s.as_mut())
                            {
                                if let Some(floor) = &mut s.floor { floor.texture = t; }
                                self.needs_viewport_rebuild = true;
                            }
                        }
                    }
                }
                SectorFace::Ceiling => {
                    let cur_cy = level.current_level.as_ref()
                        .and_then(|l| l.rooms.get(room_idx))
                        .and_then(|r| r.get_sector(gx, gz))
                        .and_then(|s| s.ceiling.as_ref())
                        .map(|c| c.avg_height());

                    if let Some(mut cy) = cur_cy {
                        let before = cy;
                        let drag_resp = ui.horizontal(|ui| {
                            ui.label("Ceil  Y:");
                            ui.add(egui::DragValue::new(&mut cy).speed(0.05))
                        }).inner;
                        if drag_resp.drag_started() {
                            level.push_level_undo();
                        }
                        if (cy - before).abs() > 1e-5 {
                            if let Some(s) = level.current_level.as_mut()
                                .and_then(|l| l.rooms.get_mut(room_idx))
                                .and_then(|r| r.sectors.get_mut(gx))
                                .and_then(|col| col.get_mut(gz))
                                .and_then(|s| s.as_mut())
                            {
                                if let Some(ceil) = &mut s.ceiling { ceil.heights = [cy; 4]; }
                                self.needs_viewport_rebuild = true;
                            }
                        }
                    }
                    // Texture fields
                    let tex = level.current_level.as_ref()
                        .and_then(|l| l.rooms.get(room_idx))
                        .and_then(|r| r.get_sector(gx, gz))
                        .and_then(|s| s.ceiling.as_ref())
                        .map(|c| c.texture.clone());
                    if let Some(mut t) = tex {
                        let before = t.clone();
                        ui.horizontal(|ui| {
                            ui.label("Pack:");
                            ui.add(egui::TextEdit::singleline(&mut t.pack).desired_width(80.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Tex: ");
                            ui.add(egui::TextEdit::singleline(&mut t.name).desired_width(80.0));
                        });
                        if t.pack != before.pack || t.name != before.name {
                            level.push_level_undo();
                            if let Some(s) = level.current_level.as_mut()
                                .and_then(|l| l.rooms.get_mut(room_idx))
                                .and_then(|r| r.sectors.get_mut(gx))
                                .and_then(|col| col.get_mut(gz))
                                .and_then(|s| s.as_mut())
                            {
                                if let Some(ceil) = &mut s.ceiling { ceil.texture = t; }
                                self.needs_viewport_rebuild = true;
                            }
                        }
                    }
                }
                wall_face => {
                    // Wall: show editable top/bottom Y + texture
                    let (dir, widx) = match wall_face {
                        SectorFace::WallNorth(i) => (Direction::North, i),
                        SectorFace::WallEast(i)  => (Direction::East,  i),
                        SectorFace::WallSouth(i) => (Direction::South, i),
                        SectorFace::WallWest(i)  => (Direction::West,  i),
                        SectorFace::WallNwSe(i)  => (Direction::NwSe,  i),
                        SectorFace::WallNeSw(i)  => (Direction::NeSw,  i),
                        _ => unreachable!(),
                    };
                    let cur_heights = level.current_level.as_ref()
                        .and_then(|l| l.rooms.get(room_idx))
                        .and_then(|r| r.get_sector(gx, gz))
                        .and_then(|s| s.walls(dir).get(widx))
                        .map(|w| (w.y_bottom(), w.y_top()));

                    if let Some((mut bot, mut top)) = cur_heights {
                        let before = (bot, top);
                        let drag_bot = ui.horizontal(|ui| {
                            ui.label("Bot Y:");
                            ui.add(egui::DragValue::new(&mut bot).speed(10.0))
                        }).inner;
                        let drag_top = ui.horizontal(|ui| {
                            ui.label("Top Y:");
                            ui.add(egui::DragValue::new(&mut top).speed(10.0))
                        }).inner;
                        if drag_bot.drag_started() || drag_top.drag_started() {
                            level.push_level_undo();
                        }
                        if (bot - before.0).abs() > 0.5 || (top - before.1).abs() > 0.5 {
                            if let Some(s) = level.current_level.as_mut()
                                .and_then(|l| l.rooms.get_mut(room_idx))
                                .and_then(|r| r.sectors.get_mut(gx))
                                .and_then(|col| col.get_mut(gz))
                                .and_then(|s| s.as_mut())
                            {
                                if let Some(w) = s.walls_mut(dir).get_mut(widx) {
                                    w.heights[0] = bot; w.heights[1] = bot;
                                    w.heights[2] = top; w.heights[3] = top;
                                }
                                self.needs_viewport_rebuild = true;
                            }
                        }
                    }
                    // Wall texture
                    let tex = level.current_level.as_ref()
                        .and_then(|l| l.rooms.get(room_idx))
                        .and_then(|r| r.get_sector(gx, gz))
                        .and_then(|s| s.walls(dir).get(widx))
                        .map(|w| w.texture.clone());
                    if let Some(mut t) = tex {
                        let before = t.clone();
                        ui.horizontal(|ui| {
                            ui.label("Pack:");
                            ui.add(egui::TextEdit::singleline(&mut t.pack).desired_width(80.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Tex: ");
                            ui.add(egui::TextEdit::singleline(&mut t.name).desired_width(80.0));
                        });
                        if t.pack != before.pack || t.name != before.name {
                            level.push_level_undo();
                            if let Some(s) = level.current_level.as_mut()
                                .and_then(|l| l.rooms.get_mut(room_idx))
                                .and_then(|r| r.sectors.get_mut(gx))
                                .and_then(|col| col.get_mut(gz))
                                .and_then(|s| s.as_mut())
                            {
                                if let Some(w) = s.walls_mut(dir).get_mut(widx) {
                                    w.texture = t;
                                }
                                self.needs_viewport_rebuild = true;
                            }
                        }
                    }
                }
            }

            ui.separator();
        } else {
            ui.weak("Click a face to select it");
            ui.separator();
        }
    }

    /// Properties panel content for 2D selected sector
    fn draw_2d_sector_properties(&mut self, ui: &mut egui::Ui, level: &mut LevelEditState, _state: &mut AppState) {
        if let Some((gx, gz)) = self.selected_sector {
            ui.label(egui::RichText::new(format!("Sector ({}, {})", gx, gz)).strong());

            if let Some(room) = level.current_level.as_ref()
                .and_then(|l| l.rooms.get(self.current_room))
            {
                if let Some(sector) = room.get_sector(gx, gz) {
                    if let Some(floor) = &sector.floor {
                        ui.label(format!("Floor Y: {:.2}", floor.avg_height()));
                    } else {
                        ui.weak("No floor");
                    }
                    if let Some(ceil) = &sector.ceiling {
                        ui.label(format!("Ceiling Y: {:.2}", ceil.avg_height()));
                    } else {
                        ui.weak("No ceiling");
                    }
                    let wall_count = sector.walls_north.len() + sector.walls_east.len()
                        + sector.walls_south.len() + sector.walls_west.len();
                    ui.label(format!("Walls: {}", wall_count));
                    let obj_count = room.objects.iter()
                        .filter(|o| o.sector_x == gx && o.sector_z == gz)
                        .count();
                    ui.label(format!("Objects: {}", obj_count));
                } else {
                    ui.weak("Empty sector");
                }
            }

            ui.separator();
            ui.label(egui::RichText::new("Edit sector").small().color(theme::TEXT_DIM));

            let (cur_floor, cur_ceil) = level.current_level.as_ref()
                .and_then(|l| l.rooms.get(self.current_room))
                .and_then(|r| r.get_sector(gx, gz))
                .map(|s| (
                    s.floor.as_ref().map(|f| f.avg_height()),
                    s.ceiling.as_ref().map(|c| c.avg_height()),
                ))
                .unwrap_or((None, None));

            if let Some(mut fy) = cur_floor {
                let before = fy;
                let drag_resp = ui.horizontal(|ui| {
                    ui.label("Floor Y:");
                    ui.add(egui::DragValue::new(&mut fy).speed(0.05))
                }).inner;
                if drag_resp.drag_started() {
                    level.push_level_undo();
                }
                if (fy - before).abs() > 1e-5 {
                    if let Some(s) = level.current_level.as_mut()
                        .and_then(|l| l.rooms.get_mut(self.current_room))
                        .and_then(|r| r.sectors.get_mut(gx))
                        .and_then(|col| col.get_mut(gz))
                        .and_then(|s| s.as_mut())
                    {
                        if let Some(floor) = &mut s.floor { floor.heights = [fy; 4]; }
                        self.needs_viewport_rebuild = true;
                    }
                }
            }
            if let Some(mut cy) = cur_ceil {
                let before = cy;
                let drag_resp = ui.horizontal(|ui| {
                    ui.label("Ceil  Y:");
                    ui.add(egui::DragValue::new(&mut cy).speed(0.05))
                }).inner;
                if drag_resp.drag_started() {
                    level.push_level_undo();
                }
                if (cy - before).abs() > 1e-5 {
                    if let Some(s) = level.current_level.as_mut()
                        .and_then(|l| l.rooms.get_mut(self.current_room))
                        .and_then(|r| r.sectors.get_mut(gx))
                        .and_then(|col| col.get_mut(gz))
                        .and_then(|s| s.as_mut())
                    {
                        if let Some(ceil) = &mut s.ceiling { ceil.heights = [cy; 4]; }
                        self.needs_viewport_rebuild = true;
                    }
                }
            }

            ui.separator();
        } else {
            ui.weak("Click a sector to select it");
            ui.separator();
        }
    }

    // -----------------------------------------------------------------------
    // Center: 3D viewport interaction area (transparent — 3D rendered behind)
    // -----------------------------------------------------------------------
    pub fn draw_panels(&mut self, ctx: &egui::Context, level: &mut LevelEditState, state: &mut AppState) {
        self.draw_render_bar(ctx);
        self.draw_room_panel(ctx, level, state);
        self.draw_properties_panel(ctx, level, state);
    }

    pub fn draw_central_inside(&mut self, ui: &mut egui::Ui, level: &mut LevelEditState, state: &mut AppState) {
        if self.show_3d_view {
            egui::CentralPanel::default()
                .frame(egui::Frame::none())
                .show_inside(ui, |ui| {
                    self.draw_3d_content(ui, level, state);
                });
        } else {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(theme::GRID_BG))
                .show_inside(ui, |ui| {
                    self.draw_grid_content(ui, level, state);
                });
        }

        // Tool wheel floats above the viewport
        self.tick_tool_wheel(ui.ctx());
    }

    fn tool_wheel_items() -> Vec<RadialItem<EditorTool>> {
        vec![
            RadialItem::new(icon::POINTER,    "Select",  EditorTool::Select),
            RadialItem::new(icon::LAYERS,     "Floor",   EditorTool::DrawFloor),
            RadialItem::new(icon::BRICK_WALL, "Wall",    EditorTool::DrawWall),
            RadialItem::new(icon::BOX,        "Ceiling", EditorTool::DrawCeiling),
            RadialItem::new(icon::MAP_PIN,    "Object",  EditorTool::PlaceObject),
            RadialItem::new(icon::ERASER,     "Erase",   EditorTool::Erase),
        ]
    }

    fn tick_tool_wheel(&mut self, ctx: &egui::Context) {
        // E key opens the tool wheel (only when no text input is focused)
        if !ctx.wants_keyboard_input() {
            let e_pressed = ctx.input(|i| i.key_pressed(egui::Key::E));
            if e_pressed && self.tool_wheel.is_none() {
                self.tool_wheel = Some(WheelSession::open(ctx, "tool_wheel", Self::tool_wheel_items()));
            }
        }

        let Some(session) = self.tool_wheel.as_mut() else { return };
        match session.show(ctx) {
            WheelOut::Open => {}
            WheelOut::Dismissed => { self.tool_wheel = None; }
            WheelOut::Selected(tool) => {
                self.tool = tool;
                self.tool_wheel = None;
            }
        }
    }

    fn draw_3d_content(&mut self, ui: &mut egui::Ui, level: &mut LevelEditState, state: &mut AppState) {
        let ctx = ui.ctx().clone();
        {
                let available = ui.available_rect_before_wrap();
                let (rect, response) = ui.allocate_exact_size(
                    available.size(),
                    Sense::click_and_drag(),
                );

                let shift = ctx.input(|i| i.modifiers.shift);
                let fb_w = WIDTH as f32;
                let fb_h = HEIGHT as f32;

                // ---- Camera orbit (right-drag) ----
                if response.dragged_by(egui::PointerButton::Secondary) {
                    let delta = response.drag_delta();
                    if shift {
                        // Shift+right-drag: pan orbit target
                        let pan_speed = self.orbit_distance * 0.002;
                        let right = self.camera.basis_x;
                        let up = self.camera.basis_y;
                        self.orbit_target.x -= right.x * delta.x * pan_speed;
                        self.orbit_target.y -= right.y * delta.x * pan_speed;
                        self.orbit_target.z -= right.z * delta.x * pan_speed;
                        self.orbit_target.x += up.x * delta.y * pan_speed;
                        self.orbit_target.y += up.y * delta.y * pan_speed;
                        self.orbit_target.z += up.z * delta.y * pan_speed;
                    } else {
                        // Right-drag: rotate around target
                        self.orbit_azimuth += delta.x * 0.005;
                        self.orbit_elevation = (self.orbit_elevation + delta.y * 0.005)
                            .clamp(-1.4, 1.4);
                    }
                    self.sync_camera_from_orbit();
                }

                // ---- Zoom (scroll) ----
                let scroll = ui.input(|i| i.raw_scroll_delta.y);
                if scroll.abs() > 0.0 {
                    let factor = if scroll > 0.0 { 0.9 } else { 1.1 };
                    self.orbit_distance = (self.orbit_distance * factor).clamp(50.0, 20000.0);
                    self.sync_camera_from_orbit();
                }

                // ---- Mouse → framebuffer coordinates ----
                // The framebuffer is stretched to fill the FULL WINDOW by the wgpu fullscreen
                // quad, so we must normalize against the window rect, not the central panel rect.
                let mouse_fb = response.hover_pos().map(|pos| {
                    let screen = ctx.input(|i| i.screen_rect());
                    let t_x = (pos.x - screen.min.x) / screen.width();
                    let t_y = (pos.y - screen.min.y) / screen.height();
                    (t_x * fb_w, t_y * fb_h)
                });

                // ---- Face picking (existing geometry) ----
                if let (Some(mfb), Some(level)) = (mouse_fb, level.current_level.as_ref()) {
                    if !self.hidden_rooms.contains(&self.current_room) {
                        self.hovered_face = find_hovered_face(
                            level,
                            self.current_room,
                            mfb,
                            &self.camera,
                            WIDTH,
                            HEIGHT,
                        );
                    } else {
                        self.hovered_face = None;
                    }
                } else {
                    self.hovered_face = None;
                }

                // ---- Ray-plane picking for draw tools (works in empty space) ----
                self.hovered_placement_sector = None;
                if matches!(self.tool, EditorTool::DrawFloor | EditorTool::DrawCeiling
                    | EditorTool::DrawWall | EditorTool::PlaceObject)
                {
                    if let (Some(mfb), Some(level)) = (mouse_fb, level.current_level.as_ref()) {
                        if let Some(room) = level.rooms.get(self.current_room) {
                            let plane_y = match self.tool {
                                EditorTool::DrawCeiling => self.ceiling_height,
                                _ => self.floor_height,
                            };
                            self.hovered_placement_sector =
                                pick_sector_floor_plane(room, mfb, &self.camera, plane_y);
                        }
                    }
                }

                // ---- Drag tracking for draw tools ----
                if response.drag_started_by(egui::PointerButton::Primary) {
                    if !matches!(self.tool, EditorTool::Select) {
                        level.push_level_undo();
                        self.drag_placing = true;
                    }
                }
                if response.drag_stopped_by(egui::PointerButton::Primary) {
                    self.drag_placing = false;
                }

                // ---- Apply tool on left click / drag ----
                if response.clicked() {
                    // Short click (not a drag) — apply_tool_* will push their own undo
                    if let Some((room_idx, gx, gz, face)) = self.hovered_face {
                        self.selected_face = Some((room_idx, gx, gz, face));
                        self.selected_sector = Some((gx, gz));
                        state.select(Selection::Room(room_idx));
                        self.apply_tool_to_face(level, state, gx, gz, face);
                    } else if let Some((pi_gx, pi_gz)) = self.hovered_placement_sector {
                        if pi_gx >= 0 && pi_gz >= 0 {
                            self.apply_tool_in_space(level, state, pi_gx as usize, pi_gz as usize);
                        }
                        self.selected_face = None;
                        self.selected_sector = None;
                    } else {
                        self.selected_face = None;
                        self.selected_sector = None;
                    }
                } else if response.dragged_by(egui::PointerButton::Primary) && self.drag_placing {
                    // Drag-to-place: drag_placing=true suppresses undo inside apply_tool_*
                    if let Some((room_idx, gx, gz, face)) = self.hovered_face {
                        self.selected_face = Some((room_idx, gx, gz, face));
                        self.selected_sector = Some((gx, gz));
                        state.select(Selection::Room(room_idx));
                        self.apply_tool_to_face(level, state, gx, gz, face);
                    } else if let Some((pi_gx, pi_gz)) = self.hovered_placement_sector {
                        if pi_gx >= 0 && pi_gz >= 0 {
                            self.apply_tool_in_space(level, state, pi_gx as usize, pi_gz as usize);
                        }
                    }
                }

                // ---- Status bar ----
                let painter = ui.painter();
                let dir_hint = if self.tool == EditorTool::DrawWall {
                    format!("  [{}]  (R)", self.wall_direction.name())
                } else {
                    String::new()
                };
                let hover_str = self.hovered_face.map_or_else(
                    || self.hovered_placement_sector.map_or(String::new(),
                        |(gx, gz)| format!("Grid ({},{})", gx, gz)),
                    |(_, gx, gz, face)| format!("Sector ({},{}) {:?}", gx, gz, face),
                );
                // Status message overlay (timed)
                let msg_str = self.status_message.as_ref().and_then(|(msg, t)| {
                    if t.elapsed().as_secs_f32() < 2.5 { Some(msg.as_str()) } else { None }
                }).unwrap_or("");
                let dirty_str = if level.level_dirty { " *" } else { "" };
                let status = format!(
                    "Tool: {:?}{}{}  |  Dist: {:.0}  |  {}  {}",
                    self.tool, dir_hint, dirty_str, self.orbit_distance, hover_str, msg_str,
                );
                painter.text(
                    Pos2::new(rect.left() + 6.0, rect.bottom() - 6.0),
                    egui::Align2::LEFT_BOTTOM,
                    status,
                    egui::FontId::monospace(11.0),
                    theme::TEXT_DIM,
                );

                // ---- Controls hint (top-right) ----
                painter.text(
                    Pos2::new(rect.right() - 6.0, rect.top() + 6.0),
                    egui::Align2::RIGHT_TOP,
                    "RMB: orbit  Shift+RMB: pan  Scroll: zoom  .: center  B: bounds  Del: delete  Esc: desel  C/V: copy/paste",
                    egui::FontId::monospace(10.0),
                    theme::TEXT_DIM,
                );

                // ---- No-level hint ----
                if level.current_level.is_none() {
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "No level loaded. Use Level > New Level.",
                        egui::FontId::proportional(14.0),
                        theme::TEXT_DIM,
                    );
                }

                // ---- Keyboard shortcuts ----
                if !ctx.wants_keyboard_input() {
                    // Ctrl+C: copy selected face texture to clipboard
                    let ctrl_c = ctx.input(|i| i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::C));
                    if ctrl_c {
                        if let Some((room_idx, gx, gz, face)) = self.selected_face {
                            if let Some(tex) = get_face_texture(&level.current_level, room_idx, gx, gz, face) {
                                self.face_clipboard = Some(FaceClipboard { texture: tex });
                                self.set_status("Copied face texture");
                            }
                        }
                    }

                    // Ctrl+V: paste clipboard texture onto selected face
                    let ctrl_v = ctx.input(|i| i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::V));
                    if ctrl_v {
                        if let (Some((room_idx, gx, gz, face)), Some(clip)) =
                            (self.selected_face, self.face_clipboard.clone())
                        {
                            level.push_level_undo();
                            paste_face_texture(level, room_idx, gx, gz, face, clip.texture);
                            self.needs_viewport_rebuild = true;
                            self.set_status("Pasted face texture");
                        }
                    }

                    let level_ref = level.current_level.as_ref().map(|l| l as &Level);
                    ctx.input(|i| {
                        // Tool selection: letter keys
                        if i.key_pressed(egui::Key::S) { self.tool = EditorTool::Select; }
                        if i.key_pressed(egui::Key::F) { self.tool = EditorTool::DrawFloor; }
                        if i.key_pressed(egui::Key::C) { self.tool = EditorTool::DrawCeiling; }
                        if i.key_pressed(egui::Key::W) { self.tool = EditorTool::DrawWall; }
                        if i.key_pressed(egui::Key::O) { self.tool = EditorTool::PlaceObject; }
                        if i.key_pressed(egui::Key::E) { self.tool = EditorTool::Erase; }
                        // Tool selection: number keys (V1 style)
                        if i.key_pressed(egui::Key::Num1) { self.tool = EditorTool::Select; }
                        if i.key_pressed(egui::Key::Num2) { self.tool = EditorTool::DrawFloor; }
                        if i.key_pressed(egui::Key::Num3) { self.tool = EditorTool::DrawWall; }
                        if i.key_pressed(egui::Key::Num4) { self.tool = EditorTool::DrawCeiling; }
                        if i.key_pressed(egui::Key::Num5) { self.tool = EditorTool::PlaceObject; }
                        // R: cycle wall direction when DrawWall is active
                        if i.key_pressed(egui::Key::R) && self.tool == EditorTool::DrawWall {
                            self.wall_direction = self.wall_direction.rotate_cw();
                        }
                        // B: toggle room bounding boxes
                        if i.key_pressed(egui::Key::B) {
                            self.show_room_bounds = !self.show_room_bounds;
                        }
                        // .: center orbit camera on selected face
                        if i.key_pressed(egui::Key::Period) {
                            self.center_camera_on_selection(level_ref);
                        }
                        // Escape: clear selection
                        if i.key_pressed(egui::Key::Escape) {
                            self.selected_face = None;
                            self.selected_sector = None;
                        }
                    });

                    // Delete / Backspace: remove selected face (needs editor access, outside ctx.input)
                    let delete_pressed = ctx.input(|i| {
                        i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)
                    });
                    if delete_pressed {
                        if let Some((room_idx, gx, gz, face)) = self.selected_face.take() {
                            level.push_level_undo();
                            if let Some(s) = level.current_level.as_mut()
                                .and_then(|l| l.rooms.get_mut(room_idx))
                                .and_then(|r| r.sectors.get_mut(gx))
                                .and_then(|col| col.get_mut(gz))
                                .and_then(|s| s.as_mut())
                            {
                                match face {
                                    SectorFace::Floor          => { s.floor = None; }
                                    SectorFace::Ceiling        => { s.ceiling = None; }
                                    SectorFace::WallNorth(idx) => { if idx < s.walls_north.len() { s.walls_north.remove(idx); } }
                                    SectorFace::WallEast(idx)  => { if idx < s.walls_east.len()  { s.walls_east.remove(idx);  } }
                                    SectorFace::WallSouth(idx) => { if idx < s.walls_south.len() { s.walls_south.remove(idx); } }
                                    SectorFace::WallWest(idx)  => { if idx < s.walls_west.len()  { s.walls_west.remove(idx);  } }
                                    SectorFace::WallNwSe(idx)  => { if idx < s.walls_nwse.len()  { s.walls_nwse.remove(idx);  } }
                                    SectorFace::WallNeSw(idx)  => { if idx < s.walls_nesw.len()  { s.walls_nesw.remove(idx);  } }
                                }
                                self.needs_viewport_rebuild = true;
                            }
                        }
                    }
                }
        }
    }

    // -----------------------------------------------------------------------
    // Center: 2D grid view (secondary mode, accessible via toolbar toggle)
    // -----------------------------------------------------------------------
    fn draw_grid_content(&mut self, ui: &mut egui::Ui, level: &mut LevelEditState, state: &mut AppState) {
        let ctx = ui.ctx().clone();
        {
                let available = ui.available_rect_before_wrap();
                let (rect, response) = ui.allocate_exact_size(
                    available.size(),
                    Sense::click_and_drag(),
                );

                let mouse_pos = response.hover_pos();
                let painter = ui.painter_at(rect);

                // Pan with right drag
                if response.dragged_by(egui::PointerButton::Secondary) {
                    self.grid_offset += response.drag_delta();
                }

                // Zoom with scroll
                let scroll = ui.input(|i| i.raw_scroll_delta.y);
                if scroll.abs() > 0.0 && rect.contains(mouse_pos.unwrap_or(Pos2::ZERO)) {
                    let factor = 1.0 + (scroll * 0.008) as f64;
                    self.grid_zoom = (self.grid_zoom * factor).clamp(0.002, 2.0);
                }

                let center = rect.center() + self.grid_offset;
                let scale = self.grid_zoom as f32;
                let view_mode = self.view_mode;

                let w2s = |wa: f32, wb: f32| -> Pos2 {
                    Pos2::new(center.x + wa * scale, center.y - wb * scale)
                };
                let s2w = |pos: Pos2| -> (f32, f32) {
                    ((pos.x - center.x) / scale, -(pos.y - center.y) / scale)
                };

                if self.show_grid {
                    self.draw_grid_lines(&painter, rect, &w2s, &s2w);
                }

                if level.current_level.is_some() {
                    let cur = level.current_level.as_ref().unwrap();
                    let hovered = self.compute_hover(mouse_pos, cur, &s2w);
                    self.hovered_sector = hovered;

                    if self.tool == EditorTool::DrawWall {
                        self.hovered_edge = hovered.and_then(|(gx, gz)| {
                            cur.rooms.get(self.current_room).and_then(|room| {
                                self.compute_edge_hover(mouse_pos, room, gx, gz, &w2s)
                            })
                        });
                    } else {
                        self.hovered_edge = None;
                    }

                    self.draw_rooms(&painter, cur, &w2s, view_mode);

                    if response.clicked() {
                        if let Some((gx, gz)) = self.hovered_sector {
                            self.selected_sector = Some((gx, gz));
                            self.apply_tool_at(level, state, gx, gz);
                        } else {
                            self.selected_sector = None;
                        }
                    }
                    if response.dragged_by(egui::PointerButton::Primary) {
                        if let Some((gx, gz)) = self.hovered_sector {
                            if self.tool != EditorTool::DrawWall || self.hovered_edge.is_some() {
                                self.apply_tool_at(level, state, gx, gz);
                            }
                        }
                    }
                } else {
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "No level loaded. Use Level > New Level.",
                        egui::FontId::proportional(14.0),
                        theme::TEXT_DIM,
                    );
                }

                // Keyboard shortcuts
                if !ctx.wants_keyboard_input() {
                    ctx.input(|i| {
                        if i.key_pressed(egui::Key::S) { self.tool = EditorTool::Select; }
                        if i.key_pressed(egui::Key::F) { self.tool = EditorTool::DrawFloor; }
                        if i.key_pressed(egui::Key::C) { self.tool = EditorTool::DrawCeiling; }
                        if i.key_pressed(egui::Key::W) { self.tool = EditorTool::DrawWall; }
                        if i.key_pressed(egui::Key::O) { self.tool = EditorTool::PlaceObject; }
                        if i.key_pressed(egui::Key::E) { self.tool = EditorTool::Erase; }
                    });
                }

                // Status bar
                painter.text(
                    Pos2::new(rect.left() + 6.0, rect.bottom() - 6.0),
                    egui::Align2::LEFT_BOTTOM,
                    format!(
                        "Tool: {:?}  |  Zoom: {:.0}px/u  |  {}",
                        self.tool,
                        self.grid_zoom,
                        if let Some((gx, gz)) = self.hovered_sector {
                            format!("Sector ({}, {})", gx, gz)
                        } else {
                            String::new()
                        }
                    ),
                    egui::FontId::monospace(11.0),
                    theme::TEXT_DIM,
                );
        }
    }

    // -----------------------------------------------------------------------
    // 2D grid drawing helpers (kept intact from v1 grid_view port)
    // -----------------------------------------------------------------------
    fn draw_grid_lines(
        &self,
        painter: &Painter,
        rect: Rect,
        w2s: &impl Fn(f32, f32) -> Pos2,
        s2w: &impl Fn(Pos2) -> (f32, f32),
    ) {
        let step = self.grid_size;
        let (min_wa, max_wb) = s2w(rect.min);
        let (max_wa, min_wb) = s2w(rect.max);

        let start = (min_wa / step).floor() * step;
        let mut wa = start;
        while wa <= max_wa + step {
            let p0 = w2s(wa, max_wb);
            let p1 = w2s(wa, min_wb);
            let color = if wa.abs() < step * 0.01 { theme::GRID_AXIS_X } else { theme::GRID_LINE };
            painter.line_segment([p0, p1], Stroke::new(1.0, color));
            wa += step;
        }

        let start = (min_wb / step).floor() * step;
        let mut wb = start;
        while wb <= max_wb + step {
            let p0 = w2s(min_wa, wb);
            let p1 = w2s(max_wa, wb);
            let color = if wb.abs() < step * 0.01 { theme::GRID_AXIS_Z } else { theme::GRID_LINE };
            painter.line_segment([p0, p1], Stroke::new(1.0, color));
            wb += step;
        }
    }

    fn draw_rooms(
        &self,
        painter: &Painter,
        level: &Level,
        w2s: &impl Fn(f32, f32) -> Pos2,
        view_mode: GridViewMode,
    ) {
        for pass in 0..2usize {
            for (room_idx, room) in level.rooms.iter().enumerate() {
                let is_current = room_idx == self.current_room;
                if (pass == 0) == is_current { continue; }
                if self.hidden_rooms.contains(&room_idx) { continue; }
                self.draw_room(painter, room, is_current, w2s, view_mode);
            }
        }
    }

    fn draw_room(
        &self,
        painter: &Painter,
        room: &Room,
        is_current: bool,
        w2s: &impl Fn(f32, f32) -> Pos2,
        view_mode: GridViewMode,
    ) {
        let dim = if is_current { 1.0f32 } else { 0.4 };

        for (gx, gz, sector) in room.iter_sectors() {
            let (base_a, base_b, size_a, size_b) = match view_mode {
                GridViewMode::Top => {
                    let bx = room.position.x + (gx as f32) * SECTOR_SIZE;
                    let bz = room.position.z + (gz as f32) * SECTOR_SIZE;
                    (bx, bz, SECTOR_SIZE, SECTOR_SIZE)
                }
                GridViewMode::Front => {
                    let bx = room.position.x + (gx as f32) * SECTOR_SIZE;
                    let floor_y = room.position.y + sector.floor.as_ref().map(|f| f.avg_height()).unwrap_or(0.0);
                    let ceil_y  = room.position.y + sector.ceiling.as_ref().map(|c| c.avg_height()).unwrap_or(3.0);
                    (bx, floor_y, SECTOR_SIZE, ceil_y - floor_y)
                }
                GridViewMode::Side => {
                    let bz = room.position.z + (gz as f32) * SECTOR_SIZE;
                    let floor_y = room.position.y + sector.floor.as_ref().map(|f| f.avg_height()).unwrap_or(0.0);
                    let ceil_y  = room.position.y + sector.ceiling.as_ref().map(|c| c.avg_height()).unwrap_or(3.0);
                    (bz, floor_y, SECTOR_SIZE, ceil_y - floor_y)
                }
            };

            let tl = w2s(base_a, base_b + size_b);
            let br = w2s(base_a + size_a, base_b);
            let rect = Rect::from_two_pos(tl, br);

            let has_floor = sector.floor.is_some();
            let has_ceil  = sector.ceiling.is_some();
            let fill = if !is_current {
                Color32::from_rgba_unmultiplied(30, 50, 30, 180)
            } else if has_floor && has_ceil {
                Color32::from_rgba_unmultiplied(55, 75, 55, 220)
            } else if has_floor {
                Color32::from_rgba_unmultiplied(45, 65, 45, 200)
            } else if has_ceil {
                Color32::from_rgba_unmultiplied(45, 45, 70, 200)
            } else {
                Color32::from_rgba_unmultiplied(35, 35, 45, 150)
            };
            painter.rect_filled(rect, 0.0, dim_color(fill, dim));

            let wall_stroke = Stroke::new(2.0, dim_color(theme::WALL_COLOR, dim));
            if !sector.walls_north.is_empty() { painter.line_segment([rect.left_top(), rect.right_top()], wall_stroke); }
            if !sector.walls_south.is_empty() { painter.line_segment([rect.left_bottom(), rect.right_bottom()], wall_stroke); }
            if !sector.walls_west.is_empty()  { painter.line_segment([rect.left_top(), rect.left_bottom()], wall_stroke); }
            if !sector.walls_east.is_empty()  { painter.line_segment([rect.right_top(), rect.right_bottom()], wall_stroke); }

            painter.rect_stroke(rect, 0.0, Stroke::new(1.0, dim_color(theme::SECTOR_BORDER, dim)), StrokeKind::Middle);

            if is_current && self.hovered_sector == Some((gx, gz)) {
                painter.rect_stroke(rect, 0.0, Stroke::new(2.0, theme::SECTOR_HOVER), StrokeKind::Middle);
            }

            if is_current && self.tool == EditorTool::DrawWall && self.hovered_sector == Some((gx, gz)) {
                if let Some(dir) = self.hovered_edge {
                    let edge = match dir {
                        Direction::North => Some((rect.left_top(), rect.right_top())),
                        Direction::South => Some((rect.left_bottom(), rect.right_bottom())),
                        Direction::West  => Some((rect.left_top(), rect.left_bottom())),
                        Direction::East  => Some((rect.right_top(), rect.right_bottom())),
                        _ => None,
                    };
                    if let Some((p0, p1)) = edge {
                        painter.line_segment([p0, p1], Stroke::new(3.0, theme::ACCENT));
                    }
                }
            }

            if is_current && self.selected_sector == Some((gx, gz)) {
                painter.rect_stroke(rect, 0.0, Stroke::new(2.0, theme::SECTOR_SELECT), StrokeKind::Middle);
            }
        }
    }

    // -----------------------------------------------------------------------
    // 2D hover / edge detection
    // -----------------------------------------------------------------------
    fn compute_hover(
        &self,
        mouse_pos: Option<Pos2>,
        level: &Level,
        s2w: &impl Fn(Pos2) -> (f32, f32),
    ) -> Option<(usize, usize)> {
        let pos = mouse_pos?;
        let (wx, wz) = s2w(pos);
        let room = level.rooms.get(self.current_room)?;
        let local_x = wx - room.position.x;
        let local_z = wz - room.position.z;
        if local_x < 0.0 || local_z < 0.0 { return None; }
        let gx = (local_x / SECTOR_SIZE) as usize;
        let gz = (local_z / SECTOR_SIZE) as usize;
        if gx < room.width && gz < room.depth && room.get_sector(gx, gz).is_some() {
            Some((gx, gz))
        } else {
            None
        }
    }

    fn compute_edge_hover(
        &self,
        mouse_pos: Option<Pos2>,
        room: &Room,
        gx: usize,
        gz: usize,
        w2s: &impl Fn(f32, f32) -> Pos2,
    ) -> Option<Direction> {
        let mpos = mouse_pos?;
        let bx = room.position.x + gx as f32 * SECTOR_SIZE;
        let bz = room.position.z + gz as f32 * SECTOR_SIZE;

        let tl = w2s(bx,               bz + SECTOR_SIZE);
        let tr = w2s(bx + SECTOR_SIZE, bz + SECTOR_SIZE);
        let bl = w2s(bx,               bz);
        let br = w2s(bx + SECTOR_SIZE, bz);

        let dist_north = dist_to_segment(mpos, tl, tr);
        let dist_south = dist_to_segment(mpos, bl, br);
        let dist_west  = dist_to_segment(mpos, tl, bl);
        let dist_east  = dist_to_segment(mpos, tr, br);

        let threshold = 10.0f32;
        let min_dist = [dist_north, dist_south, dist_west, dist_east]
            .iter().cloned().fold(f32::INFINITY, f32::min);

        if min_dist > threshold { return None; }

        if      (min_dist - dist_north).abs() < 0.5 { Some(Direction::North) }
        else if (min_dist - dist_south).abs() < 0.5 { Some(Direction::South) }
        else if (min_dist - dist_west).abs()  < 0.5 { Some(Direction::West)  }
        else                                         { Some(Direction::East)  }
    }

    // -----------------------------------------------------------------------
    // Tool application (2D sector-based — for 2D grid view)
    // -----------------------------------------------------------------------
    fn apply_tool_at(&mut self, level: &mut LevelEditState, _state: &mut AppState, gx: usize, gz: usize) {
        if self.tool == EditorTool::Select { return; }
        level.push_level_undo();
        self.needs_viewport_rebuild = true;
        let Some(level) = level.current_level.as_mut() else { return };
        let Some(room) = level.rooms.get_mut(self.current_room) else { return };
        let tex = self.selected_texture.clone();

        match self.tool {
            EditorTool::DrawFloor => {
                if gx < room.width && gz < room.depth {
                    let s = room.sectors[gx][gz].get_or_insert_with(Sector::default);
                    s.floor = Some(HorizontalFace::flat(self.floor_height, tex));
                }
            }
            EditorTool::DrawCeiling => {
                if gx < room.width && gz < room.depth {
                    let s = room.sectors[gx][gz].get_or_insert_with(Sector::default);
                    s.ceiling = Some(HorizontalFace::flat(self.ceiling_height, tex));
                }
            }
            EditorTool::DrawWall => {
                let Some(dir) = self.hovered_edge else { return };
                if gx < room.width && gz < room.depth {
                    let s = room.sectors[gx][gz].get_or_insert_with(Sector::default);
                    let bot = s.floor.as_ref().map(|f| f.avg_height()).unwrap_or(self.floor_height);
                    let top = s.ceiling.as_ref().map(|c| c.avg_height()).unwrap_or(self.ceiling_height);
                    let wall = VerticalFace::new(bot, top, tex);
                    let walls = s.walls_mut(dir);
                    walls.clear();
                    walls.push(wall);
                }
            }
            EditorTool::PlaceObject => {
                room.objects.push(AssetInstance::new(gx, gz));
            }
            EditorTool::Erase => {
                room.remove_sector(gx, gz);
            }
            EditorTool::Select => {}
        }
    }

    // -----------------------------------------------------------------------
    // Tool application (3D face-based — for 3D viewport)
    // -----------------------------------------------------------------------
    fn apply_tool_to_face(&mut self, level: &mut LevelEditState, _state: &mut AppState, gx: usize, gz: usize, face: SectorFace) {
        if self.tool == EditorTool::Select { return; }
        // drag_placing=true means a single undo was already pushed at drag start
        if !self.drag_placing {
            level.push_level_undo();
        }
        self.needs_viewport_rebuild = true;
        let Some(level) = level.current_level.as_mut() else { return };
        let Some(room) = level.rooms.get_mut(self.current_room) else { return };
        let tex = self.selected_texture.clone();
        let wall_dir = self.wall_direction;

        match self.tool {
            EditorTool::DrawFloor => {
                if gx < room.width && gz < room.depth {
                    let s = room.sectors[gx][gz].get_or_insert_with(Sector::default);
                    s.floor = Some(HorizontalFace::flat(self.floor_height, tex));
                }
            }
            EditorTool::DrawCeiling => {
                if gx < room.width && gz < room.depth {
                    let s = room.sectors[gx][gz].get_or_insert_with(Sector::default);
                    s.ceiling = Some(HorizontalFace::flat(self.ceiling_height, tex));
                }
            }
            EditorTool::DrawWall => {
                // Clicking an existing wall → replace that layer.
                // Clicking floor/ceiling → add a wall on the current wall_direction edge.
                let dir = match face {
                    SectorFace::WallNorth(_) => Direction::North,
                    SectorFace::WallEast(_)  => Direction::East,
                    SectorFace::WallSouth(_) => Direction::South,
                    SectorFace::WallWest(_)  => Direction::West,
                    SectorFace::WallNwSe(_)  => Direction::NwSe,
                    SectorFace::WallNeSw(_)  => Direction::NeSw,
                    SectorFace::Floor | SectorFace::Ceiling => wall_dir,
                };
                if gx < room.width && gz < room.depth {
                    let s = room.sectors[gx][gz].get_or_insert_with(Sector::default);
                    let bot = s.floor.as_ref().map(|f| f.avg_height()).unwrap_or(self.floor_height);
                    let top = s.ceiling.as_ref().map(|c| c.avg_height()).unwrap_or(self.ceiling_height);
                    let wall = VerticalFace::new(bot, top, tex);
                    let walls = s.walls_mut(dir);
                    walls.clear();
                    walls.push(wall);
                }
            }
            EditorTool::PlaceObject => {
                room.objects.push(AssetInstance::new(gx, gz));
            }
            EditorTool::Erase => {
                if let Some(Some(s)) = room.sectors.get_mut(gx).and_then(|col| col.get_mut(gz)) {
                    match face {
                        SectorFace::Floor => { s.floor = None; }
                        SectorFace::Ceiling => { s.ceiling = None; }
                        SectorFace::WallNorth(i) => { if i < s.walls_north.len() { s.walls_north.remove(i); } }
                        SectorFace::WallEast(i)  => { if i < s.walls_east.len()  { s.walls_east.remove(i);  } }
                        SectorFace::WallSouth(i) => { if i < s.walls_south.len() { s.walls_south.remove(i); } }
                        SectorFace::WallWest(i)  => { if i < s.walls_west.len()  { s.walls_west.remove(i);  } }
                        SectorFace::WallNwSe(i)  => { if i < s.walls_nwse.len()  { s.walls_nwse.remove(i);  } }
                        SectorFace::WallNeSw(i)  => { if i < s.walls_nesw.len()  { s.walls_nesw.remove(i);  } }
                    }
                }
            }
            EditorTool::Select => {}
        }
    }

    /// Apply the current draw tool to a grid position that may be empty.
    /// Creates the sector if needed and expands the room grid when placing outside bounds.
    fn apply_tool_in_space(&mut self, level: &mut LevelEditState, _state: &mut AppState, gx: usize, gz: usize) {
        if !matches!(self.tool,
            EditorTool::DrawFloor | EditorTool::DrawCeiling
            | EditorTool::DrawWall | EditorTool::PlaceObject)
        {
            return;
        }
        // drag_placing=true means the undo was already pushed at drag start
        if !self.drag_placing {
            level.push_level_undo();
        }
        self.needs_viewport_rebuild = true;
        let Some(level) = level.current_level.as_mut() else { return };
        let Some(room) = level.rooms.get_mut(self.current_room) else { return };
        let tex = self.selected_texture.clone();
        let wall_dir = self.wall_direction;

        // Expand grid to accommodate the target position
        expand_room_to_fit(room, gx, gz);

        match self.tool {
            EditorTool::DrawFloor => {
                let s = room.sectors[gx][gz].get_or_insert_with(Sector::default);
                s.floor = Some(HorizontalFace::flat(self.floor_height, tex));
            }
            EditorTool::DrawCeiling => {
                let s = room.sectors[gx][gz].get_or_insert_with(Sector::default);
                s.ceiling = Some(HorizontalFace::flat(self.ceiling_height, tex));
            }
            EditorTool::DrawWall => {
                let s = room.sectors[gx][gz].get_or_insert_with(Sector::default);
                let bot = s.floor.as_ref().map(|f| f.avg_height()).unwrap_or(self.floor_height);
                let top = s.ceiling.as_ref().map(|c| c.avg_height()).unwrap_or(self.ceiling_height);
                let wall = VerticalFace::new(bot, top, tex);
                let walls = s.walls_mut(wall_dir);
                walls.clear();
                walls.push(wall);
            }
            EditorTool::PlaceObject => {
                room.objects.push(AssetInstance::new(gx, gz));
            }
            _ => {}
        }
    }
}

impl Default for WorldEditorPanel {
    fn default() -> Self { Self::new() }
}

// ---------------------------------------------------------------------------
// 3D picking helpers (ported from v1/src/editor/viewport_3d.rs)
// ---------------------------------------------------------------------------

/// Find the hovered sector face for the current room.
/// Returns (room_idx, gx, gz, SectorFace) for the closest face under the mouse.
fn find_hovered_face(
    level: &Level,
    current_room: usize,
    mouse_fb: (f32, f32),
    camera: &Camera,
    fb_width: usize,
    fb_height: usize,
) -> Option<(usize, usize, usize, SectorFace)> {
    let (mouse_x, mouse_y) = mouse_fb;
    let room = level.rooms.get(current_room)?;
    let room_y = room.position.y;

    let mut best: Option<(usize, usize, usize, SectorFace, f32)> = None;

    for (gx, gz, sector) in room.iter_sectors() {
        let base_x = room.position.x + (gx as f32) * SECTOR_SIZE;
        let base_z = room.position.z + (gz as f32) * SECTOR_SIZE;

        // Floor
        if let Some(floor) = &sector.floor {
            let corners = [
                Vec3::new(base_x,              room_y + floor.heights[0], base_z),
                Vec3::new(base_x + SECTOR_SIZE, room_y + floor.heights[1], base_z),
                Vec3::new(base_x + SECTOR_SIZE, room_y + floor.heights[2], base_z + SECTOR_SIZE),
                Vec3::new(base_x,              room_y + floor.heights[3], base_z + SECTOR_SIZE),
            ];
            if let Some(d) = check_quad_hit(mouse_x, mouse_y, &corners, camera, fb_width, fb_height) {
                if best.map_or(true, |(_, _, _, _, bd)| d < bd) {
                    best = Some((current_room, gx, gz, SectorFace::Floor, d));
                }
            }
        }

        // Ceiling
        if let Some(ceiling) = &sector.ceiling {
            let corners = [
                Vec3::new(base_x,              room_y + ceiling.heights[0], base_z),
                Vec3::new(base_x + SECTOR_SIZE, room_y + ceiling.heights[1], base_z),
                Vec3::new(base_x + SECTOR_SIZE, room_y + ceiling.heights[2], base_z + SECTOR_SIZE),
                Vec3::new(base_x,              room_y + ceiling.heights[3], base_z + SECTOR_SIZE),
            ];
            if let Some(d) = check_quad_hit(mouse_x, mouse_y, &corners, camera, fb_width, fb_height) {
                if best.map_or(true, |(_, _, _, _, bd)| d < bd) {
                    best = Some((current_room, gx, gz, SectorFace::Ceiling, d));
                }
            }
        }

        // Cardinal walls
        let wall_configs: [(&Vec<VerticalFace>, f32, f32, f32, f32, fn(usize) -> SectorFace); 4] = [
            (&sector.walls_north, base_x, base_z, base_x + SECTOR_SIZE, base_z,              |i| SectorFace::WallNorth(i)),
            (&sector.walls_east,  base_x + SECTOR_SIZE, base_z, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE, |i| SectorFace::WallEast(i)),
            (&sector.walls_south, base_x + SECTOR_SIZE, base_z + SECTOR_SIZE, base_x, base_z + SECTOR_SIZE, |i| SectorFace::WallSouth(i)),
            (&sector.walls_west,  base_x, base_z + SECTOR_SIZE, base_x, base_z,              |i| SectorFace::WallWest(i)),
        ];
        for (walls, x0, z0, x1, z1, make_face) in &wall_configs {
            for (i, wall) in walls.iter().enumerate() {
                let corners = [
                    Vec3::new(*x0, room_y + wall.heights[0], *z0),
                    Vec3::new(*x1, room_y + wall.heights[1], *z1),
                    Vec3::new(*x1, room_y + wall.heights[2], *z1),
                    Vec3::new(*x0, room_y + wall.heights[3], *z0),
                ];
                if let Some(d) = check_quad_hit(mouse_x, mouse_y, &corners, camera, fb_width, fb_height) {
                    if best.map_or(true, |(_, _, _, _, bd)| d < bd) {
                        best = Some((current_room, gx, gz, make_face(i), d));
                    }
                }
            }
        }

        // Diagonal NW-SE walls
        for (i, wall) in sector.walls_nwse.iter().enumerate() {
            let corners = [
                Vec3::new(base_x,              room_y + wall.heights[0], base_z),
                Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[1], base_z + SECTOR_SIZE),
                Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[2], base_z + SECTOR_SIZE),
                Vec3::new(base_x,              room_y + wall.heights[3], base_z),
            ];
            if let Some(d) = check_quad_hit(mouse_x, mouse_y, &corners, camera, fb_width, fb_height) {
                if best.map_or(true, |(_, _, _, _, bd)| d < bd) {
                    best = Some((current_room, gx, gz, SectorFace::WallNwSe(i), d));
                }
            }
        }

        // Diagonal NE-SW walls
        for (i, wall) in sector.walls_nesw.iter().enumerate() {
            let corners = [
                Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[0], base_z),
                Vec3::new(base_x,              room_y + wall.heights[1], base_z + SECTOR_SIZE),
                Vec3::new(base_x,              room_y + wall.heights[2], base_z + SECTOR_SIZE),
                Vec3::new(base_x + SECTOR_SIZE, room_y + wall.heights[3], base_z),
            ];
            if let Some(d) = check_quad_hit(mouse_x, mouse_y, &corners, camera, fb_width, fb_height) {
                if best.map_or(true, |(_, _, _, _, bd)| d < bd) {
                    best = Some((current_room, gx, gz, SectorFace::WallNeSw(i), d));
                }
            }
        }
    }

    best.map(|(ri, gx, gz, face, _)| (ri, gx, gz, face))
}

/// Test if (mouse_x, mouse_y) in framebuffer coordinates hits a world-space quad.
/// Returns the interpolated depth at the hit point, or None.
/// Ported from v1/src/editor/viewport_3d.rs: check_quad_hit_with_depth.
fn check_quad_hit(
    mouse_x: f32,
    mouse_y: f32,
    corners: &[Vec3; 4],
    camera: &Camera,
    fb_width: usize,
    fb_height: usize,
) -> Option<f32> {
    use crate::rasterizer::{world_to_screen_with_depth, point_in_triangle_2d};

    let project = |c: Vec3| {
        world_to_screen_with_depth(
            c,
            camera.position, camera.basis_x, camera.basis_y, camera.basis_z,
            fb_width, fb_height,
        )
    };

    let (sx0, sy0, d0) = project(corners[0])?;
    let (sx1, sy1, d1) = project(corners[1])?;
    let (sx2, sy2, d2) = project(corners[2])?;
    let (sx3, sy3, d3) = project(corners[3])?;

    // Triangle (0, 1, 2)
    if point_in_triangle_2d(mouse_x, mouse_y, sx0, sy0, sx1, sy1, sx2, sy2) {
        return Some(interp_depth(mouse_x, mouse_y, sx0, sy0, d0, sx1, sy1, d1, sx2, sy2, d2));
    }
    // Triangle (0, 2, 3)
    if point_in_triangle_2d(mouse_x, mouse_y, sx0, sy0, sx2, sy2, sx3, sy3) {
        return Some(interp_depth(mouse_x, mouse_y, sx0, sy0, d0, sx2, sy2, d2, sx3, sy3, d3));
    }

    None
}

/// Barycentric depth interpolation inside a triangle.
/// Ported from v1/src/editor/viewport_3d.rs: interpolate_depth_in_triangle.
fn interp_depth(
    px: f32, py: f32,
    x0: f32, y0: f32, d0: f32,
    x1: f32, y1: f32, d1: f32,
    x2: f32, y2: f32, d2: f32,
) -> f32 {
    let area = (x1 - x0) * (y2 - y0) - (x2 - x0) * (y1 - y0);
    if area.abs() < 0.0001 {
        return (d0 + d1 + d2) / 3.0;
    }
    let w0 = ((x1 - px) * (y2 - py) - (x2 - px) * (y1 - py)) / area;
    let w1 = ((x2 - px) * (y0 - py) - (x0 - px) * (y2 - py)) / area;
    let w2 = 1.0 - w0 - w1;
    w0 * d0 + w1 * d1 + w2 * d2
}

/// Get the 4 world-space corners of a sector face for wireframe drawing.
fn face_corners(room: &Room, gx: usize, gz: usize, face: SectorFace) -> Option<[Vec3; 4]> {
    let room_y = room.position.y;
    let base_x = room.position.x + gx as f32 * SECTOR_SIZE;
    let base_z = room.position.z + gz as f32 * SECTOR_SIZE;
    let sector = room.get_sector(gx, gz)?;

    let corners = match face {
        SectorFace::Floor => {
            let f = sector.floor.as_ref()?;
            [
                Vec3::new(base_x,              room_y + f.heights[0], base_z),
                Vec3::new(base_x + SECTOR_SIZE, room_y + f.heights[1], base_z),
                Vec3::new(base_x + SECTOR_SIZE, room_y + f.heights[2], base_z + SECTOR_SIZE),
                Vec3::new(base_x,              room_y + f.heights[3], base_z + SECTOR_SIZE),
            ]
        }
        SectorFace::Ceiling => {
            let c = sector.ceiling.as_ref()?;
            [
                Vec3::new(base_x,              room_y + c.heights[0], base_z),
                Vec3::new(base_x + SECTOR_SIZE, room_y + c.heights[1], base_z),
                Vec3::new(base_x + SECTOR_SIZE, room_y + c.heights[2], base_z + SECTOR_SIZE),
                Vec3::new(base_x,              room_y + c.heights[3], base_z + SECTOR_SIZE),
            ]
        }
        SectorFace::WallNorth(i) => {
            let w = sector.walls_north.get(i)?;
            [
                Vec3::new(base_x,              room_y + w.heights[0], base_z),
                Vec3::new(base_x + SECTOR_SIZE, room_y + w.heights[1], base_z),
                Vec3::new(base_x + SECTOR_SIZE, room_y + w.heights[2], base_z),
                Vec3::new(base_x,              room_y + w.heights[3], base_z),
            ]
        }
        SectorFace::WallEast(i) => {
            let w = sector.walls_east.get(i)?;
            [
                Vec3::new(base_x + SECTOR_SIZE, room_y + w.heights[0], base_z),
                Vec3::new(base_x + SECTOR_SIZE, room_y + w.heights[1], base_z + SECTOR_SIZE),
                Vec3::new(base_x + SECTOR_SIZE, room_y + w.heights[2], base_z + SECTOR_SIZE),
                Vec3::new(base_x + SECTOR_SIZE, room_y + w.heights[3], base_z),
            ]
        }
        SectorFace::WallSouth(i) => {
            let w = sector.walls_south.get(i)?;
            [
                Vec3::new(base_x + SECTOR_SIZE, room_y + w.heights[0], base_z + SECTOR_SIZE),
                Vec3::new(base_x,              room_y + w.heights[1], base_z + SECTOR_SIZE),
                Vec3::new(base_x,              room_y + w.heights[2], base_z + SECTOR_SIZE),
                Vec3::new(base_x + SECTOR_SIZE, room_y + w.heights[3], base_z + SECTOR_SIZE),
            ]
        }
        SectorFace::WallWest(i) => {
            let w = sector.walls_west.get(i)?;
            [
                Vec3::new(base_x, room_y + w.heights[0], base_z + SECTOR_SIZE),
                Vec3::new(base_x, room_y + w.heights[1], base_z),
                Vec3::new(base_x, room_y + w.heights[2], base_z),
                Vec3::new(base_x, room_y + w.heights[3], base_z + SECTOR_SIZE),
            ]
        }
        SectorFace::WallNwSe(i) => {
            let w = sector.walls_nwse.get(i)?;
            [
                Vec3::new(base_x,              room_y + w.heights[0], base_z),
                Vec3::new(base_x + SECTOR_SIZE, room_y + w.heights[1], base_z + SECTOR_SIZE),
                Vec3::new(base_x + SECTOR_SIZE, room_y + w.heights[2], base_z + SECTOR_SIZE),
                Vec3::new(base_x,              room_y + w.heights[3], base_z),
            ]
        }
        SectorFace::WallNeSw(i) => {
            let w = sector.walls_nesw.get(i)?;
            [
                Vec3::new(base_x + SECTOR_SIZE, room_y + w.heights[0], base_z),
                Vec3::new(base_x,              room_y + w.heights[1], base_z + SECTOR_SIZE),
                Vec3::new(base_x,              room_y + w.heights[2], base_z + SECTOR_SIZE),
                Vec3::new(base_x + SECTOR_SIZE, room_y + w.heights[3], base_z),
            ]
        }
    };

    Some(corners)
}

// ---------------------------------------------------------------------------
// 3D placement helpers
// ---------------------------------------------------------------------------

/// Cast a ray from mouse framebuffer coords and intersect with the horizontal plane
/// at `room.position.y + floor_y`. Returns the integer grid cell (gx, gz) the ray hits.
/// Grid coords can be negative or beyond room bounds — caller handles expansion/clamping.
fn pick_sector_floor_plane(
    room: &Room,
    mouse_fb: (f32, f32),
    camera: &Camera,
    floor_y: f32,
) -> Option<(i32, i32)> {
    use crate::rasterizer::{screen_to_ray, ray_plane_intersection};

    let ray = screen_to_ray(mouse_fb.0, mouse_fb.1, WIDTH, HEIGHT, camera);
    let plane_y = room.position.y + floor_y;
    let t = ray_plane_intersection(&ray, Vec3::new(0.0, plane_y, 0.0), Vec3::new(0.0, 1.0, 0.0))?;
    let hit = ray.at(t);

    let local_x = hit.x - room.position.x;
    let local_z = hit.z - room.position.z;
    Some((
        (local_x / SECTOR_SIZE).floor() as i32,
        (local_z / SECTOR_SIZE).floor() as i32,
    ))
}

/// Expand the room's sector grid so it can hold position (gx, gz).
/// Only expands in the +X/+Z directions (appends columns/rows).
fn expand_room_to_fit(room: &mut Room, gx: usize, gz: usize) {
    if gx >= room.width {
        let new_width = gx + 1;
        while room.sectors.len() < new_width {
            room.sectors.push((0..room.depth).map(|_| None).collect());
        }
        room.width = new_width;
    }
    if gz >= room.depth {
        let new_depth = gz + 1;
        for col in room.sectors.iter_mut() {
            while col.len() < new_depth {
                col.push(None);
            }
        }
        room.depth = new_depth;
    }
}

/// Compute the 4 world-space corners of a wall quad on the given direction edge of a sector.
/// `bx`/`bz` are the sector's world-space NW corner; `y_bot`/`y_top` are world-space Y bounds.
fn wall_direction_corners(bx: f32, bz: f32, y_bot: f32, y_top: f32, dir: Direction) -> [Vec3; 4] {
    match dir {
        Direction::North => [
            Vec3::new(bx,              y_bot, bz),
            Vec3::new(bx + SECTOR_SIZE, y_bot, bz),
            Vec3::new(bx + SECTOR_SIZE, y_top, bz),
            Vec3::new(bx,              y_top, bz),
        ],
        Direction::East => [
            Vec3::new(bx + SECTOR_SIZE, y_bot, bz),
            Vec3::new(bx + SECTOR_SIZE, y_bot, bz + SECTOR_SIZE),
            Vec3::new(bx + SECTOR_SIZE, y_top, bz + SECTOR_SIZE),
            Vec3::new(bx + SECTOR_SIZE, y_top, bz),
        ],
        Direction::South => [
            Vec3::new(bx + SECTOR_SIZE, y_bot, bz + SECTOR_SIZE),
            Vec3::new(bx,              y_bot, bz + SECTOR_SIZE),
            Vec3::new(bx,              y_top, bz + SECTOR_SIZE),
            Vec3::new(bx + SECTOR_SIZE, y_top, bz + SECTOR_SIZE),
        ],
        Direction::West => [
            Vec3::new(bx, y_bot, bz + SECTOR_SIZE),
            Vec3::new(bx, y_bot, bz),
            Vec3::new(bx, y_top, bz),
            Vec3::new(bx, y_top, bz + SECTOR_SIZE),
        ],
        Direction::NwSe => [
            Vec3::new(bx,              y_bot, bz),
            Vec3::new(bx + SECTOR_SIZE, y_bot, bz + SECTOR_SIZE),
            Vec3::new(bx + SECTOR_SIZE, y_top, bz + SECTOR_SIZE),
            Vec3::new(bx,              y_top, bz),
        ],
        Direction::NeSw => [
            Vec3::new(bx + SECTOR_SIZE, y_bot, bz),
            Vec3::new(bx,              y_bot, bz + SECTOR_SIZE),
            Vec3::new(bx,              y_top, bz + SECTOR_SIZE),
            Vec3::new(bx + SECTOR_SIZE, y_top, bz),
        ],
    }
}

// ---------------------------------------------------------------------------
// 2D geometry helpers
// ---------------------------------------------------------------------------

fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_sq();
    if len_sq < 1e-6 { return (p - a).length(); }
    let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

// ---------------------------------------------------------------------------
// Face copy-paste helpers
// ---------------------------------------------------------------------------

/// Extract the texture from a face in the level (for Ctrl+C).
fn get_face_texture(
    level: &Option<crate::scene::Level>,
    room_idx: usize,
    gx: usize,
    gz: usize,
    face: SectorFace,
) -> Option<TextureRef> {
    let sector = level.as_ref()?.rooms.get(room_idx)?.get_sector(gx, gz)?;
    match face {
        SectorFace::Floor         => Some(sector.floor.as_ref()?.texture.clone()),
        SectorFace::Ceiling       => Some(sector.ceiling.as_ref()?.texture.clone()),
        SectorFace::WallNorth(i)  => Some(sector.walls_north.get(i)?.texture.clone()),
        SectorFace::WallEast(i)   => Some(sector.walls_east.get(i)?.texture.clone()),
        SectorFace::WallSouth(i)  => Some(sector.walls_south.get(i)?.texture.clone()),
        SectorFace::WallWest(i)   => Some(sector.walls_west.get(i)?.texture.clone()),
        SectorFace::WallNwSe(i)   => Some(sector.walls_nwse.get(i)?.texture.clone()),
        SectorFace::WallNeSw(i)   => Some(sector.walls_nesw.get(i)?.texture.clone()),
    }
}

/// Apply a clipboard texture to a face (for Ctrl+V).
fn paste_face_texture(
    level: &mut LevelEditState,
    room_idx: usize,
    gx: usize,
    gz: usize,
    face: SectorFace,
    tex: TextureRef,
) {
    let Some(s) = level.current_level.as_mut()
        .and_then(|l| l.rooms.get_mut(room_idx))
        .and_then(|r| r.sectors.get_mut(gx))
        .and_then(|col| col.get_mut(gz))
        .and_then(|s| s.as_mut())
    else { return };

    match face {
        SectorFace::Floor        => { if let Some(f) = &mut s.floor        { f.texture = tex; } }
        SectorFace::Ceiling      => { if let Some(c) = &mut s.ceiling      { c.texture = tex; } }
        SectorFace::WallNorth(i) => { if let Some(w) = s.walls_north.get_mut(i) { w.texture = tex; } }
        SectorFace::WallEast(i)  => { if let Some(w) = s.walls_east.get_mut(i)  { w.texture = tex; } }
        SectorFace::WallSouth(i) => { if let Some(w) = s.walls_south.get_mut(i) { w.texture = tex; } }
        SectorFace::WallWest(i)  => { if let Some(w) = s.walls_west.get_mut(i)  { w.texture = tex; } }
        SectorFace::WallNwSe(i)  => { if let Some(w) = s.walls_nwse.get_mut(i)  { w.texture = tex; } }
        SectorFace::WallNeSw(i)  => { if let Some(w) = s.walls_nesw.get_mut(i)  { w.texture = tex; } }
    }
}

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------
fn dim_color(c: Color32, factor: f32) -> Color32 {
    let [r, g, b, a] = c.to_array();
    Color32::from_rgba_unmultiplied(
        (r as f32 * factor) as u8,
        (g as f32 * factor) as u8,
        (b as f32 * factor) as u8,
        a,
    )
}
