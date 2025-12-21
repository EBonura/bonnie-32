//! Editor layout - TRLE-inspired panel arrangement

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext, SplitPanel, draw_panel, panel_content_rect, Toolbar, icon, draw_knob, draw_ps1_color_picker, ps1_color_picker_height};
use crate::rasterizer::{Framebuffer, Texture as RasterTexture};
use super::{EditorState, EditorTool, SECTOR_SIZE};
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
    Load(String),   // Path to load
    PromptLoad,     // Show file prompt
    Export,         // Browser: download as file
    Import,         // Browser: upload file
    BrowseExamples, // Open example browser
    Exit,           // Close/quit
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
        // Use high IDs (1000+) to avoid collision with toolbar button IDs
        // which are auto-generated starting from 1 via ctx.next_id()
        Self {
            main_split: SplitPanel::horizontal(1000).with_ratio(0.25).with_min_size(150.0),
            right_split: SplitPanel::horizontal(1001).with_ratio(0.75).with_min_size(150.0),
            left_split: SplitPanel::vertical(1002).with_ratio(0.6).with_min_size(100.0),
            right_panel_split: SplitPanel::vertical(1003).with_ratio(0.6).with_min_size(100.0),
        }
    }

    /// Apply layout config from a level (panel splits only)
    pub fn apply_config(&mut self, config: &crate::world::EditorLayoutConfig) {
        self.main_split.ratio = config.main_split;
        self.right_split.ratio = config.right_split;
        self.left_split.ratio = config.left_split;
        self.right_panel_split.ratio = config.right_panel_split;
    }

    /// Extract current layout as a config (for saving with level)
    /// Takes grid view and orbit camera state from EditorState since they're stored there
    pub fn to_config(
        &self,
        grid_offset_x: f32,
        grid_offset_y: f32,
        grid_zoom: f32,
        orbit_target: crate::rasterizer::Vec3,
        orbit_distance: f32,
        orbit_azimuth: f32,
        orbit_elevation: f32,
    ) -> crate::world::EditorLayoutConfig {
        crate::world::EditorLayoutConfig {
            main_split: self.main_split.ratio,
            right_split: self.right_split.ratio,
            left_split: self.left_split.ratio,
            right_panel_split: self.right_panel_split.ratio,
            grid_offset_x,
            grid_offset_y,
            grid_zoom,
            orbit_target_x: orbit_target.x,
            orbit_target_y: orbit_target.y,
            orbit_target_z: orbit_target.z,
            orbit_distance,
            orbit_azimuth,
            orbit_elevation,
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
    draw_room_properties(ctx, panel_content_rect(room_props_rect, true), state, icon_font);

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
        if toolbar.icon_button(ctx, icon::SAVE_AS, icon_font, "Save As") {
            action = EditorAction::SaveAs;
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

    // Level browser (works on both native and WASM)
    if toolbar.icon_button(ctx, icon::BOOK_OPEN, icon_font, "Browse") {
        action = EditorAction::BrowseExamples;
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
        (icon::MAP_PIN, "Object", EditorTool::PlaceObject),
    ];

    for (icon_char, tooltip, tool) in tools {
        let is_active = state.tool == tool;
        if toolbar.icon_button_active(ctx, icon_char, icon_font, tooltip, is_active) {
            state.tool = tool;
        }
    }

    // Object type picker (only show when PlaceObject tool is active)
    // Shows "< Type >" with arrows to cycle through object types
    if state.tool == EditorTool::PlaceObject {
        toolbar.separator();

        use crate::world::{ObjectType, SpawnPointType};
        let object_types: [(&str, ObjectType); 6] = [
            ("Player", ObjectType::Spawn(SpawnPointType::PlayerStart)),
            ("Checkpoint", ObjectType::Spawn(SpawnPointType::Checkpoint)),
            ("Enemy", ObjectType::Spawn(SpawnPointType::Enemy)),
            ("Item", ObjectType::Spawn(SpawnPointType::Item)),
            ("Light", ObjectType::Light {
                color: crate::rasterizer::Color::new(255, 240, 200),
                intensity: 1.0,
                radius: 2048.0,
            }),
            ("Trigger", ObjectType::Trigger {
                trigger_id: String::new(),
                trigger_type: "on_enter".to_string(),
            }),
        ];

        // Find current index
        let current_idx = object_types.iter().position(|(_, obj_type)| {
            std::mem::discriminant(&state.selected_object_type) == std::mem::discriminant(obj_type)
        }).unwrap_or(0);

        // Draw "< Type >" navigation
        if toolbar.arrow_picker(ctx, icon_font, object_types[current_idx].0, &mut |delta: i32| {
            let new_idx = (current_idx as i32 + delta).rem_euclid(object_types.len() as i32) as usize;
            state.selected_object_type = object_types[new_idx].1.clone();
        }) {
            // Arrow was clicked, selection already changed via callback
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

    // Camera mode toggle
    use super::CameraMode;
    let is_free = state.camera_mode == CameraMode::Free;
    let is_orbit = state.camera_mode == CameraMode::Orbit;

    if toolbar.icon_button_active(ctx, icon::EYE, icon_font, "Free Camera (WASD)", is_free) {
        state.camera_mode = CameraMode::Free;
        state.set_status("Camera: Free (WASD + mouse)", 2.0);
    }
    if toolbar.icon_button_active(ctx, icon::ORBIT, icon_font, "Orbit Camera", is_orbit) {
        state.camera_mode = CameraMode::Orbit;
        // Update orbit target based on current selection
        state.update_orbit_target();
        state.sync_camera_from_orbit();
        state.set_status("Camera: Orbit (drag to rotate)", 2.0);
    }

    // Room boundaries toggle
    if toolbar.icon_button_active(ctx, icon::BOX, icon_font, "Show Room Bounds", state.show_room_bounds) {
        state.show_room_bounds = !state.show_room_bounds;
        let mode = if state.show_room_bounds { "visible" } else { "hidden" };
        state.set_status(&format!("Room boundaries: {}", mode), 2.0);
    }

    // Wireframe overlay toggle
    if toolbar.icon_button_active(ctx, icon::GRID, icon_font, "Wireframe Overlay", state.raster_settings.wireframe_overlay) {
        state.raster_settings.wireframe_overlay = !state.raster_settings.wireframe_overlay;
        let mode = if state.raster_settings.wireframe_overlay { "ON" } else { "OFF" };
        state.set_status(&format!("Wireframe overlay: {}", mode), 2.0);
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
        // Create new room offset from existing rooms
        let new_id = state.level.rooms.len();

        // Calculate position: offset from the last room or origin
        let offset_x = if let Some(last_room) = state.level.rooms.last() {
            // Place new room to the east of the last room
            last_room.position.x + (last_room.width as f32) * SECTOR_SIZE + SECTOR_SIZE
        } else {
            0.0
        };

        let new_room = crate::world::Room::new(
            new_id,
            crate::rasterizer::Vec3::new(offset_x, 0.0, 0.0),
            1, // 1x1 grid to start
            1,
        );

        state.save_undo();
        state.level.rooms.push(new_room);
        state.current_room = new_id;
        state.set_status(&format!("Created Room {}", new_id), 2.0);
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
    if toolbar.icon_button_active(ctx, icon::PROPORTIONS, icon_font, "Aspect Ratio (4:3 / Stretch)", !state.raster_settings.stretch_to_fill) {
        state.raster_settings.stretch_to_fill = !state.raster_settings.stretch_to_fill;
        let mode = if state.raster_settings.stretch_to_fill { "Stretch" } else { "4:3" };
        state.set_status(&format!("Aspect Ratio: {}", mode), 2.0);
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

fn draw_room_properties(ctx: &mut UiContext, rect: Rect, state: &mut EditorState, icon_font: Option<&Font>) {
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

        // Count lights in this room from level objects
        let light_count = state.level.objects.iter()
            .filter(|obj| obj.room == state.current_room && obj.is_light())
            .count();
        draw_text(&format!("Lights: {}", light_count), x, (y + 14.0).floor(), 16.0, WHITE);
        y += line_height;

        // Ambient light knob
        y += 8.0; // Extra space before knob
        let knob_radius = 20.0;
        let knob_center_x = x + rect.w / 2.0 - 4.0; // Center in panel
        let knob_center_y = y + knob_radius + 12.0; // Account for label above

        // Convert ambient (0.0-1.0) to knob value (0-127)
        // Scale so 100% ambient = 127 (max knob position)
        let ambient_value = (room.ambient * 127.0).round() as u8;
        let knob_result = draw_knob(
            ctx,
            knob_center_x,
            knob_center_y,
            knob_radius,
            ambient_value.min(127),
            "Ambient",
            false, // Not bipolar
            false, // Not editing (no text entry mode for now)
        );

        // Apply knob changes
        if let Some(new_val) = knob_result.value {
            let clamped: u8 = new_val.min(127);
            let new_ambient = (clamped as f32) / 127.0;
            if let Some(room) = state.level.rooms.get_mut(state.current_room) {
                if (room.ambient - new_ambient).abs() > 0.001 {
                    room.ambient = new_ambient;
                }
            }
        }

        y += knob_radius * 2.0 + 30.0; // Knob height + label + value box

        // Room list
        y += 10.0;
        draw_text("Rooms:", x, (y + 14.0).floor(), 16.0, Color::from_rgba(150, 150, 150, 255));
        y += line_height;

        let icon_btn_size = 18.0;
        let num_rooms = state.level.rooms.len();

        for i in 0..num_rooms {
            let room = &state.level.rooms[i];
            let is_selected = i == state.current_room;
            let is_hidden = state.hidden_rooms.contains(&i);

            let text_color = if is_hidden {
                Color::from_rgba(80, 80, 80, 255) // Dimmed when hidden
            } else if is_selected {
                Color::from_rgba(100, 200, 100, 255)
            } else {
                WHITE
            };

            // Visibility toggle button on the left
            let vis_btn_rect = Rect::new(x, y + 1.0, icon_btn_size, icon_btn_size);
            let vis_icon = if is_hidden { icon::EYE_OFF } else { icon::EYE };
            let vis_tooltip = if is_hidden { "Show room" } else { "Hide room" };
            if crate::ui::icon_button(ctx, vis_btn_rect, vis_icon, icon_font, vis_tooltip) {
                if is_hidden {
                    state.hidden_rooms.remove(&i);
                } else {
                    state.hidden_rooms.insert(i);
                }
            }

            // Room row (clickable area to the right of visibility button)
            let room_btn_rect = Rect::new(x + icon_btn_size + 2.0, y, rect.w - icon_btn_size - 6.0, line_height);
            if ctx.mouse.clicked(&room_btn_rect) {
                state.current_room = i;
            }

            if is_selected {
                draw_rectangle(room_btn_rect.x.floor(), room_btn_rect.y.floor(), room_btn_rect.w, room_btn_rect.h, Color::from_rgba(60, 80, 60, 255));
            }

            let sector_count = room.iter_sectors().count();
            draw_text(&format!("Room {} ({} sectors)", room.id, sector_count), (x + icon_btn_size + 4.0).floor(), (y + 14.0).floor(), 16.0, text_color);
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
    let button_row_height = 24.0;
    let color_row_height = 20.0; // Color preview + label
    let uv_controls_height = 54.0; // offset row + scale row + angle row
    let color_picker_height = ps1_color_picker_height() + 54.0; // PS1 color picker widget
    let mut lines = 3; // texture, height, walkable
    if !face.is_flat() {
        lines += 1; // extra line for individual heights
    }
    // Add space for UV info, controls, buttons, color, and color picker
    let uv_lines = if face.uv.is_some() { 2 } else { 1 }; // "Custom UVs" or "Default UVs"
    header_height + CONTAINER_PADDING * 2.0 + (lines as f32) * line_height + (uv_lines as f32) * line_height + uv_controls_height + button_row_height + color_row_height + color_picker_height
}

/// Calculate height needed for a wall face container
fn wall_face_container_height(wall: &crate::world::VerticalFace) -> f32 {
    let line_height = 18.0;
    let header_height = 22.0;
    let button_row_height = 24.0;
    let color_row_height = 20.0; // Color preview + label
    let uv_controls_height = 54.0; // offset row + scale row + angle row
    let color_picker_height = ps1_color_picker_height() + 54.0; // PS1 color picker widget
    let lines = 3; // texture, y range, blend
    // Add space for UV info, controls, buttons, color, and color picker
    let uv_lines = if wall.uv.is_some() { 2 } else { 1 }; // "Custom UVs" or "Default UVs"
    header_height + CONTAINER_PADDING * 2.0 + (lines as f32) * line_height + (uv_lines as f32) * line_height + uv_controls_height + button_row_height + color_row_height + color_picker_height
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
    content_y += line_height;

    // UV coordinates display
    let uv_label_color = Color::from_rgba(150, 150, 150, 255);
    if let Some(uv) = &face.uv {
        draw_text("UV: Custom", content_x.floor(), (content_y + 12.0).floor(), 13.0, uv_label_color);
        content_y += line_height;
        // Show UV coordinates compactly
        draw_text(&format!("  [{:.2},{:.2}] [{:.2},{:.2}]", uv[0].x, uv[0].y, uv[1].x, uv[1].y),
            content_x.floor(), (content_y + 12.0).floor(), 11.0, Color::from_rgba(120, 120, 120, 255));
        content_y += line_height;
    } else {
        draw_text("UV: Default", content_x.floor(), (content_y + 12.0).floor(), 13.0, uv_label_color);
        content_y += line_height;
    }

    // UV parameter editing controls
    let controls_width = width - CONTAINER_PADDING * 2.0;
    if let Some(new_uv) = draw_uv_controls(ctx, content_x, content_y, controls_width, &face.uv, state, icon_font) {
        state.save_undo();
        if let Some(r) = state.level.rooms.get_mut(room_idx) {
            if let Some(s) = r.get_sector_mut(gx, gz) {
                if is_floor {
                    if let Some(f) = &mut s.floor { f.uv = Some(new_uv); }
                } else if let Some(c) = &mut s.ceiling { c.uv = Some(new_uv); }
            }
        }
    }
    content_y += 54.0; // Height of UV controls (3 rows * 18px)

    // UV manipulation buttons
    let btn_size = 20.0;
    let btn_spacing = 4.0;
    let mut btn_x = content_x;

    // Reset UV button
    let reset_rect = Rect::new(btn_x, content_y, btn_size, btn_size);
    if crate::ui::icon_button(ctx, reset_rect, icon::REFRESH_CW, icon_font, "Reset UV") {
        state.save_undo();
        if let Some(r) = state.level.rooms.get_mut(room_idx) {
            if let Some(s) = r.get_sector_mut(gx, gz) {
                if is_floor {
                    if let Some(f) = &mut s.floor { f.uv = None; }
                } else if let Some(c) = &mut s.ceiling { c.uv = None; }
            }
        }
    }
    btn_x += btn_size + btn_spacing;

    // Flip Horizontal button
    let flip_h_rect = Rect::new(btn_x, content_y, btn_size, btn_size);
    if crate::ui::icon_button(ctx, flip_h_rect, icon::FLIP_HORIZONTAL, icon_font, "Flip UV Horizontal") {
        state.save_undo();
        if let Some(r) = state.level.rooms.get_mut(room_idx) {
            if let Some(s) = r.get_sector_mut(gx, gz) {
                if is_floor {
                    if let Some(f) = &mut s.floor { flip_uv_horizontal(&mut f.uv); }
                } else if let Some(c) = &mut s.ceiling { flip_uv_horizontal(&mut c.uv); }
            }
        }
    }
    btn_x += btn_size + btn_spacing;

    // Flip Vertical button
    let flip_v_rect = Rect::new(btn_x, content_y, btn_size, btn_size);
    if crate::ui::icon_button(ctx, flip_v_rect, icon::FLIP_VERTICAL, icon_font, "Flip UV Vertical") {
        state.save_undo();
        if let Some(r) = state.level.rooms.get_mut(room_idx) {
            if let Some(s) = r.get_sector_mut(gx, gz) {
                if is_floor {
                    if let Some(f) = &mut s.floor { flip_uv_vertical(&mut f.uv); }
                } else if let Some(c) = &mut s.ceiling { flip_uv_vertical(&mut c.uv); }
            }
        }
    }
    btn_x += btn_size + btn_spacing;

    // Rotate 90° CW button
    let rotate_rect = Rect::new(btn_x, content_y, btn_size, btn_size);
    if crate::ui::icon_button(ctx, rotate_rect, icon::ROTATE_CW, icon_font, "Rotate UV 90° CW") {
        state.save_undo();
        if let Some(r) = state.level.rooms.get_mut(room_idx) {
            if let Some(s) = r.get_sector_mut(gx, gz) {
                if is_floor {
                    if let Some(f) = &mut s.floor { rotate_uv_cw(&mut f.uv); }
                } else if let Some(c) = &mut s.ceiling { rotate_uv_cw(&mut c.uv); }
            }
        }
    }
    content_y += btn_size + 4.0;

    // Face vertex colors (PS1-style texture modulation)
    // Show 4 vertex color swatches in a 2x2 grid matching the face corners
    let swatch_size = 14.0;
    let swatch_spacing = 2.0;

    // Label
    let is_uniform = face.has_uniform_color();
    let color_text = if is_uniform {
        let c = face.colors[0];
        if c.r == 128 && c.g == 128 && c.b == 128 {
            String::from("Tint: Neutral")
        } else {
            format!("Tint: ({}, {}, {})", c.r, c.g, c.b)
        }
    } else {
        String::from("Tint: Per-vertex")
    };
    draw_text(&color_text, content_x.floor(), (content_y + 12.0).floor(), 12.0,
        macroquad::color::Color::from_rgba(180, 180, 180, 255));

    // Draw 4 vertex color swatches in 2x2 grid (NW, NE / SW, SE layout)
    let grid_x = content_x + 90.0;
    let vertex_labels = ["NW", "NE", "SW", "SE"];
    let grid_positions = [(0, 0), (1, 0), (0, 1), (1, 1)]; // (col, row)
    let vertex_indices = [0, 1, 3, 2]; // Map grid to corner indices: NW=0, NE=1, SE=2, SW=3

    for (grid_idx, &(col, row)) in grid_positions.iter().enumerate() {
        let vert_idx = vertex_indices[grid_idx];
        let vert_color = face.colors[vert_idx];
        let sx = grid_x + (col as f32) * (swatch_size + swatch_spacing);
        let sy = content_y + (row as f32) * (swatch_size + swatch_spacing);
        let swatch_rect = Rect::new(sx, sy, swatch_size, swatch_size);

        // Draw swatch
        draw_rectangle(swatch_rect.x, swatch_rect.y, swatch_rect.w, swatch_rect.h,
            macroquad::color::Color::new(
                vert_color.r as f32 / 255.0,
                vert_color.g as f32 / 255.0,
                vert_color.b as f32 / 255.0,
                1.0
            ));

        // Check if this vertex is selected
        let is_selected = state.selected_vertex_indices.contains(&vert_idx);
        let hovered = ctx.mouse.inside(&swatch_rect);
        let border_color = if is_selected {
            macroquad::color::Color::from_rgba(0, 255, 255, 255) // Cyan for selected
        } else if hovered {
            macroquad::color::Color::from_rgba(255, 255, 0, 255) // Yellow for hover
        } else {
            macroquad::color::Color::from_rgba(80, 80, 80, 255)
        };
        draw_rectangle_lines(swatch_rect.x, swatch_rect.y, swatch_rect.w, swatch_rect.h,
            if is_selected { 2.0 } else { 1.0 }, border_color);

        // Handle click - toggle selection of this vertex
        if hovered && ctx.mouse.left_pressed {
            if is_selected {
                state.selected_vertex_indices.retain(|&v| v != vert_idx);
            } else {
                state.selected_vertex_indices.push(vert_idx);
            }
        }

        // Tooltip
        if hovered {
            let status = if is_selected { "selected" } else { "click to select" };
            ctx.tooltip = Some(crate::ui::PendingTooltip {
                text: format!("{}: ({}, {}, {}) - {}", vertex_labels[grid_idx], vert_color.r, vert_color.g, vert_color.b, status),
                x: ctx.mouse.x,
                y: ctx.mouse.y,
            });
        }
    }

    // Color preset buttons (apply to all vertices)
    let preset_x = grid_x + 2.0 * (swatch_size + swatch_spacing) + 8.0;
    let preset_size = 14.0;
    let preset_spacing = 2.0;

    // Preset colors: Neutral, Red tint, Blue tint, Green tint, Warm, Cool
    let presets: [(crate::rasterizer::Color, &str); 6] = [
        (crate::rasterizer::Color::NEUTRAL, "Neutral (no tint)"),
        (crate::rasterizer::Color::new(160, 120, 120), "Red tint"),
        (crate::rasterizer::Color::new(120, 120, 160), "Blue tint"),
        (crate::rasterizer::Color::new(120, 160, 120), "Green tint"),
        (crate::rasterizer::Color::new(150, 130, 110), "Warm tint"),
        (crate::rasterizer::Color::new(110, 130, 150), "Cool tint"),
    ];

    for (i, (preset_color, tooltip)) in presets.iter().enumerate() {
        let px = preset_x + (i as f32) * (preset_size + preset_spacing);
        let preset_rect = Rect::new(px, content_y + 8.0, preset_size, preset_size);

        // Draw preset swatch
        draw_rectangle(preset_rect.x, preset_rect.y, preset_rect.w, preset_rect.h,
            macroquad::color::Color::new(
                preset_color.r as f32 / 255.0,
                preset_color.g as f32 / 255.0,
                preset_color.b as f32 / 255.0,
                1.0
            ));

        // Highlight if hovered or all vertices match
        let all_match = is_uniform && face.colors[0].r == preset_color.r &&
            face.colors[0].g == preset_color.g && face.colors[0].b == preset_color.b;
        let hovered = ctx.mouse.inside(&preset_rect);
        let border_color = if all_match {
            macroquad::color::Color::from_rgba(0, 200, 200, 255)
        } else if hovered {
            macroquad::color::Color::from_rgba(200, 200, 200, 255)
        } else {
            macroquad::color::Color::from_rgba(80, 80, 80, 255)
        };
        draw_rectangle_lines(preset_rect.x, preset_rect.y, preset_rect.w, preset_rect.h, 1.0, border_color);

        // Handle click - apply to selected vertices (or all if none selected)
        if hovered && ctx.mouse.left_pressed {
            state.save_undo();
            if let Some(r) = state.level.rooms.get_mut(room_idx) {
                if let Some(s) = r.get_sector_mut(gx, gz) {
                    let face_ref = if is_floor { &mut s.floor } else { &mut s.ceiling };
                    if let Some(f) = face_ref {
                        if state.selected_vertex_indices.is_empty() {
                            // No vertices selected - apply to all
                            f.set_uniform_color(*preset_color);
                        } else {
                            // Apply only to selected vertices
                            for &idx in &state.selected_vertex_indices {
                                if idx < 4 {
                                    f.colors[idx] = *preset_color;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Tooltip
        let target = if state.selected_vertex_indices.is_empty() {
            "all vertices"
        } else {
            "selected vertices"
        };
        if hovered {
            ctx.tooltip = Some(crate::ui::PendingTooltip {
                text: format!("{} (apply to {})", tooltip, target),
                x: ctx.mouse.x,
                y: ctx.mouse.y,
            });
        }
    }

    // PS1 color picker below swatches (for custom colors)
    content_y += 36.0;
    let picker_label = if state.selected_vertex_indices.is_empty() {
        "Custom (all)"
    } else {
        "Custom (selected)"
    };

    // Get current color to display in picker (use first selected vertex, or first vertex if none selected)
    let display_color = if !state.selected_vertex_indices.is_empty() {
        let idx = state.selected_vertex_indices[0].min(3);
        face.colors[idx]
    } else {
        face.colors[0]
    };

    let picker_result = draw_ps1_color_picker(
        ctx,
        content_x,
        content_y + 14.0,
        width - CONTAINER_PADDING * 2.0,
        display_color,
        picker_label,
        &mut state.vertex_color_slider,
    );

    if let Some(new_color) = picker_result.color {
        state.save_undo();
        if let Some(r) = state.level.rooms.get_mut(room_idx) {
            if let Some(s) = r.get_sector_mut(gx, gz) {
                let face_ref = if is_floor { &mut s.floor } else { &mut s.ceiling };
                if let Some(f) = face_ref {
                    if state.selected_vertex_indices.is_empty() {
                        // No vertices selected - apply to all
                        f.set_uniform_color(new_color);
                    } else {
                        // Apply only to selected vertices
                        for &idx in &state.selected_vertex_indices {
                            if idx < 4 {
                                f.colors[idx] = new_color;
                            }
                        }
                    }
                }
            }
        }
    }

    container_height
}

/// Helper: Flip UV coordinates horizontally
fn flip_uv_horizontal(uv: &mut Option<[crate::rasterizer::Vec2; 4]>) {
    use crate::rasterizer::Vec2;
    let current = uv.unwrap_or([
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(0.0, 1.0),
    ]);
    // Flip X: swap left and right
    *uv = Some([
        Vec2::new(1.0 - current[0].x, current[0].y),
        Vec2::new(1.0 - current[1].x, current[1].y),
        Vec2::new(1.0 - current[2].x, current[2].y),
        Vec2::new(1.0 - current[3].x, current[3].y),
    ]);
}

/// Helper: Flip UV coordinates vertically
fn flip_uv_vertical(uv: &mut Option<[crate::rasterizer::Vec2; 4]>) {
    use crate::rasterizer::Vec2;
    let current = uv.unwrap_or([
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(0.0, 1.0),
    ]);
    // Flip Y: swap top and bottom
    *uv = Some([
        Vec2::new(current[0].x, 1.0 - current[0].y),
        Vec2::new(current[1].x, 1.0 - current[1].y),
        Vec2::new(current[2].x, 1.0 - current[2].y),
        Vec2::new(current[3].x, 1.0 - current[3].y),
    ]);
}

/// Helper: Rotate UV coordinates 90° clockwise
/// This rotates the texture appearance by shifting which UV goes to which corner
fn rotate_uv_cw(uv: &mut Option<[crate::rasterizer::Vec2; 4]>) {
    use crate::rasterizer::Vec2;
    let current = uv.unwrap_or([
        Vec2::new(0.0, 0.0),  // corner 0: NW
        Vec2::new(1.0, 0.0),  // corner 1: NE
        Vec2::new(1.0, 1.0),  // corner 2: SE
        Vec2::new(0.0, 1.0),  // corner 3: SW
    ]);
    // To rotate the texture 90° CW, each corner gets the UV from the previous corner
    // (i.e., shift the array by 1 position backwards)
    // corner 0 gets corner 3's UV, corner 1 gets corner 0's UV, etc.
    *uv = Some([
        current[3],  // corner 0 now shows what was at corner 3
        current[0],  // corner 1 now shows what was at corner 0
        current[1],  // corner 2 now shows what was at corner 1
        current[2],  // corner 3 now shows what was at corner 2
    ]);
}

/// UV parameters extracted from raw UV coordinates
#[derive(Debug, Clone, Copy)]
struct UvParams {
    x_offset: f32,
    y_offset: f32,
    x_scale: f32,
    y_scale: f32,
    angle: f32, // in degrees
}

impl Default for UvParams {
    fn default() -> Self {
        Self {
            x_offset: 0.0,
            y_offset: 0.0,
            x_scale: 1.0,
            y_scale: 1.0,
            angle: 0.0,
        }
    }
}

/// Extract UV parameters from 4-corner UV coordinates
/// Assumes default UV is [(0,0), (1,0), (1,1), (0,1)] for NW, NE, SE, SW
fn extract_uv_params(uv: &Option<[crate::rasterizer::Vec2; 4]>) -> UvParams {
    use crate::rasterizer::Vec2;
    let coords = uv.unwrap_or([
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(0.0, 1.0),
    ]);

    // Calculate center (average of all corners)
    let center_x = (coords[0].x + coords[1].x + coords[2].x + coords[3].x) / 4.0;
    let center_y = (coords[0].y + coords[1].y + coords[2].y + coords[3].y) / 4.0;

    // Offset is how much the center has moved from default (0.5, 0.5)
    let x_offset = center_x - 0.5;
    let y_offset = center_y - 0.5;

    // Scale: measure the width and height of the UV quad
    // Width = distance from NW to NE (along X), Height = distance from NW to SW (along Y)
    let width = ((coords[1].x - coords[0].x).powi(2) + (coords[1].y - coords[0].y).powi(2)).sqrt();
    let height = ((coords[3].x - coords[0].x).powi(2) + (coords[3].y - coords[0].y).powi(2)).sqrt();

    // Angle: angle of the NW->NE edge from horizontal
    let dx = coords[1].x - coords[0].x;
    let dy = coords[1].y - coords[0].y;
    let angle = dy.atan2(dx).to_degrees();

    UvParams {
        x_offset,
        y_offset,
        x_scale: width,
        y_scale: height,
        angle,
    }
}

/// Apply UV parameters to generate 4-corner UV coordinates
fn apply_uv_params(params: &UvParams) -> [crate::rasterizer::Vec2; 4] {
    use crate::rasterizer::Vec2;

    // Start with unit square centered at origin
    let half_w = params.x_scale / 2.0;
    let half_h = params.y_scale / 2.0;

    // Corners before rotation (centered at origin)
    let corners = [
        Vec2::new(-half_w, -half_h), // NW
        Vec2::new(half_w, -half_h),  // NE
        Vec2::new(half_w, half_h),   // SE
        Vec2::new(-half_w, half_h),  // SW
    ];

    // Rotate around center
    let rad = params.angle.to_radians();
    let cos_a = rad.cos();
    let sin_a = rad.sin();

    let rotated: Vec<Vec2> = corners.iter().map(|c| {
        Vec2::new(
            c.x * cos_a - c.y * sin_a,
            c.x * sin_a + c.y * cos_a,
        )
    }).collect();

    // Translate to final position (center at 0.5 + offset)
    let center_x = 0.5 + params.x_offset;
    let center_y = 0.5 + params.y_offset;

    [
        Vec2::new(rotated[0].x + center_x, rotated[0].y + center_y),
        Vec2::new(rotated[1].x + center_x, rotated[1].y + center_y),
        Vec2::new(rotated[2].x + center_x, rotated[2].y + center_y),
        Vec2::new(rotated[3].x + center_x, rotated[3].y + center_y),
    ]
}

/// Draw UV editing controls and return if any value changed
fn draw_uv_controls(
    ctx: &mut UiContext,
    x: f32,
    y: f32,
    width: f32,
    uv: &Option<[crate::rasterizer::Vec2; 4]>,
    state: &mut EditorState,
    icon_font: Option<&Font>,
) -> Option<[crate::rasterizer::Vec2; 4]> {
    use crate::ui::{draw_drag_value_compact_editable, icon_button_active, Rect, icon};

    let mut params = extract_uv_params(uv);
    let mut changed = false;
    let row_height = 18.0;
    let link_btn_size = 16.0;
    let label_width = 42.0;
    let value_width = (width - label_width - link_btn_size - 12.0) / 2.0;
    let label_color = Color::from_rgba(150, 150, 150, 255);

    let mut current_y = y;

    // Row 1: Offset - [Link] Label [X] [Y]
    let link_rect = Rect::new(x, current_y + 1.0, link_btn_size, link_btn_size);
    let link_icon = if state.uv_offset_linked { icon::LINK } else { icon::UNLINK };
    if icon_button_active(ctx, link_rect, link_icon, icon_font, "Link X/Y", state.uv_offset_linked) {
        state.uv_offset_linked = !state.uv_offset_linked;
    }

    draw_text("Offset", x + link_btn_size + 4.0, current_y + 12.0, 11.0, label_color);
    let value_start = x + link_btn_size + 4.0 + label_width;
    let ox_rect = Rect::new(value_start, current_y, value_width - 2.0, row_height);
    let result = draw_drag_value_compact_editable(
        ctx, ox_rect, params.x_offset, 0.1, 1001,
        &mut state.uv_drag_active[0], &mut state.uv_drag_start_value[0], &mut state.uv_drag_start_x[0],
        Some(&mut state.uv_editing_field), Some((&mut state.uv_edit_buffer, 0)),
    );
    if let Some(v) = result.value {
        let delta = v - params.x_offset;
        params.x_offset = v;
        if state.uv_offset_linked {
            params.y_offset += delta;
        }
        changed = true;
    }
    let oy_rect = Rect::new(value_start + value_width, current_y, value_width - 2.0, row_height);
    let result = draw_drag_value_compact_editable(
        ctx, oy_rect, params.y_offset, 0.1, 1002,
        &mut state.uv_drag_active[1], &mut state.uv_drag_start_value[1], &mut state.uv_drag_start_x[1],
        Some(&mut state.uv_editing_field), Some((&mut state.uv_edit_buffer, 1)),
    );
    if let Some(v) = result.value {
        let delta = v - params.y_offset;
        params.y_offset = v;
        if state.uv_offset_linked {
            params.x_offset += delta;
        }
        changed = true;
    }
    current_y += row_height;

    // Row 2: Scale - [Link] Label [X] [Y]
    let link_rect = Rect::new(x, current_y + 1.0, link_btn_size, link_btn_size);
    let link_icon = if state.uv_scale_linked { icon::LINK } else { icon::UNLINK };
    if icon_button_active(ctx, link_rect, link_icon, icon_font, "Link X/Y", state.uv_scale_linked) {
        state.uv_scale_linked = !state.uv_scale_linked;
    }

    draw_text("Scale", x + link_btn_size + 4.0, current_y + 12.0, 11.0, label_color);
    let sx_rect = Rect::new(value_start, current_y, value_width - 2.0, row_height);
    let result = draw_drag_value_compact_editable(
        ctx, sx_rect, params.x_scale, 0.1, 1003,
        &mut state.uv_drag_active[2], &mut state.uv_drag_start_value[2], &mut state.uv_drag_start_x[2],
        Some(&mut state.uv_editing_field), Some((&mut state.uv_edit_buffer, 2)),
    );
    if let Some(v) = result.value {
        let old_scale = params.x_scale;
        params.x_scale = v.max(0.01_f32); // Prevent zero/negative scale
        if state.uv_scale_linked && old_scale > 0.001 {
            let ratio = params.x_scale / old_scale;
            params.y_scale = (params.y_scale * ratio).max(0.01);
        }
        changed = true;
    }
    let sy_rect = Rect::new(value_start + value_width, current_y, value_width - 2.0, row_height);
    let result = draw_drag_value_compact_editable(
        ctx, sy_rect, params.y_scale, 0.1, 1004,
        &mut state.uv_drag_active[3], &mut state.uv_drag_start_value[3], &mut state.uv_drag_start_x[3],
        Some(&mut state.uv_editing_field), Some((&mut state.uv_edit_buffer, 3)),
    );
    if let Some(v) = result.value {
        let old_scale = params.y_scale;
        params.y_scale = v.max(0.01_f32);
        if state.uv_scale_linked && old_scale > 0.001 {
            let ratio = params.y_scale / old_scale;
            params.x_scale = (params.x_scale * ratio).max(0.01);
        }
        changed = true;
    }
    current_y += row_height;

    // Row 3: Angle (no link button, full width)
    draw_text("Angle", x + link_btn_size + 4.0, current_y + 12.0, 11.0, label_color);
    let angle_rect = Rect::new(value_start, current_y, width - value_start + x - 4.0, row_height);
    let result = draw_drag_value_compact_editable(
        ctx, angle_rect, params.angle, 1.0, 1005,
        &mut state.uv_drag_active[4], &mut state.uv_drag_start_value[4], &mut state.uv_drag_start_x[4],
        Some(&mut state.uv_editing_field), Some((&mut state.uv_edit_buffer, 4)),
    );
    if let Some(v) = result.value {
        params.angle = v;
        changed = true;
    }

    if changed {
        Some(apply_uv_params(&params))
    } else {
        None
    }
}

/// Draw properties for a wall face inside a container
fn draw_wall_face_container(
    ctx: &mut UiContext,
    x: f32,
    y: f32,
    width: f32,
    wall: &crate::world::VerticalFace,
    label: &str,
    label_color: Color,
    room_idx: usize,
    gx: usize,
    gz: usize,
    wall_dir: crate::world::Direction,
    wall_idx: usize,
    state: &mut EditorState,
    icon_font: Option<&Font>,
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
    content_y += line_height;

    // UV coordinates display
    let uv_label_color = Color::from_rgba(150, 150, 150, 255);
    if let Some(uv) = &wall.uv {
        draw_text("UV: Custom", content_x.floor(), (content_y + 12.0).floor(), 13.0, uv_label_color);
        content_y += line_height;
        // Show UV coordinates compactly
        draw_text(&format!("  [{:.2},{:.2}] [{:.2},{:.2}]", uv[0].x, uv[0].y, uv[1].x, uv[1].y),
            content_x.floor(), (content_y + 12.0).floor(), 11.0, Color::from_rgba(120, 120, 120, 255));
        content_y += line_height;
    } else {
        draw_text("UV: Default", content_x.floor(), (content_y + 12.0).floor(), 13.0, uv_label_color);
        content_y += line_height;
    }

    // UV parameter editing controls
    let controls_width = width - CONTAINER_PADDING * 2.0;
    if let Some(new_uv) = draw_uv_controls(ctx, content_x, content_y, controls_width, &wall.uv, state, icon_font) {
        state.save_undo();
        if let Some(r) = state.level.rooms.get_mut(room_idx) {
            if let Some(s) = r.get_sector_mut(gx, gz) {
                if let Some(w) = s.walls_mut(wall_dir).get_mut(wall_idx) {
                    w.uv = Some(new_uv);
                }
            }
        }
    }
    content_y += 54.0; // Height of UV controls (3 rows * 18px)

    // UV manipulation buttons
    let btn_size = 20.0;
    let btn_spacing = 4.0;
    let mut btn_x = content_x;

    // Reset UV button
    let reset_rect = Rect::new(btn_x, content_y, btn_size, btn_size);
    if crate::ui::icon_button(ctx, reset_rect, icon::REFRESH_CW, icon_font, "Reset UV") {
        state.save_undo();
        if let Some(r) = state.level.rooms.get_mut(room_idx) {
            if let Some(s) = r.get_sector_mut(gx, gz) {
                if let Some(w) = s.walls_mut(wall_dir).get_mut(wall_idx) {
                    w.uv = None;
                }
            }
        }
    }
    btn_x += btn_size + btn_spacing;

    // Flip Horizontal button
    let flip_h_rect = Rect::new(btn_x, content_y, btn_size, btn_size);
    if crate::ui::icon_button(ctx, flip_h_rect, icon::FLIP_HORIZONTAL, icon_font, "Flip UV Horizontal") {
        state.save_undo();
        if let Some(r) = state.level.rooms.get_mut(room_idx) {
            if let Some(s) = r.get_sector_mut(gx, gz) {
                if let Some(w) = s.walls_mut(wall_dir).get_mut(wall_idx) {
                    flip_uv_horizontal(&mut w.uv);
                }
            }
        }
    }
    btn_x += btn_size + btn_spacing;

    // Flip Vertical button
    let flip_v_rect = Rect::new(btn_x, content_y, btn_size, btn_size);
    if crate::ui::icon_button(ctx, flip_v_rect, icon::FLIP_VERTICAL, icon_font, "Flip UV Vertical") {
        state.save_undo();
        if let Some(r) = state.level.rooms.get_mut(room_idx) {
            if let Some(s) = r.get_sector_mut(gx, gz) {
                if let Some(w) = s.walls_mut(wall_dir).get_mut(wall_idx) {
                    flip_uv_vertical(&mut w.uv);
                }
            }
        }
    }
    btn_x += btn_size + btn_spacing;

    // Rotate 90° CW button
    let rotate_rect = Rect::new(btn_x, content_y, btn_size, btn_size);
    if crate::ui::icon_button(ctx, rotate_rect, icon::ROTATE_CW, icon_font, "Rotate UV 90° CW") {
        state.save_undo();
        if let Some(r) = state.level.rooms.get_mut(room_idx) {
            if let Some(s) = r.get_sector_mut(gx, gz) {
                if let Some(w) = s.walls_mut(wall_dir).get_mut(wall_idx) {
                    rotate_uv_cw(&mut w.uv);
                }
            }
        }
    }
    content_y += btn_size + 4.0;

    // Wall vertex colors (PS1-style texture modulation)
    // Show 4 vertex color swatches in a 2x2 grid (BL, BR / TL, TR layout)
    let swatch_size = 14.0;
    let swatch_spacing = 2.0;

    // Label
    let is_uniform = wall.has_uniform_color();
    let color_text = if is_uniform {
        let c = wall.colors[0];
        if c.r == 128 && c.g == 128 && c.b == 128 {
            String::from("Tint: Neutral")
        } else {
            format!("Tint: ({}, {}, {})", c.r, c.g, c.b)
        }
    } else {
        String::from("Tint: Per-vertex")
    };
    draw_text(&color_text, content_x.floor(), (content_y + 12.0).floor(), 12.0,
        macroquad::color::Color::from_rgba(180, 180, 180, 255));

    // Draw 4 vertex color swatches in 2x2 grid (TL, TR / BL, BR layout - visual matches wall)
    let grid_x = content_x + 90.0;
    let vertex_labels = ["TL", "TR", "BL", "BR"];
    let grid_positions = [(0, 0), (1, 0), (0, 1), (1, 1)]; // (col, row)
    let vertex_indices = [3, 2, 0, 1]; // Map grid to corner indices: BL=0, BR=1, TR=2, TL=3

    for (grid_idx, &(col, row)) in grid_positions.iter().enumerate() {
        let vert_idx = vertex_indices[grid_idx];
        let vert_color = wall.colors[vert_idx];
        let sx = grid_x + (col as f32) * (swatch_size + swatch_spacing);
        let sy = content_y + (row as f32) * (swatch_size + swatch_spacing);
        let swatch_rect = Rect::new(sx, sy, swatch_size, swatch_size);

        // Draw swatch
        draw_rectangle(swatch_rect.x, swatch_rect.y, swatch_rect.w, swatch_rect.h,
            macroquad::color::Color::new(
                vert_color.r as f32 / 255.0,
                vert_color.g as f32 / 255.0,
                vert_color.b as f32 / 255.0,
                1.0
            ));

        // Check if this vertex is selected
        let is_selected = state.selected_vertex_indices.contains(&vert_idx);
        let hovered = ctx.mouse.inside(&swatch_rect);
        let border_color = if is_selected {
            macroquad::color::Color::from_rgba(0, 255, 255, 255) // Cyan for selected
        } else if hovered {
            macroquad::color::Color::from_rgba(255, 255, 0, 255) // Yellow for hover
        } else {
            macroquad::color::Color::from_rgba(80, 80, 80, 255)
        };
        draw_rectangle_lines(swatch_rect.x, swatch_rect.y, swatch_rect.w, swatch_rect.h,
            if is_selected { 2.0 } else { 1.0 }, border_color);

        // Handle click - toggle selection of this vertex
        if hovered && ctx.mouse.left_pressed {
            if is_selected {
                state.selected_vertex_indices.retain(|&v| v != vert_idx);
            } else {
                state.selected_vertex_indices.push(vert_idx);
            }
        }

        // Tooltip
        if hovered {
            let status = if is_selected { "selected" } else { "click to select" };
            ctx.tooltip = Some(crate::ui::PendingTooltip {
                text: format!("{}: ({}, {}, {}) - {}", vertex_labels[grid_idx], vert_color.r, vert_color.g, vert_color.b, status),
                x: ctx.mouse.x,
                y: ctx.mouse.y,
            });
        }
    }

    // Color preset buttons (apply to selected vertices or all)
    let preset_x = grid_x + 2.0 * (swatch_size + swatch_spacing) + 8.0;
    let preset_size = 14.0;
    let preset_spacing = 2.0;

    // Preset colors: Neutral, Red tint, Blue tint, Green tint, Warm, Cool
    let presets: [(crate::rasterizer::Color, &str); 6] = [
        (crate::rasterizer::Color::NEUTRAL, "Neutral (no tint)"),
        (crate::rasterizer::Color::new(160, 120, 120), "Red tint"),
        (crate::rasterizer::Color::new(120, 120, 160), "Blue tint"),
        (crate::rasterizer::Color::new(120, 160, 120), "Green tint"),
        (crate::rasterizer::Color::new(150, 130, 110), "Warm tint"),
        (crate::rasterizer::Color::new(110, 130, 150), "Cool tint"),
    ];

    for (i, (preset_color, tooltip)) in presets.iter().enumerate() {
        let px = preset_x + (i as f32) * (preset_size + preset_spacing);
        let preset_rect = Rect::new(px, content_y + 8.0, preset_size, preset_size);

        // Draw preset swatch
        draw_rectangle(preset_rect.x, preset_rect.y, preset_rect.w, preset_rect.h,
            macroquad::color::Color::new(
                preset_color.r as f32 / 255.0,
                preset_color.g as f32 / 255.0,
                preset_color.b as f32 / 255.0,
                1.0
            ));

        // Highlight if hovered or all vertices match
        let all_match = is_uniform && wall.colors[0].r == preset_color.r &&
            wall.colors[0].g == preset_color.g && wall.colors[0].b == preset_color.b;
        let hovered = ctx.mouse.inside(&preset_rect);
        let border_color = if all_match {
            macroquad::color::Color::from_rgba(0, 200, 200, 255)
        } else if hovered {
            macroquad::color::Color::from_rgba(200, 200, 200, 255)
        } else {
            macroquad::color::Color::from_rgba(80, 80, 80, 255)
        };
        draw_rectangle_lines(preset_rect.x, preset_rect.y, preset_rect.w, preset_rect.h, 1.0, border_color);

        // Handle click - apply to selected vertices (or all if none selected)
        if hovered && ctx.mouse.left_pressed {
            state.save_undo();
            if let Some(r) = state.level.rooms.get_mut(room_idx) {
                if let Some(s) = r.get_sector_mut(gx, gz) {
                    if let Some(w) = s.walls_mut(wall_dir).get_mut(wall_idx) {
                        if state.selected_vertex_indices.is_empty() {
                            // No vertices selected - apply to all
                            w.set_uniform_color(*preset_color);
                        } else {
                            // Apply only to selected vertices
                            for &idx in &state.selected_vertex_indices {
                                if idx < 4 {
                                    w.colors[idx] = *preset_color;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Tooltip
        let target = if state.selected_vertex_indices.is_empty() {
            "all vertices"
        } else {
            "selected vertices"
        };
        if hovered {
            ctx.tooltip = Some(crate::ui::PendingTooltip {
                text: format!("{} (apply to {})", tooltip, target),
                x: ctx.mouse.x,
                y: ctx.mouse.y,
            });
        }
    }

    // PS1 color picker below swatches (for custom colors)
    content_y += 36.0;
    let picker_label = if state.selected_vertex_indices.is_empty() {
        "Custom (all)"
    } else {
        "Custom (selected)"
    };

    // Get current color to display in picker (use first selected vertex, or first vertex if none selected)
    let display_color = if !state.selected_vertex_indices.is_empty() {
        let idx = state.selected_vertex_indices[0].min(3);
        wall.colors[idx]
    } else {
        wall.colors[0]
    };

    let picker_result = draw_ps1_color_picker(
        ctx,
        content_x,
        content_y + 14.0,
        width - CONTAINER_PADDING * 2.0,
        display_color,
        picker_label,
        &mut state.vertex_color_slider,
    );

    if let Some(new_color) = picker_result.color {
        state.save_undo();
        if let Some(r) = state.level.rooms.get_mut(room_idx) {
            if let Some(s) = r.get_sector_mut(gx, gz) {
                if let Some(w) = s.walls_mut(wall_dir).get_mut(wall_idx) {
                    if state.selected_vertex_indices.is_empty() {
                        // No vertices selected - apply to all
                        w.set_uniform_color(new_color);
                    } else {
                        // Apply only to selected vertices
                        for &idx in &state.selected_vertex_indices {
                            if idx < 4 {
                                w.colors[idx] = new_color;
                            }
                        }
                    }
                }
            }
        }
    }

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
                            let _ = h + CONTAINER_MARGIN; // Layout positioning for potential future faces
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
                            let _ = h + CONTAINER_MARGIN;
                        } else {
                            draw_text("(no ceiling)", x, (y + 14.0).floor(), 14.0, Color::from_rgba(100, 100, 100, 255));
                        }
                    }
                    super::SectorFace::WallNorth(i) => {
                        if let Some(wall) = sector.walls_north.get(*i) {
                            let h = draw_wall_face_container(
                                ctx, x, y, container_width, wall, "Wall (North)",
                                Color::from_rgba(255, 180, 120, 255),
                                *room, *gx, *gz, crate::world::Direction::North, *i, state, icon_font
                            );
                            let _ = h + CONTAINER_MARGIN;
                        }
                    }
                    super::SectorFace::WallEast(i) => {
                        if let Some(wall) = sector.walls_east.get(*i) {
                            let h = draw_wall_face_container(
                                ctx, x, y, container_width, wall, "Wall (East)",
                                Color::from_rgba(255, 180, 120, 255),
                                *room, *gx, *gz, crate::world::Direction::East, *i, state, icon_font
                            );
                            let _ = h + CONTAINER_MARGIN;
                        }
                    }
                    super::SectorFace::WallSouth(i) => {
                        if let Some(wall) = sector.walls_south.get(*i) {
                            let h = draw_wall_face_container(
                                ctx, x, y, container_width, wall, "Wall (South)",
                                Color::from_rgba(255, 180, 120, 255),
                                *room, *gx, *gz, crate::world::Direction::South, *i, state, icon_font
                            );
                            let _ = h + CONTAINER_MARGIN;
                        }
                    }
                    super::SectorFace::WallWest(i) => {
                        if let Some(wall) = sector.walls_west.get(*i) {
                            let h = draw_wall_face_container(
                                ctx, x, y, container_width, wall, "Wall (West)",
                                Color::from_rgba(255, 180, 120, 255),
                                *room, *gx, *gz, crate::world::Direction::West, *i, state, icon_font
                            );
                            let _ = h + CONTAINER_MARGIN;
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
                use crate::world::Direction;
                let wall_dirs: [(&str, &Vec<crate::world::VerticalFace>, Direction); 4] = [
                    ("North", &sector.walls_north, Direction::North),
                    ("East", &sector.walls_east, Direction::East),
                    ("South", &sector.walls_south, Direction::South),
                    ("West", &sector.walls_west, Direction::West),
                ];

                for (dir_name, walls, dir) in wall_dirs {
                    for (i, wall) in walls.iter().enumerate() {
                        let label = if walls.len() == 1 {
                            format!("Wall ({})", dir_name)
                        } else {
                            format!("Wall ({}) [{}]", dir_name, i)
                        };
                        let h = draw_wall_face_container(
                            ctx, x, y, container_width, wall, &label,
                            Color::from_rgba(255, 180, 120, 255),
                            *room, *gx, *gz, dir, i, state, icon_font
                        );
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
        super::Selection::Object { index } => {
            // Object properties
            let obj_idx = *index;

            if let Some(obj) = state.level.objects.get(obj_idx).cloned() {
                let obj_name = obj.object_type.display_name();
                draw_text(obj_name, x, (y + 14.0).floor(), 16.0, WHITE);
                y += 24.0;

                // Location
                draw_text("Location:", x, (y + 12.0).floor(), 13.0, Color::from_rgba(150, 150, 150, 255));
                y += 18.0;
                draw_text(&format!("  Room: {}  Sector: ({}, {})",
                    obj.room, obj.sector_x, obj.sector_z),
                    x, (y + 12.0).floor(), 13.0, WHITE);
                y += 18.0;
                draw_text(&format!("  Height: {:.0}  Facing: {:.1}°",
                    obj.height, obj.facing.to_degrees()),
                    x, (y + 12.0).floor(), 13.0, WHITE);
                y += 24.0;

                // Type-specific properties
                match &obj.object_type {
                    crate::world::ObjectType::Light { color, intensity, radius } => {
                        // Color picker
                        let picker_height = ps1_color_picker_height();
                        let picker_result = draw_ps1_color_picker(
                            ctx,
                            x,
                            y,
                            container_width - 8.0,
                            *color,
                            "Color",
                            &mut state.light_color_slider,
                        );
                        if let Some(new_color) = picker_result.color {
                            if let Some(obj_mut) = state.level.objects.get_mut(obj_idx) {
                                if let crate::world::ObjectType::Light { color: c, .. } = &mut obj_mut.object_type {
                                    *c = new_color;
                                }
                            }
                        }
                        y += picker_height + 16.0;

                        // Intensity knob
                        let knob_radius = 18.0;
                        let knob_center_x = x + container_width * 0.25;
                        let knob_center_y = y + knob_radius + 12.0;

                        // Convert intensity (0.0-2.0) to knob value (0-127)
                        let intensity_value = ((intensity / 2.0) * 127.0).round() as u8;
                        let intensity_result = draw_knob(
                            ctx,
                            knob_center_x,
                            knob_center_y,
                            knob_radius,
                            intensity_value.min(127),
                            "Intensity",
                            false,
                            false,
                        );

                        if let Some(new_val) = intensity_result.value {
                            let new_intensity = (new_val as f32 / 127.0) * 2.0;
                            if let Some(obj_mut) = state.level.objects.get_mut(obj_idx) {
                                if let crate::world::ObjectType::Light { intensity: i, .. } = &mut obj_mut.object_type {
                                    *i = new_intensity;
                                }
                            }
                        }

                        // Radius knob
                        let knob_center_x2 = x + container_width * 0.75;

                        // Convert radius (0-16384) to knob value (0-127)
                        // 16384 = 16 sectors worth of radius
                        const MAX_LIGHT_RADIUS: f32 = 16384.0;
                        let radius_value = ((radius / MAX_LIGHT_RADIUS) * 127.0).round() as u8;
                        let radius_result = draw_knob(
                            ctx,
                            knob_center_x2,
                            knob_center_y,
                            knob_radius,
                            radius_value.min(127),
                            "Radius",
                            false,
                            false,
                        );

                        if let Some(new_val) = radius_result.value {
                            let new_radius = (new_val as f32 / 127.0) * MAX_LIGHT_RADIUS;
                            if let Some(obj_mut) = state.level.objects.get_mut(obj_idx) {
                                if let crate::world::ObjectType::Light { radius: r, .. } = &mut obj_mut.object_type {
                                    *r = new_radius;
                                }
                            }
                        }

                        y += knob_radius * 2.0 + 35.0;
                    }
                    crate::world::ObjectType::Spawn(spawn_type) => {
                        draw_text(&format!("Type: {:?}", spawn_type), x, (y + 12.0).floor(), 13.0, WHITE);
                        y += 24.0;
                    }
                    crate::world::ObjectType::Prop(model_name) => {
                        draw_text(&format!("Model: {}", model_name), x, (y + 12.0).floor(), 13.0, WHITE);
                        y += 24.0;
                    }
                    _ => {
                        // Other object types - just show enabled status
                        y += 8.0;
                    }
                }

                // Enabled toggle
                let enabled_btn_rect = Rect::new(x, y, container_width - 8.0, 22.0);
                let enabled_hovered = enabled_btn_rect.contains(ctx.mouse.x, ctx.mouse.y);
                let enabled_color = if obj.enabled {
                    if enabled_hovered { Color::from_rgba(60, 140, 60, 255) } else { Color::from_rgba(40, 100, 40, 255) }
                } else {
                    if enabled_hovered { Color::from_rgba(100, 100, 100, 255) } else { Color::from_rgba(60, 60, 60, 255) }
                };
                draw_rectangle(enabled_btn_rect.x, enabled_btn_rect.y, enabled_btn_rect.w, enabled_btn_rect.h, enabled_color);
                if enabled_hovered {
                    draw_rectangle_lines(enabled_btn_rect.x, enabled_btn_rect.y, enabled_btn_rect.w, enabled_btn_rect.h, 1.0, WHITE);
                }
                let enabled_text = if obj.enabled { "Enabled" } else { "Disabled" };
                draw_text(enabled_text, x + 10.0, (y + 15.0).floor(), 13.0, WHITE);

                if enabled_hovered && ctx.mouse.left_pressed {
                    state.save_undo();
                    if let Some(obj_mut) = state.level.objects.get_mut(obj_idx) {
                        obj_mut.enabled = !obj_mut.enabled;
                    }
                }
                y += 28.0;

                // Delete button
                let delete_btn_rect = Rect::new(x, y, container_width - 8.0, 22.0);
                let delete_hovered = delete_btn_rect.contains(ctx.mouse.x, ctx.mouse.y);
                let delete_color = if delete_hovered {
                    Color::from_rgba(180, 60, 60, 255)
                } else {
                    Color::from_rgba(120, 40, 40, 255)
                };
                draw_rectangle(delete_btn_rect.x, delete_btn_rect.y, delete_btn_rect.w, delete_btn_rect.h, delete_color);
                if delete_hovered {
                    draw_rectangle_lines(delete_btn_rect.x, delete_btn_rect.y, delete_btn_rect.w, delete_btn_rect.h, 1.0, WHITE);
                }
                draw_text("Delete Object", x + 10.0, (y + 15.0).floor(), 13.0, WHITE);

                if delete_hovered && ctx.mouse.left_pressed {
                    state.save_undo();
                    state.level.remove_object(obj_idx);
                    state.set_selection(super::Selection::None);
                    state.set_status("Object deleted", 2.0);
                }
            } else {
                draw_text("Object not found", x, (y + 14.0).floor(), 14.0, Color::from_rgba(255, 100, 100, 255));
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
        super::Selection::Object { index } => {
            // Base height for all objects: header + location + enabled + delete
            let mut height = 24.0 + 18.0 + 18.0 + 24.0 + 28.0 + 28.0; // header + location lines + type-specific + enabled + delete

            // Add extra height for light objects (color picker + 2 knobs)
            if let Some(obj) = state.level.objects.get(*index) {
                if matches!(obj.object_type, crate::world::ObjectType::Light { .. }) {
                    height += 18.0 + ps1_color_picker_height() + 16.0 + 18.0 * 2.0 + 35.0; // color label + picker + knobs
                } else {
                    height += 24.0; // Just the type info line
                }
            }
            height
        }
    }
}

fn draw_status_bar(rect: Rect, state: &EditorState) {
    draw_rectangle(rect.x.floor(), rect.y.floor(), rect.w, rect.h, Color::from_rgba(40, 40, 45, 255));

    // Show status message on the left if available
    if let Some(msg) = state.get_status() {
        draw_text(&msg, (rect.x + 10.0).floor(), (rect.y + 15.0).floor(), 16.0, Color::from_rgba(100, 255, 100, 255));
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
