//! Editor layout - TRLE-inspired panel arrangement

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext, SplitPanel, draw_panel, panel_content_rect, Toolbar, icon};
use crate::rasterizer::{Framebuffer, Texture as RasterTexture};
use super::{EditorState, EditorTool};
use super::grid_view::draw_grid_view;
use super::viewport_3d::draw_viewport_3d;
use super::texture_palette::draw_texture_palette;

/// Actions that can be triggered by the editor UI
#[derive(Debug, Clone, PartialEq)]
pub enum EditorAction {
    None,
    Play,
    New,
    Save,
    SaveAs,
    Load(String), // Path to load
    PromptLoad,   // Show file prompt
    Export,       // Browser: download as file
    Import,       // Browser: upload file
    Exit,         // Close/quit
}

/// Editor layout state (split panel ratios)
pub struct EditorLayout {
    /// Main horizontal split (left panels | center+right)
    pub main_split: SplitPanel,
    /// Right split (center viewport | right panels)
    pub right_split: SplitPanel,
    /// Left vertical split (2D grid | room properties)
    pub left_split: SplitPanel,
    /// Right vertical split (texture palette | properties)
    pub right_panel_split: SplitPanel,
}

impl EditorLayout {
    pub fn new() -> Self {
        Self {
            main_split: SplitPanel::horizontal(1).with_ratio(0.25).with_min_size(150.0),
            right_split: SplitPanel::horizontal(2).with_ratio(0.75).with_min_size(150.0),
            left_split: SplitPanel::vertical(3).with_ratio(0.6).with_min_size(100.0),
            right_panel_split: SplitPanel::vertical(4).with_ratio(0.6).with_min_size(100.0),
        }
    }

    /// Apply layout config from a level
    pub fn apply_config(&mut self, config: &crate::world::EditorLayoutConfig) {
        self.main_split.ratio = config.main_split;
        self.right_split.ratio = config.right_split;
        self.left_split.ratio = config.left_split;
        self.right_panel_split.ratio = config.right_panel_split;
    }

    /// Extract current layout as a config (for saving with level)
    pub fn to_config(&self) -> crate::world::EditorLayoutConfig {
        crate::world::EditorLayoutConfig {
            main_split: self.main_split.ratio,
            right_split: self.right_split.ratio,
            left_split: self.left_split.ratio,
            right_panel_split: self.right_panel_split.ratio,
        }
    }
}

/// Draw the complete editor UI, returns action if triggered
pub fn draw_editor(
    ctx: &mut UiContext,
    layout: &mut EditorLayout,
    state: &mut EditorState,
    textures: &[RasterTexture],
    fb: &mut Framebuffer,
    bounds: Rect,
    icon_font: Option<&Font>,
) -> EditorAction {
    let screen = bounds;

    // Single unified toolbar at top
    let toolbar_height = 36.0;
    let toolbar_rect = screen.slice_top(toolbar_height);
    let main_rect = screen.remaining_after_top(toolbar_height);

    // Status bar at bottom
    let status_height = 22.0;
    let status_rect = main_rect.slice_bottom(status_height);
    let panels_rect = main_rect.remaining_after_bottom(status_height);

    // Draw unified toolbar
    let action = draw_unified_toolbar(ctx, toolbar_rect, state, icon_font);

    // Main split: left panels | rest
    let (left_rect, rest_rect) = layout.main_split.update(ctx, panels_rect);

    // Right split: center viewport | right panels
    let (center_rect, right_rect) = layout.right_split.update(ctx, rest_rect);

    // Left split: 2D grid view | room controls
    let (grid_rect, room_props_rect) = layout.left_split.update(ctx, left_rect);

    // Right split: texture palette | face properties
    let (texture_rect, props_rect) = layout.right_panel_split.update(ctx, right_rect);

    // Draw panels
    draw_panel(grid_rect, Some("2D Grid"), Color::from_rgba(35, 35, 40, 255));
    draw_grid_view(ctx, panel_content_rect(grid_rect, true), state);

    draw_panel(room_props_rect, Some("Room"), Color::from_rgba(35, 35, 40, 255));
    draw_room_properties(ctx, panel_content_rect(room_props_rect, true), state);

    draw_panel(center_rect, Some("3D Viewport"), Color::from_rgba(25, 25, 30, 255));
    draw_viewport_3d(ctx, panel_content_rect(center_rect, true), state, textures, fb);

    draw_panel(texture_rect, Some("Textures"), Color::from_rgba(35, 35, 40, 255));
    draw_texture_palette(ctx, panel_content_rect(texture_rect, true), state, icon_font);

    draw_panel(props_rect, Some("Properties"), Color::from_rgba(35, 35, 40, 255));
    draw_properties(ctx, panel_content_rect(props_rect, true), state, icon_font);

    // Draw status bar
    draw_status_bar(status_rect, state);

    action
}

fn draw_unified_toolbar(ctx: &mut UiContext, rect: Rect, state: &mut EditorState, icon_font: Option<&Font>) -> EditorAction {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(40, 40, 45, 255));

    let mut action = EditorAction::None;
    let mut toolbar = Toolbar::new(rect);

    // File operations
    if toolbar.icon_button(ctx, icon::FILE_PLUS, icon_font, "New") {
        action = EditorAction::New;
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        if toolbar.icon_button(ctx, icon::FOLDER_OPEN, icon_font, "Open") {
            action = EditorAction::PromptLoad;
        }
        if toolbar.icon_button(ctx, icon::SAVE, icon_font, "Save") {
            action = EditorAction::Save;
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        if toolbar.icon_button(ctx, icon::FOLDER_OPEN, icon_font, "Upload") {
            action = EditorAction::Import;
        }
        if toolbar.icon_button(ctx, icon::SAVE, icon_font, "Download") {
            action = EditorAction::Export;
        }
    }

    toolbar.separator();

    // Edit operations
    if toolbar.icon_button(ctx, icon::UNDO, icon_font, "Undo") {
        state.undo();
    }
    if toolbar.icon_button(ctx, icon::REDO, icon_font, "Redo") {
        state.redo();
    }

    toolbar.separator();

    // Play button
    if toolbar.icon_button(ctx, icon::PLAY, icon_font, "Play") {
        action = EditorAction::Play;
    }

    toolbar.separator();

    // Tool buttons
    let tools = [
        (icon::MOVE, "Select", EditorTool::Select),
        (icon::SQUARE, "Floor", EditorTool::DrawFloor),
        (icon::BOX, "Wall", EditorTool::DrawWall),
        (icon::LAYERS, "Ceiling", EditorTool::DrawCeiling),
        (icon::DOOR_CLOSED, "Portal", EditorTool::PlacePortal),
    ];

    for (icon_char, tooltip, tool) in tools {
        let is_active = state.tool == tool;
        if toolbar.icon_button_active(ctx, icon_char, icon_font, tooltip, is_active) {
            state.tool = tool;
        }
    }

    toolbar.separator();

    // Vertex mode toggle
    let link_icon = if state.link_coincident_vertices { icon::LINK } else { icon::UNLINK };
    let link_tooltip = if state.link_coincident_vertices { "Vertices Linked" } else { "Vertices Independent" };
    if toolbar.icon_button_active(ctx, link_icon, icon_font, link_tooltip, state.link_coincident_vertices) {
        state.link_coincident_vertices = !state.link_coincident_vertices;
        let mode = if state.link_coincident_vertices { "Linked" } else { "Independent" };
        state.set_status(&format!("Vertex mode: {}", mode), 2.0);
    }

    toolbar.separator();

    // Room navigation
    toolbar.label(&format!("Room: {}", state.current_room));

    if toolbar.icon_button(ctx, icon::CIRCLE_CHEVRON_LEFT, icon_font, "Previous Room") {
        if state.current_room > 0 {
            state.current_room -= 1;
        }
    }
    if toolbar.icon_button(ctx, icon::CIRCLE_CHEVRON_RIGHT, icon_font, "Next Room") {
        if state.current_room + 1 < state.level.rooms.len() {
            state.current_room += 1;
        }
    }
    if toolbar.icon_button(ctx, icon::PLUS, icon_font, "Add Room") {
        // TODO: Add new room
        println!("Add room clicked");
    }

    toolbar.separator();

    // PS1 effect toggles
    if toolbar.icon_button_active(ctx, icon::WAVES, icon_font, "Affine Textures (PS1 warp)", state.raster_settings.affine_textures) {
        state.raster_settings.affine_textures = !state.raster_settings.affine_textures;
        let mode = if state.raster_settings.affine_textures { "ON" } else { "OFF" };
        state.set_status(&format!("Affine textures: {}", mode), 2.0);
    }
    if toolbar.icon_button_active(ctx, icon::MAGNET, icon_font, "Vertex Snap (PS1 jitter)", state.raster_settings.vertex_snap) {
        state.raster_settings.vertex_snap = !state.raster_settings.vertex_snap;
        let mode = if state.raster_settings.vertex_snap { "ON" } else { "OFF" };
        state.set_status(&format!("Vertex snap: {}", mode), 2.0);
    }
    if toolbar.icon_button_active(ctx, icon::SUN, icon_font, "Gouraud Shading", state.raster_settings.shading != crate::rasterizer::ShadingMode::None) {
        use crate::rasterizer::ShadingMode;
        state.raster_settings.shading = if state.raster_settings.shading == ShadingMode::None {
            ShadingMode::Gouraud
        } else {
            ShadingMode::None
        };
        let mode = if state.raster_settings.shading != ShadingMode::None { "ON" } else { "OFF" };
        state.set_status(&format!("Shading: {}", mode), 2.0);
    }
    if toolbar.icon_button_active(ctx, icon::MONITOR, icon_font, "Low Resolution (PS1 320x240)", state.raster_settings.low_resolution) {
        state.raster_settings.low_resolution = !state.raster_settings.low_resolution;
        let mode = if state.raster_settings.low_resolution { "320x240" } else { "High-res" };
        state.set_status(&format!("Resolution: {}", mode), 2.0);
    }
    if toolbar.icon_button_active(ctx, icon::BLEND, icon_font, "Dithering (PS1 color banding)", state.raster_settings.dithering) {
        state.raster_settings.dithering = !state.raster_settings.dithering;
        let mode = if state.raster_settings.dithering { "ON" } else { "OFF" };
        state.set_status(&format!("Dithering: {}", mode), 2.0);
    }

    toolbar.separator();

    // Current file label
    let file_label = match &state.current_file {
        Some(path) => {
            let name = path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "untitled".to_string());
            if state.dirty {
                format!("{}*", name)
            } else {
                name
            }
        }
        None => {
            if state.dirty {
                "untitled*".to_string()
            } else {
                "untitled".to_string()
            }
        }
    };
    toolbar.label(&file_label);

    // Keyboard shortcuts
    let ctrl = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl)
             || is_key_down(KeyCode::LeftSuper) || is_key_down(KeyCode::RightSuper);
    let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);

    if ctrl && is_key_pressed(KeyCode::N) {
        action = EditorAction::New;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if ctrl && is_key_pressed(KeyCode::O) {
            action = EditorAction::PromptLoad;
        }
        if ctrl && shift && is_key_pressed(KeyCode::S) {
            action = EditorAction::SaveAs;
        } else if ctrl && is_key_pressed(KeyCode::S) {
            action = EditorAction::Save;
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        if ctrl && is_key_pressed(KeyCode::O) {
            action = EditorAction::Import;
        }
        if ctrl && is_key_pressed(KeyCode::S) {
            action = EditorAction::Export;
        }
    }
    if ctrl && is_key_pressed(KeyCode::Z) {
        if shift {
            state.redo();
        } else {
            state.undo();
        }
    }

    action
}

fn draw_room_properties(ctx: &mut UiContext, rect: Rect, state: &mut EditorState) {
    let mut y = rect.y.floor();
    let x = rect.x.floor();
    let line_height = 20.0;

    if let Some(room) = state.current_room() {
        draw_text(&format!("ID: {}", room.id), x, (y + 14.0).floor(), 16.0, WHITE);
        y += line_height;

        draw_text(
            &format!("Pos: ({:.1}, {:.1}, {:.1})", room.position.x, room.position.y, room.position.z),
            x, (y + 14.0).floor(), 16.0, WHITE,
        );
        y += line_height;

        // Count sectors
        let sector_count = room.iter_sectors().count();
        draw_text(&format!("Size: {}x{}", room.width, room.depth), x, (y + 14.0).floor(), 16.0, WHITE);
        y += line_height;

        draw_text(&format!("Sectors: {}", sector_count), x, (y + 14.0).floor(), 16.0, WHITE);
        y += line_height;

        draw_text(&format!("Portals: {}", room.portals.len()), x, (y + 14.0).floor(), 16.0, WHITE);
        y += line_height;

        // Room list
        y += 10.0;
        draw_text("Rooms:", x, (y + 14.0).floor(), 16.0, Color::from_rgba(150, 150, 150, 255));
        y += line_height;

        for (i, room) in state.level.rooms.iter().enumerate() {
            let is_selected = i == state.current_room;
            let color = if is_selected {
                Color::from_rgba(100, 200, 100, 255)
            } else {
                WHITE
            };

            let room_btn_rect = Rect::new(x, y, rect.w - 4.0, line_height);
            if ctx.mouse.clicked(&room_btn_rect) {
                state.current_room = i;
            }

            if is_selected {
                draw_rectangle(room_btn_rect.x.floor(), room_btn_rect.y.floor(), room_btn_rect.w, room_btn_rect.h, Color::from_rgba(60, 80, 60, 255));
            }

            let sector_count = room.iter_sectors().count();
            draw_text(&format!("  Room {} ({} sectors)", room.id, sector_count), x, (y + 14.0).floor(), 16.0, color);
            y += line_height;

            if y > rect.bottom() - line_height {
                break;
            }
        }
    } else {
        draw_text("No room selected", x, (y + 14.0).floor(), 16.0, Color::from_rgba(150, 150, 150, 255));
    }
}

/// Container configuration
const CONTAINER_PADDING: f32 = 8.0;
const CONTAINER_MARGIN: f32 = 6.0;

/// Draw a container box with a colored header
fn draw_container_start(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    header_text: &str,
    header_color: Color,
) {
    let header_height = 22.0;

    // Container background
    draw_rectangle(
        x.floor(), y.floor(),
        width, height,
        Color::from_rgba(30, 30, 35, 255)
    );

    // Container border
    draw_rectangle_lines(
        x.floor(), y.floor(),
        width, height,
        1.0,
        Color::from_rgba(60, 60, 70, 255)
    );

    // Header background
    draw_rectangle(
        x.floor(), y.floor(),
        width, header_height,
        Color::from_rgba(header_color.r as u8 / 4, header_color.g as u8 / 4, header_color.b as u8 / 4, 200)
    );

    // Header text
    draw_text(header_text, (x + CONTAINER_PADDING).floor(), (y + 15.0).floor(), 14.0, header_color);
}

/// Calculate height needed for a horizontal face container
fn horizontal_face_container_height(face: &crate::world::HorizontalFace) -> f32 {
    let line_height = 18.0;
    let header_height = 22.0;
    let mut lines = 3; // texture, height, walkable
    if !face.is_flat() {
        lines += 1; // extra line for individual heights
    }
    header_height + CONTAINER_PADDING * 2.0 + (lines as f32) * line_height
}

/// Calculate height needed for a wall face container
fn wall_face_container_height(_wall: &crate::world::VerticalFace) -> f32 {
    let line_height = 18.0;
    let header_height = 22.0;
    let lines = 3; // texture, y range, blend
    header_height + CONTAINER_PADDING * 2.0 + (lines as f32) * line_height
}

/// Draw properties for a horizontal face inside a container
fn draw_horizontal_face_container(
    ctx: &mut UiContext,
    x: f32,
    y: f32,
    width: f32,
    face: &crate::world::HorizontalFace,
    label: &str,
    label_color: Color,
    room_idx: usize,
    gx: usize,
    gz: usize,
    is_floor: bool,
    state: &mut EditorState,
    icon_font: Option<&Font>,
) -> f32 {
    let line_height = 18.0;
    let header_height = 22.0;
    let container_height = horizontal_face_container_height(face);

    // Draw container
    draw_container_start(x, y, width, container_height, label, label_color);

    // Content starts after header
    let content_x = x + CONTAINER_PADDING;
    let mut content_y = y + header_height + CONTAINER_PADDING;

    // Texture
    let tex_display = if face.texture.is_valid() {
        format!("Texture: {}", face.texture.name)
    } else {
        String::from("Texture: (fallback)")
    };
    draw_text(&tex_display, content_x.floor(), (content_y + 12.0).floor(), 13.0, WHITE);
    content_y += line_height;

    // Heights
    if !face.is_flat() {
        draw_text(&format!("Heights: [{:.0}, {:.0}, {:.0}, {:.0}]",
            face.heights[0], face.heights[1], face.heights[2], face.heights[3]),
            content_x.floor(), (content_y + 12.0).floor(), 13.0, WHITE);
        content_y += line_height;
    }
    draw_text(&format!("Base: {:.0}", face.heights[0]), content_x.floor(), (content_y + 12.0).floor(), 13.0, WHITE);
    content_y += line_height;

    // Walkable icon button
    let walkable = face.walkable;
    let icon_size = 18.0;
    let btn_rect = Rect::new(content_x, content_y - 2.0, icon_size, icon_size);
    let clicked = crate::ui::icon_button_active(ctx, btn_rect, icon::FOOTPRINTS, icon_font, "Walkable", walkable);

    if clicked {
        if let Some(r) = state.level.rooms.get_mut(room_idx) {
            if let Some(s) = r.get_sector_mut(gx, gz) {
                if is_floor {
                    if let Some(f) = &mut s.floor {
                        f.walkable = !f.walkable;
                    }
                } else if let Some(c) = &mut s.ceiling {
                    c.walkable = !c.walkable;
                }
            }
        }
    }

    container_height
}

/// Draw properties for a wall face inside a container
fn draw_wall_face_container(
    x: f32,
    y: f32,
    width: f32,
    wall: &crate::world::VerticalFace,
    label: &str,
    label_color: Color,
) -> f32 {
    let line_height = 18.0;
    let header_height = 22.0;
    let container_height = wall_face_container_height(wall);

    // Draw container
    draw_container_start(x, y, width, container_height, label, label_color);

    // Content starts after header
    let content_x = x + CONTAINER_PADDING;
    let mut content_y = y + header_height + CONTAINER_PADDING;

    // Texture
    let tex_display = if wall.texture.is_valid() {
        format!("Texture: {}", wall.texture.name)
    } else {
        String::from("Texture: (fallback)")
    };
    draw_text(&tex_display, content_x.floor(), (content_y + 12.0).floor(), 13.0, WHITE);
    content_y += line_height;

    // Height range
    draw_text(&format!("Y Range: {:.0} - {:.0}", wall.y_bottom(), wall.y_top()), content_x.floor(), (content_y + 12.0).floor(), 13.0, WHITE);
    content_y += line_height;

    // Blend mode
    draw_text(&format!("Blend: {:?}", wall.blend_mode), content_x.floor(), (content_y + 12.0).floor(), 13.0, Color::from_rgba(150, 150, 150, 255));

    container_height
}

fn draw_properties(ctx: &mut UiContext, rect: Rect, state: &mut EditorState, icon_font: Option<&Font>) {
    let x = rect.x.floor();
    let container_width = rect.w - 4.0;

    // Handle scroll input
    let inside = ctx.mouse.inside(&rect);
    if inside && ctx.mouse.scroll != 0.0 {
        state.properties_scroll -= ctx.mouse.scroll * 30.0;
    }

    // Clone selection to avoid borrow issues
    let selection = state.selection.clone();

    // Calculate total content height first
    let total_height = calculate_properties_content_height(&selection, state);

    // Clamp scroll
    let max_scroll = (total_height - rect.h + 20.0).max(0.0);
    state.properties_scroll = state.properties_scroll.clamp(0.0, max_scroll);

    // Enable scissor for clipping
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

    // Start Y position with scroll offset
    let mut y = rect.y.floor() - state.properties_scroll;

    match &selection {
        super::Selection::None => {
            draw_text("Nothing selected", x, (y + 14.0).floor(), 16.0, Color::from_rgba(150, 150, 150, 255));
        }
        super::Selection::Room(idx) => {
            draw_text(&format!("Room {}", idx), x, (y + 14.0).floor(), 16.0, WHITE);
        }
        super::Selection::SectorFace { room, x: gx, z: gz, face } => {
            // Single face selected (from 3D view click)
            draw_text(&format!("Sector ({}, {})", gx, gz), x, (y + 14.0).floor(), 14.0, Color::from_rgba(150, 150, 150, 255));
            y += 24.0;

            // Get sector data
            let sector_data = state.level.rooms.get(*room)
                .and_then(|r| r.get_sector(*gx, *gz))
                .cloned();

            if let Some(sector) = sector_data {
                match face {
                    super::SectorFace::Floor => {
                        if let Some(floor) = &sector.floor {
                            let h = draw_horizontal_face_container(
                                ctx, x, y, container_width, floor, "Floor",
                                Color::from_rgba(150, 200, 255, 255),
                                *room, *gx, *gz, true, state, icon_font
                            );
                            y += h + CONTAINER_MARGIN;
                        } else {
                            draw_text("(no floor)", x, (y + 14.0).floor(), 14.0, Color::from_rgba(100, 100, 100, 255));
                        }
                    }
                    super::SectorFace::Ceiling => {
                        if let Some(ceiling) = &sector.ceiling {
                            let h = draw_horizontal_face_container(
                                ctx, x, y, container_width, ceiling, "Ceiling",
                                Color::from_rgba(200, 150, 255, 255),
                                *room, *gx, *gz, false, state, icon_font
                            );
                            y += h + CONTAINER_MARGIN;
                        } else {
                            draw_text("(no ceiling)", x, (y + 14.0).floor(), 14.0, Color::from_rgba(100, 100, 100, 255));
                        }
                    }
                    super::SectorFace::WallNorth(i) => {
                        if let Some(wall) = sector.walls_north.get(*i) {
                            let h = draw_wall_face_container(x, y, container_width, wall, "Wall (North)", Color::from_rgba(255, 180, 120, 255));
                            y += h + CONTAINER_MARGIN;
                        }
                    }
                    super::SectorFace::WallEast(i) => {
                        if let Some(wall) = sector.walls_east.get(*i) {
                            let h = draw_wall_face_container(x, y, container_width, wall, "Wall (East)", Color::from_rgba(255, 180, 120, 255));
                            y += h + CONTAINER_MARGIN;
                        }
                    }
                    super::SectorFace::WallSouth(i) => {
                        if let Some(wall) = sector.walls_south.get(*i) {
                            let h = draw_wall_face_container(x, y, container_width, wall, "Wall (South)", Color::from_rgba(255, 180, 120, 255));
                            y += h + CONTAINER_MARGIN;
                        }
                    }
                    super::SectorFace::WallWest(i) => {
                        if let Some(wall) = sector.walls_west.get(*i) {
                            let h = draw_wall_face_container(x, y, container_width, wall, "Wall (West)", Color::from_rgba(255, 180, 120, 255));
                            y += h + CONTAINER_MARGIN;
                        }
                    }
                }
            } else {
                draw_text("Sector not found", x, (y + 14.0).floor(), 14.0, Color::from_rgba(255, 100, 100, 255));
            }
        }
        super::Selection::Sector { room, x: gx, z: gz } => {
            // Whole sector selected (from 2D view click) - show all faces in containers
            draw_text(&format!("Sector ({}, {})", gx, gz), x, (y + 14.0).floor(), 16.0, Color::from_rgba(255, 200, 80, 255));
            y += 24.0;

            // Get sector data
            let sector_data = state.level.rooms.get(*room)
                .and_then(|r| r.get_sector(*gx, *gz))
                .cloned();

            if let Some(sector) = sector_data {
                // === FLOOR ===
                if let Some(floor) = &sector.floor {
                    let h = draw_horizontal_face_container(
                        ctx, x, y, container_width, floor, "Floor",
                        Color::from_rgba(150, 200, 255, 255),
                        *room, *gx, *gz, true, state, icon_font
                    );
                    y += h + CONTAINER_MARGIN;
                }

                // === CEILING ===
                if let Some(ceiling) = &sector.ceiling {
                    let h = draw_horizontal_face_container(
                        ctx, x, y, container_width, ceiling, "Ceiling",
                        Color::from_rgba(200, 150, 255, 255),
                        *room, *gx, *gz, false, state, icon_font
                    );
                    y += h + CONTAINER_MARGIN;
                }

                // === WALLS ===
                let wall_dirs: [(&str, &Vec<crate::world::VerticalFace>); 4] = [
                    ("North", &sector.walls_north),
                    ("East", &sector.walls_east),
                    ("South", &sector.walls_south),
                    ("West", &sector.walls_west),
                ];

                for (dir_name, walls) in wall_dirs {
                    for (i, wall) in walls.iter().enumerate() {
                        let label = if walls.len() == 1 {
                            format!("Wall ({})", dir_name)
                        } else {
                            format!("Wall ({}) [{}]", dir_name, i)
                        };
                        let h = draw_wall_face_container(x, y, container_width, wall, &label, Color::from_rgba(255, 180, 120, 255));
                        y += h + CONTAINER_MARGIN;
                    }
                }
            } else {
                draw_text("Sector not found", x, (y + 14.0).floor(), 14.0, Color::from_rgba(255, 100, 100, 255));
            }
        }
        super::Selection::Portal { room, portal } => {
            draw_text(&format!("Portal {} in Room {}", portal, room), x, (y + 14.0).floor(), 16.0, WHITE);
        }
        super::Selection::Edge { room, x: gx, z: gz, face_idx, edge_idx, wall_face } => {
            // Determine face name based on type
            let face_name = if *face_idx == 0 {
                "Floor".to_string()
            } else if *face_idx == 1 {
                "Ceiling".to_string()
            } else if let Some(wf) = wall_face {
                match wf {
                    super::SectorFace::WallNorth(_) => "Wall North".to_string(),
                    super::SectorFace::WallEast(_) => "Wall East".to_string(),
                    super::SectorFace::WallSouth(_) => "Wall South".to_string(),
                    super::SectorFace::WallWest(_) => "Wall West".to_string(),
                    _ => "Wall".to_string(),
                }
            } else {
                "Wall".to_string()
            };

            // Edge names differ for walls vs floor/ceiling
            let edge_name = if *face_idx == 2 {
                // Wall edges: bottom, right, top, left
                match edge_idx {
                    0 => "Bottom",
                    1 => "Right",
                    2 => "Top",
                    _ => "Left",
                }
            } else {
                // Floor/ceiling edges: north, east, south, west
                match edge_idx {
                    0 => "North",
                    1 => "East",
                    2 => "South",
                    _ => "West",
                }
            };
            draw_text(&format!("{} Edge ({})", face_name, edge_name), x, (y + 14.0).floor(), 16.0, WHITE);
            y += 24.0;

            // Get vertex coordinates
            if let Some(room_data) = state.level.rooms.get(*room) {
                if let Some(sector) = room_data.get_sector(*gx, *gz) {
                    let base_x = room_data.position.x + (*gx as f32) * crate::world::SECTOR_SIZE;
                    let base_z = room_data.position.z + (*gz as f32) * crate::world::SECTOR_SIZE;

                    // Get heights based on face type
                    let heights = if *face_idx == 0 {
                        sector.floor.as_ref().map(|f| f.heights)
                    } else if *face_idx == 1 {
                        sector.ceiling.as_ref().map(|c| c.heights)
                    } else if let Some(wf) = wall_face {
                        // Get wall heights
                        match wf {
                            super::SectorFace::WallNorth(i) => sector.walls_north.get(*i).map(|w| w.heights),
                            super::SectorFace::WallEast(i) => sector.walls_east.get(*i).map(|w| w.heights),
                            super::SectorFace::WallSouth(i) => sector.walls_south.get(*i).map(|w| w.heights),
                            super::SectorFace::WallWest(i) => sector.walls_west.get(*i).map(|w| w.heights),
                            _ => None,
                        }
                    } else {
                        None
                    };

                    if let Some(h) = heights {
                        let corner0 = *edge_idx;
                        let corner1 = (*edge_idx + 1) % 4;

                        // Get corner positions - for walls these are different
                        if *face_idx == 2 {
                            // Wall corners: heights are [bottom-left, bottom-right, top-right, top-left]
                            draw_text("Vertex 1:", x, (y + 12.0).floor(), 13.0, Color::from_rgba(150, 150, 150, 255));
                            y += 18.0;
                            draw_text(&format!("  Height: {:.0}", h[corner0]),
                                x, (y + 12.0).floor(), 13.0, WHITE);
                            y += 18.0;

                            draw_text("Vertex 2:", x, (y + 12.0).floor(), 13.0, Color::from_rgba(150, 150, 150, 255));
                            y += 18.0;
                            draw_text(&format!("  Height: {:.0}", h[corner1]),
                                x, (y + 12.0).floor(), 13.0, WHITE);
                        } else {
                            // Floor/ceiling corners
                            let corners = [
                                (base_x, base_z),                                           // NW - 0
                                (base_x + crate::world::SECTOR_SIZE, base_z),               // NE - 1
                                (base_x + crate::world::SECTOR_SIZE, base_z + crate::world::SECTOR_SIZE), // SE - 2
                                (base_x, base_z + crate::world::SECTOR_SIZE),               // SW - 3
                            ];

                            draw_text("Vertex 1:", x, (y + 12.0).floor(), 13.0, Color::from_rgba(150, 150, 150, 255));
                            y += 18.0;
                            draw_text(&format!("  X: {:.0}  Z: {:.0}  Y: {:.0}", corners[corner0].0, corners[corner0].1, h[corner0]),
                                x, (y + 12.0).floor(), 13.0, WHITE);
                            y += 18.0;

                            draw_text("Vertex 2:", x, (y + 12.0).floor(), 13.0, Color::from_rgba(150, 150, 150, 255));
                            y += 18.0;
                            draw_text(&format!("  X: {:.0}  Z: {:.0}  Y: {:.0}", corners[corner1].0, corners[corner1].1, h[corner1]),
                                x, (y + 12.0).floor(), 13.0, WHITE);
                        }
                    }
                }
            }
        }
    }

    // Disable scissor
    unsafe {
        get_internal_gl().quad_gl.scissor(None);
    }

    // Draw scroll indicator if content overflows
    if total_height > rect.h {
        let scrollbar_height = (rect.h / total_height) * rect.h;
        let scrollbar_y = rect.y + (state.properties_scroll / max_scroll) * (rect.h - scrollbar_height);
        let scrollbar_x = rect.right() - 4.0;

        // Track background
        draw_rectangle(scrollbar_x - 1.0, rect.y, 5.0, rect.h, Color::from_rgba(20, 20, 25, 255));
        // Scrollbar thumb
        draw_rectangle(scrollbar_x, scrollbar_y, 3.0, scrollbar_height, Color::from_rgba(80, 80, 90, 255));
    }
}

/// Calculate total content height for properties panel (for scroll bounds)
fn calculate_properties_content_height(selection: &super::Selection, state: &EditorState) -> f32 {
    let header_height = 24.0;

    match selection {
        super::Selection::None | super::Selection::Room(_) | super::Selection::Portal { .. } => 30.0,

        super::Selection::Edge { .. } => 120.0, // Edge header + 2 vertex coords

        super::Selection::SectorFace { room, x: gx, z: gz, face } => {
            let sector_data = state.level.rooms.get(*room)
                .and_then(|r| r.get_sector(*gx, *gz));

            let mut height = header_height;

            if let Some(sector) = sector_data {
                match face {
                    super::SectorFace::Floor => {
                        if let Some(floor) = &sector.floor {
                            height += horizontal_face_container_height(floor) + CONTAINER_MARGIN;
                        }
                    }
                    super::SectorFace::Ceiling => {
                        if let Some(ceiling) = &sector.ceiling {
                            height += horizontal_face_container_height(ceiling) + CONTAINER_MARGIN;
                        }
                    }
                    super::SectorFace::WallNorth(i) => {
                        if let Some(wall) = sector.walls_north.get(*i) {
                            height += wall_face_container_height(wall) + CONTAINER_MARGIN;
                        }
                    }
                    super::SectorFace::WallEast(i) => {
                        if let Some(wall) = sector.walls_east.get(*i) {
                            height += wall_face_container_height(wall) + CONTAINER_MARGIN;
                        }
                    }
                    super::SectorFace::WallSouth(i) => {
                        if let Some(wall) = sector.walls_south.get(*i) {
                            height += wall_face_container_height(wall) + CONTAINER_MARGIN;
                        }
                    }
                    super::SectorFace::WallWest(i) => {
                        if let Some(wall) = sector.walls_west.get(*i) {
                            height += wall_face_container_height(wall) + CONTAINER_MARGIN;
                        }
                    }
                }
            }
            height
        }

        super::Selection::Sector { room, x: gx, z: gz } => {
            let sector_data = state.level.rooms.get(*room)
                .and_then(|r| r.get_sector(*gx, *gz));

            let mut height = header_height;

            if let Some(sector) = sector_data {
                if let Some(floor) = &sector.floor {
                    height += horizontal_face_container_height(floor) + CONTAINER_MARGIN;
                }
                if let Some(ceiling) = &sector.ceiling {
                    height += horizontal_face_container_height(ceiling) + CONTAINER_MARGIN;
                }
                for wall in &sector.walls_north {
                    height += wall_face_container_height(wall) + CONTAINER_MARGIN;
                }
                for wall in &sector.walls_east {
                    height += wall_face_container_height(wall) + CONTAINER_MARGIN;
                }
                for wall in &sector.walls_south {
                    height += wall_face_container_height(wall) + CONTAINER_MARGIN;
                }
                for wall in &sector.walls_west {
                    height += wall_face_container_height(wall) + CONTAINER_MARGIN;
                }
            }
            height
        }
    }
}

fn draw_status_bar(rect: Rect, state: &EditorState) {
    draw_rectangle(rect.x.floor(), rect.y.floor(), rect.w, rect.h, Color::from_rgba(40, 40, 45, 255));

    // Show status message in center if available
    if let Some(msg) = state.get_status() {
        let msg_width = msg.len() as f32 * 8.0;
        let center_x = rect.x + rect.w * 0.5 - msg_width * 0.5;
        draw_text(&msg, center_x.floor(), (rect.y + 15.0).floor(), 16.0, Color::from_rgba(100, 255, 100, 255));
    }

    // Show keyboard shortcuts hint on the right (platform-specific)
    #[cfg(not(target_arch = "wasm32"))]
    let hints = "Ctrl+S: Save | Ctrl+Shift+S: Save As | Ctrl+O: Open | Ctrl+N: New";
    #[cfg(target_arch = "wasm32")]
    let hints = "Ctrl+S: Download | Ctrl+O: Upload | Ctrl+N: New";

    let hint_width = hints.len() as f32 * 6.0; // Approximate width
    draw_text(
        hints,
        (rect.right() - hint_width - 8.0).floor(),
        (rect.y + 15.0).floor(),
        14.0,
        Color::from_rgba(100, 100, 100, 255),
    );
}
