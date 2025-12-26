//! Modeler UI layout and rendering

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext, SplitPanel, draw_panel, panel_content_rect, Toolbar, icon, icon_button, draw_ps1_color_picker_with_blend_mode, ps1_color_picker_with_blend_mode_height};
use crate::rasterizer::{Framebuffer, render_mesh, Camera, OrthoProjection};
use crate::rasterizer::{Vertex as RasterVertex, Face as RasterFace, Color as RasterColor};
use super::state::{ModelerState, SelectMode, TransformTool, ViewportId, ViewMode, ContextMenu, ModalTransform};
use super::viewport::draw_modeler_viewport;
use super::mesh_editor::EditableMesh;
use crate::rasterizer::Vec3;

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

    // No timeline in simplified mesh-only mode
    let panels_rect = content_rect;
    let timeline_rect: Option<Rect> = None;

    // Draw toolbar
    let action = draw_toolbar(ctx, toolbar_rect, state, icon_font);

    // Main split: left panels | rest
    let (left_rect, rest_rect) = layout.main_split.update(ctx, panels_rect);

    // Right split: center viewport | right panels
    let (center_rect, right_rect) = layout.right_split.update(ctx, rest_rect);

    // Left panel: Overview only (full height now, UV/Atlas merged into right side)
    let hierarchy_rect = left_rect;

    // Right split: Texture/UV panel | properties
    let (texture_rect, props_rect) = layout.right_panel_split.update(ctx, right_rect);

    // Draw panels - simplified for mesh-only workflow
    draw_panel(hierarchy_rect, Some("Overview"), Color::from_rgba(35, 35, 40, 255));
    draw_overview_panel(ctx, panel_content_rect(hierarchy_rect, true), state);

    // Draw 4-panel viewport (PicoCAD-style)
    draw_4panel_viewport(ctx, center_rect, state, fb);

    // Unified Texture/UV panel (like PicoCAD - V toggles between build and texture editing)
    let texture_title = if state.view_mode == ViewMode::Texture { "Texture (Editing)" } else { "Texture (View)" };
    draw_panel(texture_rect, Some(texture_title), Color::from_rgba(35, 35, 40, 255));
    draw_texture_uv_panel(ctx, panel_content_rect(texture_rect, true), state);

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

    // Draw context menu (on top of everything)
    draw_context_menu(ctx, state);

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

    // Transform tools (Rotate/Scale only - Select/Move replaced by hover+gizmo)
    let tools = [
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
    if toolbar.icon_button_active(ctx, icon::LAYERS, icon_font, "Wireframe Mode", state.raster_settings.wireframe_overlay) {
        state.raster_settings.wireframe_overlay = !state.raster_settings.wireframe_overlay;
        let mode = if state.raster_settings.wireframe_overlay { "Wireframe" } else { "Solid" };
        state.set_status(&format!("Render: {}", mode), 1.5);
    }

    toolbar.separator();

    // Snap toggle
    if toolbar.icon_button_active(ctx, icon::GRID, icon_font, &format!("Snap to Grid ({}) [S key]", state.snap_settings.grid_size), state.snap_settings.enabled) {
        state.snap_settings.enabled = !state.snap_settings.enabled;
        let mode = if state.snap_settings.enabled { "ON" } else { "OFF" };
        state.set_status(&format!("Grid Snap: {}", mode), 1.5);
    }
    // Vertex linking toggle (move coincident vertices together)
    let link_icon = if state.vertex_linking { icon::LINK } else { icon::LINK_OFF };
    if toolbar.icon_button_active(ctx, link_icon, icon_font, "Vertex Linking (move welded verts together)", state.vertex_linking) {
        state.vertex_linking = !state.vertex_linking;
        let mode = if state.vertex_linking { "ON" } else { "OFF" };
        state.set_status(&format!("Vertex Linking: {}", mode), 1.5);
    }

    toolbar.separator();

    // Mesh stats
    let stats = format!("Verts:{} Faces:{}", state.mesh.vertex_count(), state.mesh.face_count());
    toolbar.label(&stats);

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

/// Draw the Overview panel (PicoCAD-style object list)
fn draw_overview_panel(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState) {
    let row_height = 22.0;
    let icon_width = 20.0;
    let mut y = rect.y;

    // Header with project stats
    let total_verts = state.project.total_vertices();
    let total_faces = state.project.total_faces();
    draw_text(
        &format!("{} objects | {} verts | {} faces",
            state.project.objects.len(), total_verts, total_faces),
        rect.x, y + 14.0, 12.0, TEXT_DIM,
    );
    y += row_height;

    // Separator
    draw_line(rect.x, y, rect.x + rect.w, y, 1.0, Color::from_rgba(60, 60, 65, 255));
    y += 4.0;

    // List of objects
    let selected_idx = state.project.selected_object;
    let mouse_pos = mouse_position();
    let mut clicked_object: Option<usize> = None;
    let mut toggle_visibility: Option<usize> = None;

    for (i, obj) in state.project.objects.iter().enumerate() {
        let row_rect = Rect {
            x: rect.x,
            y,
            w: rect.w,
            h: row_height,
        };

        let is_selected = selected_idx == Some(i);
        let is_hovered = row_rect.contains(mouse_pos.0, mouse_pos.1);

        // Background for selected/hovered
        if is_selected {
            draw_rectangle(row_rect.x, row_rect.y, row_rect.w, row_rect.h, Color::from_rgba(60, 90, 120, 255));
        } else if is_hovered {
            draw_rectangle(row_rect.x, row_rect.y, row_rect.w, row_rect.h, Color::from_rgba(50, 50, 55, 255));
        }

        // Visibility icon (eye)
        let eye_rect = Rect { x: rect.x + 2.0, y, w: icon_width, h: row_height };
        let eye_hovered = eye_rect.contains(mouse_pos.0, mouse_pos.1);
        let eye_color = if obj.visible {
            if eye_hovered { ACCENT_COLOR } else { TEXT_COLOR }
        } else {
            TEXT_DIM
        };
        let eye_icon = if obj.visible { "●" } else { "○" };
        draw_text(eye_icon, eye_rect.x + 4.0, y + 16.0, 14.0, eye_color);

        // Lock icon
        let lock_x = rect.x + icon_width + 2.0;
        let lock_color = if obj.locked { Color::from_rgba(255, 180, 100, 255) } else { TEXT_DIM };
        let lock_icon = if obj.locked { "L" } else { "" };
        draw_text(lock_icon, lock_x + 2.0, y + 16.0, 12.0, lock_color);

        // Object name
        let name_x = lock_x + 16.0;
        let name_color = if obj.visible { TEXT_COLOR } else { TEXT_DIM };
        let display_name = if obj.name.len() > 20 {
            format!("{}...", &obj.name[..17])
        } else {
            obj.name.clone()
        };
        draw_text(&display_name, name_x, y + 16.0, 14.0, name_color);

        // Face count
        let face_count = obj.mesh.face_count();
        let count_text = format!("{}", face_count);
        let count_x = rect.x + rect.w - 30.0;
        draw_text(&count_text, count_x, y + 16.0, 12.0, TEXT_DIM);

        // Handle clicks
        if is_mouse_button_pressed(MouseButton::Left) {
            if eye_hovered {
                toggle_visibility = Some(i);
            } else if is_hovered && !obj.locked {
                clicked_object = Some(i);
            }
        }

        y += row_height;

        // Stop if we run out of space
        if y + row_height > rect.y + rect.h {
            break;
        }
    }

    // Apply interactions
    if let Some(i) = toggle_visibility {
        state.project.objects[i].visible = !state.project.objects[i].visible;
    }
    if let Some(i) = clicked_object {
        state.project.selected_object = Some(i);
        state.selection.clear();
    }

    // Show selection info at bottom
    if let Some(idx) = state.project.selected_object {
        if let Some(obj) = state.project.objects.get(idx) {
            let info_y = rect.y + rect.h - 32.0;
            draw_line(rect.x, info_y - 4.0, rect.x + rect.w, info_y - 4.0, 1.0, Color::from_rgba(60, 60, 65, 255));

            // Selection info for the current object
            match &state.selection {
                super::state::ModelerSelection::Vertices(indices) => {
                    draw_text(
                        &format!("{} vertices selected", indices.len()),
                        rect.x, info_y + 12.0, 12.0, ACCENT_COLOR,
                    );
                }
                super::state::ModelerSelection::Edges(indices) => {
                    draw_text(
                        &format!("{} edges selected", indices.len()),
                        rect.x, info_y + 12.0, 12.0, ACCENT_COLOR,
                    );
                }
                super::state::ModelerSelection::Faces(indices) => {
                    draw_text(
                        &format!("{} faces selected", indices.len()),
                        rect.x, info_y + 12.0, 12.0, ACCENT_COLOR,
                    );
                }
                _ => {
                    draw_text(
                        &format!("\"{}\" - {} faces", obj.name, obj.mesh.face_count()),
                        rect.x, info_y + 12.0, 12.0, TEXT_DIM,
                    );
                }
            }
        }
    }
}

/// Unified Texture/UV Panel - combines atlas display with UV editing
/// Press V to toggle between View mode (read-only) and Edit mode (interactive)
fn draw_texture_uv_panel(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState) {
    let atlas = &state.project.atlas;
    let atlas_width = atlas.width;
    let atlas_height = atlas.height;
    let atlas_w = atlas_width as f32;
    let atlas_h = atlas_height as f32;

    // Reserve space for PS1 color picker at bottom (swatch + 3 RGB sliders + blend mode + presets + label)
    let palette_height = ps1_color_picker_with_blend_mode_height() + 20.0; // 72 + 20 for label
    let header_height = 20.0;
    let atlas_area_height = rect.h - palette_height - header_height;

    // Scale to fit panel
    let padding = 4.0;
    let available_w = rect.w - padding * 2.0;
    let available_h = atlas_area_height - padding * 2.0;
    let scale = (available_w / atlas_w).min(available_h / atlas_h);

    let atlas_screen_w = atlas_w * scale;
    let atlas_screen_h = atlas_h * scale;
    let atlas_x = rect.x + (rect.w - atlas_screen_w) * 0.5;
    let atlas_y = rect.y + header_height + padding;

    // Draw checkerboard background (for transparency)
    let checker_size = 8.0;
    let check_cols = (atlas_screen_w / checker_size).ceil() as usize;
    let check_rows = (atlas_screen_h / checker_size).ceil() as usize;
    for cy in 0..check_rows {
        for cx in 0..check_cols {
            let color = if (cx + cy) % 2 == 0 {
                Color::from_rgba(40, 40, 45, 255)
            } else {
                Color::from_rgba(55, 55, 60, 255)
            };
            let px = atlas_x + cx as f32 * checker_size;
            let py = atlas_y + cy as f32 * checker_size;
            let pw = checker_size.min(atlas_x + atlas_screen_w - px);
            let ph = checker_size.min(atlas_y + atlas_screen_h - py);
            if pw > 0.0 && ph > 0.0 {
                draw_rectangle(px, py, pw, ph, color);
            }
        }
    }

    // Draw the actual texture atlas pixels
    let pixels_per_block = (1.0 / scale).max(1.0) as usize;
    for by in (0..atlas_height).step_by(pixels_per_block.max(1)) {
        for bx in (0..atlas_width).step_by(pixels_per_block.max(1)) {
            let pixel = state.project.atlas.get_pixel(bx, by);
            let px = atlas_x + bx as f32 * scale;
            let py = atlas_y + by as f32 * scale;
            let pw = (pixels_per_block as f32 * scale).min(atlas_x + atlas_screen_w - px).max(scale);
            let ph = (pixels_per_block as f32 * scale).min(atlas_y + atlas_screen_h - py).max(scale);
            if pw > 0.0 && ph > 0.0 {
                draw_rectangle(px, py, pw, ph, Color::from_rgba(pixel.r, pixel.g, pixel.b, 255));
            }
        }
    }

    // Draw atlas border
    draw_rectangle_lines(atlas_x, atlas_y, atlas_screen_w, atlas_screen_h, 1.0, Color::from_rgba(100, 100, 105, 255));

    // Helper to convert UV to screen coordinates
    let uv_to_screen = |u: f32, v: f32| -> (f32, f32) {
        let sx = atlas_x + u * atlas_screen_w;
        let sy = atlas_y + (1.0 - v) * atlas_screen_h; // Flip Y for screen coords
        (sx, sy)
    };

    // Helper to convert screen to UV coordinates (for future UV dragging)
    let _screen_to_uv = |sx: f32, sy: f32| -> (f32, f32) {
        let u = (sx - atlas_x) / atlas_screen_w;
        let v = 1.0 - (sy - atlas_y) / atlas_screen_h;
        (u.clamp(0.0, 1.0), v.clamp(0.0, 1.0))
    };

    let atlas_rect = Rect::new(atlas_x, atlas_y, atlas_screen_w, atlas_screen_h);
    let (mx, my) = mouse_position();
    let inside_atlas = atlas_rect.contains(mx, my);

    // Draw UVs of the selected object's faces
    if let Some(obj) = state.project.selected() {
        let face_color = Color::from_rgba(100, 200, 255, 180);
        let selected_color = Color::from_rgba(255, 200, 100, 255);
        let vertex_color = Color::from_rgba(255, 255, 255, 255);

        for (fi, face) in obj.mesh.faces.iter().enumerate() {
            let is_selected = matches!(&state.selection, super::state::ModelerSelection::Faces(faces) if faces.contains(&fi));

            // Get UV coordinates from vertices
            let v0 = &obj.mesh.vertices[face.v0];
            let v1 = &obj.mesh.vertices[face.v1];
            let v2 = &obj.mesh.vertices[face.v2];

            let screen_uvs = [
                uv_to_screen(v0.uv.x, v0.uv.y),
                uv_to_screen(v1.uv.x, v1.uv.y),
                uv_to_screen(v2.uv.x, v2.uv.y),
            ];

            // Draw edges of the UV triangle
            let color = if is_selected { selected_color } else { face_color };
            let line_width = if is_selected { 2.0 } else { 1.0 };
            for i in 0..3 {
                let j = (i + 1) % 3;
                draw_line(
                    screen_uvs[i].0, screen_uvs[i].1,
                    screen_uvs[j].0, screen_uvs[j].1,
                    line_width,
                    color,
                );
            }

            // Draw UV vertices as draggable points (in edit mode)
            if state.view_mode == ViewMode::Texture {
                for (sx, sy) in &screen_uvs {
                    let dot_size = if is_selected { 6.0 } else { 4.0 };
                    draw_rectangle(sx - dot_size * 0.5, sy - dot_size * 0.5, dot_size, dot_size, vertex_color);
                }
            }
        }
    }

    // ========================================================================
    // Interactive UV editing (only in Texture mode)
    // ========================================================================
    if state.view_mode == ViewMode::Texture && inside_atlas {
        // Check if we're near a UV vertex for dragging
        let mut nearest_uv_vertex: Option<(usize, usize, f32, f32)> = None; // (obj_idx, vert_idx, screen_x, screen_y)
        let mut nearest_dist = f32::MAX;
        let click_threshold = 8.0;

        if let Some(obj_idx) = state.project.selected_object {
            if let Some(obj) = state.project.objects.get(obj_idx) {
                for (vi, vert) in obj.mesh.vertices.iter().enumerate() {
                    let (sx, sy) = uv_to_screen(vert.uv.x, vert.uv.y);
                    let dist = ((mx - sx).powi(2) + (my - sy).powi(2)).sqrt();
                    if dist < click_threshold && dist < nearest_dist {
                        nearest_uv_vertex = Some((obj_idx, vi, sx, sy));
                        nearest_dist = dist;
                    }
                }
            }
        }

        // Show hover indicator for UV vertices
        if let Some((_, _, sx, sy)) = nearest_uv_vertex {
            draw_rectangle(sx - 5.0, sy - 5.0, 10.0, 10.0, Color::from_rgba(255, 255, 100, 150));
        }

        // Start UV drag on click near a vertex
        if is_mouse_button_pressed(MouseButton::Left) {
            if let Some((obj_idx, vert_idx, _, _)) = nearest_uv_vertex {
                // Save undo state
                state.push_undo("Move UV");
                state.uv_drag_active = true;
                state.uv_drag_start = (mx, my);

                // Collect all UVs of selected faces' vertices (or just the clicked vertex)
                let mut start_uvs = Vec::new();
                if let Some(obj) = state.project.objects.get(obj_idx) {
                    // If we have face selection, move all UVs of selected faces
                    if let super::state::ModelerSelection::Faces(face_indices) = &state.selection {
                        let mut vert_set = std::collections::HashSet::new();
                        for &fi in face_indices {
                            if let Some(face) = obj.mesh.faces.get(fi) {
                                vert_set.insert(face.v0);
                                vert_set.insert(face.v1);
                                vert_set.insert(face.v2);
                            }
                        }
                        for vi in vert_set {
                            if let Some(v) = obj.mesh.vertices.get(vi) {
                                start_uvs.push((obj_idx, vi, v.uv));
                            }
                        }
                    } else {
                        // Just move the clicked vertex
                        if let Some(v) = obj.mesh.vertices.get(vert_idx) {
                            start_uvs.push((obj_idx, vert_idx, v.uv));
                        }
                    }
                }
                state.uv_drag_start_uvs = start_uvs;
            }
        }

        // Continue UV drag
        if state.uv_drag_active && is_mouse_button_down(MouseButton::Left) {
            let du = (mx - state.uv_drag_start.0) / atlas_screen_w;
            let dv = -(my - state.uv_drag_start.1) / atlas_screen_h; // Flip Y

            for (obj_idx, vert_idx, original_uv) in &state.uv_drag_start_uvs {
                if let Some(obj) = state.project.objects.get_mut(*obj_idx) {
                    if let Some(vert) = obj.mesh.vertices.get_mut(*vert_idx) {
                        vert.uv.x = (original_uv.x + du).clamp(0.0, 1.0);
                        vert.uv.y = (original_uv.y + dv).clamp(0.0, 1.0);
                    }
                }
            }
            // Live sync so viewport shows UV changes in real-time
            state.sync_mesh_from_project();
            state.dirty = true;
        }

        // End UV drag
        if !is_mouse_button_down(MouseButton::Left) && state.uv_drag_active {
            state.uv_drag_active = false;
            state.uv_drag_start_uvs.clear();
            // Sync UV changes to state.mesh so viewport renders them
            state.sync_mesh_from_project();
            state.set_status("UV moved", 0.5);
        }

        // Painting on atlas with left-click (when not near UV vertex to avoid conflict with UV dragging)
        if is_mouse_button_down(MouseButton::Left) && nearest_uv_vertex.is_none() && !state.uv_drag_active {
            let px = ((mx - atlas_x) / scale) as usize;
            let py = ((my - atlas_y) / scale) as usize;
            let color = state.paint_color;
            let brush = state.brush_size as usize;
            for dy in 0..brush {
                for dx in 0..brush {
                    state.project.atlas.set_pixel_blended(px + dx, py + dy, color, state.paint_blend_mode);
                }
            }
            state.dirty = true;
        }

        // Show brush cursor when not near a UV vertex
        if nearest_uv_vertex.is_none() {
            let brush_size = state.brush_size;
            let cursor_x = atlas_x + ((mx - atlas_x) / scale).floor() * scale;
            let cursor_y = atlas_y + ((my - atlas_y) / scale).floor() * scale;
            let cursor_w = brush_size * scale;
            draw_rectangle_lines(cursor_x, cursor_y, cursor_w, cursor_w, 1.0, Color::from_rgba(255, 255, 255, 200));
        }
    }

    // ========================================================================
    // PS1 Color Picker with Blend Mode (PS1 uses discrete modes, not alpha)
    // ========================================================================
    let palette_y = rect.y + rect.h - palette_height;
    draw_line(rect.x, palette_y, rect.x + rect.w, palette_y, 1.0, Color::from_rgba(60, 60, 65, 255));

    // Draw PS1 color picker with blend mode selector
    let picker_result = draw_ps1_color_picker_with_blend_mode(
        ctx,
        rect.x + 4.0,
        palette_y + 14.0,
        rect.w - 8.0,
        state.paint_color,
        state.paint_blend_mode,
        "Paint Color (PS1 15-bit)",
        &mut state.color_picker_slider,
    );

    // Update paint color if changed
    if let Some(new_color) = picker_result.color {
        state.paint_color = new_color;
    }

    // Update blend mode if changed
    if let Some(new_blend) = picker_result.blend_mode {
        state.paint_blend_mode = new_blend;
    }

    // Header info
    draw_text(
        &format!("{}x{}", atlas_width, atlas_height),
        rect.x + 4.0,
        rect.y + 14.0,
        12.0,
        TEXT_DIM,
    );

    // Mode indicator
    let mode_text = if state.view_mode == ViewMode::Texture { "EDIT (V to exit)" } else { "VIEW (V to edit)" };
    let mode_color = if state.view_mode == ViewMode::Texture { ACCENT_COLOR } else { TEXT_DIM };
    let text_width = mode_text.len() as f32 * 6.0;
    draw_text(mode_text, rect.x + rect.w - text_width - 8.0, rect.y + 14.0, 11.0, mode_color);

    // Brush size indicator (in edit mode)
    if state.view_mode == ViewMode::Texture {
        draw_text(
            &format!("Brush: {}px", state.brush_size as i32),
            rect.x + 60.0,
            rect.y + 14.0,
            11.0,
            TEXT_DIM,
        );
    }
}

/// DEPRECATED: Draw the UV Editor panel with the texture atlas and face UVs
/// Kept for reference, replaced by draw_texture_uv_panel
#[allow(dead_code)]
fn draw_uv_editor(_ctx: &mut UiContext, rect: Rect, state: &ModelerState) {
    let atlas = &state.project.atlas;
    let atlas_w = atlas.width as f32;
    let atlas_h = atlas.height as f32;

    // Calculate scale to fit atlas in panel
    let padding = 10.0;
    let available_w = rect.w - padding * 2.0;
    let available_h = rect.h - padding * 2.0 - 16.0; // Reserve space for header
    let scale = (available_w / atlas_w).min(available_h / atlas_h);

    let atlas_screen_w = atlas_w * scale;
    let atlas_screen_h = atlas_h * scale;
    let atlas_x = rect.x + (rect.w - atlas_screen_w) * 0.5;
    let atlas_y = rect.y + padding + 16.0;

    // Draw checkerboard background behind atlas (for transparency)
    let checker_size = 8.0;
    let check_cols = (atlas_screen_w / checker_size).ceil() as usize;
    let check_rows = (atlas_screen_h / checker_size).ceil() as usize;
    for cy in 0..check_rows {
        for cx in 0..check_cols {
            let color = if (cx + cy) % 2 == 0 {
                Color::from_rgba(40, 40, 45, 255)
            } else {
                Color::from_rgba(55, 55, 60, 255)
            };
            let px = atlas_x + cx as f32 * checker_size;
            let py = atlas_y + cy as f32 * checker_size;
            let pw = checker_size.min(atlas_x + atlas_screen_w - px);
            let ph = checker_size.min(atlas_y + atlas_screen_h - py);
            if pw > 0.0 && ph > 0.0 {
                draw_rectangle(px, py, pw, ph, color);
            }
        }
    }

    // Draw the actual texture atlas pixels
    // For performance, we draw in blocks instead of pixel-by-pixel
    let block_size = (scale * 4.0).max(1.0); // Larger blocks for zoomed-out view
    let pixels_per_block = (block_size / scale).max(1.0) as usize;

    for by in (0..atlas.height).step_by(pixels_per_block) {
        for bx in (0..atlas.width).step_by(pixels_per_block) {
            // Sample the pixel (or average a block for downsampled view)
            let pixel = atlas.get_pixel(bx, by);
            let px = atlas_x + bx as f32 * scale;
            let py = atlas_y + by as f32 * scale;
            let pw = (pixels_per_block as f32 * scale).min(atlas_x + atlas_screen_w - px);
            let ph = (pixels_per_block as f32 * scale).min(atlas_y + atlas_screen_h - py);
            if pw > 0.0 && ph > 0.0 {
                draw_rectangle(px, py, pw, ph, Color::from_rgba(pixel.r, pixel.g, pixel.b, 255));
            }
        }
    }

    // Draw atlas border
    draw_rectangle_lines(atlas_x, atlas_y, atlas_screen_w, atlas_screen_h, 1.0, Color::from_rgba(100, 100, 105, 255));

    // Draw UVs of the selected object's faces
    if let Some(obj) = state.project.selected() {
        let face_color = Color::from_rgba(100, 200, 255, 180);
        let selected_color = Color::from_rgba(255, 200, 100, 255);

        for (fi, face) in obj.mesh.faces.iter().enumerate() {
            let is_selected = matches!(&state.selection, super::state::ModelerSelection::Faces(faces) if faces.contains(&fi));

            // Get UV coordinates from vertices
            let v0 = &obj.mesh.vertices[face.v0];
            let v1 = &obj.mesh.vertices[face.v1];
            let v2 = &obj.mesh.vertices[face.v2];

            // Convert UVs to screen coordinates
            let uv_to_screen = |uv: crate::rasterizer::Vec2| {
                let sx = atlas_x + uv.x * atlas_screen_w;
                let sy = atlas_y + (1.0 - uv.y) * atlas_screen_h; // Flip Y for screen coords
                (sx, sy)
            };

            let screen_uvs = [
                uv_to_screen(v0.uv),
                uv_to_screen(v1.uv),
                uv_to_screen(v2.uv),
            ];

            // Draw edges of the UV triangle
            let color = if is_selected { selected_color } else { face_color };
            for i in 0..3 {
                let j = (i + 1) % 3;
                draw_line(
                    screen_uvs[i].0, screen_uvs[i].1,
                    screen_uvs[j].0, screen_uvs[j].1,
                    if is_selected { 2.0 } else { 1.0 },
                    color,
                );
            }

            // Draw UV vertices
            for (sx, sy) in &screen_uvs {
                let dot_size = if is_selected { 4.0 } else { 2.0 };
                draw_rectangle(sx - dot_size * 0.5, sy - dot_size * 0.5, dot_size, dot_size, color);
            }
        }
    }

    // Header showing atlas dimensions
    draw_text(
        &format!("Atlas: {}x{}", atlas.width, atlas.height),
        rect.x + 4.0,
        rect.y + 12.0,
        12.0,
        TEXT_DIM,
    );

    // Mode indicator
    let mode_text = if state.view_mode == ViewMode::Texture { "EDIT MODE" } else { "VIEW ONLY" };
    let mode_color = if state.view_mode == ViewMode::Texture { ACCENT_COLOR } else { TEXT_DIM };
    draw_text(mode_text, rect.x + rect.w - 70.0, rect.y + 12.0, 11.0, mode_color);
}

/// Draw PicoCAD-style 4-panel viewport layout with resizable splits
/// ┌─────────────┬─────────────┐
/// │   3D View   │   Top (XZ)  │
/// │ (Perspective)│             │
/// ├─────────────┼─────────────┤
/// │  Front (XY) │  Side (YZ)  │
/// │             │             │
/// └─────────────┴─────────────┘
fn draw_4panel_viewport(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, fb: &mut Framebuffer) {
    let gap = 2.0; // Gap between panels
    let divider_hit_size = 6.0; // Hit area for dragging dividers

    // Check for fullscreen mode (Space key toggles)
    if let Some(fullscreen_id) = state.fullscreen_viewport {
        // Draw single viewport fullscreen
        let content = rect.pad(1.0);
        draw_single_viewport(ctx, content, state, fb, fullscreen_id);
        return;
    }

    // Calculate split positions using state ratios
    let h_split = state.viewport_h_split.clamp(0.15, 0.85);
    let v_split = state.viewport_v_split.clamp(0.15, 0.85);

    let left_w = (rect.w - gap) * h_split;
    let right_w = (rect.w - gap) * (1.0 - h_split);
    let top_h = (rect.h - gap) * v_split;
    let bottom_h = (rect.h - gap) * (1.0 - v_split);

    let viewports = [
        (ViewportId::Perspective, Rect::new(rect.x, rect.y, left_w, top_h)),
        (ViewportId::Top, Rect::new(rect.x + left_w + gap, rect.y, right_w, top_h)),
        (ViewportId::Front, Rect::new(rect.x, rect.y + top_h + gap, left_w, bottom_h)),
        (ViewportId::Side, Rect::new(rect.x + left_w + gap, rect.y + top_h + gap, right_w, bottom_h)),
    ];

    // Handle divider dragging
    let h_divider_rect = Rect::new(rect.x + left_w - divider_hit_size/2.0, rect.y, gap + divider_hit_size, rect.h);
    let v_divider_rect = Rect::new(rect.x, rect.y + top_h - divider_hit_size/2.0, rect.w, gap + divider_hit_size);

    // Check if dragging dividers
    if ctx.mouse.left_down && ctx.mouse.inside(&h_divider_rect) {
        state.viewport_h_split = ((ctx.mouse.x - rect.x) / rect.w).clamp(0.15, 0.85);
    }
    if ctx.mouse.left_down && ctx.mouse.inside(&v_divider_rect) {
        state.viewport_v_split = ((ctx.mouse.y - rect.y) / rect.h).clamp(0.15, 0.85);
    }

    // Update active viewport based on mouse hover (only if not on divider)
    let on_divider = ctx.mouse.inside(&h_divider_rect) || ctx.mouse.inside(&v_divider_rect);
    if !on_divider {
        for (id, vp_rect) in &viewports {
            if ctx.mouse.inside(vp_rect) {
                state.active_viewport = *id;
                break;
            }
        }
    }

    // Draw each viewport
    for (id, vp_rect) in viewports {
        draw_single_viewport(ctx, vp_rect, state, fb, id);
    }
}

/// Draw a single viewport with its label and border
fn draw_single_viewport(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, fb: &mut Framebuffer, viewport_id: ViewportId) {
    let is_active = state.active_viewport == viewport_id;

    // Background
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(25, 25, 30, 255));

    // Border (highlighted if active)
    let border_color = if is_active {
        ACCENT_COLOR
    } else {
        Color::from_rgba(60, 60, 65, 255)
    };
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, border_color);

    // Content area (inset for border)
    let content = rect.pad(1.0);

    // Draw the actual 3D content
    if viewport_id == ViewportId::Perspective {
        // Perspective view uses the existing orbit camera
        draw_modeler_viewport(ctx, content, state, fb);
    } else {
        // Ortho views use rasterizer with orthographic projection
        draw_ortho_viewport(ctx, content, state, viewport_id, fb);
    }

    // Label in top-left corner
    let label = viewport_id.label();
    let label_bg = Rect::new(rect.x + 2.0, rect.y + 2.0, label.len() as f32 * 7.0 + 8.0, 16.0);
    draw_rectangle(label_bg.x, label_bg.y, label_bg.w, label_bg.h, Color::from_rgba(0, 0, 0, 180));
    draw_text(label, label_bg.x + 4.0, label_bg.y + 12.0, 12.0, TEXT_COLOR);

    // Show view mode indicator if in texture mode
    if state.view_mode == ViewMode::Texture {
        let mode_label = "UV";
        let mode_bg = Rect::new(rect.right() - 28.0, rect.y + 2.0, 24.0, 16.0);
        draw_rectangle(mode_bg.x, mode_bg.y, mode_bg.w, mode_bg.h, Color::from_rgba(100, 50, 150, 200));
        draw_text(mode_label, mode_bg.x + 4.0, mode_bg.y + 12.0, 12.0, TEXT_COLOR);
    }
}

/// Calculate distance from point to line segment (for edge hover detection)
fn point_to_line_dist(px: f32, py: f32, x0: f32, y0: f32, x1: f32, y1: f32) -> f32 {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len_sq = dx * dx + dy * dy;

    if len_sq < 0.001 {
        return ((px - x0).powi(2) + (py - y0).powi(2)).sqrt();
    }

    let t = ((px - x0) * dx + (py - y0) * dy) / len_sq;
    let t = t.clamp(0.0, 1.0);

    let proj_x = x0 + t * dx;
    let proj_y = y0 + t * dy;

    ((px - proj_x).powi(2) + (py - proj_y).powi(2)).sqrt()
}

/// Draw an orthographic viewport (top/front/side view)
fn draw_ortho_viewport(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, viewport_id: ViewportId, fb: &mut Framebuffer) {
    let ortho_cam = state.get_ortho_camera(viewport_id);
    let zoom = ortho_cam.zoom;
    let center = ortho_cam.center;

    // Draw grid
    let grid_size = state.snap_settings.grid_size;
    let grid_color = Color::from_rgba(45, 45, 50, 255);
    let axis_color = Color::from_rgba(80, 80, 85, 255);

    // Calculate visible range in world units
    let half_w = rect.w / (2.0 * zoom);
    let half_h = rect.h / (2.0 * zoom);

    // World to screen helper for this ortho view
    let world_to_ortho = |wx: f32, wy: f32| -> (f32, f32) {
        let sx = rect.center_x() + (wx - center.x) * zoom;
        let sy = rect.center_y() - (wy - center.y) * zoom; // Y flipped for screen coords
        (sx, sy)
    };

    // Draw grid lines
    let start_x = ((center.x - half_w) / grid_size).floor() as i32;
    let end_x = ((center.x + half_w) / grid_size).ceil() as i32;
    let start_y = ((center.y - half_h) / grid_size).floor() as i32;
    let end_y = ((center.y + half_h) / grid_size).ceil() as i32;

    // Vertical lines
    for i in start_x..=end_x {
        let wx = i as f32 * grid_size;
        let (sx, _) = world_to_ortho(wx, 0.0);
        if sx >= rect.x && sx <= rect.right() {
            let color = if i == 0 { axis_color } else { grid_color };
            draw_line(sx, rect.y, sx, rect.bottom(), 1.0, color);
        }
    }

    // Horizontal lines
    for i in start_y..=end_y {
        let wy = i as f32 * grid_size;
        let (_, sy) = world_to_ortho(0.0, wy);
        if sy >= rect.y && sy <= rect.bottom() {
            let color = if i == 0 { axis_color } else { grid_color };
            draw_line(rect.x, sy, rect.right(), sy, 1.0, color);
        }
    }

    // Helper: project 3D vertex to ortho screen coords
    let project_vertex = |v: &crate::rasterizer::Vertex| -> (f32, f32) {
        match viewport_id {
            ViewportId::Top => world_to_ortho(v.pos.x, v.pos.z),    // XZ plane, looking down Y
            ViewportId::Front => world_to_ortho(v.pos.x, v.pos.y),  // XY plane, looking down Z
            ViewportId::Side => world_to_ortho(v.pos.z, v.pos.y),   // ZY plane, looking down X
            ViewportId::Perspective => (0.0, 0.0), // Shouldn't happen
        }
    };

    let mesh = &state.mesh;
    let mouse_pos = (ctx.mouse.x, ctx.mouse.y);
    let inside_viewport = ctx.mouse.inside(&rect);

    // =========================================================================
    // Hover detection for ortho views (same priority as world editor: vertex > edge > face)
    // =========================================================================
    let mut ortho_hovered_vertex: Option<usize> = None;
    let mut ortho_hovered_edge: Option<(usize, usize)> = None;
    let mut ortho_hovered_face: Option<usize> = None;

    if inside_viewport && state.active_viewport == viewport_id && !state.transform_active && state.modal_transform == ModalTransform::None && !state.box_select_active {
        const VERTEX_THRESHOLD: f32 = 10.0;
        const EDGE_THRESHOLD: f32 = 6.0;
        const FACE_THRESHOLD: f32 = 20.0;

        // Check vertices
        let mut best_vert_dist = VERTEX_THRESHOLD;
        for (idx, vert) in mesh.vertices.iter().enumerate() {
            let (sx, sy) = project_vertex(vert);
            if sx >= rect.x && sx <= rect.right() && sy >= rect.y && sy <= rect.bottom() {
                let dist = ((mouse_pos.0 - sx).powi(2) + (mouse_pos.1 - sy).powi(2)).sqrt();
                if dist < best_vert_dist {
                    best_vert_dist = dist;
                    ortho_hovered_vertex = Some(idx);
                }
            }
        }

        // Check edges if no vertex hovered
        if ortho_hovered_vertex.is_none() {
            let mut best_edge_dist = EDGE_THRESHOLD;
            for face in &mesh.faces {
                let edges = [(face.v0, face.v1), (face.v1, face.v2), (face.v2, face.v0)];
                for (v0_idx, v1_idx) in edges {
                    if let (Some(v0), Some(v1)) = (mesh.vertices.get(v0_idx), mesh.vertices.get(v1_idx)) {
                        let (sx0, sy0) = project_vertex(v0);
                        let (sx1, sy1) = project_vertex(v1);
                        let dist = point_to_line_dist(mouse_pos.0, mouse_pos.1, sx0, sy0, sx1, sy1);
                        if dist < best_edge_dist {
                            best_edge_dist = dist;
                            ortho_hovered_edge = Some(if v0_idx < v1_idx { (v0_idx, v1_idx) } else { (v1_idx, v0_idx) });
                        }
                    }
                }
            }
        }

        // Check faces if no vertex or edge hovered
        if ortho_hovered_vertex.is_none() && ortho_hovered_edge.is_none() {
            let mut best_face_dist = FACE_THRESHOLD;
            for (idx, face) in mesh.faces.iter().enumerate() {
                if let (Some(v0), Some(v1), Some(v2)) = (
                    mesh.vertices.get(face.v0),
                    mesh.vertices.get(face.v1),
                    mesh.vertices.get(face.v2),
                ) {
                    // Face center
                    let center_pos = crate::rasterizer::Vertex {
                        pos: (v0.pos + v1.pos + v2.pos) * (1.0 / 3.0),
                        ..v0.clone()
                    };
                    let (sx, sy) = project_vertex(&center_pos);
                    let dist = ((mouse_pos.0 - sx).powi(2) + (mouse_pos.1 - sy).powi(2)).sqrt();
                    if dist < best_face_dist {
                        best_face_dist = dist;
                        ortho_hovered_face = Some(idx);
                    }
                }
            }
        }

        // Update global hover state if this is the active viewport
        state.hovered_vertex = ortho_hovered_vertex;
        state.hovered_edge = ortho_hovered_edge;
        state.hovered_face = ortho_hovered_face;
    }

    // =========================================================================
    // Draw mesh in ortho view using rasterizer with proper ortho camera
    // =========================================================================
    let hover_color = Color::from_rgba(255, 200, 150, 255);   // Orange for hover
    let select_color = Color::from_rgba(100, 180, 255, 255);  // Blue for selection
    let wire_color = Color::from_rgba(150, 150, 160, 255);
    let vertex_color = Color::from_rgba(180, 180, 190, 255);
    let wireframe_mode = state.raster_settings.wireframe_overlay;

    if !mesh.vertices.is_empty() && !wireframe_mode {
        // Create ortho camera for this view direction
        let ortho_camera = match viewport_id {
            ViewportId::Top => Camera::ortho_top(),
            ViewportId::Front => Camera::ortho_front(),
            ViewportId::Side => Camera::ortho_side(),
            ViewportId::Perspective => unreachable!(),
        };

        // Build rasterizer vertex/face data
        let vertices: Vec<RasterVertex> = mesh.vertices.iter().map(|v| {
            RasterVertex {
                pos: v.pos,
                normal: v.normal,
                uv: v.uv,
                color: RasterColor::new(180, 180, 180),
                bone_index: None,
            }
        }).collect();

        let faces: Vec<RasterFace> = mesh.faces.iter().map(|f| {
            RasterFace {
                v0: f.v0,
                v1: f.v1,
                v2: f.v2,
                texture_id: Some(0),
            }
        }).collect();

        if !vertices.is_empty() && !faces.is_empty() {
            // Resize framebuffer to match viewport
            let fb_w = (rect.w as usize).max(1);
            let fb_h = (rect.h as usize).max(1);
            fb.resize(fb_w, fb_h);

            // Clear with transparent so grid shows through
            fb.clear_transparent();

            // Set up ortho projection - center is the pan offset
            let mut ortho_settings = state.raster_settings.clone();
            ortho_settings.ortho_projection = Some(OrthoProjection {
                zoom,
                center_x: center.x,
                center_y: center.y,
            });
            ortho_settings.backface_cull = false; // Show all faces in ortho views
            ortho_settings.backface_wireframe = false;

            // Convert atlas to texture
            let atlas_texture = state.project.atlas.to_raster_texture();
            let textures = [atlas_texture];

            render_mesh(fb, &vertices, &faces, &textures, &ortho_camera, &ortho_settings);

            // Blit framebuffer to screen with alpha blending
            let texture = Texture2D::from_rgba8(fb.width as u16, fb.height as u16, &fb.pixels);
            texture.set_filter(FilterMode::Nearest);
            draw_texture_ex(
                &texture,
                rect.x,
                rect.y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(rect.w, rect.h)),
                    ..Default::default()
                },
            );
        }
    }

    if !mesh.vertices.is_empty() {
        // Draw wireframe edges
        // In wireframe mode: draw all edges
        // In solid mode: only draw hovered/selected faces
        for (idx, face) in mesh.faces.iter().enumerate() {
            let is_hovered = state.hovered_face == Some(idx);
            let is_selected = matches!(&state.selection, super::state::ModelerSelection::Faces(f) if f.contains(&idx));

            // In solid mode, skip unselected/unhovered faces
            if !wireframe_mode && !is_hovered && !is_selected {
                continue;
            }

            if let (Some(v0), Some(v1), Some(v2)) = (
                mesh.vertices.get(face.v0),
                mesh.vertices.get(face.v1),
                mesh.vertices.get(face.v2),
            ) {
                let (x0, y0) = project_vertex(v0);
                let (x1, y1) = project_vertex(v1);
                let (x2, y2) = project_vertex(v2);

                // Choose color based on hover/selection
                let color = if is_hovered { hover_color } else if is_selected { select_color } else { wire_color };
                let thickness = if is_hovered || is_selected { 2.0 } else { 1.0 };

                draw_line(x0, y0, x1, y1, thickness, color);
                draw_line(x1, y1, x2, y2, thickness, color);
                draw_line(x2, y2, x0, y0, thickness, color);
            }
        }

        // Draw hovered edge (on top of faces)
        if let Some((v0_idx, v1_idx)) = state.hovered_edge {
            if let (Some(v0), Some(v1)) = (mesh.vertices.get(v0_idx), mesh.vertices.get(v1_idx)) {
                let (x0, y0) = project_vertex(v0);
                let (x1, y1) = project_vertex(v1);
                draw_line(x0, y0, x1, y1, 3.0, hover_color);
            }
        }

        // Draw selected edges
        if let super::state::ModelerSelection::Edges(selected_edges) = &state.selection {
            for (v0_idx, v1_idx) in selected_edges {
                if let (Some(v0), Some(v1)) = (mesh.vertices.get(*v0_idx), mesh.vertices.get(*v1_idx)) {
                    let (x0, y0) = project_vertex(v0);
                    let (x1, y1) = project_vertex(v1);
                    draw_line(x0, y0, x1, y1, 2.5, select_color);
                }
            }
        }

        // Draw vertices
        // In wireframe mode: draw all vertices
        // In solid mode: only draw hovered/selected vertices
        for (idx, vert) in mesh.vertices.iter().enumerate() {
            let is_hovered = state.hovered_vertex == Some(idx);
            let is_selected = matches!(&state.selection, super::state::ModelerSelection::Vertices(v) if v.contains(&idx));

            // In solid mode, skip unselected/unhovered vertices
            if !wireframe_mode && !is_hovered && !is_selected {
                continue;
            }

            let (x, y) = project_vertex(vert);

            // Only draw if in viewport
            if x >= rect.x && x <= rect.right() && y >= rect.y && y <= rect.bottom() {
                let color = if is_hovered { hover_color } else if is_selected { select_color } else { vertex_color };
                let radius = if is_hovered { 5.0 } else if is_selected { 4.0 } else { 3.0 };
                draw_circle(x, y, radius, color);
            }
        }
    }

    // =========================================================================
    // Handle click to select in ortho views
    // =========================================================================
    if inside_viewport && state.active_viewport == viewport_id && is_mouse_button_pressed(MouseButton::Left) && state.modal_transform == ModalTransform::None && !state.box_select_active {
        let multi_select = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) || is_key_down(KeyCode::X);

        if let Some(vert_idx) = state.hovered_vertex {
            if multi_select {
                match &mut state.selection {
                    super::state::ModelerSelection::Vertices(verts) => {
                        if let Some(pos) = verts.iter().position(|&v| v == vert_idx) {
                            verts.remove(pos);
                        } else {
                            verts.push(vert_idx);
                        }
                    }
                    _ => state.selection = super::state::ModelerSelection::Vertices(vec![vert_idx]),
                }
            } else {
                state.selection = super::state::ModelerSelection::Vertices(vec![vert_idx]);
            }
            state.select_mode = SelectMode::Vertex;
        } else if let Some((v0, v1)) = state.hovered_edge {
            if multi_select {
                match &mut state.selection {
                    super::state::ModelerSelection::Edges(edges) => {
                        if let Some(pos) = edges.iter().position(|e| *e == (v0, v1) || *e == (v1, v0)) {
                            edges.remove(pos);
                        } else {
                            edges.push((v0, v1));
                        }
                    }
                    _ => state.selection = super::state::ModelerSelection::Edges(vec![(v0, v1)]),
                }
            } else {
                state.selection = super::state::ModelerSelection::Edges(vec![(v0, v1)]);
            }
            state.select_mode = SelectMode::Edge;
        } else if let Some(face_idx) = state.hovered_face {
            if multi_select {
                match &mut state.selection {
                    super::state::ModelerSelection::Faces(faces) => {
                        if let Some(pos) = faces.iter().position(|&f| f == face_idx) {
                            faces.remove(pos);
                        } else {
                            faces.push(face_idx);
                        }
                    }
                    _ => state.selection = super::state::ModelerSelection::Faces(vec![face_idx]),
                }
            } else {
                state.selection = super::state::ModelerSelection::Faces(vec![face_idx]);
            }
            state.select_mode = SelectMode::Face;
        } else if !is_key_down(KeyCode::X) {
            // Clicked on nothing - clear selection
            state.selection = super::state::ModelerSelection::None;
        }
    }

    // =========================================================================
    // Handle left-drag to move selection in ortho views
    // =========================================================================
    if inside_viewport && state.active_viewport == viewport_id && !state.selection.is_empty() && state.modal_transform == ModalTransform::None {
        // Get zoom value before any mutable borrows
        let ortho_zoom = state.get_ortho_camera(viewport_id).zoom;

        // Screen to world delta helper for this ortho view
        let screen_to_world_delta = |dx: f32, dy: f32| -> crate::rasterizer::Vec3 {
            let world_dx = dx / ortho_zoom;
            let world_dy = -dy / ortho_zoom; // Y inverted

            match viewport_id {
                ViewportId::Top => crate::rasterizer::Vec3::new(world_dx, 0.0, world_dy),    // XZ plane
                ViewportId::Front => crate::rasterizer::Vec3::new(world_dx, world_dy, 0.0),  // XY plane
                ViewportId::Side => crate::rasterizer::Vec3::new(0.0, world_dy, world_dx),   // ZY plane
                ViewportId::Perspective => crate::rasterizer::Vec3::ZERO,
            }
        };

        // Start drag on left-down (when we have selection and not clicking to select)
        if ctx.mouse.left_down && !state.transform_active {
            let dx = (ctx.mouse.x - state.viewport_last_mouse.0).abs();
            let dy = (ctx.mouse.y - state.viewport_last_mouse.1).abs();
            // Only start drag if we've moved a bit (distinguish from click-to-select)
            if dx > 3.0 || dy > 3.0 {
                // Start transform
                state.transform_active = true;
                state.transform_start_mouse = (ctx.mouse.x, ctx.mouse.y);

                // Collect starting positions
                let mut indices = state.selection.get_affected_vertex_indices(&state.mesh);
                if state.vertex_linking {
                    indices = state.mesh.expand_to_coincident(&indices, 0.001);
                }
                state.transform_start_positions = indices.iter()
                    .filter_map(|&idx| state.mesh.vertices.get(idx).map(|v| v.pos))
                    .collect();
            }
        }

        // Continue drag
        if state.transform_active && ctx.mouse.left_down {
            let delta = screen_to_world_delta(
                ctx.mouse.x - state.transform_start_mouse.0,
                ctx.mouse.y - state.transform_start_mouse.1,
            );

            let mut indices = state.selection.get_affected_vertex_indices(&state.mesh);
            if state.vertex_linking {
                indices = state.mesh.expand_to_coincident(&indices, 0.001);
            }

            for (i, &idx) in indices.iter().enumerate() {
                if let (Some(vert), Some(&start)) = (state.mesh.vertices.get_mut(idx), state.transform_start_positions.get(i)) {
                    vert.pos = start + delta;

                    // Apply grid snapping if enabled
                    if state.snap_settings.enabled && !is_key_down(KeyCode::Z) {
                        let snap = state.snap_settings.grid_size;
                        vert.pos.x = (vert.pos.x / snap).round() * snap;
                        vert.pos.y = (vert.pos.y / snap).round() * snap;
                        vert.pos.z = (vert.pos.z / snap).round() * snap;
                    }
                }
            }
            state.dirty = true;
        }

        // End drag
        if !ctx.mouse.left_down && state.transform_active {
            if !state.transform_start_positions.is_empty() {
                state.push_undo("Ortho Move");
                state.sync_mesh_to_project();
            }
            state.transform_active = false;
            state.transform_start_positions.clear();
        }
    }

    // Handle ortho viewport input (pan/zoom)
    if inside_viewport {
        // Mouse wheel zoom
        let scroll = ctx.mouse.scroll;
        if scroll != 0.0 {
            let zoom_factor = if scroll > 0.0 { 1.1 } else { 0.9 };
            let ortho_cam = state.get_ortho_camera_mut(viewport_id);
            ortho_cam.zoom = (ortho_cam.zoom * zoom_factor).clamp(0.1, 20.0);
        }
    }

    // Right-drag to pan (using separate ortho_last_mouse to avoid conflict with perspective view)
    let is_our_pan = state.ortho_pan_viewport == Some(viewport_id);

    if ctx.mouse.right_down && (inside_viewport || is_our_pan) {
        if is_our_pan {
            // Continue panning - apply delta using ortho-specific last mouse
            let dx = ctx.mouse.x - state.ortho_last_mouse.0;
            let dy = ctx.mouse.y - state.ortho_last_mouse.1;
            let ortho_cam = state.get_ortho_camera_mut(viewport_id);
            ortho_cam.center.x -= dx / ortho_cam.zoom;
            ortho_cam.center.y += dy / ortho_cam.zoom; // Y inverted
        }
        // Capture this viewport for panning
        if inside_viewport && state.ortho_pan_viewport.is_none() {
            state.ortho_pan_viewport = Some(viewport_id);
        }
        // Always update ortho last mouse while panning
        state.ortho_last_mouse = (ctx.mouse.x, ctx.mouse.y);
    } else if !ctx.mouse.right_down && is_our_pan {
        // Release pan capture
        state.ortho_pan_viewport = None;
    }
}

fn draw_viewport(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, fb: &mut Framebuffer) {
    draw_modeler_viewport(ctx, rect, state, fb);
}

/// DEPRECATED: Draw the Atlas panel with the actual texture and painting support
/// Kept for reference, replaced by draw_texture_uv_panel
#[allow(dead_code)]
fn draw_atlas_panel(_ctx: &mut UiContext, rect: Rect, state: &mut ModelerState) {
    // Cache atlas dimensions to avoid borrow issues
    let atlas_width = state.project.atlas.width;
    let atlas_height = state.project.atlas.height;
    let atlas_w = atlas_width as f32;
    let atlas_h = atlas_height as f32;

    // Reserve space for color palette at bottom
    let palette_height = 50.0;
    let atlas_area_height = rect.h - palette_height - 24.0; // Also reserve space for title

    // Scale to fit panel
    let padding = 4.0;
    let available_w = rect.w - padding * 2.0;
    let available_h = atlas_area_height - padding * 2.0;
    let scale = (available_w / atlas_w).min(available_h / atlas_h);

    let atlas_screen_w = atlas_w * scale;
    let atlas_screen_h = atlas_h * scale;
    let atlas_x = rect.x + (rect.w - atlas_screen_w) * 0.5;
    let atlas_y = rect.y + padding;

    // Draw the actual texture atlas
    let pixels_per_block = (1.0 / scale).max(1.0) as usize;
    for by in (0..atlas_height).step_by(pixels_per_block.max(1)) {
        for bx in (0..atlas_width).step_by(pixels_per_block.max(1)) {
            let pixel = state.project.atlas.get_pixel(bx, by);
            let px = atlas_x + bx as f32 * scale;
            let py = atlas_y + by as f32 * scale;
            let pw = (pixels_per_block as f32 * scale).min(atlas_x + atlas_screen_w - px).max(scale);
            let ph = (pixels_per_block as f32 * scale).min(atlas_y + atlas_screen_h - py).max(scale);
            if pw > 0.0 && ph > 0.0 {
                draw_rectangle(px, py, pw, ph, Color::from_rgba(pixel.r, pixel.g, pixel.b, 255));
            }
        }
    }

    // Draw atlas border
    draw_rectangle_lines(atlas_x, atlas_y, atlas_screen_w, atlas_screen_h, 1.0, Color::from_rgba(100, 100, 105, 255));

    // Handle painting in texture mode
    let (mx, my) = mouse_position();
    let atlas_rect = Rect::new(atlas_x, atlas_y, atlas_screen_w, atlas_screen_h);

    if state.view_mode == super::state::ViewMode::Texture && atlas_rect.contains(mx, my) {
        // Convert mouse position to atlas pixel coordinates
        let px = ((mx - atlas_x) / scale) as usize;
        let py = ((my - atlas_y) / scale) as usize;

        // Draw cursor preview
        let brush_size = state.brush_size;
        let cursor_x = atlas_x + (px as f32) * scale;
        let cursor_y = atlas_y + (py as f32) * scale;
        let cursor_w = brush_size * scale;
        draw_rectangle_lines(cursor_x, cursor_y, cursor_w, cursor_w, 1.0, Color::from_rgba(255, 255, 255, 200));

        // Paint on click/drag
        if is_mouse_button_down(MouseButton::Left) {
            let color = state.paint_color;
            let brush = brush_size as usize;
            for dy in 0..brush {
                for dx in 0..brush {
                    state.project.atlas.set_pixel(px + dx, py + dy, color);
                }
            }
            state.dirty = true;
        }
    }

    // Draw color palette
    let palette_y = rect.y + rect.h - palette_height;
    draw_line(rect.x, palette_y, rect.x + rect.w, palette_y, 1.0, Color::from_rgba(60, 60, 65, 255));

    // PS1-style limited palette (16 colors)
    let palette: [(u8, u8, u8); 16] = [
        (0, 0, 0),       // Black
        (255, 255, 255), // White
        (128, 128, 128), // Gray
        (64, 64, 64),    // Dark gray
        (255, 0, 0),     // Red
        (0, 255, 0),     // Green
        (0, 0, 255),     // Blue
        (255, 255, 0),   // Yellow
        (255, 0, 255),   // Magenta
        (0, 255, 255),   // Cyan
        (255, 128, 0),   // Orange
        (128, 0, 255),   // Purple
        (255, 128, 128), // Light red
        (128, 255, 128), // Light green
        (128, 128, 255), // Light blue
        (192, 192, 192), // Light gray
    ];

    let swatch_size = (rect.w - 16.0) / 8.0;
    let swatch_y = palette_y + 8.0;

    for (i, (r, g, b)) in palette.iter().enumerate() {
        let col = i % 8;
        let row = i / 8;
        let sx = rect.x + 8.0 + col as f32 * swatch_size;
        let sy = swatch_y + row as f32 * swatch_size;

        let swatch_color = Color::from_rgba(*r, *g, *b, 255);
        draw_rectangle(sx, sy, swatch_size - 2.0, swatch_size - 2.0, swatch_color);

        // Highlight current color
        let is_current = state.paint_color.r == *r
            && state.paint_color.g == *g
            && state.paint_color.b == *b;
        if is_current {
            draw_rectangle_lines(sx - 1.0, sy - 1.0, swatch_size, swatch_size, 2.0, WHITE);
        }

        // Handle click to select color
        let swatch_rect = Rect::new(sx, sy, swatch_size - 2.0, swatch_size - 2.0);
        if is_mouse_button_pressed(MouseButton::Left) && swatch_rect.contains(mx, my) {
            state.paint_color = crate::rasterizer::Color::new(*r, *g, *b);
        }
    }

    // Size label
    draw_text(
        &format!("{}x{}", atlas_width, atlas_height),
        rect.x + 4.0,
        atlas_y + atlas_screen_h + 14.0,
        11.0,
        TEXT_DIM,
    );

    // Brush size indicator
    draw_text(
        &format!("Brush: {}px", state.brush_size as i32),
        rect.x + rect.w - 60.0,
        atlas_y + atlas_screen_h + 14.0,
        11.0,
        TEXT_DIM,
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
        super::state::ModelerSelection::Mesh => {
            draw_text("Mesh (whole)", rect.x, y + 14.0, 12.0, TEXT_COLOR);
        }
        super::state::ModelerSelection::Vertices(verts) => {
            draw_text(&format!("{} vertex(es)", verts.len()), rect.x, y + 14.0, 12.0, TEXT_COLOR);
        }
        super::state::ModelerSelection::Edges(edges) => {
            draw_text(&format!("{} edge(s)", edges.len()), rect.x, y + 14.0, 12.0, TEXT_COLOR);
        }
        super::state::ModelerSelection::Faces(faces) => {
            draw_text(&format!("{} face(s)", faces.len()), rect.x, y + 14.0, 12.0, TEXT_COLOR);
        }
    }

    y += line_height * 2.0;

    // Tool info
    draw_text("Tool:", rect.x, y + 14.0, 12.0, TEXT_DIM);
    y += line_height;
    draw_text(state.tool.label(), rect.x, y + 14.0, 12.0, TEXT_COLOR);

    y += line_height * 2.0;

    // Keyboard shortcuts help
    draw_text("Shortcuts:", rect.x, y + 14.0, 12.0, TEXT_DIM);
    y += line_height;

    let shortcuts = [
        ("Arrows", "Move selection"),
        ("Z+Arrows", "Move (free)"),
        ("E", "Extrude face"),
        ("G", "Move (Grab)"),
        ("R", "Rotate"),
        ("S", "Scale"),
        ("X/Del", "Delete"),
        ("1/2/3", "Vert/Edge/Face"),
        ("V", "Toggle Build/UV"),
        ("Space", "Fullscreen"),
    ];

    for (key, desc) in shortcuts {
        if y + line_height > rect.bottom() {
            break;
        }
        draw_text(&format!("{}: {}", key, desc), rect.x, y + 12.0, 10.0, TEXT_DIM);
        y += line_height * 0.8;
    }
    y += line_height;

    // Lights section
    if y + line_height * 3.0 < rect.bottom() {
        draw_text("Lights:", rect.x, y + 14.0, 12.0, TEXT_DIM);
        y += line_height;

        // Show light count and add/remove buttons
        let light_count = state.raster_settings.lights.len();
        draw_text(&format!("{} light(s)", light_count), rect.x, y + 14.0, 12.0, TEXT_COLOR);
        y += line_height;

        // Add light button
        let btn_size = 20.0;
        let add_rect = Rect::new(rect.x, y, btn_size, btn_size);
        if icon_button(ctx, add_rect, icon::PLUS, icon_font, "Add directional light") {
            use crate::rasterizer::{Light, LightType, Vec3};
            let new_light = Light {
                light_type: LightType::Directional { direction: Vec3::new(-1.0, -1.0, -1.0).normalize() },
                color: crate::rasterizer::Color::new(255, 255, 255),
                intensity: 0.5,
                enabled: true,
                name: format!("Light {}", light_count + 1),
            };
            state.raster_settings.lights.push(new_light);
            state.set_status(&format!("Added light {}", light_count + 1), 1.0);
        }

        // Remove last light button
        let remove_rect = Rect::new(rect.x + btn_size + 4.0, y, btn_size, btn_size);
        if icon_button(ctx, remove_rect, icon::MINUS, icon_font, "Remove last light") && light_count > 0 {
            state.raster_settings.lights.pop();
            state.set_status(&format!("Removed light (now {})", light_count.saturating_sub(1)), 1.0);
        }

        y += btn_size + 8.0;

        // List lights with enable toggle - collect click actions first
        let mut toggle_light: Option<usize> = None;

        for (i, light) in state.raster_settings.lights.iter().enumerate() {
            if y + line_height > rect.bottom() {
                break;
            }

            let light_type_str = match &light.light_type {
                crate::rasterizer::LightType::Directional { .. } => "Dir",
                crate::rasterizer::LightType::Point { .. } => "Pt",
                crate::rasterizer::LightType::Spot { .. } => "Sp",
            };

            // Toggle button
            let toggle_rect = Rect::new(rect.x, y, 50.0, 16.0);
            let toggle_color = if light.enabled { ACCENT_COLOR } else { Color::from_rgba(60, 60, 65, 255) };
            draw_rectangle(toggle_rect.x, toggle_rect.y, toggle_rect.w, toggle_rect.h, toggle_color);
            draw_text(&format!("{} {}", light_type_str, i + 1), toggle_rect.x + 4.0, toggle_rect.y + 12.0, 10.0, TEXT_COLOR);

            if ctx.mouse.inside(&toggle_rect) && ctx.mouse.left_pressed {
                toggle_light = Some(i);
            }

            // Intensity indicator
            let intensity_str = format!("{:.0}%", light.intensity * 100.0);
            draw_text(&intensity_str, rect.x + 55.0, y + 12.0, 10.0, TEXT_DIM);

            y += line_height;
        }

        // Apply toggle action after the loop
        if let Some(i) = toggle_light {
            let was_enabled = state.raster_settings.lights[i].enabled;
            state.raster_settings.lights[i].enabled = !was_enabled;
            let status = if !was_enabled { "ON" } else { "OFF" };
            state.set_status(&format!("Light {}: {}", i + 1, status), 0.5);
        }
    }
}

fn draw_timeline(_ctx: &mut UiContext, rect: Rect, _state: &mut ModelerState, _icon_font: Option<&Font>) {
    // Timeline disabled in mesh-only mode
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, HEADER_COLOR);
    draw_text("Timeline (disabled)", rect.x + 10.0, rect.y + 20.0, 14.0, TEXT_DIM);
}

fn draw_status_bar(rect: Rect, state: &ModelerState) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(40, 40, 45, 255));

    // Status message
    if let Some(msg) = state.get_status() {
        let center_x = rect.x + rect.w * 0.5 - (msg.len() as f32 * 4.0);
        draw_text(msg, center_x, rect.y + 15.0, 14.0, Color::from_rgba(100, 255, 100, 255));
    }

    // PicoCAD-style hints (always visible on left)
    let pico_hints = "V:View M:Render L:Shade Space:Fullscreen Tab:Add";
    draw_text(pico_hints, rect.x + 8.0, rect.y + 15.0, 12.0, TEXT_DIM);

    // Mesh editing hints (on right)
    let hints = "X:Multi E:Extrude Del:Delete Ctrl+Z:Undo";
    draw_text(hints, rect.right() - (hints.len() as f32 * 6.0) - 8.0, rect.y + 15.0, 12.0, TEXT_DIM);
}

fn handle_keyboard(state: &mut ModelerState) {
    let ctrl = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl)
             || is_key_down(KeyCode::LeftSuper) || is_key_down(KeyCode::RightSuper);
    let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);

    // ========================================================================
    // Undo/Redo (Ctrl+Z, Ctrl+Shift+Z, Ctrl+Y)
    // ========================================================================
    if ctrl && is_key_pressed(KeyCode::Z) {
        if shift {
            state.redo();
        } else {
            state.undo();
        }
        return; // Don't process other shortcuts
    }
    if ctrl && is_key_pressed(KeyCode::Y) {
        state.redo();
        return;
    }

    // File shortcuts (Ctrl+N, Ctrl+S, etc.) are now handled in draw_toolbar
    // to properly return ModelerAction

    // ========================================================================
    // Z key = Temporarily disable grid snap (hold to disable)
    // ========================================================================
    // Note: We store snap state when Z is pressed, restore when released
    // For simplicity, we just check if Z is held and temporarily override
    let snap_override = is_key_down(KeyCode::Z);

    // ========================================================================
    // Arrow key movement (PicoCAD-style: move selection by grid units)
    // ========================================================================
    handle_arrow_key_movement(state, shift, snap_override);

    // ========================================================================
    // PicoCAD-style shortcuts
    // ========================================================================

    // V = Toggle Build/Texture view mode
    if is_key_pressed(KeyCode::V) && !ctrl {
        state.toggle_view_mode();
    }

    // Space = Toggle fullscreen viewport
    if is_key_pressed(KeyCode::Space) {
        state.toggle_fullscreen_viewport();
    }

    // M = Toggle wireframe/solid render mode - like PicoCAD
    if is_key_pressed(KeyCode::M) && !ctrl {
        state.raster_settings.wireframe_overlay = !state.raster_settings.wireframe_overlay;
        let mode = if state.raster_settings.wireframe_overlay { "Wireframe" } else { "Solid" };
        state.set_status(&format!("Render: {}", mode), 1.0);
    }

    // L = Toggle shading (like PicoCAD)
    if is_key_pressed(KeyCode::L) && !ctrl {
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
        state.set_status(&format!("Shading: {}", mode), 1.0);
    }

    // ========================================================================
    // Selection mode shortcuts
    // ========================================================================

    // 1/2/3 = Vertex/Edge/Face selection modes
    if is_key_pressed(KeyCode::Key1) && !ctrl {
        state.select_mode = SelectMode::Vertex;
        state.selection = super::state::ModelerSelection::None;
        state.set_status("Vertex mode", 1.0);
    }

    if is_key_pressed(KeyCode::Key2) && !ctrl {
        state.select_mode = SelectMode::Edge;
        state.selection = super::state::ModelerSelection::None;
        state.set_status("Edge mode", 1.0);
    }

    if is_key_pressed(KeyCode::Key3) && !ctrl {
        state.select_mode = SelectMode::Face;
        state.selection = super::state::ModelerSelection::None;
        state.set_status("Face mode", 1.0);
    }

    // Transform tools (not in Object mode for Mesh context)
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
        // Perform extrude immediately on selected faces
        if let super::state::ModelerSelection::Faces(face_indices) = &state.selection {
            if !face_indices.is_empty() {
                let indices = face_indices.clone();
                state.push_undo("Extrude"); // Save state before extrude
                let extrude_amount = state.snap_settings.grid_size; // Extrude by one grid unit
                let new_faces = state.mesh.extrude_faces(&indices, extrude_amount);
                state.selection = super::state::ModelerSelection::Faces(new_faces);
                state.sync_mesh_to_project(); // Keep project in sync
                state.dirty = true;
                state.set_status(&format!("Extruded {} face(s)", indices.len()), 1.0);
            } else {
                state.set_status("Select faces to extrude", 1.0);
            }
        } else {
            state.set_status("Switch to Face mode (3) to extrude", 1.0);
        }
    }

    // Delete selection (Delete or Backspace - NOT X since X is for multi-select)
    if is_key_pressed(KeyCode::Delete) || is_key_pressed(KeyCode::Backspace) {
        delete_selection(state);
    }
}

/// Handle arrow key movement for selected vertices/faces
/// PicoCAD-style: arrow keys move by grid units, shift for smaller increments
fn handle_arrow_key_movement(state: &mut ModelerState, shift: bool, snap_disabled: bool) {
    use crate::rasterizer::Vec3;

    // Check for arrow key presses
    let left = is_key_pressed(KeyCode::Left);
    let right = is_key_pressed(KeyCode::Right);
    let up = is_key_pressed(KeyCode::Up);
    let down = is_key_pressed(KeyCode::Down);

    if !left && !right && !up && !down {
        return;
    }

    // Determine move amount
    let grid = state.snap_settings.grid_size;
    let move_amount = if snap_disabled {
        1.0  // 1 unit when snap disabled (Z held)
    } else if shift {
        grid * 0.5  // Half grid when shift held
    } else {
        grid  // Full grid step
    };

    // Determine move direction based on active viewport
    // Different viewports have different axis mappings
    let delta = match state.active_viewport {
        ViewportId::Perspective | ViewportId::Front => {
            // Front view (XY plane): Left/Right = X, Up/Down = Y
            Vec3::new(
                if right { move_amount } else if left { -move_amount } else { 0.0 },
                if up { move_amount } else if down { -move_amount } else { 0.0 },
                0.0,
            )
        }
        ViewportId::Top => {
            // Top view (XZ plane): Left/Right = X, Up/Down = Z
            Vec3::new(
                if right { move_amount } else if left { -move_amount } else { 0.0 },
                0.0,
                if up { -move_amount } else if down { move_amount } else { 0.0 },
            )
        }
        ViewportId::Side => {
            // Side view (ZY plane): Left/Right = Z, Up/Down = Y
            Vec3::new(
                0.0,
                if up { move_amount } else if down { -move_amount } else { 0.0 },
                if right { move_amount } else if left { -move_amount } else { 0.0 },
            )
        }
    };

    // Get selected vertex indices
    let mut vertex_indices = get_selected_vertex_indices(state);

    if vertex_indices.is_empty() {
        return;
    }

    // If vertex linking is enabled, expand to include coincident vertices
    if state.vertex_linking {
        vertex_indices = state.mesh.expand_to_coincident(&vertex_indices, 0.001);
    }

    // Save undo state before moving
    state.push_undo("Move");

    // Move selected vertices
    for &vi in &vertex_indices {
        if let Some(vert) = state.mesh.vertices.get_mut(vi) {
            vert.pos.x += delta.x;
            vert.pos.y += delta.y;
            vert.pos.z += delta.z;
        }
    }

    state.sync_mesh_to_project(); // Keep project in sync
    state.dirty = true;

    // Show status
    let snap_status = if snap_disabled { " (free)" } else { "" };
    state.set_status(&format!("Moved {} vert(s){}", vertex_indices.len(), snap_status), 0.5);
}

/// Delete the current selection (faces, edges, or vertices)
fn delete_selection(state: &mut ModelerState) {
    // Clone selection to avoid borrow issues
    let selection = state.selection.clone();

    match selection {
        super::state::ModelerSelection::Faces(face_indices) => {
            if face_indices.is_empty() {
                state.set_status("No faces selected", 1.0);
                return;
            }
            state.push_undo("Delete faces");

            // Sort indices in reverse order so we can remove without index shifting issues
            let mut indices = face_indices.clone();
            indices.sort();
            indices.reverse();

            for fi in indices {
                if fi < state.mesh.faces.len() {
                    state.mesh.faces.remove(fi);
                }
            }

            let count = face_indices.len();
            state.selection = super::state::ModelerSelection::None;
            state.dirty = true;
            state.set_status(&format!("Deleted {} face(s)", count), 1.0);
        }
        super::state::ModelerSelection::Vertices(vert_indices) => {
            if vert_indices.is_empty() {
                state.set_status("No vertices selected", 1.0);
                return;
            }
            state.push_undo("Delete vertices");

            // First, remove all faces that reference these vertices
            let vert_set: std::collections::HashSet<usize> = vert_indices.iter().copied().collect();
            state.mesh.faces.retain(|f| {
                !vert_set.contains(&f.v0) && !vert_set.contains(&f.v1) && !vert_set.contains(&f.v2)
            });

            // Then remove vertices (in reverse order to avoid index shifting)
            let mut indices = vert_indices.clone();
            indices.sort();
            indices.reverse();

            for vi in &indices {
                if *vi < state.mesh.vertices.len() {
                    state.mesh.vertices.remove(*vi);

                    // Update face indices that are higher than the removed vertex
                    for face in &mut state.mesh.faces {
                        if face.v0 > *vi { face.v0 -= 1; }
                        if face.v1 > *vi { face.v1 -= 1; }
                        if face.v2 > *vi { face.v2 -= 1; }
                    }
                }
            }

            let count = vert_indices.len();
            state.selection = super::state::ModelerSelection::None;
            state.dirty = true;
            state.set_status(&format!("Deleted {} vertex(es)", count), 1.0);
        }
        super::state::ModelerSelection::Edges(edge_indices) => {
            if edge_indices.is_empty() {
                state.set_status("No edges selected", 1.0);
                return;
            }
            // For edges, we delete any face that contains this edge
            state.push_undo("Delete edges");

            let edge_set: std::collections::HashSet<(usize, usize)> = edge_indices.iter()
                .map(|&(a, b)| (a.min(b), a.max(b)))
                .collect();

            let faces_before = state.mesh.faces.len();
            state.mesh.faces.retain(|f| {
                let e1 = (f.v0.min(f.v1), f.v0.max(f.v1));
                let e2 = (f.v1.min(f.v2), f.v1.max(f.v2));
                let e3 = (f.v2.min(f.v0), f.v2.max(f.v0));
                !edge_set.contains(&e1) && !edge_set.contains(&e2) && !edge_set.contains(&e3)
            });

            let deleted = faces_before - state.mesh.faces.len();
            state.selection = super::state::ModelerSelection::None;
            state.dirty = true;
            state.set_status(&format!("Deleted {} face(s) with edges", deleted), 1.0);
        }
        _ => {
            state.set_status("Nothing selected to delete", 1.0);
            return; // Early return - no sync needed
        }
    }

    // Sync geometry changes to project
    state.sync_mesh_to_project();
}

/// Get all vertex indices affected by current selection
fn get_selected_vertex_indices(state: &ModelerState) -> Vec<usize> {
    match &state.selection {
        super::state::ModelerSelection::Vertices(indices) => indices.clone(),
        super::state::ModelerSelection::Edges(edges) => {
            // Collect unique vertices from edges
            let mut verts: Vec<usize> = edges.iter()
                .flat_map(|(v0, v1)| vec![*v0, *v1])
                .collect();
            verts.sort();
            verts.dedup();
            verts
        }
        super::state::ModelerSelection::Faces(face_indices) => {
            // Collect unique vertices from faces
            let mut verts: Vec<usize> = face_indices.iter()
                .filter_map(|&fi| state.mesh.face_vertices(fi))
                .flat_map(|[v0, v1, v2]| vec![v0, v1, v2])
                .collect();
            verts.sort();
            verts.dedup();
            verts
        }
        _ => Vec::new(),
    }
}

// ============================================================================
// Context Menu (right-click to add primitives)
// ============================================================================

/// Primitive types that can be added via context menu
#[derive(Debug, Clone, Copy)]
enum PrimitiveType {
    Cube,
    Plane,
    Prism,
    Cylinder,
    Pyramid,
    Pent,
    Hex,
}

impl PrimitiveType {
    const ALL: [PrimitiveType; 7] = [
        PrimitiveType::Cube,
        PrimitiveType::Plane,
        PrimitiveType::Prism,
        PrimitiveType::Cylinder,
        PrimitiveType::Pyramid,
        PrimitiveType::Pent,
        PrimitiveType::Hex,
    ];

    fn label(&self) -> &'static str {
        match self {
            PrimitiveType::Cube => "Cube",
            PrimitiveType::Plane => "Plane",
            PrimitiveType::Prism => "Prism (Wedge)",
            PrimitiveType::Cylinder => "Cylinder",
            PrimitiveType::Pyramid => "Pyramid",
            PrimitiveType::Pent => "Pentagon",
            PrimitiveType::Hex => "Hexagon",
        }
    }

    fn create(&self, size: f32) -> EditableMesh {
        match self {
            PrimitiveType::Cube => EditableMesh::cube(size),
            PrimitiveType::Plane => EditableMesh::plane(size),
            PrimitiveType::Prism => EditableMesh::prism(size, size),
            PrimitiveType::Cylinder => EditableMesh::cylinder(size / 2.0, size, 8),
            PrimitiveType::Pyramid => EditableMesh::pyramid(size, size),
            PrimitiveType::Pent => EditableMesh::pent(size / 2.0, size),
            PrimitiveType::Hex => EditableMesh::hex(size / 2.0, size),
        }
    }
}

/// Draw and handle context menu
fn draw_context_menu(ctx: &mut UiContext, state: &mut ModelerState) {
    // Check for Tab key to open menu (avoids conflict with right-click camera rotation)
    // On Mac, right-click is used for camera rotation, so use Tab instead
    if is_key_pressed(KeyCode::Tab) {
        let (mx, my) = (ctx.mouse.x, ctx.mouse.y);

        // Compute world position based on active viewport
        let world_pos = screen_to_world_position(state, mx, my);

        // Snap to grid
        let snapped = state.snap_settings.snap_vec3(world_pos);

        state.context_menu = Some(ContextMenu::new(mx, my, snapped, state.active_viewport));
    }

    // Close menu on left click outside or Escape
    if is_key_pressed(KeyCode::Escape) {
        state.context_menu = None;
        return;
    }

    let menu = match &state.context_menu {
        Some(m) => m.clone(),
        None => return,
    };

    // Menu dimensions
    let item_height = 24.0;
    let menu_width = 130.0;
    let separator_height = 8.0;

    // Items: primitives + separator + clone + clear
    let primitive_count = PrimitiveType::ALL.len();
    let menu_height = (primitive_count as f32 * item_height) + separator_height + (2.0 * item_height) + 8.0;

    // Keep menu on screen
    let menu_x = menu.x.min(screen_width() - menu_width - 5.0);
    let menu_y = menu.y.min(screen_height() - menu_height - 5.0);

    let menu_rect = Rect::new(menu_x, menu_y, menu_width, menu_height);

    // Draw menu background
    draw_rectangle(menu_rect.x - 1.0, menu_rect.y - 1.0, menu_rect.w + 2.0, menu_rect.h + 2.0, Color::from_rgba(80, 80, 85, 255));
    draw_rectangle(menu_rect.x, menu_rect.y, menu_rect.w, menu_rect.h, Color::from_rgba(45, 45, 50, 255));

    let mut y = menu_rect.y + 4.0;

    // Header
    draw_text("Add Primitive", menu_rect.x + 8.0, y + 14.0, 12.0, TEXT_DIM);
    y += item_height;

    // Primitive items
    let mut clicked_primitive: Option<PrimitiveType> = None;
    for prim in PrimitiveType::ALL {
        let item_rect = Rect::new(menu_rect.x + 2.0, y, menu_width - 4.0, item_height);

        // Hover highlight
        if ctx.mouse.inside(&item_rect) {
            draw_rectangle(item_rect.x, item_rect.y, item_rect.w, item_rect.h, Color::from_rgba(60, 60, 70, 255));

            if ctx.mouse.left_pressed {
                clicked_primitive = Some(prim);
            }
        }

        draw_text(prim.label(), item_rect.x + 8.0, item_rect.y + 16.0, 14.0, TEXT_COLOR);
        y += item_height;
    }

    // Separator
    y += 4.0;
    draw_line(menu_rect.x + 8.0, y, menu_rect.right() - 8.0, y, 1.0, Color::from_rgba(70, 70, 75, 255));
    y += separator_height;

    // Clone mesh option
    let clone_rect = Rect::new(menu_rect.x + 2.0, y, menu_width - 4.0, item_height);
    let mut clone_clicked = false;
    if ctx.mouse.inside(&clone_rect) {
        draw_rectangle(clone_rect.x, clone_rect.y, clone_rect.w, clone_rect.h, Color::from_rgba(60, 60, 70, 255));
        if ctx.mouse.left_pressed {
            clone_clicked = true;
        }
    }
    draw_text("Clone Mesh", clone_rect.x + 8.0, clone_rect.y + 16.0, 14.0, TEXT_COLOR);
    y += item_height;

    // Clear mesh option
    let clear_rect = Rect::new(menu_rect.x + 2.0, y, menu_width - 4.0, item_height);
    let mut clear_clicked = false;
    if ctx.mouse.inside(&clear_rect) {
        draw_rectangle(clear_rect.x, clear_rect.y, clear_rect.w, clear_rect.h, Color::from_rgba(80, 50, 50, 255));
        if ctx.mouse.left_pressed {
            clear_clicked = true;
        }
    }
    draw_text("Clear All", clear_rect.x + 8.0, clear_rect.y + 16.0, 14.0, Color::from_rgba(255, 150, 150, 255));

    // Handle clicks
    if let Some(prim) = clicked_primitive {
        state.push_undo(&format!("Add {}", prim.label()));
        let size = state.snap_settings.grid_size * 2.0; // 2 grid units
        let new_mesh = prim.create(size);
        state.mesh.merge(&new_mesh, menu.world_pos);
        state.sync_mesh_to_project(); // Keep project in sync
        state.dirty = true;
        state.set_status(&format!("Added {}", prim.label()), 1.0);
        state.context_menu = None;
    }

    if clone_clicked {
        state.push_undo("Clone mesh");
        // Clone entire mesh at offset
        let offset = Vec3::new(
            state.snap_settings.grid_size * 2.0,
            0.0,
            state.snap_settings.grid_size * 2.0,
        );
        let clone = state.mesh.clone();
        state.mesh.merge(&clone, offset);
        state.sync_mesh_to_project(); // Keep project in sync
        state.dirty = true;
        state.set_status("Cloned mesh", 1.0);
        state.context_menu = None;
    }

    if clear_clicked {
        state.push_undo("Clear mesh");
        state.mesh = EditableMesh::new();
        state.sync_mesh_to_project(); // Keep project in sync
        state.selection.clear();
        state.dirty = true;
        state.set_status("Cleared mesh", 1.0);
        state.context_menu = None;
    }

    // Close if clicked outside menu
    if ctx.mouse.left_pressed && !ctx.mouse.inside(&menu_rect) {
        state.context_menu = None;
    }
}

/// Convert screen position to world position based on active viewport
fn screen_to_world_position(state: &ModelerState, _screen_x: f32, _screen_y: f32) -> Vec3 {
    // For now, place at grid origin elevated slightly
    // A full implementation would ray-cast into the viewport
    match state.active_viewport {
        ViewportId::Perspective => {
            // Place in front of camera on ground plane
            Vec3::new(0.0, 0.0, 0.0)
        }
        ViewportId::Top => {
            // Place on XZ plane
            let center = state.ortho_top.center;
            Vec3::new(center.x, 0.0, center.y)
        }
        ViewportId::Front => {
            // Place on XY plane
            let center = state.ortho_front.center;
            Vec3::new(center.x, center.y, 0.0)
        }
        ViewportId::Side => {
            // Place on YZ plane
            let center = state.ortho_side.center;
            Vec3::new(0.0, center.y, center.x)
        }
    }
}
