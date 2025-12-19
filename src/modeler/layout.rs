//! Modeler UI layout and rendering

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext, SplitPanel, draw_panel, panel_content_rect, Toolbar, icon, icon_button};
use crate::rasterizer::Framebuffer;
use super::state::{ModelerState, ModelerView, SelectMode, TransformTool};
use super::viewport::draw_modeler_viewport;

// Colors (matching tracker/editor style)
const BG_COLOR: Color = Color::new(0.11, 0.11, 0.13, 1.0);
const HEADER_COLOR: Color = Color::new(0.15, 0.15, 0.18, 1.0);
const TEXT_COLOR: Color = Color::new(0.8, 0.8, 0.85, 1.0);
const TEXT_DIM: Color = Color::new(0.4, 0.4, 0.45, 1.0);
const ACCENT_COLOR: Color = Color::new(0.0, 0.75, 0.9, 1.0);

/// Actions that can be triggered by the modeler UI
#[derive(Debug, Clone, PartialEq)]
pub enum ModelerAction {
    None,
    New,
    Save,
    SaveAs,
    PromptLoad,     // Show file dialog
    Load(String),   // Load specific path
    Export,         // Browser: download as file
    Import,         // Browser: upload file
    BrowseModels,   // Open model browser
    BrowseMeshes,   // Open mesh browser (OBJ files)
}

/// Modeler layout state (split panel ratios)
pub struct ModelerLayout {
    /// Main horizontal split (left panels | center+right)
    pub main_split: SplitPanel,
    /// Right split (center viewport | right panels)
    pub right_split: SplitPanel,
    /// Left vertical split (hierarchy/dopesheet | UV editor)
    pub left_split: SplitPanel,
    /// Right vertical split (atlas | properties)
    pub right_panel_split: SplitPanel,
    /// Timeline height
    pub timeline_height: f32,
}

impl ModelerLayout {
    pub fn new() -> Self {
        Self {
            main_split: SplitPanel::horizontal(100).with_ratio(0.20).with_min_size(150.0),
            right_split: SplitPanel::horizontal(101).with_ratio(0.80).with_min_size(150.0),
            left_split: SplitPanel::vertical(102).with_ratio(0.5).with_min_size(100.0),
            right_panel_split: SplitPanel::vertical(103).with_ratio(0.4).with_min_size(80.0),
            timeline_height: 80.0,
        }
    }
}

impl Default for ModelerLayout {
    fn default() -> Self {
        Self::new()
    }
}

/// Draw the complete modeler UI
pub fn draw_modeler(
    ctx: &mut UiContext,
    layout: &mut ModelerLayout,
    state: &mut ModelerState,
    fb: &mut Framebuffer,
    bounds: Rect,
    icon_font: Option<&Font>,
) -> ModelerAction {
    let screen = bounds;

    // Toolbar at top
    let toolbar_height = 36.0;
    let toolbar_rect = screen.slice_top(toolbar_height);
    let main_rect = screen.remaining_after_top(toolbar_height);

    // Status bar at bottom
    let status_height = 22.0;
    let status_rect = main_rect.slice_bottom(status_height);
    let content_rect = main_rect.remaining_after_bottom(status_height);

    // Timeline at bottom of content (only in Animate mode)
    let (panels_rect, timeline_rect) = if state.view == ModelerView::Animate {
        let timeline = content_rect.slice_bottom(layout.timeline_height);
        (content_rect.remaining_after_bottom(layout.timeline_height), Some(timeline))
    } else {
        (content_rect, None)
    };

    // Draw toolbar
    let action = draw_toolbar(ctx, toolbar_rect, state, icon_font);

    // Main split: left panels | rest
    let (left_rect, rest_rect) = layout.main_split.update(ctx, panels_rect);

    // Right split: center viewport | right panels
    let (center_rect, right_rect) = layout.right_split.update(ctx, rest_rect);

    // Left split: hierarchy/dopesheet | UV editor
    let (hierarchy_rect, uv_rect) = layout.left_split.update(ctx, left_rect);

    // Right split: atlas | properties
    let (atlas_rect, props_rect) = layout.right_panel_split.update(ctx, right_rect);

    // Draw panels based on view mode
    let left_top_label = match state.view {
        ModelerView::Animate => "Dopesheet",
        _ => "Hierarchy",
    };
    draw_panel(hierarchy_rect, Some(left_top_label), Color::from_rgba(35, 35, 40, 255));
    draw_hierarchy_panel(ctx, panel_content_rect(hierarchy_rect, true), state);

    draw_panel(uv_rect, Some("UV Editor"), Color::from_rgba(35, 35, 40, 255));
    draw_uv_editor(ctx, panel_content_rect(uv_rect, true), state);

    draw_panel(center_rect, Some("3D Viewport"), Color::from_rgba(25, 25, 30, 255));
    draw_viewport(ctx, panel_content_rect(center_rect, true), state, fb);

    draw_panel(atlas_rect, Some("Atlas"), Color::from_rgba(35, 35, 40, 255));
    draw_atlas_panel(ctx, panel_content_rect(atlas_rect, true), state);

    draw_panel(props_rect, Some("Properties"), Color::from_rgba(35, 35, 40, 255));
    draw_properties_panel(ctx, panel_content_rect(props_rect, true), state, icon_font);

    // Draw timeline if in animate mode
    if let Some(tl_rect) = timeline_rect {
        draw_panel(tl_rect, Some("Timeline"), Color::from_rgba(30, 30, 35, 255));
        draw_timeline(ctx, panel_content_rect(tl_rect, true), state, icon_font);
    }

    // Draw status bar
    draw_status_bar(status_rect, state);

    // Handle keyboard shortcuts
    handle_keyboard(state);

    action
}

fn draw_toolbar(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, icon_font: Option<&Font>) -> ModelerAction {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(40, 40, 45, 255));

    let mut action = ModelerAction::None;
    let mut toolbar = Toolbar::new(rect);

    // File operations
    if toolbar.icon_button(ctx, icon::FILE_PLUS, icon_font, "New (Ctrl+N)") {
        action = ModelerAction::New;
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        if toolbar.icon_button(ctx, icon::FOLDER_OPEN, icon_font, "Open (Ctrl+O)") {
            action = ModelerAction::PromptLoad;
        }
        if toolbar.icon_button(ctx, icon::SAVE, icon_font, "Save (Ctrl+S)") {
            action = ModelerAction::Save;
        }
        if toolbar.icon_button(ctx, icon::SAVE_AS, icon_font, "Save As (Ctrl+Shift+S)") {
            action = ModelerAction::SaveAs;
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        if toolbar.icon_button(ctx, icon::FOLDER_OPEN, icon_font, "Upload") {
            action = ModelerAction::Import;
        }
        if toolbar.icon_button(ctx, icon::SAVE, icon_font, "Download") {
            action = ModelerAction::Export;
        }
    }

    // Model browser (works on both native and WASM)
    if toolbar.icon_button(ctx, icon::BOOK_OPEN, icon_font, "Browse Models") {
        action = ModelerAction::BrowseModels;
    }

    // Mesh browser for OBJ files
    if toolbar.icon_button(ctx, icon::FOLDER_OPEN, icon_font, "Browse Meshes") {
        action = ModelerAction::BrowseMeshes;
    }

    toolbar.separator();

    // Undo/Redo
    if toolbar.icon_button(ctx, icon::UNDO, icon_font, "Undo") {
        state.undo();
    }
    if toolbar.icon_button(ctx, icon::REDO, icon_font, "Redo") {
        state.redo();
    }

    toolbar.separator();

    // Tool buttons
    let tools = [
        (icon::POINTER, "Select", TransformTool::Select),
        (icon::MOVE, "Move (G)", TransformTool::Move),
        (icon::ROTATE_3D, "Rotate (R)", TransformTool::Rotate),
        (icon::SCALE_3D, "Scale (S)", TransformTool::Scale),
    ];

    for (icon_char, tooltip, tool) in tools {
        let is_active = state.tool == tool;
        if toolbar.icon_button_active(ctx, icon_char, icon_font, tooltip, is_active) {
            state.tool = tool;
        }
    }

    toolbar.separator();

    // View mode selector
    toolbar.label("Mode:");
    for view in ModelerView::ALL {
        let is_active = state.view == view;
        let icon_char = match view {
            ModelerView::Model => icon::BOX,
            ModelerView::UV => icon::MAXIMIZE_2,
            ModelerView::Paint => icon::BRUSH,
            ModelerView::Hierarchy => icon::GIT_BRANCH,
            ModelerView::Animate => icon::PLAY,
        };
        if toolbar.icon_button_active(ctx, icon_char, icon_font, view.label(), is_active) {
            state.view = view;
        }
    }

    toolbar.separator();

    // Selection mode (only in Model mode)
    if state.view == ModelerView::Model {
        for mode in SelectMode::ALL {
            let is_active = state.select_mode == mode;
            let icon_char = match mode {
                SelectMode::Bone => icon::BONE,
                SelectMode::Part => icon::BOX,
                SelectMode::Vertex => icon::CIRCLE_DOT,
                SelectMode::Edge => icon::MINUS,
                SelectMode::Face => icon::SCAN,
            };
            if toolbar.icon_button_active(ctx, icon_char, icon_font, mode.label(), is_active) {
                state.select_mode = mode;
            }
        }

        toolbar.separator();
    }

    // PS1 effect toggles
    if toolbar.icon_button_active(ctx, icon::WAVES, icon_font, "Affine Textures (warpy)", state.raster_settings.affine_textures) {
        state.raster_settings.affine_textures = !state.raster_settings.affine_textures;
        let mode = if state.raster_settings.affine_textures { "ON" } else { "OFF" };
        state.set_status(&format!("Affine textures: {}", mode), 1.5);
    }
    if toolbar.icon_button_active(ctx, icon::MAGNET, icon_font, "Vertex Snap (jittery)", state.raster_settings.vertex_snap) {
        state.raster_settings.vertex_snap = !state.raster_settings.vertex_snap;
        let mode = if state.raster_settings.vertex_snap { "ON" } else { "OFF" };
        state.set_status(&format!("Vertex snap: {}", mode), 1.5);
    }
    if toolbar.icon_button_active(ctx, icon::MONITOR, icon_font, "Low Resolution (320x240)", state.raster_settings.low_resolution) {
        state.raster_settings.low_resolution = !state.raster_settings.low_resolution;
        let mode = if state.raster_settings.low_resolution { "320x240" } else { "640x480" };
        state.set_status(&format!("Resolution: {}", mode), 1.5);
    }
    // Shading toggle (cycle through None -> Flat -> Gouraud)
    let shading_active = state.raster_settings.shading != crate::rasterizer::ShadingMode::None;
    if toolbar.icon_button_active(ctx, icon::SUN, icon_font, "Shading (None/Flat/Gouraud)", shading_active) {
        use crate::rasterizer::ShadingMode;
        state.raster_settings.shading = match state.raster_settings.shading {
            ShadingMode::None => ShadingMode::Flat,
            ShadingMode::Flat => ShadingMode::Gouraud,
            ShadingMode::Gouraud => ShadingMode::None,
        };
        let mode = match state.raster_settings.shading {
            ShadingMode::None => "None",
            ShadingMode::Flat => "Flat",
            ShadingMode::Gouraud => "Gouraud",
        };
        state.set_status(&format!("Shading: {}", mode), 1.5);
    }
    if toolbar.icon_button_active(ctx, icon::PROPORTIONS, icon_font, "Aspect Ratio (4:3 / Stretch)", !state.raster_settings.stretch_to_fill) {
        state.raster_settings.stretch_to_fill = !state.raster_settings.stretch_to_fill;
        let mode = if state.raster_settings.stretch_to_fill { "Stretch" } else { "4:3" };
        state.set_status(&format!("Aspect Ratio: {}", mode), 1.5);
    }
    if toolbar.icon_button_active(ctx, icon::LAYERS, icon_font, "Wireframe Overlay", state.raster_settings.wireframe_overlay) {
        state.raster_settings.wireframe_overlay = !state.raster_settings.wireframe_overlay;
        let mode = if state.raster_settings.wireframe_overlay { "ON" } else { "OFF" };
        state.set_status(&format!("Wireframe overlay: {}", mode), 1.5);
    }

    toolbar.separator();

    // Snap toggle
    if toolbar.icon_button_active(ctx, icon::GRID, icon_font, &format!("Snap to Grid ({}) [S key]", state.snap_settings.grid_size), state.snap_settings.enabled) {
        state.snap_settings.enabled = !state.snap_settings.enabled;
        let mode = if state.snap_settings.enabled { "ON" } else { "OFF" };
        state.set_status(&format!("Grid Snap: {}", mode), 1.5);
    }

    toolbar.separator();

    // Model stats
    toolbar.label(&format!(
        "Parts:{} Verts:{} Faces:{}",
        state.model.parts.len(),
        state.model.vertex_count(),
        state.model.face_count()
    ));

    toolbar.separator();

    // Current file label (like world editor)
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

    // Keyboard shortcuts for file operations
    let ctrl = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl)
             || is_key_down(KeyCode::LeftSuper) || is_key_down(KeyCode::RightSuper);
    let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);

    if ctrl && is_key_pressed(KeyCode::N) && action == ModelerAction::None {
        action = ModelerAction::New;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if ctrl && is_key_pressed(KeyCode::O) && action == ModelerAction::None {
            action = ModelerAction::PromptLoad;
        }
        if ctrl && shift && is_key_pressed(KeyCode::S) && action == ModelerAction::None {
            action = ModelerAction::SaveAs;
        } else if ctrl && is_key_pressed(KeyCode::S) && action == ModelerAction::None {
            action = ModelerAction::Save;
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        if ctrl && is_key_pressed(KeyCode::O) && action == ModelerAction::None {
            action = ModelerAction::Import;
        }
        if ctrl && is_key_pressed(KeyCode::S) && action == ModelerAction::None {
            action = ModelerAction::Export;
        }
    }

    action
}

fn draw_hierarchy_panel(_ctx: &mut UiContext, rect: Rect, state: &ModelerState) {
    let mut y = rect.y;
    let line_height = 20.0;
    let indent = 16.0;

    // Draw part tree
    fn draw_part_tree(
        parts: &[super::model::ModelPart],
        parent: Option<usize>,
        depth: usize,
        y: &mut f32,
        rect: &Rect,
        line_height: f32,
        indent: f32,
        _state: &ModelerState,
    ) {
        for (i, part) in parts.iter().enumerate() {
            if part.parent == parent {
                if *y > rect.bottom() - line_height {
                    return;
                }

                let x = rect.x + (depth as f32 * indent);

                // Draw part name
                draw_text(
                    &format!("{}{}",
                        if depth > 0 { "└ " } else { "▼ " },
                        part.name
                    ),
                    x,
                    *y + 14.0,
                    14.0,
                    TEXT_COLOR,
                );

                *y += line_height;

                // Recursively draw children
                draw_part_tree(parts, Some(i), depth + 1, y, rect, line_height, indent, _state);
            }
        }
    }

    if state.model.parts.is_empty() {
        draw_text("No parts", rect.x, y + 14.0, 14.0, TEXT_DIM);
    } else {
        draw_part_tree(&state.model.parts, None, 0, &mut y, &rect, line_height, indent, state);
    }
}

fn draw_uv_editor(_ctx: &mut UiContext, rect: Rect, state: &ModelerState) {
    // Draw checkerboard background
    let checker_size = 8.0;
    for cy in 0..(rect.h as usize / checker_size as usize) {
        for cx in 0..(rect.w as usize / checker_size as usize) {
            let color = if (cx + cy) % 2 == 0 {
                Color::from_rgba(40, 40, 45, 255)
            } else {
                Color::from_rgba(50, 50, 55, 255)
            };
            draw_rectangle(
                rect.x + cx as f32 * checker_size,
                rect.y + cy as f32 * checker_size,
                checker_size,
                checker_size,
                color,
            );
        }
    }

    // Draw atlas texture
    let atlas = &state.model.atlas;
    let atlas_dim = atlas.dimension() as f32;

    // Scale to fit panel (with padding)
    let padding = 10.0;
    let available = rect.w.min(rect.h) - padding * 2.0;
    let scale = available / atlas_dim;

    let atlas_x = rect.x + (rect.w - atlas_dim * scale) * 0.5;
    let atlas_y = rect.y + (rect.h - atlas_dim * scale) * 0.5;

    // Draw atlas pixels (simplified - would be a texture in real impl)
    draw_rectangle(atlas_x, atlas_y, atlas_dim * scale, atlas_dim * scale, Color::from_rgba(100, 100, 100, 255));

    // Draw UV wireframe for selected part
    // TODO: implement UV display

    draw_text(
        &format!("Atlas: {}", atlas.size.label()),
        rect.x + 4.0,
        rect.y + 14.0,
        12.0,
        TEXT_DIM,
    );
}

fn draw_viewport(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, fb: &mut Framebuffer) {
    draw_modeler_viewport(ctx, rect, state, fb);
}

fn draw_atlas_panel(_ctx: &mut UiContext, rect: Rect, state: &ModelerState) {
    let atlas = &state.model.atlas;
    let atlas_dim = atlas.dimension() as f32;

    // Scale to fit panel
    let padding = 4.0;
    let available = rect.w.min(rect.h - 30.0) - padding * 2.0;
    let scale = available / atlas_dim;

    let atlas_x = rect.x + (rect.w - atlas_dim * scale) * 0.5;
    let atlas_y = rect.y + padding;

    // Draw atlas (simplified placeholder)
    draw_rectangle(atlas_x, atlas_y, atlas_dim * scale, atlas_dim * scale, Color::from_rgba(100, 100, 100, 255));
    draw_rectangle_lines(atlas_x, atlas_y, atlas_dim * scale, atlas_dim * scale, 1.0, Color::from_rgba(80, 80, 85, 255));

    // Size label below
    draw_text(
        atlas.size.label(),
        rect.x + (rect.w - 40.0) * 0.5,
        atlas_y + atlas_dim * scale + 16.0,
        12.0,
        TEXT_COLOR,
    );
}

fn draw_properties_panel(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, icon_font: Option<&Font>) {
    let mut y = rect.y;
    let line_height = 18.0;

    draw_text("Selection:", rect.x, y + 14.0, 12.0, TEXT_DIM);
    y += line_height;

    match &state.selection {
        super::state::ModelerSelection::None => {
            draw_text("Nothing selected", rect.x, y + 14.0, 12.0, TEXT_COLOR);
        }
        super::state::ModelerSelection::Bones(bones) => {
            draw_text(&format!("{} bone(s)", bones.len()), rect.x, y + 14.0, 12.0, TEXT_COLOR);
        }
        super::state::ModelerSelection::Parts(parts) => {
            draw_text(&format!("{} part(s)", parts.len()), rect.x, y + 14.0, 12.0, TEXT_COLOR);
        }
        super::state::ModelerSelection::Vertices { part, verts } => {
            draw_text(&format!("{} vertex(es) in part {}", verts.len(), part), rect.x, y + 14.0, 12.0, TEXT_COLOR);
        }
        super::state::ModelerSelection::Edges { part, edges } => {
            draw_text(&format!("{} edge(s) in part {}", edges.len(), part), rect.x, y + 14.0, 12.0, TEXT_COLOR);
        }
        super::state::ModelerSelection::Faces { part, faces } => {
            draw_text(&format!("{} face(s) in part {}", faces.len(), part), rect.x, y + 14.0, 12.0, TEXT_COLOR);
        }
        super::state::ModelerSelection::SpineJoints(joints) => {
            draw_text(&format!("{} spine joint(s)", joints.len()), rect.x, y + 14.0, 12.0, TEXT_COLOR);
        }
        super::state::ModelerSelection::SpineBones(bones) => {
            draw_text(&format!("{} spine bone(s)", bones.len()), rect.x, y + 14.0, 12.0, TEXT_COLOR);
        }
        super::state::ModelerSelection::SpineMeshVertices(verts) => {
            draw_text(&format!("{} mesh vertex(es)", verts.len()), rect.x, y + 14.0, 12.0, TEXT_COLOR);
        }
        super::state::ModelerSelection::SpineMeshEdges(edges) => {
            draw_text(&format!("{} mesh edge(s)", edges.len()), rect.x, y + 14.0, 12.0, TEXT_COLOR);
        }
        super::state::ModelerSelection::SpineMeshFaces(faces) => {
            draw_text(&format!("{} mesh face(s)", faces.len()), rect.x, y + 14.0, 12.0, TEXT_COLOR);
        }
    }

    y += line_height * 2.0;

    // Tool info
    draw_text("Tool:", rect.x, y + 14.0, 12.0, TEXT_DIM);
    y += line_height;
    draw_text(state.tool.label(), rect.x, y + 14.0, 12.0, TEXT_COLOR);

    y += line_height * 2.0;

    // Spine segment properties (when spine model exists)
    // First, read all values we need
    let segment_info: Option<(usize, String, u8, bool, bool)> = {
        let seg_idx = match &state.selection {
            super::state::ModelerSelection::SpineJoints(joints) if !joints.is_empty() => joints[0].0,
            super::state::ModelerSelection::SpineBones(bones) if !bones.is_empty() => bones[0].0,
            _ => 0,
        };

        state.spine_model.as_ref().and_then(|spine_model| {
            spine_model.segments.get(seg_idx).map(|segment| {
                (seg_idx, segment.name.clone(), segment.sides, segment.cap_start, segment.cap_end)
            })
        })
    };

    if let Some((seg_idx, name, sides, cap_start, cap_end)) = segment_info {
        draw_text("Segment:", rect.x, y + 14.0, 12.0, TEXT_DIM);
        y += line_height;
        draw_text(&name, rect.x, y + 14.0, 12.0, TEXT_COLOR);
        y += line_height * 1.5;

        // Sides control
        draw_text("Sides:", rect.x, y + 14.0, 12.0, TEXT_DIM);
        y += line_height;

        let btn_size = 20.0;
        let minus_rect = Rect::new(rect.x, y, btn_size, btn_size);
        let value_x = rect.x + btn_size + 4.0;
        let plus_rect = Rect::new(value_x + 30.0, y, btn_size, btn_size);

        // Draw current value
        draw_text(&format!("{}", sides), value_x + 8.0, y + 14.0, 12.0, TEXT_COLOR);

        // Minus button
        if icon_button(ctx, minus_rect, icon::MINUS, icon_font, "Decrease sides") {
            if let Some(spine_model) = &mut state.spine_model {
                if let Some(segment) = spine_model.segments.get_mut(seg_idx) {
                    if segment.sides > 3 {
                        segment.sides -= 1;
                    }
                }
            }
            state.set_status(&format!("Sides: {}", sides.saturating_sub(1).max(3)), 0.5);
        }

        // Plus button
        if icon_button(ctx, plus_rect, icon::PLUS, icon_font, "Increase sides") {
            if let Some(spine_model) = &mut state.spine_model {
                if let Some(segment) = spine_model.segments.get_mut(seg_idx) {
                    if segment.sides < 24 {
                        segment.sides += 1;
                    }
                }
            }
            state.set_status(&format!("Sides: {}", (sides + 1).min(24)), 0.5);
        }

        y += btn_size + 8.0;

        // Cap controls
        draw_text("Caps:", rect.x, y + 14.0, 12.0, TEXT_DIM);
        y += line_height;

        // Start cap toggle
        let start_rect = Rect::new(rect.x, y, 60.0, 18.0);
        let start_color = if cap_start { ACCENT_COLOR } else { Color::from_rgba(60, 60, 65, 255) };
        draw_rectangle(start_rect.x, start_rect.y, start_rect.w, start_rect.h, start_color);
        draw_text("Start", start_rect.x + 8.0, start_rect.y + 13.0, 11.0, TEXT_COLOR);

        if ctx.mouse.inside(&start_rect) && ctx.mouse.left_pressed {
            if let Some(spine_model) = &mut state.spine_model {
                if let Some(segment) = spine_model.segments.get_mut(seg_idx) {
                    segment.cap_start = !segment.cap_start;
                }
            }
            let status = if !cap_start { "Start cap ON" } else { "Start cap OFF" };
            state.set_status(status, 0.5);
        }

        // End cap toggle
        let end_rect = Rect::new(rect.x + 65.0, y, 60.0, 18.0);
        let end_color = if cap_end { ACCENT_COLOR } else { Color::from_rgba(60, 60, 65, 255) };
        draw_rectangle(end_rect.x, end_rect.y, end_rect.w, end_rect.h, end_color);
        draw_text("End", end_rect.x + 14.0, end_rect.y + 13.0, 11.0, TEXT_COLOR);

        if ctx.mouse.inside(&end_rect) && ctx.mouse.left_pressed {
            if let Some(spine_model) = &mut state.spine_model {
                if let Some(segment) = spine_model.segments.get_mut(seg_idx) {
                    segment.cap_end = !segment.cap_end;
                }
            }
            let status = if !cap_end { "End cap ON" } else { "End cap OFF" };
            state.set_status(status, 0.5);
        }

        y += 30.0;
    }

    // Keyboard shortcuts help (always show when spine model exists)
    if state.spine_model.is_some() {
        draw_text("Shortcuts:", rect.x, y + 14.0, 12.0, TEXT_DIM);
        y += line_height;

        let shortcuts = [
            ("E", "Extrude joint"),
            ("X", "Delete joint/bone"),
            ("W", "Subdivide bone"),
            ("D", "Duplicate segment"),
            ("N", "New segment"),
            ("M", "Mirror segment"),
            ("Shift+X", "Delete segment"),
            ("Shift+Click", "Multi-select"),
            ("S", "Toggle snap"),
            ("Scroll", "Adjust radius"),
        ];

        for (key, desc) in shortcuts {
            if y + line_height > rect.bottom() {
                break;
            }
            draw_text(&format!("{}: {}", key, desc), rect.x, y + 12.0, 10.0, TEXT_DIM);
            y += line_height * 0.8;
        }
    }
}

fn draw_timeline(_ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, icon_font: Option<&Font>) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, HEADER_COLOR);

    // Transport controls
    let mut toolbar = Toolbar::new(Rect::new(rect.x, rect.y, 200.0, 32.0));

    if toolbar.icon_button(_ctx, icon::SKIP_BACK, icon_font, "Stop & Rewind") {
        state.stop_playback();
    }

    let play_icon = if state.playing { icon::PAUSE } else { icon::PLAY };
    if toolbar.icon_button(_ctx, play_icon, icon_font, if state.playing { "Pause" } else { "Play" }) {
        state.toggle_playback();
    }

    toolbar.separator();

    // Frame counter
    let last_frame = state.current_animation()
        .map(|a| a.last_frame())
        .unwrap_or(60);

    toolbar.label(&format!("Frame: {:03}/{:03}", state.current_frame, last_frame));

    toolbar.separator();

    // Keyframe buttons
    if toolbar.icon_button(_ctx, icon::PLUS, icon_font, "Insert Keyframe (I)") {
        state.insert_keyframe();
    }
    if toolbar.icon_button(_ctx, icon::MINUS, icon_font, "Delete Keyframe (K)") {
        state.delete_keyframe();
    }

    // Timeline scrubber area
    let scrub_rect = Rect::new(rect.x + 10.0, rect.y + 40.0, rect.w - 20.0, 30.0);
    draw_rectangle(scrub_rect.x, scrub_rect.y, scrub_rect.w, scrub_rect.h, Color::from_rgba(20, 20, 25, 255));

    // Draw frame markers
    let frames_visible = 60;
    let frame_width = scrub_rect.w / frames_visible as f32;

    for f in 0..=frames_visible {
        let x = scrub_rect.x + f as f32 * frame_width;
        let is_beat = f % 10 == 0;
        draw_line(
            x, scrub_rect.y,
            x, scrub_rect.y + if is_beat { 15.0 } else { 8.0 },
            1.0,
            if is_beat { TEXT_COLOR } else { TEXT_DIM },
        );

        if is_beat {
            draw_text(&format!("{}", f), x - 8.0, scrub_rect.y + 25.0, 10.0, TEXT_DIM);
        }
    }

    // Draw keyframe markers
    if let Some(anim) = state.current_animation() {
        for kf in &anim.keyframes {
            if kf.frame <= frames_visible as u32 {
                let x = scrub_rect.x + kf.frame as f32 * frame_width;
                // Diamond shape
                draw_poly(x, scrub_rect.y + 12.0, 4, 5.0, 45.0, ACCENT_COLOR);
            }
        }
    }

    // Draw playhead
    let playhead_x = scrub_rect.x + state.current_frame as f32 * frame_width;
    draw_line(playhead_x, scrub_rect.y, playhead_x, scrub_rect.bottom(), 2.0, Color::from_rgba(255, 100, 100, 255));
}

fn draw_status_bar(rect: Rect, state: &ModelerState) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(40, 40, 45, 255));

    // Status message
    if let Some(msg) = state.get_status() {
        let center_x = rect.x + rect.w * 0.5 - (msg.len() as f32 * 4.0);
        draw_text(msg, center_x, rect.y + 15.0, 14.0, Color::from_rgba(100, 255, 100, 255));
    }

    // Keyboard hints - show spine hints if spine model is active
    let hints = if state.spine_model.is_some() && state.view == ModelerView::Model {
        "E:Extrude X:Del W:Subdivide D:Dup N:New M:Mirror Shift+X:Del Seg"
    } else {
        match state.view {
            ModelerView::Model => "G:Move R:Rotate S:Scale E:Extrude X:Delete",
            ModelerView::UV => "G:Move S:Scale U:Unwrap",
            ModelerView::Paint => "LMB:Paint Shift+LMB:Pick [/]:Brush Size",
            ModelerView::Hierarchy => "Drag to reparent | Del:Delete part",
            ModelerView::Animate => "Space:Play I:Insert Key K:Delete Key",
        }
    };
    draw_text(hints, rect.right() - (hints.len() as f32 * 6.0) - 8.0, rect.y + 15.0, 12.0, TEXT_DIM);
}

fn handle_keyboard(state: &mut ModelerState) {
    let ctrl = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl)
             || is_key_down(KeyCode::LeftSuper) || is_key_down(KeyCode::RightSuper);

    // Undo/Redo
    if ctrl && is_key_pressed(KeyCode::Z) {
        if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) {
            state.redo();
        } else {
            state.undo();
        }
    }

    // File shortcuts (Ctrl+N, Ctrl+S, etc.) are now handled in draw_toolbar
    // to properly return ModelerAction

    // View mode cycling
    if is_key_pressed(KeyCode::Tab) {
        state.next_view();
    }

    // Selection mode (1-4 keys)
    if is_key_pressed(KeyCode::Key1) {
        state.select_mode = SelectMode::Part;
        state.set_status("Part select mode", 1.0);
    }
    if is_key_pressed(KeyCode::Key2) {
        state.select_mode = SelectMode::Vertex;
        state.set_status("Vertex select mode", 1.0);
    }
    if is_key_pressed(KeyCode::Key3) {
        state.select_mode = SelectMode::Edge;
        state.set_status("Edge select mode", 1.0);
    }
    if is_key_pressed(KeyCode::Key4) {
        state.select_mode = SelectMode::Face;
        state.set_status("Face select mode", 1.0);
    }

    // Transform tools
    if is_key_pressed(KeyCode::G) {
        state.tool = TransformTool::Move;
        state.set_status("Move", 1.0);
    }
    if is_key_pressed(KeyCode::R) {
        state.tool = TransformTool::Rotate;
        state.set_status("Rotate", 1.0);
    }
    if is_key_pressed(KeyCode::S) && !ctrl {
        state.tool = TransformTool::Scale;
        state.set_status("Scale", 1.0);
    }
    if is_key_pressed(KeyCode::E) {
        state.tool = TransformTool::Extrude;
        state.set_status("Extrude", 1.0);
    }

    // Animation controls (in Animate mode)
    if state.view == ModelerView::Animate {
        if is_key_pressed(KeyCode::Space) {
            state.toggle_playback();
        }
        if is_key_pressed(KeyCode::I) {
            state.insert_keyframe();
        }
        if is_key_pressed(KeyCode::K) {
            state.delete_keyframe();
        }
        if is_key_pressed(KeyCode::Left) {
            if state.current_frame > 0 {
                state.current_frame -= 1;
            }
        }
        if is_key_pressed(KeyCode::Right) {
            state.current_frame += 1;
        }
        if is_key_pressed(KeyCode::Home) {
            state.current_frame = 0;
        }
    }
}
