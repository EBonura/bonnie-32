//! World editor — 2D sector grid view + tools, ported from v1 editor/grid_view.rs.
//! macroquad draw calls → egui Painter. Same coordinate system, same tool set.

use egui::{Color32, Painter, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use crate::scene::{AssetInstance, Direction, Level, Room, Sector, HorizontalFace, VerticalFace, TextureRef, SECTOR_SIZE};
use super::context::{EditorAction, EditorContext, Selection};
use super::icons::{icon, icon_button, icon_toggle};
use super::theme;

// ---------------------------------------------------------------------------
// Tool & view state
// ---------------------------------------------------------------------------

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
    Top,   // X-Z plane (looking down Y axis) — primary editing view
    Front, // X-Y plane
    Side,  // Z-Y plane
}

pub struct WorldEditorPanel {
    pub tool: EditorTool,
    pub view_mode: GridViewMode,
    pub current_room: usize,

    // 2D view navigation (pan + zoom)
    pub grid_zoom: f64,
    pub grid_offset: Vec2,
    panning: bool,
    last_mouse: Pos2,

    // Grid display
    pub show_grid: bool,
    pub grid_size: f32,

    // Hidden rooms
    pub hidden_rooms: std::collections::HashSet<usize>,

    // Draw tool settings
    pub floor_height: f32,
    pub ceiling_height: f32,
    pub selected_texture: TextureRef,

    // Selection
    pub hovered_sector: Option<(usize, usize)>,
    pub selected_sector: Option<(usize, usize)>,
    pub hovered_edge: Option<Direction>,
}

impl WorldEditorPanel {
    pub fn new() -> Self {
        Self {
            tool: EditorTool::DrawFloor,
            view_mode: GridViewMode::Top,
            current_room: 0,
            grid_zoom: 0.1, // pixels per world unit; at 0.1 each 1024-unit sector is ~102px
            grid_offset: Vec2::ZERO,
            panning: false,
            last_mouse: Pos2::ZERO,
            show_grid: true,
            grid_size: SECTOR_SIZE,
            hidden_rooms: std::collections::HashSet::new(),
            floor_height: 0.0,
            ceiling_height: 3.0,
            selected_texture: TextureRef::new("_DEFAULT", "checkerboard"),
            hovered_sector: None,
            selected_sector: None,
            hovered_edge: None,
        }
    }

    /// Draw the full world editor layout.
    pub fn draw(&mut self, ctx: &egui::Context, editor: &mut EditorContext) {
        self.draw_tool_panel(ctx, editor);
        self.draw_room_panel(ctx, editor);
        self.draw_properties_panel(ctx, editor);
        self.draw_grid_view(ctx, editor);
    }

    // -----------------------------------------------------------------------
    // Left: tool palette
    // -----------------------------------------------------------------------
    fn draw_tool_panel(&mut self, ctx: &egui::Context, _editor: &mut EditorContext) {
        egui::SidePanel::left("we_tools")
            .resizable(false)
            .exact_width(40.0)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(4.0);
                    self.tool_btn(ui, EditorTool::Select,      icon::POINTER,      "Select (S)");
                    self.tool_btn(ui, EditorTool::DrawFloor,   icon::LAYERS,       "Draw Floor (F)");
                    self.tool_btn(ui, EditorTool::DrawCeiling, icon::BOX,          "Draw Ceiling (C)");
                    self.tool_btn(ui, EditorTool::DrawWall,    icon::BRICK_WALL,   "Draw Wall (W)");
                    self.tool_btn(ui, EditorTool::PlaceObject, icon::MAP_PIN,      "Place Object (O)");
                    ui.separator();
                    self.tool_btn(ui, EditorTool::Erase,       icon::ERASER,       "Erase (E)");

                    ui.add_space(8.0);
                    ui.separator();

                    // Grid toggle
                    let grid_active = self.show_grid;
                    if icon_toggle(ui, icon::GRID, theme::ICON_SIZE_MD, grid_active, "Toggle Grid") {
                        self.show_grid = !self.show_grid;
                    }
                });
            });
    }

    fn tool_btn(&mut self, ui: &mut egui::Ui, tool: EditorTool, ic: char, tip: &str) {
        let active = self.tool == tool;
        if icon_toggle(ui, ic, theme::ICON_SIZE_LG, active, tip) {
            self.tool = tool;
        }
        ui.add_space(2.0);
    }

    // -----------------------------------------------------------------------
    // Left-inner: room list (below tool panel, combined)
    // -----------------------------------------------------------------------
    fn draw_room_panel(&mut self, ctx: &egui::Context, editor: &mut EditorContext) {
        egui::SidePanel::left("we_rooms")
            .resizable(true)
            .default_width(160.0)
            .min_width(100.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("Rooms");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_button(ui, icon::PLUS, theme::ICON_SIZE_SM, "Add room") {
                            editor.request_action(EditorAction::AddRoom);
                        }
                    });
                });
                ui.separator();

                let room_count = editor.current_level.as_ref().map(|l| l.rooms.len()).unwrap_or(0);
                if room_count == 0 {
                    ui.weak("No level loaded.\nUse Level > New Level.");
                    return;
                }

                for i in 0..room_count {
                    let hidden = self.hidden_rooms.contains(&i);
                    let current = self.current_room == i;

                    ui.horizontal(|ui| {
                        // Eye toggle
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
                            editor.select(Selection::Room(i));
                        }
                    });
                }

                // View mode switcher
                ui.add_space(8.0);
                ui.separator();
                ui.label(egui::RichText::new("View").small().color(theme::TEXT_DIM));
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
            });
    }

    // -----------------------------------------------------------------------
    // Right: sector properties + object list
    // -----------------------------------------------------------------------
    fn draw_properties_panel(&mut self, ctx: &egui::Context, editor: &mut EditorContext) {
        egui::SidePanel::right("we_props")
            .resizable(true)
            .default_width(220.0)
            .show(ctx, |ui| {
                ui.strong("Properties");
                ui.separator();

                // ── Brush settings (always visible) ─────────────────────
                ui.label(egui::RichText::new("Brush").small().color(theme::TEXT_DIM));
                ui.horizontal(|ui| {
                    ui.label("Floor Y:");
                    ui.add(egui::DragValue::new(&mut self.floor_height).speed(0.1));
                });
                ui.horizontal(|ui| {
                    ui.label("Ceil  Y:");
                    ui.add(egui::DragValue::new(&mut self.ceiling_height).speed(0.1));
                });

                ui.separator();

                // ── Selected sector info ─────────────────────────────────
                if let Some((gx, gz)) = self.selected_sector {
                    ui.label(egui::RichText::new(format!("Sector ({}, {})", gx, gz)).strong());

                    // Read-only sector info
                    if let Some(room) = editor.current_level.as_ref()
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
                            let wall_count = sector.walls_north.len()
                                + sector.walls_east.len()
                                + sector.walls_south.len()
                                + sector.walls_west.len();
                            ui.label(format!("Walls: {}", wall_count));
                            let obj_count = room.objects.iter()
                                .filter(|o| o.sector_x == gx && o.sector_z == gz)
                                .count();
                            ui.label(format!("Objects: {}", obj_count));
                        } else {
                            ui.weak("Empty sector");
                        }
                    }

                    // In-place floor/ceiling height editing
                    ui.separator();
                    ui.label(egui::RichText::new("Edit sector").small().color(theme::TEXT_DIM));

                    let mut changed = false;
                    // We need mutable access — extract values first
                    let (cur_floor, cur_ceil) = editor.current_level.as_ref()
                        .and_then(|l| l.rooms.get(self.current_room))
                        .and_then(|r| r.get_sector(gx, gz))
                        .map(|s| (
                            s.floor.as_ref().map(|f| f.avg_height()),
                            s.ceiling.as_ref().map(|c| c.avg_height()),
                        ))
                        .unwrap_or((None, None));

                    if let Some(mut fy) = cur_floor {
                        let before = fy;
                        ui.horizontal(|ui| {
                            ui.label("Floor Y:");
                            ui.add(egui::DragValue::new(&mut fy).speed(0.05));
                        });
                        if (fy - before).abs() > 1e-5 {
                            if let Some(sector) = editor.current_level.as_mut()
                                .and_then(|l| l.rooms.get_mut(self.current_room))
                                .and_then(|r| r.sectors.get_mut(gx))
                                .and_then(|col| col.get_mut(gz))
                                .and_then(|s| s.as_mut())
                            {
                                if let Some(floor) = &mut sector.floor {
                                    floor.heights = [fy; 4];
                                    changed = true;
                                }
                            }
                        }
                    }
                    if let Some(mut cy) = cur_ceil {
                        let before = cy;
                        ui.horizontal(|ui| {
                            ui.label("Ceil  Y:");
                            ui.add(egui::DragValue::new(&mut cy).speed(0.05));
                        });
                        if (cy - before).abs() > 1e-5 {
                            if let Some(sector) = editor.current_level.as_mut()
                                .and_then(|l| l.rooms.get_mut(self.current_room))
                                .and_then(|r| r.sectors.get_mut(gx))
                                .and_then(|col| col.get_mut(gz))
                                .and_then(|s| s.as_mut())
                            {
                                if let Some(ceil) = &mut sector.ceiling {
                                    ceil.heights = [cy; 4];
                                    changed = true;
                                }
                            }
                        }
                    }
                    let _ = changed; // viewport rebuild on save

                    ui.separator();
                } else {
                    ui.weak("Click a sector to select it");
                    ui.separator();
                }

                // ── Objects in current room ──────────────────────────────
                ui.label(egui::RichText::new("Objects").small().color(theme::TEXT_DIM));

                let room_idx = self.current_room;
                let obj_count = editor.current_level.as_ref()
                    .and_then(|l| l.rooms.get(room_idx))
                    .map(|r| r.objects.len())
                    .unwrap_or(0);

                let mut remove_obj: Option<usize> = None;
                egui::ScrollArea::vertical()
                    .id_salt("we_objects")
                    .max_height(160.0)
                    .show(ui, |ui| {
                        for i in 0..obj_count {
                            let obj = editor.current_level.as_ref()
                                .and_then(|l| l.rooms.get(room_idx))
                                .and_then(|r| r.objects.get(i));
                            if let Some(obj) = obj {
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
                    editor.push_level_undo();
                    if let Some(room) = editor.current_level.as_mut()
                        .and_then(|l| l.rooms.get_mut(room_idx))
                    {
                        if idx < room.objects.len() {
                            room.objects.remove(idx);
                        }
                    }
                }

                ui.separator();
                if ui.button("Save Level").clicked() {
                    editor.request_action(EditorAction::SaveLevel);
                }
            });
    }

    // -----------------------------------------------------------------------
    // Center: 2D grid view (the main editing canvas)
    // -----------------------------------------------------------------------
    fn draw_grid_view(&mut self, ctx: &egui::Context, editor: &mut EditorContext) {
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(theme::GRID_BG))
            .show(ctx, |ui| {
                let available = ui.available_rect_before_wrap();
                let (rect, response) = ui.allocate_exact_size(available.size(), Sense::click_and_drag());

                let mouse_pos = response.hover_pos();
                let painter = ui.painter_at(rect);

                // -- Pan with right drag --
                if response.dragged_by(egui::PointerButton::Secondary) {
                    self.grid_offset += response.drag_delta();
                }

                // -- Zoom with scroll (matches v1: 0.002..2.0 px/world-unit) --
                let scroll = ui.input(|i| i.raw_scroll_delta.y);
                if scroll.abs() > 0.0 && rect.contains(mouse_pos.unwrap_or(Pos2::ZERO)) {
                    let factor = 1.0 + (scroll * 0.008) as f64;
                    self.grid_zoom = (self.grid_zoom * factor).clamp(0.002, 2.0);
                }

                let center = rect.center() + self.grid_offset;
                let scale = self.grid_zoom as f32;
                let view_mode = self.view_mode;

                // World ↔ screen helpers (same math as v1)
                let w2s = |wa: f32, wb: f32| -> Pos2 {
                    Pos2::new(center.x + wa * scale, center.y - wb * scale)
                };
                let s2w = |pos: Pos2| -> (f32, f32) {
                    ((pos.x - center.x) / scale, -(pos.y - center.y) / scale)
                };

                // Draw grid lines
                if self.show_grid {
                    self.draw_grid_lines(&painter, rect, &w2s, &s2w);
                }

                // Draw rooms from the level
                if let Some(level) = &editor.current_level {
                    let hovered = self.compute_hover(mouse_pos, level, &s2w);
                    self.hovered_sector = hovered;

                    // Compute hovered edge for wall tool
                    if self.tool == EditorTool::DrawWall {
                        self.hovered_edge = hovered.and_then(|(gx, gz)| {
                            level.rooms.get(self.current_room).and_then(|room| {
                                self.compute_edge_hover(mouse_pos, room, gx, gz, &w2s)
                            })
                        });
                    } else {
                        self.hovered_edge = None;
                    }

                    self.draw_rooms(&painter, level, &w2s, view_mode);

                    // Handle click — apply tool
                    if response.clicked() {
                        if let Some((gx, gz)) = hovered {
                            self.selected_sector = Some((gx, gz));
                            self.apply_tool_at(editor, gx, gz);
                        } else {
                            self.selected_sector = None;
                        }
                    }
                    if response.dragged_by(egui::PointerButton::Primary) {
                        if let Some((gx, gz)) = hovered {
                            // Wall tool: only apply on drag if edge is hovered
                            if self.tool != EditorTool::DrawWall || self.hovered_edge.is_some() {
                                self.apply_tool_at(editor, gx, gz);
                            }
                        }
                    }
                } else {
                    // No level — show hint
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "No level loaded. Use Level > New Level.",
                        egui::FontId::proportional(14.0),
                        theme::TEXT_DIM,
                    );
                }

                // Keyboard shortcuts
                ctx.input(|i| {
                    if i.key_pressed(egui::Key::S) { self.tool = EditorTool::Select; }
                    if i.key_pressed(egui::Key::F) { self.tool = EditorTool::DrawFloor; }
                    if i.key_pressed(egui::Key::C) { self.tool = EditorTool::DrawCeiling; }
                    if i.key_pressed(egui::Key::W) { self.tool = EditorTool::DrawWall; }
                    if i.key_pressed(egui::Key::O) { self.tool = EditorTool::PlaceObject; }
                    if i.key_pressed(egui::Key::E) { self.tool = EditorTool::Erase; }
                });

                // Status bar at bottom
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
            });
    }

    // -----------------------------------------------------------------------
    // Grid lines
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

        // Vertical lines
        let start = (min_wa / step).floor() * step;
        let mut wa = start;
        while wa <= max_wa + step {
            let p0 = w2s(wa, max_wb);
            let p1 = w2s(wa, min_wb);
            let color = if wa.abs() < step * 0.01 {
                theme::GRID_AXIS_X
            } else {
                theme::GRID_LINE
            };
            painter.line_segment([p0, p1], Stroke::new(1.0, color));
            wa += step;
        }

        // Horizontal lines
        let start = (min_wb / step).floor() * step;
        let mut wb = start;
        while wb <= max_wb + step {
            let p0 = w2s(min_wa, wb);
            let p1 = w2s(max_wa, wb);
            let color = if wb.abs() < step * 0.01 {
                theme::GRID_AXIS_Z
            } else {
                theme::GRID_LINE
            };
            painter.line_segment([p0, p1], Stroke::new(1.0, color));
            wb += step;
        }
    }

    // -----------------------------------------------------------------------
    // Room + sector rendering
    // -----------------------------------------------------------------------
    fn draw_rooms(
        &self,
        painter: &Painter,
        level: &Level,
        w2s: &impl Fn(f32, f32) -> Pos2,
        view_mode: GridViewMode,
    ) {
        // Draw non-current rooms first (dimmed), then current room on top
        for pass in 0..2usize {
            for (room_idx, room) in level.rooms.iter().enumerate() {
                let is_current = room_idx == self.current_room;
                if (pass == 0) == is_current { continue; }
                if self.hidden_rooms.contains(&room_idx) { continue; }
                self.draw_room(painter, room, room_idx, is_current, w2s, view_mode);
            }
        }
    }

    fn draw_room(
        &self,
        painter: &Painter,
        room: &Room,
        _room_idx: usize,
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

            // Floor fill
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
            let fill = dim_color(fill, dim);
            painter.rect_filled(rect, 0.0, fill);

            // Walls — draw colored edges
            let wall_stroke = Stroke::new(2.0, dim_color(theme::WALL_COLOR, dim));
            if !sector.walls_north.is_empty() {
                painter.line_segment([rect.left_top(), rect.right_top()], wall_stroke);
            }
            if !sector.walls_south.is_empty() {
                painter.line_segment([rect.left_bottom(), rect.right_bottom()], wall_stroke);
            }
            if !sector.walls_west.is_empty() {
                painter.line_segment([rect.left_top(), rect.left_bottom()], wall_stroke);
            }
            if !sector.walls_east.is_empty() {
                painter.line_segment([rect.right_top(), rect.right_bottom()], wall_stroke);
            }

            // Border
            let border_col = dim_color(theme::SECTOR_BORDER, dim);
            painter.rect_stroke(rect, 0.0, Stroke::new(1.0, border_col), StrokeKind::Middle);

            // Hover highlight
            if is_current && self.hovered_sector == Some((gx, gz)) {
                painter.rect_stroke(rect, 0.0, Stroke::new(2.0, theme::SECTOR_HOVER), StrokeKind::Middle);
            }

            // Wall tool: hovered edge highlight
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

            // Selection highlight
            if is_current && self.selected_sector == Some((gx, gz)) {
                painter.rect_stroke(rect, 0.0, Stroke::new(2.0, theme::SECTOR_SELECT), StrokeKind::Middle);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Edge hover detection (for DrawWall tool)
    // -----------------------------------------------------------------------
    /// Returns which edge of sector (gx, gz) the mouse is near, in screen space.
    /// A threshold of 10 pixels is used for each edge.
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

        // Four corner positions in screen space
        let tl = w2s(bx,               bz + SECTOR_SIZE);  // top-left  (north-west)
        let tr = w2s(bx + SECTOR_SIZE, bz + SECTOR_SIZE);  // top-right (north-east)
        let bl = w2s(bx,               bz);                // bot-left  (south-west)
        let br = w2s(bx + SECTOR_SIZE, bz);                // bot-right (south-east)

        // In egui Y increases downward, and our w2s flips Y, so:
        //   "top" of rect = North, "bottom" = South (same as the draw_room rect)

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
    // Hover detection
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

    // -----------------------------------------------------------------------
    // Tool application
    // -----------------------------------------------------------------------
    fn apply_tool_at(&self, editor: &mut EditorContext, gx: usize, gz: usize) {
        if self.tool == EditorTool::Select { return; }

        // Snapshot for undo before mutating
        editor.push_level_undo();

        let Some(level) = editor.current_level.as_mut() else { return };
        let Some(room) = level.rooms.get_mut(self.current_room) else { return };
        let tex = self.selected_texture.clone();

        match self.tool {
            EditorTool::DrawFloor => {
                if gx < room.width && gz < room.depth {
                    let slot = &mut room.sectors[gx][gz];
                    let s = slot.get_or_insert_with(Sector::default);
                    s.floor = Some(HorizontalFace::flat(self.floor_height, tex));
                }
            }
            EditorTool::DrawCeiling => {
                if gx < room.width && gz < room.depth {
                    let slot = &mut room.sectors[gx][gz];
                    let s = slot.get_or_insert_with(Sector::default);
                    s.ceiling = Some(HorizontalFace::flat(self.ceiling_height, tex));
                }
            }
            EditorTool::DrawWall => {
                let Some(dir) = self.hovered_edge else { return };
                if gx < room.width && gz < room.depth {
                    let slot = &mut room.sectors[gx][gz];
                    let s = slot.get_or_insert_with(Sector::default);
                    // Use existing floor/ceil heights if the sector has them
                    let bot = s.floor.as_ref().map(|f| f.avg_height()).unwrap_or(self.floor_height);
                    let top = s.ceiling.as_ref().map(|c| c.avg_height()).unwrap_or(self.ceiling_height);
                    let wall = VerticalFace::new(bot, top, tex);
                    let walls = s.walls_mut(dir);
                    walls.clear();
                    walls.push(wall);
                }
            }
            EditorTool::PlaceObject => {
                // Place a new AssetInstance at the clicked sector (asset_id=0 means "unassigned")
                let instance = AssetInstance::new(gx, gz, 0);
                room.objects.push(instance);
            }
            EditorTool::Erase => {
                room.remove_sector(gx, gz);
            }
            EditorTool::Select => {}
        }
    }
}

impl Default for WorldEditorPanel {
    fn default() -> Self { Self::new() }
}

// ---------------------------------------------------------------------------
// Geometry helpers
// ---------------------------------------------------------------------------

/// Distance from point `p` to the closest point on segment [a, b]
fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_sq();
    if len_sq < 1e-6 {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    let closest = a + ab * t;
    (p - closest).length()
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
