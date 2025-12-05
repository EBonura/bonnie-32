//! Editor layout - TRLE-inspired panel arrangement

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext, SplitPanel, draw_panel, panel_content_rect, Toolbar};
use crate::rasterizer::{Framebuffer, Texture as RasterTexture, RasterSettings};
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
}

/// Draw the complete editor UI, returns action if triggered
pub fn draw_editor(
    ctx: &mut UiContext,
    layout: &mut EditorLayout,
    state: &mut EditorState,
    textures: &[RasterTexture],
    fb: &mut Framebuffer,
    settings: &RasterSettings,
) -> EditorAction {
    let screen = Rect::screen(screen_width(), screen_height());

    // Menu bar at top
    let menu_height = 24.0;
    let menu_rect = screen.slice_top(menu_height);
    let content_rect = screen.remaining_after_top(menu_height);

    // Toolbar below menu
    let toolbar_height = 28.0;
    let toolbar_rect = content_rect.slice_top(toolbar_height);
    let main_rect = content_rect.remaining_after_top(toolbar_height);

    // Status bar at bottom
    let status_height = 22.0;
    let status_rect = main_rect.slice_bottom(status_height);
    let panels_rect = main_rect.remaining_after_bottom(status_height);

    // Draw menu bar and get action
    let action = draw_menu_bar(ctx, menu_rect, state);

    // Draw toolbar
    draw_toolbar(ctx, toolbar_rect, state);

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
    draw_viewport_3d(ctx, panel_content_rect(center_rect, true), state, textures, fb, settings);

    draw_panel(texture_rect, Some("Textures"), Color::from_rgba(35, 35, 40, 255));
    draw_texture_palette(ctx, panel_content_rect(texture_rect, true), state);

    draw_panel(props_rect, Some("Properties"), Color::from_rgba(35, 35, 40, 255));
    draw_properties(ctx, panel_content_rect(props_rect, true), state);

    // Draw status bar
    draw_status_bar(status_rect, state);

    action
}

fn draw_menu_bar(ctx: &mut UiContext, rect: Rect, state: &mut EditorState) -> EditorAction {
    use macroquad::prelude::*;

    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(45, 45, 50, 255));

    let mut action = EditorAction::None;
    let mut toolbar = Toolbar::new(rect);

    // File operations - platform-specific buttons
    if toolbar.button(ctx, "New", 35.0) {
        action = EditorAction::New;
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        // Desktop: native file dialogs
        if toolbar.button(ctx, "Open", 40.0) {
            action = EditorAction::PromptLoad;
        }
        if toolbar.button(ctx, "Save", 40.0) {
            action = EditorAction::Save;
        }
        if toolbar.button(ctx, "Save As", 55.0) {
            action = EditorAction::SaveAs;
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        // Browser: upload/download via JS
        if toolbar.button(ctx, "Upload", 50.0) {
            action = EditorAction::Import;
        }
        if toolbar.button(ctx, "Download", 60.0) {
            action = EditorAction::Export;
        }
    }

    toolbar.separator();

    // Edit operations
    if toolbar.button(ctx, "Undo", 40.0) {
        state.undo();
    }
    if toolbar.button(ctx, "Redo", 40.0) {
        state.redo();
    }

    toolbar.separator();

    // Play button
    if toolbar.button(ctx, "Play", 50.0) {
        action = EditorAction::Play;
    }

    // Check keyboard shortcuts (Ctrl/Cmd + key)
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

    // Show current file in menu bar
    toolbar.separator();
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

    action
}

fn draw_toolbar(ctx: &mut UiContext, rect: Rect, state: &mut EditorState) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(50, 50, 55, 255));

    let mut toolbar = Toolbar::new(rect);

    // Tool buttons
    let tools = [
        ("Select", EditorTool::Select),
        ("Floor", EditorTool::DrawFloor),
        ("Wall", EditorTool::DrawWall),
        ("Ceil", EditorTool::DrawCeiling),
        ("Portal", EditorTool::PlacePortal),
    ];

    for (label, tool) in tools {
        let is_active = state.tool == tool;
        // Highlight active tool
        if is_active {
            let btn_rect = Rect::new(toolbar.cursor_x(), rect.y + 2.0, 50.0, rect.h - 4.0);
            draw_rectangle(btn_rect.x, btn_rect.y, btn_rect.w, btn_rect.h, Color::from_rgba(80, 100, 140, 255));
        }
        if toolbar.button(ctx, label, 50.0) {
            state.tool = tool;
        }
    }

    toolbar.separator();

    // Room navigation
    toolbar.label(&format!("Room: {}", state.current_room));

    if toolbar.button(ctx, "<", 24.0) {
        if state.current_room > 0 {
            state.current_room -= 1;
        }
    }
    if toolbar.button(ctx, ">", 24.0) {
        if state.current_room + 1 < state.level.rooms.len() {
            state.current_room += 1;
        }
    }
    if toolbar.button(ctx, "+", 24.0) {
        // TODO: Add new room
        println!("Add room clicked");
    }
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

        draw_text(&format!("Vertices: {}", room.vertices.len()), x, (y + 14.0).floor(), 16.0, WHITE);
        y += line_height;

        draw_text(&format!("Faces: {}", room.faces.len()), x, (y + 14.0).floor(), 16.0, WHITE);
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

            draw_text(&format!("  Room {} ({} faces)", room.id, room.faces.len()), x, (y + 14.0).floor(), 16.0, color);
            y += line_height;

            if y > rect.bottom() - line_height {
                break;
            }
        }
    } else {
        draw_text("No room selected", x, (y + 14.0).floor(), 16.0, Color::from_rgba(150, 150, 150, 255));
    }
}

fn draw_properties(_ctx: &mut UiContext, rect: Rect, state: &mut EditorState) {
    let mut y = rect.y.floor();
    let x = rect.x.floor();
    let line_height = 20.0;

    match &state.selection {
        super::Selection::None => {
            draw_text("Nothing selected", x, (y + 14.0).floor(), 16.0, Color::from_rgba(150, 150, 150, 255));
        }
        super::Selection::Room(idx) => {
            draw_text(&format!("Room {}", idx), x, (y + 14.0).floor(), 16.0, WHITE);
        }
        super::Selection::Face { room, face } => {
            draw_text(&format!("Face {} in Room {}", face, room), x, (y + 14.0).floor(), 16.0, WHITE);
            y += line_height;

            if let Some(r) = state.level.rooms.get(*room) {
                if let Some(f) = r.faces.get(*face) {
                    draw_text(&format!("Texture: {}", f.texture_id), x, (y + 14.0).floor(), 16.0, WHITE);
                    y += line_height;
                    draw_text(&format!("Triangle: {}", f.is_triangle), x, (y + 14.0).floor(), 16.0, WHITE);
                    y += line_height;
                    draw_text(&format!("Double-sided: {}", f.double_sided), x, (y + 14.0).floor(), 16.0, WHITE);
                }
            }
        }
        super::Selection::Vertex { room, vertex } => {
            draw_text(&format!("Vertex {} in Room {}", vertex, room), x, (y + 14.0).floor(), 16.0, WHITE);
        }
        super::Selection::Portal { room, portal } => {
            draw_text(&format!("Portal {} in Room {}", portal, room), x, (y + 14.0).floor(), 16.0, WHITE);
        }
    }

    // Selected texture preview
    y = (rect.y + 100.0).floor();
    draw_text("Selected Texture:", x, (y + 14.0).floor(), 16.0, Color::from_rgba(150, 150, 150, 255));
    y += line_height;
    draw_text(&format!("ID: {}", state.selected_texture), x, (y + 14.0).floor(), 16.0, WHITE);
}

fn draw_status_bar(rect: Rect, state: &EditorState) {
    draw_rectangle(rect.x.floor(), rect.y.floor(), rect.w, rect.h, Color::from_rgba(40, 40, 45, 255));

    // Show status message if available, otherwise show default status
    let status = if let Some(msg) = state.get_status() {
        msg.to_string()
    } else {
        format!(
            "Room: {} | Tool: {:?} | Zoom: {:.1}x | {}",
            state.current_room,
            state.tool,
            state.grid_zoom / 20.0,
            if state.dirty { "Modified" } else { "Saved" }
        )
    };

    draw_text(&status, (rect.x + 8.0).floor(), (rect.y + 15.0).floor(), 16.0, WHITE);

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
