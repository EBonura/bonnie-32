//! Editor layout - TRLE-inspired panel arrangement

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext, SplitPanel, draw_panel, panel_content_rect, draw_collapsible_panel, COLLAPSED_PANEL_HEIGHT, Toolbar, icon, draw_knob, draw_ps1_color_picker, ps1_color_picker_height, ActionRegistry};
use crate::rasterizer::{Framebuffer, Texture as RasterTexture, Camera, render_mesh, Color as RasterColor, Vec3, RasterSettings, Light, ShadingMode};
use crate::input::InputState;
use super::{EditorState, EditorTool, Selection, SectorFace, GridViewMode, SECTOR_SIZE, FaceClipboard, GeometryClipboard, CopiedFace, CopiedFaceData};
use crate::world::{UV_SCALE, Sector};
use super::grid_view::draw_grid_view;
use super::viewport_3d::draw_viewport_3d;
use super::texture_palette::draw_texture_palette;
use super::actions::{create_editor_actions, build_context, flags};
use crate::modeler::{draw_asset_browser, AssetBrowserAction};

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

/// Standard font sizes for consistent UI
const FONT_SIZE_HEADER: f32 = 14.0;
const FONT_SIZE_CONTENT: f32 = 12.0;
const LINE_HEIGHT: f32 = 16.0;

/// Editor layout state (split panel ratios)
pub struct EditorLayout {
    /// Main horizontal split (left panels | center+right)
    pub main_split: SplitPanel,
    /// Right split (center viewport | right panels)
    pub right_split: SplitPanel,
    /// Left split 1: Skybox | (2D Grid + Room + Debug)
    pub left_split_1: SplitPanel,
    /// Left split 2: 2D Grid | (Room + Debug)
    pub left_split_2: SplitPanel,
    /// Left split 3: Room | Debug
    pub left_split_3: SplitPanel,
    /// Right vertical split (texture palette | properties)
    pub right_panel_split: SplitPanel,
    /// Action registry for keyboard shortcuts
    pub actions: ActionRegistry,
    /// Collapsed state for left panels
    pub left_collapsed: [bool; 4], // Skybox, 2D Grid, Room, Debug
}

impl EditorLayout {
    pub fn new() -> Self {
        // Use high IDs (1000+) to avoid collision with toolbar button IDs
        // which are auto-generated starting from 1 via ctx.next_id()
        Self {
            main_split: SplitPanel::horizontal(1000).with_ratio(0.25).with_min_size(150.0),
            right_split: SplitPanel::horizontal(1001).with_ratio(0.73).with_min_size(150.0),
            // Left sidebar: 4 panels with 3 splits
            // Skybox ~20%, 2D Grid ~35%, Room ~30%, Debug ~15%
            left_split_1: SplitPanel::vertical(1002).with_ratio(0.20).with_min_size(50.0),
            left_split_2: SplitPanel::vertical(1004).with_ratio(0.45).with_min_size(50.0),
            left_split_3: SplitPanel::vertical(1005).with_ratio(0.65).with_min_size(50.0),
            right_panel_split: SplitPanel::vertical(1003).with_ratio(0.6).with_min_size(100.0),
            actions: create_editor_actions(),
            left_collapsed: [false, false, false, true], // Debug collapsed by default
        }
    }

    /// Apply layout config from a level (panel splits only)
    pub fn apply_config(&mut self, config: &crate::world::EditorLayoutConfig) {
        self.main_split.ratio = config.main_split;
        self.right_split.ratio = config.right_split;
        // left_split from old config maps to left_split_2 (2D Grid | Room+Debug)
        self.left_split_2.ratio = config.left_split;
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
            left_split: self.left_split_2.ratio, // Save 2D Grid | Room+Debug ratio
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

/// Result from drawing a player property field
struct PlayerPropResult {
    new_y: f32,
    new_value: Option<f32>,
}

/// Draw a click-to-edit property field for player settings
/// Returns the new Y position and optionally a new value if edited
fn draw_player_prop_field(
    ctx: &mut UiContext,
    x: f32,
    y: f32,
    container_width: f32,
    line_height: f32,
    label: &str,
    value: f32,
    field_id: usize,
    editing: &mut Option<usize>,
    buffer: &mut String,
    label_color: Color,
) -> PlayerPropResult {
    let value_color = Color::from_rgba(220, 220, 230, 255);
    let accent_color = Color::from_rgba(0, 180, 180, 255);

    draw_text(label, x, (y + 13.0).floor(), 12.0, label_color);

    let value_x = x + 80.0;
    let value_w = container_width - 90.0;
    let value_rect = Rect::new(value_x, y, value_w, line_height - 2.0);

    let hovered = value_rect.contains(ctx.mouse.x, ctx.mouse.y);
    let is_editing = *editing == Some(field_id);

    let bg_color = if is_editing {
        Color::from_rgba(50, 60, 70, 255)
    } else if hovered {
        Color::from_rgba(55, 55, 65, 255)
    } else {
        Color::from_rgba(45, 45, 55, 255)
    };
    let border_color = if is_editing {
        accent_color
    } else {
        Color::from_rgba(60, 60, 65, 255)
    };

    draw_rectangle(value_rect.x, value_rect.y, value_rect.w, value_rect.h, bg_color);
    draw_rectangle_lines(value_rect.x, value_rect.y, value_rect.w, value_rect.h, 1.0, border_color);

    let mut new_value = None;

    if is_editing {
        // Text input mode
        let text_y = y + 13.0;
        let display_text = if buffer.is_empty() { "0" } else { buffer.as_str() };
        let text_dims = measure_text(display_text, None, 12, 1.0);
        let text_x = value_x + 4.0;
        draw_text(display_text, text_x, text_y.floor(), 12.0, accent_color);

        // Draw cursor (blinking)
        let time = macroquad::time::get_time();
        if (time * 2.0) as i32 % 2 == 0 {
            let cursor_x = text_x + text_dims.width + 1.0;
            draw_line(cursor_x, y + 3.0, cursor_x, y + line_height - 5.0, 1.0, accent_color);
        }

        // Handle keyboard input
        while let Some(c) = get_char_pressed() {
            if c.is_ascii_digit() || c == '.' || c == '-' {
                buffer.push(c);
            }
        }

        // Handle backspace
        if is_key_pressed(KeyCode::Backspace) {
            buffer.pop();
        }

        // Handle Enter - confirm edit
        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) {
            if let Ok(v) = buffer.parse::<f32>() {
                new_value = Some(v);
            }
            *editing = None;
            buffer.clear();
        }

        // Handle Escape - cancel edit
        if is_key_pressed(KeyCode::Escape) {
            *editing = None;
            buffer.clear();
        }

        // Click outside to confirm
        if ctx.mouse.left_pressed && !hovered {
            if let Ok(v) = buffer.parse::<f32>() {
                new_value = Some(v);
            }
            *editing = None;
            buffer.clear();
        }
    } else {
        // Display mode
        draw_text(&format!("{:.0}", value), value_x + 4.0, (y + 13.0).floor(), 12.0, value_color);

        // Click to start editing
        if hovered && ctx.mouse.left_pressed {
            *editing = Some(field_id);
            *buffer = format!("{:.0}", value);
        }
    }

    PlayerPropResult { new_y: y + line_height, new_value }
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
    input: &InputState,
) -> EditorAction {
    use super::state::EditorFrameTimings;
    let frame_start = EditorFrameTimings::start();

    let screen = bounds;

    // Single unified toolbar at top
    let toolbar_height = 36.0;
    let toolbar_rect = screen.slice_top(toolbar_height);
    let main_rect = screen.remaining_after_top(toolbar_height);

    // Status bar at bottom
    let status_height = 22.0;
    let status_rect = main_rect.slice_bottom(status_height);
    let panels_rect = main_rect.remaining_after_bottom(status_height);

    // Draw unified toolbar and handle keyboard shortcuts
    let toolbar_start = EditorFrameTimings::start();
    let action = draw_unified_toolbar(ctx, toolbar_rect, state, icon_font, &layout.actions);
    let toolbar_ms = EditorFrameTimings::elapsed_ms(toolbar_start);

    // Main split: left panels | rest
    let (left_rect, rest_rect) = layout.main_split.update(ctx, panels_rect);

    // Right split: center viewport | right panels
    let (center_rect, right_rect) = layout.right_split.update(ctx, rest_rect);

    // Right panel: collapsible sections for Textures and Properties
    // (Old split panel layout replaced with collapsible sections)

    // === LEFT PANEL ===
    let left_start = EditorFrameTimings::start();

    // Left sidebar: 4 collapsible panels (Skybox, 2D Grid, Room, Debug)
    let panel_bg = Color::from_rgba(35, 35, 40, 255);
    let header_h = COLLAPSED_PANEL_HEIGHT;

    // Count collapsed panels and calculate available height for expanded ones
    let num_collapsed = layout.left_collapsed.iter().filter(|&&c| c).count();
    let collapsed_height = num_collapsed as f32 * header_h;
    let available_height = (left_rect.h - collapsed_height).max(0.0);

    // Calculate heights for expanded panels (equal distribution)
    let num_expanded = 4 - num_collapsed;
    let expanded_panel_height = if num_expanded > 0 {
        available_height / num_expanded as f32
    } else {
        0.0
    };

    // Calculate panel rects and draw them
    let mut y = left_rect.y;
    let panel_names = ["Skybox", "2D Grid", "Rooms", "Debug"];

    // Panel 0: Skybox
    let skybox_h = if layout.left_collapsed[0] { header_h } else { expanded_panel_height };
    let skybox_rect = Rect::new(left_rect.x, y, left_rect.w, skybox_h);
    let (clicked, skybox_content) = draw_collapsible_panel(ctx, skybox_rect, panel_names[0], layout.left_collapsed[0], panel_bg);
    if clicked { layout.left_collapsed[0] = !layout.left_collapsed[0]; }
    if let Some(content) = skybox_content {
        draw_skybox_panel(ctx, content, state);
    }
    y += skybox_h;

    // Panel 1: 2D Grid
    let grid_h = if layout.left_collapsed[1] { header_h } else { expanded_panel_height };
    let grid_rect = Rect::new(left_rect.x, y, left_rect.w, grid_h);
    let (clicked, grid_content) = draw_collapsible_panel(ctx, grid_rect, panel_names[1], layout.left_collapsed[1], panel_bg);
    if clicked { layout.left_collapsed[1] = !layout.left_collapsed[1]; }
    if let Some(content) = grid_content {
        // Add view mode toolbar inside the 2D grid panel
        let view_toolbar_height = 22.0;
        let view_toolbar_rect = Rect::new(content.x, content.y, content.w, view_toolbar_height);
        let grid_view_rect = Rect::new(content.x, content.y + view_toolbar_height, content.w, content.h - view_toolbar_height);

        // Draw view mode toolbar
        draw_rectangle(view_toolbar_rect.x, view_toolbar_rect.y, view_toolbar_rect.w, view_toolbar_rect.h, Color::from_rgba(45, 45, 50, 255));
        let mut view_toolbar = Toolbar::new(view_toolbar_rect);

        if view_toolbar.letter_button_active(ctx, 'T', "Top view (X-Z)", state.grid_view_mode == GridViewMode::Top) {
            state.grid_view_mode = GridViewMode::Top;
        }
        if view_toolbar.letter_button_active(ctx, 'F', "Front view (X-Y)", state.grid_view_mode == GridViewMode::Front) {
            state.grid_view_mode = GridViewMode::Front;
        }
        if view_toolbar.letter_button_active(ctx, 'S', "Side view (Y-Z)", state.grid_view_mode == GridViewMode::Side) {
            state.grid_view_mode = GridViewMode::Side;
        }

        // Center 2D view on current room button (right-aligned)
        if view_toolbar.icon_button_right(ctx, icon::SQUARE_SQUARE, icon_font, "Center 2D view on current room") {
            state.center_2d_on_current_room();
        }

        draw_grid_view(ctx, grid_view_rect, state);
    }
    y += grid_h;

    // Panel 2: Rooms
    let room_h = if layout.left_collapsed[2] { header_h } else { expanded_panel_height };
    let room_rect = Rect::new(left_rect.x, y, left_rect.w, room_h);
    let (clicked, room_content) = draw_collapsible_panel(ctx, room_rect, panel_names[2], layout.left_collapsed[2], panel_bg);
    if clicked { layout.left_collapsed[2] = !layout.left_collapsed[2]; }
    if let Some(content) = room_content {
        draw_room_properties(ctx, content, state, icon_font);
    }
    y += room_h;

    // Panel 3: Debug
    let debug_h = if layout.left_collapsed[3] { header_h } else { expanded_panel_height };
    let debug_rect = Rect::new(left_rect.x, y, left_rect.w, debug_h);
    let (clicked, debug_content) = draw_collapsible_panel(ctx, debug_rect, panel_names[3], layout.left_collapsed[3], panel_bg);
    if clicked { layout.left_collapsed[3] = !layout.left_collapsed[3]; }
    if let Some(content) = debug_content {
        draw_debug_panel(ctx, content, state);
    }

    let left_panel_ms = EditorFrameTimings::elapsed_ms(left_start);

    // === 3D VIEWPORT ===
    let viewport_start = EditorFrameTimings::start();
    // Draw panel without title, then draw title with focus color
    draw_panel(center_rect, None, Color::from_rgba(25, 25, 30, 255));
    // Title bar
    let title_height = 20.0;
    draw_rectangle(center_rect.x, center_rect.y, center_rect.w, title_height, Color::from_rgba(50, 50, 60, 255));
    let title_color = if state.active_panel == super::state::ActivePanel::Viewport3D {
        Color::from_rgba(80, 180, 255, 255) // Cyan when focused
    } else {
        WHITE
    };
    draw_text("3D Viewport", center_rect.x + 5.0, center_rect.y + 14.0, 16.0, title_color);
    draw_viewport_3d(ctx, panel_content_rect(center_rect, true), state, textures, fb, input, icon_font);
    let viewport_3d_ms = EditorFrameTimings::elapsed_ms(viewport_start);

    // === RIGHT PANEL (Collapsible Sections) ===
    let right_start = EditorFrameTimings::start();
    let panel_bg = Color::from_rgba(35, 35, 40, 255);
    let header_h = COLLAPSED_PANEL_HEIGHT;

    // Count collapsed panels and calculate available height for expanded ones
    let textures_collapsed = !state.textures_section_expanded;
    let properties_collapsed = !state.properties_section_expanded;
    let num_collapsed = [textures_collapsed, properties_collapsed].iter().filter(|&&c| c).count();
    let collapsed_height = num_collapsed as f32 * header_h;
    let available_height = (right_rect.h - collapsed_height).max(0.0);

    // Calculate heights for expanded panels (equal distribution)
    let num_expanded = 2 - num_collapsed;
    let expanded_panel_height = if num_expanded > 0 {
        available_height / num_expanded as f32
    } else {
        0.0
    };

    // Panel 0: Textures
    let mut y = right_rect.y;
    let textures_h = if textures_collapsed { header_h } else { expanded_panel_height };
    let texture_rect = Rect::new(right_rect.x, y, right_rect.w, textures_h);
    let (clicked, textures_content) = draw_collapsible_panel(ctx, texture_rect, "Textures", textures_collapsed, panel_bg);
    if clicked {
        state.textures_section_expanded = !state.textures_section_expanded;
        // Also set focus when clicking on the header
        state.active_panel = super::state::ActivePanel::TexturePalette;
    }
    if let Some(content) = textures_content {
        draw_texture_palette(ctx, content, state, icon_font);
    }
    y += textures_h;

    // Panel 1: Properties
    let props_h = if properties_collapsed { header_h } else { expanded_panel_height };
    let props_rect = Rect::new(right_rect.x, y, right_rect.w, props_h);
    let (clicked, props_content) = draw_collapsible_panel(ctx, props_rect, "Properties", properties_collapsed, panel_bg);
    if clicked { state.properties_section_expanded = !state.properties_section_expanded; }
    if let Some(content) = props_content {
        draw_properties(ctx, content, state, icon_font);
    }

    let right_panel_ms = EditorFrameTimings::elapsed_ms(right_start);

    // === STATUS BAR ===
    let status_start = EditorFrameTimings::start();
    draw_status_bar(status_rect, state);
    let status_ms = EditorFrameTimings::elapsed_ms(status_start);

    // Store frame timings (viewport sub-timings are stored by viewport_3d.rs)
    state.frame_timings.total_ms = EditorFrameTimings::elapsed_ms(frame_start);
    state.frame_timings.toolbar_ms = toolbar_ms;
    state.frame_timings.left_panel_ms = left_panel_ms;
    state.frame_timings.viewport_3d_ms = viewport_3d_ms;
    state.frame_timings.right_panel_ms = right_panel_ms;
    state.frame_timings.status_ms = status_ms;

    // Update memory stats (not every frame - every 30 frames to reduce overhead)
    static mut FRAME_COUNTER: u32 = 0;
    unsafe {
        FRAME_COUNTER += 1;
        if FRAME_COUNTER % 30 == 0 {
            state.memory_stats.update_process_memory();

            // Calculate texture memory (4 bytes per pixel for Color struct)
            let mut tex_bytes = 0usize;
            let mut tex_count = 0usize;
            for pack in &state.texture_packs {
                for tex in &pack.textures {
                    tex_bytes += tex.width * tex.height * 4; // Color is 4 bytes (r,g,b,blend)
                    tex_count += 1;
                }
            }
            state.memory_stats.texture_bytes = tex_bytes;
            state.memory_stats.texture_count = tex_count;

            // RGB555 texture cache (2 bytes per pixel)
            let mut tex15_bytes = 0usize;
            for tex in &state.textures_15_cache {
                tex15_bytes += tex.width * tex.height * 2; // Color15 is 2 bytes
            }
            state.memory_stats.texture15_bytes = tex15_bytes;

            // Framebuffer: 320x240 x (4 bytes RGBA + 4 bytes zbuffer)
            state.memory_stats.framebuffer_bytes = 320 * 240 * 8;

            // GPU texture cache count
            state.memory_stats.gpu_cache_count = state.gpu_texture_cache.len();
        }
    }

    // Draw Asset Browser modal (if open)
    let browser_action = draw_asset_browser(ctx, &mut state.asset_browser, icon_font, fb);
    match browser_action {
        AssetBrowserAction::SelectPreview(idx) => {
            // Load the selected asset for preview (with texture resolution)
            if let Some(asset_info) = state.asset_browser.assets.get(idx) {
                if let Some(asset) = state.asset_library.get(&asset_info.name) {
                    state.asset_browser.set_preview(asset.clone(), &state.user_textures);
                }
            }
        }
        AssetBrowserAction::OpenAsset => {
            // User confirmed selection - set selected_asset and close browser
            if let Some(asset_info) = state.asset_browser.selected_asset() {
                state.selected_asset = Some(asset_info.name.clone());
            }
            state.asset_browser.close();
        }
        AssetBrowserAction::Cancel | AssetBrowserAction::NewAsset => {
            // Close browser (NewAsset doesn't apply to World Editor)
            state.asset_browser.close();
        }
        AssetBrowserAction::None => {}
    }

    action
}

fn draw_unified_toolbar(ctx: &mut UiContext, rect: Rect, state: &mut EditorState, icon_font: Option<&Font>, actions: &ActionRegistry) -> EditorAction {
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

    // Tool buttons (Portal removed - portals are now auto-generated)
    // Wall tool handles all 6 directions (N, E, S, W, NW-SE, NE-SW) - use R to rotate
    let tools = [
        (icon::MOVE, "Select", EditorTool::Select),
        (icon::SQUARE, "Floor", EditorTool::DrawFloor),
        (icon::BRICK_WALL, "Wall", EditorTool::DrawWall),
        (icon::LAYERS, "Ceiling", EditorTool::DrawCeiling),
        (icon::MAP_PIN, "Object", EditorTool::PlaceObject),
    ];

    for (icon_char, tooltip, tool) in tools {
        let is_active = state.tool == tool;
        if toolbar.icon_button_active(ctx, icon_char, icon_font, tooltip, is_active) {
            state.tool = tool;
            // Show direction hint when selecting wall tool
            if tool == EditorTool::DrawWall {
                state.set_status(&format!("Wall direction: {} (R to rotate, F for gap)", state.wall_direction.name()), 2.0);
            }
        }
    }

    // Asset picker - always visible
    // Shows "< asset_name [browse] >" - clicking name activates PlaceObject, chevrons cycle + activate
    if !state.asset_library.is_empty() {
        toolbar.separator();

        // Collect asset names
        let asset_names: Vec<&str> = state.asset_library.names().collect();

        // Auto-select first asset if none selected
        if state.selected_asset.is_none() && !asset_names.is_empty() {
            state.selected_asset = Some(asset_names[0].to_string());
        }

        // Find current asset index
        let (current_asset_idx, current_label) = if let Some(ref selected) = state.selected_asset {
            let idx = asset_names.iter().position(|n| *n == selected.as_str()).unwrap_or(0);
            (idx, asset_names.get(idx).copied().unwrap_or("(none)"))
        } else {
            (0, "(none)")
        };

        let label = current_label.to_string();

        // Highlight if PlaceObject mode is active
        let is_active = state.tool == EditorTool::PlaceObject;

        // Draw "< Asset >" navigation - clicking arrows or label activates PlaceObject mode
        let picker_clicked = toolbar.arrow_picker_active(ctx, icon_font, &label, is_active, &mut |delta: i32| {
            // Activate PlaceObject mode when cycling
            state.tool = EditorTool::PlaceObject;
            if asset_names.is_empty() {
                return;
            }
            let new_idx = (current_asset_idx as i32 + delta).rem_euclid(asset_names.len() as i32) as usize;
            state.selected_asset = Some(asset_names[new_idx].to_string());
        });
        if picker_clicked {
            // Label was clicked - activate PlaceObject mode
            state.tool = EditorTool::PlaceObject;
        }

        // Browse assets button (opens Asset Browser modal)
        if toolbar.icon_button(ctx, icon::BOOK_OPEN, icon_font, "Browse Assets") {
            state.tool = EditorTool::PlaceObject;
            // Open the asset browser with current assets
            let assets: Vec<_> = state.asset_library.names()
                .map(|name| crate::modeler::AssetInfo {
                    name: name.to_string(),
                    path: std::path::PathBuf::from(format!("assets/userdata/assets/{}.ron", name)),
                })
                .collect();
            state.asset_browser.open(assets);
        }
    }

    toolbar.separator();

    // Vertex mode toggle
    let link_icon = if state.link_coincident_vertices { icon::LINK } else { icon::LINK_OFF };
    let link_tooltip = if state.link_coincident_vertices { "Geometry Linked" } else { "Geometry Independent" };
    if toolbar.icon_button_active(ctx, link_icon, icon_font, link_tooltip, state.link_coincident_vertices) {
        state.link_coincident_vertices = !state.link_coincident_vertices;
        let mode = if state.link_coincident_vertices { "Linked" } else { "Independent" };
        state.set_status(&format!("Vertex mode: {}", mode), 2.0);
    }

    toolbar.separator();

    // Camera mode toggle (single button that cycles between modes)
    use super::CameraMode;
    let (camera_icon, camera_tooltip) = match state.camera_mode {
        CameraMode::Free => (icon::EYE, "Camera: Free (WASD)"),
        CameraMode::Orbit => (icon::ORBIT, "Camera: Orbit"),
    };
    if toolbar.icon_button(ctx, camera_icon, icon_font, camera_tooltip) {
        match state.camera_mode {
            CameraMode::Free => {
                state.camera_mode = CameraMode::Orbit;
                state.update_orbit_target();
                state.sync_camera_from_orbit();
                state.set_status("Camera: Orbit (drag to rotate)", 2.0);
            }
            CameraMode::Orbit => {
                state.camera_mode = CameraMode::Free;
                state.set_status("Camera: Free (WASD + mouse)", 2.0);
            }
        }
    }

    // Room boundaries toggle
    let room_bounds_tooltip = if state.show_room_bounds { "Room Bounds: ON" } else { "Room Bounds: OFF" };
    if toolbar.icon_button_active(ctx, icon::BOX, icon_font, room_bounds_tooltip, state.show_room_bounds) {
        state.show_room_bounds = !state.show_room_bounds;
        let mode = if state.show_room_bounds { "visible" } else { "hidden" };
        state.set_status(&format!("Room boundaries: {}", mode), 2.0);
    }

    // Wireframe toggle
    let wireframe_tooltip = if state.raster_settings.wireframe_overlay { "Wireframe: ON" } else { "Wireframe: OFF" };
    if toolbar.icon_button_active(ctx, icon::GRID, icon_font, wireframe_tooltip, state.raster_settings.wireframe_overlay) {
        state.raster_settings.wireframe_overlay = !state.raster_settings.wireframe_overlay;
        let mode = if state.raster_settings.wireframe_overlay { "ON" } else { "OFF" };
        state.set_status(&format!("Wireframe: {}", mode), 2.0);
    }

    // Backface culling toggle (cycles through 3 states)
    // State 0: Both sides visible (backface_cull=false)
    // State 1: Wireframe on back (backface_cull=true, backface_wireframe=true) - default
    // State 2: Hidden (backface_cull=true, backface_wireframe=false)
    let (backface_icon, backface_tooltip) = if !state.raster_settings.backface_cull {
        (icon::EYE, "Backfaces: Both Sides Visible")
    } else if state.raster_settings.backface_wireframe {
        (icon::SCAN, "Backfaces: Wireframe")
    } else {
        (icon::EYE_OFF, "Backfaces: Hidden")
    };
    if toolbar.icon_button(ctx, backface_icon, icon_font, backface_tooltip) {
        // Cycle to next state
        if !state.raster_settings.backface_cull {
            // Was: both visible → Now: wireframe on back
            state.raster_settings.backface_cull = true;
            state.raster_settings.backface_wireframe = true;
            state.set_status("Backfaces: Wireframe", 2.0);
        } else if state.raster_settings.backface_wireframe {
            // Was: wireframe → Now: hidden
            state.raster_settings.backface_wireframe = false;
            state.set_status("Backfaces: Hidden", 2.0);
        } else {
            // Was: hidden → Now: both visible
            state.raster_settings.backface_cull = false;
            state.set_status("Backfaces: Both Sides Visible", 2.0);
        }
    }

    toolbar.separator();

    // PS1 effect toggles
    if toolbar.icon_button_active(ctx, icon::WAVES, icon_font, "Affine Textures (PS1 warp)", state.raster_settings.affine_textures) {
        state.raster_settings.affine_textures = !state.raster_settings.affine_textures;
        let mode = if state.raster_settings.affine_textures { "ON" } else { "OFF" };
        state.set_status(&format!("Affine textures: {}", mode), 2.0);
    }
    if toolbar.icon_button_active(ctx, icon::HASH, icon_font, "Fixed-Point Math (PS1 jitter)", state.raster_settings.use_fixed_point) {
        state.raster_settings.use_fixed_point = !state.raster_settings.use_fixed_point;
        let mode = if state.raster_settings.use_fixed_point { "ON" } else { "OFF" };
        state.set_status(&format!("Fixed-point: {}", mode), 2.0);
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
    // Z-buffer toggle (ON = z-buffer, OFF = painter's algorithm)
    if toolbar.icon_button_active(ctx, icon::ARROW_DOWN_UP, icon_font, "Z-Buffer (OFF = painter's algorithm)", state.raster_settings.use_zbuffer) {
        state.raster_settings.use_zbuffer = !state.raster_settings.use_zbuffer;
        let mode = if state.raster_settings.use_zbuffer { "Z-Buffer" } else { "Painter's Algorithm" };
        state.set_status(&format!("Depth: {}", mode), 2.0);
    }
    // RGB555 toggle (PS1-authentic 15-bit color mode)
    if toolbar.icon_button_active(ctx, icon::PALETTE, icon_font, "RGB555 (PS1 15-bit color mode)", state.raster_settings.use_rgb555) {
        state.raster_settings.use_rgb555 = !state.raster_settings.use_rgb555;
        let mode = if state.raster_settings.use_rgb555 { "RGB555 (15-bit)" } else { "RGB888 (24-bit)" };
        state.set_status(&format!("Color: {}", mode), 2.0);
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

    // Build action context for keyboard shortcuts
    let has_object_selection = matches!(&state.selection, Selection::Object { .. });
    let has_sector_selection = matches!(&state.selection, Selection::Sector { .. } | Selection::SectorFace { .. });
    let has_selection = has_object_selection || has_sector_selection;

    let mut selection_flags = 0u32;
    if has_object_selection {
        selection_flags |= flags::OBJECT_SELECTED;
    }
    if has_sector_selection {
        selection_flags |= flags::SECTOR_SELECTED;
    }

    let actx = build_context(
        !state.undo_stack.is_empty(),
        !state.redo_stack.is_empty(),
        has_selection,
        state.clipboard.is_some() || state.face_clipboard.is_some(),
        selection_flags,
        false, // text_editing
        state.dirty,
    );

    // File actions
    if actions.triggered("file.new", &actx) {
        action = EditorAction::New;
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        if actions.triggered("file.open", &actx) {
            action = EditorAction::PromptLoad;
        }
        if actions.triggered("file.save_as", &actx) {
            action = EditorAction::SaveAs;
        } else if actions.triggered("file.save", &actx) {
            action = EditorAction::Save;
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        if actions.triggered("file.open", &actx) {
            action = EditorAction::Import;
        }
        if actions.triggered("file.save", &actx) {
            action = EditorAction::Export;
        }
    }

    // Edit actions (unified undo/redo for both level and selection changes)
    if actions.triggered("edit.undo", &actx) {
        state.undo();
    }
    if actions.triggered("edit.redo", &actx) {
        state.redo();
    }

    // Copy selected object or face(s)
    if actions.triggered("edit.copy", &actx) {
        // Check if we have multiple faces or sectors selected - if so, copy geometry
        let has_multi_faces = !state.multi_selection.is_empty() &&
            state.multi_selection.iter().any(|s| matches!(s, Selection::SectorFace { .. }));
        let has_multi_sectors = !state.multi_selection.is_empty() &&
            state.multi_selection.iter().any(|s| matches!(s, Selection::Sector { .. }));

        if has_multi_faces || has_multi_sectors {
            // Copy entire geometry (all selected faces/sectors with positions)
            copy_geometry_selection(state);
        } else {
            // Single selection - copy properties or object
            match &state.selection {
                Selection::Object { room, index } => {
                    if let Some(r) = state.level.rooms.get(*room) {
                        if let Some(obj) = r.objects.get(*index) {
                            state.clipboard = Some(obj.clone());
                            state.set_status("Object copied to clipboard", 2.0);
                        }
                    }
                }
                Selection::SectorFace { room, x, z, face } => {
                    if let Some(r) = state.level.rooms.get(*room) {
                        if let Some(sector) = r.get_sector(*x, *z) {
                            // Copy face properties based on face type
                            let copied = match face {
                                SectorFace::Floor => {
                                    sector.floor.as_ref().map(|f| FaceClipboard::Horizontal {
                                        split_direction: f.split_direction,
                                        texture: f.texture.clone(),
                                        uv: f.uv,
                                        colors: f.colors,
                                        texture_2: f.texture_2.clone(),
                                        uv_2: f.uv_2,
                                        colors_2: f.colors_2,
                                        walkable: f.walkable,
                                        blend_mode: f.blend_mode,
                                        normal_mode: f.normal_mode,
                                        black_transparent: f.black_transparent,
                                    })
                                }
                                SectorFace::Ceiling => {
                                    sector.ceiling.as_ref().map(|f| FaceClipboard::Horizontal {
                                        split_direction: f.split_direction,
                                        texture: f.texture.clone(),
                                        uv: f.uv,
                                        colors: f.colors,
                                        texture_2: f.texture_2.clone(),
                                        uv_2: f.uv_2,
                                        colors_2: f.colors_2,
                                        walkable: f.walkable,
                                        blend_mode: f.blend_mode,
                                        normal_mode: f.normal_mode,
                                        black_transparent: f.black_transparent,
                                    })
                                }
                                SectorFace::WallNorth(i) => {
                                    sector.walls_north.get(*i).map(|w| FaceClipboard::Vertical {
                                        texture: w.texture.clone(),
                                        uv: w.uv,
                                        solid: w.solid,
                                        blend_mode: w.blend_mode,
                                        colors: w.colors,
                                        normal_mode: w.normal_mode,
                                        black_transparent: w.black_transparent,
                                        uv_projection: w.uv_projection,
                                    })
                                }
                                SectorFace::WallEast(i) => {
                                    sector.walls_east.get(*i).map(|w| FaceClipboard::Vertical {
                                        texture: w.texture.clone(),
                                        uv: w.uv,
                                        solid: w.solid,
                                        blend_mode: w.blend_mode,
                                        colors: w.colors,
                                        normal_mode: w.normal_mode,
                                        black_transparent: w.black_transparent,
                                        uv_projection: w.uv_projection,
                                    })
                                }
                                SectorFace::WallSouth(i) => {
                                    sector.walls_south.get(*i).map(|w| FaceClipboard::Vertical {
                                        texture: w.texture.clone(),
                                        uv: w.uv,
                                        solid: w.solid,
                                        blend_mode: w.blend_mode,
                                        colors: w.colors,
                                        normal_mode: w.normal_mode,
                                        black_transparent: w.black_transparent,
                                        uv_projection: w.uv_projection,
                                    })
                                }
                                SectorFace::WallWest(i) => {
                                    sector.walls_west.get(*i).map(|w| FaceClipboard::Vertical {
                                        texture: w.texture.clone(),
                                        uv: w.uv,
                                        solid: w.solid,
                                        blend_mode: w.blend_mode,
                                        colors: w.colors,
                                        normal_mode: w.normal_mode,
                                        black_transparent: w.black_transparent,
                                        uv_projection: w.uv_projection,
                                    })
                                }
                                SectorFace::WallNwSe(i) => {
                                    sector.walls_nwse.get(*i).map(|w| FaceClipboard::Vertical {
                                        texture: w.texture.clone(),
                                        uv: w.uv,
                                        solid: w.solid,
                                        blend_mode: w.blend_mode,
                                        colors: w.colors,
                                        normal_mode: w.normal_mode,
                                        black_transparent: w.black_transparent,
                                        uv_projection: w.uv_projection,
                                    })
                                }
                                SectorFace::WallNeSw(i) => {
                                    sector.walls_nesw.get(*i).map(|w| FaceClipboard::Vertical {
                                        texture: w.texture.clone(),
                                        uv: w.uv,
                                        solid: w.solid,
                                        blend_mode: w.blend_mode,
                                        colors: w.colors,
                                        normal_mode: w.normal_mode,
                                        black_transparent: w.black_transparent,
                                        uv_projection: w.uv_projection,
                                    })
                                }
                            };
                            if let Some(fc) = copied {
                                state.face_clipboard = Some(fc);
                                state.set_status("Face properties copied", 2.0);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Paste object or face properties
    if actions.triggered("edit.paste", &actx) {
        // Collect all face selections (primary + multi)
        let mut face_selections: Vec<(usize, usize, usize, SectorFace)> = Vec::new();
        if let Selection::SectorFace { room, x, z, face } = &state.selection {
            face_selections.push((*room, *x, *z, face.clone()));
        }
        for sel in &state.multi_selection {
            if let Selection::SectorFace { room, x, z, face } = sel {
                face_selections.push((*room, *x, *z, face.clone()));
            }
        }

        if !face_selections.is_empty() {
            if let Some(fc) = &state.face_clipboard {
                let fc_clone = fc.clone();

                // Check compatibility with first selection (all should be same type)
                let compatible = match (&face_selections[0].3, &fc_clone) {
                    (SectorFace::Floor, FaceClipboard::Horizontal { .. }) |
                    (SectorFace::Ceiling, FaceClipboard::Horizontal { .. }) => true,
                    (SectorFace::WallNorth(_), FaceClipboard::Vertical { .. }) |
                    (SectorFace::WallEast(_), FaceClipboard::Vertical { .. }) |
                    (SectorFace::WallSouth(_), FaceClipboard::Vertical { .. }) |
                    (SectorFace::WallWest(_), FaceClipboard::Vertical { .. }) |
                    (SectorFace::WallNwSe(_), FaceClipboard::Vertical { .. }) |
                    (SectorFace::WallNeSw(_), FaceClipboard::Vertical { .. }) => true,
                    _ => false,
                };

                if compatible {
                    // Save undo BEFORE getting mutable borrow
                    state.save_undo();

                    let mut paste_count = 0;
                    for (room_idx, sx, sz, target_face) in face_selections {
                        let success = if let Some(room) = state.level.rooms.get_mut(room_idx) {
                            if let Some(sector) = room.get_sector_mut(sx, sz) {
                                paste_face_properties(sector, &target_face, &fc_clone)
                            } else { false }
                        } else { false };
                        if success {
                            paste_count += 1;
                        }
                    }

                    if paste_count > 0 {
                        if paste_count == 1 {
                            state.set_status("Face properties pasted", 2.0);
                        } else {
                            state.set_status(&format!("Face properties pasted to {} faces", paste_count), 2.0);
                        }
                    }
                } else {
                    state.set_status("Cannot paste: incompatible face types", 2.0);
                }
            } else if let Some(copied) = state.clipboard.clone() {
                // Fall back to object paste if no face clipboard
                paste_object(state, copied);
            } else {
                state.set_status("Nothing in clipboard", 2.0);
            }
        } else if let Some(gc) = state.geometry_clipboard.clone() {
            // Paste geometry at selected sector
            paste_geometry_selection(state, &gc);
        } else if let Some(copied) = state.clipboard.clone() {
            // Regular object paste
            paste_object(state, copied);
        } else {
            state.set_status("Nothing in clipboard", 2.0);
        }
    }

    action
}

/// Helper function to paste an asset instance at the selected sector
fn paste_object(state: &mut EditorState, copied: crate::world::AssetInstance) {
    // Get target sector from selection
    let target = match &state.selection {
        Selection::Sector { room, x, z } => Some((*room, *x, *z)),
        Selection::SectorFace { room, x, z, .. } => Some((*room, *x, *z)),
        Selection::Object { room, index } => {
            // If an object is selected, paste to that object's sector
            state.level.rooms.get(*room).and_then(|r| {
                r.objects.get(*index).map(|obj| (*room, obj.sector_x, obj.sector_z))
            })
        }
        _ => None,
    };

    if let Some((room_idx, sector_x, sector_z)) = target {
        // Create a new object with the copied properties but at the target sector
        let mut new_obj = copied;
        new_obj.sector_x = sector_x;
        new_obj.sector_z = sector_z;

        state.save_undo();
        // Add to the room
        if let Some(room) = state.level.rooms.get_mut(room_idx) {
            let new_index = room.objects.len();
            room.objects.push(new_obj);
            state.set_selection(Selection::Object { room: room_idx, index: new_index });
            state.set_status("Object pasted", 2.0);
        }
    } else {
        state.set_status("Select a sector to paste into", 2.0);
    }
}

/// Helper function to paste face properties from clipboard to a sector face
fn paste_face_properties(sector: &mut Sector, target_face: &SectorFace, fc: &FaceClipboard) -> bool {
    match (target_face, fc) {
        // Horizontal -> Horizontal (Floor)
        (SectorFace::Floor, FaceClipboard::Horizontal {
            split_direction, texture, uv, colors,
            texture_2, uv_2, colors_2, walkable,
            blend_mode, normal_mode, black_transparent
        }) => {
            if let Some(f) = sector.floor.as_mut() {
                f.split_direction = *split_direction;
                f.texture = texture.clone();
                f.uv = *uv;
                f.colors = *colors;
                f.texture_2 = texture_2.clone();
                f.uv_2 = *uv_2;
                f.colors_2 = *colors_2;
                f.walkable = *walkable;
                f.blend_mode = *blend_mode;
                f.normal_mode = *normal_mode;
                f.black_transparent = *black_transparent;
                true
            } else { false }
        }
        // Horizontal -> Horizontal (Ceiling)
        (SectorFace::Ceiling, FaceClipboard::Horizontal {
            split_direction, texture, uv, colors,
            texture_2, uv_2, colors_2, walkable,
            blend_mode, normal_mode, black_transparent
        }) => {
            if let Some(f) = sector.ceiling.as_mut() {
                f.split_direction = *split_direction;
                f.texture = texture.clone();
                f.uv = *uv;
                f.colors = *colors;
                f.texture_2 = texture_2.clone();
                f.uv_2 = *uv_2;
                f.colors_2 = *colors_2;
                f.walkable = *walkable;
                f.blend_mode = *blend_mode;
                f.normal_mode = *normal_mode;
                f.black_transparent = *black_transparent;
                true
            } else { false }
        }
        // Vertical -> Vertical (walls)
        (SectorFace::WallNorth(i), FaceClipboard::Vertical {
            texture, uv, solid, blend_mode, colors,
            normal_mode, black_transparent, uv_projection
        }) => {
            if let Some(w) = sector.walls_north.get_mut(*i) {
                w.texture = texture.clone();
                w.uv = *uv;
                w.solid = *solid;
                w.blend_mode = *blend_mode;
                w.colors = *colors;
                w.normal_mode = *normal_mode;
                w.black_transparent = *black_transparent;
                w.uv_projection = *uv_projection;
                true
            } else { false }
        }
        (SectorFace::WallEast(i), FaceClipboard::Vertical {
            texture, uv, solid, blend_mode, colors,
            normal_mode, black_transparent, uv_projection
        }) => {
            if let Some(w) = sector.walls_east.get_mut(*i) {
                w.texture = texture.clone();
                w.uv = *uv;
                w.solid = *solid;
                w.blend_mode = *blend_mode;
                w.colors = *colors;
                w.normal_mode = *normal_mode;
                w.black_transparent = *black_transparent;
                w.uv_projection = *uv_projection;
                true
            } else { false }
        }
        (SectorFace::WallSouth(i), FaceClipboard::Vertical {
            texture, uv, solid, blend_mode, colors,
            normal_mode, black_transparent, uv_projection
        }) => {
            if let Some(w) = sector.walls_south.get_mut(*i) {
                w.texture = texture.clone();
                w.uv = *uv;
                w.solid = *solid;
                w.blend_mode = *blend_mode;
                w.colors = *colors;
                w.normal_mode = *normal_mode;
                w.black_transparent = *black_transparent;
                w.uv_projection = *uv_projection;
                true
            } else { false }
        }
        (SectorFace::WallWest(i), FaceClipboard::Vertical {
            texture, uv, solid, blend_mode, colors,
            normal_mode, black_transparent, uv_projection
        }) => {
            if let Some(w) = sector.walls_west.get_mut(*i) {
                w.texture = texture.clone();
                w.uv = *uv;
                w.solid = *solid;
                w.blend_mode = *blend_mode;
                w.colors = *colors;
                w.normal_mode = *normal_mode;
                w.black_transparent = *black_transparent;
                w.uv_projection = *uv_projection;
                true
            } else { false }
        }
        (SectorFace::WallNwSe(i), FaceClipboard::Vertical {
            texture, uv, solid, blend_mode, colors,
            normal_mode, black_transparent, uv_projection
        }) => {
            if let Some(w) = sector.walls_nwse.get_mut(*i) {
                w.texture = texture.clone();
                w.uv = *uv;
                w.solid = *solid;
                w.blend_mode = *blend_mode;
                w.colors = *colors;
                w.normal_mode = *normal_mode;
                w.black_transparent = *black_transparent;
                w.uv_projection = *uv_projection;
                true
            } else { false }
        }
        (SectorFace::WallNeSw(i), FaceClipboard::Vertical {
            texture, uv, solid, blend_mode, colors,
            normal_mode, black_transparent, uv_projection
        }) => {
            if let Some(w) = sector.walls_nesw.get_mut(*i) {
                w.texture = texture.clone();
                w.uv = *uv;
                w.solid = *solid;
                w.blend_mode = *blend_mode;
                w.colors = *colors;
                w.normal_mode = *normal_mode;
                w.black_transparent = *black_transparent;
                w.uv_projection = *uv_projection;
                true
            } else { false }
        }
        _ => false,
    }
}

/// Copy all selected faces as geometry (with relative positions)
/// Handles both SectorFace selections (individual faces) and Sector selections (all faces in sector)
fn copy_geometry_selection(state: &mut EditorState) {
    // Collect all sector positions that need their faces extracted
    let mut sector_positions: Vec<(usize, usize, usize)> = Vec::new();
    let mut all_faces: Vec<(usize, usize, usize, SectorFace)> = Vec::new();

    // Handle primary selection
    match &state.selection {
        Selection::SectorFace { room, x, z, face } => {
            all_faces.push((*room, *x, *z, face.clone()));
        }
        Selection::Sector { room, x, z } => {
            sector_positions.push((*room, *x, *z));
        }
        _ => {}
    }

    // Handle multi-selection
    for sel in &state.multi_selection {
        match sel {
            Selection::SectorFace { room, x, z, face } => {
                all_faces.push((*room, *x, *z, face.clone()));
            }
            Selection::Sector { room, x, z } => {
                sector_positions.push((*room, *x, *z));
            }
            _ => {}
        }
    }

    // Extract faces from sector positions
    for (room_idx, x, z) in sector_positions {
        if let Some(room) = state.level.rooms.get(room_idx) {
            if let Some(sector) = room.get_sector(x, z) {
                // Add floor if present
                if sector.floor.is_some() {
                    all_faces.push((room_idx, x, z, SectorFace::Floor));
                }
                // Add ceiling if present
                if sector.ceiling.is_some() {
                    all_faces.push((room_idx, x, z, SectorFace::Ceiling));
                }
                // Add all walls
                for i in 0..sector.walls_north.len() {
                    all_faces.push((room_idx, x, z, SectorFace::WallNorth(i)));
                }
                for i in 0..sector.walls_east.len() {
                    all_faces.push((room_idx, x, z, SectorFace::WallEast(i)));
                }
                for i in 0..sector.walls_south.len() {
                    all_faces.push((room_idx, x, z, SectorFace::WallSouth(i)));
                }
                for i in 0..sector.walls_west.len() {
                    all_faces.push((room_idx, x, z, SectorFace::WallWest(i)));
                }
                for i in 0..sector.walls_nwse.len() {
                    all_faces.push((room_idx, x, z, SectorFace::WallNwSe(i)));
                }
                for i in 0..sector.walls_nesw.len() {
                    all_faces.push((room_idx, x, z, SectorFace::WallNeSw(i)));
                }
            }
        }
    }

    if all_faces.is_empty() {
        state.set_status("No geometry to copy", 2.0);
        return;
    }

    // Find anchor point (minimum x, z coordinates)
    let anchor_x = all_faces.iter().map(|(_, x, _, _)| *x as i32).min().unwrap_or(0);
    let anchor_z = all_faces.iter().map(|(_, _, z, _)| *z as i32).min().unwrap_or(0);

    let mut copied_faces: Vec<CopiedFace> = Vec::new();

    for (room_idx, sx, sz, face) in &all_faces {
        let rel_x = *sx as i32 - anchor_x;
        let rel_z = *sz as i32 - anchor_z;

        if let Some(room) = state.level.rooms.get(*room_idx) {
            if let Some(sector) = room.get_sector(*sx, *sz) {
                let face_data = match face {
                    SectorFace::Floor => {
                        sector.floor.as_ref().map(|f| CopiedFaceData::Floor(f.clone()))
                    }
                    SectorFace::Ceiling => {
                        sector.ceiling.as_ref().map(|f| CopiedFaceData::Ceiling(f.clone()))
                    }
                    SectorFace::WallNorth(i) => {
                        sector.walls_north.get(*i).map(|w| CopiedFaceData::WallNorth(*i, w.clone()))
                    }
                    SectorFace::WallEast(i) => {
                        sector.walls_east.get(*i).map(|w| CopiedFaceData::WallEast(*i, w.clone()))
                    }
                    SectorFace::WallSouth(i) => {
                        sector.walls_south.get(*i).map(|w| CopiedFaceData::WallSouth(*i, w.clone()))
                    }
                    SectorFace::WallWest(i) => {
                        sector.walls_west.get(*i).map(|w| CopiedFaceData::WallWest(*i, w.clone()))
                    }
                    SectorFace::WallNwSe(i) => {
                        sector.walls_nwse.get(*i).map(|w| CopiedFaceData::WallNwSe(*i, w.clone()))
                    }
                    SectorFace::WallNeSw(i) => {
                        sector.walls_nesw.get(*i).map(|w| CopiedFaceData::WallNeSw(*i, w.clone()))
                    }
                };

                if let Some(data) = face_data {
                    copied_faces.push(CopiedFace {
                        rel_x,
                        rel_z,
                        face: data,
                    });
                }
            }
        }
    }

    if !copied_faces.is_empty() {
        let count = copied_faces.len();
        state.geometry_clipboard = Some(GeometryClipboard {
            faces: copied_faces,
            flip_h: false,
            flip_v: false,
            rotation: 0,
        });
        state.set_status(&format!("Copied {} faces to geometry clipboard", count), 2.0);
    }
}

/// Paste geometry from clipboard at the selected/hovered sector
fn paste_geometry_selection(state: &mut EditorState, gc: &GeometryClipboard) {
    // Get anchor point from current selection (Sector or SectorFace)
    let anchor = match &state.selection {
        Selection::Sector { room, x, z } => Some((*room, *x as i32, *z as i32)),
        Selection::SectorFace { room, x, z, .. } => Some((*room, *x as i32, *z as i32)),
        _ => None,
    };

    let Some((room_idx, anchor_x, anchor_z)) = anchor else {
        state.set_status("Select a sector to paste geometry", 2.0);
        return;
    };

    paste_geometry_at_impl(state, gc, room_idx, anchor_x, anchor_z);
}

/// Transform position based on rotation (0-3 = 0°, 90°, 180°, 270° CW) and flips
/// Returns (new_rel_x, new_rel_z, effective_width, effective_depth)
fn transform_clipboard_position(
    rel_x: i32, rel_z: i32,
    width: i32, depth: i32,
    rotation: u8, flip_h: bool, flip_v: bool,
) -> (i32, i32, i32, i32) {
    // Apply rotation first
    let (rx, rz, rw, rd) = match rotation {
        1 => (depth - rel_z, rel_x, depth, width),         // 90° CW
        2 => (width - rel_x, depth - rel_z, width, depth), // 180°
        3 => (rel_z, width - rel_x, depth, width),         // 270° CW
        _ => (rel_x, rel_z, width, depth),                 // 0° (no rotation)
    };

    // Then apply flips
    let (fx, fz) = match (flip_h, flip_v) {
        (true, true) => (rw - rx, rd - rz),
        (true, false) => (rw - rx, rz),
        (false, true) => (rx, rd - rz),
        (false, false) => (rx, rz),
    };

    (fx, fz, rw, rd)
}

/// Rotate heights array for horizontal faces (90° CW per step)
fn rotate_heights(heights: [f32; 4], rotation: u8) -> [f32; 4] {
    match rotation {
        1 => [heights[3], heights[0], heights[1], heights[2]], // 90° CW: SW→NW, NW→NE, NE→SE, SE→SW
        2 => [heights[2], heights[3], heights[0], heights[1]], // 180°
        3 => [heights[1], heights[2], heights[3], heights[0]], // 270° CW
        _ => heights,
    }
}

/// Rotate colors array for horizontal faces (90° CW per step)
/// Same corner ordering as heights: [NW, NE, SE, SW]
fn rotate_colors(colors: [RasterColor; 4], rotation: u8) -> [RasterColor; 4] {
    match rotation {
        1 => [colors[3], colors[0], colors[1], colors[2]], // 90° CW
        2 => [colors[2], colors[3], colors[0], colors[1]], // 180°
        3 => [colors[1], colors[2], colors[3], colors[0]], // 270° CW
        _ => colors,
    }
}

/// Wall direction for rotation and flip calculations
#[derive(Clone, Copy, PartialEq)]
enum WallDir { North, East, South, West, NwSe, NeSw }

/// Transform wall direction based on rotation and flips
fn transform_wall_direction(dir: WallDir, rotation: u8, flip_h: bool, flip_v: bool) -> WallDir {
    // First apply rotation (clockwise)
    let rotated = match rotation % 4 {
        1 => match dir { // 90° CW
            WallDir::North => WallDir::East,
            WallDir::East => WallDir::South,
            WallDir::South => WallDir::West,
            WallDir::West => WallDir::North,
            WallDir::NwSe => WallDir::NeSw,
            WallDir::NeSw => WallDir::NwSe,
        },
        2 => match dir { // 180°
            WallDir::North => WallDir::South,
            WallDir::East => WallDir::West,
            WallDir::South => WallDir::North,
            WallDir::West => WallDir::East,
            WallDir::NwSe => WallDir::NwSe,
            WallDir::NeSw => WallDir::NeSw,
        },
        3 => match dir { // 270° CW
            WallDir::North => WallDir::West,
            WallDir::East => WallDir::North,
            WallDir::South => WallDir::East,
            WallDir::West => WallDir::South,
            WallDir::NwSe => WallDir::NeSw,
            WallDir::NeSw => WallDir::NwSe,
        },
        _ => dir, // 0°
    };

    // Then apply flips
    match (flip_h, flip_v) {
        (true, true) => match rotated {
            WallDir::North => WallDir::South,
            WallDir::South => WallDir::North,
            WallDir::East => WallDir::West,
            WallDir::West => WallDir::East,
            d => d, // Diagonal: both flips = no change
        },
        (true, false) => match rotated {
            WallDir::East => WallDir::West,
            WallDir::West => WallDir::East,
            WallDir::NwSe => WallDir::NeSw,
            WallDir::NeSw => WallDir::NwSe,
            d => d,
        },
        (false, true) => match rotated {
            WallDir::North => WallDir::South,
            WallDir::South => WallDir::North,
            WallDir::NwSe => WallDir::NeSw,
            WallDir::NeSw => WallDir::NwSe,
            d => d,
        },
        (false, false) => rotated,
    }
}

/// Paste geometry from clipboard at specific anchor coordinates (public for viewport click)
pub fn paste_geometry_at(state: &mut EditorState, gc: &GeometryClipboard, anchor_x: i32, anchor_z: i32) {
    paste_geometry_at_impl(state, gc, state.current_room, anchor_x, anchor_z);
}

/// Internal implementation for pasting geometry
fn paste_geometry_at_impl(state: &mut EditorState, gc: &GeometryClipboard, room_idx: usize, anchor_x: i32, anchor_z: i32) {
    state.save_undo();

    let mut paste_count = 0;
    let (min_x, max_x, min_z, max_z) = gc.bounds();
    let width = max_x - min_x;
    let depth = max_z - min_z;

    // First pass: calculate bounds needed to expand room
    let mut target_min_x = i32::MAX;
    let mut target_max_x = i32::MIN;
    let mut target_min_z = i32::MAX;
    let mut target_max_z = i32::MIN;

    for face in &gc.faces {
        let (rel_x, rel_z, _, _) = transform_clipboard_position(
            face.rel_x, face.rel_z, width, depth,
            gc.rotation, gc.flip_h, gc.flip_v,
        );
        let target_x = anchor_x + rel_x;
        let target_z = anchor_z + rel_z;
        target_min_x = target_min_x.min(target_x);
        target_max_x = target_max_x.max(target_x);
        target_min_z = target_min_z.min(target_z);
        target_max_z = target_max_z.max(target_z);
    }

    // Expand room grid to accommodate all target positions
    let mut offset_x = 0i32;
    let mut offset_z = 0i32;

    if let Some(room) = state.level.rooms.get_mut(room_idx) {
        // Expand in negative X direction
        while target_min_x + offset_x < 0 {
            room.position.x -= SECTOR_SIZE;
            room.sectors.insert(0, (0..room.depth).map(|_| None).collect());
            room.width += 1;
            offset_x += 1;
        }

        // Expand in negative Z direction
        while target_min_z + offset_z < 0 {
            room.position.z -= SECTOR_SIZE;
            for col in &mut room.sectors {
                col.insert(0, None);
            }
            room.depth += 1;
            offset_z += 1;
        }

        // Expand in positive X direction
        while (target_max_x + offset_x) as usize >= room.width {
            room.width += 1;
            room.sectors.push((0..room.depth).map(|_| None).collect());
        }

        // Expand in positive Z direction
        while (target_max_z + offset_z) as usize >= room.depth {
            room.depth += 1;
            for col in &mut room.sectors {
                col.push(None);
            }
        }
    }

    // Calculate effective split direction change:
    // - Rotation by odd amount (90° or 270°) flips the diagonal
    // - Flip H XOR V also flips the diagonal
    // Combined: XOR of both conditions
    let rotation_flips_split = gc.rotation % 2 == 1;
    let flip_flips_split = gc.flip_h != gc.flip_v;
    let should_flip_split = rotation_flips_split != flip_flips_split;

    // Second pass: paste all faces with adjusted coordinates
    for face in &gc.faces {
        // Apply rotation and flip transformations
        let (rel_x, rel_z, _, _) = transform_clipboard_position(
            face.rel_x, face.rel_z, width, depth,
            gc.rotation, gc.flip_h, gc.flip_v,
        );

        let target_x = (anchor_x + rel_x + offset_x) as usize;
        let target_z = (anchor_z + rel_z + offset_z) as usize;

        if let Some(room) = state.level.rooms.get_mut(room_idx) {
            // Ensure sector exists
            room.ensure_sector(target_x, target_z);

            if let Some(sector) = room.get_sector_mut(target_x, target_z) {
                match &face.face {
                    CopiedFaceData::Floor(f) => {
                        let mut new_face = f.clone();
                        // Apply rotation to heights first
                        new_face.heights = rotate_heights(new_face.heights, gc.rotation);
                        if let Some(h2) = new_face.heights_2 {
                            new_face.heights_2 = Some(rotate_heights(h2, gc.rotation));
                        }
                        // Then apply flips to already-rotated heights
                        if gc.flip_h {
                            new_face.heights = [new_face.heights[1], new_face.heights[0], new_face.heights[3], new_face.heights[2]];
                            if let Some(h2) = &mut new_face.heights_2 {
                                *h2 = [h2[1], h2[0], h2[3], h2[2]];
                            }
                        }
                        if gc.flip_v {
                            new_face.heights = [new_face.heights[3], new_face.heights[2], new_face.heights[1], new_face.heights[0]];
                            if let Some(h2) = &mut new_face.heights_2 {
                                *h2 = [h2[3], h2[2], h2[1], h2[0]];
                            }
                        }
                        // Apply rotation to colors
                        new_face.colors = rotate_colors(new_face.colors, gc.rotation);
                        if let Some(c2) = new_face.colors_2 {
                            new_face.colors_2 = Some(rotate_colors(c2, gc.rotation));
                        }
                        // Apply flips to colors
                        if gc.flip_h {
                            new_face.colors = [new_face.colors[1], new_face.colors[0], new_face.colors[3], new_face.colors[2]];
                            if let Some(c2) = &mut new_face.colors_2 {
                                *c2 = [c2[1], c2[0], c2[3], c2[2]];
                            }
                        }
                        if gc.flip_v {
                            new_face.colors = [new_face.colors[3], new_face.colors[2], new_face.colors[1], new_face.colors[0]];
                            if let Some(c2) = &mut new_face.colors_2 {
                                *c2 = [c2[3], c2[2], c2[1], c2[0]];
                            }
                        }
                        // Flip diagonal split direction when needed
                        if should_flip_split {
                            new_face.split_direction = new_face.split_direction.next();
                            // Also swap triangle 1 and 2 properties since they switch positions
                            let tex1 = new_face.texture.clone();
                            let tex2 = new_face.texture_2.take().unwrap_or_else(|| tex1.clone());
                            new_face.texture = tex2;
                            new_face.texture_2 = Some(tex1);

                            std::mem::swap(&mut new_face.uv, &mut new_face.uv_2);

                            let colors1 = new_face.colors;
                            let colors2 = new_face.colors_2.take().unwrap_or(colors1);
                            new_face.colors = colors2;
                            new_face.colors_2 = Some(colors1);

                            let heights1 = new_face.heights;
                            let heights2 = new_face.heights_2.take().unwrap_or(heights1);
                            new_face.heights = heights2;
                            new_face.heights_2 = Some(heights1);
                        }
                        sector.floor = Some(new_face);
                        paste_count += 1;
                    }
                    CopiedFaceData::Ceiling(f) => {
                        let mut new_face = f.clone();
                        // Apply rotation to heights first
                        new_face.heights = rotate_heights(new_face.heights, gc.rotation);
                        if let Some(h2) = new_face.heights_2 {
                            new_face.heights_2 = Some(rotate_heights(h2, gc.rotation));
                        }
                        // Then apply flips to already-rotated heights
                        if gc.flip_h {
                            new_face.heights = [new_face.heights[1], new_face.heights[0], new_face.heights[3], new_face.heights[2]];
                            if let Some(h2) = &mut new_face.heights_2 {
                                *h2 = [h2[1], h2[0], h2[3], h2[2]];
                            }
                        }
                        if gc.flip_v {
                            new_face.heights = [new_face.heights[3], new_face.heights[2], new_face.heights[1], new_face.heights[0]];
                            if let Some(h2) = &mut new_face.heights_2 {
                                *h2 = [h2[3], h2[2], h2[1], h2[0]];
                            }
                        }
                        // Apply rotation to colors
                        new_face.colors = rotate_colors(new_face.colors, gc.rotation);
                        if let Some(c2) = new_face.colors_2 {
                            new_face.colors_2 = Some(rotate_colors(c2, gc.rotation));
                        }
                        // Apply flips to colors
                        if gc.flip_h {
                            new_face.colors = [new_face.colors[1], new_face.colors[0], new_face.colors[3], new_face.colors[2]];
                            if let Some(c2) = &mut new_face.colors_2 {
                                *c2 = [c2[1], c2[0], c2[3], c2[2]];
                            }
                        }
                        if gc.flip_v {
                            new_face.colors = [new_face.colors[3], new_face.colors[2], new_face.colors[1], new_face.colors[0]];
                            if let Some(c2) = &mut new_face.colors_2 {
                                *c2 = [c2[3], c2[2], c2[1], c2[0]];
                            }
                        }
                        // Flip diagonal split direction when needed
                        if should_flip_split {
                            new_face.split_direction = new_face.split_direction.next();
                            // Also swap triangle 1 and 2 properties since they switch positions
                            let tex1 = new_face.texture.clone();
                            let tex2 = new_face.texture_2.take().unwrap_or_else(|| tex1.clone());
                            new_face.texture = tex2;
                            new_face.texture_2 = Some(tex1);

                            std::mem::swap(&mut new_face.uv, &mut new_face.uv_2);

                            let colors1 = new_face.colors;
                            let colors2 = new_face.colors_2.take().unwrap_or(colors1);
                            new_face.colors = colors2;
                            new_face.colors_2 = Some(colors1);

                            let heights1 = new_face.heights;
                            let heights2 = new_face.heights_2.take().unwrap_or(heights1);
                            new_face.heights = heights2;
                            new_face.heights_2 = Some(heights1);
                        }
                        sector.ceiling = Some(new_face);
                        paste_count += 1;
                    }
                    CopiedFaceData::WallNorth(i, w) => {
                        let target_dir = transform_wall_direction(WallDir::North, gc.rotation, gc.flip_h, gc.flip_v);
                        let target_wall = match target_dir {
                            WallDir::North => &mut sector.walls_north,
                            WallDir::East => &mut sector.walls_east,
                            WallDir::South => &mut sector.walls_south,
                            WallDir::West => &mut sector.walls_west,
                            _ => &mut sector.walls_north,
                        };
                        if *i < target_wall.len() { target_wall[*i] = w.clone(); }
                        else { target_wall.push(w.clone()); }
                        paste_count += 1;
                    }
                    CopiedFaceData::WallSouth(i, w) => {
                        let target_dir = transform_wall_direction(WallDir::South, gc.rotation, gc.flip_h, gc.flip_v);
                        let target_wall = match target_dir {
                            WallDir::North => &mut sector.walls_north,
                            WallDir::East => &mut sector.walls_east,
                            WallDir::South => &mut sector.walls_south,
                            WallDir::West => &mut sector.walls_west,
                            _ => &mut sector.walls_south,
                        };
                        if *i < target_wall.len() { target_wall[*i] = w.clone(); }
                        else { target_wall.push(w.clone()); }
                        paste_count += 1;
                    }
                    CopiedFaceData::WallEast(i, w) => {
                        let target_dir = transform_wall_direction(WallDir::East, gc.rotation, gc.flip_h, gc.flip_v);
                        let target_wall = match target_dir {
                            WallDir::North => &mut sector.walls_north,
                            WallDir::East => &mut sector.walls_east,
                            WallDir::South => &mut sector.walls_south,
                            WallDir::West => &mut sector.walls_west,
                            _ => &mut sector.walls_east,
                        };
                        if *i < target_wall.len() { target_wall[*i] = w.clone(); }
                        else { target_wall.push(w.clone()); }
                        paste_count += 1;
                    }
                    CopiedFaceData::WallWest(i, w) => {
                        let target_dir = transform_wall_direction(WallDir::West, gc.rotation, gc.flip_h, gc.flip_v);
                        let target_wall = match target_dir {
                            WallDir::North => &mut sector.walls_north,
                            WallDir::East => &mut sector.walls_east,
                            WallDir::South => &mut sector.walls_south,
                            WallDir::West => &mut sector.walls_west,
                            _ => &mut sector.walls_west,
                        };
                        if *i < target_wall.len() { target_wall[*i] = w.clone(); }
                        else { target_wall.push(w.clone()); }
                        paste_count += 1;
                    }
                    CopiedFaceData::WallNwSe(i, w) => {
                        let target_dir = transform_wall_direction(WallDir::NwSe, gc.rotation, gc.flip_h, gc.flip_v);
                        let target_wall = if target_dir == WallDir::NeSw {
                            &mut sector.walls_nesw
                        } else {
                            &mut sector.walls_nwse
                        };
                        if *i < target_wall.len() { target_wall[*i] = w.clone(); }
                        else { target_wall.push(w.clone()); }
                        paste_count += 1;
                    }
                    CopiedFaceData::WallNeSw(i, w) => {
                        let target_dir = transform_wall_direction(WallDir::NeSw, gc.rotation, gc.flip_h, gc.flip_v);
                        let target_wall = if target_dir == WallDir::NwSe {
                            &mut sector.walls_nwse
                        } else {
                            &mut sector.walls_nesw
                        };
                        if *i < target_wall.len() { target_wall[*i] = w.clone(); }
                        else { target_wall.push(w.clone()); }
                        paste_count += 1;
                    }
                }
            }
        }
    }

    // Recalculate room bounds
    if let Some(room) = state.level.rooms.get_mut(room_idx) {
        room.recalculate_bounds();
    }

    if paste_count > 0 {
        state.set_status(&format!("Pasted {} faces", paste_count), 2.0);
    } else {
        state.set_status("No faces pasted (out of bounds?)", 2.0);
    }
}

/// Draw the skybox configuration panel - PS1 Spyro-style with collapsible sections
fn draw_skybox_panel(ctx: &mut UiContext, rect: Rect, state: &mut EditorState) {
    use crate::world::{Skybox, HorizonDirection, CloudLayer, MountainRange};

    let x = rect.x.floor();
    let mut y = rect.y.floor();
    let label_gray = Color::from_rgba(150, 150, 150, 255);
    let panel_w = rect.w;

    // Enable/disable toggle
    let has_skybox = state.level.skybox.is_some();
    let toggle_rect = Rect::new(x, y, 50.0, 16.0);
    let toggle_hovered = toggle_rect.contains(ctx.mouse.x, ctx.mouse.y);

    let (bg_color, text) = if has_skybox {
        (Color::from_rgba(60, 120, 80, 255), "ON")
    } else {
        (Color::from_rgba(60, 60, 65, 255), "OFF")
    };
    draw_rectangle(toggle_rect.x, toggle_rect.y, toggle_rect.w, toggle_rect.h, bg_color);
    if toggle_hovered {
        draw_rectangle_lines(toggle_rect.x, toggle_rect.y, toggle_rect.w, toggle_rect.h, 1.0, WHITE);
    }
    draw_text(text, toggle_rect.x + 16.0, toggle_rect.y + 12.0, 11.0, WHITE);

    if toggle_hovered && ctx.mouse.left_pressed {
        if has_skybox {
            state.level.skybox = None;
        } else {
            state.level.skybox = Some(Skybox::default());
        }
    }

    // Draw gradient preview strip
    if let Some(skybox) = &state.level.skybox {
        let preview_x = x + 58.0;
        let preview_w = panel_w - 66.0;
        let preview_h = 16.0;

        // Draw vertical gradient preview
        for py in 0..preview_h as usize {
            let phi = (py as f32 / (preview_h - 1.0)) * std::f32::consts::PI;
            let color = skybox.sample_at_direction(0.0, phi, 0.0);
            draw_line(
                preview_x,
                y + py as f32,
                preview_x + preview_w,
                y + py as f32,
                1.0,
                Color::from_rgba(color.r, color.g, color.b, 255),
            );
        }
        draw_rectangle_lines(preview_x, y, preview_w, preview_h, 1.0, Color::from_rgba(80, 80, 90, 255));

        // Draw horizon marker
        let horizon_y = y + skybox.horizon * preview_h;
        draw_line(preview_x - 3.0, horizon_y, preview_x + preview_w + 3.0, horizon_y, 1.0, WHITE);
    }

    y += 22.0;

    // === SKYBOX CONTROLS ===
    if let Some(skybox) = state.level.skybox.clone() {
        // Helper to draw a collapsible section header
        let draw_section = |y: &mut f32, label: &str, expanded: &mut bool, ctx: &UiContext| -> bool {
            let header_rect = Rect::new(x, *y, panel_w - 8.0, 16.0);
            let hovered = header_rect.contains(ctx.mouse.x, ctx.mouse.y);

            // Draw background
            let bg = if hovered { Color::from_rgba(60, 60, 70, 255) } else { Color::from_rgba(45, 45, 55, 255) };
            draw_rectangle(header_rect.x, header_rect.y, header_rect.w, header_rect.h, bg);

            // Draw arrow indicator
            let arrow = if *expanded { "v" } else { ">" };
            draw_text(arrow, x + 4.0, *y + 12.0, 12.0, Color::from_rgba(180, 180, 180, 255));
            draw_text(label, x + 16.0, *y + 12.0, 11.0, WHITE);

            *y += 20.0;

            // Return if clicked
            hovered && ctx.mouse.left_pressed
        };

        // === GRADIENT SECTION ===
        if draw_section(&mut y, "Gradient", &mut state.skybox_gradient_expanded, ctx) {
            state.skybox_gradient_expanded = !state.skybox_gradient_expanded;
        }

        if state.skybox_gradient_expanded {
            // Horizon slider
            draw_text("Horizon", x + 4.0, y + 10.0, 10.0, label_gray);
            let horizon_slider = Rect::new(x + 50.0, y, panel_w - 58.0, 12.0);
            if let Some(new_val) = draw_slider(ctx, horizon_slider, skybox.horizon, 0.1, 0.9,
                Color::from_rgba(100, 140, 180, 255), &mut state.skybox_active_slider, 100) {
                state.level.skybox.as_mut().unwrap().horizon = new_val;
            }
            y += 16.0;

            // 4 gradient color swatches: Z, HS, HG, N
            draw_text("Colors", x + 4.0, y + 10.0, 10.0, label_gray);
            let swatch_labels = ["Z", "HS", "HG", "N"];
            let gradient_colors = [skybox.zenith_color, skybox.horizon_sky_color,
                                   skybox.horizon_ground_color, skybox.nadir_color];

            for (i, (label, color)) in swatch_labels.iter().zip(gradient_colors.iter()).enumerate() {
                let sx = x + 50.0 + i as f32 * 36.0;
                let swatch_rect = Rect::new(sx, y, 14.0, 14.0);

                draw_rectangle(swatch_rect.x, swatch_rect.y, swatch_rect.w, swatch_rect.h,
                    Color::from_rgba(color.r, color.g, color.b, 255));
                draw_text(label, sx + 16.0, y + 10.0, 9.0, label_gray);

                let is_selected = state.skybox_selected_color == Some(i);
                if is_selected {
                    draw_rectangle_lines(swatch_rect.x - 1.0, swatch_rect.y - 1.0,
                        swatch_rect.w + 2.0, swatch_rect.h + 2.0, 2.0, WHITE);
                } else if swatch_rect.contains(ctx.mouse.x, ctx.mouse.y) {
                    draw_rectangle_lines(swatch_rect.x, swatch_rect.y, swatch_rect.w, swatch_rect.h,
                        1.0, Color::from_rgba(200, 200, 200, 255));
                }

                if swatch_rect.contains(ctx.mouse.x, ctx.mouse.y) && ctx.mouse.left_pressed {
                    state.skybox_selected_color = Some(i);
                }
            }
            y += 18.0;

            // RGB sliders for selected gradient color
            if let Some(idx) = state.skybox_selected_color {
                if idx < 4 {
                    let color = gradient_colors[idx];
                    if let Some(new_color) = draw_compact_rgb_sliders(ctx, x + 4.0, y, panel_w - 12.0,
                        color, &mut state.skybox_active_slider) {
                        let sb = state.level.skybox.as_mut().unwrap();
                        match idx {
                            0 => sb.zenith_color = new_color,
                            1 => sb.horizon_sky_color = new_color,
                            2 => sb.horizon_ground_color = new_color,
                            3 => sb.nadir_color = new_color,
                            _ => {}
                        }
                    }
                    y += 18.0;
                }
            }

            // Horizontal tint row
            let tint_toggle = Rect::new(x + 4.0, y, 28.0, 14.0);
            let tint_hovered = tint_toggle.contains(ctx.mouse.x, ctx.mouse.y);
            let (tint_bg, tint_text) = if skybox.horizontal_tint_enabled {
                (Color::from_rgba(60, 120, 80, 255), "ON")
            } else {
                (Color::from_rgba(60, 60, 65, 255), "OFF")
            };
            draw_rectangle(tint_toggle.x, tint_toggle.y, tint_toggle.w, tint_toggle.h, tint_bg);
            if tint_hovered {
                draw_rectangle_lines(tint_toggle.x, tint_toggle.y, tint_toggle.w, tint_toggle.h, 1.0, WHITE);
            }
            draw_text(tint_text, tint_toggle.x + 4.0, tint_toggle.y + 10.0, 9.0, WHITE);
            if tint_hovered && ctx.mouse.left_pressed {
                state.level.skybox.as_mut().unwrap().horizontal_tint_enabled = !skybox.horizontal_tint_enabled;
            }

            draw_text("Tint", x + 36.0, y + 10.0, 10.0, label_gray);

            // Direction dropdown (simple cycle through E/N/W/S)
            let dir_rect = Rect::new(x + 60.0, y, 20.0, 14.0);
            let dir_hovered = dir_rect.contains(ctx.mouse.x, ctx.mouse.y);
            let dir_label = match skybox.horizontal_tint_direction {
                HorizonDirection::East => "E",
                HorizonDirection::North => "N",
                HorizonDirection::West => "W",
                HorizonDirection::South => "S",
            };
            draw_rectangle(dir_rect.x, dir_rect.y, dir_rect.w, dir_rect.h, Color::from_rgba(50, 50, 60, 255));
            if dir_hovered {
                draw_rectangle_lines(dir_rect.x, dir_rect.y, dir_rect.w, dir_rect.h, 1.0, WHITE);
            }
            draw_text(dir_label, dir_rect.x + 6.0, dir_rect.y + 10.0, 10.0, WHITE);
            if dir_hovered && ctx.mouse.left_pressed {
                let sb = state.level.skybox.as_mut().unwrap();
                sb.horizontal_tint_direction = match sb.horizontal_tint_direction {
                    HorizonDirection::East => HorizonDirection::North,
                    HorizonDirection::North => HorizonDirection::West,
                    HorizonDirection::West => HorizonDirection::South,
                    HorizonDirection::South => HorizonDirection::East,
                };
            }

            // Tint color swatch
            let tint_swatch = Rect::new(x + 84.0, y, 14.0, 14.0);
            draw_rectangle(tint_swatch.x, tint_swatch.y, tint_swatch.w, tint_swatch.h,
                Color::from_rgba(skybox.horizontal_tint_color.r, skybox.horizontal_tint_color.g,
                    skybox.horizontal_tint_color.b, 255));
            let tint_selected = state.skybox_selected_color == Some(10);
            if tint_selected {
                draw_rectangle_lines(tint_swatch.x - 1.0, tint_swatch.y - 1.0,
                    tint_swatch.w + 2.0, tint_swatch.h + 2.0, 2.0, WHITE);
            } else if tint_swatch.contains(ctx.mouse.x, ctx.mouse.y) {
                draw_rectangle_lines(tint_swatch.x, tint_swatch.y, tint_swatch.w, tint_swatch.h,
                    1.0, Color::from_rgba(200, 200, 200, 255));
            }
            if tint_swatch.contains(ctx.mouse.x, ctx.mouse.y) && ctx.mouse.left_pressed {
                state.skybox_selected_color = Some(10);
            }

            // Intensity slider
            let int_slider = Rect::new(x + 102.0, y, panel_w - 110.0, 12.0);
            if let Some(new_val) = draw_slider(ctx, int_slider, skybox.horizontal_tint_intensity, 0.0, 1.0,
                Color::from_rgba(180, 140, 100, 255), &mut state.skybox_active_slider, 101) {
                state.level.skybox.as_mut().unwrap().horizontal_tint_intensity = new_val;
            }
            y += 16.0;

            // RGB sliders for tint color if selected
            if state.skybox_selected_color == Some(10) {
                if let Some(new_color) = draw_compact_rgb_sliders(ctx, x + 4.0, y, panel_w - 12.0,
                    skybox.horizontal_tint_color, &mut state.skybox_active_slider) {
                    state.level.skybox.as_mut().unwrap().horizontal_tint_color = new_color;
                }
                y += 18.0;
            }

            y += 4.0;
        }

        // === CELESTIAL SECTION ===
        if draw_section(&mut y, "Celestial", &mut state.skybox_celestial_expanded, ctx) {
            state.skybox_celestial_expanded = !state.skybox_celestial_expanded;
        }

        if state.skybox_celestial_expanded {
            // Sun controls
            let sun_toggle = Rect::new(x + 4.0, y, 28.0, 14.0);
            let sun_hovered = sun_toggle.contains(ctx.mouse.x, ctx.mouse.y);
            let (sun_bg, sun_text) = if skybox.sun.enabled {
                (Color::from_rgba(60, 120, 80, 255), "ON")
            } else {
                (Color::from_rgba(60, 60, 65, 255), "OFF")
            };
            draw_rectangle(sun_toggle.x, sun_toggle.y, sun_toggle.w, sun_toggle.h, sun_bg);
            if sun_hovered {
                draw_rectangle_lines(sun_toggle.x, sun_toggle.y, sun_toggle.w, sun_toggle.h, 1.0, WHITE);
            }
            draw_text(sun_text, sun_toggle.x + 4.0, sun_toggle.y + 10.0, 9.0, WHITE);
            if sun_hovered && ctx.mouse.left_pressed {
                state.level.skybox.as_mut().unwrap().sun.enabled = !skybox.sun.enabled;
            }

            draw_text("Sun", x + 36.0, y + 10.0, 10.0, label_gray);

            // Sun core color
            let sun_core_swatch = Rect::new(x + 56.0, y, 14.0, 14.0);
            draw_rectangle(sun_core_swatch.x, sun_core_swatch.y, sun_core_swatch.w, sun_core_swatch.h,
                Color::from_rgba(skybox.sun.color.r, skybox.sun.color.g, skybox.sun.color.b, 255));
            if state.skybox_selected_color == Some(20) {
                draw_rectangle_lines(sun_core_swatch.x - 1.0, sun_core_swatch.y - 1.0,
                    sun_core_swatch.w + 2.0, sun_core_swatch.h + 2.0, 2.0, WHITE);
            } else if sun_core_swatch.contains(ctx.mouse.x, ctx.mouse.y) {
                draw_rectangle_lines(sun_core_swatch.x, sun_core_swatch.y, sun_core_swatch.w, sun_core_swatch.h,
                    1.0, Color::from_rgba(200, 200, 200, 255));
            }
            if sun_core_swatch.contains(ctx.mouse.x, ctx.mouse.y) && ctx.mouse.left_pressed {
                state.skybox_selected_color = Some(20);
            }

            // Sun glow color
            let sun_glow_swatch = Rect::new(x + 74.0, y, 14.0, 14.0);
            draw_rectangle(sun_glow_swatch.x, sun_glow_swatch.y, sun_glow_swatch.w, sun_glow_swatch.h,
                Color::from_rgba(skybox.sun.glow_color.r, skybox.sun.glow_color.g, skybox.sun.glow_color.b, 255));
            if state.skybox_selected_color == Some(21) {
                draw_rectangle_lines(sun_glow_swatch.x - 1.0, sun_glow_swatch.y - 1.0,
                    sun_glow_swatch.w + 2.0, sun_glow_swatch.h + 2.0, 2.0, WHITE);
            } else if sun_glow_swatch.contains(ctx.mouse.x, ctx.mouse.y) {
                draw_rectangle_lines(sun_glow_swatch.x, sun_glow_swatch.y, sun_glow_swatch.w, sun_glow_swatch.h,
                    1.0, Color::from_rgba(200, 200, 200, 255));
            }
            if sun_glow_swatch.contains(ctx.mouse.x, ctx.mouse.y) && ctx.mouse.left_pressed {
                state.skybox_selected_color = Some(21);
            }

            // Size slider
            let size_slider = Rect::new(x + 92.0, y, panel_w - 100.0, 12.0);
            if let Some(new_val) = draw_slider(ctx, size_slider, skybox.sun.size, 0.02, 0.3,
                Color::from_rgba(200, 180, 100, 255), &mut state.skybox_active_slider, 102) {
                state.level.skybox.as_mut().unwrap().sun.size = new_val;
            }
            y += 16.0;

            // Sun azimuth/elevation sliders
            draw_text("Az", x + 4.0, y + 10.0, 10.0, label_gray);
            let az_slider = Rect::new(x + 20.0, y, 70.0, 12.0);
            if let Some(new_val) = draw_slider(ctx, az_slider, skybox.sun.azimuth / (2.0 * std::f32::consts::PI), 0.0, 1.0,
                Color::from_rgba(120, 120, 180, 255), &mut state.skybox_active_slider, 103) {
                state.level.skybox.as_mut().unwrap().sun.azimuth = new_val * 2.0 * std::f32::consts::PI;
            }

            draw_text("El", x + 96.0, y + 10.0, 10.0, label_gray);
            let el_slider = Rect::new(x + 112.0, y, panel_w - 120.0, 12.0);
            if let Some(new_val) = draw_slider(ctx, el_slider, skybox.sun.elevation / (std::f32::consts::PI / 2.0), 0.0, 1.0,
                Color::from_rgba(120, 180, 120, 255), &mut state.skybox_active_slider, 104) {
                state.level.skybox.as_mut().unwrap().sun.elevation = new_val * std::f32::consts::PI / 2.0;
            }
            y += 16.0;

            // RGB sliders for selected sun color
            if state.skybox_selected_color == Some(20) {
                if let Some(new_color) = draw_compact_rgb_sliders(ctx, x + 4.0, y, panel_w - 12.0,
                    skybox.sun.color, &mut state.skybox_active_slider) {
                    state.level.skybox.as_mut().unwrap().sun.color = new_color;
                }
                y += 18.0;
            } else if state.skybox_selected_color == Some(21) {
                if let Some(new_color) = draw_compact_rgb_sliders(ctx, x + 4.0, y, panel_w - 12.0,
                    skybox.sun.glow_color, &mut state.skybox_active_slider) {
                    state.level.skybox.as_mut().unwrap().sun.glow_color = new_color;
                }
                y += 18.0;
            }

            // Moon controls (similar to sun)
            let moon_toggle = Rect::new(x + 4.0, y, 28.0, 14.0);
            let moon_hovered = moon_toggle.contains(ctx.mouse.x, ctx.mouse.y);
            let (moon_bg, moon_text) = if skybox.moon.enabled {
                (Color::from_rgba(60, 120, 80, 255), "ON")
            } else {
                (Color::from_rgba(60, 60, 65, 255), "OFF")
            };
            draw_rectangle(moon_toggle.x, moon_toggle.y, moon_toggle.w, moon_toggle.h, moon_bg);
            if moon_hovered {
                draw_rectangle_lines(moon_toggle.x, moon_toggle.y, moon_toggle.w, moon_toggle.h, 1.0, WHITE);
            }
            draw_text(moon_text, moon_toggle.x + 4.0, moon_toggle.y + 10.0, 9.0, WHITE);
            if moon_hovered && ctx.mouse.left_pressed {
                state.level.skybox.as_mut().unwrap().moon.enabled = !skybox.moon.enabled;
            }

            draw_text("Moon", x + 36.0, y + 10.0, 10.0, label_gray);
            y += 18.0;

            y += 4.0;
        }

        // === CLOUDS SECTION ===
        if draw_section(&mut y, "Clouds", &mut state.skybox_clouds_expanded, ctx) {
            state.skybox_clouds_expanded = !state.skybox_clouds_expanded;
        }

        if state.skybox_clouds_expanded {
            // Layer tabs
            draw_text("Layer", x + 4.0, y + 10.0, 10.0, label_gray);
            for i in 0..2 {
                let tab_rect = Rect::new(x + 40.0 + i as f32 * 24.0, y, 20.0, 14.0);
                let tab_hovered = tab_rect.contains(ctx.mouse.x, ctx.mouse.y);
                let is_active = state.skybox_selected_cloud_layer == i;
                let has_layer = skybox.cloud_layers[i].is_some();

                let tab_bg = if is_active {
                    Color::from_rgba(80, 80, 100, 255)
                } else if has_layer {
                    Color::from_rgba(50, 60, 70, 255)
                } else {
                    Color::from_rgba(40, 40, 50, 255)
                };
                draw_rectangle(tab_rect.x, tab_rect.y, tab_rect.w, tab_rect.h, tab_bg);
                if tab_hovered {
                    draw_rectangle_lines(tab_rect.x, tab_rect.y, tab_rect.w, tab_rect.h, 1.0, WHITE);
                }
                draw_text(&format!("{}", i + 1), tab_rect.x + 7.0, tab_rect.y + 10.0, 10.0, WHITE);

                if tab_hovered && ctx.mouse.left_pressed {
                    state.skybox_selected_cloud_layer = i;
                }
            }

            // Enable toggle for current layer
            let layer_idx = state.skybox_selected_cloud_layer;
            let layer_enabled = skybox.cloud_layers[layer_idx].is_some();

            let enable_toggle = Rect::new(x + 92.0, y, 28.0, 14.0);
            let enable_hovered = enable_toggle.contains(ctx.mouse.x, ctx.mouse.y);
            let (en_bg, en_text) = if layer_enabled {
                (Color::from_rgba(60, 120, 80, 255), "ON")
            } else {
                (Color::from_rgba(60, 60, 65, 255), "OFF")
            };
            draw_rectangle(enable_toggle.x, enable_toggle.y, enable_toggle.w, enable_toggle.h, en_bg);
            if enable_hovered {
                draw_rectangle_lines(enable_toggle.x, enable_toggle.y, enable_toggle.w, enable_toggle.h, 1.0, WHITE);
            }
            draw_text(en_text, enable_toggle.x + 4.0, enable_toggle.y + 10.0, 9.0, WHITE);
            if enable_hovered && ctx.mouse.left_pressed {
                let sb = state.level.skybox.as_mut().unwrap();
                if layer_enabled {
                    sb.cloud_layers[layer_idx] = None;
                } else {
                    sb.cloud_layers[layer_idx] = Some(CloudLayer::default());
                }
            }
            y += 18.0;

            // Layer controls if enabled
            if let Some(layer) = &skybox.cloud_layers[layer_idx] {
                // Height and thickness
                draw_text("Ht", x + 4.0, y + 10.0, 10.0, label_gray);
                let ht_slider = Rect::new(x + 20.0, y, 60.0, 12.0);
                if let Some(new_val) = draw_slider(ctx, ht_slider, layer.height, 0.0, 1.0,
                    Color::from_rgba(140, 140, 180, 255), &mut state.skybox_active_slider, 200 + layer_idx * 10) {
                    state.level.skybox.as_mut().unwrap().cloud_layers[layer_idx].as_mut().unwrap().height = new_val;
                }

                draw_text("Th", x + 86.0, y + 10.0, 10.0, label_gray);
                let th_slider = Rect::new(x + 102.0, y, panel_w - 110.0, 12.0);
                if let Some(new_val) = draw_slider(ctx, th_slider, layer.thickness, 0.01, 0.2,
                    Color::from_rgba(140, 180, 140, 255), &mut state.skybox_active_slider, 201 + layer_idx * 10) {
                    state.level.skybox.as_mut().unwrap().cloud_layers[layer_idx].as_mut().unwrap().thickness = new_val;
                }
                y += 16.0;

                // Color swatch + opacity
                let cloud_swatch = Rect::new(x + 4.0, y, 14.0, 14.0);
                draw_rectangle(cloud_swatch.x, cloud_swatch.y, cloud_swatch.w, cloud_swatch.h,
                    Color::from_rgba(layer.color.r, layer.color.g, layer.color.b, 255));
                let cloud_selected = state.skybox_selected_color == Some(30 + layer_idx);
                if cloud_selected {
                    draw_rectangle_lines(cloud_swatch.x - 1.0, cloud_swatch.y - 1.0,
                        cloud_swatch.w + 2.0, cloud_swatch.h + 2.0, 2.0, WHITE);
                } else if cloud_swatch.contains(ctx.mouse.x, ctx.mouse.y) {
                    draw_rectangle_lines(cloud_swatch.x, cloud_swatch.y, cloud_swatch.w, cloud_swatch.h,
                        1.0, Color::from_rgba(200, 200, 200, 255));
                }
                if cloud_swatch.contains(ctx.mouse.x, ctx.mouse.y) && ctx.mouse.left_pressed {
                    state.skybox_selected_color = Some(30 + layer_idx);
                }

                draw_text("Op", x + 22.0, y + 10.0, 10.0, label_gray);
                let op_slider = Rect::new(x + 38.0, y, 50.0, 12.0);
                if let Some(new_val) = draw_slider(ctx, op_slider, layer.opacity, 0.0, 1.0,
                    Color::from_rgba(160, 160, 180, 255), &mut state.skybox_active_slider, 202 + layer_idx * 10) {
                    state.level.skybox.as_mut().unwrap().cloud_layers[layer_idx].as_mut().unwrap().opacity = new_val;
                }

                draw_text("Spd", x + 94.0, y + 10.0, 10.0, label_gray);
                let spd_slider = Rect::new(x + 116.0, y, panel_w - 124.0, 12.0);
                if let Some(new_val) = draw_slider(ctx, spd_slider, (layer.scroll_speed + 0.1) / 0.2, 0.0, 1.0,
                    Color::from_rgba(120, 100, 160, 255), &mut state.skybox_active_slider, 203 + layer_idx * 10) {
                    state.level.skybox.as_mut().unwrap().cloud_layers[layer_idx].as_mut().unwrap().scroll_speed = new_val * 0.2 - 0.1;
                }
                y += 16.0;

                // Wispiness and density
                draw_text("Wispy", x + 4.0, y + 10.0, 10.0, label_gray);
                let wispy_slider = Rect::new(x + 38.0, y, 50.0, 12.0);
                if let Some(new_val) = draw_slider(ctx, wispy_slider, layer.wispiness, 0.0, 1.0,
                    Color::from_rgba(180, 160, 140, 255), &mut state.skybox_active_slider, 204 + layer_idx * 10) {
                    state.level.skybox.as_mut().unwrap().cloud_layers[layer_idx].as_mut().unwrap().wispiness = new_val;
                }

                draw_text("Dens", x + 94.0, y + 10.0, 10.0, label_gray);
                let dens_slider = Rect::new(x + 124.0, y, panel_w - 132.0, 12.0);
                if let Some(new_val) = draw_slider(ctx, dens_slider, layer.density / 2.0, 0.0, 1.0,
                    Color::from_rgba(140, 140, 180, 255), &mut state.skybox_active_slider, 205 + layer_idx * 10) {
                    state.level.skybox.as_mut().unwrap().cloud_layers[layer_idx].as_mut().unwrap().density = new_val * 2.0;
                }
                y += 16.0;

                // RGB sliders for cloud color if selected
                if cloud_selected {
                    if let Some(new_color) = draw_compact_rgb_sliders(ctx, x + 4.0, y, panel_w - 12.0,
                        layer.color, &mut state.skybox_active_slider) {
                        state.level.skybox.as_mut().unwrap().cloud_layers[layer_idx].as_mut().unwrap().color = new_color;
                    }
                    y += 18.0;
                }
            }

            y += 4.0;
        }

        // === MOUNTAINS SECTION ===
        if draw_section(&mut y, "Mountains", &mut state.skybox_mountains_expanded, ctx) {
            state.skybox_mountains_expanded = !state.skybox_mountains_expanded;
        }

        if state.skybox_mountains_expanded {
            // Light direction
            draw_text("Light", x + 4.0, y + 10.0, 10.0, label_gray);
            let light_dir_rect = Rect::new(x + 36.0, y, 20.0, 14.0);
            let light_hovered = light_dir_rect.contains(ctx.mouse.x, ctx.mouse.y);
            let light_label = match skybox.mountain_light_direction {
                HorizonDirection::East => "E",
                HorizonDirection::North => "N",
                HorizonDirection::West => "W",
                HorizonDirection::South => "S",
            };
            draw_rectangle(light_dir_rect.x, light_dir_rect.y, light_dir_rect.w, light_dir_rect.h, Color::from_rgba(50, 50, 60, 255));
            if light_hovered {
                draw_rectangle_lines(light_dir_rect.x, light_dir_rect.y, light_dir_rect.w, light_dir_rect.h, 1.0, WHITE);
            }
            draw_text(light_label, light_dir_rect.x + 6.0, light_dir_rect.y + 10.0, 10.0, WHITE);
            if light_hovered && ctx.mouse.left_pressed {
                let sb = state.level.skybox.as_mut().unwrap();
                sb.mountain_light_direction = match sb.mountain_light_direction {
                    HorizonDirection::East => HorizonDirection::North,
                    HorizonDirection::North => HorizonDirection::West,
                    HorizonDirection::West => HorizonDirection::South,
                    HorizonDirection::South => HorizonDirection::East,
                };
            }

            // Range tabs
            draw_text("Range", x + 64.0, y + 10.0, 10.0, label_gray);
            for i in 0..2 {
                let tab_rect = Rect::new(x + 100.0 + i as f32 * 24.0, y, 20.0, 14.0);
                let tab_hovered = tab_rect.contains(ctx.mouse.x, ctx.mouse.y);
                let is_active = state.skybox_selected_mountain_range == i;
                let has_range = skybox.mountain_ranges[i].is_some();

                let tab_bg = if is_active {
                    Color::from_rgba(80, 80, 100, 255)
                } else if has_range {
                    Color::from_rgba(50, 60, 70, 255)
                } else {
                    Color::from_rgba(40, 40, 50, 255)
                };
                draw_rectangle(tab_rect.x, tab_rect.y, tab_rect.w, tab_rect.h, tab_bg);
                if tab_hovered {
                    draw_rectangle_lines(tab_rect.x, tab_rect.y, tab_rect.w, tab_rect.h, 1.0, WHITE);
                }
                draw_text(&format!("{}", i + 1), tab_rect.x + 7.0, tab_rect.y + 10.0, 10.0, WHITE);

                if tab_hovered && ctx.mouse.left_pressed {
                    state.skybox_selected_mountain_range = i;
                }
            }
            y += 18.0;

            // Enable toggle for current range
            let range_idx = state.skybox_selected_mountain_range;
            let range_enabled = skybox.mountain_ranges[range_idx].is_some();

            let enable_toggle = Rect::new(x + 4.0, y, 28.0, 14.0);
            let enable_hovered = enable_toggle.contains(ctx.mouse.x, ctx.mouse.y);
            let (en_bg, en_text) = if range_enabled {
                (Color::from_rgba(60, 120, 80, 255), "ON")
            } else {
                (Color::from_rgba(60, 60, 65, 255), "OFF")
            };
            draw_rectangle(enable_toggle.x, enable_toggle.y, enable_toggle.w, enable_toggle.h, en_bg);
            if enable_hovered {
                draw_rectangle_lines(enable_toggle.x, enable_toggle.y, enable_toggle.w, enable_toggle.h, 1.0, WHITE);
            }
            draw_text(en_text, enable_toggle.x + 4.0, enable_toggle.y + 10.0, 9.0, WHITE);
            if enable_hovered && ctx.mouse.left_pressed {
                let sb = state.level.skybox.as_mut().unwrap();
                if range_enabled {
                    sb.mountain_ranges[range_idx] = None;
                } else {
                    sb.mountain_ranges[range_idx] = Some(MountainRange::default());
                }
            }

            // Range controls if enabled
            if let Some(range) = &skybox.mountain_ranges[range_idx] {
                // Color swatches: Lit, Shadow, Highlight
                let lit_swatch = Rect::new(x + 36.0, y, 14.0, 14.0);
                draw_rectangle(lit_swatch.x, lit_swatch.y, lit_swatch.w, lit_swatch.h,
                    Color::from_rgba(range.lit_color.r, range.lit_color.g, range.lit_color.b, 255));
                let lit_selected = state.skybox_selected_color == Some(40 + range_idx * 10);
                if lit_selected {
                    draw_rectangle_lines(lit_swatch.x - 1.0, lit_swatch.y - 1.0,
                        lit_swatch.w + 2.0, lit_swatch.h + 2.0, 2.0, WHITE);
                } else if lit_swatch.contains(ctx.mouse.x, ctx.mouse.y) {
                    draw_rectangle_lines(lit_swatch.x, lit_swatch.y, lit_swatch.w, lit_swatch.h,
                        1.0, Color::from_rgba(200, 200, 200, 255));
                }
                if lit_swatch.contains(ctx.mouse.x, ctx.mouse.y) && ctx.mouse.left_pressed {
                    state.skybox_selected_color = Some(40 + range_idx * 10);
                }

                let shd_swatch = Rect::new(x + 54.0, y, 14.0, 14.0);
                draw_rectangle(shd_swatch.x, shd_swatch.y, shd_swatch.w, shd_swatch.h,
                    Color::from_rgba(range.shadow_color.r, range.shadow_color.g, range.shadow_color.b, 255));
                let shd_selected = state.skybox_selected_color == Some(41 + range_idx * 10);
                if shd_selected {
                    draw_rectangle_lines(shd_swatch.x - 1.0, shd_swatch.y - 1.0,
                        shd_swatch.w + 2.0, shd_swatch.h + 2.0, 2.0, WHITE);
                } else if shd_swatch.contains(ctx.mouse.x, ctx.mouse.y) {
                    draw_rectangle_lines(shd_swatch.x, shd_swatch.y, shd_swatch.w, shd_swatch.h,
                        1.0, Color::from_rgba(200, 200, 200, 255));
                }
                if shd_swatch.contains(ctx.mouse.x, ctx.mouse.y) && ctx.mouse.left_pressed {
                    state.skybox_selected_color = Some(41 + range_idx * 10);
                }

                let hi_swatch = Rect::new(x + 72.0, y, 14.0, 14.0);
                draw_rectangle(hi_swatch.x, hi_swatch.y, hi_swatch.w, hi_swatch.h,
                    Color::from_rgba(range.highlight_color.r, range.highlight_color.g, range.highlight_color.b, 255));
                let hi_selected = state.skybox_selected_color == Some(42 + range_idx * 10);
                if hi_selected {
                    draw_rectangle_lines(hi_swatch.x - 1.0, hi_swatch.y - 1.0,
                        hi_swatch.w + 2.0, hi_swatch.h + 2.0, 2.0, WHITE);
                } else if hi_swatch.contains(ctx.mouse.x, ctx.mouse.y) {
                    draw_rectangle_lines(hi_swatch.x, hi_swatch.y, hi_swatch.w, hi_swatch.h,
                        1.0, Color::from_rgba(200, 200, 200, 255));
                }
                if hi_swatch.contains(ctx.mouse.x, ctx.mouse.y) && ctx.mouse.left_pressed {
                    state.skybox_selected_color = Some(42 + range_idx * 10);
                }

                // Height slider
                draw_text("Ht", x + 90.0, y + 10.0, 10.0, label_gray);
                let ht_slider = Rect::new(x + 106.0, y, panel_w - 114.0, 12.0);
                if let Some(new_val) = draw_slider(ctx, ht_slider, range.height, 0.0, 0.4,
                    Color::from_rgba(100, 80, 60, 255), &mut state.skybox_active_slider, 300 + range_idx * 10) {
                    state.level.skybox.as_mut().unwrap().mountain_ranges[range_idx].as_mut().unwrap().height = new_val;
                }
                y += 16.0;

                // Depth and jaggedness
                draw_text("Depth", x + 4.0, y + 10.0, 10.0, label_gray);
                let depth_slider = Rect::new(x + 38.0, y, 50.0, 12.0);
                if let Some(new_val) = draw_slider(ctx, depth_slider, range.depth, 0.0, 1.0,
                    Color::from_rgba(80, 80, 120, 255), &mut state.skybox_active_slider, 301 + range_idx * 10) {
                    state.level.skybox.as_mut().unwrap().mountain_ranges[range_idx].as_mut().unwrap().depth = new_val;
                }

                draw_text("Jagged", x + 94.0, y + 10.0, 10.0, label_gray);
                let jag_slider = Rect::new(x + 132.0, y, panel_w - 140.0, 12.0);
                if let Some(new_val) = draw_slider(ctx, jag_slider, range.jaggedness, 0.0, 1.0,
                    Color::from_rgba(100, 100, 80, 255), &mut state.skybox_active_slider, 302 + range_idx * 10) {
                    state.level.skybox.as_mut().unwrap().mountain_ranges[range_idx].as_mut().unwrap().jaggedness = new_val;
                }
                y += 16.0;

                // RGB sliders for selected mountain color
                if lit_selected {
                    if let Some(new_color) = draw_compact_rgb_sliders(ctx, x + 4.0, y, panel_w - 12.0,
                        range.lit_color, &mut state.skybox_active_slider) {
                        state.level.skybox.as_mut().unwrap().mountain_ranges[range_idx].as_mut().unwrap().lit_color = new_color;
                    }
                    y += 18.0;
                } else if shd_selected {
                    if let Some(new_color) = draw_compact_rgb_sliders(ctx, x + 4.0, y, panel_w - 12.0,
                        range.shadow_color, &mut state.skybox_active_slider) {
                        state.level.skybox.as_mut().unwrap().mountain_ranges[range_idx].as_mut().unwrap().shadow_color = new_color;
                    }
                    y += 18.0;
                } else if hi_selected {
                    if let Some(new_color) = draw_compact_rgb_sliders(ctx, x + 4.0, y, panel_w - 12.0,
                        range.highlight_color, &mut state.skybox_active_slider) {
                        state.level.skybox.as_mut().unwrap().mountain_ranges[range_idx].as_mut().unwrap().highlight_color = new_color;
                    }
                    y += 18.0;
                }
            }

            y += 4.0;
        }

        // === STARS SECTION ===
        if draw_section(&mut y, "Stars", &mut state.skybox_stars_expanded, ctx) {
            state.skybox_stars_expanded = !state.skybox_stars_expanded;
        }

        if state.skybox_stars_expanded {
            let stars_toggle = Rect::new(x + 4.0, y, 28.0, 14.0);
            let stars_hovered = stars_toggle.contains(ctx.mouse.x, ctx.mouse.y);
            let (stars_bg, stars_text) = if skybox.stars.enabled {
                (Color::from_rgba(60, 120, 80, 255), "ON")
            } else {
                (Color::from_rgba(60, 60, 65, 255), "OFF")
            };
            draw_rectangle(stars_toggle.x, stars_toggle.y, stars_toggle.w, stars_toggle.h, stars_bg);
            if stars_hovered {
                draw_rectangle_lines(stars_toggle.x, stars_toggle.y, stars_toggle.w, stars_toggle.h, 1.0, WHITE);
            }
            draw_text(stars_text, stars_toggle.x + 4.0, stars_toggle.y + 10.0, 9.0, WHITE);
            if stars_hovered && ctx.mouse.left_pressed {
                state.level.skybox.as_mut().unwrap().stars.enabled = !skybox.stars.enabled;
            }

            // Star color swatch
            let star_swatch = Rect::new(x + 36.0, y, 14.0, 14.0);
            draw_rectangle(star_swatch.x, star_swatch.y, star_swatch.w, star_swatch.h,
                Color::from_rgba(skybox.stars.color.r, skybox.stars.color.g, skybox.stars.color.b, 255));
            let star_selected = state.skybox_selected_color == Some(60);
            if star_selected {
                draw_rectangle_lines(star_swatch.x - 1.0, star_swatch.y - 1.0,
                    star_swatch.w + 2.0, star_swatch.h + 2.0, 2.0, WHITE);
            } else if star_swatch.contains(ctx.mouse.x, ctx.mouse.y) {
                draw_rectangle_lines(star_swatch.x, star_swatch.y, star_swatch.w, star_swatch.h,
                    1.0, Color::from_rgba(200, 200, 200, 255));
            }
            if star_swatch.contains(ctx.mouse.x, ctx.mouse.y) && ctx.mouse.left_pressed {
                state.skybox_selected_color = Some(60);
            }

            // Count slider
            draw_text("Cnt", x + 54.0, y + 10.0, 10.0, label_gray);
            let cnt_slider = Rect::new(x + 76.0, y, panel_w - 84.0, 12.0);
            if let Some(new_val) = draw_slider(ctx, cnt_slider, skybox.stars.count as f32 / 200.0, 0.0, 1.0,
                Color::from_rgba(180, 180, 200, 255), &mut state.skybox_active_slider, 400) {
                state.level.skybox.as_mut().unwrap().stars.count = (new_val * 200.0) as u16;
            }
            y += 16.0;

            // Size and twinkle
            draw_text("Size", x + 4.0, y + 10.0, 10.0, label_gray);
            let size_slider = Rect::new(x + 32.0, y, 50.0, 12.0);
            if let Some(new_val) = draw_slider(ctx, size_slider, skybox.stars.size / 4.0, 0.0, 1.0,
                Color::from_rgba(160, 160, 180, 255), &mut state.skybox_active_slider, 401) {
                state.level.skybox.as_mut().unwrap().stars.size = new_val * 4.0;
            }

            draw_text("Twinkle", x + 88.0, y + 10.0, 10.0, label_gray);
            let twinkle_slider = Rect::new(x + 132.0, y, panel_w - 140.0, 12.0);
            if let Some(new_val) = draw_slider(ctx, twinkle_slider, skybox.stars.twinkle_speed / 2.0, 0.0, 1.0,
                Color::from_rgba(140, 140, 180, 255), &mut state.skybox_active_slider, 402) {
                state.level.skybox.as_mut().unwrap().stars.twinkle_speed = new_val * 2.0;
            }
            y += 16.0;

            // RGB sliders for star color if selected
            if star_selected {
                if let Some(new_color) = draw_compact_rgb_sliders(ctx, x + 4.0, y, panel_w - 12.0,
                    skybox.stars.color, &mut state.skybox_active_slider) {
                    state.level.skybox.as_mut().unwrap().stars.color = new_color;
                }
                y += 18.0;
            }

            y += 4.0;
        }

        // === ATMOSPHERE SECTION ===
        if draw_section(&mut y, "Atmosphere", &mut state.skybox_atmo_expanded, ctx) {
            state.skybox_atmo_expanded = !state.skybox_atmo_expanded;
        }

        if state.skybox_atmo_expanded {
            let haze_toggle = Rect::new(x + 4.0, y, 28.0, 14.0);
            let haze_hovered = haze_toggle.contains(ctx.mouse.x, ctx.mouse.y);
            let (haze_bg, haze_text) = if skybox.horizon_haze.enabled {
                (Color::from_rgba(60, 120, 80, 255), "ON")
            } else {
                (Color::from_rgba(60, 60, 65, 255), "OFF")
            };
            draw_rectangle(haze_toggle.x, haze_toggle.y, haze_toggle.w, haze_toggle.h, haze_bg);
            if haze_hovered {
                draw_rectangle_lines(haze_toggle.x, haze_toggle.y, haze_toggle.w, haze_toggle.h, 1.0, WHITE);
            }
            draw_text(haze_text, haze_toggle.x + 4.0, haze_toggle.y + 10.0, 9.0, WHITE);
            if haze_hovered && ctx.mouse.left_pressed {
                state.level.skybox.as_mut().unwrap().horizon_haze.enabled = !skybox.horizon_haze.enabled;
            }

            draw_text("Haze", x + 36.0, y + 10.0, 10.0, label_gray);

            // Haze color swatch
            let haze_swatch = Rect::new(x + 64.0, y, 14.0, 14.0);
            draw_rectangle(haze_swatch.x, haze_swatch.y, haze_swatch.w, haze_swatch.h,
                Color::from_rgba(skybox.horizon_haze.color.r, skybox.horizon_haze.color.g, skybox.horizon_haze.color.b, 255));
            let haze_selected = state.skybox_selected_color == Some(70);
            if haze_selected {
                draw_rectangle_lines(haze_swatch.x - 1.0, haze_swatch.y - 1.0,
                    haze_swatch.w + 2.0, haze_swatch.h + 2.0, 2.0, WHITE);
            } else if haze_swatch.contains(ctx.mouse.x, ctx.mouse.y) {
                draw_rectangle_lines(haze_swatch.x, haze_swatch.y, haze_swatch.w, haze_swatch.h,
                    1.0, Color::from_rgba(200, 200, 200, 255));
            }
            if haze_swatch.contains(ctx.mouse.x, ctx.mouse.y) && ctx.mouse.left_pressed {
                state.skybox_selected_color = Some(70);
            }

            // Intensity slider
            let int_slider = Rect::new(x + 82.0, y, panel_w - 90.0, 12.0);
            if let Some(new_val) = draw_slider(ctx, int_slider, skybox.horizon_haze.intensity, 0.0, 1.0,
                Color::from_rgba(160, 140, 120, 255), &mut state.skybox_active_slider, 500) {
                state.level.skybox.as_mut().unwrap().horizon_haze.intensity = new_val;
            }
            y += 16.0;

            // Extent slider
            draw_text("Extent", x + 4.0, y + 10.0, 10.0, label_gray);
            let ext_slider = Rect::new(x + 44.0, y, panel_w - 52.0, 12.0);
            if let Some(new_val) = draw_slider(ctx, ext_slider, skybox.horizon_haze.extent / 0.3, 0.0, 1.0,
                Color::from_rgba(140, 140, 160, 255), &mut state.skybox_active_slider, 501) {
                state.level.skybox.as_mut().unwrap().horizon_haze.extent = new_val * 0.3;
            }
            y += 16.0;

            // RGB sliders for haze color if selected
            if haze_selected {
                if let Some(new_color) = draw_compact_rgb_sliders(ctx, x + 4.0, y, panel_w - 12.0,
                    skybox.horizon_haze.color, &mut state.skybox_active_slider) {
                    state.level.skybox.as_mut().unwrap().horizon_haze.color = new_color;
                }
                y += 18.0;
            }

            y += 4.0;
        }

        // === PRESETS ===
        y += 4.0;
        draw_text("Presets", x, y + 10.0, 10.0, label_gray);

        let preset_names = ["Sunset", "Twilight", "Night", "Arctic"];
        let preset_w = (panel_w - 8.0 - 45.0 - 3.0 * 4.0) / 4.0;

        for (i, name) in preset_names.iter().enumerate() {
            let btn_rect = Rect::new(x + 45.0 + i as f32 * (preset_w + 4.0), y, preset_w, 14.0);
            let btn_hovered = btn_rect.contains(ctx.mouse.x, ctx.mouse.y);

            let btn_bg = if btn_hovered { Color::from_rgba(70, 70, 90, 255) } else { Color::from_rgba(50, 50, 65, 255) };
            draw_rectangle(btn_rect.x, btn_rect.y, btn_rect.w, btn_rect.h, btn_bg);
            if btn_hovered {
                draw_rectangle_lines(btn_rect.x, btn_rect.y, btn_rect.w, btn_rect.h, 1.0, WHITE);
            }

            // Center text
            let text_w = name.len() as f32 * 5.0;
            draw_text(name, btn_rect.x + (btn_rect.w - text_w) / 2.0, btn_rect.y + 10.0, 9.0, WHITE);

            if btn_hovered && ctx.mouse.left_pressed {
                let sb = state.level.skybox.as_mut().unwrap();
                match i {
                    0 => *sb = Skybox::preset_sunset(),
                    1 => *sb = Skybox::preset_twilight(),
                    2 => *sb = Skybox::preset_night(),
                    3 => *sb = Skybox::preset_arctic(),
                    _ => {}
                }
            }
        }
    }
}

/// Helper to draw a slider and return new value if changed
fn draw_slider(
    ctx: &UiContext,
    rect: Rect,
    value: f32,
    min: f32,
    max: f32,
    fill_color: Color,
    active_slider: &mut Option<usize>,
    slider_id: usize,
) -> Option<f32> {
    let hovered = rect.contains(ctx.mouse.x, ctx.mouse.y);

    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(40, 40, 45, 255));
    let normalized = (value - min) / (max - min);
    let fill_w = normalized * rect.w;
    draw_rectangle(rect.x, rect.y, fill_w, rect.h, fill_color);

    if hovered {
        draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, WHITE);
    }

    if hovered && ctx.mouse.left_pressed {
        *active_slider = Some(slider_id);
    }

    if *active_slider == Some(slider_id) {
        if ctx.mouse.left_down {
            let t = ((ctx.mouse.x - rect.x) / rect.w).clamp(0.0, 1.0);
            return Some(min + t * (max - min));
        } else {
            *active_slider = None;
        }
    }

    None
}

/// Compact RGB sliders (single row with R/G/B)
fn draw_compact_rgb_sliders(
    ctx: &mut UiContext,
    x: f32,
    y: f32,
    width: f32,
    color: crate::rasterizer::Color,
    active_slider: &mut Option<usize>,
) -> Option<crate::rasterizer::Color> {
    let slider_w = (width - 6.0) / 3.0;
    let mut new_color = None;

    for (i, (val, label, c)) in [(color.r, "R", Color::from_rgba(200, 80, 80, 255)),
                                   (color.g, "G", Color::from_rgba(80, 200, 80, 255)),
                                   (color.b, "B", Color::from_rgba(80, 80, 200, 255))].iter().enumerate() {
        let sx = x + i as f32 * (slider_w + 3.0);
        let slider_rect = Rect::new(sx, y, slider_w, 14.0);
        let hovered = slider_rect.contains(ctx.mouse.x, ctx.mouse.y);

        // Background
        draw_rectangle(slider_rect.x, slider_rect.y, slider_rect.w, slider_rect.h, Color::from_rgba(40, 40, 45, 255));

        // Fill bar
        let fill_w = (*val as f32 / 255.0) * slider_rect.w;
        draw_rectangle(slider_rect.x, slider_rect.y, fill_w, slider_rect.h, *c);

        // Label
        draw_text(label, slider_rect.x + 2.0, slider_rect.y + 10.0, 10.0, WHITE);

        // Handle interaction
        if hovered && ctx.mouse.left_pressed {
            *active_slider = Some(i);
        }

        if *active_slider == Some(i) {
            if ctx.mouse.left_down {
                let t = ((ctx.mouse.x - slider_rect.x) / slider_rect.w).clamp(0.0, 1.0);
                let new_val = (t * 255.0) as u8;
                let mut c = color;
                match i {
                    0 => c.r = new_val,
                    1 => c.g = new_val,
                    2 => c.b = new_val,
                    _ => {}
                }
                new_color = Some(c);
            } else {
                *active_slider = None;
            }
        }

        if hovered {
            draw_rectangle_lines(slider_rect.x, slider_rect.y, slider_rect.w, slider_rect.h, 1.0, WHITE);
        }
    }

    new_color
}

/// Draw debug panel with frame timing information
fn draw_debug_panel(_ctx: &mut UiContext, rect: Rect, state: &mut EditorState) {
    use macroquad::prelude::*;

    let mut y = rect.y.floor();
    let x = rect.x.floor();
    let bar_w = rect.w - 4.0;

    // Colors for timing breakdown
    let label_color = Color::from_rgba(150, 150, 160, 255);
    let value_color = Color::from_rgba(200, 200, 210, 255);
    let toolbar_color = Color::from_rgba(100, 180, 255, 255);   // Blue
    let left_color = Color::from_rgba(180, 100, 255, 255);      // Purple
    let viewport_color = Color::from_rgba(255, 100, 100, 255);  // Red
    let right_color = Color::from_rgba(255, 200, 100, 255);     // Orange
    let status_color = Color::from_rgba(100, 255, 180, 255);    // Cyan

    // 3D viewport sub-timing colors (shades of red)
    let vp_input_color = Color::from_rgba(255, 180, 180, 255);   // Light red
    let vp_clear_color = Color::from_rgba(255, 150, 150, 255);   // Light-mid red
    let vp_grid_color = Color::from_rgba(255, 120, 120, 255);    // Mid red
    let vp_lights_color = Color::from_rgba(240, 100, 100, 255);  // Mid-dark red
    let vp_texconv_color = Color::from_rgba(220, 80, 80, 255);   // Dark red
    let vp_meshgen_color = Color::from_rgba(200, 60, 60, 255);   // Darker red
    let vp_raster_color = Color::from_rgba(180, 40, 40, 255);    // Darkest red
    let vp_preview_color = Color::from_rgba(255, 100, 150, 255); // Red-pink
    let vp_upload_color = Color::from_rgba(255, 130, 100, 255);  // Red-orange

    // FPS and frame time
    let fps = get_fps();
    let frame_time_ms = get_frame_time() * 1000.0;

    let fps_color = if fps >= 55 {
        Color::from_rgba(100, 255, 100, 255)
    } else if fps >= 30 {
        Color::from_rgba(255, 200, 100, 255)
    } else {
        Color::from_rgba(255, 100, 100, 255)
    };

    draw_text(&format!("FPS: {}", fps), x, y + 10.0, FONT_SIZE_CONTENT, fps_color);
    y += LINE_HEIGHT;

    draw_text(&format!("Frame: {:.2}ms", frame_time_ms), x, y + 10.0, FONT_SIZE_CONTENT, WHITE);
    y += LINE_HEIGHT;

    // Draw stacked timing bar
    let bar_y = y + 2.0;
    let bar_h = 10.0;

    // Background
    draw_rectangle(x, bar_y, bar_w, bar_h, Color::from_rgba(30, 30, 30, 255));

    // Get timing data
    let t = &state.frame_timings;
    let total = t.total_ms.max(0.001); // Avoid division by zero

    // Draw bar segments (stacked horizontally)
    let mut bar_x = x;
    let segments = [
        (t.toolbar_ms, toolbar_color),
        (t.left_panel_ms, left_color),
        (t.viewport_3d_ms, viewport_color),
        (t.right_panel_ms, right_color),
        (t.status_ms, status_color),
    ];

    for (ms, color) in segments.iter() {
        let seg_w = (*ms / total) * bar_w;
        if seg_w > 0.5 {
            draw_rectangle(bar_x, bar_y, seg_w, bar_h, *color);
            bar_x += seg_w;
        }
    }

    // Target line (16.67ms = 60fps)
    let target_ms = 16.67;
    let target_x = x + (target_ms / total.max(target_ms)) * bar_w;
    if target_x < x + bar_w {
        draw_line(target_x, bar_y - 1.0, target_x, bar_y + bar_h + 1.0, 1.0, Color::from_rgba(255, 255, 255, 150));
    }

    y += LINE_HEIGHT + 2.0;

    // Total timing text
    draw_text(&format!("Total: {:.2}ms", t.total_ms), x, y + 10.0, FONT_SIZE_CONTENT, value_color);
    y += LINE_HEIGHT;

    // Main breakdown
    y += 2.0;
    draw_text("Main:", x, y + 10.0, FONT_SIZE_CONTENT, label_color);
    y += LINE_HEIGHT;

    // Timing breakdown with colored boxes
    let box_size = 8.0;
    let items = [
        ("Toolbar", t.toolbar_ms, toolbar_color),
        ("Left", t.left_panel_ms, left_color),
        ("3D View", t.viewport_3d_ms, viewport_color),
        ("Right", t.right_panel_ms, right_color),
        ("Status", t.status_ms, status_color),
    ];

    for (name, ms, color) in items.iter() {
        draw_rectangle(x, y + 4.0, box_size, box_size, *color);
        draw_text(name, x + box_size + 4.0, y + 10.0, FONT_SIZE_CONTENT, label_color);
        let value_str = format!("{:.2}ms", ms);
        let value_w = value_str.len() as f32 * 6.0;
        draw_text(&value_str, x + bar_w - value_w, y + 10.0, FONT_SIZE_CONTENT, value_color);
        y += LINE_HEIGHT;
    }

    // 3D Viewport breakdown
    y += 4.0;
    draw_text("3D View:", x, y + 10.0, FONT_SIZE_CONTENT, label_color);
    y += LINE_HEIGHT;

    let vp_items = [
        ("Input", t.vp_input_ms, vp_input_color),
        ("Clear", t.vp_clear_ms, vp_clear_color),
        ("Grid", t.vp_grid_ms, vp_grid_color),
        ("Lights", t.vp_lights_ms, vp_lights_color),
        ("TexConv", t.vp_texconv_ms, vp_texconv_color),
        ("MeshGen", t.vp_meshgen_ms, vp_meshgen_color),
        ("Raster", t.vp_raster_ms, vp_raster_color),
        ("Preview", t.vp_preview_ms, vp_preview_color),
        ("Upload", t.vp_upload_ms, vp_upload_color),
    ];

    let indent = 8.0;
    for (name, ms, color) in vp_items.iter() {
        draw_rectangle(x + indent, y + 4.0, box_size, box_size, *color);
        draw_text(name, x + indent + box_size + 4.0, y + 10.0, FONT_SIZE_CONTENT, label_color);
        let value_str = format!("{:.2}ms", ms);
        let value_w = value_str.len() as f32 * 6.0;
        draw_text(&value_str, x + bar_w - value_w, y + 10.0, FONT_SIZE_CONTENT, value_color);
        y += LINE_HEIGHT;
    }

    // === MEMORY SECTION ===
    y += 8.0;
    draw_text("Memory:", x, y + 10.0, FONT_SIZE_CONTENT, label_color);
    y += LINE_HEIGHT;

    let m = &state.memory_stats;

    // Process memory (from OS)
    let physical_str = super::state::MemoryStats::format_bytes(m.physical_bytes);
    draw_text("Process RSS", x + indent, y + 10.0, FONT_SIZE_CONTENT, label_color);
    let val_w = physical_str.len() as f32 * 6.0;
    draw_text(&physical_str, x + bar_w - val_w, y + 10.0, FONT_SIZE_CONTENT, value_color);
    y += LINE_HEIGHT;

    // Texture memory breakdown
    y += 4.0;
    draw_text("Textures:", x + indent, y + 10.0, FONT_SIZE_CONTENT, label_color);
    y += LINE_HEIGHT;

    let tex_str = format!("{} ({})", super::state::MemoryStats::format_bytes(m.texture_bytes), m.texture_count);
    draw_text("RGB888", x + indent * 2.0, y + 10.0, FONT_SIZE_CONTENT, label_color);
    let val_w = tex_str.len() as f32 * 6.0;
    draw_text(&tex_str, x + bar_w - val_w, y + 10.0, FONT_SIZE_CONTENT, value_color);
    y += LINE_HEIGHT;

    let tex15_str = super::state::MemoryStats::format_bytes(m.texture15_bytes);
    draw_text("RGB555 cache", x + indent * 2.0, y + 10.0, FONT_SIZE_CONTENT, label_color);
    let val_w = tex15_str.len() as f32 * 6.0;
    draw_text(&tex15_str, x + bar_w - val_w, y + 10.0, FONT_SIZE_CONTENT, value_color);
    y += LINE_HEIGHT;

    let fb_str = super::state::MemoryStats::format_bytes(m.framebuffer_bytes);
    draw_text("Framebuffer", x + indent, y + 10.0, FONT_SIZE_CONTENT, label_color);
    let val_w = fb_str.len() as f32 * 6.0;
    draw_text(&fb_str, x + bar_w - val_w, y + 10.0, FONT_SIZE_CONTENT, value_color);
    y += LINE_HEIGHT;

    let gpu_str = format!("{}", m.gpu_cache_count);
    draw_text("GPU cache", x + indent, y + 10.0, FONT_SIZE_CONTENT, label_color);
    let val_w = gpu_str.len() as f32 * 6.0;
    draw_text(&gpu_str, x + bar_w - val_w, y + 10.0, FONT_SIZE_CONTENT, value_color);
    y += LINE_HEIGHT;

    // Show tracked vs untracked
    let tracked = m.texture_bytes + m.texture15_bytes + m.framebuffer_bytes;
    let untracked = m.physical_bytes.saturating_sub(tracked);
    y += 4.0;
    let tracked_str = super::state::MemoryStats::format_bytes(tracked);
    draw_text("Tracked", x + indent, y + 10.0, FONT_SIZE_CONTENT, label_color);
    let val_w = tracked_str.len() as f32 * 6.0;
    draw_text(&tracked_str, x + bar_w - val_w, y + 10.0, FONT_SIZE_CONTENT, value_color);
    y += LINE_HEIGHT;

    let untracked_str = super::state::MemoryStats::format_bytes(untracked);
    draw_text("Untracked", x + indent, y + 10.0, FONT_SIZE_CONTENT, label_color);
    let val_w = untracked_str.len() as f32 * 6.0;
    draw_text(&untracked_str, x + bar_w - val_w, y + 10.0, FONT_SIZE_CONTENT, Color::from_rgba(255, 180, 100, 255));
    let _ = y; // suppress unused warning
}

fn draw_room_properties(ctx: &mut UiContext, rect: Rect, state: &mut EditorState, icon_font: Option<&Font>) {
    let mut y = rect.y.floor();
    let x = rect.x.floor();
    let icon_btn_size = 14.0;

    // Room list at the top
    let num_rooms = state.level.rooms.len();
    let max_visible_rooms = 6; // Show up to 6 rooms before scrolling would be needed
    let rooms_to_show = num_rooms.min(max_visible_rooms);
    let mut room_to_delete: Option<usize> = None;

    for i in 0..num_rooms {
        if i >= rooms_to_show {
            // Show "... and N more" indicator
            let remaining = num_rooms - rooms_to_show;
            draw_text(&format!("... +{} more", remaining), x, (y + 10.0).floor(), FONT_SIZE_CONTENT, Color::from_rgba(100, 100, 100, 255));
            y += LINE_HEIGHT;
            break;
        }

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

        // Delete button on the right
        let del_btn_rect = Rect::new(x + rect.w - icon_btn_size - 4.0, y + 1.0, icon_btn_size, icon_btn_size);
        if crate::ui::icon_button(ctx, del_btn_rect, icon::TRASH, icon_font, "Delete room") {
            room_to_delete = Some(i);
        }

        // Room row (clickable area between visibility and delete buttons)
        let room_btn_rect = Rect::new(x + icon_btn_size + 2.0, y, rect.w - icon_btn_size * 2.0 - 10.0, LINE_HEIGHT);
        if ctx.mouse.clicked(&room_btn_rect) {
            state.current_room = i;
        }

        if is_selected {
            draw_rectangle(room_btn_rect.x.floor(), room_btn_rect.y.floor(), room_btn_rect.w, room_btn_rect.h, Color::from_rgba(60, 80, 60, 255));
        }

        let sector_count = room.iter_sectors().count();
        draw_text(&format!("Room {} ({} sectors)", room.id, sector_count), (x + icon_btn_size + 4.0).floor(), (y + 11.0).floor(), FONT_SIZE_CONTENT, text_color);
        y += LINE_HEIGHT;
    }

    // Handle room deletion after iteration
    if let Some(i) = room_to_delete {
        state.save_undo();
        state.level.rooms.remove(i);
        // Update current_room if needed
        if state.current_room >= state.level.rooms.len() && !state.level.rooms.is_empty() {
            state.current_room = state.level.rooms.len() - 1;
        }
        // Update hidden_rooms: remove this room and shift higher indices down
        state.hidden_rooms.remove(&i);
        state.hidden_rooms = state.hidden_rooms.iter()
            .filter_map(|&idx| if idx > i { Some(idx - 1) } else if idx < i { Some(idx) } else { None })
            .collect();
        // Clear selection if it was in the deleted room
        if let Selection::SectorFace { room, .. } | Selection::Object { room, .. } = &state.selection {
            if *room == i {
                state.selection = Selection::None;
            }
        }
        state.multi_selection.clear();
        state.mark_portals_dirty();
        state.set_status(&format!("Deleted Room {}", i), 2.0);
    }

    if state.level.rooms.is_empty() {
        draw_text("No rooms", x, (y + 10.0).floor(), FONT_SIZE_CONTENT, Color::from_rgba(150, 150, 150, 255));
        y += LINE_HEIGHT;
    }

    // Add Room button
    let add_btn_rect = Rect::new(x, y + 2.0, icon_btn_size, icon_btn_size);
    if crate::ui::icon_button(ctx, add_btn_rect, icon::PLUS, icon_font, "Add Room") {
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
    draw_text("Add Room", (x + icon_btn_size + 4.0).floor(), (y + 12.0).floor(), FONT_SIZE_CONTENT, Color::from_rgba(150, 150, 150, 255));
    y += LINE_HEIGHT;

    // Separator line
    y += 6.0;
    draw_line(x, y, x + rect.w - 4.0, y, 1.0, Color::from_rgba(60, 60, 70, 255));
    y += 10.0;

    // Properties for selected room
    // Extract values first to avoid borrow conflicts with mutations
    let room_data = state.current_room().map(|room| {
        // Count lights by checking asset components
        let light_count = room.objects.iter().filter(|obj| {
            state.asset_library.get_by_id(obj.asset_id)
                .map(|asset| asset.has_light())
                .unwrap_or(false)
        }).count();
        (
            room.position,
            room.width,
            room.depth,
            room.iter_sectors().count(),
            room.portals.len(),
            light_count,
            room.ambient,
            room.fog.enabled,
            room.fog.color,
            room.fog.start,
            room.fog.falloff,
            room.fog.cull_offset,
        )
    });

    if let Some((position, width, depth, sector_count, portal_count, light_count, ambient, fog_enabled, fog_color, fog_start, fog_falloff, fog_cull_offset)) = room_data {
        // Section header
        draw_text("Properties", x, (y + 10.0).floor(), FONT_SIZE_HEADER, Color::from_rgba(150, 150, 150, 255));
        y += LINE_HEIGHT;

        draw_text(
            &format!("Pos: ({:.0}, {:.0}, {:.0})", position.x, position.y, position.z),
            x, (y + 10.0).floor(), FONT_SIZE_CONTENT, WHITE,
        );
        y += LINE_HEIGHT;

        draw_text(&format!("Size: {}x{}", width, depth), x, (y + 10.0).floor(), FONT_SIZE_CONTENT, WHITE);
        y += LINE_HEIGHT;

        draw_text(&format!("Sectors: {}", sector_count), x, (y + 10.0).floor(), FONT_SIZE_CONTENT, WHITE);
        y += LINE_HEIGHT;

        draw_text(&format!("Portals: {}", portal_count), x, (y + 10.0).floor(), FONT_SIZE_CONTENT, WHITE);
        y += LINE_HEIGHT;

        draw_text(&format!("Lights: {}", light_count), x, (y + 10.0).floor(), FONT_SIZE_CONTENT, WHITE);
        y += LINE_HEIGHT;

        // Ambient light slider (0-31 display, maps to 0.0-1.0 internally)
        y += 8.0;
        let slider_height = 12.0;
        let label_width = 55.0;
        let value_width = 24.0;
        let slider_x = x + label_width;
        let slider_width = rect.w - label_width - value_width - 12.0;

        let text_color = Color::new(0.8, 0.8, 0.8, 1.0);
        let track_bg = Color::new(0.15, 0.15, 0.18, 1.0);
        let tint = Color::new(0.9, 0.85, 0.4, 1.0); // Yellow/warm for light

        // Label
        draw_text("Ambient", x, y + slider_height - 2.0, 11.0, text_color);

        // Convert ambient (0.0-1.0) to display value (0-31)
        let ambient_31 = (ambient * 31.0).round() as u8;

        // Slider track background
        let track_rect = Rect::new(slider_x, y, slider_width, slider_height);
        draw_rectangle(track_rect.x, track_rect.y, track_rect.w, track_rect.h, track_bg);

        // Filled portion
        let fill_ratio = ambient_31 as f32 / 31.0;
        let fill_width = fill_ratio * slider_width;
        draw_rectangle(track_rect.x, track_rect.y, fill_width, track_rect.h, tint);

        // Thumb indicator
        let thumb_x = track_rect.x + fill_width - 1.0;
        draw_rectangle(thumb_x, track_rect.y, 3.0, track_rect.h, WHITE);

        // Value text
        draw_text(&format!("{:2}", ambient_31), slider_x + slider_width + 4.0, y + slider_height - 2.0, 11.0, text_color);

        // Handle slider interaction
        let hovered = ctx.mouse.inside(&track_rect);

        // Start dragging
        if hovered && ctx.mouse.left_pressed {
            state.ambient_slider_active = true;
        }

        // Continue dragging
        if state.ambient_slider_active && ctx.mouse.left_down {
            let rel_x = (ctx.mouse.x - track_rect.x).clamp(0.0, slider_width);
            let new_val = ((rel_x / slider_width) * 31.0).round() as u8;
            let new_ambient = new_val as f32 / 31.0;
            if let Some(room) = state.level.rooms.get_mut(state.current_room) {
                if (room.ambient - new_ambient).abs() > 0.001 {
                    room.ambient = new_ambient;
                }
            }
        }

        // End dragging
        if state.ambient_slider_active && !ctx.mouse.left_down {
            state.ambient_slider_active = false;
        }

        // === FOG SETTINGS (PS1-style depth cueing) ===
        y += LINE_HEIGHT + 4.0;
        draw_text("Fog (Depth Cueing)", x, (y + 10.0).floor(), FONT_SIZE_CONTENT, WHITE);
        y += LINE_HEIGHT;

        // Fog enable checkbox
        let checkbox_size = 12.0;
        let checkbox_rect = Rect::new(x, y, checkbox_size, checkbox_size);
        let checkbox_bg = Color::new(0.2, 0.2, 0.25, 1.0);
        let checkbox_check = Color::new(0.4, 0.8, 1.0, 1.0);

        draw_rectangle(checkbox_rect.x, checkbox_rect.y, checkbox_rect.w, checkbox_rect.h, checkbox_bg);
        if fog_enabled {
            draw_rectangle(checkbox_rect.x + 2.0, checkbox_rect.y + 2.0, checkbox_rect.w - 4.0, checkbox_rect.h - 4.0, checkbox_check);
        }
        draw_text("Enabled", x + checkbox_size + 6.0, y + checkbox_size - 2.0, 11.0, text_color);

        // Handle fog enable checkbox click
        if ctx.mouse.inside(&checkbox_rect) && ctx.mouse.left_pressed {
            if let Some(room) = state.level.rooms.get_mut(state.current_room) {
                room.fog.enabled = !room.fog.enabled;
            }
        }

        y += LINE_HEIGHT;

        // Only show fog controls if fog is enabled
        if fog_enabled {
            let fog_tint = Color::new(0.6, 0.7, 0.9, 1.0);
            let r_label_w = 12.0;

            // Fog color RGB sliders
            draw_text("Color", x, y + slider_height - 2.0, 11.0, text_color);
            y += LINE_HEIGHT - 2.0;

            // R slider
            draw_text("R", x + 4.0, y + slider_height - 2.0, 10.0, Color::new(1.0, 0.5, 0.5, 1.0));
            let r_track = Rect::new(x + r_label_w + 4.0, y, slider_width - r_label_w, slider_height);
            draw_rectangle(r_track.x, r_track.y, r_track.w, r_track.h, track_bg);
            let r_fill = fog_color.0 * r_track.w;
            draw_rectangle(r_track.x, r_track.y, r_fill, r_track.h, Color::new(1.0, 0.3, 0.3, 1.0));
            draw_rectangle(r_track.x + r_fill - 1.0, r_track.y, 3.0, r_track.h, WHITE);
            draw_text(&format!("{:.0}", fog_color.0 * 31.0), r_track.x + r_track.w + 4.0, y + slider_height - 2.0, 10.0, text_color);

            if ctx.mouse.inside(&r_track) && ctx.mouse.left_down {
                if let Some(room) = state.level.rooms.get_mut(state.current_room) {
                    room.fog.color.0 = ((ctx.mouse.x - r_track.x) / r_track.w).clamp(0.0, 1.0);
                }
            }
            y += LINE_HEIGHT - 4.0;

            // G slider
            draw_text("G", x + 4.0, y + slider_height - 2.0, 10.0, Color::new(0.5, 1.0, 0.5, 1.0));
            let g_track = Rect::new(x + r_label_w + 4.0, y, slider_width - r_label_w, slider_height);
            draw_rectangle(g_track.x, g_track.y, g_track.w, g_track.h, track_bg);
            let g_fill = fog_color.1 * g_track.w;
            draw_rectangle(g_track.x, g_track.y, g_fill, g_track.h, Color::new(0.3, 1.0, 0.3, 1.0));
            draw_rectangle(g_track.x + g_fill - 1.0, g_track.y, 3.0, g_track.h, WHITE);
            draw_text(&format!("{:.0}", fog_color.1 * 31.0), g_track.x + g_track.w + 4.0, y + slider_height - 2.0, 10.0, text_color);

            if ctx.mouse.inside(&g_track) && ctx.mouse.left_down {
                if let Some(room) = state.level.rooms.get_mut(state.current_room) {
                    room.fog.color.1 = ((ctx.mouse.x - g_track.x) / g_track.w).clamp(0.0, 1.0);
                }
            }
            y += LINE_HEIGHT - 4.0;

            // B slider
            draw_text("B", x + 4.0, y + slider_height - 2.0, 10.0, Color::new(0.5, 0.5, 1.0, 1.0));
            let b_track = Rect::new(x + r_label_w + 4.0, y, slider_width - r_label_w, slider_height);
            draw_rectangle(b_track.x, b_track.y, b_track.w, b_track.h, track_bg);
            let b_fill = fog_color.2 * b_track.w;
            draw_rectangle(b_track.x, b_track.y, b_fill, b_track.h, Color::new(0.3, 0.3, 1.0, 1.0));
            draw_rectangle(b_track.x + b_fill - 1.0, b_track.y, 3.0, b_track.h, WHITE);
            draw_text(&format!("{:.0}", fog_color.2 * 31.0), b_track.x + b_track.w + 4.0, y + slider_height - 2.0, 10.0, text_color);

            if ctx.mouse.inside(&b_track) && ctx.mouse.left_down {
                if let Some(room) = state.level.rooms.get_mut(state.current_room) {
                    room.fog.color.2 = ((ctx.mouse.x - b_track.x) / b_track.w).clamp(0.0, 1.0);
                }
            }
            y += LINE_HEIGHT;

            // Fog start distance slider (0-50000) - world units, SECTOR_SIZE=1024
            let fog_max = 50000.0;
            draw_text("Start", x, y + slider_height - 2.0, 11.0, text_color);
            let start_track = Rect::new(slider_x, y, slider_width, slider_height);
            draw_rectangle(start_track.x, start_track.y, start_track.w, start_track.h, track_bg);
            let start_fill = (fog_start / fog_max).min(1.0) * start_track.w;
            draw_rectangle(start_track.x, start_track.y, start_fill, start_track.h, fog_tint);
            draw_rectangle(start_track.x + start_fill - 1.0, start_track.y, 3.0, start_track.h, WHITE);
            draw_text(&format!("{:.0}", fog_start), slider_x + slider_width + 4.0, y + slider_height - 2.0, 10.0, text_color);

            if ctx.mouse.inside(&start_track) && ctx.mouse.left_down {
                if let Some(room) = state.level.rooms.get_mut(state.current_room) {
                    let step = 512.0;
                    let raw = (ctx.mouse.x - start_track.x) / start_track.w * fog_max;
                    let new_start = (raw / step).round() * step;
                    room.fog.start = new_start.clamp(0.0, fog_max);
                }
            }
            y += LINE_HEIGHT;

            // Fog falloff distance slider (512-50000) - distance from start to full fog
            let falloff_max = 50000.0;
            let falloff_min = 512.0;
            draw_text("Falloff", x, y + slider_height - 2.0, 11.0, text_color);
            let falloff_track = Rect::new(slider_x, y, slider_width, slider_height);
            draw_rectangle(falloff_track.x, falloff_track.y, falloff_track.w, falloff_track.h, track_bg);
            let falloff_fill = (fog_falloff / falloff_max).min(1.0) * falloff_track.w;
            draw_rectangle(falloff_track.x, falloff_track.y, falloff_fill, falloff_track.h, fog_tint);
            draw_rectangle(falloff_track.x + falloff_fill - 1.0, falloff_track.y, 3.0, falloff_track.h, WHITE);
            draw_text(&format!("{:.0}", fog_falloff), slider_x + slider_width + 4.0, y + slider_height - 2.0, 10.0, text_color);

            if ctx.mouse.inside(&falloff_track) && ctx.mouse.left_down {
                if let Some(room) = state.level.rooms.get_mut(state.current_room) {
                    let step = 512.0;
                    let raw = (ctx.mouse.x - falloff_track.x) / falloff_track.w * falloff_max;
                    let new_falloff = (raw / step).round() * step;
                    room.fog.falloff = new_falloff.clamp(falloff_min, falloff_max);
                }
            }
            y += LINE_HEIGHT;

            // Fog cull offset slider (0-10000) - additional distance after full fog before culling
            let cull_max = 10000.0;
            draw_text("Cull +", x, y + slider_height - 2.0, 11.0, text_color);
            let cull_track = Rect::new(slider_x, y, slider_width, slider_height);
            draw_rectangle(cull_track.x, cull_track.y, cull_track.w, cull_track.h, track_bg);
            let cull_fill = (fog_cull_offset / cull_max).min(1.0) * cull_track.w;
            draw_rectangle(cull_track.x, cull_track.y, cull_fill, cull_track.h, fog_tint);
            draw_rectangle(cull_track.x + cull_fill - 1.0, cull_track.y, 3.0, cull_track.h, WHITE);
            draw_text(&format!("{:.0}", fog_cull_offset), slider_x + slider_width + 4.0, y + slider_height - 2.0, 10.0, text_color);

            if ctx.mouse.inside(&cull_track) && ctx.mouse.left_down {
                if let Some(room) = state.level.rooms.get_mut(state.current_room) {
                    let step = 512.0;
                    let raw = (ctx.mouse.x - cull_track.x) / cull_track.w * cull_max;
                    let new_cull = (raw / step).round() * step;
                    room.fog.cull_offset = new_cull.clamp(0.0, cull_max);
                }
            }
        }
    } else {
        draw_text("No room selected", x, (y + 10.0).floor(), FONT_SIZE_CONTENT, Color::from_rgba(150, 150, 150, 255));
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

/// Apply normal mode to a face within a sector
fn apply_normal_mode_to_face(
    level: &mut crate::world::Level,
    room: usize,
    x: usize,
    z: usize,
    face: &SectorFace,
    mode: crate::world::FaceNormalMode,
) {
    if let Some(r) = level.rooms.get_mut(room) {
        if let Some(s) = r.get_sector_mut(x, z) {
            match face {
                SectorFace::Floor => {
                    if let Some(f) = &mut s.floor {
                        f.normal_mode = mode;
                    }
                }
                SectorFace::Ceiling => {
                    if let Some(c) = &mut s.ceiling {
                        c.normal_mode = mode;
                    }
                }
                SectorFace::WallNorth(i) => {
                    if let Some(w) = s.walls_north.get_mut(*i) {
                        w.normal_mode = mode;
                    }
                }
                SectorFace::WallEast(i) => {
                    if let Some(w) = s.walls_east.get_mut(*i) {
                        w.normal_mode = mode;
                    }
                }
                SectorFace::WallSouth(i) => {
                    if let Some(w) = s.walls_south.get_mut(*i) {
                        w.normal_mode = mode;
                    }
                }
                SectorFace::WallWest(i) => {
                    if let Some(w) = s.walls_west.get_mut(*i) {
                        w.normal_mode = mode;
                    }
                }
                SectorFace::WallNwSe(i) => {
                    if let Some(w) = s.walls_nwse.get_mut(*i) {
                        w.normal_mode = mode;
                    }
                }
                SectorFace::WallNeSw(i) => {
                    if let Some(w) = s.walls_nesw.get_mut(*i) {
                        w.normal_mode = mode;
                    }
                }
            }
        }
    }
}

/// Apply black_transparent to a face within a sector
fn apply_black_transparent_to_face(
    level: &mut crate::world::Level,
    room: usize,
    x: usize,
    z: usize,
    face: &SectorFace,
    value: bool,
) {
    if let Some(r) = level.rooms.get_mut(room) {
        if let Some(s) = r.get_sector_mut(x, z) {
            match face {
                SectorFace::Floor => {
                    if let Some(f) = &mut s.floor {
                        f.black_transparent = value;
                    }
                }
                SectorFace::Ceiling => {
                    if let Some(c) = &mut s.ceiling {
                        c.black_transparent = value;
                    }
                }
                SectorFace::WallNorth(i) => {
                    if let Some(w) = s.walls_north.get_mut(*i) {
                        w.black_transparent = value;
                    }
                }
                SectorFace::WallEast(i) => {
                    if let Some(w) = s.walls_east.get_mut(*i) {
                        w.black_transparent = value;
                    }
                }
                SectorFace::WallSouth(i) => {
                    if let Some(w) = s.walls_south.get_mut(*i) {
                        w.black_transparent = value;
                    }
                }
                SectorFace::WallWest(i) => {
                    if let Some(w) = s.walls_west.get_mut(*i) {
                        w.black_transparent = value;
                    }
                }
                SectorFace::WallNwSe(i) => {
                    if let Some(w) = s.walls_nwse.get_mut(*i) {
                        w.black_transparent = value;
                    }
                }
                SectorFace::WallNeSw(i) => {
                    if let Some(w) = s.walls_nesw.get_mut(*i) {
                        w.black_transparent = value;
                    }
                }
            }
        }
    }
}

/// Apply vertex colors to a face within a sector
fn apply_vertex_colors_to_face(
    level: &mut crate::world::Level,
    room: usize,
    x: usize,
    z: usize,
    face: &SectorFace,
    vertex_indices: &[usize],
    color: crate::rasterizer::Color,
) {
    if let Some(r) = level.rooms.get_mut(room) {
        if let Some(s) = r.get_sector_mut(x, z) {
            match face {
                SectorFace::Floor => {
                    if let Some(f) = &mut s.floor {
                        for &idx in vertex_indices {
                            if idx < 4 {
                                f.colors[idx] = color;
                            }
                        }
                    }
                }
                SectorFace::Ceiling => {
                    if let Some(c) = &mut s.ceiling {
                        for &idx in vertex_indices {
                            if idx < 4 {
                                c.colors[idx] = color;
                            }
                        }
                    }
                }
                SectorFace::WallNorth(i) => {
                    if let Some(w) = s.walls_north.get_mut(*i) {
                        for &idx in vertex_indices {
                            if idx < 4 {
                                w.colors[idx] = color;
                            }
                        }
                    }
                }
                SectorFace::WallEast(i) => {
                    if let Some(w) = s.walls_east.get_mut(*i) {
                        for &idx in vertex_indices {
                            if idx < 4 {
                                w.colors[idx] = color;
                            }
                        }
                    }
                }
                SectorFace::WallSouth(i) => {
                    if let Some(w) = s.walls_south.get_mut(*i) {
                        for &idx in vertex_indices {
                            if idx < 4 {
                                w.colors[idx] = color;
                            }
                        }
                    }
                }
                SectorFace::WallWest(i) => {
                    if let Some(w) = s.walls_west.get_mut(*i) {
                        for &idx in vertex_indices {
                            if idx < 4 {
                                w.colors[idx] = color;
                            }
                        }
                    }
                }
                SectorFace::WallNwSe(i) => {
                    if let Some(w) = s.walls_nwse.get_mut(*i) {
                        for &idx in vertex_indices {
                            if idx < 4 {
                                w.colors[idx] = color;
                            }
                        }
                    }
                }
                SectorFace::WallNeSw(i) => {
                    if let Some(w) = s.walls_nesw.get_mut(*i) {
                        for &idx in vertex_indices {
                            if idx < 4 {
                                w.colors[idx] = color;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Calculate height needed for a horizontal face container
fn horizontal_face_container_height(face: &crate::world::HorizontalFace, is_floor: bool) -> f32 {
    let line_height = 18.0;
    let header_height = 22.0;
    let button_row_height = 24.0;
    let color_row_height = 20.0; // Color preview + label
    let uv_controls_height = 80.0; // X offset + Y offset + scale row + angle row (4 rows × 20px)
    let color_picker_height = ps1_color_picker_height() + 54.0; // PS1 color picker widget
    let normal_mode_height = 40.0; // Label + 3-way toggle
    let split_diagram_height = 50.0; // Mini split diagram with toggle
    let triangle_textures_height = 40.0; // Dual texture slots with link toggle
    let extrude_button_height = if is_floor { 56.0 } else { 0.0 }; // Extrude button only for floors
    // Height link row + optional height controls when unlinked
    let height_link_row = 20.0;
    let height_controls_height = if face.has_split_heights() { 36.0 } else { 0.0 }; // 2 rows × 18px when unlinked
    let lines = 1; // walkable only (height moved to link row)
    // Add space for UV coordinates, controls, buttons, color, color picker, normal mode, split diagram, triangle textures, and extrude
    let uv_lines = 1; // Just coordinates
    header_height + CONTAINER_PADDING * 2.0 + (lines as f32) * line_height + (uv_lines as f32) * line_height + uv_controls_height + button_row_height + color_row_height + color_picker_height + normal_mode_height + split_diagram_height + triangle_textures_height + height_link_row + height_controls_height + extrude_button_height
}

/// Calculate height needed for a wall face container
fn wall_face_container_height(_wall: &crate::world::VerticalFace) -> f32 {
    let line_height = 18.0;
    let header_height = 22.0;
    let button_row_height = 24.0;
    let color_row_height = 20.0; // Color preview + label
    let uv_controls_height = 80.0; // X offset + Y offset + scale row + angle row (4 rows × 20px)
    let color_picker_height = ps1_color_picker_height() + 54.0; // PS1 color picker widget
    let normal_mode_height = 40.0; // Label + 3-way toggle
    let lines = 3; // texture, y range, blend
    // Add space for UV coordinates, controls, buttons, color, color picker, and normal mode
    let uv_lines = 1; // Just coordinates
    header_height + CONTAINER_PADDING * 2.0 + (lines as f32) * line_height + (uv_lines as f32) * line_height + uv_controls_height + button_row_height + color_row_height + color_picker_height + normal_mode_height
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
    let container_height = horizontal_face_container_height(face, is_floor);

    // Draw container
    draw_container_start(x, y, width, container_height, label, label_color);

    // Content starts after header
    let content_x = x + CONTAINER_PADDING;
    let mut content_y = y + header_height + CONTAINER_PADDING;

    // === Split Direction with Mini Diagram ===
    let diagram_size = 36.0;
    let diagram_x = content_x;
    let diagram_y = content_y;

    // Draw mini quad diagram showing split direction
    let quad_color = Color::from_rgba(60, 70, 80, 255);
    let line_color = Color::from_rgba(255, 180, 100, 255);
    let label_color_dim = Color::from_rgba(120, 120, 120, 255);

    draw_rectangle(diagram_x, diagram_y, diagram_size, diagram_size, quad_color);
    draw_rectangle_lines(diagram_x, diagram_y, diagram_size, diagram_size, 1.0, Color::from_rgba(80, 90, 100, 255));

    // Draw diagonal based on split direction
    use crate::world::SplitDirection;
    match face.split_direction {
        SplitDirection::NwSe => {
            // NW to SE diagonal
            draw_line(diagram_x, diagram_y, diagram_x + diagram_size, diagram_y + diagram_size, 2.0, line_color);
        }
        SplitDirection::NeSw => {
            // NE to SW diagonal
            draw_line(diagram_x + diagram_size, diagram_y, diagram_x, diagram_y + diagram_size, 2.0, line_color);
        }
    }

    // Triangle labels inside the diagram
    let tri1_label_x;
    let tri1_label_y;
    let tri2_label_x;
    let tri2_label_y;
    match face.split_direction {
        SplitDirection::NwSe => {
            // Tri1 is top-right (NW,NE,SE), Tri2 is bottom-left (NW,SE,SW)
            tri1_label_x = diagram_x + diagram_size * 0.65;
            tri1_label_y = diagram_y + diagram_size * 0.35;
            tri2_label_x = diagram_x + diagram_size * 0.25;
            tri2_label_y = diagram_y + diagram_size * 0.7;
        }
        SplitDirection::NeSw => {
            // Tri1 is top-left (NW,NE,SW), Tri2 is bottom-right (NE,SE,SW)
            tri1_label_x = diagram_x + diagram_size * 0.25;
            tri1_label_y = diagram_y + diagram_size * 0.35;
            tri2_label_x = diagram_x + diagram_size * 0.65;
            tri2_label_y = diagram_y + diagram_size * 0.7;
        }
    }
    draw_text("1", tri1_label_x.floor(), tri1_label_y.floor(), 10.0, WHITE);
    draw_text("2", tri2_label_x.floor(), tri2_label_y.floor(), 10.0, WHITE);

    // Split direction toggle button next to diagram
    let toggle_x = diagram_x + diagram_size + 8.0;
    let toggle_btn_rect = Rect::new(toggle_x, diagram_y + 8.0, 50.0, 20.0);
    let toggle_hovered = ctx.mouse.inside(&toggle_btn_rect);
    let toggle_bg = if toggle_hovered {
        Color::from_rgba(60, 80, 100, 255)
    } else {
        Color::from_rgba(45, 50, 60, 255)
    };
    draw_rectangle(toggle_btn_rect.x, toggle_btn_rect.y, toggle_btn_rect.w, toggle_btn_rect.h, toggle_bg);
    draw_rectangle_lines(toggle_btn_rect.x, toggle_btn_rect.y, toggle_btn_rect.w, toggle_btn_rect.h, 1.0, Color::from_rgba(80, 90, 100, 255));
    draw_text(face.split_direction.label(), (toggle_btn_rect.x + 6.0).floor(), (toggle_btn_rect.y + 14.0).floor(), 11.0, WHITE);

    if toggle_hovered && ctx.mouse.left_pressed {
        state.save_undo();
        if let Some(r) = state.level.rooms.get_mut(room_idx) {
            if let Some(s) = r.get_sector_mut(gx, gz) {
                let face_ref = if is_floor { &mut s.floor } else { &mut s.ceiling };
                if let Some(f) = face_ref {
                    f.split_direction = f.split_direction.next();
                }
            }
        }
    }

    if toggle_hovered {
        ctx.tooltip = Some(crate::ui::PendingTooltip {
            text: String::from("Toggle split direction"),
            x: ctx.mouse.x,
            y: ctx.mouse.y,
        });
    }
    content_y += diagram_size + 8.0;

    // === Dual Triangle Texture Slots with Link Toggle ===
    use super::TriangleSelection;

    let slot_width = 70.0;
    let slot_height = 32.0;
    let link_btn_size = 18.0;
    let spacing = 4.0;

    // Determine if textures are linked (texture_2 is None means linked)
    let textures_linked = face.texture_2.is_none();
    let tex1 = &face.texture;
    let tex2 = face.texture_2.as_ref().unwrap_or(&face.texture);

    // Determine which slot is selected based on state
    let slot1_selected = matches!(state.selected_triangle, TriangleSelection::Tri1 | TriangleSelection::Both);
    let slot2_selected = matches!(state.selected_triangle, TriangleSelection::Tri2 | TriangleSelection::Both);

    // Triangle 1 texture slot
    let slot1_x = content_x;
    let slot1_rect = Rect::new(slot1_x, content_y, slot_width, slot_height);
    let slot1_hovered = ctx.mouse.inside(&slot1_rect);
    let slot1_bg = if slot1_hovered {
        Color::from_rgba(50, 60, 70, 255)
    } else if slot1_selected {
        Color::from_rgba(40, 50, 65, 255)
    } else {
        Color::from_rgba(35, 40, 50, 255)
    };
    let slot1_border = if slot1_selected {
        Color::from_rgba(100, 150, 200, 255)
    } else {
        Color::from_rgba(80, 90, 100, 255)
    };
    draw_rectangle(slot1_rect.x, slot1_rect.y, slot1_rect.w, slot1_rect.h, slot1_bg);
    draw_rectangle_lines(slot1_rect.x, slot1_rect.y, slot1_rect.w, slot1_rect.h,
        if slot1_selected { 2.0 } else { 1.0 }, slot1_border);

    // Tri 1 label and texture name
    draw_text("Tri 1", (slot1_rect.x + 4.0).floor(), (slot1_rect.y + 12.0).floor(), 9.0, label_color_dim);
    let tex1_name = if tex1.is_valid() { &tex1.name } else { "(none)" };
    let tex1_display: String = if tex1_name.len() > 8 { format!("{}...", &tex1_name[..6]) } else { tex1_name.to_string() };
    draw_text(&tex1_display, (slot1_rect.x + 4.0).floor(), (slot1_rect.y + 24.0).floor(), 10.0, WHITE);

    // Link button between slots
    let link_x = slot1_x + slot_width + spacing;
    let link_rect = Rect::new(link_x, content_y + (slot_height - link_btn_size) / 2.0, link_btn_size, link_btn_size);
    let link_icon = if textures_linked { icon::LINK } else { icon::LINK_OFF };
    let link_tooltip = if textures_linked { "Unlink triangle textures" } else { "Link triangle textures" };
    let link_clicked = crate::ui::icon_button(ctx, link_rect, link_icon, icon_font, link_tooltip);

    if link_clicked {
        state.save_undo();
        if let Some(r) = state.level.rooms.get_mut(room_idx) {
            if let Some(s) = r.get_sector_mut(gx, gz) {
                let face_ref = if is_floor { &mut s.floor } else { &mut s.ceiling };
                if let Some(f) = face_ref {
                    if textures_linked {
                        // Unlink: copy texture to texture_2, select Tri1
                        f.texture_2 = Some(f.texture.clone());
                        state.selected_triangle = TriangleSelection::Tri1;
                    } else {
                        // Link: clear texture_2 (will use texture), select Both
                        f.texture_2 = None;
                        state.selected_triangle = TriangleSelection::Both;
                    }
                }
            }
        }
    }

    // Triangle 2 texture slot
    let slot2_x = link_x + link_btn_size + spacing;
    let slot2_rect = Rect::new(slot2_x, content_y, slot_width, slot_height);
    let slot2_hovered = ctx.mouse.inside(&slot2_rect);
    let slot2_bg = if slot2_hovered {
        Color::from_rgba(50, 60, 70, 255)
    } else if slot2_selected {
        Color::from_rgba(40, 50, 65, 255)
    } else {
        Color::from_rgba(35, 40, 50, 255)
    };
    let slot2_border = if slot2_selected {
        Color::from_rgba(100, 150, 200, 255)
    } else {
        Color::from_rgba(80, 90, 100, 255)
    };
    draw_rectangle(slot2_rect.x, slot2_rect.y, slot2_rect.w, slot2_rect.h, slot2_bg);
    draw_rectangle_lines(slot2_rect.x, slot2_rect.y, slot2_rect.w, slot2_rect.h,
        if slot2_selected { 2.0 } else { 1.0 }, slot2_border);

    // Tri 2 label and texture name
    draw_text("Tri 2", (slot2_rect.x + 4.0).floor(), (slot2_rect.y + 12.0).floor(), 9.0, label_color_dim);
    let tex2_name = if tex2.is_valid() { &tex2.name } else { "(none)" };
    let tex2_display: String = if tex2_name.len() > 8 { format!("{}...", &tex2_name[..6]) } else { tex2_name.to_string() };
    draw_text(&tex2_display, (slot2_rect.x + 4.0).floor(), (slot2_rect.y + 24.0).floor(), 10.0, WHITE);

    // Handle texture slot clicks - SELECT the slot and update selected_texture to match
    if slot1_hovered && ctx.mouse.left_pressed {
        if textures_linked {
            // Linked: select both, update selected_texture to match
            state.selected_triangle = TriangleSelection::Both;
            state.selected_texture = tex1.clone();
        } else {
            // Unlinked: select just tri1
            state.selected_triangle = TriangleSelection::Tri1;
            state.selected_texture = tex1.clone();
        }
    }
    if slot2_hovered && ctx.mouse.left_pressed {
        if textures_linked {
            // Linked: select both, update selected_texture to match
            state.selected_triangle = TriangleSelection::Both;
            state.selected_texture = tex2.clone();
        } else {
            // Unlinked: select just tri2
            state.selected_triangle = TriangleSelection::Tri2;
            state.selected_texture = tex2.clone();
        }
    }

    if slot1_hovered {
        let tip = if textures_linked { "Click to select (linked)" } else { "Click to select triangle 1" };
        ctx.tooltip = Some(crate::ui::PendingTooltip { text: String::from(tip), x: ctx.mouse.x, y: ctx.mouse.y });
    }
    if slot2_hovered {
        let tip = if textures_linked { "Click to select (linked)" } else { "Click to select triangle 2" };
        ctx.tooltip = Some(crate::ui::PendingTooltip { text: String::from(tip), x: ctx.mouse.x, y: ctx.mouse.y });
    }

    content_y += slot_height + 8.0;

    // === Triangle Heights with Link Toggle ===
    let heights_linked = face.heights_linked();
    let heights_1 = &face.heights;
    let heights_2 = face.get_heights_2();

    // Height link button
    let height_link_btn_size = 18.0;
    let height_link_rect = Rect::new(content_x, content_y, height_link_btn_size, height_link_btn_size);
    let height_link_icon = if heights_linked { icon::LINK } else { icon::LINK_OFF };
    let height_link_tooltip = if heights_linked { "Unlink triangle heights" } else { "Link triangle heights" };
    let height_link_clicked = crate::ui::icon_button(ctx, height_link_rect, height_link_icon, icon_font, height_link_tooltip);

    if height_link_clicked {
        state.save_undo();
        if let Some(r) = state.level.rooms.get_mut(room_idx) {
            if let Some(s) = r.get_sector_mut(gx, gz) {
                let face_ref = if is_floor { &mut s.floor } else { &mut s.ceiling };
                if let Some(f) = face_ref {
                    if heights_linked {
                        // Unlink: copy heights to heights_2
                        f.heights_2 = Some(f.heights);
                    } else {
                        // Link: clear heights_2 (will use heights)
                        f.heights_2 = None;
                    }
                }
            }
        }
    }

    // Height display/label next to link button
    let height_label_x = content_x + height_link_btn_size + 6.0;
    if heights_linked {
        // Show single height (base height from NW corner)
        draw_text(&format!("Height: {:.0}", heights_1[0]), height_label_x.floor(), (content_y + 13.0).floor(), 12.0, WHITE);
    } else {
        // Show both heights
        draw_text("Heights unlinked", height_label_x.floor(), (content_y + 13.0).floor(), 12.0, Color::from_rgba(255, 180, 100, 255));
    }
    content_y += 20.0;

    // When heights are unlinked, show height controls for each triangle
    if !heights_linked {
        // Triangle 1 height row
        draw_text("Tri 1:", content_x.floor(), (content_y + 12.0).floor(), 11.0, label_color_dim);
        let h1_display = format!("{:.0}", heights_1[0]);
        draw_text(&h1_display, (content_x + 40.0).floor(), (content_y + 12.0).floor(), 11.0, WHITE);

        // Height adjustment buttons for Tri 1
        let adj_btn_size = 16.0;
        let adj_x = content_x + 70.0;
        let minus_rect = Rect::new(adj_x, content_y, adj_btn_size, adj_btn_size);
        if crate::ui::icon_button(ctx, minus_rect, icon::MINUS, icon_font, "Lower Tri 1 by 256") {
            state.save_undo();
            if let Some(r) = state.level.rooms.get_mut(room_idx) {
                if let Some(s) = r.get_sector_mut(gx, gz) {
                    let face_ref = if is_floor { &mut s.floor } else { &mut s.ceiling };
                    if let Some(f) = face_ref {
                        for h in &mut f.heights {
                            *h -= 256.0;
                        }
                    }
                }
            }
        }
        let plus_rect = Rect::new(adj_x + adj_btn_size + 2.0, content_y, adj_btn_size, adj_btn_size);
        if crate::ui::icon_button(ctx, plus_rect, icon::PLUS, icon_font, "Raise Tri 1 by 256") {
            state.save_undo();
            if let Some(r) = state.level.rooms.get_mut(room_idx) {
                if let Some(s) = r.get_sector_mut(gx, gz) {
                    let face_ref = if is_floor { &mut s.floor } else { &mut s.ceiling };
                    if let Some(f) = face_ref {
                        for h in &mut f.heights {
                            *h += 256.0;
                        }
                    }
                }
            }
        }
        content_y += 18.0;

        // Triangle 2 height row
        draw_text("Tri 2:", content_x.floor(), (content_y + 12.0).floor(), 11.0, label_color_dim);
        let h2_display = format!("{:.0}", heights_2[0]);
        draw_text(&h2_display, (content_x + 40.0).floor(), (content_y + 12.0).floor(), 11.0, WHITE);

        // Height adjustment buttons for Tri 2
        let minus_rect = Rect::new(adj_x, content_y, adj_btn_size, adj_btn_size);
        if crate::ui::icon_button(ctx, minus_rect, icon::MINUS, icon_font, "Lower Tri 2 by 256") {
            state.save_undo();
            if let Some(r) = state.level.rooms.get_mut(room_idx) {
                if let Some(s) = r.get_sector_mut(gx, gz) {
                    let face_ref = if is_floor { &mut s.floor } else { &mut s.ceiling };
                    if let Some(f) = face_ref {
                        if let Some(h2) = &mut f.heights_2 {
                            for h in h2 {
                                *h -= 256.0;
                            }
                        }
                    }
                }
            }
        }
        let plus_rect = Rect::new(adj_x + adj_btn_size + 2.0, content_y, adj_btn_size, adj_btn_size);
        if crate::ui::icon_button(ctx, plus_rect, icon::PLUS, icon_font, "Raise Tri 2 by 256") {
            state.save_undo();
            if let Some(r) = state.level.rooms.get_mut(room_idx) {
                if let Some(s) = r.get_sector_mut(gx, gz) {
                    let face_ref = if is_floor { &mut s.floor } else { &mut s.ceiling };
                    if let Some(f) = face_ref {
                        if let Some(h2) = &mut f.heights_2 {
                            for h in h2 {
                                *h += 256.0;
                            }
                        }
                    }
                }
            }
        }
        content_y += 18.0;
    }

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

    // UV coordinates display (scaled by UV_SCALE)
    let uv = face.uv.unwrap_or([
        crate::rasterizer::Vec2::new(0.0, 0.0),           // NW
        crate::rasterizer::Vec2::new(UV_SCALE, 0.0),      // NE
        crate::rasterizer::Vec2::new(UV_SCALE, UV_SCALE), // SE
        crate::rasterizer::Vec2::new(0.0, UV_SCALE),      // SW
    ]);
    draw_text(&format!("UV: [{:.2},{:.2}] [{:.2},{:.2}]", uv[0].x, uv[0].y, uv[1].x, uv[1].y),
        content_x.floor(), (content_y + 12.0).floor(), 11.0, Color::from_rgba(120, 120, 120, 255));
    content_y += line_height;

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
    content_y += 80.0; // Height of UV controls (4 rows × 20px: X offset, Y offset, Scale, Angle)

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
    btn_x += btn_size + btn_spacing;

    // 1:1 Texel mapping button - resets scale to 1.0 (one texture per block)
    let texel_rect = Rect::new(btn_x, content_y, btn_size, btn_size);
    if crate::ui::icon_button(ctx, texel_rect, icon::RATIO, icon_font, "1:1 Texel Mapping") {
        state.save_undo();
        if let Some(r) = state.level.rooms.get_mut(room_idx) {
            if let Some(s) = r.get_sector_mut(gx, gz) {
                let face_ref = if is_floor { &mut s.floor } else { &mut s.ceiling };
                if let Some(f) = face_ref {
                    // Reset to default 1:1 mapping: scale 1.0 = one texture per block
                    let mut params = extract_uv_params(&f.uv);
                    params.x_scale = 1.0;
                    params.y_scale = 1.0;
                    f.uv = Some(apply_uv_params(&params));
                }
            }
        }
    }
    content_y += btn_size + 4.0;

    // Face vertex colors (PS1-style texture modulation)
    // Layout: 2x2 vertex swatches on left, color picker on right
    let swatch_size = 18.0;
    let swatch_spacing = 2.0;
    let swatches_width = 2.0 * swatch_size + swatch_spacing; // Width of 2x2 grid
    let picker_offset = swatches_width + 8.0; // Gap between swatches and picker

    // Default to all vertices selected if none are selected
    if state.selected_vertex_indices.is_empty() {
        state.selected_vertex_indices = vec![0, 1, 2, 3];
    }

    // Label
    draw_text("Vertex Colour", content_x.floor(), (content_y + 12.0).floor(), 12.0,
        macroquad::color::Color::from_rgba(150, 150, 150, 255));
    content_y += 16.0;

    let section_start_y = content_y; // Remember where this section starts

    // Draw 4 vertex color swatches in 2x2 grid (NW, NE / SW, SE layout)
    let grid_x = content_x;
    let vertex_labels = ["NW", "NE", "SW", "SE"];
    let grid_positions = [(0, 0), (1, 0), (0, 1), (1, 1)]; // (col, row)
    let vertex_indices = [0, 1, 3, 2]; // Map grid to corner indices: NW=0, NE=1, SE=2, SW=3

    for (grid_idx, &(col, row)) in grid_positions.iter().enumerate() {
        let vert_idx = vertex_indices[grid_idx];
        let vert_color = face.colors[vert_idx];
        let sx = grid_x + (col as f32) * (swatch_size + swatch_spacing);
        let sy = section_start_y + (row as f32) * (swatch_size + swatch_spacing);
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

        // Handle click - toggle selection of this vertex (but don't allow deselecting the last one)
        if hovered && ctx.mouse.left_pressed {
            if is_selected {
                // Only deselect if there's more than one selected
                if state.selected_vertex_indices.len() > 1 {
                    state.selected_vertex_indices.retain(|&v| v != vert_idx);
                }
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

    // Vertical separator between swatches and picker
    let separator_x = content_x + swatches_width + 4.0;
    let swatches_height = 2.0 * swatch_size + swatch_spacing;
    draw_line(separator_x, section_start_y, separator_x, section_start_y + swatches_height, 1.0,
        macroquad::color::Color::from_rgba(60, 60, 65, 255));

    // PS1 color picker to the right of vertex swatches
    let picker_x = content_x + picker_offset;
    let picker_width = width - CONTAINER_PADDING * 2.0 - picker_offset;

    // Get current color to display in picker (use first selected vertex)
    let display_color = {
        let idx = state.selected_vertex_indices[0].min(3);
        face.colors[idx]
    };

    let picker_result = draw_ps1_color_picker(
        ctx,
        picker_x,
        section_start_y,
        picker_width,
        display_color,
        RasterColor::from_ps1(16, 16, 16),
        "",
        &mut state.vertex_color_slider,
    );

    if let Some(new_color) = picker_result.color {
        state.save_undo();
        let vertex_indices = state.selected_vertex_indices.clone();
        // Apply to primary selection
        let primary_face = if is_floor { SectorFace::Floor } else { SectorFace::Ceiling };
        apply_vertex_colors_to_face(&mut state.level, room_idx, gx, gz, &primary_face, &vertex_indices, new_color);
        // Apply to multi-selection (only matching face types: floors or ceilings)
        for sel in state.multi_selection.clone() {
            if let Selection::SectorFace { room, x, z, face } = sel {
                let is_matching = match (&face, is_floor) {
                    (SectorFace::Floor, true) | (SectorFace::Ceiling, false) => true,
                    _ => false,
                };
                if is_matching {
                    apply_vertex_colors_to_face(&mut state.level, room, x, z, &face, &vertex_indices, new_color);
                }
            }
        }
    }

    // Advance content_y by the taller of: swatches (2 rows) or picker
    let swatches_height = 2.0 * swatch_size + swatch_spacing;
    content_y += swatches_height.max(ps1_color_picker_height()) + 8.0;

    // Normal mode 3-way toggle
    draw_text("Normal", content_x.floor(), (content_y + 12.0).floor(), 12.0, Color::from_rgba(150, 150, 150, 255));
    content_y += 16.0;

    let toggle_rect = Rect::new(content_x, content_y, width - CONTAINER_PADDING * 2.0, 24.0);
    let current_mode = match face.normal_mode {
        crate::world::FaceNormalMode::Front => 0,
        crate::world::FaceNormalMode::Both => 1,
        crate::world::FaceNormalMode::Back => 2,
    };
    if let Some(new_mode) = crate::ui::draw_three_way_toggle(ctx, toggle_rect, ["Front", "Both", "Back"], current_mode) {
        state.save_undo();
        let mode = match new_mode {
            0 => crate::world::FaceNormalMode::Front,
            1 => crate::world::FaceNormalMode::Both,
            _ => crate::world::FaceNormalMode::Back,
        };
        // Apply to primary selection
        let primary_face = if is_floor { SectorFace::Floor } else { SectorFace::Ceiling };
        apply_normal_mode_to_face(&mut state.level, room_idx, gx, gz, &primary_face, mode);
        // Apply to multi-selection (only matching face types: floors or ceilings)
        for sel in state.multi_selection.clone() {
            if let Selection::SectorFace { room, x, z, face } = sel {
                let is_matching = match (&face, is_floor) {
                    (SectorFace::Floor, true) | (SectorFace::Ceiling, false) => true,
                    _ => false,
                };
                if is_matching {
                    apply_normal_mode_to_face(&mut state.level, room, x, z, &face, mode);
                }
            }
        }
    }
    content_y += 28.0;

    // Black transparent toggle (PS1 CLUT-style transparency) - icon button
    draw_text("Black", content_x.floor(), (content_y + 12.0).floor(), 12.0, Color::from_rgba(150, 150, 150, 255));

    let btn_x = content_x + 40.0;
    let btn_size = 20.0;
    let btn_rect = Rect::new(btn_x, content_y, btn_size, btn_size);
    let icon_char = if face.black_transparent { icon::EYE_OFF } else { icon::EYE };
    let tooltip = if face.black_transparent { "Black = Transparent (click to make visible)" } else { "Black = Visible (click to make transparent)" };

    if crate::ui::icon_button(ctx, btn_rect, icon_char, icon_font, tooltip) {
        state.save_undo();
        let new_value = !face.black_transparent;
        // Apply to primary selection
        let primary_face = if is_floor { SectorFace::Floor } else { SectorFace::Ceiling };
        apply_black_transparent_to_face(&mut state.level, room_idx, gx, gz, &primary_face, new_value);
        // Apply to multi-selection (only matching face types: floors or ceilings)
        for sel in state.multi_selection.clone() {
            if let Selection::SectorFace { room, x, z, face } = sel {
                let is_matching = match (&face, is_floor) {
                    (SectorFace::Floor, true) | (SectorFace::Ceiling, false) => true,
                    _ => false,
                };
                if is_matching {
                    apply_black_transparent_to_face(&mut state.level, room, x, z, &face, new_value);
                }
            }
        }
    }

    // Show current state as text
    let state_text = if face.black_transparent { "Transparent" } else { "Visible" };
    draw_text(state_text, (btn_x + btn_size + 6.0).floor(), (content_y + 12.0).floor(), 11.0, Color::from_rgba(120, 120, 120, 255));

    // Extrude button (only for floors)
    if is_floor {
        content_y += 32.0;
        let extrude_btn_rect = Rect::new(content_x, content_y, 80.0, 24.0);

        // Draw button background
        let hovered = ctx.mouse.inside(&extrude_btn_rect);
        let bg_color = if hovered {
            Color::from_rgba(60, 80, 100, 255)
        } else {
            Color::from_rgba(40, 45, 55, 255)
        };
        draw_rectangle(extrude_btn_rect.x, extrude_btn_rect.y, extrude_btn_rect.w, extrude_btn_rect.h, bg_color);
        draw_rectangle_lines(extrude_btn_rect.x, extrude_btn_rect.y, extrude_btn_rect.w, extrude_btn_rect.h, 1.0,
            Color::from_rgba(80, 90, 100, 255));

        // Draw icon and label
        let icon_rect = Rect::new(content_x + 4.0, content_y + 2.0, 20.0, 20.0);
        crate::ui::draw_icon_centered(icon_font, icon::UNFOLD_VERTICAL, &icon_rect, 14.0, WHITE);
        draw_text("Extrude", (content_x + 26.0).floor(), (content_y + 16.0).floor(), 13.0, WHITE);

        // Handle click
        if hovered && ctx.mouse.left_pressed {
            state.save_undo();
            // Get wall texture from currently selected texture
            let wall_texture = state.selected_texture.clone();
            if let Some(r) = state.level.rooms.get_mut(room_idx) {
                if let Some(s) = r.get_sector_mut(gx, gz) {
                    // Extrude by 256 units (quarter sector)
                    if s.extrude_floor(256.0, wall_texture) {
                        state.set_status("Extruded floor by 256 units", 2.0);
                    }
                }
            }
            // Recalculate room bounds
            if let Some(r) = state.level.rooms.get_mut(room_idx) {
                r.recalculate_bounds();
            }
        }

        // Tooltip
        if hovered {
            ctx.tooltip = Some(crate::ui::PendingTooltip {
                text: String::from("Raise floor and create walls (256 units)"),
                x: ctx.mouse.x,
                y: ctx.mouse.y,
            });
        }
    }

    container_height
}

/// Helper: Flip UV coordinates horizontally
fn flip_uv_horizontal(uv: &mut Option<[crate::rasterizer::Vec2; 4]>) {
    use crate::rasterizer::Vec2;
    let current = uv.unwrap_or([
        Vec2::new(0.0, 0.0),
        Vec2::new(UV_SCALE, 0.0),
        Vec2::new(UV_SCALE, UV_SCALE),
        Vec2::new(0.0, UV_SCALE),
    ]);
    // Flip X: swap left and right (flip within the current scale)
    *uv = Some([
        Vec2::new(UV_SCALE - current[0].x, current[0].y),
        Vec2::new(UV_SCALE - current[1].x, current[1].y),
        Vec2::new(UV_SCALE - current[2].x, current[2].y),
        Vec2::new(UV_SCALE - current[3].x, current[3].y),
    ]);
}

/// Helper: Flip UV coordinates vertically
fn flip_uv_vertical(uv: &mut Option<[crate::rasterizer::Vec2; 4]>) {
    use crate::rasterizer::Vec2;
    let current = uv.unwrap_or([
        Vec2::new(0.0, 0.0),
        Vec2::new(UV_SCALE, 0.0),
        Vec2::new(UV_SCALE, UV_SCALE),
        Vec2::new(0.0, UV_SCALE),
    ]);
    // Flip Y: swap top and bottom (flip within the current scale)
    *uv = Some([
        Vec2::new(current[0].x, UV_SCALE - current[0].y),
        Vec2::new(current[1].x, UV_SCALE - current[1].y),
        Vec2::new(current[2].x, UV_SCALE - current[2].y),
        Vec2::new(current[3].x, UV_SCALE - current[3].y),
    ]);
}

/// Helper: Rotate UV coordinates 90° clockwise
/// This rotates the texture appearance by shifting which UV goes to which corner
fn rotate_uv_cw(uv: &mut Option<[crate::rasterizer::Vec2; 4]>) {
    use crate::rasterizer::Vec2;
    let current = uv.unwrap_or([
        Vec2::new(0.0, 0.0),           // corner 0: NW
        Vec2::new(UV_SCALE, 0.0),      // corner 1: NE
        Vec2::new(UV_SCALE, UV_SCALE), // corner 2: SE
        Vec2::new(0.0, UV_SCALE),      // corner 3: SW
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
/// Assumes default UV is [(0,0), (UV_SCALE,0), (UV_SCALE,UV_SCALE), (0,UV_SCALE)] for NW, NE, SE, SW
/// Scale is normalized so that 1.0 = default (one texture per block)
fn extract_uv_params(uv: &Option<[crate::rasterizer::Vec2; 4]>) -> UvParams {
    use crate::rasterizer::Vec2;
    let coords = uv.unwrap_or([
        Vec2::new(0.0, 0.0),
        Vec2::new(UV_SCALE, 0.0),
        Vec2::new(UV_SCALE, UV_SCALE),
        Vec2::new(0.0, UV_SCALE),
    ]);

    // Calculate center (average of all corners)
    let center_x = (coords[0].x + coords[1].x + coords[2].x + coords[3].x) / 4.0;
    let center_y = (coords[0].y + coords[1].y + coords[2].y + coords[3].y) / 4.0;

    // Offset is how much the center has moved from default (UV_SCALE/2, UV_SCALE/2)
    // Normalize by UV_SCALE so offset 1.0 = one block
    let x_offset = (center_x - UV_SCALE / 2.0) / UV_SCALE;
    let y_offset = (center_y - UV_SCALE / 2.0) / UV_SCALE;

    // Scale: measure the width and height of the UV quad
    // Width = distance from NW to NE (along X), Height = distance from NW to SW (along Y)
    let width = ((coords[1].x - coords[0].x).powi(2) + (coords[1].y - coords[0].y).powi(2)).sqrt();
    let height = ((coords[3].x - coords[0].x).powi(2) + (coords[3].y - coords[0].y).powi(2)).sqrt();

    // Angle: angle of the NW->NE edge from horizontal
    let dx = coords[1].x - coords[0].x;
    let dy = coords[1].y - coords[0].y;
    let angle = dy.atan2(dx).to_degrees();

    // Normalize scale by UV_SCALE so 1.0 = default (one texture per block)
    UvParams {
        x_offset,
        y_offset,
        x_scale: width / UV_SCALE,
        y_scale: height / UV_SCALE,
        angle,
    }
}

/// Apply UV parameters to generate 4-corner UV coordinates
/// Scale 1.0 = default (one texture per block), which maps to UV_SCALE in UV space
fn apply_uv_params(params: &UvParams) -> [crate::rasterizer::Vec2; 4] {
    use crate::rasterizer::Vec2;

    // Convert normalized scale to actual UV space (multiply by UV_SCALE)
    let actual_w = params.x_scale * UV_SCALE;
    let actual_h = params.y_scale * UV_SCALE;
    let half_w = actual_w / 2.0;
    let half_h = actual_h / 2.0;

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

    // Translate to final position (center at UV_SCALE/2 + offset * UV_SCALE)
    let center_x = UV_SCALE / 2.0 + params.x_offset * UV_SCALE;
    let center_y = UV_SCALE / 2.0 + params.y_offset * UV_SCALE;

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
    use crate::ui::{draw_drag_value_compact_editable, icon_button, Rect, icon};

    let mut params = extract_uv_params(uv);
    let mut changed = false;
    let row_height = 20.0;
    let label_color = Color::from_rgba(150, 150, 150, 255);

    let mut current_y = y;

    // UV Offset in pixels (0-63, wraps at 64)
    // Conversion: offset_blocks * 32 = pixels (since 1 block = 32 texels)
    // Full texture = 64 pixels = 2 blocks
    let x_pixels = ((params.x_offset * 32.0).round() as i32).rem_euclid(64);
    let y_pixels = ((params.y_offset * 32.0).round() as i32).rem_euclid(64);

    // Row 1: X offset with pixel buttons
    // Layout: X: [◄◄] [◄]  value  [►] [►►]
    draw_text("X:", x, current_y + 12.0, 11.0, label_color);

    let btn_size = 16.0;
    let btn_spacing = 2.0;
    let value_width = 28.0;
    let btn_start = x + 18.0;

    // Coarse left (−32 pixels)
    let coarse_left_rect = Rect::new(btn_start, current_y + 1.0, btn_size, btn_size);
    if icon_button(ctx, coarse_left_rect, icon::SKIP_BACK, icon_font, "−32 pixels") {
        params.x_offset -= 1.0; // 1 block = 32 pixels
        changed = true;
    }

    // Fine left (−1 pixel)
    let fine_left_rect = Rect::new(btn_start + btn_size + btn_spacing, current_y + 1.0, btn_size, btn_size);
    if icon_button(ctx, fine_left_rect, icon::CHEVRON_LEFT, icon_font, "−1 pixel") {
        params.x_offset -= 1.0 / 32.0;
        changed = true;
    }

    // Value display (centered)
    let value_x = btn_start + (btn_size + btn_spacing) * 2.0;
    let value_rect = Rect::new(value_x, current_y, value_width, row_height);
    draw_rectangle(value_rect.x, value_rect.y, value_rect.w, value_rect.h, Color::from_rgba(40, 40, 45, 255));
    let value_text = format!("{}", x_pixels);
    let text_dims = measure_text(&value_text, None, 11, 1.0);
    draw_text(&value_text, value_rect.x + (value_rect.w - text_dims.width) / 2.0, current_y + 12.0, 11.0, WHITE);

    // Fine right (+1 pixel)
    let fine_right_rect = Rect::new(value_x + value_width + btn_spacing, current_y + 1.0, btn_size, btn_size);
    if icon_button(ctx, fine_right_rect, icon::CHEVRON_RIGHT, icon_font, "+1 pixel") {
        params.x_offset += 1.0 / 32.0;
        changed = true;
    }

    // Coarse right (+32 pixels)
    let coarse_right_rect = Rect::new(value_x + value_width + btn_spacing + btn_size + btn_spacing, current_y + 1.0, btn_size, btn_size);
    if icon_button(ctx, coarse_right_rect, icon::SKIP_FORWARD, icon_font, "+32 pixels") {
        params.x_offset += 1.0; // 1 block = 32 pixels
        changed = true;
    }

    current_y += row_height;

    // Row 2: Y offset with pixel buttons
    draw_text("Y:", x, current_y + 12.0, 11.0, label_color);

    // Coarse left (−32 pixels)
    let coarse_left_rect = Rect::new(btn_start, current_y + 1.0, btn_size, btn_size);
    if icon_button(ctx, coarse_left_rect, icon::SKIP_BACK, icon_font, "−32 pixels") {
        params.y_offset -= 1.0;
        changed = true;
    }

    // Fine left (−1 pixel)
    let fine_left_rect = Rect::new(btn_start + btn_size + btn_spacing, current_y + 1.0, btn_size, btn_size);
    if icon_button(ctx, fine_left_rect, icon::CHEVRON_LEFT, icon_font, "−1 pixel") {
        params.y_offset -= 1.0 / 32.0;
        changed = true;
    }

    // Value display (centered)
    let value_rect = Rect::new(value_x, current_y, value_width, row_height);
    draw_rectangle(value_rect.x, value_rect.y, value_rect.w, value_rect.h, Color::from_rgba(40, 40, 45, 255));
    let value_text = format!("{}", y_pixels);
    let text_dims = measure_text(&value_text, None, 11, 1.0);
    draw_text(&value_text, value_rect.x + (value_rect.w - text_dims.width) / 2.0, current_y + 12.0, 11.0, WHITE);

    // Fine right (+1 pixel)
    let fine_right_rect = Rect::new(value_x + value_width + btn_spacing, current_y + 1.0, btn_size, btn_size);
    if icon_button(ctx, fine_right_rect, icon::CHEVRON_RIGHT, icon_font, "+1 pixel") {
        params.y_offset += 1.0 / 32.0;
        changed = true;
    }

    // Coarse right (+32 pixels)
    let coarse_right_rect = Rect::new(value_x + value_width + btn_spacing + btn_size + btn_spacing, current_y + 1.0, btn_size, btn_size);
    if icon_button(ctx, coarse_right_rect, icon::SKIP_FORWARD, icon_font, "+32 pixels") {
        params.y_offset += 1.0;
        changed = true;
    }

    current_y += row_height;

    // Row 3: Scale - [Link] Label [X] [Y]
    let link_btn_size = 16.0;
    let label_width = 42.0;
    let scale_value_width = (width - label_width - link_btn_size - 12.0) / 2.0;
    let scale_value_start = x + link_btn_size + 4.0 + label_width;

    let link_rect = Rect::new(x, current_y + 1.0, link_btn_size, link_btn_size);
    let link_icon = if state.uv_scale_linked { icon::LINK } else { icon::LINK_OFF };
    if crate::ui::icon_button_active(ctx, link_rect, link_icon, icon_font, "Link X/Y", state.uv_scale_linked) {
        state.uv_scale_linked = !state.uv_scale_linked;
    }

    draw_text("Scale", x + link_btn_size + 4.0, current_y + 12.0, 11.0, label_color);
    let sx_rect = Rect::new(scale_value_start, current_y, scale_value_width - 2.0, row_height);
    let result = draw_drag_value_compact_editable(
        ctx, sx_rect, params.x_scale, 0.25, 2003,
        &mut state.uv_drag_active[2], &mut state.uv_drag_start_value[2], &mut state.uv_drag_start_x[2],
        Some(&mut state.uv_editing_field), Some((&mut state.uv_edit_buffer, 2)),
    );
    if let Some(v) = result.value {
        let old_scale = params.x_scale;
        // Snap to 0.25 increments to match level geometry
        params.x_scale = (v * 4.0).round() / 4.0;
        params.x_scale = params.x_scale.max(0.25); // Minimum 0.25 scale
        if state.uv_scale_linked && old_scale > 0.001 {
            let ratio = params.x_scale / old_scale;
            params.y_scale = ((params.y_scale * ratio) * 4.0).round() / 4.0;
            params.y_scale = params.y_scale.max(0.25);
        }
        changed = true;
    }
    let sy_rect = Rect::new(scale_value_start + scale_value_width, current_y, scale_value_width - 2.0, row_height);
    let result = draw_drag_value_compact_editable(
        ctx, sy_rect, params.y_scale, 0.25, 2004,
        &mut state.uv_drag_active[3], &mut state.uv_drag_start_value[3], &mut state.uv_drag_start_x[3],
        Some(&mut state.uv_editing_field), Some((&mut state.uv_edit_buffer, 3)),
    );
    if let Some(v) = result.value {
        let old_scale = params.y_scale;
        // Snap to 0.25 increments to match level geometry
        params.y_scale = (v * 4.0).round() / 4.0;
        params.y_scale = params.y_scale.max(0.25); // Minimum 0.25 scale
        if state.uv_scale_linked && old_scale > 0.001 {
            let ratio = params.y_scale / old_scale;
            params.x_scale = ((params.x_scale * ratio) * 4.0).round() / 4.0;
            params.x_scale = params.x_scale.max(0.25);
        }
        changed = true;
    }
    current_y += row_height;

    // Row 4: Angle (no link button, full width)
    draw_text("Angle", x + link_btn_size + 4.0, current_y + 12.0, 11.0, label_color);
    let angle_rect = Rect::new(scale_value_start, current_y, width - scale_value_start + x - 4.0, row_height);
    let result = draw_drag_value_compact_editable(
        ctx, angle_rect, params.angle, 1.0, 2005,
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
    wall_face: super::SectorFace,
    state: &mut EditorState,
    icon_font: Option<&Font>,
) -> f32 {
    // Helper to get mutable wall reference by SectorFace
    fn get_wall_mut<'a>(sector: &'a mut crate::world::Sector, face: &super::SectorFace) -> Option<&'a mut crate::world::VerticalFace> {
        match face {
            super::SectorFace::WallNorth(i) => sector.walls_north.get_mut(*i),
            super::SectorFace::WallEast(i) => sector.walls_east.get_mut(*i),
            super::SectorFace::WallSouth(i) => sector.walls_south.get_mut(*i),
            super::SectorFace::WallWest(i) => sector.walls_west.get_mut(*i),
            super::SectorFace::WallNwSe(i) => sector.walls_nwse.get_mut(*i),
            super::SectorFace::WallNeSw(i) => sector.walls_nesw.get_mut(*i),
            _ => None,
        }
    }

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
        format!("Texture: {}/{}", wall.texture.pack, wall.texture.name)
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

    // UV coordinates display (scaled by UV_SCALE)
    let uv = wall.uv.unwrap_or([
        crate::rasterizer::Vec2::new(0.0, UV_SCALE),       // bottom-left
        crate::rasterizer::Vec2::new(UV_SCALE, UV_SCALE),  // bottom-right
        crate::rasterizer::Vec2::new(UV_SCALE, 0.0),       // top-right
        crate::rasterizer::Vec2::new(0.0, 0.0),            // top-left
    ]);
    draw_text(&format!("UV: [{:.2},{:.2}] [{:.2},{:.2}]", uv[0].x, uv[0].y, uv[1].x, uv[1].y),
        content_x.floor(), (content_y + 12.0).floor(), 11.0, Color::from_rgba(120, 120, 120, 255));
    content_y += line_height;

    // UV parameter editing controls
    let controls_width = width - CONTAINER_PADDING * 2.0;
    if let Some(new_uv) = draw_uv_controls(ctx, content_x, content_y, controls_width, &wall.uv, state, icon_font) {
        state.save_undo();
        if let Some(r) = state.level.rooms.get_mut(room_idx) {
            if let Some(s) = r.get_sector_mut(gx, gz) {
                if let Some(w) = get_wall_mut(s, &wall_face) {
                    w.uv = Some(new_uv);
                }
            }
        }
    }
    content_y += 80.0; // Height of UV controls (4 rows × 20px: X offset, Y offset, Scale, Angle)

    // UV manipulation buttons
    let btn_size = 20.0;
    let btn_spacing = 4.0;
    let mut btn_x = content_x;

    // Collect all wall selections (primary + multi-selection) for UV operations
    // Returns (room, x, z, face) tuples where face is the full SectorFace enum
    let collect_wall_selections = |state: &EditorState| -> Vec<(usize, usize, usize, super::SectorFace)> {
        let mut walls = Vec::new();
        let mut all_selections: Vec<super::Selection> = vec![state.selection.clone()];
        all_selections.extend(state.multi_selection.clone());

        for sel in all_selections {
            if let super::Selection::SectorFace { room, x, z, face } = sel {
                match face {
                    super::SectorFace::WallNorth(_) |
                    super::SectorFace::WallEast(_) |
                    super::SectorFace::WallSouth(_) |
                    super::SectorFace::WallWest(_) |
                    super::SectorFace::WallNwSe(_) |
                    super::SectorFace::WallNeSw(_) => walls.push((room, x, z, face)),
                    _ => {} // Skip floor/ceiling for wall UV operations
                }
            }
        }
        walls
    };

    // Reset UV button
    let reset_rect = Rect::new(btn_x, content_y, btn_size, btn_size);
    if crate::ui::icon_button(ctx, reset_rect, icon::REFRESH_CW, icon_font, "Reset UV") {
        let walls = collect_wall_selections(state);
        if !walls.is_empty() {
            state.save_undo();
            for (room_idx, gx, gz, face) in walls {
                if let Some(r) = state.level.rooms.get_mut(room_idx) {
                    if let Some(s) = r.get_sector_mut(gx, gz) {
                        if let Some(w) = get_wall_mut(s, &face) {
                            w.uv = None;
                        }
                    }
                }
            }
        }
    }
    btn_x += btn_size + btn_spacing;

    // Flip Horizontal button
    let flip_h_rect = Rect::new(btn_x, content_y, btn_size, btn_size);
    if crate::ui::icon_button(ctx, flip_h_rect, icon::FLIP_HORIZONTAL, icon_font, "Flip UV Horizontal") {
        let walls = collect_wall_selections(state);
        if !walls.is_empty() {
            state.save_undo();
            for (room_idx, gx, gz, face) in walls {
                if let Some(r) = state.level.rooms.get_mut(room_idx) {
                    if let Some(s) = r.get_sector_mut(gx, gz) {
                        if let Some(w) = get_wall_mut(s, &face) {
                            flip_uv_horizontal(&mut w.uv);
                        }
                    }
                }
            }
        }
    }
    btn_x += btn_size + btn_spacing;

    // Flip Vertical button
    let flip_v_rect = Rect::new(btn_x, content_y, btn_size, btn_size);
    if crate::ui::icon_button(ctx, flip_v_rect, icon::FLIP_VERTICAL, icon_font, "Flip UV Vertical") {
        let walls = collect_wall_selections(state);
        if !walls.is_empty() {
            state.save_undo();
            for (room_idx, gx, gz, face) in walls {
                if let Some(r) = state.level.rooms.get_mut(room_idx) {
                    if let Some(s) = r.get_sector_mut(gx, gz) {
                        if let Some(w) = get_wall_mut(s, &face) {
                            flip_uv_vertical(&mut w.uv);
                        }
                    }
                }
            }
        }
    }
    btn_x += btn_size + btn_spacing;

    // Rotate 90° CW button
    let rotate_rect = Rect::new(btn_x, content_y, btn_size, btn_size);
    if crate::ui::icon_button(ctx, rotate_rect, icon::ROTATE_CW, icon_font, "Rotate UV 90° CW") {
        let walls = collect_wall_selections(state);
        if !walls.is_empty() {
            state.save_undo();
            for (room_idx, gx, gz, face) in walls {
                if let Some(r) = state.level.rooms.get_mut(room_idx) {
                    if let Some(s) = r.get_sector_mut(gx, gz) {
                        if let Some(w) = get_wall_mut(s, &face) {
                            rotate_uv_cw(&mut w.uv);
                        }
                    }
                }
            }
        }
    }
    btn_x += btn_size + btn_spacing;

    // 1:1 Texel mapping button - resets H scale to 1.0 and sets V scale to match wall height
    let texel_rect = Rect::new(btn_x, content_y, btn_size, btn_size);
    if crate::ui::icon_button(ctx, texel_rect, icon::RATIO, icon_font, "1:1 Texel Mapping") {
        let walls = collect_wall_selections(state);
        if !walls.is_empty() {
            state.save_undo();
            for (room_idx, gx, gz, face) in walls {
                if let Some(r) = state.level.rooms.get_mut(room_idx) {
                    if let Some(s) = r.get_sector_mut(gx, gz) {
                        if let Some(w) = get_wall_mut(s, &face) {
                            // Calculate V scale based on wall height relative to SECTOR_SIZE
                            // Scale is normalized: 1.0 = one block width
                            let wall_height = w.height();
                            let v_scale = wall_height / crate::world::SECTOR_SIZE;
                            let mut params = extract_uv_params(&w.uv);
                            params.x_scale = 1.0; // Wall width = 1 block
                            params.y_scale = v_scale;
                            w.uv = Some(apply_uv_params(&params));
                        }
                    }
                }
            }
        }
    }
    btn_x += btn_size + btn_spacing;

    // UV Projection toggle - makes texture appear flat on sloped walls
    let is_projected = wall.uv_projection == crate::world::UvProjection::Projected;
    let proj_icon = if is_projected { icon::LAYERS } else { icon::SCAN };
    let proj_tooltip = if is_projected { "UV Projection: ON (click to disable)" } else { "UV Projection: OFF (click for uniform texture on slopes)" };
    let proj_rect = Rect::new(btn_x, content_y, btn_size, btn_size);
    if crate::ui::icon_button(ctx, proj_rect, proj_icon, icon_font, proj_tooltip) {
        let walls = collect_wall_selections(state);
        if !walls.is_empty() {
            state.save_undo();
            let new_projection = if is_projected {
                crate::world::UvProjection::Default
            } else {
                crate::world::UvProjection::Projected
            };
            for (room_idx, gx, gz, face) in walls {
                if let Some(r) = state.level.rooms.get_mut(room_idx) {
                    if let Some(s) = r.get_sector_mut(gx, gz) {
                        if let Some(w) = get_wall_mut(s, &face) {
                            w.uv_projection = new_projection;
                        }
                    }
                }
            }
        }
    }
    content_y += btn_size + 4.0;

    // Wall vertex colors (PS1-style texture modulation)
    // Layout: 2x2 vertex swatches on left, color picker on right
    let swatch_size = 18.0;
    let swatch_spacing = 2.0;
    let swatches_width = 2.0 * swatch_size + swatch_spacing; // Width of 2x2 grid
    let picker_offset = swatches_width + 8.0; // Gap between swatches and picker

    // Default to all vertices selected if none are selected
    if state.selected_vertex_indices.is_empty() {
        state.selected_vertex_indices = vec![0, 1, 2, 3];
    }

    // Label
    draw_text("Vertex Colour", content_x.floor(), (content_y + 12.0).floor(), 12.0,
        macroquad::color::Color::from_rgba(150, 150, 150, 255));
    content_y += 16.0;

    let section_start_y = content_y; // Remember where this section starts

    // Draw 4 vertex color swatches in 2x2 grid (TL, TR / BL, BR layout - visual matches wall)
    let grid_x = content_x;
    let vertex_labels = ["TL", "TR", "BL", "BR"];
    let grid_positions = [(0, 0), (1, 0), (0, 1), (1, 1)]; // (col, row)
    let vertex_indices = [3, 2, 0, 1]; // Map grid to corner indices: BL=0, BR=1, TR=2, TL=3

    for (grid_idx, &(col, row)) in grid_positions.iter().enumerate() {
        let vert_idx = vertex_indices[grid_idx];
        let vert_color = wall.colors[vert_idx];
        let sx = grid_x + (col as f32) * (swatch_size + swatch_spacing);
        let sy = section_start_y + (row as f32) * (swatch_size + swatch_spacing);
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

        // Handle click - toggle selection of this vertex (but don't allow deselecting the last one)
        if hovered && ctx.mouse.left_pressed {
            if is_selected {
                // Only deselect if there's more than one selected
                if state.selected_vertex_indices.len() > 1 {
                    state.selected_vertex_indices.retain(|&v| v != vert_idx);
                }
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

    // Vertical separator between swatches and picker
    let separator_x = content_x + swatches_width + 4.0;
    let swatches_height = 2.0 * swatch_size + swatch_spacing;
    draw_line(separator_x, section_start_y, separator_x, section_start_y + swatches_height, 1.0,
        macroquad::color::Color::from_rgba(60, 60, 65, 255));

    // PS1 color picker to the right of vertex swatches
    let picker_x = content_x + picker_offset;
    let picker_width = width - CONTAINER_PADDING * 2.0 - picker_offset;

    // Get current color to display in picker (use first selected vertex)
    let display_color = {
        let idx = state.selected_vertex_indices[0].min(3);
        wall.colors[idx]
    };

    let picker_result = draw_ps1_color_picker(
        ctx,
        picker_x,
        section_start_y,
        picker_width,
        display_color,
        RasterColor::from_ps1(16, 16, 16),
        "",
        &mut state.vertex_color_slider,
    );

    if let Some(new_color) = picker_result.color {
        state.save_undo();
        let vertex_indices = state.selected_vertex_indices.clone();
        // Apply to primary selection
        apply_vertex_colors_to_face(&mut state.level, room_idx, gx, gz, &wall_face, &vertex_indices, new_color);
        // Apply to multi-selection (only wall faces)
        for sel in state.multi_selection.clone() {
            if let Selection::SectorFace { room, x, z, face } = sel {
                // Check if it's a wall face (any type)
                let is_wall = matches!(face,
                    SectorFace::WallNorth(_) | SectorFace::WallEast(_) |
                    SectorFace::WallSouth(_) | SectorFace::WallWest(_) |
                    SectorFace::WallNwSe(_) | SectorFace::WallNeSw(_)
                );
                if is_wall {
                    apply_vertex_colors_to_face(&mut state.level, room, x, z, &face, &vertex_indices, new_color);
                }
            }
        }
    }

    // Advance content_y by the taller of: swatches (2 rows) or picker
    let swatches_height = 2.0 * swatch_size + swatch_spacing;
    content_y += swatches_height.max(ps1_color_picker_height()) + 8.0;

    // Normal mode 3-way toggle
    draw_text("Normal", content_x.floor(), (content_y + 12.0).floor(), 12.0, Color::from_rgba(150, 150, 150, 255));
    content_y += 16.0;

    let toggle_rect = Rect::new(content_x, content_y, width - CONTAINER_PADDING * 2.0, 24.0);
    let current_mode = match wall.normal_mode {
        crate::world::FaceNormalMode::Front => 0,
        crate::world::FaceNormalMode::Both => 1,
        crate::world::FaceNormalMode::Back => 2,
    };
    if let Some(new_mode) = crate::ui::draw_three_way_toggle(ctx, toggle_rect, ["Front", "Both", "Back"], current_mode) {
        state.save_undo();
        let mode = match new_mode {
            0 => crate::world::FaceNormalMode::Front,
            1 => crate::world::FaceNormalMode::Both,
            _ => crate::world::FaceNormalMode::Back,
        };
        // Apply to primary selection
        apply_normal_mode_to_face(&mut state.level, room_idx, gx, gz, &wall_face, mode);
        // Apply to multi-selection (only wall faces)
        for sel in state.multi_selection.clone() {
            if let Selection::SectorFace { room, x, z, face } = sel {
                // Check if it's a wall face (any type)
                let is_wall = matches!(face,
                    SectorFace::WallNorth(_) | SectorFace::WallEast(_) |
                    SectorFace::WallSouth(_) | SectorFace::WallWest(_) |
                    SectorFace::WallNwSe(_) | SectorFace::WallNeSw(_)
                );
                if is_wall {
                    apply_normal_mode_to_face(&mut state.level, room, x, z, &face, mode);
                }
            }
        }
    }
    content_y += 28.0;

    // Black transparent toggle (PS1 CLUT-style transparency) - icon button
    draw_text("Black", content_x.floor(), (content_y + 12.0).floor(), 12.0, Color::from_rgba(150, 150, 150, 255));

    let btn_x = content_x + 40.0;
    let btn_size = 20.0;
    let btn_rect = Rect::new(btn_x, content_y, btn_size, btn_size);
    let icon_char = if wall.black_transparent { icon::EYE_OFF } else { icon::EYE };
    let tooltip = if wall.black_transparent { "Black = Transparent (click to make visible)" } else { "Black = Visible (click to make transparent)" };

    if crate::ui::icon_button(ctx, btn_rect, icon_char, icon_font, tooltip) {
        state.save_undo();
        let new_value = !wall.black_transparent;
        // Apply to primary selection
        apply_black_transparent_to_face(&mut state.level, room_idx, gx, gz, &wall_face, new_value);
        // Apply to multi-selection (only wall faces)
        for sel in state.multi_selection.clone() {
            if let Selection::SectorFace { room, x, z, face } = sel {
                // Check if it's a wall face (any type)
                let is_wall = matches!(face,
                    SectorFace::WallNorth(_) | SectorFace::WallEast(_) |
                    SectorFace::WallSouth(_) | SectorFace::WallWest(_) |
                    SectorFace::WallNwSe(_) | SectorFace::WallNeSw(_)
                );
                if is_wall {
                    apply_black_transparent_to_face(&mut state.level, room, x, z, &face, new_value);
                }
            }
        }
    }

    // Show current state as text
    let state_text = if wall.black_transparent { "Transparent" } else { "Visible" };
    draw_text(state_text, (btn_x + btn_size + 6.0).floor(), (content_y + 12.0).floor(), 11.0, Color::from_rgba(120, 120, 120, 255));

    container_height
}

fn draw_properties(ctx: &mut UiContext, rect: Rect, state: &mut EditorState, icon_font: Option<&Font>) {
    let x = rect.x.floor();
    let container_width = rect.w - 4.0;

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
            draw_text("Nothing selected", x, (y + 10.0).floor(), FONT_SIZE_CONTENT, Color::from_rgba(150, 150, 150, 255));
        }
        super::Selection::Room(idx) => {
            draw_text(&format!("Room {}", idx), x, (y + 10.0).floor(), FONT_SIZE_HEADER, WHITE);
        }
        super::Selection::SectorFace { room, x: gx, z: gz, face } => {
            // Single face selected (from 3D view click)
            draw_text(&format!("Sector ({}, {})", gx, gz), x, (y + 10.0).floor(), FONT_SIZE_HEADER, Color::from_rgba(150, 150, 150, 255));
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
                            draw_text("(no floor)", x, (y + 10.0).floor(), FONT_SIZE_CONTENT, Color::from_rgba(100, 100, 100, 255));
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
                            draw_text("(no ceiling)", x, (y + 10.0).floor(), FONT_SIZE_CONTENT, Color::from_rgba(100, 100, 100, 255));
                        }
                    }
                    super::SectorFace::WallNorth(i) => {
                        if let Some(wall) = sector.walls_north.get(*i) {
                            let h = draw_wall_face_container(
                                ctx, x, y, container_width, wall, "Wall (North)",
                                Color::from_rgba(255, 180, 120, 255),
                                *room, *gx, *gz, super::SectorFace::WallNorth(*i), state, icon_font
                            );
                            let _ = h + CONTAINER_MARGIN;
                        }
                    }
                    super::SectorFace::WallEast(i) => {
                        if let Some(wall) = sector.walls_east.get(*i) {
                            let h = draw_wall_face_container(
                                ctx, x, y, container_width, wall, "Wall (East)",
                                Color::from_rgba(255, 180, 120, 255),
                                *room, *gx, *gz, super::SectorFace::WallEast(*i), state, icon_font
                            );
                            let _ = h + CONTAINER_MARGIN;
                        }
                    }
                    super::SectorFace::WallSouth(i) => {
                        if let Some(wall) = sector.walls_south.get(*i) {
                            let h = draw_wall_face_container(
                                ctx, x, y, container_width, wall, "Wall (South)",
                                Color::from_rgba(255, 180, 120, 255),
                                *room, *gx, *gz, super::SectorFace::WallSouth(*i), state, icon_font
                            );
                            let _ = h + CONTAINER_MARGIN;
                        }
                    }
                    super::SectorFace::WallWest(i) => {
                        if let Some(wall) = sector.walls_west.get(*i) {
                            let h = draw_wall_face_container(
                                ctx, x, y, container_width, wall, "Wall (West)",
                                Color::from_rgba(255, 180, 120, 255),
                                *room, *gx, *gz, super::SectorFace::WallWest(*i), state, icon_font
                            );
                            let _ = h + CONTAINER_MARGIN;
                        }
                    }
                    super::SectorFace::WallNwSe(i) => {
                        if let Some(wall) = sector.walls_nwse.get(*i) {
                            let h = draw_wall_face_container(
                                ctx, x, y, container_width, wall, "Wall (NW-SE)",
                                Color::from_rgba(255, 200, 150, 255),
                                *room, *gx, *gz, super::SectorFace::WallNwSe(*i), state, icon_font
                            );
                            let _ = h + CONTAINER_MARGIN;
                        }
                    }
                    super::SectorFace::WallNeSw(i) => {
                        if let Some(wall) = sector.walls_nesw.get(*i) {
                            let h = draw_wall_face_container(
                                ctx, x, y, container_width, wall, "Wall (NE-SW)",
                                Color::from_rgba(255, 200, 150, 255),
                                *room, *gx, *gz, super::SectorFace::WallNeSw(*i), state, icon_font
                            );
                            let _ = h + CONTAINER_MARGIN;
                        }
                    }
                }
            } else {
                draw_text("Sector not found", x, (y + 14.0).floor(), 14.0, Color::from_rgba(255, 100, 100, 255));
            }
        }
        super::Selection::Vertex { room, x: gx, z: gz, face, corner_idx } => {
            // Single vertex selected - show face properties with this vertex highlighted
            draw_text(&format!("Vertex {} of Sector ({}, {})", corner_idx, gx, gz), x, (y + 14.0).floor(), 14.0, Color::from_rgba(150, 150, 150, 255));
            y += 24.0;

            // Get sector data
            let sector_data = state.level.rooms.get(*room)
                .and_then(|r| r.get_sector(*gx, *gz))
                .cloned();

            if let Some(sector) = sector_data {
                // Show the face this vertex belongs to
                match face {
                    super::SectorFace::Floor => {
                        if let Some(floor) = &sector.floor {
                            let h = draw_horizontal_face_container(
                                ctx, x, y, container_width, floor, "Floor",
                                Color::from_rgba(150, 200, 255, 255),
                                *room, *gx, *gz, true, state, icon_font
                            );
                            let _ = h + CONTAINER_MARGIN;
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
                        }
                    }
                    super::SectorFace::WallNorth(i) => {
                        if let Some(wall) = sector.walls_north.get(*i) {
                            let _ = draw_wall_face_container(
                                ctx, x, y, container_width, wall, "Wall (North)",
                                Color::from_rgba(255, 180, 120, 255),
                                *room, *gx, *gz, super::SectorFace::WallNorth(*i), state, icon_font
                            );
                        }
                    }
                    super::SectorFace::WallEast(i) => {
                        if let Some(wall) = sector.walls_east.get(*i) {
                            let _ = draw_wall_face_container(
                                ctx, x, y, container_width, wall, "Wall (East)",
                                Color::from_rgba(255, 180, 120, 255),
                                *room, *gx, *gz, super::SectorFace::WallEast(*i), state, icon_font
                            );
                        }
                    }
                    super::SectorFace::WallSouth(i) => {
                        if let Some(wall) = sector.walls_south.get(*i) {
                            let _ = draw_wall_face_container(
                                ctx, x, y, container_width, wall, "Wall (South)",
                                Color::from_rgba(255, 180, 120, 255),
                                *room, *gx, *gz, super::SectorFace::WallSouth(*i), state, icon_font
                            );
                        }
                    }
                    super::SectorFace::WallWest(i) => {
                        if let Some(wall) = sector.walls_west.get(*i) {
                            let _ = draw_wall_face_container(
                                ctx, x, y, container_width, wall, "Wall (West)",
                                Color::from_rgba(255, 180, 120, 255),
                                *room, *gx, *gz, super::SectorFace::WallWest(*i), state, icon_font
                            );
                        }
                    }
                    super::SectorFace::WallNwSe(i) => {
                        if let Some(wall) = sector.walls_nwse.get(*i) {
                            let _ = draw_wall_face_container(
                                ctx, x, y, container_width, wall, "Wall (NW-SE)",
                                Color::from_rgba(255, 200, 150, 255),
                                *room, *gx, *gz, super::SectorFace::WallNwSe(*i), state, icon_font
                            );
                        }
                    }
                    super::SectorFace::WallNeSw(i) => {
                        if let Some(wall) = sector.walls_nesw.get(*i) {
                            let _ = draw_wall_face_container(
                                ctx, x, y, container_width, wall, "Wall (NE-SW)",
                                Color::from_rgba(255, 200, 150, 255),
                                *room, *gx, *gz, super::SectorFace::WallNeSw(*i), state, icon_font
                            );
                        }
                    }
                }
            } else {
                draw_text("Sector not found", x, (y + 10.0).floor(), FONT_SIZE_CONTENT, Color::from_rgba(255, 100, 100, 255));
            }
        }
        super::Selection::Sector { room, x: gx, z: gz } => {
            // Whole sector selected (from 2D view click) - show all faces in containers
            draw_text(&format!("Sector ({}, {})", gx, gz), x, (y + 10.0).floor(), FONT_SIZE_HEADER, Color::from_rgba(255, 200, 80, 255));
            y += 20.0;

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
                // Cardinal walls
                let wall_dirs: [(&str, &Vec<crate::world::VerticalFace>, fn(usize) -> super::SectorFace); 4] = [
                    ("North", &sector.walls_north, |i| super::SectorFace::WallNorth(i)),
                    ("East", &sector.walls_east, |i| super::SectorFace::WallEast(i)),
                    ("South", &sector.walls_south, |i| super::SectorFace::WallSouth(i)),
                    ("West", &sector.walls_west, |i| super::SectorFace::WallWest(i)),
                ];

                for (dir_name, walls, make_face) in wall_dirs {
                    for (i, wall) in walls.iter().enumerate() {
                        let label = if walls.len() == 1 {
                            format!("Wall ({})", dir_name)
                        } else {
                            format!("Wall ({}) [{}]", dir_name, i)
                        };
                        let h = draw_wall_face_container(
                            ctx, x, y, container_width, wall, &label,
                            Color::from_rgba(255, 180, 120, 255),
                            *room, *gx, *gz, make_face(i), state, icon_font
                        );
                        y += h + CONTAINER_MARGIN;
                    }
                }

                // Diagonal walls (NW-SE)
                for (i, wall) in sector.walls_nwse.iter().enumerate() {
                    let label = if sector.walls_nwse.len() == 1 {
                        "Wall (NW-SE)".to_string()
                    } else {
                        format!("Wall (NW-SE) [{}]", i)
                    };
                    let h = draw_wall_face_container(
                        ctx, x, y, container_width, wall, &label,
                        Color::from_rgba(255, 200, 150, 255),
                        *room, *gx, *gz, super::SectorFace::WallNwSe(i), state, icon_font
                    );
                    y += h + CONTAINER_MARGIN;
                }

                // Diagonal walls (NE-SW)
                for (i, wall) in sector.walls_nesw.iter().enumerate() {
                    let label = if sector.walls_nesw.len() == 1 {
                        "Wall (NE-SW)".to_string()
                    } else {
                        format!("Wall (NE-SW) [{}]", i)
                    };
                    let h = draw_wall_face_container(
                        ctx, x, y, container_width, wall, &label,
                        Color::from_rgba(255, 200, 150, 255),
                        *room, *gx, *gz, super::SectorFace::WallNeSw(i), state, icon_font
                    );
                    y += h + CONTAINER_MARGIN;
                }
            } else {
                draw_text("Sector not found", x, (y + 10.0).floor(), FONT_SIZE_CONTENT, Color::from_rgba(255, 100, 100, 255));
            }
        }
        super::Selection::Portal { room, portal } => {
            draw_text(&format!("Portal {} in Room {}", portal, room), x, (y + 10.0).floor(), FONT_SIZE_HEADER, WHITE);
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
                    super::SectorFace::WallNwSe(_) => "Wall NW-SE".to_string(),
                    super::SectorFace::WallNeSw(_) => "Wall NE-SW".to_string(),
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
            draw_text(&format!("{} Edge ({})", face_name, edge_name), x, (y + 10.0).floor(), FONT_SIZE_HEADER, WHITE);
            y += 20.0;

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
                            super::SectorFace::WallNwSe(i) => sector.walls_nwse.get(*i).map(|w| w.heights),
                            super::SectorFace::WallNeSw(i) => sector.walls_nesw.get(*i).map(|w| w.heights),
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
        super::Selection::Object { room: room_idx, index } => {
            // Object properties (asset-based)
            let obj_room_idx = *room_idx;
            let obj_idx = *index;

            let obj_opt = state.level.rooms.get(obj_room_idx)
                .and_then(|room| room.objects.get(obj_idx))
                .cloned();

            if let Some(obj) = obj_opt {
                // Get asset name from library
                let asset_name = state.asset_library.get_name_by_id(obj.asset_id)
                    .unwrap_or("Unknown");
                let asset = state.asset_library.get_by_id(obj.asset_id);

                // Header with asset name
                draw_text(asset_name, x, (y + 10.0).floor(), FONT_SIZE_HEADER, WHITE);
                y += 20.0;

                // Location
                draw_text("Location:", x, (y + 10.0).floor(), FONT_SIZE_HEADER, Color::from_rgba(150, 150, 150, 255));
                y += LINE_HEIGHT;
                draw_text(&format!("  Room: {}  Sector: ({}, {})",
                    obj_room_idx, obj.sector_x, obj.sector_z),
                    x, (y + 10.0).floor(), FONT_SIZE_CONTENT, WHITE);
                y += LINE_HEIGHT;
                draw_text(&format!("  Height: {:.0}  Facing: {:.1}°",
                    obj.height, obj.facing.to_degrees()),
                    x, (y + 10.0).floor(), FONT_SIZE_CONTENT, WHITE);
                y += 20.0;

                // Show asset components
                if let Some(asset) = asset {
                    draw_text("Components:", x, (y + 10.0).floor(), FONT_SIZE_HEADER, Color::from_rgba(150, 150, 150, 255));
                    y += LINE_HEIGHT;
                    for component in &asset.components {
                        let comp_name = component.type_name();
                        draw_text(&format!("  • {}", comp_name), x, (y + 10.0).floor(), FONT_SIZE_CONTENT, WHITE);
                        y += LINE_HEIGHT;
                    }
                    if asset.components.is_empty() {
                        draw_text("  (none)", x, (y + 10.0).floor(), FONT_SIZE_CONTENT, Color::from_rgba(150, 150, 150, 255));
                        y += LINE_HEIGHT;
                    }
                    y += 8.0;

                    // Player spawn shows player settings
                    if asset.has_spawn_point(true) {
                        let section_color = Color::from_rgba(120, 150, 180, 255);
                        let line_height = 20.0;
                        let label_color = Color::from_rgba(180, 180, 190, 255);

                        // === Collision Section ===
                        draw_text("Collision", x, (y + 12.0).floor(), 11.0, section_color);
                        y += 18.0;

                        let r = draw_player_prop_field(ctx, x, y, container_width, line_height, "Radius",
                            state.level.player_settings.radius, 0,
                            &mut state.player_prop_editing, &mut state.player_prop_buffer, label_color);
                        if let Some(v) = r.new_value { state.level.player_settings.radius = v; }
                        y = r.new_y;

                        let r = draw_player_prop_field(ctx, x, y, container_width, line_height, "Height",
                            state.level.player_settings.height, 1,
                            &mut state.player_prop_editing, &mut state.player_prop_buffer, label_color);
                        if let Some(v) = r.new_value { state.level.player_settings.height = v; }
                        y = r.new_y;

                        let r = draw_player_prop_field(ctx, x, y, container_width, line_height, "Step",
                            state.level.player_settings.step_height, 2,
                            &mut state.player_prop_editing, &mut state.player_prop_buffer, label_color);
                        if let Some(v) = r.new_value { state.level.player_settings.step_height = v; }
                        y = r.new_y;

                        y += 6.0;

                        // === Movement Section ===
                        draw_text("Movement", x, (y + 12.0).floor(), 11.0, section_color);
                        y += 18.0;

                        let r = draw_player_prop_field(ctx, x, y, container_width, line_height, "Walk",
                            state.level.player_settings.walk_speed, 3,
                            &mut state.player_prop_editing, &mut state.player_prop_buffer, label_color);
                        if let Some(v) = r.new_value { state.level.player_settings.walk_speed = v; }
                        y = r.new_y;

                        let r = draw_player_prop_field(ctx, x, y, container_width, line_height, "Run",
                            state.level.player_settings.run_speed, 4,
                            &mut state.player_prop_editing, &mut state.player_prop_buffer, label_color);
                        if let Some(v) = r.new_value { state.level.player_settings.run_speed = v; }
                        y = r.new_y;

                        let r = draw_player_prop_field(ctx, x, y, container_width, line_height, "Gravity",
                            state.level.player_settings.gravity, 5,
                            &mut state.player_prop_editing, &mut state.player_prop_buffer, label_color);
                        if let Some(v) = r.new_value { state.level.player_settings.gravity = v; }
                        y = r.new_y;

                        y += 6.0;

                        // === Camera Section ===
                        draw_text("Camera", x, (y + 12.0).floor(), 11.0, section_color);
                        y += 18.0;

                        let r = draw_player_prop_field(ctx, x, y, container_width, line_height, "Distance",
                            state.level.player_settings.camera_distance, 6,
                            &mut state.player_prop_editing, &mut state.player_prop_buffer, label_color);
                        if let Some(v) = r.new_value { state.level.player_settings.camera_distance = v; }
                        y = r.new_y;

                        let r = draw_player_prop_field(ctx, x, y, container_width, line_height, "Y Offset",
                            state.level.player_settings.camera_vertical_offset, 7,
                            &mut state.player_prop_editing, &mut state.player_prop_buffer, label_color);
                        if let Some(v) = r.new_value { state.level.player_settings.camera_vertical_offset = v; }
                        y = r.new_y;

                        y += 10.0;

                        // === Camera Preview ===
                        draw_text("Preview", x, (y + 12.0).floor(), 11.0, section_color);
                        y += 18.0;

                        // Calculate player world position
                        let player_world_pos = if let Some(room) = state.level.rooms.get(obj_room_idx) {
                            obj.world_position(room)
                        } else {
                            Vec3::new(0.0, 0.0, 0.0)
                        };

                        // Camera position: behind and above the player (orbit style preview)
                        let settings = &state.level.player_settings;
                        let look_at = Vec3::new(
                            player_world_pos.x,
                            player_world_pos.y + settings.camera_vertical_offset,
                            player_world_pos.z,
                        );
                        let cam_pos = Vec3::new(
                            player_world_pos.x,
                            player_world_pos.y + settings.camera_vertical_offset + settings.camera_distance * 0.2,
                            player_world_pos.z - settings.camera_distance,
                        );

                        // Preview dimensions (4:3 aspect ratio)
                        let preview_w = (container_width - 8.0).min(160.0);
                        let preview_h = preview_w * 0.75;

                        // Render camera preview
                        draw_player_camera_preview(
                            x,
                            y,
                            preview_w,
                            preview_h,
                            cam_pos,
                            look_at,
                            player_world_pos,
                            settings.radius,
                            settings.height,
                            &state.level,
                            &state.texture_packs,
                            &state.user_textures,
                            &state.asset_library,
                        );

                        y += preview_h + 8.0;
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
                    if let Some(obj_mut) = state.level.get_object_mut(obj_room_idx, obj_idx) {
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
                    state.level.remove_object(obj_room_idx, obj_idx);
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

    // Handle panel scroll
    let inside = ctx.mouse.inside(&rect);
    if inside && ctx.mouse.scroll != 0.0 {
        state.properties_scroll -= ctx.mouse.scroll * 30.0;
        state.properties_scroll = state.properties_scroll.clamp(0.0, max_scroll);
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

        super::Selection::Vertex { room, x: gx, z: gz, face, .. } => {
            // Same as SectorFace - we show the face this vertex belongs to
            let sector_data = state.level.rooms.get(*room)
                .and_then(|r| r.get_sector(*gx, *gz));

            let mut height = header_height;

            if let Some(sector) = sector_data {
                match face {
                    super::SectorFace::Floor => {
                        if let Some(floor) = &sector.floor {
                            height += horizontal_face_container_height(floor, true) + CONTAINER_MARGIN;
                        }
                    }
                    super::SectorFace::Ceiling => {
                        if let Some(ceiling) = &sector.ceiling {
                            height += horizontal_face_container_height(ceiling, false) + CONTAINER_MARGIN;
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
                    super::SectorFace::WallNwSe(i) => {
                        if let Some(wall) = sector.walls_nwse.get(*i) {
                            height += wall_face_container_height(wall) + CONTAINER_MARGIN;
                        }
                    }
                    super::SectorFace::WallNeSw(i) => {
                        if let Some(wall) = sector.walls_nesw.get(*i) {
                            height += wall_face_container_height(wall) + CONTAINER_MARGIN;
                        }
                    }
                }
            }
            height
        }

        super::Selection::SectorFace { room, x: gx, z: gz, face } => {
            let sector_data = state.level.rooms.get(*room)
                .and_then(|r| r.get_sector(*gx, *gz));

            let mut height = header_height;

            if let Some(sector) = sector_data {
                match face {
                    super::SectorFace::Floor => {
                        if let Some(floor) = &sector.floor {
                            height += horizontal_face_container_height(floor, true) + CONTAINER_MARGIN;
                        }
                    }
                    super::SectorFace::Ceiling => {
                        if let Some(ceiling) = &sector.ceiling {
                            height += horizontal_face_container_height(ceiling, false) + CONTAINER_MARGIN;
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
                    super::SectorFace::WallNwSe(i) => {
                        if let Some(wall) = sector.walls_nwse.get(*i) {
                            height += wall_face_container_height(wall) + CONTAINER_MARGIN;
                        }
                    }
                    super::SectorFace::WallNeSw(i) => {
                        if let Some(wall) = sector.walls_nesw.get(*i) {
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
                    height += horizontal_face_container_height(floor, true) + CONTAINER_MARGIN;
                }
                if let Some(ceiling) = &sector.ceiling {
                    height += horizontal_face_container_height(ceiling, false) + CONTAINER_MARGIN;
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
        super::Selection::Object { room: room_idx, index } => {
            // Base height for all objects: header + location + enabled + delete
            let mut height = 24.0 + 18.0 + 18.0 + 24.0 + 28.0 + 28.0; // header + location lines + components + enabled + delete

            // Add extra height for objects with custom properties
            let obj_opt = state.level.rooms.get(*room_idx)
                .and_then(|room| room.objects.get(*index));
            if let Some(obj) = obj_opt {
                // Add height for component list
                if let Some(asset) = state.asset_library.get_by_id(obj.asset_id) {
                    height += 18.0 + asset.components.len() as f32 * 18.0; // Components header + list

                    if asset.has_spawn_point(true) {
                        // Player settings: 3 sections with scroll-to-edit rows
                        // Collision: header 18 + 3 rows at 20 = 78
                        // Movement: header 18 + 3 rows at 20 = 78 + 6 gap
                        // Camera: header 18 + 2 rows at 20 = 58 + 6 gap + 8 final
                        height += 78.0 + 6.0 + 78.0 + 6.0 + 58.0 + 8.0; // = 234
                    }
                }
            }
            height
        }
    }
}

fn draw_status_bar(rect: Rect, state: &EditorState) {
    draw_rectangle(rect.x.floor(), rect.y.floor(), rect.w, rect.h, Color::from_rgba(40, 40, 45, 255));

    // Show status message on the left if available
    let status_end_x = if let Some(msg) = state.get_status() {
        let msg_dims = measure_text(&msg, None, 14, 1.0);
        draw_text(&msg, (rect.x + 10.0).floor(), (rect.y + 15.0).floor(), 14.0, Color::from_rgba(100, 255, 100, 255));
        rect.x + 10.0 + msg_dims.width + 20.0
    } else {
        rect.x + 10.0
    };

    // Context-sensitive shortcuts based on current tool/mode
    let mut shortcuts: Vec<&str> = Vec::new();

    match state.tool {
        EditorTool::DrawWall => {
            let dir = match state.wall_direction {
                crate::world::Direction::North => "N",
                crate::world::Direction::East => "E",
                crate::world::Direction::South => "S",
                crate::world::Direction::West => "W",
                crate::world::Direction::NwSe => "NW-SE",
                crate::world::Direction::NeSw => "NE-SW",
            };
            let gap = if state.wall_prefer_high { "High" } else { "Low" };
            // Build dynamic strings for wall tool
            let shortcuts_text = format!("[R] Rotate ({})  [F] Gap ({})  [E] Extrude", dir, gap);
            let text_dims = measure_text(&shortcuts_text, None, 14, 1.0);
            let text_x = rect.right() - text_dims.width - 10.0;
            let text_y = rect.y + (rect.h + text_dims.height) / 2.0 - 2.0;
            if text_x > status_end_x {
                draw_text(&shortcuts_text, text_x.floor(), text_y.floor(), 14.0, Color::from_rgba(180, 180, 190, 255));
            }
            return;
        }
        EditorTool::Select => {
            shortcuts.push("[E] Extrude");
            shortcuts.push("[Del] Delete");
            shortcuts.push("[.] Focus");
        }
        EditorTool::PlaceObject => {
            shortcuts.push("[Click] Place object");
            shortcuts.push("[Del] Delete");
        }
        _ => {}
    }

    // Add vertex linking hint
    if state.link_coincident_vertices {
        shortcuts.push("[L] Unlink vertices");
    } else {
        shortcuts.push("[L] Link vertices");
    }

    if !shortcuts.is_empty() {
        let shortcuts_text = shortcuts.join("  ");
        let text_dims = measure_text(&shortcuts_text, None, 14, 1.0);
        let text_x = rect.right() - text_dims.width - 10.0;
        let text_y = rect.y + (rect.h + text_dims.height) / 2.0 - 2.0;

        if text_x > status_end_x {
            draw_text(&shortcuts_text, text_x.floor(), text_y.floor(), 14.0, Color::from_rgba(180, 180, 190, 255));
        }
    }
}

/// Draw a small camera preview showing what the player camera sees
fn draw_player_camera_preview(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    cam_pos: Vec3,
    look_at: Vec3,
    player_pos: Vec3,
    player_radius: f32,
    player_height: f32,
    level: &crate::world::Level,
    texture_packs: &[super::TexturePack],
    user_textures: &crate::texture::TextureLibrary,
    asset_library: &crate::asset::AssetLibrary,
) {
    // Create a small framebuffer for the preview
    let fb_w = (width as usize).max(80);
    let fb_h = (height as usize).max(60);
    let mut fb = Framebuffer::new(fb_w, fb_h);
    fb.clear(RasterColor::new(20, 20, 25));

    // Set up camera looking from cam_pos toward look_at
    let mut camera = Camera::new();
    camera.position = cam_pos;

    // Calculate direction from camera to look_at point
    let dir = Vec3::new(
        look_at.x - cam_pos.x,
        look_at.y - cam_pos.y,
        look_at.z - cam_pos.z,
    );
    let len = (dir.x * dir.x + dir.y * dir.y + dir.z * dir.z).sqrt();
    if len > 0.001 {
        let nx = dir.x / len;
        let ny = dir.y / len;
        let nz = dir.z / len;

        // rotation_x (pitch): from y = -sin(rotation_x)
        camera.rotation_x = (-ny).asin();
        // rotation_y (yaw): from x/z = sin(rotation_y)/cos(rotation_y)
        camera.rotation_y = nx.atan2(nz);
    }
    camera.update_basis();

    // Build lighting from level - collect lights from any asset with Light component
    let mut lights = Vec::new();
    let mut total_ambient = 0.0;
    let mut room_count = 0;
    for room in &level.rooms {
        total_ambient += room.ambient;
        room_count += 1;
        // Collect lights from room objects (any asset with Light component)
        for obj in room.objects.iter().filter(|o| {
            o.enabled && asset_library.get_by_id(o.asset_id)
                .map(|a| a.has_light())
                .unwrap_or(false)
        }) {
            // Use default light settings for preview
            let world_pos = obj.world_position(room);
            let light = Light::point(world_pos, 5000.0, 1.0);
            lights.push(light);
        }
    }
    let ambient = if room_count > 0 { total_ambient / room_count as f32 } else { 0.5 };

    // Render settings
    let settings = RasterSettings {
        shading: ShadingMode::Gouraud,
        lights,
        ambient,
        ..RasterSettings::default()
    };

    // Build texture map (packs + user textures)
    let mut textures: Vec<RasterTexture> = texture_packs
        .iter()
        .flat_map(|pack| &pack.textures)
        .cloned()
        .collect();

    let mut texture_map: std::collections::HashMap<(String, String), usize> = std::collections::HashMap::new();
    let mut texture_idx = 0;
    for pack in texture_packs {
        for tex in &pack.textures {
            texture_map.insert((pack.name.clone(), tex.name.clone()), texture_idx);
            texture_idx += 1;
        }
    }
    // Add user textures
    for name in user_textures.names() {
        if let Some(user_tex) = user_textures.get(name) {
            textures.push(user_tex.to_raster_texture());
            texture_map.insert((crate::world::USER_TEXTURE_PACK.to_string(), name.to_string()), texture_idx);
            texture_idx += 1;
        }
    }

    let resolve_texture = |tex_ref: &crate::world::TextureRef| -> Option<usize> {
        if !tex_ref.is_valid() {
            return Some(0);
        }
        texture_map.get(&(tex_ref.pack.clone(), tex_ref.name.clone())).copied()
    };

    // Render each room
    for room in &level.rooms {
        let (vertices, faces) = room.to_render_data_with_textures(&resolve_texture);
        if !vertices.is_empty() {
            render_mesh(&mut fb, &vertices, &faces, &textures, &camera, &settings);
        }
    }

    // Draw player cylinder wireframe
    let cylinder_color = RasterColor::new(100, 255, 100); // Green
    draw_preview_wireframe_cylinder(&mut fb, &camera, player_pos, player_radius, player_height, 12, cylinder_color);

    // Draw framebuffer to screen
    let fb_texture = Texture2D::from_rgba8(fb.width as u16, fb.height as u16, &fb.pixels);
    fb_texture.set_filter(FilterMode::Nearest);

    // Draw border
    draw_rectangle(x - 1.0, y - 1.0, width + 2.0, height + 2.0, Color::from_rgba(60, 60, 65, 255));

    draw_texture_ex(
        &fb_texture,
        x,
        y,
        WHITE,
        DrawTextureParams {
            dest_size: Some(vec2(width, height)),
            ..Default::default()
        },
    );
}

/// Draw a wireframe cylinder for the camera preview
fn draw_preview_wireframe_cylinder(
    fb: &mut Framebuffer,
    camera: &Camera,
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
        let px = center.x + radius * angle.cos();
        let pz = center.z + radius * angle.sin();

        bottom_points.push(Vec3::new(px, center.y, pz));
        top_points.push(Vec3::new(px, center.y + height, pz));
    }

    // Draw bottom circle
    for i in 0..segments {
        let next = (i + 1) % segments;
        draw_preview_3d_line(fb, bottom_points[i], bottom_points[next], camera, color);
    }

    // Draw top circle
    for i in 0..segments {
        let next = (i + 1) % segments;
        draw_preview_3d_line(fb, top_points[i], top_points[next], camera, color);
    }

    // Draw vertical lines connecting top and bottom
    let skip = if segments > 8 { 2 } else { 1 };
    for i in (0..segments).step_by(skip) {
        draw_preview_3d_line(fb, bottom_points[i], top_points[i], camera, color);
    }
}

/// Draw a 3D line into the framebuffer for camera preview
fn draw_preview_3d_line(
    fb: &mut Framebuffer,
    p0: Vec3,
    p1: Vec3,
    camera: &Camera,
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
    let s0 = preview_world_to_screen(clipped_p0, camera, fb.width, fb.height);
    let s1 = preview_world_to_screen(clipped_p1, camera, fb.width, fb.height);

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

/// Project world position to screen for camera preview
fn preview_world_to_screen(pos: Vec3, camera: &Camera, width: usize, height: usize) -> Option<(f32, f32)> {
    let rel = pos - camera.position;

    // Transform to camera space
    let cam_x = rel.dot(camera.basis_x);
    let cam_y = rel.dot(camera.basis_y);
    let cam_z = rel.dot(camera.basis_z);

    if cam_z < 0.1 {
        return None;
    }

    // Project (simple perspective)
    let scale = (height as f32) / cam_z;
    let screen_x = (width as f32 / 2.0) + cam_x * scale;
    let screen_y = (height as f32 / 2.0) - cam_y * scale;

    Some((screen_x, screen_y))
}
