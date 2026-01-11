//! Modeler UI layout and rendering

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext, SplitPanel, draw_panel, panel_content_rect, Toolbar, icon, icon_button, icon_button_active, ActionRegistry, draw_icon_centered};
use crate::rasterizer::{Framebuffer, render_mesh, render_mesh_15, Camera, OrthoProjection, point_in_triangle_2d};
use crate::rasterizer::{Vertex as RasterVertex, Face as RasterFace, Color as RasterColor};
use crate::rasterizer::{ClutDepth, Clut, Color15};
use super::state::{ModelerState, SelectMode, ViewportId, ContextMenu, ModalTransform, CameraMode};
use crate::texture::{
    UserTexture, TextureSize,
    draw_texture_canvas, draw_tool_panel, draw_palette_panel, draw_mode_tabs,
    TextureEditorMode, UvOverlayData, UvVertex, UvFace,
};
use super::tools::ModelerToolId;
use super::viewport::{draw_modeler_viewport, draw_modeler_viewport_ext};
use super::mesh_editor::EditableMesh;
use super::actions::{create_modeler_actions, build_context};
use crate::rasterizer::Vec3;

// Colors (matching tracker/editor style)
const BG_COLOR: Color = Color::new(0.11, 0.11, 0.13, 1.0);
const HEADER_COLOR: Color = Color::new(0.15, 0.15, 0.18, 1.0);
const TEXT_COLOR: Color = Color::new(0.8, 0.8, 0.85, 1.0);
const TEXT_DIM: Color = Color::new(0.4, 0.4, 0.45, 1.0);
const ACCENT_COLOR: Color = Color::new(0.0, 0.75, 0.9, 1.0);

// PS1 polygon budget colors
const POLY_GREEN: Color = Color::new(0.4, 0.9, 0.4, 1.0);   // < 300 faces - very safe
const POLY_YELLOW: Color = Color::new(0.9, 0.9, 0.3, 1.0);  // 300-800 faces - moderate
const POLY_RED: Color = Color::new(0.9, 0.4, 0.4, 1.0);     // > 800 faces - heavy

/// Get color for face count based on PS1-realistic polygon budgets
fn poly_count_color(face_count: usize) -> Color {
    if face_count < 300 {
        POLY_GREEN
    } else if face_count < 800 {
        POLY_YELLOW
    } else {
        POLY_RED
    }
}

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
    /// Timeline height
    pub timeline_height: f32,
    /// Action registry for keyboard shortcuts
    pub actions: ActionRegistry,
}

impl ModelerLayout {
    pub fn new() -> Self {
        Self {
            main_split: SplitPanel::horizontal(100).with_ratio(0.18).with_min_size(150.0),
            right_split: SplitPanel::horizontal(101).with_ratio(0.78).with_min_size(150.0),
            timeline_height: 80.0,
            actions: create_modeler_actions(),
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

    // Draw panels with collapsible sections
    // Left panel: Overview + Selection + Lights + Shortcuts
    draw_panel(left_rect, None, Color::from_rgba(35, 35, 40, 255));
    draw_left_panel(ctx, panel_content_rect(left_rect, false), state, icon_font);

    // Draw 4-panel viewport (PicoCAD-style)
    draw_4panel_viewport(ctx, center_rect, state, fb);

    // Right panel: Atlas + UV Tools + Paint Tools + CLUT
    draw_panel(right_rect, None, Color::from_rgba(35, 35, 40, 255));
    draw_right_panel(ctx, panel_content_rect(right_rect, false), state, icon_font);

    // Draw timeline if in animate mode
    if let Some(tl_rect) = timeline_rect {
        draw_panel(tl_rect, Some("Timeline"), Color::from_rgba(30, 30, 35, 255));
        draw_timeline(ctx, panel_content_rect(tl_rect, true), state, icon_font);
    }

    // Draw status bar
    draw_status_bar(status_rect, state);

    // Handle keyboard shortcuts using action registry
    let keyboard_action = handle_actions(&layout.actions, state, ctx);
    let action = if keyboard_action != ModelerAction::None { keyboard_action } else { action };

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

    // Transform tools with gizmos (using new tool system)
    let tools = [
        (icon::MOVE, "Move (G)", ModelerToolId::Move),
        (icon::ROTATE_3D, "Rotate (R)", ModelerToolId::Rotate),
        (icon::SCALE_3D, "Scale (T)", ModelerToolId::Scale),
    ];

    for (icon_char, tooltip, tool_id) in tools {
        let is_active = state.tool_box.is_active(tool_id);
        if toolbar.icon_button_active(ctx, icon_char, icon_font, tooltip, is_active) {
            state.tool_box.toggle(tool_id);
        }
    }

    toolbar.separator();

    // PS1 effect toggles
    if toolbar.icon_button_active(ctx, icon::WAVES, icon_font, "Affine Textures (warpy)", state.raster_settings.affine_textures) {
        state.raster_settings.affine_textures = !state.raster_settings.affine_textures;
        let mode = if state.raster_settings.affine_textures { "ON" } else { "OFF" };
        state.set_status(&format!("Affine textures: {}", mode), 1.5);
    }
    if toolbar.icon_button_active(ctx, icon::HASH, icon_font, "Fixed-Point Math (jittery)", state.raster_settings.use_fixed_point) {
        state.raster_settings.use_fixed_point = !state.raster_settings.use_fixed_point;
        let mode = if state.raster_settings.use_fixed_point { "ON" } else { "OFF" };
        state.set_status(&format!("Fixed-point: {}", mode), 1.5);
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
    // Backface culling toggle (cycles through 3 states like world editor)
    // State 0: Both sides visible (backface_cull=false)
    // State 1: Wireframe on back (backface_cull=true, backface_wireframe=true)
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
            state.set_status("Backfaces: Wireframe", 1.5);
        } else if state.raster_settings.backface_wireframe {
            // Was: wireframe → Now: hidden
            state.raster_settings.backface_wireframe = false;
            state.set_status("Backfaces: Hidden", 1.5);
        } else {
            // Was: hidden → Now: both visible
            state.raster_settings.backface_cull = false;
            state.set_status("Backfaces: Both Sides Visible", 1.5);
        }
    }
    // Z-buffer toggle (ON = z-buffer, OFF = painter's algorithm)
    if toolbar.icon_button_active(ctx, icon::ARROW_DOWN_UP, icon_font, "Z-Buffer (OFF = painter's algorithm)", state.raster_settings.use_zbuffer) {
        state.raster_settings.use_zbuffer = !state.raster_settings.use_zbuffer;
        let mode = if state.raster_settings.use_zbuffer { "Z-Buffer" } else { "Painter's Algorithm" };
        state.set_status(&format!("Depth: {}", mode), 1.5);
    }
    // RGB555 toggle (PS1-authentic 15-bit color mode)
    if toolbar.icon_button_active(ctx, icon::PALETTE, icon_font, "RGB555 (PS1 15-bit color mode)", state.raster_settings.use_rgb555) {
        state.raster_settings.use_rgb555 = !state.raster_settings.use_rgb555;
        let mode = if state.raster_settings.use_rgb555 { "RGB555 (15-bit)" } else { "RGB888 (24-bit)" };
        state.set_status(&format!("Color: {}", mode), 1.5);
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

    // Camera mode toggle (Free / Orbit)
    let is_free = state.camera_mode == CameraMode::Free;
    let is_orbit = state.camera_mode == CameraMode::Orbit;

    if toolbar.icon_button_active(ctx, icon::EYE, icon_font, "Free Camera (WASD + mouse)", is_free) {
        state.camera_mode = CameraMode::Free;
        state.set_status("Camera: Free (WASD + right-drag to look)", 2.0);
    }
    if toolbar.icon_button_active(ctx, icon::ORBIT, icon_font, "Orbit Camera", is_orbit) {
        state.camera_mode = CameraMode::Orbit;
        // Sync orbit camera to current view direction when switching
        state.sync_camera_from_orbit();
        state.set_status("Camera: Orbit (right-drag to rotate)", 2.0);
    }

    toolbar.separator();

    // Mesh stats
    let stats = format!("Verts:{} Faces:{}", state.mesh().vertex_count(), state.mesh().face_count());
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

    // Note: Keyboard shortcuts are now handled through the ActionRegistry in handle_actions()

    action
}

/// Draw the Overview panel (PicoCAD-style object list)
fn draw_overview_panel(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, icon_font: Option<&Font>) {
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
    let mouse_pos = (ctx.mouse.x, ctx.mouse.y);
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
        let eye_icon = if obj.visible { icon::EYE } else { icon::EYE_OFF };
        draw_icon_centered(icon_font, eye_icon, &eye_rect, 14.0, eye_color);

        // Lock icon
        let lock_rect = Rect { x: rect.x + icon_width + 2.0, y, w: icon_width, h: row_height };
        if obj.locked {
            draw_icon_centered(icon_font, icon::LOCK, &lock_rect, 12.0, Color::from_rgba(255, 180, 100, 255));
        }

        // Object name
        let name_x = lock_rect.x + icon_width;
        let name_color = if obj.visible { TEXT_COLOR } else { TEXT_DIM };
        let display_name = if obj.name.len() > 20 {
            format!("{}...", &obj.name[..17])
        } else {
            obj.name.clone()
        };
        draw_text(&display_name, name_x, y + 16.0, 14.0, name_color);

        // Face count (color-coded for PS1 polygon budget)
        let face_count = obj.mesh.face_count();
        let count_text = format!("{}", face_count);
        let count_x = rect.x + rect.w - 30.0;
        let count_color = poly_count_color(face_count);
        draw_text(&count_text, count_x, y + 16.0, 12.0, count_color);

        // Handle clicks
        if ctx.mouse.left_pressed {
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
                    let fc = obj.mesh.face_count();
                    draw_text(
                        &format!("\"{}\" - {} faces", obj.name, fc),
                        rect.x, info_y + 12.0, 12.0, poly_count_color(fc),
                    );
                }
            }
        }
    }
}

// ============================================================================
// Left Panel (Overview + Selection + Lights + Shortcuts)
// ============================================================================

/// Draw a simple section label (non-collapsible)
fn draw_section_label(x: f32, y: &mut f32, width: f32, label: &str) {
    draw_rectangle(x, *y, width, 18.0, Color::from_rgba(45, 45, 52, 255));
    draw_text(label, x + 4.0, *y + 13.0, 13.0, TEXT_COLOR);
    *y += 20.0;
}

fn draw_left_panel(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, icon_font: Option<&Font>) {
    let mut y = rect.y;
    let width = rect.w;
    let x = rect.x;

    // === OVERVIEW SECTION ===
    draw_section_label(x, &mut y, width, "Overview");
    let overview_height = (rect.h * 0.35).min(180.0);
    let overview_rect = Rect::new(x, y, width, overview_height);
    draw_overview_content(ctx, overview_rect, state, icon_font);
    y += overview_height + 4.0;

    // === SELECTION SECTION ===
    draw_section_label(x, &mut y, width, "Selection");
    draw_selection_info(ctx, x, &mut y, width, state);
    y += 4.0;

    // === LIGHTS SECTION ===
    draw_section_label(x, &mut y, width, "Lights");
    draw_lights_section(ctx, x, &mut y, width, state, icon_font);
    y += 4.0;

    // === SHORTCUTS SECTION ===
    draw_section_label(x, &mut y, width, "Shortcuts");
    draw_shortcuts_section(x, &mut y, width, rect.bottom());
}

/// Draw overview content (object list with visibility toggles)
fn draw_overview_content(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, icon_font: Option<&Font>) {
    let line_height = 18.0;
    let mut y = rect.y;

    // Collect click actions first (to avoid borrow issues)
    let mut select_idx: Option<usize> = None;
    let mut toggle_vis_idx: Option<usize> = None;

    let obj_count = state.project.objects.len();
    for idx in 0..obj_count {
        if y + line_height > rect.bottom() {
            break;
        }

        let obj = &state.project.objects[idx];
        let is_selected = state.project.selected_object == Some(idx);
        let item_rect = Rect::new(rect.x, y, rect.w, line_height);

        // Selection highlight
        if is_selected {
            draw_rectangle(item_rect.x, item_rect.y, item_rect.w, item_rect.h, Color::from_rgba(60, 80, 100, 255));
        } else if ctx.mouse.inside(&item_rect) {
            draw_rectangle(item_rect.x, item_rect.y, item_rect.w, item_rect.h, Color::from_rgba(50, 50, 55, 255));
        }

        // Visibility toggle (eye icon)
        let vis_rect = Rect::new(rect.x + 2.0, y + 1.0, 16.0, 16.0);
        let vis_icon = if obj.visible { icon::EYE } else { icon::EYE_OFF };
        let vis_color = if obj.visible { TEXT_COLOR } else { TEXT_DIM };
        draw_icon_centered(icon_font, vis_icon, &vis_rect, 11.0, vis_color);

        if ctx.mouse.inside(&vis_rect) && ctx.mouse.left_pressed {
            toggle_vis_idx = Some(idx);
        }

        // Object name with face count
        let fc = obj.mesh.face_count();
        let name_color = poly_count_color(fc);
        draw_text(&format!("{} ({})", obj.name, fc), rect.x + 20.0, y + 13.0, 13.0, name_color);

        // Handle selection click (not on visibility toggle)
        let name_rect = Rect::new(rect.x + 20.0, y, rect.w - 20.0, line_height);
        if ctx.mouse.inside(&name_rect) && ctx.mouse.left_pressed && toggle_vis_idx.is_none() {
            select_idx = Some(idx);
        }

        y += line_height;
    }

    // Apply actions after the loop
    if let Some(idx) = toggle_vis_idx {
        state.project.objects[idx].visible = !state.project.objects[idx].visible;
    } else if let Some(idx) = select_idx {
        state.project.selected_object = Some(idx);
        // Project is single source of truth - mesh() reads from it directly
    }
}

/// Draw selection info (what's selected, tool, etc.)
fn draw_selection_info(_ctx: &mut UiContext, x: f32, y: &mut f32, _width: f32, state: &ModelerState) {
    let line_height = 16.0;

    // Selection type
    let sel_text = match &state.selection {
        super::state::ModelerSelection::None => "Nothing selected".to_string(),
        super::state::ModelerSelection::Mesh => "Mesh (whole)".to_string(),
        super::state::ModelerSelection::Vertices(v) => format!("{} vertex(es)", v.len()),
        super::state::ModelerSelection::Edges(e) => format!("{} edge(s)", e.len()),
        super::state::ModelerSelection::Faces(f) => format!("{} face(s)", f.len()),
    };
    draw_text(&sel_text, x + 4.0, *y + 12.0, 13.0, TEXT_COLOR);
    *y += line_height;

    // Current tool
    let tool_label = match state.tool_box.active_transform_tool() {
        Some(ModelerToolId::Move) => "Tool: Move (G)",
        Some(ModelerToolId::Rotate) => "Tool: Rotate (R)",
        Some(ModelerToolId::Scale) => "Tool: Scale (S)",
        _ => "Tool: Select",
    };
    draw_text(tool_label, x + 4.0, *y + 12.0, 13.0, TEXT_DIM);
    *y += line_height;

    // Select mode
    let mode_label = match state.select_mode {
        super::state::SelectMode::Vertex => "Mode: Vertex (1)",
        super::state::SelectMode::Edge => "Mode: Edge (2)",
        super::state::SelectMode::Face => "Mode: Face (3)",
    };
    draw_text(mode_label, x + 4.0, *y + 12.0, 13.0, TEXT_DIM);
    *y += line_height;

    // Note: Face Properties (blend mode) is now in the right panel texture section
}

/// Draw lights section
fn draw_lights_section(ctx: &mut UiContext, x: f32, y: &mut f32, width: f32, state: &mut ModelerState, icon_font: Option<&Font>) {
    let line_height = 18.0;
    let btn_size = 18.0;

    // Light count and add/remove buttons
    let light_count = state.raster_settings.lights.len();
    draw_text(&format!("{} light(s)", light_count), x + 4.0, *y + 13.0, 13.0, TEXT_COLOR);

    // Add button
    let add_rect = Rect::new(x + width - btn_size * 2.0 - 8.0, *y, btn_size, btn_size);
    if icon_button(ctx, add_rect, icon::PLUS, icon_font, "Add light") {
        use crate::rasterizer::{Light, LightType, Vec3};
        state.raster_settings.lights.push(Light {
            light_type: LightType::Directional { direction: Vec3::new(-1.0, -1.0, -1.0).normalize() },
            color: crate::rasterizer::Color::new(255, 255, 255),
            intensity: 0.5,
            enabled: true,
            name: format!("Light {}", light_count + 1),
        });
    }

    // Remove button
    let rem_rect = Rect::new(x + width - btn_size - 4.0, *y, btn_size, btn_size);
    if icon_button(ctx, rem_rect, icon::MINUS, icon_font, "Remove light") && light_count > 0 {
        state.raster_settings.lights.pop();
    }

    *y += btn_size + 4.0;

    // List lights
    let mut toggle_idx: Option<usize> = None;
    for (i, light) in state.raster_settings.lights.iter().enumerate() {
        let toggle_rect = Rect::new(x + 4.0, *y, 50.0, 14.0);
        let toggle_color = if light.enabled { ACCENT_COLOR } else { Color::from_rgba(60, 60, 65, 255) };
        draw_rectangle(toggle_rect.x, toggle_rect.y, toggle_rect.w, toggle_rect.h, toggle_color);

        let type_str = match &light.light_type {
            crate::rasterizer::LightType::Directional { .. } => "Dir",
            crate::rasterizer::LightType::Point { .. } => "Pt",
            crate::rasterizer::LightType::Spot { .. } => "Sp",
        };
        draw_text(&format!("{} {}", type_str, i + 1), toggle_rect.x + 4.0, *y + 10.0, 13.0, TEXT_COLOR);

        if ctx.mouse.inside(&toggle_rect) && ctx.mouse.left_pressed {
            toggle_idx = Some(i);
        }

        // Intensity
        draw_text(&format!("{:.0}%", light.intensity * 100.0), x + 58.0, *y + 10.0, 13.0, TEXT_DIM);

        *y += line_height;
    }

    if let Some(i) = toggle_idx {
        state.raster_settings.lights[i].enabled = !state.raster_settings.lights[i].enabled;
    }
}

/// Draw shortcuts reference section
fn draw_shortcuts_section(x: f32, y: &mut f32, _width: f32, max_y: f32) {
    let shortcuts = [
        ("G/R/S", "Move/Rotate/Scale"),
        ("E", "Extrude face"),
        ("X/Del", "Delete"),
        ("1/2/3", "Vert/Edge/Face"),
        ("V", "Toggle Build/UV"),
        ("Space", "Fullscreen viewport"),
        ("Ctrl+Z/Y", "Undo/Redo"),
    ];

    for (key, desc) in shortcuts {
        if *y + 14.0 > max_y {
            break;
        }
        draw_text(&format!("{}: {}", key, desc), x + 4.0, *y + 10.0, 13.0, TEXT_DIM);
        *y += 14.0;
    }
}

// ============================================================================
// Right Panel (Atlas + UV Tools + Paint Tools + CLUT)
// ============================================================================

fn draw_right_panel(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, icon_font: Option<&Font>) {
    let mut y = rect.y + 4.0;

    // === UNIFIED TEXTURE EDITOR (collapsible) ===
    // Combines Paint + UV modes with tab-based switching
    let editor_expanded = draw_collapsible_header(ctx, rect.x, &mut y, rect.w, "Texture", state.paint_section_expanded, icon_font);
    if editor_expanded != state.paint_section_expanded {
        state.paint_section_expanded = editor_expanded;
        // Initialize editing texture when expanding
        if editor_expanded && state.editing_texture.is_none() {
            state.editing_texture = Some(create_editing_texture(state));
            state.texture_editor.reset();
        }
    }

    if state.paint_section_expanded {
        // Draw the unified texture editor with Paint/UV tabs
        let remaining_h = rect.bottom() - y - 4.0;
        let editor_rect = Rect::new(rect.x, y, rect.w, remaining_h);
        draw_paint_section(ctx, editor_rect, state, icon_font);
    }
}

/// Draw a collapsible section header, returns new expanded state
fn draw_collapsible_header(
    ctx: &mut UiContext,
    x: f32,
    y: &mut f32,
    width: f32,
    label: &str,
    expanded: bool,
    icon_font: Option<&Font>,
) -> bool {
    let header_h = 24.0;
    let header_rect = Rect::new(x + 2.0, *y, width - 4.0, header_h);
    let hovered = ctx.mouse.inside(&header_rect);

    // Background
    let bg = if hovered {
        Color::from_rgba(55, 55, 62, 255)
    } else {
        Color::from_rgba(45, 45, 52, 255)
    };
    draw_rectangle(header_rect.x, header_rect.y, header_rect.w, header_rect.h, bg);

    // Expand/collapse icon
    let chevron = if expanded { icon::CHEVRON_DOWN } else { icon::CHEVRON_RIGHT };
    if let Some(font) = icon_font {
        draw_text_ex(
            &chevron.to_string(),
            header_rect.x + 6.0,
            header_rect.y + 17.0,
            TextParams { font: Some(font), font_size: 14, color: TEXT_COLOR, ..Default::default() },
        );
    }

    // Label
    draw_text(label, header_rect.x + 24.0, header_rect.y + 16.0, 13.0, TEXT_COLOR);

    *y += header_h + 2.0;

    // Handle click
    if ctx.mouse.clicked(&header_rect) {
        !expanded
    } else {
        expanded
    }
}

/// Create a UserTexture for editing from the project's IndexedAtlas
fn create_editing_texture(state: &ModelerState) -> UserTexture {
    let indexed = &state.project.atlas;
    // Get CLUT for palette colors
    let clut = state.project.clut_pool.get(indexed.default_clut)
        .or_else(|| state.project.clut_pool.iter().next())
        .cloned()
        .unwrap_or_else(|| Clut::new_4bit("default".to_string()));

    UserTexture {
        name: "atlas".to_string(),
        width: indexed.width,
        height: indexed.height,
        depth: indexed.depth,
        indices: indexed.indices.clone(),
        palette: clut.colors.clone(),
        blend_mode: crate::rasterizer::BlendMode::Opaque,
    }
}

/// Available thumbnail sizes for texture grid
const THUMB_SIZES: [f32; 5] = [32.0, 48.0, 64.0, 96.0, 128.0];

/// Get the next smaller thumbnail size
fn smaller_thumb_size(current: f32) -> f32 {
    for i in (0..THUMB_SIZES.len()).rev() {
        if THUMB_SIZES[i] < current {
            return THUMB_SIZES[i];
        }
    }
    THUMB_SIZES[0]
}

/// Get the next larger thumbnail size
fn larger_thumb_size(current: f32) -> f32 {
    for size in THUMB_SIZES {
        if size > current {
            return size;
        }
    }
    THUMB_SIZES[THUMB_SIZES.len() - 1]
}

/// Draw zoom buttons for thumbnail size control. Returns (zoom_out_clicked, zoom_in_clicked)
fn draw_zoom_buttons(ctx: &mut UiContext, x: f32, y: f32, btn_size: f32, icon_font: Option<&Font>) -> (bool, bool) {
    let mut zoom_out = false;
    let mut zoom_in = false;

    // Zoom out button (smaller thumbnails)
    let out_rect = Rect::new(x, y, btn_size, btn_size);
    let out_hovered = ctx.mouse.inside(&out_rect);
    if out_hovered {
        draw_rectangle(out_rect.x, out_rect.y, out_rect.w, out_rect.h, Color::from_rgba(60, 60, 70, 255));
    }
    let out_color = if out_hovered { WHITE } else { Color::from_rgba(180, 180, 180, 255) };
    draw_icon_centered(icon_font, icon::ZOOM_OUT, &out_rect, 12.0, out_color);
    if ctx.mouse.clicked(&out_rect) {
        zoom_out = true;
    }

    // Zoom in button (larger thumbnails)
    let in_rect = Rect::new(x + btn_size + 2.0, y, btn_size, btn_size);
    let in_hovered = ctx.mouse.inside(&in_rect);
    if in_hovered {
        draw_rectangle(in_rect.x, in_rect.y, in_rect.w, in_rect.h, Color::from_rgba(60, 60, 70, 255));
    }
    let in_color = if in_hovered { WHITE } else { Color::from_rgba(180, 180, 180, 255) };
    draw_icon_centered(icon_font, icon::ZOOM_IN, &in_rect, 12.0, in_color);
    if ctx.mouse.clicked(&in_rect) {
        zoom_in = true;
    }

    (zoom_out, zoom_in)
}

/// Draw the paint section with unified texture editor
fn draw_paint_section(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, icon_font: Option<&Font>) {
    // If editing a texture, show the texture editor
    if state.editing_indexed_atlas {
        draw_paint_texture_editor(ctx, rect, state, icon_font);
    } else {
        // Show the texture browser with New/Edit buttons
        draw_paint_texture_browser(ctx, rect, state, icon_font);
    }
}

/// Draw the texture browser header with New/Edit buttons (matches World Editor)
fn draw_paint_header(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, icon_font: Option<&Font>) {
    draw_rectangle(rect.x.floor(), rect.y.floor(), rect.w, rect.h, Color::from_rgba(40, 40, 45, 255));

    let btn_h = rect.h - 8.0;
    let btn_w = 60.0;
    let btn_y = rect.y + 4.0;

    // "New" button
    let new_rect = Rect::new(rect.x + 4.0, btn_y, btn_w, btn_h);
    let new_hovered = ctx.mouse.inside(&new_rect);
    let new_bg = if new_hovered { Color::from_rgba(70, 70, 80, 255) } else { Color::from_rgba(55, 55, 65, 255) };
    draw_rectangle(new_rect.x, new_rect.y, new_rect.w, new_rect.h, new_bg);
    draw_rectangle_lines(new_rect.x, new_rect.y, new_rect.w, new_rect.h, 1.0, Color::from_rgba(80, 80, 90, 255));

    // Draw plus icon and "New" text
    let icon_rect = Rect::new(new_rect.x + 2.0, new_rect.y, 16.0, new_rect.h);
    draw_icon_centered(icon_font, icon::PLUS, &icon_rect, 12.0, if new_hovered { WHITE } else { Color::from_rgba(200, 200, 200, 255) });
    draw_text("New", (new_rect.x + 18.0).floor(), (new_rect.y + new_rect.h / 2.0 + 4.0).floor(), 12.0, if new_hovered { WHITE } else { Color::from_rgba(200, 200, 200, 255) });

    if ctx.mouse.clicked(&new_rect) {
        // Create a new texture with auto-numbered name (texture_001, texture_002, etc.)
        let name = state.user_textures.next_available_name();
        let tex = UserTexture::new(&name, TextureSize::Size64x64, ClutDepth::Bpp4);
        state.user_textures.add(tex);
        state.editing_texture = Some(state.user_textures.get(&name).unwrap().clone());
        state.editing_indexed_atlas = true;
        state.texture_editor.reset();
    }

    // "Edit" button - edits the selected texture
    let has_selection = state.selected_user_texture.is_some();
    let edit_rect = Rect::new(rect.x + 8.0 + btn_w, btn_y, btn_w, btn_h);
    let edit_hovered = ctx.mouse.inside(&edit_rect);
    let edit_enabled = has_selection;
    let edit_bg = if !edit_enabled {
        Color::from_rgba(40, 40, 45, 255) // Dimmed when disabled
    } else if edit_hovered {
        Color::from_rgba(70, 70, 80, 255)
    } else {
        Color::from_rgba(55, 55, 65, 255)
    };
    draw_rectangle(edit_rect.x, edit_rect.y, edit_rect.w, edit_rect.h, edit_bg);
    draw_rectangle_lines(edit_rect.x, edit_rect.y, edit_rect.w, edit_rect.h, 1.0, Color::from_rgba(80, 80, 90, 255));

    let icon_color = if !edit_enabled {
        Color::from_rgba(100, 100, 100, 255) // Dimmed when disabled
    } else if edit_hovered {
        WHITE
    } else {
        Color::from_rgba(200, 200, 200, 255)
    };
    let icon_rect = Rect::new(edit_rect.x + 2.0, edit_rect.y, 16.0, edit_rect.h);
    draw_icon_centered(icon_font, icon::PENCIL, &icon_rect, 12.0, icon_color);
    draw_text("Edit", (edit_rect.x + 18.0).floor(), (edit_rect.y + edit_rect.h / 2.0 + 4.0).floor(), 12.0, icon_color);

    // Edit button edits the selected texture
    if edit_enabled && ctx.mouse.clicked(&edit_rect) {
        if let Some(name) = &state.selected_user_texture {
            if let Some(tex) = state.user_textures.get(name) {
                state.editing_texture = Some(tex.clone());
                state.editing_indexed_atlas = true;
                state.texture_editor.reset();
            }
        }
    }

    // Texture count on right side
    let count = state.user_textures.len();
    let count_text = format!("{} textures", count);
    let count_dims = measure_text(&count_text, None, 11, 1.0);
    let count_x = (rect.right() - count_dims.width - 8.0).floor();
    draw_text(
        &count_text,
        count_x,
        (rect.y + (rect.h + count_dims.height) / 2.0).floor(),
        11.0,
        Color::from_rgba(150, 150, 150, 255),
    );

    // Zoom buttons - before texture count
    let zoom_btn_size = 20.0;
    let zoom_x = count_x - (zoom_btn_size * 2.0 + 2.0) - 8.0;
    let (zoom_out, zoom_in) = draw_zoom_buttons(ctx, zoom_x, (rect.y + 4.0).round(), zoom_btn_size, icon_font);
    if zoom_out {
        state.paint_thumb_size = smaller_thumb_size(state.paint_thumb_size);
    }
    if zoom_in {
        state.paint_thumb_size = larger_thumb_size(state.paint_thumb_size);
    }
}

/// Draw the texture browser grid (matches World Editor)
fn draw_paint_texture_browser(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, icon_font: Option<&Font>) {
    const HEADER_HEIGHT: f32 = 28.0;
    const THUMB_PADDING: f32 = 4.0;

    // Get thumbnail size from state
    let thumb_size = state.paint_thumb_size;

    // Header with New/Edit buttons
    let header_rect = Rect::new(rect.x, rect.y, rect.w, HEADER_HEIGHT);
    draw_paint_header(ctx, header_rect, state, icon_font);

    // Content area for texture grid
    let content_rect = Rect::new(rect.x, rect.y + HEADER_HEIGHT, rect.w, rect.h - HEADER_HEIGHT);
    let texture_count = state.user_textures.len();

    if texture_count == 0 {
        draw_text(
            "No user textures yet",
            (content_rect.x + 10.0).floor(),
            (content_rect.y + 20.0).floor(),
            14.0,
            Color::from_rgba(100, 100, 100, 255),
        );
        draw_text(
            "Click 'New' to create one",
            (content_rect.x + 10.0).floor(),
            (content_rect.y + 38.0).floor(),
            12.0,
            Color::from_rgba(80, 80, 80, 255),
        );
        return;
    }

    // Calculate grid layout
    let cols = ((content_rect.w - THUMB_PADDING) / (thumb_size + THUMB_PADDING)).floor() as usize;
    let cols = cols.max(1);
    let rows = (texture_count + cols - 1) / cols;
    let total_height = rows as f32 * (thumb_size + THUMB_PADDING) + THUMB_PADDING;

    // Scroll state
    let max_scroll = (total_height - content_rect.h).max(0.0);
    state.paint_texture_scroll = state.paint_texture_scroll.clamp(0.0, max_scroll);

    // Handle mouse wheel scrolling
    if ctx.mouse.inside(&content_rect) {
        state.paint_texture_scroll -= ctx.mouse.scroll * 12.0;
        state.paint_texture_scroll = state.paint_texture_scroll.clamp(0.0, max_scroll);
    }

    // Draw scrollbar if needed
    if total_height > content_rect.h && max_scroll > 0.0 {
        let scrollbar_width = 8.0;
        let scrollbar_x = content_rect.right() - scrollbar_width - 2.0;
        let scrollbar_height = content_rect.h;
        let thumb_height = (content_rect.h / total_height * scrollbar_height).max(20.0);
        let thumb_y = content_rect.y + (state.paint_texture_scroll / max_scroll) * (scrollbar_height - thumb_height);

        draw_rectangle(scrollbar_x, content_rect.y, scrollbar_width, scrollbar_height, Color::from_rgba(15, 15, 20, 255));
        draw_rectangle(scrollbar_x, thumb_y, scrollbar_width, thumb_height, Color::from_rgba(80, 80, 90, 255));
    }

    // Collect texture names first to avoid borrow issues
    let texture_names: Vec<String> = state.user_textures.names().map(|s| s.to_string()).collect();

    // Track clicked/double-clicked textures
    let mut clicked_texture: Option<String> = None;
    let mut double_clicked_texture: Option<String> = None;

    // Enable scissor clipping
    let dpi = screen_dpi_scale();
    gl_use_default_material();
    unsafe {
        get_internal_gl().quad_gl.scissor(Some((
            (content_rect.x * dpi) as i32,
            (content_rect.y * dpi) as i32,
            (content_rect.w * dpi) as i32,
            (content_rect.h * dpi) as i32,
        )));
    }

    for (i, name) in texture_names.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;

        let x = content_rect.x + THUMB_PADDING + col as f32 * (thumb_size + THUMB_PADDING);
        let y = content_rect.y + THUMB_PADDING + row as f32 * (thumb_size + THUMB_PADDING) - state.paint_texture_scroll;

        // Skip if outside visible area
        if y + thumb_size < content_rect.y || y > content_rect.bottom() {
            continue;
        }

        let thumb_rect = Rect::new(x, y, thumb_size, thumb_size);

        // Get texture for rendering
        if let Some(tex) = state.user_textures.get(name) {
            // Draw texture thumbnail
            let mq_tex = user_texture_to_mq_texture(tex);
            draw_texture_ex(
                &mq_tex,
                x,
                y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(macroquad::math::Vec2::new(thumb_size, thumb_size)),
                    ..Default::default()
                },
            );
        } else {
            // Placeholder for missing texture
            draw_rectangle(x, y, thumb_size, thumb_size, Color::from_rgba(60, 60, 70, 255));
        }

        // Check visible portion for click detection
        let visible_rect = Rect::new(
            thumb_rect.x,
            thumb_rect.y.max(content_rect.y),
            thumb_rect.w,
            (thumb_rect.bottom().min(content_rect.bottom()) - thumb_rect.y.max(content_rect.y)).max(0.0),
        );

        if visible_rect.h > 0.0 {
            if ctx.mouse.double_clicked {
                if ctx.mouse.inside(&visible_rect) {
                    double_clicked_texture = Some(name.clone());
                }
            } else if ctx.mouse.clicked(&visible_rect) {
                clicked_texture = Some(name.clone());
            }
        }

        // Check if this texture is selected
        let is_selected = state.selected_user_texture.as_ref() == Some(name);

        // Selection highlight (golden border, like source texture selection)
        if is_selected {
            draw_rectangle_lines(x - 2.0, y - 2.0, thumb_size + 4.0, thumb_size + 4.0, 2.0, Color::from_rgba(255, 200, 50, 255));
        } else if ctx.mouse.inside(&visible_rect) {
            // Hover highlight (only if not selected)
            draw_rectangle_lines(x - 1.0, y - 1.0, thumb_size + 2.0, thumb_size + 2.0, 1.0, Color::from_rgba(150, 150, 200, 255));
        }

        // Draw texture name (truncated if needed)
        if y + thumb_size - 2.0 >= content_rect.y && y + thumb_size - 2.0 <= content_rect.bottom() {
            let display_name = if name.len() > 8 { &name[..8] } else { name };
            draw_text(display_name, (x + 2.0).floor(), (y + thumb_size - 2.0).floor(), 12.0, Color::from_rgba(255, 255, 255, 200));
        }
    }

    // Disable scissor clipping
    unsafe {
        get_internal_gl().quad_gl.scissor(None);
    }

    // Handle single-click to select
    if let Some(name) = clicked_texture {
        state.selected_user_texture = Some(name);
    }

    // Handle double-click to edit (also sets selection)
    if let Some(name) = double_clicked_texture {
        state.selected_user_texture = Some(name.clone());
        if let Some(tex) = state.user_textures.get(&name) {
            state.editing_texture = Some(tex.clone());
            state.editing_indexed_atlas = true;
            state.texture_editor.reset();
        }
    }
}

/// Convert a UserTexture to a macroquad texture for display
fn user_texture_to_mq_texture(texture: &UserTexture) -> Texture2D {
    let mut pixels = Vec::with_capacity(texture.width * texture.height * 4);
    for y in 0..texture.height {
        for x in 0..texture.width {
            let idx = texture.indices[y * texture.width + x] as usize;
            let color = texture.palette.get(idx).copied().unwrap_or_default();
            pixels.push(color.r8());
            pixels.push(color.g8());
            pixels.push(color.b8());
            pixels.push(255); // Full alpha
        }
    }

    let tex = Texture2D::from_rgba8(texture.width as u16, texture.height as u16, &pixels);
    tex.set_filter(FilterMode::Nearest);
    tex
}

/// Draw the texture editor panel (when editing a texture)
fn draw_paint_texture_editor(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, icon_font: Option<&Font>) {
    // Early return if no texture being edited
    if state.editing_texture.is_none() {
        state.editing_indexed_atlas = false;
        return;
    }

    // Build UV overlay data BEFORE getting mutable borrow of tex (avoids borrow conflict)
    let uv_data = if state.texture_editor.mode == TextureEditorMode::Uv {
        build_uv_overlay_data(state)
    } else {
        None
    };

    // Now get mutable reference to the texture
    let tex = state.editing_texture.as_mut().unwrap();
    // Extract dimensions for later use (to avoid borrow conflicts)
    let tex_width_f = tex.width as f32;
    let tex_height_f = tex.height as f32;

    // Header with texture name and buttons (match main toolbar sizing: 36px height, 32px buttons, 16px icons)
    let header_h = 36.0;
    let btn_size = 32.0;
    let icon_size = 16.0;
    let header_rect = Rect::new(rect.x, rect.y, rect.w, header_h);
    draw_rectangle(header_rect.x, header_rect.y, header_rect.w, header_rect.h, Color::from_rgba(45, 45, 55, 255));

    // Back button (arrow-big-left) - closes editor
    let back_rect = Rect::new(rect.right() - btn_size - 2.0, rect.y + 2.0, btn_size, btn_size);
    let back_hovered = ctx.mouse.inside(&back_rect);
    if back_hovered {
        draw_rectangle(back_rect.x, back_rect.y, back_rect.w, back_rect.h, Color::from_rgba(80, 60, 60, 255));
    }
    draw_icon_centered(icon_font, icon::ARROW_BIG_LEFT, &back_rect, icon_size, if back_hovered { WHITE } else { Color::from_rgba(200, 200, 200, 255) });

    if ctx.mouse.clicked(&back_rect) {
        // TODO: Sync texture back to atlas if needed
        state.editing_texture = None;
        state.editing_indexed_atlas = false;
        return;
    }

    // Save button
    let save_rect = Rect::new(back_rect.x - btn_size - 2.0, rect.y + 2.0, btn_size, btn_size);
    let save_hovered = ctx.mouse.inside(&save_rect);
    if save_hovered {
        draw_rectangle(save_rect.x, save_rect.y, save_rect.w, save_rect.h, Color::from_rgba(60, 80, 60, 255));
    }
    draw_icon_centered(icon_font, icon::SAVE, &save_rect, icon_size, if save_hovered { WHITE } else { Color::from_rgba(200, 200, 200, 255) });

    // Get texture name for save and display (clone before we use tex mutably)
    let tex_name = tex.name.clone();

    if ctx.mouse.clicked(&save_rect) {
        // Save to user textures library
        if let Err(e) = state.user_textures.save_texture(&tex_name) {
            eprintln!("Failed to save texture: {}", e);
        }
    }

    // Texture name (vertically centered in header)
    let name_text = format!("Editing: {}", tex_name);
    draw_text(&name_text, (header_rect.x + 8.0).floor(), (header_rect.y + header_h / 2.0 + 4.0).floor(), 12.0, WHITE);

    // Content area below header
    let content_rect_full = Rect::new(rect.x, rect.y + header_h, rect.w, rect.h - header_h);

    // Draw mode tabs (Paint / UV) at the top
    let content_rect = draw_mode_tabs(ctx, content_rect_full, &mut state.texture_editor);

    // Layout: Canvas (square, capped size) + Tool panel (right), Palette panel (below, gets remaining space)
    // This matches the World Editor's texture_palette.rs layout exactly
    let tool_panel_w = 66.0;  // 2-column layout: 2 * 28px buttons + 2px gap + 4px padding each side
    let canvas_w = content_rect.w - tool_panel_w;
    // Tool panel needs ~280px height (6 tools + undo/redo/zoom/grid + size/shape options)
    // Palette needs: depth buttons (~22) + gen row (~24) + grid (~65) + color editor (~60) + effect (~18) = ~190
    let min_canvas_h = 280.0;  // Minimum for tool panel to fit all buttons
    let min_palette_h = 190.0;
    let max_canvas_h = (content_rect.h - min_palette_h).min(canvas_w).max(min_canvas_h);
    let canvas_h = max_canvas_h;
    let palette_panel_h = (content_rect.h - canvas_h).max(min_palette_h);  // Remaining space goes to palette

    let canvas_rect = Rect::new(content_rect.x, content_rect.y, canvas_w, canvas_h);
    let tool_rect = Rect::new(content_rect.x + canvas_w, content_rect.y, tool_panel_w, canvas_h);
    let palette_rect = Rect::new(content_rect.x, content_rect.y + canvas_h, content_rect.w, palette_panel_h);

    // Draw panels using the shared texture editor components
    draw_texture_canvas(ctx, canvas_rect, tex, &mut state.texture_editor, uv_data.as_ref());
    draw_tool_panel(ctx, tool_rect, &mut state.texture_editor, icon_font);
    draw_palette_panel(ctx, palette_rect, tex, &mut state.texture_editor, icon_font);

    // Handle UV modal transforms (G/S/R) - apply to actual mesh vertices
    apply_uv_modal_transform(ctx, &canvas_rect, tex_width_f, tex_height_f, state);

    // Handle direct UV dragging (with pixel snapping)
    apply_uv_direct_drag(ctx, &canvas_rect, tex_width_f, tex_height_f, state);

    // Handle UV operations (flip/rotate/reset buttons)
    apply_uv_operation(tex_width_f, tex_height_f, state);

    // Handle undo save signals from texture editor (save BEFORE the action is applied)
    if state.texture_editor.undo_save_pending.take().is_some() {
        state.save_texture_undo();
    }

    // Handle undo/redo button requests (uses global undo system)
    if state.texture_editor.undo_requested {
        state.texture_editor.undo_requested = false;
        state.undo();
    }
    if state.texture_editor.redo_requested {
        state.texture_editor.redo_requested = false;
        state.redo();
    }
}

/// Apply UV modal transforms (G/S/R) to actual mesh vertices
fn apply_uv_modal_transform(
    ctx: &UiContext,
    canvas_rect: &Rect,
    tex_width: f32,
    tex_height: f32,
    state: &mut ModelerState,
) {
    use crate::texture::UvModalTransform;

    let transform = state.texture_editor.uv_modal_transform;
    if transform == UvModalTransform::None {
        return;
    }

    // Get the mesh to modify
    let obj = match state.project.selected_mut() {
        Some(o) => o,
        None => return,
    };

    // Calculate transform parameters
    let zoom = state.texture_editor.zoom;
    let pan_x = state.texture_editor.pan_x;
    let pan_y = state.texture_editor.pan_y;
    let (start_mx, start_my) = state.texture_editor.uv_modal_start_mouse;

    // Calculate texture position on screen
    let canvas_cx = canvas_rect.x + canvas_rect.w / 2.0;
    let canvas_cy = canvas_rect.y + canvas_rect.h / 2.0;
    let tex_x = canvas_cx - tex_width * zoom / 2.0 + pan_x;
    let tex_y = canvas_cy - tex_height * zoom / 2.0 + pan_y;

    // Screen delta in UV space
    let delta_screen_x = ctx.mouse.x - start_mx;
    let delta_screen_y = ctx.mouse.y - start_my;
    let delta_u = delta_screen_x / (tex_width * zoom);
    let delta_v = -delta_screen_y / (tex_height * zoom); // Inverted Y

    match transform {
        UvModalTransform::Grab => {
            // Move selected vertices by delta with pixel snapping
            for (vi, original_uv) in &state.texture_editor.uv_modal_start_uvs {
                if let Some(v) = obj.mesh.vertices.get_mut(*vi) {
                    let new_u = original_uv.x + delta_u;
                    let new_v = original_uv.y + delta_v;
                    // Snap to pixel boundaries
                    v.uv.x = (new_u * tex_width).round() / tex_width;
                    v.uv.y = (new_v * tex_height).round() / tex_height;
                }
            }
            state.dirty = true;
        }
        UvModalTransform::Scale => {
            // Scale around center with pixel snapping
            let center = state.texture_editor.uv_modal_center;
            // Scale factor based on horizontal mouse movement
            let scale = 1.0 + delta_screen_x * 0.01;
            let scale = scale.max(0.01); // Prevent negative/zero scale

            for (vi, original_uv) in &state.texture_editor.uv_modal_start_uvs {
                if let Some(v) = obj.mesh.vertices.get_mut(*vi) {
                    let offset_x = original_uv.x - center.x;
                    let offset_y = original_uv.y - center.y;
                    let new_u = center.x + offset_x * scale;
                    let new_v = center.y + offset_y * scale;
                    // Snap to pixel boundaries
                    v.uv.x = (new_u * tex_width).round() / tex_width;
                    v.uv.y = (new_v * tex_height).round() / tex_height;
                }
            }
            state.dirty = true;
        }
        UvModalTransform::Rotate => {
            // Rotate around center with pixel snapping
            let center = state.texture_editor.uv_modal_center;
            // Rotation angle based on horizontal mouse movement
            let angle = delta_screen_x * 0.01; // Radians
            let cos_a = angle.cos();
            let sin_a = angle.sin();

            for (vi, original_uv) in &state.texture_editor.uv_modal_start_uvs {
                if let Some(v) = obj.mesh.vertices.get_mut(*vi) {
                    let offset_x = original_uv.x - center.x;
                    let offset_y = original_uv.y - center.y;
                    let new_u = center.x + offset_x * cos_a - offset_y * sin_a;
                    let new_v = center.y + offset_x * sin_a + offset_y * cos_a;
                    // Snap to pixel boundaries
                    v.uv.x = (new_u * tex_width).round() / tex_width;
                    v.uv.y = (new_v * tex_height).round() / tex_height;
                }
            }
            state.dirty = true;
        }
        UvModalTransform::None => {}
    }
}

/// Apply direct UV drag with pixel snapping
fn apply_uv_direct_drag(
    ctx: &UiContext,
    _canvas_rect: &Rect,
    tex_width: f32,
    tex_height: f32,
    state: &mut ModelerState,
) {
    if !state.texture_editor.uv_drag_active {
        return;
    }

    // Get the mesh to modify
    let obj = match state.project.selected_mut() {
        Some(o) => o,
        None => return,
    };

    // Calculate transform parameters
    let zoom = state.texture_editor.zoom;
    let (start_mx, start_my) = state.texture_editor.uv_drag_start;

    // Screen delta in UV space
    let delta_screen_x = ctx.mouse.x - start_mx;
    let delta_screen_y = ctx.mouse.y - start_my;
    let delta_u = delta_screen_x / (tex_width * zoom);
    let delta_v = -delta_screen_y / (tex_height * zoom); // Inverted Y

    // Move selected vertices by delta with pixel snapping
    for &(_, vi, original_uv) in &state.texture_editor.uv_drag_start_uvs {
        if let Some(v) = obj.mesh.vertices.get_mut(vi) {
            // Calculate new UV
            let new_u = original_uv.x + delta_u;
            let new_v = original_uv.y + delta_v;

            // Snap to pixel boundaries
            // UV coords are 0-1, pixels are 0 to tex_width-1
            // Snap u to n/tex_width and v to m/tex_height
            let snapped_u = (new_u * tex_width).round() / tex_width;
            let snapped_v = (new_v * tex_height).round() / tex_height;

            v.uv.x = snapped_u;
            v.uv.y = snapped_v;
        }
    }
    state.dirty = true;
}

/// Apply a UV operation (flip/rotate/reset) to selected vertices
fn apply_uv_operation(
    tex_width: f32,
    tex_height: f32,
    state: &mut ModelerState,
) {
    use crate::texture::UvOperation;

    let operation = match state.texture_editor.uv_operation_pending.take() {
        Some(op) => op,
        None => return,
    };

    // Get the mesh to modify
    let obj = match state.project.selected_mut() {
        Some(o) => o,
        None => return,
    };

    // Get selected vertex indices
    let selected_vertices = state.texture_editor.uv_selection.clone();
    if selected_vertices.is_empty() {
        state.texture_editor.set_status("No vertices selected");
        return;
    }

    // Calculate center of selection (for flip/rotate operations)
    let mut center_u = 0.0f32;
    let mut center_v = 0.0f32;
    let mut count = 0;
    for &vi in &selected_vertices {
        if let Some(v) = obj.mesh.vertices.get(vi) {
            center_u += v.uv.x;
            center_v += v.uv.y;
            count += 1;
        }
    }
    if count > 0 {
        center_u /= count as f32;
        center_v /= count as f32;
    }

    match operation {
        UvOperation::FlipHorizontal => {
            // Flip UVs horizontally around center
            for &vi in &selected_vertices {
                if let Some(v) = obj.mesh.vertices.get_mut(vi) {
                    let offset = v.uv.x - center_u;
                    let new_u = center_u - offset;
                    // Snap to pixels
                    v.uv.x = (new_u * tex_width).round() / tex_width;
                }
            }
        }
        UvOperation::FlipVertical => {
            // Flip UVs vertically around center
            for &vi in &selected_vertices {
                if let Some(v) = obj.mesh.vertices.get_mut(vi) {
                    let offset = v.uv.y - center_v;
                    let new_v = center_v - offset;
                    // Snap to pixels
                    v.uv.y = (new_v * tex_height).round() / tex_height;
                }
            }
        }
        UvOperation::RotateCW => {
            // Rotate UVs 90 degrees clockwise around center
            for &vi in &selected_vertices {
                if let Some(v) = obj.mesh.vertices.get_mut(vi) {
                    let offset_u = v.uv.x - center_u;
                    let offset_v = v.uv.y - center_v;
                    // 90 deg CW rotation: (x, y) -> (y, -x)
                    let new_u = center_u + offset_v;
                    let new_v = center_v - offset_u;
                    // Snap to pixels
                    v.uv.x = (new_u * tex_width).round() / tex_width;
                    v.uv.y = (new_v * tex_height).round() / tex_height;
                }
            }
        }
        UvOperation::ResetUVs => {
            // Reset UVs to default positions (0-1 range covering texture)
            // For a typical face, distribute vertices evenly
            // Simple approach: reset to corners of unit square based on vertex order
            let defaults = [
                (0.0, 0.0),  // bottom-left
                (1.0, 0.0),  // bottom-right
                (1.0, 1.0),  // top-right
                (0.0, 1.0),  // top-left
            ];
            for (i, &vi) in selected_vertices.iter().enumerate() {
                if let Some(v) = obj.mesh.vertices.get_mut(vi) {
                    let (u, vv) = defaults[i % defaults.len()];
                    v.uv.x = u;
                    v.uv.y = vv;
                }
            }
        }
    }

    state.dirty = true;
}

/// Build UV overlay data from currently selected faces
fn build_uv_overlay_data(state: &ModelerState) -> Option<UvOverlayData> {
    let obj = state.project.selected()?;

    // Get selected face indices
    let selected_faces = match &state.selection {
        super::state::ModelerSelection::Faces(indices) => indices.clone(),
        _ => return None, // No faces selected
    };

    if selected_faces.is_empty() {
        return None;
    }

    // Collect all unique vertices from selected faces
    let mut vertex_map: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    let mut vertices = Vec::new();
    let mut faces = Vec::new();

    for &fi in &selected_faces {
        if let Some(face) = obj.mesh.faces.get(fi) {
            let mut face_vertex_indices = Vec::new();

            for &vi in &face.vertices {
                let uv_idx = if let Some(&existing_idx) = vertex_map.get(&vi) {
                    existing_idx
                } else {
                    let new_idx = vertices.len();
                    if let Some(v) = obj.mesh.vertices.get(vi) {
                        vertices.push(UvVertex {
                            uv: v.uv,
                            vertex_index: vi,
                        });
                    }
                    vertex_map.insert(vi, new_idx);
                    new_idx
                };
                face_vertex_indices.push(uv_idx);
            }

            faces.push(UvFace {
                vertex_indices: face_vertex_indices,
            });
        }
    }

    let num_faces = faces.len();
    Some(UvOverlayData {
        vertices,
        faces,
        selected_faces: (0..num_faces).collect(), // All faces in our list are "selected"
    })
}

/// Handle UV-specific interactions (separate from paint)
fn handle_uv_interaction(ctx: &mut UiContext, atlas_rect: Rect, _scale: f32, _state: &mut ModelerState) {
    let (mx, my) = (ctx.mouse.x, ctx.mouse.y);
    let inside_atlas = atlas_rect.contains(mx, my);

    if !inside_atlas {
        return;
    }

    // TODO: UV vertex selection and dragging logic
    // This will be implemented when we add full UV editing to the collapsible section
}

/// Draw the atlas preview (texture + UV overlay)
fn draw_atlas_preview(
    _ctx: &mut UiContext,
    atlas_x: f32,
    atlas_y: f32,
    atlas_screen_w: f32,
    atlas_screen_h: f32,
    scale: f32,
    state: &ModelerState,
) {
    let atlas = &state.project.atlas;
    let atlas_width = atlas.width;
    let atlas_height = atlas.height;

    // Get CLUT for rendering atlas preview
    let clut = state.project.effective_clut();

    // Draw checkerboard background
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

    // Draw texture pixels (indexed - use CLUT to get colors)
    let pixels_per_block = (1.0 / scale).max(1.0) as usize;
    if let Some(clut) = clut {
        for by in (0..atlas_height).step_by(pixels_per_block.max(1)) {
            for bx in (0..atlas_width).step_by(pixels_per_block.max(1)) {
                let index = atlas.get_index(bx, by);
                // Index 0 = transparent (PS1 convention)
                if index == 0 {
                    continue;
                }
                let c15 = clut.lookup(index);
                let px = atlas_x + bx as f32 * scale;
                let py = atlas_y + by as f32 * scale;
                let pw = (pixels_per_block as f32 * scale).min(atlas_x + atlas_screen_w - px).max(scale);
                let ph = (pixels_per_block as f32 * scale).min(atlas_y + atlas_screen_h - py).max(scale);
                if pw > 0.0 && ph > 0.0 {
                    // Convert Color15 (5-bit per channel) to 8-bit
                    let r = (c15.r5() << 3) | (c15.r5() >> 2);
                    let g = (c15.g5() << 3) | (c15.g5() >> 2);
                    let b = (c15.b5() << 3) | (c15.b5() >> 2);
                    draw_rectangle(px, py, pw, ph, Color::from_rgba(r, g, b, 255));
                }
            }
        }
    }

    // Draw border
    draw_rectangle_lines(atlas_x, atlas_y, atlas_screen_w, atlas_screen_h, 1.0, Color::from_rgba(100, 100, 105, 255));

    // Draw UV overlay for selected faces
    let atlas_w = atlas_width as f32;
    let atlas_h = atlas_height as f32;

    let uv_to_screen = |u: f32, v: f32| -> (f32, f32) {
        let px = (u * atlas_w).floor();
        let py = ((1.0 - v) * atlas_h).floor();
        (atlas_x + (px + 0.5) * scale, atlas_y + (py + 0.5) * scale)
    };

    if let Some(obj) = state.project.selected() {
        let face_edge_color = Color::from_rgba(255, 200, 100, 255);
        let vertex_color = Color::from_rgba(255, 255, 255, 255);
        let selected_vertex_color = Color::from_rgba(100, 200, 255, 255);

        if let super::state::ModelerSelection::Faces(selected_faces) = &state.selection {
            for &fi in selected_faces {
                if let Some(face) = obj.mesh.faces.get(fi) {
                    // Collect screen UVs for all vertices of n-gon
                    let screen_uvs: Vec<_> = face.vertices.iter()
                        .filter_map(|&vi| obj.mesh.vertices.get(vi))
                        .map(|v| uv_to_screen(v.uv.x, v.uv.y))
                        .collect();

                    // Draw edges (all edges of n-gon)
                    let n = screen_uvs.len();
                    for i in 0..n {
                        let j = (i + 1) % n;
                        draw_line(
                            screen_uvs[i].0, screen_uvs[i].1,
                            screen_uvs[j].0, screen_uvs[j].1,
                            2.0, face_edge_color,
                        );
                    }

                    // Draw vertices (only when UV section is expanded)
                    if state.uv_section_expanded {
                        for (i, &vi) in face.vertices.iter().enumerate() {
                            if let Some((sx, sy)) = screen_uvs.get(i) {
                                let is_selected = state.uv_selection.contains(&vi);
                                let color = if is_selected { selected_vertex_color } else { vertex_color };
                                let size = if is_selected { 8.0 } else { 6.0 };
                                draw_rectangle(sx - size * 0.5, sy - size * 0.5, size, size, color);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Draw atlas size selector buttons
fn draw_atlas_size_selector(ctx: &mut UiContext, x: f32, y: &mut f32, _width: f32, state: &mut ModelerState) {
    let sizes = [(32, "32"), (64, "64"), (128, "128"), (256, "256")];
    let btn_h = 18.0;
    let btn_spacing = 2.0;

    draw_text("Size:", x + 4.0, *y + 12.0, 12.0, TEXT_DIM);

    let mut btn_x = x + 32.0;
    for (size, label) in sizes {
        let btn_w = label.len() as f32 * 7.0 + 6.0;
        let btn_rect = Rect::new(btn_x, *y, btn_w, btn_h);
        let is_current = state.project.atlas.width == size;
        let hovered = ctx.mouse.inside(&btn_rect);

        let bg = if is_current {
            ACCENT_COLOR
        } else if hovered {
            Color::from_rgba(70, 70, 75, 255)
        } else {
            Color::from_rgba(50, 50, 55, 255)
        };
        draw_rectangle(btn_rect.x, btn_rect.y, btn_rect.w, btn_rect.h, bg);

        let text_color = if is_current { WHITE } else { TEXT_DIM };
        draw_text(label, btn_x + 3.0, *y + 12.0, 12.0, text_color);

        if hovered && ctx.mouse.left_pressed && !is_current {
            state.push_undo_with_atlas("Resize Atlas");
            state.project.atlas.resize(size, size);
            state.dirty = true;
        }

        btn_x += btn_w + btn_spacing;
    }

    *y += btn_h + 4.0;
}

// NOTE: draw_mode_toggle removed - replaced by collapsible sections in draw_right_panel

/// Draw UV tools content (without mode toggle - that's now at top)
fn draw_uv_tools_content(ctx: &mut UiContext, x: f32, y: &mut f32, _width: f32, state: &mut ModelerState, icon_font: Option<&Font>) {
    let btn_size = 20.0;
    let btn_spacing = 2.0;

    // Transform buttons
    let has_selection = !state.uv_selection.is_empty();
    let buttons: &[(char, &str, &str)] = &[
        (icon::FLIP_HORIZONTAL, "Flip H", "uv_flip_h"),
        (icon::FLIP_VERTICAL, "Flip V", "uv_flip_v"),
        (icon::ROTATE_CW, "Rotate CW", "uv_rot_cw"),
        (icon::REFRESH_CW, "Reset UVs", "uv_reset"),
    ];

    let mut btn_x = x + 4.0;
    for (icon_char, tooltip, action) in buttons {
        let btn_rect = Rect::new(btn_x, *y, btn_size, btn_size);
        let hovered = ctx.mouse.inside(&btn_rect);

        let bg = if !has_selection {
            Color::from_rgba(40, 40, 45, 255)
        } else if hovered {
            Color::from_rgba(80, 80, 90, 255)
        } else {
            Color::from_rgba(55, 55, 60, 255)
        };
        draw_rectangle(btn_rect.x, btn_rect.y, btn_rect.w, btn_rect.h, bg);

        let icon_color = if has_selection { TEXT_COLOR } else { Color::from_rgba(80, 80, 85, 255) };
        draw_icon_centered(icon_font, *icon_char, &btn_rect, 12.0, icon_color);

        if has_selection && hovered && is_mouse_button_pressed(MouseButton::Left) {
            match *action {
                "uv_flip_h" => flip_selected_uvs(state, true, false),
                "uv_flip_v" => flip_selected_uvs(state, false, true),
                "uv_rot_cw" => rotate_selected_uvs(state, true),
                "uv_reset" => reset_selected_uvs(state),
                _ => {}
            }
        }

        if hovered {
            ctx.set_tooltip(tooltip, ctx.mouse.x, ctx.mouse.y);
        }

        btn_x += btn_size + btn_spacing;
    }

    *y += btn_size + 4.0;
}

/// Draw paint tools content (without mode toggle - that's now at top)
fn draw_paint_tools_content(ctx: &mut UiContext, x: f32, y: &mut f32, width: f32, state: &mut ModelerState, icon_font: Option<&Font>) {
    let btn_size = 20.0;

    // Brush type buttons
    let brush_rect = Rect::new(x + 4.0, *y, btn_size, btn_size);
    let brush_selected = state.brush_type == super::state::BrushType::Square;
    if icon_button_active(ctx, brush_rect, icon::BRUSH, icon_font, "Brush (B)", brush_selected) {
        state.brush_type = super::state::BrushType::Square;
    }

    let fill_rect = Rect::new(brush_rect.x + btn_size + 2.0, *y, btn_size, btn_size);
    let fill_selected = state.brush_type == super::state::BrushType::Fill;
    if icon_button_active(ctx, fill_rect, icon::PAINT_BUCKET, icon_font, "Fill (F)", fill_selected) {
        state.brush_type = super::state::BrushType::Fill;
    }

    // Brush size slider (for square brush)
    if state.brush_type == super::state::BrushType::Square {
        let slider_x = fill_rect.x + btn_size + 8.0;
        let slider_w = width - (slider_x - x) - 30.0;
        let slider_h = 12.0;
        let slider_y = *y + (btn_size - slider_h) / 2.0;

        let track_rect = Rect::new(slider_x, slider_y, slider_w.max(40.0), slider_h);
        draw_rectangle(track_rect.x, track_rect.y, track_rect.w, track_rect.h, Color::from_rgba(40, 40, 45, 255));

        let min_size = 1.0;
        let max_size = 16.0;
        let fill_ratio = (state.brush_size - min_size) / (max_size - min_size);
        let fill_width = fill_ratio * track_rect.w;
        draw_rectangle(track_rect.x, track_rect.y, fill_width, slider_h, ACCENT_COLOR);

        draw_text(&format!("{}", state.brush_size as i32), slider_x + track_rect.w + 4.0, *y + 14.0, 12.0, TEXT_DIM);

        // Handle slider interaction
        if ctx.mouse.inside(&track_rect) && ctx.mouse.left_down && !state.brush_size_slider_active {
            state.brush_size_slider_active = true;
        }
        if state.brush_size_slider_active {
            if ctx.mouse.left_down {
                let rel_x = (ctx.mouse.x - track_rect.x).clamp(0.0, track_rect.w);
                state.brush_size = (min_size + (rel_x / track_rect.w) * (max_size - min_size)).round().clamp(min_size, max_size);
            } else {
                state.brush_size_slider_active = false;
            }
        }
    }

    *y += btn_size + 4.0;

    // CLUT Editor section (only in Paint mode)
    draw_section_label(x, y, width, "CLUT");
    let clut_height = 200.0; // Fixed height for CLUT editor
    draw_clut_editor_panel(ctx, x, *y, width, clut_height, state, icon_font);
    *y += clut_height + 4.0;
}

/// Draw face properties section (blend mode for selected faces)
fn draw_face_properties(ctx: &mut UiContext, x: f32, y: &mut f32, _width: f32, state: &mut ModelerState, _icon_font: Option<&Font>) {
    use crate::rasterizer::BlendMode;

    // Get selected face indices
    let face_indices = if let super::state::ModelerSelection::Faces(indices) = &state.selection {
        indices.clone()
    } else {
        return;
    };

    if face_indices.is_empty() {
        return;
    }

    // Get current blend mode from first selected face
    let current_blend = face_indices.first()
        .and_then(|&idx| state.mesh().faces.get(idx))
        .map(|f| f.blend_mode)
        .unwrap_or(BlendMode::Opaque);

    // Check if all selected faces have the same blend mode
    let all_same = face_indices.iter()
        .filter_map(|&idx| state.mesh().faces.get(idx))
        .all(|f| f.blend_mode == current_blend);

    // Blend mode label
    draw_text("Blend:", x + 4.0, *y + 12.0, 13.0, TEXT_DIM);

    // Blend mode buttons (inline row)
    let btn_modes = [
        (BlendMode::Opaque, "O", "Opaque"),
        (BlendMode::Average, "A", "Average (50/50)"),
        (BlendMode::Add, "+", "Additive"),
        (BlendMode::Subtract, "-", "Subtractive"),
        (BlendMode::AddQuarter, "Q", "Quarter-Add"),
    ];

    let btn_width = 22.0;
    let btn_height = 18.0;
    let mut bx = x + 40.0;

    for (mode, label, tooltip) in btn_modes.iter() {
        let btn_rect = Rect::new(bx, *y, btn_width, btn_height);
        let is_selected = all_same && current_blend == *mode;
        let hovered = ctx.mouse.inside(&btn_rect);

        // Draw button background
        let bg_color = if is_selected {
            Color::from_rgba(70, 130, 180, 255)
        } else if hovered {
            Color::from_rgba(60, 60, 68, 255)
        } else {
            Color::from_rgba(50, 50, 58, 255)
        };
        draw_rectangle(btn_rect.x, btn_rect.y, btn_rect.w, btn_rect.h, bg_color);
        draw_rectangle_lines(btn_rect.x, btn_rect.y, btn_rect.w, btn_rect.h, 1.0, Color::from_rgba(80, 80, 90, 255));

        // Draw label
        let text_color = if is_selected { WHITE } else { TEXT_COLOR };
        let text_x = btn_rect.x + (btn_rect.w - measure_text(label, None, 10, 1.0).width) / 2.0;
        draw_text(label, text_x, btn_rect.y + 13.0, 12.0, text_color);

        // Tooltip
        if hovered {
            ctx.set_tooltip(tooltip, ctx.mouse.x, ctx.mouse.y);
        }

        // Click handler
        if ctx.mouse.clicked(&btn_rect) {
            // Apply to all selected faces
            if let Some(mesh) = state.mesh_mut() {
                for &face_idx in &face_indices {
                    if let Some(face) = mesh.faces.get_mut(face_idx) {
                        face.blend_mode = *mode;
                    }
                }
            }
            state.dirty = true;
        }

        bx += btn_width + 2.0;
    }

    *y += btn_height + 4.0;

    // Show "Mixed" indicator if faces have different blend modes
    if !all_same {
        draw_text("(Mixed)", x + 4.0, *y + 10.0, 12.0, Color::from_rgba(180, 140, 60, 255));
        *y += 14.0;
    }
}

// NOTE: draw_uv_tools_section removed - replaced by draw_uv_tools_content in UV collapsible section

// NOTE: draw_paint_tools_section removed - replaced by draw_paint_section using unified texture editor

// NOTE: handle_atlas_interaction removed - replaced by unified texture editor in Paint section

// NOTE: draw_texture_uv_panel removed - replaced by collapsible UV and Paint sections in draw_right_panel
// The UV section displays atlas preview with UV overlay and UV tools
// The Paint section uses the unified texture editor from src/texture/texture_editor.rs


/// Draw the CLUT (Color Look-Up Table) editor panel
/// Shows: CLUT pool list, palette grid (4x4 or 16x16), color editor (RGB555 sliders)
fn draw_clut_editor_panel(
    ctx: &mut UiContext,
    x: f32,
    y: f32,
    width: f32,
    _height: f32,
    state: &mut ModelerState,
    _icon_font: Option<&Font>,
) {
    let padding = 4.0;
    let mut cur_y = y + padding;

    // ========================================================================
    // Section 1: CLUT Pool List with buttons
    // ========================================================================
    draw_text("CLUT Pool", x + padding, cur_y + 10.0, 13.0, TEXT_DIM);
    cur_y += 14.0;

    // Buttons to add new CLUTs
    let btn_h = 18.0;
    let btn_w = 50.0;

    // [+ 4-bit] button
    let btn_4bit_rect = Rect::new(x + padding, cur_y, btn_w, btn_h);
    let hovered_4bit = ctx.mouse.inside(&btn_4bit_rect);
    let bg_4bit = if hovered_4bit {
        Color::from_rgba(70, 70, 80, 255)
    } else {
        Color::from_rgba(50, 50, 55, 255)
    };
    draw_rectangle(btn_4bit_rect.x, btn_4bit_rect.y, btn_4bit_rect.w, btn_4bit_rect.h, bg_4bit);
    draw_text("+ 4-bit", x + padding + 4.0, cur_y + 13.0, 12.0, TEXT_COLOR);
    if hovered_4bit {
        ctx.set_tooltip("Add 4-bit CLUT (16 colors)", ctx.mouse.x, ctx.mouse.y);
    }

    if hovered_4bit && ctx.mouse.left_pressed {
        let clut = Clut::new_4bit(format!("CLUT {}", state.project.clut_pool.len() + 1));
        let id = state.project.clut_pool.add_clut(clut);
        state.selected_clut = Some(id);
        state.set_status("Added 4-bit CLUT", 1.0);
    }

    // [+ 8-bit] button
    let btn_8bit_rect = Rect::new(x + padding + btn_w + 4.0, cur_y, btn_w, btn_h);
    let hovered_8bit = ctx.mouse.inside(&btn_8bit_rect);
    let bg_8bit = if hovered_8bit {
        Color::from_rgba(70, 70, 80, 255)
    } else {
        Color::from_rgba(50, 50, 55, 255)
    };
    draw_rectangle(btn_8bit_rect.x, btn_8bit_rect.y, btn_8bit_rect.w, btn_8bit_rect.h, bg_8bit);
    draw_text("+ 8-bit", btn_8bit_rect.x + 4.0, cur_y + 13.0, 12.0, TEXT_COLOR);
    if hovered_8bit {
        ctx.set_tooltip("Add 8-bit CLUT (256 colors)", ctx.mouse.x, ctx.mouse.y);
    }

    if hovered_8bit && ctx.mouse.left_pressed {
        let clut = Clut::new_8bit(format!("CLUT {}", state.project.clut_pool.len() + 1));
        let id = state.project.clut_pool.add_clut(clut);
        state.selected_clut = Some(id);
        state.set_status("Added 8-bit CLUT", 1.0);
    }

    cur_y += btn_h + 4.0;

    // List of CLUTs in pool
    let list_height = 40.0;
    let item_height = 16.0;

    // Draw list background
    draw_rectangle(x + padding, cur_y, width - padding * 2.0, list_height, Color::from_rgba(30, 30, 35, 255));

    // Draw CLUT items
    let clut_count = state.project.clut_pool.len();
    if clut_count == 0 {
        draw_text("(empty)", x + padding + 4.0, cur_y + 12.0, 12.0, TEXT_DIM);
    } else {
        let mut item_y = cur_y + 2.0;
        for clut in state.project.clut_pool.iter() {
            if item_y + item_height > cur_y + list_height {
                break; // Scroll limit
            }

            let item_rect = Rect::new(x + padding + 2.0, item_y, width - padding * 2.0 - 4.0, item_height);
            let is_selected = state.selected_clut == Some(clut.id);
            let hovered = ctx.mouse.inside(&item_rect);

            // Background
            let bg = if is_selected {
                ACCENT_COLOR
            } else if hovered {
                Color::from_rgba(50, 50, 55, 255)
            } else {
                Color::from_rgba(30, 30, 35, 0)
            };
            draw_rectangle(item_rect.x, item_rect.y, item_rect.w, item_rect.h, bg);

            // Name + depth badge
            let text_color = if is_selected { WHITE } else { TEXT_COLOR };
            draw_text(&clut.name, item_rect.x + 2.0, item_y + 11.0, 12.0, text_color);

            // Depth badge
            let badge_text = clut.depth.short_label();
            let badge_x = item_rect.x + item_rect.w - 24.0;
            draw_rectangle(badge_x, item_y + 2.0, 20.0, 12.0, Color::from_rgba(60, 60, 70, 255));
            draw_text(badge_text, badge_x + 2.0, item_y + 11.0, 13.0, TEXT_DIM);

            // Handle click
            if hovered && ctx.mouse.left_pressed {
                state.selected_clut = Some(clut.id);
                state.selected_clut_entry = 0;
            }

            item_y += item_height;
        }
    }

    cur_y += list_height + 4.0;

    // ========================================================================
    // Section 2: Palette Grid (4x4 for 4-bit, 16x16 for 8-bit)
    // ========================================================================
    if let Some(clut_id) = state.selected_clut {
        if let Some(clut) = state.project.clut_pool.get(clut_id) {
            // Draw palette grid
            let grid_size = match clut.depth {
                ClutDepth::Bpp4 => 4,  // 4x4 grid
                ClutDepth::Bpp8 => 16, // 16x16 grid
            };

            // Fill available width (prioritize using full width for the palette)
            let cell_size = (width - padding * 2.0) / grid_size as f32;

            let grid_w = cell_size * grid_size as f32;
            let grid_x = x + (width - grid_w) * 0.5;
            let grid_y = cur_y;

            for gy in 0..grid_size {
                for gx in 0..grid_size {
                    let idx = gy * grid_size + gx;
                    if idx >= clut.colors.len() {
                        break;
                    }

                    let cell_x = grid_x + gx as f32 * cell_size;
                    let cell_y = grid_y + gy as f32 * cell_size;
                    let cell_rect = Rect::new(cell_x, cell_y, cell_size, cell_size);

                    // Get color
                    let color15 = clut.colors[idx];
                    let (r8, g8, b8) = (color15.r8(), color15.g8(), color15.b8());

                    // Draw color cell
                    if color15.is_transparent() {
                        // Checkerboard for transparent
                        let check = 4.0;
                        for cy in 0..2 {
                            for cx in 0..2 {
                                let c = if (cx + cy) % 2 == 0 {
                                    Color::from_rgba(60, 60, 65, 255)
                                } else {
                                    Color::from_rgba(40, 40, 45, 255)
                                };
                                draw_rectangle(
                                    cell_x + cx as f32 * check,
                                    cell_y + cy as f32 * check,
                                    check.min(cell_size - cx as f32 * check),
                                    check.min(cell_size - cy as f32 * check),
                                    c,
                                );
                            }
                        }
                    } else {
                        draw_rectangle(cell_x, cell_y, cell_size, cell_size, Color::from_rgba(r8, g8, b8, 255));
                    }

                    // Selection highlight
                    let is_selected = state.selected_clut_entry == idx;
                    let hovered = ctx.mouse.inside(&cell_rect);

                    if is_selected {
                        draw_rectangle_lines(cell_x, cell_y, cell_size, cell_size, 2.0, WHITE);
                    } else if hovered {
                        draw_rectangle_lines(cell_x, cell_y, cell_size, cell_size, 1.0, Color::from_rgba(255, 255, 255, 100));
                    }

                    // Handle click to select
                    if hovered && ctx.mouse.left_pressed {
                        state.selected_clut_entry = idx;
                        state.active_palette_index = idx as u8;
                    }
                }
            }

            cur_y += grid_size as f32 * cell_size + 4.0;

            // ========================================================================
            // Section 3: Color15 Editor (R/G/B 5-bit sliders)
            // ========================================================================
            if state.selected_clut_entry < clut.colors.len() {
                let color = clut.colors[state.selected_clut_entry];

                // Index label
                draw_text(
                    &format!("Index: {}", state.selected_clut_entry),
                    x + padding,
                    cur_y + 10.0,
                    10.0,
                    TEXT_DIM,
                );

                // Semi-transparent toggle
                let semi_x = x + padding + 60.0;
                let semi_rect = Rect::new(semi_x, cur_y, 14.0, 14.0);
                let is_semi = color.is_semi_transparent();
                let semi_bg = if is_semi { ACCENT_COLOR } else { Color::from_rgba(50, 50, 55, 255) };
                draw_rectangle(semi_rect.x, semi_rect.y, semi_rect.w, semi_rect.h, semi_bg);
                if is_semi {
                    draw_text("✓", semi_x + 2.0, cur_y + 11.0, 12.0, WHITE);
                }
                draw_text("Semi-trans", semi_x + 18.0, cur_y + 10.0, 12.0, TEXT_COLOR);

                if ctx.mouse.inside(&semi_rect) && ctx.mouse.left_pressed {
                    // Toggle semi-transparent bit
                    if let Some(clut_mut) = state.project.clut_pool.get_mut(clut_id) {
                        let c = &mut clut_mut.colors[state.selected_clut_entry];
                        *c = Color15::new_semi(c.r5(), c.g5(), c.b5(), !c.is_semi_transparent());
                        state.dirty = true;
                    }
                }

                cur_y += 16.0;

                // RGB sliders (5-bit, 0-31)
                let slider_w = width - padding * 2.0 - 40.0;
                let slider_h = 10.0;

                let channels = [
                    ("R", color.r5(), Color::from_rgba(180, 80, 80, 255), 0),
                    ("G", color.g5(), Color::from_rgba(80, 180, 80, 255), 1),
                    ("B", color.b5(), Color::from_rgba(80, 80, 180, 255), 2),
                ];

                for (label, value, tint, slider_idx) in channels {
                    let slider_x = x + padding + 14.0;
                    let track_rect = Rect::new(slider_x, cur_y, slider_w, slider_h);

                    // Label
                    draw_text(label, x + padding, cur_y + 8.0, 12.0, tint);

                    // Track background
                    draw_rectangle(track_rect.x, track_rect.y, track_rect.w, track_rect.h, Color::from_rgba(30, 30, 35, 255));

                    // Fill
                    let fill_ratio = value as f32 / 31.0;
                    draw_rectangle(track_rect.x, track_rect.y, track_rect.w * fill_ratio, slider_h, tint);

                    // Handle
                    let handle_x = track_rect.x + track_rect.w * fill_ratio - 2.0;
                    draw_rectangle(handle_x.max(track_rect.x), track_rect.y, 4.0, slider_h, WHITE);

                    // Value
                    draw_text(&format!("{}", value), track_rect.x + track_rect.w + 4.0, cur_y + 8.0, 12.0, TEXT_DIM);

                    // Handle slider interaction
                    let hovered = ctx.mouse.inside(&track_rect);
                    if hovered && ctx.mouse.left_down && state.clut_color_slider.is_none() {
                        state.clut_color_slider = Some(slider_idx);
                    }

                    if state.clut_color_slider == Some(slider_idx) {
                        if ctx.mouse.left_down {
                            let rel_x = (ctx.mouse.x - track_rect.x).clamp(0.0, slider_w);
                            let new_val = ((rel_x / slider_w) * 31.0).round() as u8;

                            if let Some(clut_mut) = state.project.clut_pool.get_mut(clut_id) {
                                let c = clut_mut.colors[state.selected_clut_entry];
                                let semi = c.is_semi_transparent();
                                let (r, g, b) = match slider_idx {
                                    0 => (new_val, c.g5(), c.b5()),
                                    1 => (c.r5(), new_val, c.b5()),
                                    _ => (c.r5(), c.g5(), new_val),
                                };
                                clut_mut.colors[state.selected_clut_entry] = Color15::new_semi(r, g, b, semi);
                                state.dirty = true;
                            }
                        } else {
                            state.clut_color_slider = None;
                        }
                    }

                    cur_y += slider_h + 4.0;
                }
            }
        }
    } else {
        // No CLUT selected - show hint
        draw_text("Select or create a CLUT", x + padding, cur_y + 10.0, 12.0, TEXT_DIM);
    }
}

/// DEPRECATED: Draw the UV Editor panel with the texture atlas and face UVs
/// Kept for reference, replaced by draw_texture_uv_panel
#[allow(dead_code)]
fn draw_uv_editor(_ctx: &mut UiContext, rect: Rect, state: &ModelerState) {
    let atlas = &state.project.atlas;
    let atlas_w = atlas.width as f32;
    let atlas_h = atlas.height as f32;
    let clut = state.project.effective_clut();

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

    // Draw the actual texture atlas pixels (indexed - use CLUT)
    let block_size = (scale * 4.0).max(1.0);
    let pixels_per_block = (block_size / scale).max(1.0) as usize;

    if let Some(clut) = clut {
        for by in (0..atlas.height).step_by(pixels_per_block) {
            for bx in (0..atlas.width).step_by(pixels_per_block) {
                let index = atlas.get_index(bx, by);
                // Index 0 = transparent
                if index == 0 {
                    continue;
                }
                let c15 = clut.lookup(index);
                let px = atlas_x + bx as f32 * scale;
                let py = atlas_y + by as f32 * scale;
                let pw = (pixels_per_block as f32 * scale).min(atlas_x + atlas_screen_w - px);
                let ph = (pixels_per_block as f32 * scale).min(atlas_y + atlas_screen_h - py);
                if pw > 0.0 && ph > 0.0 {
                    let r = (c15.r5() << 3) | (c15.r5() >> 2);
                    let g = (c15.g5() << 3) | (c15.g5() >> 2);
                    let b = (c15.b5() << 3) | (c15.b5() >> 2);
                    draw_rectangle(px, py, pw, ph, Color::from_rgba(r, g, b, 255));
                }
            }
        }
    }

    // Draw atlas border
    draw_rectangle_lines(atlas_x, atlas_y, atlas_screen_w, atlas_screen_h, 1.0, Color::from_rgba(100, 100, 105, 255));

    // Draw UVs of the selected object's faces
    if let Some(obj) = state.project.selected() {
        let face_color = Color::from_rgba(100, 200, 255, 180);
        let selected_color = Color::from_rgba(255, 200, 100, 255);

        // Convert UVs to screen coordinates (snapped to pixel centers)
        let uv_to_screen = |uv: crate::rasterizer::Vec2| {
            let px = (uv.x * atlas_w).floor();
            let py = ((1.0 - uv.y) * atlas_h).floor();
            let sx = atlas_x + (px + 0.5) * scale;
            let sy = atlas_y + (py + 0.5) * scale;
            (sx, sy)
        };

        for (fi, face) in obj.mesh.faces.iter().enumerate() {
            let is_selected = matches!(&state.selection, super::state::ModelerSelection::Faces(faces) if faces.contains(&fi));

            // Collect screen UVs for all vertices of n-gon
            let screen_uvs: Vec<_> = face.vertices.iter()
                .filter_map(|&vi| obj.mesh.vertices.get(vi))
                .map(|v| uv_to_screen(v.uv))
                .collect();

            // Draw edges of the UV face (n-gon)
            let color = if is_selected { selected_color } else { face_color };
            let n = screen_uvs.len();
            for i in 0..n {
                let j = (i + 1) % n;
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

    // Draw the actual 3D content - unified for both perspective and ortho views
    draw_modeler_viewport_ext(ctx, content, state, fb, viewport_id);

    // Label in top-left corner
    let label = viewport_id.label();
    let label_bg = Rect::new(rect.x + 2.0, rect.y + 2.0, label.len() as f32 * 7.0 + 8.0, 16.0);
    draw_rectangle(label_bg.x, label_bg.y, label_bg.w, label_bg.h, Color::from_rgba(0, 0, 0, 180));
    draw_text(label, label_bg.x + 4.0, label_bg.y + 12.0, 12.0, TEXT_COLOR);
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

    // Draw grid - use SECTOR_SIZE (1024 units = 1 meter) for consistency with 3D view
    let grid_size = crate::world::SECTOR_SIZE;
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

    let mouse_pos = (ctx.mouse.x, ctx.mouse.y);
    let inside_viewport = ctx.mouse.inside(&rect);

    // =========================================================================
    // Hover detection for ortho views (same priority as world editor: vertex > edge > face)
    // =========================================================================
    // Use a scope to end the mesh borrow before we mutate state.hovered_*
    let (ortho_hovered_vertex, ortho_hovered_edge, ortho_hovered_face) = {
        let mesh = state.mesh();
        let mut hovered_vertex: Option<usize> = None;
        let mut hovered_edge: Option<(usize, usize)> = None;
        let mut hovered_face: Option<usize> = None;

        if inside_viewport && state.active_viewport == viewport_id && !state.drag_manager.is_dragging() && state.modal_transform == ModalTransform::None {
            const VERTEX_THRESHOLD: f32 = 6.0;
            const EDGE_THRESHOLD: f32 = 4.0;

            // Check vertices
            let mut best_vert_dist = VERTEX_THRESHOLD;
            for (idx, vert) in mesh.vertices.iter().enumerate() {
                let (sx, sy) = project_vertex(vert);
                if sx >= rect.x && sx <= rect.right() && sy >= rect.y && sy <= rect.bottom() {
                    let dist = ((mouse_pos.0 - sx).powi(2) + (mouse_pos.1 - sy).powi(2)).sqrt();
                    if dist < best_vert_dist {
                        best_vert_dist = dist;
                        hovered_vertex = Some(idx);
                    }
                }
            }

            // Check edges if no vertex hovered (iterate over n-gon edges)
            if hovered_vertex.is_none() {
                let mut best_edge_dist = EDGE_THRESHOLD;
                for face in &mesh.faces {
                    for (v0_idx, v1_idx) in face.edges() {
                        if let (Some(v0), Some(v1)) = (mesh.vertices.get(v0_idx), mesh.vertices.get(v1_idx)) {
                            let (sx0, sy0) = project_vertex(v0);
                            let (sx1, sy1) = project_vertex(v1);
                            let dist = point_to_line_dist(mouse_pos.0, mouse_pos.1, sx0, sy0, sx1, sy1);
                            if dist < best_edge_dist {
                                best_edge_dist = dist;
                                hovered_edge = Some(if v0_idx < v1_idx { (v0_idx, v1_idx) } else { (v1_idx, v0_idx) });
                            }
                        }
                    }
                }
            }

            // Check faces if no vertex or edge hovered - triangulate n-gons for hit testing
            if hovered_vertex.is_none() && hovered_edge.is_none() {
                'face_loop: for (idx, face) in mesh.faces.iter().enumerate() {
                    // Triangulate the face and check each triangle
                    for [i0, i1, i2] in face.triangulate() {
                        if let (Some(v0), Some(v1), Some(v2)) = (
                            mesh.vertices.get(i0),
                            mesh.vertices.get(i1),
                            mesh.vertices.get(i2),
                        ) {
                            let (sx0, sy0) = project_vertex(v0);
                            let (sx1, sy1) = project_vertex(v1);
                            let (sx2, sy2) = project_vertex(v2);

                            // Check if mouse is inside the triangle
                            if point_in_triangle_2d(mouse_pos.0, mouse_pos.1, sx0, sy0, sx1, sy1, sx2, sy2) {
                                // In ortho view, just pick the first matching face
                                // (no depth ordering needed as we see orthographically)
                                hovered_face = Some(idx);
                                break 'face_loop;
                            }
                        }
                    }
                }
            }
        }
        (hovered_vertex, hovered_edge, hovered_face)
    };

    // Update global hover state if this is the active viewport (borrow has ended)
    if inside_viewport && state.active_viewport == viewport_id && !state.drag_manager.is_dragging() && state.modal_transform == ModalTransform::None {
        state.hovered_vertex = ortho_hovered_vertex;
        state.hovered_edge = ortho_hovered_edge;
        state.hovered_face = ortho_hovered_face;
    }

    // Get a fresh mesh reference for the rendering section
    let mesh = state.mesh();

    // =========================================================================
    // Draw mesh in ortho view using rasterizer with proper ortho camera
    // =========================================================================
    let hover_color = Color::from_rgba(255, 200, 150, 255);   // Orange for hover
    let select_color = Color::from_rgba(100, 180, 255, 255);  // Blue for selection
    let wire_color = Color::from_rgba(150, 150, 160, 255);
    let vertex_color = Color::from_rgba(180, 180, 190, 255);
    let wireframe_mode = state.raster_settings.wireframe_overlay;

    // Check if any visible object has vertices
    let has_visible_geometry = state.project.objects.iter().any(|obj| obj.visible && !obj.mesh.vertices.is_empty())
        || (!mesh.vertices.is_empty() && state.project.selected_object.map_or(true, |i| state.project.objects.get(i).map_or(true, |o| o.visible)));

    if has_visible_geometry && !wireframe_mode {
        // Create ortho camera for this view direction
        let ortho_camera = match viewport_id {
            ViewportId::Top => Camera::ortho_top(),
            ViewportId::Front => Camera::ortho_front(),
            ViewportId::Side => Camera::ortho_side(),
            ViewportId::Perspective => unreachable!(),
        };

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

        // Convert atlas to texture (shared by all objects)
        // Get effective CLUT for rendering
        let clut = state.project.effective_clut();
        let default_clut = Clut::new_4bit("default");
        let clut_ref = clut.unwrap_or(&default_clut);

        let use_rgb555 = state.raster_settings.use_rgb555;
        let atlas_texture = state.project.atlas.to_raster_texture(clut_ref, "atlas");
        let atlas_texture_15 = if use_rgb555 {
            Some(state.project.atlas.to_texture15(clut_ref, "atlas"))
        } else {
            None
        };

        // Render all visible objects
        for (obj_idx, obj) in state.project.objects.iter().enumerate() {
            // Skip hidden objects
            if !obj.visible {
                continue;
            }

            // Use project mesh directly (mesh() accessor returns selected object's mesh)
            let obj_mesh = &obj.mesh;

            // Dim non-selected objects slightly
            let base_color = if state.project.selected_object == Some(obj_idx) {
                180u8
            } else {
                140u8
            };

            let vertices: Vec<RasterVertex> = obj_mesh.vertices.iter().map(|v| {
                RasterVertex {
                    pos: v.pos,
                    normal: v.normal,
                    uv: v.uv,
                    color: RasterColor::new(base_color, base_color, base_color),
                    bone_index: None,
                }
            }).collect();

            // Triangulate n-gon faces for rendering
            let mut faces: Vec<RasterFace> = Vec::new();
            for edit_face in &obj_mesh.faces {
                for [v0, v1, v2] in edit_face.triangulate() {
                    faces.push(RasterFace {
                        v0,
                        v1,
                        v2,
                        texture_id: Some(0),
                        black_transparent: edit_face.black_transparent,
                        blend_mode: edit_face.blend_mode,
                    });
                }
            }

            if !vertices.is_empty() && !faces.is_empty() {
                // Collect per-face blend modes
                let blend_modes: Vec<crate::rasterizer::BlendMode> = faces.iter()
                    .map(|f| f.blend_mode)
                    .collect();

                if use_rgb555 {
                    // RGB555 rendering path
                    let textures_15 = [atlas_texture_15.as_ref().unwrap().clone()];
                    render_mesh_15(
                        fb,
                        &vertices,
                        &faces,
                        &textures_15,
                        Some(&blend_modes),
                        &ortho_camera,
                        &ortho_settings,
                        None,
                    );
                } else {
                    // RGB888 rendering path (original)
                    let textures = [atlas_texture.clone()];
                    render_mesh(fb, &vertices, &faces, &textures, &ortho_camera, &ortho_settings);
                }
            }
        }

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

    // Enable scissor clipping for selection overlays, wireframes, and gizmos
    // This prevents them from rendering outside the ortho viewport bounds
    let dpi = screen_dpi_scale();
    gl_use_default_material();
    unsafe {
        get_internal_gl().quad_gl.scissor(
            Some((
                (rect.x * dpi) as i32,
                (rect.y * dpi) as i32,
                (rect.w * dpi) as i32,
                (rect.h * dpi) as i32,
            ))
        );
    }

    if !mesh.vertices.is_empty() {
        // Draw wireframe edges (n-gon edges)
        // In wireframe mode: draw all edges
        // In solid mode: only draw hovered/selected faces
        for (idx, face) in mesh.faces.iter().enumerate() {
            let is_hovered = state.hovered_face == Some(idx);
            let is_selected = matches!(&state.selection, super::state::ModelerSelection::Faces(f) if f.contains(&idx));

            // In solid mode, skip unselected/unhovered faces
            if !wireframe_mode && !is_hovered && !is_selected {
                continue;
            }

            // Choose color based on hover/selection
            let color = if is_hovered { hover_color } else if is_selected { select_color } else { wire_color };
            let thickness = if is_hovered || is_selected { 2.0 } else { 1.0 };

            // Draw all edges of n-gon
            for (v0_idx, v1_idx) in face.edges() {
                if let (Some(v0), Some(v1)) = (mesh.vertices.get(v0_idx), mesh.vertices.get(v1_idx)) {
                    let (x0, y0) = project_vertex(v0);
                    let (x1, y1) = project_vertex(v1);
                    draw_line(x0, y0, x1, y1, thickness, color);
                }
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
    // Draw transform gizmo in ortho views (2-axis version)
    // =========================================================================
    if !state.selection.is_empty() && state.tool_box.active_transform_tool().is_some() {
        if let Some(center) = state.selection.compute_center(state.mesh()) {
            // Project center to screen using world_to_ortho directly
            let (cx, cy) = match viewport_id {
                ViewportId::Top => world_to_ortho(center.x, center.z),
                ViewportId::Front => world_to_ortho(center.x, center.y),
                ViewportId::Side => world_to_ortho(center.z, center.y),
                ViewportId::Perspective => (0.0, 0.0),
            };

            // Only draw if center is in viewport
            if cx >= rect.x && cx <= rect.right() && cy >= rect.y && cy <= rect.bottom() {
                let gizmo_length = 40.0; // Fixed screen-space length

                // Determine which world axes map to screen X and Y for this ortho view
                let (x_color, y_color) = match viewport_id {
                    ViewportId::Top => (RED, BLUE),    // X right, Z up
                    ViewportId::Front => (RED, GREEN), // X right, Y up
                    ViewportId::Side => (BLUE, GREEN), // Z right, Y up
                    ViewportId::Perspective => (WHITE, WHITE),
                };

                // Screen directions: right is +X, up is -Y (screen coords)
                let x_end = (cx + gizmo_length, cy);
                let y_end = (cx, cy - gizmo_length);

                // Check which axis is hovered
                let dist_to_x = point_to_line_dist(ctx.mouse.x, ctx.mouse.y, cx, cy, x_end.0, x_end.1);
                let dist_to_y = point_to_line_dist(ctx.mouse.x, ctx.mouse.y, cx, cy, y_end.0, y_end.1);

                let hover_threshold = 8.0;
                let x_hovered = inside_viewport && dist_to_x < hover_threshold && dist_to_x < dist_to_y;
                let y_hovered = inside_viewport && dist_to_y < hover_threshold && dist_to_y < dist_to_x;

                // Update gizmo hover state for ortho views
                if x_hovered {
                    state.ortho_gizmo_hovered_axis = Some(match viewport_id {
                        ViewportId::Top => super::state::Axis::X,
                        ViewportId::Front => super::state::Axis::X,
                        ViewportId::Side => super::state::Axis::Z,
                        _ => super::state::Axis::X,
                    });
                } else if y_hovered {
                    state.ortho_gizmo_hovered_axis = Some(match viewport_id {
                        ViewportId::Top => super::state::Axis::Z,
                        ViewportId::Front => super::state::Axis::Y,
                        ViewportId::Side => super::state::Axis::Y,
                        _ => super::state::Axis::Y,
                    });
                } else if inside_viewport {
                    state.ortho_gizmo_hovered_axis = None;
                }

                // Draw colors (brighten on hover)
                let x_draw_color = if x_hovered { YELLOW } else { Color::new(x_color.r * 0.8, x_color.g * 0.8, x_color.b * 0.8, 1.0) };
                let y_draw_color = if y_hovered { YELLOW } else { Color::new(y_color.r * 0.8, y_color.g * 0.8, y_color.b * 0.8, 1.0) };

                // Draw gizmo lines
                let line_thickness = 2.0;
                draw_line(cx, cy, x_end.0, x_end.1, line_thickness, x_draw_color);
                draw_line(cx, cy, y_end.0, y_end.1, line_thickness, y_draw_color);

                // Draw arrowheads for move gizmo
                if matches!(state.tool_box.active_transform_tool(), Some(ModelerToolId::Move)) {
                    let arrow_size = 8.0;
                    // X arrow (pointing right)
                    draw_triangle(
                        Vec2::new(x_end.0, x_end.1),
                        Vec2::new(x_end.0 - arrow_size, x_end.1 - arrow_size * 0.5),
                        Vec2::new(x_end.0 - arrow_size, x_end.1 + arrow_size * 0.5),
                        x_draw_color,
                    );
                    // Y arrow (pointing up)
                    draw_triangle(
                        Vec2::new(y_end.0, y_end.1),
                        Vec2::new(y_end.0 - arrow_size * 0.5, y_end.1 + arrow_size),
                        Vec2::new(y_end.0 + arrow_size * 0.5, y_end.1 + arrow_size),
                        y_draw_color,
                    );
                }

                // Draw small squares for scale gizmo
                if matches!(state.tool_box.active_transform_tool(), Some(ModelerToolId::Scale)) {
                    let box_size = 6.0;
                    draw_rectangle(x_end.0 - box_size/2.0, x_end.1 - box_size/2.0, box_size, box_size, x_draw_color);
                    draw_rectangle(y_end.0 - box_size/2.0, y_end.1 - box_size/2.0, box_size, box_size, y_draw_color);
                }

                // Draw circles for rotate gizmo
                if matches!(state.tool_box.active_transform_tool(), Some(ModelerToolId::Rotate)) {
                    draw_circle_lines(cx, cy, gizmo_length * 0.8, 1.5, Color::new(0.6, 0.6, 0.6, 0.8));
                }

                // Draw center dot
                draw_circle(cx, cy, 4.0, WHITE);
            }
        }
    }

    // Box selection overlay is now drawn by the unified viewport.rs handler

    // Disable scissor clipping
    unsafe {
        get_internal_gl().quad_gl.scissor(None);
    }

    // =========================================================================
    // Handle click to select in ortho views
    // =========================================================================
    // Skip selection if gizmo is hovered - gizmo takes precedence
    if inside_viewport && state.active_viewport == viewport_id && ctx.mouse.left_pressed && state.modal_transform == ModalTransform::None && !state.drag_manager.is_dragging() && state.ortho_gizmo_hovered_axis.is_none() {
        let multi_select = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) || is_key_down(KeyCode::X);

        if let Some(vert_idx) = state.hovered_vertex {
            if multi_select {
                state.save_selection_undo();
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
                state.set_selection(super::state::ModelerSelection::Vertices(vec![vert_idx]));
            }
            state.select_mode = SelectMode::Vertex;
        } else if let Some((v0, v1)) = state.hovered_edge {
            if multi_select {
                state.save_selection_undo();
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
                state.set_selection(super::state::ModelerSelection::Edges(vec![(v0, v1)]));
            }
            state.select_mode = SelectMode::Edge;
        } else if let Some(face_idx) = state.hovered_face {
            if multi_select {
                state.save_selection_undo();
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
                state.set_selection(super::state::ModelerSelection::Faces(vec![face_idx]));
            }
            state.select_mode = SelectMode::Face;
        } else if !is_key_down(KeyCode::X) {
            // Clicked on nothing - start potential box select (don't clear selection yet)
            eprintln!("[DEBUG] Clicked empty space in {:?} - setting box_select_pending_start", viewport_id);
            state.ortho_box_select_pending_start = Some((ctx.mouse.x, ctx.mouse.y));
            state.ortho_box_select_viewport = Some(viewport_id);
        }
    }

    // =========================================================================
    // Handle ortho box selection (click+drag on empty space)
    // =========================================================================
    if state.ortho_box_select_viewport == Some(viewport_id) {
        let ortho_cam = state.get_ortho_camera(viewport_id);
        let ortho_zoom = ortho_cam.zoom;
        let ortho_center = ortho_cam.center;

        // Check if we're already in box select mode
        let is_box_selecting = state.drag_manager.active.is_box_select();

        eprintln!("[DEBUG ORTHO BOX] viewport={:?} is_box_selecting={} pending={:?} mouse=({}, {})",
            viewport_id, is_box_selecting, state.ortho_box_select_pending_start, ctx.mouse.x, ctx.mouse.y);

        if is_box_selecting {
            // Update the box select tracker with current mouse position
            if let super::drag::ActiveDrag::BoxSelect(tracker) = &mut state.drag_manager.active {
                tracker.current_mouse = (ctx.mouse.x, ctx.mouse.y);
            }

            // On mouse release, apply the selection
            if !ctx.mouse.left_down {
                // Get bounds from tracker before ending drag
                let bounds = if let super::drag::ActiveDrag::BoxSelect(tracker) = &state.drag_manager.active {
                    Some(tracker.bounds())
                } else {
                    None
                };

                if let Some((x0, y0, x1, y1)) = bounds {
                    eprintln!("[DEBUG] Applying ortho box selection: bounds=({}, {}) to ({}, {})", x0, y0, x1, y1);
                    // Apply box selection for ortho views
                    apply_ortho_box_selection(state, viewport_id, x0, y0, x1, y1, rect.x, rect.y, rect.w, rect.h, ortho_zoom, ortho_center);
                }

                // End the drag
                state.drag_manager.end();
                state.ortho_box_select_viewport = None;
            }
        } else if let Some(start_pos) = state.ortho_box_select_pending_start {
            // Check if we should convert pending start to actual box select
            if ctx.mouse.left_down {
                let dx = (ctx.mouse.x - start_pos.0).abs();
                let dy = (ctx.mouse.y - start_pos.1).abs();
                // Only become box select if moved at least 5 pixels
                if dx > 5.0 || dy > 5.0 {
                    eprintln!("[DEBUG] Starting ortho box select! dx={} dy={}", dx, dy);
                    state.drag_manager.start_box_select(start_pos);
                    // Update with current position
                    if let super::drag::ActiveDrag::BoxSelect(tracker) = &mut state.drag_manager.active {
                        tracker.current_mouse = (ctx.mouse.x, ctx.mouse.y);
                    }
                    state.ortho_box_select_pending_start = None;
                }
            } else {
                // Mouse released without dragging far enough - clear selection (this was the original click)
                if !is_key_down(KeyCode::X) && !is_key_down(KeyCode::LeftShift) && !is_key_down(KeyCode::RightShift) {
                    state.set_selection(super::state::ModelerSelection::None);
                }
                state.ortho_box_select_pending_start = None;
                state.ortho_box_select_viewport = None;
            }
        }
    }

    // =========================================================================
    // Handle left-drag to move selection in ortho views
    // =========================================================================
    // Don't start move if box select is pending (user clicked on empty space for box select)
    if inside_viewport && state.active_viewport == viewport_id && !state.selection.is_empty() && state.modal_transform == ModalTransform::None && state.ortho_box_select_pending_start.is_none() {
        // Get zoom value before any mutable borrows
        let ortho_zoom = state.get_ortho_camera(viewport_id).zoom;

        // Start drag on left-down (when we have selection and not clicking to select)
        // Use ortho_drag_pending_start to detect drag vs click, similar to box select
        // Only start if gizmo is hovered OR clicking on a selected/hovered element (not empty space)
        let has_hovered = state.hovered_vertex.is_some() || state.hovered_edge.is_some() || state.hovered_face.is_some() || state.ortho_gizmo_hovered_axis.is_some();
        if ctx.mouse.left_pressed && inside_viewport && !state.drag_manager.is_dragging() && has_hovered {
            // Store the gizmo axis that was clicked (if any)
            // Convert from state::Axis to ui::drag_tracker::Axis
            state.ortho_drag_pending_start = Some((ctx.mouse.x, ctx.mouse.y));
            state.ortho_drag_axis = state.ortho_gizmo_hovered_axis.map(|a| match a {
                super::state::Axis::X => crate::ui::drag_tracker::Axis::X,
                super::state::Axis::Y => crate::ui::drag_tracker::Axis::Y,
                super::state::Axis::Z => crate::ui::drag_tracker::Axis::Z,
            });
        }

        // Check if we should convert pending start to actual ortho move
        if let Some(start_pos) = state.ortho_drag_pending_start {
            if ctx.mouse.left_down && !state.drag_manager.is_dragging() {
                let dx = (ctx.mouse.x - start_pos.0).abs();
                let dy = (ctx.mouse.y - start_pos.1).abs();
                // Only become ortho move if moved at least 3 pixels
                if dx > 3.0 || dy > 3.0 {
                    // Collect starting positions
                    let mesh = state.mesh();
                    let mut indices = state.selection.get_affected_vertex_indices(mesh);
                    if state.vertex_linking {
                        indices = mesh.expand_to_coincident(&indices, 0.001);
                    }
                    let initial_positions: Vec<(usize, crate::rasterizer::Vec3)> = indices.iter()
                        .filter_map(|&idx| mesh.vertices.get(idx).map(|v| (idx, v.pos)))
                        .collect();

                    if !initial_positions.is_empty() {
                        // Calculate center
                        let sum: crate::rasterizer::Vec3 = initial_positions.iter()
                            .map(|(_, p)| *p)
                            .fold(crate::rasterizer::Vec3::ZERO, |acc, p| acc + p);
                        let center = sum * (1.0 / initial_positions.len() as f32);

                        // Save undo state before starting
                        state.push_undo("Ortho Move");

                        // Store ortho-specific data for the drag
                        state.ortho_drag_viewport = Some(viewport_id);
                        state.ortho_drag_zoom = ortho_zoom;

                        // Use CURRENT mouse position as reference, not original click position.
                        // This prevents snapping - delta starts at 0 and accumulates from here.
                        let drag_start_mouse = (ctx.mouse.x, ctx.mouse.y);

                        // Start move drag (constrained if gizmo axis was clicked)
                        state.drag_manager.start_move(
                            center,
                            drag_start_mouse,
                            state.ortho_drag_axis, // Use captured axis constraint
                            indices,
                            initial_positions,
                            state.snap_settings.enabled,
                            state.snap_settings.grid_size,
                        );
                    }

                    state.ortho_drag_pending_start = None;
                }
            } else if !ctx.mouse.left_down {
                // Mouse released without dragging - clear pending
                state.ortho_drag_pending_start = None;
                state.ortho_drag_axis = None;
            }
        }

    }

    // Continue ortho drag (if we're the active ortho drag viewport)
    // This is OUTSIDE the inside_viewport check so drag continues even if mouse leaves viewport
    // Use is_move() not is_free_move() since we may have axis constraints from gizmo click
    if state.drag_manager.active.is_move() && state.ortho_drag_viewport == Some(viewport_id) {
        // Use the stored zoom from when drag started
        let drag_zoom = state.ortho_drag_zoom;

        // Screen to world delta helper using stored zoom
        let screen_to_world_delta = |dx: f32, dy: f32| -> crate::rasterizer::Vec3 {
            let world_dx = dx / drag_zoom;
            let world_dy = -dy / drag_zoom; // Y inverted

            match viewport_id {
                ViewportId::Top => crate::rasterizer::Vec3::new(world_dx, 0.0, world_dy),    // XZ plane
                ViewportId::Front => crate::rasterizer::Vec3::new(world_dx, world_dy, 0.0),  // XY plane
                ViewportId::Side => crate::rasterizer::Vec3::new(0.0, world_dy, world_dx),   // ZY plane
                ViewportId::Perspective => crate::rasterizer::Vec3::ZERO,
            }
        };

        if ctx.mouse.left_down {
            // Get mouse delta from drag start
            if let Some(drag_state) = &state.drag_manager.state {
                let dx = ctx.mouse.x - drag_state.initial_mouse.0;
                let dy = ctx.mouse.y - drag_state.initial_mouse.1;

                let mut delta = screen_to_world_delta(dx, dy);

                // Apply delta to initial positions
                if let super::drag::ActiveDrag::Move(tracker) = &state.drag_manager.active {
                    // Apply axis constraint if present
                    if let Some(axis) = &tracker.axis {
                        match axis {
                            crate::ui::drag_tracker::Axis::X => { delta.y = 0.0; delta.z = 0.0; }
                            crate::ui::drag_tracker::Axis::Y => { delta.x = 0.0; delta.z = 0.0; }
                            crate::ui::drag_tracker::Axis::Z => { delta.x = 0.0; delta.y = 0.0; }
                        }
                    }

                    // Collect updates first, then apply (borrow checker)
                    let updates: Vec<_> = tracker.initial_positions.iter()
                        .map(|(idx, start_pos)| (*idx, *start_pos + delta))
                        .collect();
                    // Capture snap settings before borrowing mesh
                    let snap_enabled = state.snap_settings.enabled && !is_key_down(KeyCode::Z);
                    let snap_size = state.snap_settings.grid_size;
                    if let Some(mesh) = state.mesh_mut() {
                        for (idx, new_pos) in updates {
                            if let Some(vert) = mesh.vertices.get_mut(idx) {
                                vert.pos = new_pos;

                                // Apply grid snapping if enabled
                                if snap_enabled {
                                    vert.pos.x = (vert.pos.x / snap_size).round() * snap_size;
                                    vert.pos.y = (vert.pos.y / snap_size).round() * snap_size;
                                    vert.pos.z = (vert.pos.z / snap_size).round() * snap_size;
                                }
                            }
                        }
                    }
                    state.dirty = true;
                }
            }
        } else {
            // End drag
            state.drag_manager.end();
            state.ortho_drag_viewport = None;
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

/// Apply box selection to mesh elements in ortho views
/// Uses same approach as perspective view - project vertices to screen and check bounds
fn apply_ortho_box_selection(
    state: &mut ModelerState,
    viewport_id: ViewportId,
    screen_x0: f32,
    screen_y0: f32,
    screen_x1: f32,
    screen_y1: f32,
    rect_x: f32,
    rect_y: f32,
    rect_w: f32,
    rect_h: f32,
    ortho_zoom: f32,
    ortho_center: crate::rasterizer::Vec2,
) {
    eprintln!("[DEBUG apply_ortho_box_selection] viewport={:?} screen=({}, {})-({}, {}) rect=({}, {}, {}, {}) zoom={} center={:?}",
        viewport_id, screen_x0, screen_y0, screen_x1, screen_y1, rect_x, rect_y, rect_w, rect_h, ortho_zoom, ortho_center);
    // Check if adding to selection (Shift or X held)
    let add_to_selection = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift)
        || is_key_down(KeyCode::X);

    // Get min/max screen bounds
    let (min_sx, max_sx) = if screen_x0 < screen_x1 { (screen_x0, screen_x1) } else { (screen_x1, screen_x0) };
    let (min_sy, max_sy) = if screen_y0 < screen_y1 { (screen_y0, screen_y1) } else { (screen_y1, screen_y0) };

    // Project 3D position to screen coordinates (same math as draw_ortho_viewport)
    let world_to_screen = |pos: crate::rasterizer::Vec3| -> (f32, f32) {
        // Extract the 2D coordinates based on view
        let (wx, wy) = match viewport_id {
            ViewportId::Top => (pos.x, pos.z),    // Top: X,Z plane
            ViewportId::Front => (pos.x, pos.y),  // Front: X,Y plane
            ViewportId::Side => (pos.z, pos.y),   // Side: Z,Y plane
            ViewportId::Perspective => (0.0, 0.0),
        };

        // Convert to screen coordinates
        let screen_center_x = rect_x + rect_w / 2.0;
        let screen_center_y = rect_y + rect_h / 2.0;
        let sx = screen_center_x + (wx - ortho_center.x) * ortho_zoom;
        let sy = screen_center_y - (wy - ortho_center.y) * ortho_zoom; // Y flipped
        (sx, sy)
    };

    // Check if screen position is within selection box
    let is_in_box = |pos: crate::rasterizer::Vec3| -> bool {
        let (sx, sy) = world_to_screen(pos);
        sx >= min_sx && sx <= max_sx && sy >= min_sy && sy <= max_sy
    };

    let mesh = state.mesh();

    eprintln!("[DEBUG] select_mode={:?} mesh has {} vertices", state.select_mode, mesh.vertices.len());

    match state.select_mode {
        SelectMode::Vertex => {
            let mut selected = if add_to_selection {
                if let super::state::ModelerSelection::Vertices(v) = &state.selection { v.clone() } else { Vec::new() }
            } else {
                Vec::new()
            };

            for (idx, vert) in mesh.vertices.iter().enumerate() {
                let (sx, sy) = world_to_screen(vert.pos);
                let in_box = is_in_box(vert.pos);
                if idx < 3 {
                    eprintln!("[DEBUG] vertex {} pos={:?} -> screen=({}, {}) in_box={}", idx, vert.pos, sx, sy, in_box);
                }
                if in_box && !selected.contains(&idx) {
                    selected.push(idx);
                }
            }

            if !selected.is_empty() {
                let count = selected.len();
                state.set_selection(super::state::ModelerSelection::Vertices(selected));
                state.set_status(&format!("Selected {} vertex(es)", count), 0.5);
            } else if !add_to_selection {
                state.set_selection(super::state::ModelerSelection::None);
            }
        }
        SelectMode::Edge => {
            let mut selected = if add_to_selection {
                if let super::state::ModelerSelection::Edges(e) = &state.selection { e.clone() } else { Vec::new() }
            } else {
                Vec::new()
            };

            // Get all unique edges from mesh
            let mut edges_checked: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
            for face in &mesh.faces {
                for i in 0..face.vertices.len() {
                    let v0 = face.vertices[i];
                    let v1 = face.vertices[(i + 1) % face.vertices.len()];
                    let edge = if v0 < v1 { (v0, v1) } else { (v1, v0) };
                    if edges_checked.insert(edge) {
                        // Check if edge center is in box
                        if let (Some(p0), Some(p1)) = (mesh.vertices.get(v0), mesh.vertices.get(v1)) {
                            let center = (p0.pos + p1.pos) * 0.5;
                            if is_in_box(center) {
                                let e = (v0, v1);
                                if !selected.iter().any(|&(a, b)| (a, b) == e || (b, a) == e) {
                                    selected.push(e);
                                }
                            }
                        }
                    }
                }
            }

            if !selected.is_empty() {
                let count = selected.len();
                state.set_selection(super::state::ModelerSelection::Edges(selected));
                state.set_status(&format!("Selected {} edge(s)", count), 0.5);
            } else if !add_to_selection {
                state.set_selection(super::state::ModelerSelection::None);
            }
        }
        SelectMode::Face => {
            let mut selected = if add_to_selection {
                if let super::state::ModelerSelection::Faces(f) = &state.selection { f.clone() } else { Vec::new() }
            } else {
                Vec::new()
            };

            for (idx, face) in mesh.faces.iter().enumerate() {
                // Calculate face center
                let verts: Vec<_> = face.vertices.iter()
                    .filter_map(|&vi| mesh.vertices.get(vi))
                    .collect();
                if !verts.is_empty() {
                    let center = verts.iter().map(|v| v.pos).fold(crate::rasterizer::Vec3::ZERO, |acc, p| acc + p)
                        * (1.0 / verts.len() as f32);
                    if is_in_box(center) && !selected.contains(&idx) {
                        selected.push(idx);
                    }
                }
            }

            if !selected.is_empty() {
                let count = selected.len();
                state.set_selection(super::state::ModelerSelection::Faces(selected));
                state.set_status(&format!("Selected {} face(s)", count), 0.5);
            } else if !add_to_selection {
                state.set_selection(super::state::ModelerSelection::None);
            }
        }
    }
}

fn draw_viewport(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, fb: &mut Framebuffer) {
    draw_modeler_viewport(ctx, rect, state, fb);
}

/// DEPRECATED: Draw the Atlas panel with the actual texture and painting support
/// Kept for reference, replaced by draw_texture_uv_panel
#[allow(dead_code)]
fn draw_atlas_panel(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState) {
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
    if let Some(clut) = state.project.effective_clut() {
        for by in (0..atlas_height).step_by(pixels_per_block.max(1)) {
            for bx in (0..atlas_width).step_by(pixels_per_block.max(1)) {
                let pixel = state.project.atlas.get_color(bx, by, clut);
                let px = atlas_x + bx as f32 * scale;
                let py = atlas_y + by as f32 * scale;
                let pw = (pixels_per_block as f32 * scale).min(atlas_x + atlas_screen_w - px).max(scale);
                let ph = (pixels_per_block as f32 * scale).min(atlas_y + atlas_screen_h - py).max(scale);
                if pw > 0.0 && ph > 0.0 {
                    let r = (pixel.r5() << 3) | (pixel.r5() >> 2);
                    let g = (pixel.g5() << 3) | (pixel.g5() >> 2);
                    let b = (pixel.b5() << 3) | (pixel.b5() >> 2);
                    draw_rectangle(px, py, pw, ph, Color::from_rgba(r, g, b, 255));
                }
            }
        }
    }

    // Draw atlas border
    draw_rectangle_lines(atlas_x, atlas_y, atlas_screen_w, atlas_screen_h, 1.0, Color::from_rgba(100, 100, 105, 255));

    // Handle painting when paint section is expanded
    let (mx, my) = (ctx.mouse.x, ctx.mouse.y);
    let atlas_rect = Rect::new(atlas_x, atlas_y, atlas_screen_w, atlas_screen_h);

    if state.paint_section_expanded && atlas_rect.contains(mx, my) {
        // Convert mouse position to atlas pixel coordinates
        let px = ((mx - atlas_x) / scale) as usize;
        let py = ((my - atlas_y) / scale) as usize;

        // Draw cursor preview
        let brush_size = state.brush_size;
        let cursor_x = atlas_x + (px as f32) * scale;
        let cursor_y = atlas_y + (py as f32) * scale;
        let cursor_w = brush_size * scale;
        draw_rectangle_lines(cursor_x, cursor_y, cursor_w, cursor_w, 1.0, Color::from_rgba(255, 255, 255, 200));

        // Paint on click/drag (indexed painting with palette index)
        if ctx.mouse.left_down {
            // Save undo at stroke start
            if !state.paint_stroke_active {
                state.push_undo_with_atlas("Paint");
                state.paint_stroke_active = true;
            }
            let index = state.active_palette_index;
            let brush = brush_size as usize;
            for dy in 0..brush {
                for dx in 0..brush {
                    state.project.atlas.set_index(px + dx, py + dy, index);
                }
            }
            state.dirty = true;
        } else {
            state.paint_stroke_active = false;
        }
    } else {
        // Reset stroke when not painting on atlas
        state.paint_stroke_active = false;
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

        // Highlight current palette index
        if i as u8 == state.active_palette_index {
            draw_rectangle_lines(sx - 1.0, sy - 1.0, swatch_size, swatch_size, 2.0, WHITE);
        }

        // Handle click to select palette index
        let swatch_rect = Rect::new(sx, sy, swatch_size - 2.0, swatch_size - 2.0);
        if ctx.mouse.left_pressed && swatch_rect.contains(mx, my) {
            state.active_palette_index = i as u8;
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

    // Tool info (using new tool system)
    draw_text("Tool:", rect.x, y + 14.0, 12.0, TEXT_DIM);
    y += line_height;
    let tool_label = match state.tool_box.active_transform_tool() {
        Some(ModelerToolId::Move) => "Move (G)",
        Some(ModelerToolId::Rotate) => "Rotate (R)",
        Some(ModelerToolId::Scale) => "Scale (T)",
        _ => "Select",
    };
    draw_text(tool_label, rect.x, y + 14.0, 12.0, TEXT_COLOR);

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
        draw_text(&format!("{}: {}", key, desc), rect.x, y + 12.0, 12.0, TEXT_DIM);
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
            draw_text(&format!("{} {}", light_type_str, i + 1), toggle_rect.x + 4.0, toggle_rect.y + 12.0, 12.0, TEXT_COLOR);

            if ctx.mouse.inside(&toggle_rect) && ctx.mouse.left_pressed {
                toggle_light = Some(i);
            }

            // Intensity indicator
            let intensity_str = format!("{:.0}%", light.intensity * 100.0);
            draw_text(&intensity_str, rect.x + 55.0, y + 12.0, 12.0, TEXT_DIM);

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

// ============================================================================
// UV Transform Functions
// ============================================================================

/// Get UV vertices from selected faces
fn get_uv_vertices_from_selection(state: &ModelerState) -> Vec<usize> {
    let mut verts = std::collections::HashSet::new();
    if let Some(obj) = state.project.selected() {
        if let super::state::ModelerSelection::Faces(faces) = &state.selection {
            for &fi in faces {
                if let Some(face) = obj.mesh.faces.get(fi) {
                    // Add all vertices of n-gon face
                    for &vi in &face.vertices {
                        verts.insert(vi);
                    }
                }
            }
        }
    }
    verts.into_iter().collect()
}

/// Compute center of UV coordinates for given vertices
fn compute_uv_center(state: &ModelerState, verts: &[usize]) -> Option<crate::rasterizer::Vec2> {
    if verts.is_empty() {
        return None;
    }
    let obj = state.project.selected()?;
    let mut sum_u = 0.0f32;
    let mut sum_v = 0.0f32;
    let mut count = 0;
    for &vi in verts {
        if let Some(v) = obj.mesh.vertices.get(vi) {
            sum_u += v.uv.x;
            sum_v += v.uv.y;
            count += 1;
        }
    }
    if count == 0 {
        return None;
    }
    Some(crate::rasterizer::Vec2::new(sum_u / count as f32, sum_v / count as f32))
}

/// Snap UV to pixel boundary
fn snap_uv(u: f32, v: f32, atlas_size: f32) -> (f32, f32) {
    let px = (u * atlas_size).round() / atlas_size;
    let py = (v * atlas_size).round() / atlas_size;
    (px.clamp(0.0, 1.0), py.clamp(0.0, 1.0))
}

/// Flip selected UVs horizontally and/or vertically around their center
fn flip_selected_uvs(state: &mut ModelerState, flip_h: bool, flip_v: bool) {
    let verts = get_uv_vertices_from_selection(state);
    if verts.is_empty() {
        state.set_status("No faces selected", 1.0);
        return;
    }

    let center = match compute_uv_center(state, &verts) {
        Some(c) => c,
        None => return,
    };

    let atlas_size = state.project.atlas.width as f32;

    state.push_undo(if flip_h { "Flip UV Horizontal" } else { "Flip UV Vertical" });

    if let Some(obj) = state.project.selected_mut() {
        for &vi in &verts {
            if let Some(v) = obj.mesh.vertices.get_mut(vi) {
                if flip_h {
                    v.uv.x = center.x - (v.uv.x - center.x);
                }
                if flip_v {
                    v.uv.y = center.y - (v.uv.y - center.y);
                }
                // Snap to pixel boundary
                let (su, sv) = snap_uv(v.uv.x, v.uv.y, atlas_size);
                v.uv.x = su;
                v.uv.y = sv;
            }
        }
    }

    // Project is single source of truth
    state.dirty = true;
    state.set_status(if flip_h { "Flipped UV horizontal" } else { "Flipped UV vertical" }, 1.0);
}

/// Rotate selected UVs 90 degrees around their center
fn rotate_selected_uvs(state: &mut ModelerState, clockwise: bool) {
    let verts = get_uv_vertices_from_selection(state);
    if verts.is_empty() {
        state.set_status("No faces selected", 1.0);
        return;
    }

    let center = match compute_uv_center(state, &verts) {
        Some(c) => c,
        None => return,
    };

    let atlas_size = state.project.atlas.width as f32;

    state.push_undo("Rotate UV 90°");

    if let Some(obj) = state.project.selected_mut() {
        for &vi in &verts {
            if let Some(v) = obj.mesh.vertices.get_mut(vi) {
                // Translate to origin
                let du = v.uv.x - center.x;
                let dv = v.uv.y - center.y;
                // Rotate 90 degrees
                let (new_du, new_dv) = if clockwise {
                    (dv, -du)  // CW: (x,y) -> (y,-x)
                } else {
                    (-dv, du)  // CCW: (x,y) -> (-y,x)
                };
                // Translate back
                v.uv.x = center.x + new_du;
                v.uv.y = center.y + new_dv;
                // Snap to pixel boundary
                let (su, sv) = snap_uv(v.uv.x, v.uv.y, atlas_size);
                v.uv.x = su;
                v.uv.y = sv;
            }
        }
    }

    // Project is single source of truth
    state.dirty = true;
    state.set_status(if clockwise { "Rotated UV 90° CW" } else { "Rotated UV 90° CCW" }, 1.0);
}

/// Reset UVs to planar projection from face normals
fn reset_selected_uvs(state: &mut ModelerState) {
    if let super::state::ModelerSelection::Faces(faces) = &state.selection.clone() {
        if faces.is_empty() {
            state.set_status("No faces selected", 1.0);
            return;
        }

        let atlas_size = state.project.atlas.width as f32;

        state.push_undo("Reset UVs");

        if let Some(obj) = state.project.selected_mut() {
            for &fi in faces {
                if let Some(face) = obj.mesh.faces.get(fi).cloned() {
                    if face.vertices.len() < 3 {
                        continue;
                    }

                    // Get first 3 vertex positions for normal calculation
                    let p0 = obj.mesh.vertices[face.vertices[0]].pos;
                    let p1 = obj.mesh.vertices[face.vertices[1]].pos;
                    let p2 = obj.mesh.vertices[face.vertices[2]].pos;

                    // Compute face normal from first 3 vertices
                    let edge1 = p1 - p0;
                    let edge2 = p2 - p0;
                    let normal = edge1.cross(edge2).normalize();

                    // Choose projection axes based on dominant normal component
                    // This gives axis-aligned projections similar to TrenchBroom's paraxial
                    let abs_normal = crate::rasterizer::Vec3::new(normal.x.abs(), normal.y.abs(), normal.z.abs());

                    let (u_axis, v_axis) = if abs_normal.y >= abs_normal.x && abs_normal.y >= abs_normal.z {
                        // Top/bottom face - project onto XZ
                        (crate::rasterizer::Vec3::new(1.0, 0.0, 0.0), crate::rasterizer::Vec3::new(0.0, 0.0, 1.0))
                    } else if abs_normal.x >= abs_normal.z {
                        // Side face (X dominant) - project onto YZ
                        (crate::rasterizer::Vec3::new(0.0, 0.0, 1.0), crate::rasterizer::Vec3::new(0.0, 1.0, 0.0))
                    } else {
                        // Front/back face (Z dominant) - project onto XY
                        (crate::rasterizer::Vec3::new(1.0, 0.0, 0.0), crate::rasterizer::Vec3::new(0.0, 1.0, 0.0))
                    };

                    // Project all vertices of n-gon onto UV plane
                    // Scale factor: 1 world unit = 1/64 of texture (adjustable)
                    let uv_scale = 1.0 / 64.0;

                    // Normalize to 0-1 range by taking fractional part
                    let norm_uv = |u: f32, v: f32| {
                        let u = u.rem_euclid(1.0);
                        let v = v.rem_euclid(1.0);
                        snap_uv(u, v, atlas_size)
                    };

                    for &vi in &face.vertices {
                        let pos = obj.mesh.vertices[vi].pos;
                        let u = pos.dot(u_axis) * uv_scale;
                        let v = pos.dot(v_axis) * uv_scale;
                        let (su, sv) = norm_uv(u, v);
                        obj.mesh.vertices[vi].uv = crate::rasterizer::Vec2::new(su, sv);
                    }
                }
            }
        }

        // Project is single source of truth
        state.dirty = true;
        state.set_status("Reset UVs to planar projection", 1.0);
    } else {
        state.set_status("Select faces to reset UVs", 1.0);
    }
}

/// Handle all keyboard actions using the action registry
/// Returns a ModelerAction if a file action was triggered
fn handle_actions(actions: &ActionRegistry, state: &mut ModelerState, ui_ctx: &crate::ui::UiContext) -> ModelerAction {
    use crate::rasterizer::ShadingMode;
    use crate::ui::Axis as UiAxis;

    // Build context for action enable/disable checks
    let has_selection = !state.selection.is_empty();
    let has_face_selection = matches!(&state.selection, super::state::ModelerSelection::Faces(f) if !f.is_empty());
    let has_vertex_selection = matches!(&state.selection, super::state::ModelerSelection::Vertices(v) if !v.is_empty());
    let select_mode_str = match state.select_mode {
        SelectMode::Vertex => "vertex",
        SelectMode::Edge => "edge",
        SelectMode::Face => "face",
    };
    let is_dragging = state.drag_manager.is_dragging() || state.modal_transform != ModalTransform::None;
    let is_paint_mode = state.paint_section_expanded;

    let ctx = build_context(
        state.can_undo(),
        state.can_redo(),
        has_selection,
        has_face_selection,
        has_vertex_selection,
        select_mode_str,
        false, // text_editing - would need to track this
        state.dirty,
        is_dragging,
        is_paint_mode,
    );

    let mut action = ModelerAction::None;

    // ========================================================================
    // File Actions (return ModelerAction)
    // ========================================================================
    if actions.triggered("file.new", &ctx) {
        action = ModelerAction::New;
    }
    if actions.triggered("file.open", &ctx) {
        action = ModelerAction::PromptLoad;
    }
    if actions.triggered("file.save", &ctx) {
        action = ModelerAction::Save;
    }
    if actions.triggered("file.save_as", &ctx) {
        action = ModelerAction::SaveAs;
    }

    // ========================================================================
    // Edit Actions
    // ========================================================================
    if actions.triggered("edit.undo", &ctx) {
        state.undo();
        return action; // Don't process other shortcuts after undo
    }
    if actions.triggered("edit.redo", &ctx) || actions.triggered("edit.redo_alt", &ctx) {
        state.redo();
        return action;
    }
    if actions.triggered("edit.delete", &ctx) || actions.triggered("edit.delete_alt", &ctx) {
        delete_selection(state);
    }

    // ========================================================================
    // Selection Mode Actions
    // ========================================================================
    if actions.triggered("select.vertex_mode", &ctx) {
        state.select_mode = SelectMode::Vertex;
        state.set_selection(super::state::ModelerSelection::None);
        state.set_status("Vertex mode", 1.0);
    }
    if actions.triggered("select.edge_mode", &ctx) {
        state.select_mode = SelectMode::Edge;
        state.set_selection(super::state::ModelerSelection::None);
        state.set_status("Edge mode", 1.0);
    }
    if actions.triggered("select.face_mode", &ctx) {
        state.select_mode = SelectMode::Face;
        state.set_selection(super::state::ModelerSelection::None);
        state.set_status("Face mode", 1.0);
    }
    if actions.triggered("select.all", &ctx) {
        select_all(state);
    }

    // ========================================================================
    // Transform Actions (Modal - G/R/T)
    // These set the modal_transform mode; viewport.rs will start the actual drag
    // Also select the corresponding tool so the toolbar highlights
    // Allow switching modes during active modal transform (cancel current and start new)
    // ========================================================================
    let in_modal_transform = state.modal_transform != ModalTransform::None;
    let gizmo_dragging = state.drag_manager.is_dragging() && !in_modal_transform;

    // Helper to cancel current modal transform and restore original positions
    let cancel_modal = |state: &mut ModelerState| {
        if state.modal_transform != ModalTransform::None {
            // Sync tool state
            match state.modal_transform {
                ModalTransform::Grab => state.tool_box.tools.move_tool.end_drag(),
                ModalTransform::Scale => state.tool_box.tools.scale.end_drag(),
                ModalTransform::Rotate => state.tool_box.tools.rotate.end_drag(),
                ModalTransform::None => {}
            }
            // Cancel drag and restore positions
            if let Some(original_positions) = state.drag_manager.cancel() {
                if let Some(mesh) = state.mesh_mut() {
                    for (vert_idx, original_pos) in original_positions {
                        if let Some(vert) = mesh.vertices.get_mut(vert_idx) {
                            vert.pos = original_pos;
                        }
                    }
                }
            }
            // Pop undo since we're canceling (the push happened when drag started)
            state.undo_stack.pop();
            state.modal_transform = ModalTransform::None;
        }
    };

    if actions.triggered("transform.grab", &ctx) && !gizmo_dragging {
        if state.modal_transform != ModalTransform::Grab {
            cancel_modal(state);
            state.modal_transform = ModalTransform::Grab;
            state.tool_box.toggle(ModelerToolId::Move);
        }
    }
    if actions.triggered("transform.rotate", &ctx) && !gizmo_dragging {
        if state.modal_transform != ModalTransform::Rotate {
            cancel_modal(state);
            state.modal_transform = ModalTransform::Rotate;
            state.tool_box.toggle(ModelerToolId::Rotate);
        }
    }
    if actions.triggered("transform.scale", &ctx) && !gizmo_dragging {
        if state.modal_transform != ModalTransform::Scale {
            cancel_modal(state);
            state.modal_transform = ModalTransform::Scale;
            state.tool_box.toggle(ModelerToolId::Scale);
        }
    }
    if actions.triggered("transform.extrude", &ctx) {
        // Perform extrude immediately on selected faces
        if let super::state::ModelerSelection::Faces(face_indices) = &state.selection {
            if !face_indices.is_empty() {
                let indices = face_indices.clone();
                state.push_undo("Extrude");
                let extrude_amount = state.snap_settings.grid_size;
                let new_faces = if let Some(mesh) = state.mesh_mut() {
                    mesh.extrude_faces(&indices, extrude_amount)
                } else {
                    vec![]
                };
                state.selection = super::state::ModelerSelection::Faces(new_faces);
                state.dirty = true;
                state.set_status(&format!("Extruded {} face(s)", indices.len()), 1.0);
            } else {
                state.set_status("Select faces to extrude", 1.0);
            }
        } else {
            state.set_status("Switch to Face mode (3) to extrude", 1.0);
        }
    }

    // ========================================================================
    // View Actions
    // ========================================================================
    if actions.triggered("view.toggle_fullscreen", &ctx) {
        state.toggle_fullscreen_viewport();
    }
    if actions.triggered("view.toggle_wireframe", &ctx) {
        state.raster_settings.wireframe_overlay = !state.raster_settings.wireframe_overlay;
        let mode = if state.raster_settings.wireframe_overlay { "Wireframe" } else { "Solid" };
        state.set_status(&format!("Render: {}", mode), 1.0);
    }
    if actions.triggered("view.cycle_shading", &ctx) {
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
    // Axis Constraints (during transforms)
    // ========================================================================
    if actions.triggered("axis.constrain_x", &ctx) {
        state.drag_manager.set_axis(Some(UiAxis::X));
        state.set_status("X axis", 0.5);
    }
    if actions.triggered("axis.constrain_y", &ctx) {
        state.drag_manager.set_axis(Some(UiAxis::Y));
        state.set_status("Y axis", 0.5);
    }
    // Note: Z axis constraint only works when dragging (otherwise Z is snap toggle)
    if is_dragging && actions.triggered("axis.constrain_z", &ctx) {
        state.drag_manager.set_axis(Some(UiAxis::Z));
        state.set_status("Z axis", 0.5);
    }

    // ========================================================================
    // Atlas/Paint Mode Actions (toggle between UV and Paint section focus)
    // ========================================================================
    if actions.triggered("atlas.toggle_mode", &ctx) {
        // Toggle focus between UV and Paint sections
        if state.paint_section_expanded && !state.uv_section_expanded {
            // Paint is primary, switch to UV
            state.uv_section_expanded = true;
            state.paint_section_expanded = false;
            state.set_status("UV section", 1.0);
        } else {
            // UV is primary or both/neither, switch to Paint
            state.uv_section_expanded = false;
            state.paint_section_expanded = true;
            state.uv_selection.clear();
            // Initialize editing texture when switching to paint
            if state.editing_texture.is_none() {
                state.editing_texture = Some(create_editing_texture(state));
                state.texture_editor.reset();
            }
            state.set_status("Paint section", 1.0);
        }
    }
    if actions.triggered("brush.square", &ctx) {
        state.brush_type = super::state::BrushType::Square;
        state.set_status("Square brush", 0.5);
    }
    if actions.triggered("brush.fill", &ctx) {
        state.brush_type = super::state::BrushType::Fill;
        state.set_status("Fill brush", 0.5);
    }

    // ========================================================================
    // UV Transform Actions
    // ========================================================================
    if actions.triggered("uv.flip_horizontal", &ctx) {
        flip_selected_uvs(state, true, false);
    }
    if actions.triggered("uv.flip_vertical", &ctx) {
        flip_selected_uvs(state, false, true);
    }
    if actions.triggered("uv.rotate_cw", &ctx) {
        rotate_selected_uvs(state, true);
    }
    if actions.triggered("uv.reset", &ctx) {
        reset_selected_uvs(state);
    }

    // ========================================================================
    // Context Menu Actions
    // ========================================================================
    if actions.triggered("context.open_menu", &ctx) {
        // Tab key opens context menu at mouse position
        let (mx, my) = (ui_ctx.mouse.x, ui_ctx.mouse.y);
        let world_pos = screen_to_world_position(state, mx, my);
        let snapped = state.snap_settings.snap_vec3(world_pos);
        state.context_menu = Some(ContextMenu::new(mx, my, snapped, state.active_viewport));
    }
    if actions.triggered("context.close", &ctx) {
        // Escape closes menus or cancels operations (priority order)
        if state.context_menu.is_some() {
            state.context_menu = None;
        } else if state.drag_manager.is_dragging() {
            // Sync tool state before cancelling
            match state.modal_transform {
                ModalTransform::Grab => state.tool_box.tools.move_tool.end_drag(),
                ModalTransform::Scale => state.tool_box.tools.scale.end_drag(),
                ModalTransform::Rotate => state.tool_box.tools.rotate.end_drag(),
                ModalTransform::None => {
                    // Also handle gizmo drags (not modal)
                    if state.drag_manager.active.is_move() {
                        state.tool_box.tools.move_tool.end_drag();
                    } else if state.drag_manager.active.is_scale() {
                        state.tool_box.tools.scale.end_drag();
                    } else if state.drag_manager.active.is_rotate() {
                        state.tool_box.tools.rotate.end_drag();
                    }
                }
            }
            // Cancel active drag and restore original positions
            if let Some(original_positions) = state.drag_manager.cancel() {
                if let Some(mesh) = state.mesh_mut() {
                    for (idx, pos) in original_positions {
                        if let Some(vert) = mesh.vertices.get_mut(idx) {
                            vert.pos = pos;
                        }
                    }
                }
            }
            state.modal_transform = ModalTransform::None;
        } else if state.drag_manager.active.is_free_move() {
            // Cancel free move (perspective or ortho drag)
            if let Some(original_positions) = state.drag_manager.cancel() {
                if let Some(mesh) = state.mesh_mut() {
                    for (vert_idx, original_pos) in original_positions {
                        if let Some(vert) = mesh.vertices.get_mut(vert_idx) {
                            vert.pos = original_pos;
                        }
                    }
                }
            }
            state.ortho_drag_viewport = None;
            state.set_status("Move cancelled", 0.5);
        } else if state.drag_manager.active.is_box_select() {
            // Cancel box selection via DragManager
            state.drag_manager.cancel();
            state.box_select_pending_start = None;
        }
    }

    // ========================================================================
    // Arrow Key Movement (PicoCAD-style)
    // ========================================================================
    // Z key = temporarily disable snap (held key, not triggered through actions)
    let snap_override = is_key_down(KeyCode::Z);
    let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);

    // Handle movement with small/regular variants
    let move_triggered =
        actions.triggered("move.left", &ctx) || actions.triggered("move.right", &ctx) ||
        actions.triggered("move.up", &ctx) || actions.triggered("move.down", &ctx) ||
        actions.triggered("move.left_small", &ctx) || actions.triggered("move.right_small", &ctx) ||
        actions.triggered("move.up_small", &ctx) || actions.triggered("move.down_small", &ctx);

    if move_triggered {
        handle_arrow_key_movement(state, shift, snap_override);
    }

    action
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
        vertex_indices = state.mesh().expand_to_coincident(&vertex_indices, 0.001);
    }

    // Save undo state before moving
    state.push_undo("Move");

    // Move selected vertices
    if let Some(mesh) = state.mesh_mut() {
        for &vi in &vertex_indices {
            if let Some(vert) = mesh.vertices.get_mut(vi) {
                vert.pos.x += delta.x;
                vert.pos.y += delta.y;
                vert.pos.z += delta.z;
            }
        }
    }

    state.dirty = true;

    // Show status
    let snap_status = if snap_disabled { " (free)" } else { "" };
    state.set_status(&format!("Moved {} vert(s){}", vertex_indices.len(), snap_status), 0.5);
}

/// Select all elements based on current selection mode
fn select_all(state: &mut ModelerState) {
    let mesh = state.mesh();

    match state.select_mode {
        SelectMode::Vertex => {
            let all_verts: Vec<usize> = (0..mesh.vertices.len()).collect();
            let count = all_verts.len();
            state.set_selection(super::state::ModelerSelection::Vertices(all_verts));
            state.set_status(&format!("Selected {} vertices", count), 1.0);
        }
        SelectMode::Edge => {
            // Collect all unique edges from all faces
            let mut all_edges: Vec<(usize, usize)> = Vec::new();
            for face in &mesh.faces {
                for edge in face.edges() {
                    // Normalize edge (smaller index first)
                    let norm = if edge.0 < edge.1 { edge } else { (edge.1, edge.0) };
                    if !all_edges.contains(&norm) {
                        all_edges.push(norm);
                    }
                }
            }
            let count = all_edges.len();
            state.set_selection(super::state::ModelerSelection::Edges(all_edges));
            state.set_status(&format!("Selected {} edges", count), 1.0);
        }
        SelectMode::Face => {
            let all_faces: Vec<usize> = (0..mesh.faces.len()).collect();
            let count = all_faces.len();
            state.set_selection(super::state::ModelerSelection::Faces(all_faces));
            state.set_status(&format!("Selected {} faces", count), 1.0);
        }
    }
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

            if let Some(mesh) = state.mesh_mut() {
                for fi in indices {
                    if fi < mesh.faces.len() {
                        mesh.faces.remove(fi);
                    }
                }

                // Clean up orphaned vertices (vertices not referenced by any face)
                let mut referenced: std::collections::HashSet<usize> = std::collections::HashSet::new();
                for face in &mesh.faces {
                    for &vi in &face.vertices {
                        referenced.insert(vi);
                    }
                }

                // Find orphaned vertices (in reverse order for safe removal)
                let mut orphaned: Vec<usize> = (0..mesh.vertices.len())
                    .filter(|vi| !referenced.contains(vi))
                    .collect();
                orphaned.sort();
                orphaned.reverse();

                // Remove orphaned vertices and update face indices
                for vi in orphaned {
                    mesh.vertices.remove(vi);
                    // Update face vertex indices that are higher than the removed vertex
                    for face in &mut mesh.faces {
                        for v in &mut face.vertices {
                            if *v > vi { *v -= 1; }
                        }
                    }
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

            // Then remove vertices (in reverse order to avoid index shifting)
            let mut indices = vert_indices.clone();
            indices.sort();
            indices.reverse();

            if let Some(mesh) = state.mesh_mut() {
                // Remove faces that reference any of the deleted vertices
                mesh.faces.retain(|f| {
                    !f.vertices.iter().any(|&vi| vert_set.contains(&vi))
                });

                for vi in &indices {
                    if *vi < mesh.vertices.len() {
                        mesh.vertices.remove(*vi);

                        // Update face vertex indices that are higher than the removed vertex
                        for face in &mut mesh.faces {
                            for v in &mut face.vertices {
                                if *v > *vi { *v -= 1; }
                            }
                        }
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

            let deleted = if let Some(mesh) = state.mesh_mut() {
                let faces_before = mesh.faces.len();
                // Retain faces that don't contain any of the deleted edges
                mesh.faces.retain(|f| {
                    !f.edges().any(|(v0, v1)| {
                        let e = (v0.min(v1), v0.max(v1));
                        edge_set.contains(&e)
                    })
                });

                // Clean up orphaned vertices (vertices not referenced by any face)
                let mut referenced: std::collections::HashSet<usize> = std::collections::HashSet::new();
                for face in &mesh.faces {
                    for &vi in &face.vertices {
                        referenced.insert(vi);
                    }
                }

                let mut orphaned: Vec<usize> = (0..mesh.vertices.len())
                    .filter(|vi| !referenced.contains(vi))
                    .collect();
                orphaned.sort();
                orphaned.reverse();

                for vi in orphaned {
                    mesh.vertices.remove(vi);
                    for face in &mut mesh.faces {
                        for v in &mut face.vertices {
                            if *v > vi { *v -= 1; }
                        }
                    }
                }

                faces_before - mesh.faces.len()
            } else {
                0
            };
            state.selection = super::state::ModelerSelection::None;
            state.dirty = true;
            state.set_status(&format!("Deleted {} face(s) with edges", deleted), 1.0);
        }
        _ => {
            state.set_status("Nothing selected to delete", 1.0);
            return;
        }
    }
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
            // Collect unique vertices from faces (now returns &[usize] for n-gons)
            let mesh = state.mesh();
            let mut verts: Vec<usize> = face_indices.iter()
                .filter_map(|&fi| mesh.face_vertices(fi))
                .flat_map(|verts| verts.to_vec())
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
    // Note: Tab/Escape shortcuts are now handled through ActionRegistry in handle_actions()

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
        let size = 512.0; // Reasonable size for new primitives (half of default cube)
        let new_mesh = prim.create(size);
        if let Some(mesh) = state.mesh_mut() {
            mesh.merge(&new_mesh, menu.world_pos);
        }
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
        let clone = state.mesh().clone();
        if let Some(mesh) = state.mesh_mut() {
            mesh.merge(&clone, offset);
        }
        state.dirty = true;
        state.set_status("Cloned mesh", 1.0);
        state.context_menu = None;
    }

    if clear_clicked {
        state.push_undo("Clear mesh");
        if let Some(mesh) = state.mesh_mut() {
            *mesh = EditableMesh::new();
        }
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
