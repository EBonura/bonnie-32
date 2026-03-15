//! Modeler UI layout and rendering

use macroquad::prelude::*;
use crate::storage::Storage;
use crate::ui::{Rect, UiContext, SplitPanel, draw_panel, panel_content_rect, draw_collapsible_panel, Toolbar, icon, icon_button, ActionRegistry, draw_icon_centered, TextInputState, draw_text_input, dropdown_block_clicks, draw_dropdown_trigger, begin_dropdown, dropdown_item, dropdown_menu_rect};
use crate::rasterizer::{Framebuffer, render_mesh, render_mesh_15, Camera, OrthoProjection, point_in_triangle_2d};
use crate::rasterizer::{Vertex as RasterVertex, Face as RasterFace, Color as RasterColor};
use crate::rasterizer::{ClutDepth, Clut, Color15};
use super::state::{ModelerState, SelectMode, ViewportId, ContextMenu, ModalTransform, CameraMode, Axis, MirrorSettings, rotate_by_euler, inverse_rotate_by_euler};
use crate::asset::AssetComponent;
use crate::texture::{
    UserTexture, TextureSize, generate_texture_id,
    draw_texture_canvas, draw_tool_panel, draw_palette_panel_constrained, draw_mode_tabs,
    TextureEditorMode, UvOverlayData, UvVertex, UvFace, draw_import_dialog, ImportAction,
    load_png_to_import_state,
};
use super::tools::ModelerToolId;
use super::viewport::{draw_modeler_viewport, draw_modeler_viewport_ext};
use super::mesh_editor::{EditableMesh, MeshPart, TextureRef};
use super::actions::{create_modeler_actions, build_context};
use crate::rasterizer::{Vec3, Vec2 as RastVec2};

// Colors (matching tracker/editor style)
const BG_COLOR: Color = Color::new(0.11, 0.11, 0.13, 1.0);
const HEADER_COLOR: Color = Color::new(0.15, 0.15, 0.18, 1.0);
const TEXT_COLOR: Color = Color::new(0.8, 0.8, 0.85, 1.0);
const TEXT_DIM: Color = Color::new(0.4, 0.4, 0.45, 1.0);
const ACCENT_COLOR: Color = Color::new(0.0, 0.75, 0.9, 1.0);

/// Standard font sizes for consistent UI (matching World Editor)
const FONT_SIZE_TITLE: f32 = 16.0;
const FONT_SIZE_HEADER: f32 = 14.0;
const FONT_SIZE_CONTENT: f32 = 12.0;
const LINE_HEIGHT: f32 = 16.0;

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
    ImportObj,      // Import OBJ file
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
            right_split: SplitPanel::horizontal(101).with_ratio(0.73).with_min_size(150.0),
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
    storage: &Storage,
) -> ModelerAction {
    // Apply auto-focus transparency: dim non-selected components
    state.apply_focus_opacity();

    // Save original click state for menus (restored before processing dropdowns)
    let original_left_pressed = ctx.mouse.left_pressed;

    // Block clicks when any dropdown is open (unified dropdown system)
    dropdown_block_clicks(ctx, &state.dropdown);

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
    draw_right_panel(ctx, panel_content_rect(right_rect, false), state, icon_font, storage);

    // Draw timeline if in animate mode
    if let Some(tl_rect) = timeline_rect {
        draw_panel(tl_rect, Some("Timeline"), Color::from_rgba(30, 30, 35, 255));
        draw_timeline(ctx, panel_content_rect(tl_rect, true), state, icon_font);
    }

    // Draw status bar
    draw_status_bar(status_rect, state);

    // Handle keyboard shortcuts using action registry (but not when a dialog is open)
    let dialog_open = state.rename_dialog.is_some() || state.delete_dialog.is_some();
    let keyboard_action = if dialog_open {
        ModelerAction::None
    } else {
        handle_actions(&layout.actions, state, ctx)
    };
    let action = if keyboard_action != ModelerAction::None { keyboard_action } else { action };

    // Draw popups and menus (on top of everything)
    // Restore click state so menus can process their clicks
    ctx.mouse.left_pressed = original_left_pressed;
    draw_add_component_popup(ctx, left_rect, state, icon_font);
    draw_bone_picker_popup(ctx, left_rect, state, icon_font);
    draw_opacity_slider_popup(ctx, state);
    draw_snap_menu(ctx, state);
    draw_context_menu(ctx, state);

    // Draw radial menu (new hold-to-show menu)
    draw_and_handle_radial_menu(ctx, state);

    // Draw rename/delete dialogs (modal, on top of everything)
    draw_object_dialogs(ctx, state, icon_font);

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
        // Upload (import from local file)
        if toolbar.icon_button(ctx, icon::FOLDER_OPEN, icon_font, "Upload") {
            action = ModelerAction::Import;
        }

        // Save to cloud (only enabled when authenticated)
        if crate::auth::is_authenticated() {
            if toolbar.icon_button(ctx, icon::SAVE, icon_font, "Save to cloud") {
                action = ModelerAction::Save;
            }
        } else {
            toolbar.icon_button_disabled(ctx, icon::SAVE, icon_font, "Sign in to save custom assets");
        }

        // Download (export to local file)
        if toolbar.icon_button(ctx, icon::DOWNLOAD, icon_font, "Download") {
            action = ModelerAction::Export;
        }
    }

    // Asset browser (works on both native and WASM)
    if toolbar.icon_button(ctx, icon::BOOK_OPEN, icon_font, "Browse Assets") {
        action = ModelerAction::BrowseModels;
    }

    // Import OBJ file
    if toolbar.icon_button(ctx, icon::FOLDER_OPEN, icon_font, "Import OBJ") {
        action = ModelerAction::ImportObj;
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

    // Transform orientation toggle (Global/Local)
    {
        use super::state::TransformOrientation;
        let is_local = state.transform_orientation == TransformOrientation::Local;
        let icon_char = if is_local { icon::FOCUS } else { icon::GLOBE };
        let tooltip = if is_local { "Orientation: Local (click for Global)" } else { "Orientation: Global (click for Local)" };
        if toolbar.icon_button_active(ctx, icon_char, icon_font, tooltip, is_local) {
            state.transform_orientation = state.transform_orientation.toggle();
            state.set_status(&format!("Transform orientation: {}", state.transform_orientation.label()), 1.5);
        }
    }

    toolbar.separator();

    // Selection mode buttons (Vertex/Edge/Face)
    {
        let is_vertex = state.select_mode == SelectMode::Vertex;
        let is_edge = state.select_mode == SelectMode::Edge;
        let is_face = state.select_mode == SelectMode::Face;

        if toolbar.icon_button_active(ctx, icon::CIRCLE, icon_font, "Vertex Mode (1)", is_vertex) {
            state.select_mode = SelectMode::Vertex;
            state.selection.clear();
            state.set_status("Vertex selection mode", 1.0);
        }
        if toolbar.icon_button_active(ctx, icon::MINUS, icon_font, "Edge Mode (2)", is_edge) {
            state.select_mode = SelectMode::Edge;
            state.selection.clear();
            state.set_status("Edge selection mode", 1.0);
        }
        if toolbar.icon_button_active(ctx, icon::SQUARE, icon_font, "Face Mode (3)", is_face) {
            state.select_mode = SelectMode::Face;
            state.selection.clear();
            state.set_status("Face selection mode", 1.0);
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
    if toolbar.icon_button_active(ctx, icon::LAYERS, icon_font, "Wireframe Mode (Shift+Z)", state.raster_settings.wireframe_overlay) {
        state.raster_settings.wireframe_overlay = !state.raster_settings.wireframe_overlay;
        let mode = if state.raster_settings.wireframe_overlay { "Wireframe" } else { "Solid" };
        state.set_status(&format!("Render: {}", mode), 1.5);
    }
    if toolbar.icon_button_active(ctx, icon::BLEND, icon_font, "X-Ray Mode (Alt+Z)", state.xray_mode) {
        state.xray_mode = !state.xray_mode;
        state.raster_settings.xray_mode = state.xray_mode;
        let mode = if state.xray_mode { "ON" } else { "OFF" };
        state.set_status(&format!("X-Ray: {}", mode), 1.5);
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

    // Snap toggle (icon) + grid size (clickable label)
    if toolbar.icon_button_active(ctx, icon::GRID, icon_font, "Snap to Grid [S key]", state.snap_settings.enabled) {
        state.snap_settings.enabled = !state.snap_settings.enabled;
        let mode = if state.snap_settings.enabled { "ON" } else { "OFF" };
        state.set_status(&format!("Grid Snap: {}", mode), 1.5);
    }
    // Clickable grid size label (opens snap menu dropdown)
    let size_label = format!("{}", state.snap_settings.grid_size as i32);
    let (size_clicked, size_rect) = toolbar.clickable_label(ctx, &size_label, "Click to change snap grid size");
    if size_clicked {
        state.dropdown.toggle("snap_menu", size_rect);
    }
    // Vertex linking toggle (move coincident vertices together)
    let link_icon = if state.vertex_linking { icon::LINK } else { icon::LINK_OFF };
    if toolbar.icon_button_active(ctx, link_icon, icon_font, "Vertex Linking (move welded verts together)", state.vertex_linking) {
        state.vertex_linking = !state.vertex_linking;
        let mode = if state.vertex_linking { "ON" } else { "OFF" };
        state.set_status(&format!("Vertex Linking: {}", mode), 1.5);
    }

    // Note: Mirror editing is now per-object in the Properties section

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
    let total_verts: usize = state.objects().iter().map(|o| o.mesh.vertex_count()).sum();
    let total_faces: usize = state.objects().iter().map(|o| o.mesh.face_count()).sum();
    draw_text(
        &format!("{} objects | {} verts | {} faces",
            state.objects().len(), total_verts, total_faces),
        rect.x, y + 14.0, 12.0, TEXT_DIM,
    );
    y += row_height;

    // Separator
    draw_line(rect.x, y, rect.x + rect.w, y, 1.0, Color::from_rgba(60, 60, 65, 255));
    y += 4.0;

    // List of objects
    let selected_idx = state.selected_object;
    let mouse_pos = (ctx.mouse.x, ctx.mouse.y);
    let mut clicked_object: Option<usize> = None;
    let mut toggle_visibility: Option<usize> = None;

    for (i, obj) in state.objects().iter().enumerate() {
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
        if let Some(obj) = state.objects_mut().and_then(|v| v.get_mut(i)) {
            obj.visible = !obj.visible;
        }
    }
    if let Some(i) = clicked_object {
        state.select_object(i);
    }

    // Show selection info at bottom
    if let Some(idx) = state.selected_object {
        if let Some(obj) = state.objects().get(idx) {
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

const COLLAPSED_HEADER_HEIGHT: f32 = 20.0;

fn draw_left_panel(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, icon_font: Option<&Font>) {
    let panel_bg = Color::from_rgba(35, 35, 40, 255);

    // Calculate available height for expanded panels
    let total_height = rect.h;
    let num_panels = 3;

    // Count collapsed panels to distribute remaining space
    let collapsed_count = [
        !state.components_section_expanded,
        !state.properties_section_expanded,
        !state.lights_section_expanded,
    ].iter().filter(|&&c| c).count();

    let expanded_count = num_panels - collapsed_count;
    let collapsed_height = collapsed_count as f32 * COLLAPSED_HEADER_HEIGHT;
    let available_for_expanded = total_height - collapsed_height;

    // Distribute height among expanded panels
    let expanded_panel_height = if expanded_count > 0 {
        available_for_expanded / expanded_count as f32
    } else {
        100.0
    };

    let mut y = rect.y;

    // === COMPONENTS SECTION ===
    let comp_collapsed = !state.components_section_expanded;
    let comp_h = if comp_collapsed { COLLAPSED_HEADER_HEIGHT } else { expanded_panel_height };
    let comp_rect = Rect::new(rect.x, y, rect.w, comp_h);
    let (clicked, comp_content) = draw_collapsible_panel(ctx, comp_rect, "Components", comp_collapsed, panel_bg);
    if clicked {
        state.components_section_expanded = !state.components_section_expanded;
    }
    if let Some(content) = comp_content {
        let mut cy = content.y;
        draw_components_section(ctx, content.x, &mut cy, content.w, state, icon_font);
    }
    y += comp_h;

    // === PROPERTIES SECTION ===
    let props_collapsed = !state.properties_section_expanded;
    let props_h = if props_collapsed { COLLAPSED_HEADER_HEIGHT } else { expanded_panel_height };
    let props_title = if let Some(comp_idx) = state.selected_component {
        let comp_name = state.asset.components.get(comp_idx)
            .map(|c| c.type_name())
            .unwrap_or("Component");
        format!("Properties: {}", comp_name)
    } else {
        "Properties".to_string()
    };
    let props_rect = Rect::new(rect.x, y, rect.w, props_h);
    let (clicked, props_content) = draw_collapsible_panel(ctx, props_rect, &props_title, props_collapsed, panel_bg);
    if clicked {
        state.properties_section_expanded = !state.properties_section_expanded;
    }
    if let Some(content) = props_content {
        if let Some(comp_idx) = state.selected_component {
            // For Mesh/Skeleton, show embedded content; for others, show property editor
            let component_type = state.asset.components.get(comp_idx)
                .map(|c| (c.is_mesh(), c.is_skeleton()))
                .unwrap_or((false, false));

            match component_type {
                (true, _) => draw_mesh_editor_content(ctx, content, state, icon_font),
                (_, true) => draw_skeleton_editor_content(ctx, content, state, icon_font),
                _ => {
                    let mut cy = content.y;
                    draw_component_editor(ctx, content.x, &mut cy, content.w, state, icon_font);
                }
            }
        } else {
            draw_text("Select a component", content.x + 4.0, content.y + 12.0, FONT_SIZE_HEADER, TEXT_DIM);
        }
    }
    y += props_h;

    // === LIGHTS SECTION ===
    let lights_collapsed = !state.lights_section_expanded;
    let lights_h = if lights_collapsed { COLLAPSED_HEADER_HEIGHT } else { expanded_panel_height };
    let lights_rect = Rect::new(rect.x, y, rect.w, lights_h);
    let (clicked, lights_content) = draw_collapsible_panel(ctx, lights_rect, "Lights", lights_collapsed, panel_bg);
    if clicked {
        state.lights_section_expanded = !state.lights_section_expanded;
    }
    if let Some(content) = lights_content {
        let mut cy = content.y;
        draw_lights_section(ctx, content.x, &mut cy, content.w, state, icon_font);
    }
}

/// Helper to get a Lucide icon for component types
fn component_icon(comp: &AssetComponent) -> char {
    match comp {
        AssetComponent::Mesh { .. } => icon::BOX,
        AssetComponent::Collision { .. } => icon::SCAN,
        AssetComponent::Light { .. } => icon::SUN,
        AssetComponent::Trigger { .. } => icon::MAP_PIN,
        AssetComponent::Pickup { .. } => icon::PLUS,
        AssetComponent::Enemy { .. } => icon::PERSON_STANDING,
        AssetComponent::Door { .. } => icon::DOOR_CLOSED,
        AssetComponent::Audio { .. } => icon::MUSIC,
        AssetComponent::Particle { .. } => icon::BLEND,
        AssetComponent::CharacterController { .. } => icon::GAMEPAD_2,
        AssetComponent::SpawnPoint { .. } => icon::FOOTPRINTS,
        AssetComponent::Skeleton { .. } => icon::BONE,
    }
}

/// Draw components section (component list with add/remove)
fn draw_components_section(ctx: &mut UiContext, x: f32, y: &mut f32, width: f32, state: &mut ModelerState, icon_font: Option<&Font>) {
    let line_height = 18.0;
    let btn_size = 18.0;

    // Component count and add/remove buttons
    let comp_count = state.asset.components.len();
    draw_text(&format!("{} component(s)", comp_count), x + 4.0, *y + 13.0, FONT_SIZE_HEADER, TEXT_COLOR);

    // Add button (opens add component dropdown)
    let add_rect = Rect::new(x + width - btn_size * 2.0 - 8.0, *y, btn_size, btn_size);
    if icon_button(ctx, add_rect, icon::PLUS, icon_font, "Add component") {
        state.dropdown.toggle("add_component", add_rect);
    }

    // Remove button (disabled for Mesh, requires selection)
    let rem_rect = Rect::new(x + width - btn_size - 4.0, *y, btn_size, btn_size);
    let can_remove = state.selected_component
        .and_then(|idx| state.asset.components.get(idx))
        .map(|c| !c.is_mesh())
        .unwrap_or(false);

    if can_remove {
        if icon_button(ctx, rem_rect, icon::MINUS, icon_font, "Remove component") {
            if let Some(idx) = state.selected_component {
                state.asset.components.remove(idx);
                state.selected_component = None;
            }
        }
    } else {
        // Draw disabled button
        draw_rectangle(rem_rect.x, rem_rect.y, rem_rect.w, rem_rect.h, Color::from_rgba(40, 40, 45, 255));
        draw_icon_centered(icon_font, icon::MINUS, &rem_rect, 12.0, TEXT_DIM);
    }

    *y += btn_size + 4.0;

    // List components
    let mut select_idx: Option<usize> = None;
    let mut delete_idx: Option<usize> = None;

    // Ensure opacity vec is sized correctly
    state.ensure_opacity_vec();

    for (i, comp) in state.asset.components.iter().enumerate() {
        let is_selected = state.selected_component == Some(i);
        let is_hidden = state.is_component_hidden(i);
        let item_rect = Rect::new(x, *y, width, line_height);
        let is_hovered = ctx.mouse.inside(&item_rect);

        // Selection highlight
        if is_selected {
            draw_rectangle(item_rect.x, item_rect.y, item_rect.w, item_rect.h, Color::from_rgba(60, 80, 100, 255));
        } else if is_hovered {
            draw_rectangle(item_rect.x, item_rect.y, item_rect.w, item_rect.h, Color::from_rgba(50, 50, 55, 255));
        }

        // Opacity indicator (click to drag vertical slider)
        // Show the displayed opacity (includes auto-dim), but drag from base
        let opacity = state.get_component_opacity(i);
        let base_opacity = state.base_component_opacity.get(i).copied().unwrap_or(0);
        let indicator_size = 14.0;
        let indicator_x = x + 2.0;
        let indicator_y = *y + (line_height - indicator_size) / 2.0;
        let indicator_rect = Rect::new(indicator_x, *y, indicator_size + 2.0, line_height);

        // Draw opacity indicator: vertical bar showing current level
        let bar_height = indicator_size - 2.0;
        let bar_width = 8.0;
        let bar_x = indicator_x + (indicator_size - bar_width) / 2.0;
        let bar_y = indicator_y + 1.0;

        // Background bar
        draw_rectangle(bar_x, bar_y, bar_width, bar_height, Color::from_rgba(40, 40, 45, 255));

        // Filled portion (inverted: 0=full, 7=empty)
        let fill_ratio = 1.0 - (opacity as f32 / 7.0);
        let fill_height = bar_height * fill_ratio;
        let fill_y = bar_y + (bar_height - fill_height);
        if fill_height > 0.0 {
            let brightness = (200.0 * fill_ratio) as u8 + 55;
            draw_rectangle(bar_x, fill_y, bar_width, fill_height, Color::from_rgba(brightness, brightness, brightness, 255));
        }

        // Click to start opacity drag
        if ctx.mouse.inside(&indicator_rect) && ctx.mouse.left_pressed && state.opacity_drag.is_none() {
            use super::state::OpacityDrag;
            state.opacity_drag = Some(OpacityDrag {
                component_idx: i,
                start_y: ctx.mouse.y,
                start_opacity: base_opacity,
                popup_x: x + width + 8.0, // Position popup to the right of the panel
            });
        }

        // Component icon
        let icon_rect = Rect::new(x + 20.0, *y + 1.0, 16.0, 16.0);
        let icon_char = component_icon(comp);
        let is_dimmed = opacity > 0 && !is_hidden;
        let dimmed_color = Color::new(0.55, 0.55, 0.6, 1.0);
        let icon_color = if is_hidden {
            TEXT_DIM
        } else if is_selected {
            ACCENT_COLOR
        } else if is_dimmed {
            dimmed_color
        } else {
            TEXT_COLOR
        };
        draw_icon_centered(icon_font, icon_char, &icon_rect, 11.0, icon_color);

        // Component type name
        let type_name = comp.type_name();
        let name_color = if is_hidden {
            TEXT_DIM
        } else if is_selected {
            ACCENT_COLOR
        } else if is_dimmed {
            dimmed_color
        } else {
            TEXT_COLOR
        };

        // For Mesh, show object count
        let label = if let AssetComponent::Mesh { parts } = comp {
            format!("{} ({})", type_name, parts.len())
        } else {
            type_name.to_string()
        };
        draw_text(&label, x + 40.0, *y + 13.0, FONT_SIZE_CONTENT, name_color);

        // Delete button (show on hover/selection)
        let show_delete = is_selected || is_hovered;
        if show_delete {
            let delete_rect = Rect::new(x + width - 18.0, *y + 1.0, 16.0, 16.0);
            let delete_hover = ctx.mouse.inside(&delete_rect);
            let delete_color = if delete_hover { Color::from_rgba(255, 100, 100, 255) } else { TEXT_DIM };
            draw_icon_centered(icon_font, icon::TRASH, &delete_rect, 11.0, delete_color);

            // Use clicked() (press + release) to avoid triggering on same frame as add
            if ctx.mouse.clicked(&delete_rect) {
                delete_idx = Some(i);
            }
        }

        // Click to select (not on opacity drag or delete)
        let name_rect = Rect::new(x + 56.0, *y, width - 76.0, line_height);
        if ctx.mouse.inside(&name_rect) && ctx.mouse.left_pressed
            && state.opacity_drag.is_none() && delete_idx.is_none() {
            select_idx = Some(i);
        }

        *y += line_height;
    }

    // Apply actions
    if let Some(idx) = delete_idx {
        // Open delete confirmation dialog
        state.delete_component_dialog = Some(idx);
    } else if let Some(idx) = select_idx {
        state.selected_component = Some(idx);
        if let Some(comp) = state.asset.components.get(idx) {
            // For Collision with linked mesh, auto-select that mesh part
            if let crate::asset::AssetComponent::Collision { collision_mesh: Some(ref name), .. } = comp {
                let mesh_name = name.clone();
                if let Some(obj_idx) = state.objects().iter().position(|o| o.name == mesh_name) {
                    state.selected_object = Some(obj_idx);
                }
            } else if !matches!(comp, crate::asset::AssetComponent::Mesh { .. }) {
                // Clear mesh selection when selecting a non-Mesh, non-Collision component
                state.selection.clear();
            }
        }
    }

}

/// Draw and handle the vertical opacity slider popup
fn draw_opacity_slider_popup(ctx: &mut UiContext, state: &mut ModelerState) {
    let drag = match state.opacity_drag {
        Some(d) => d,
        None => return,
    };

    // Slider dimensions
    let slider_height = 120.0;
    let slider_width = 24.0;
    let padding = 8.0;
    let popup_width = slider_width + padding * 2.0;
    let popup_height = slider_height + padding * 2.0 + 20.0; // Extra space for label

    // Position popup near the component (centered vertically on start position)
    let popup_x = drag.popup_x;
    let popup_y = (drag.start_y - popup_height / 2.0).max(10.0);

    // Draw popup background
    draw_rectangle(popup_x, popup_y, popup_width, popup_height, Color::from_rgba(35, 38, 45, 250));
    draw_rectangle_lines(popup_x, popup_y, popup_width, popup_height, 1.0, Color::from_rgba(80, 80, 90, 255));

    // Calculate current opacity from mouse Y delta
    // Moving UP = more visible (lower opacity), moving DOWN = more hidden (higher opacity)
    let delta_y = ctx.mouse.y - drag.start_y;
    let sensitivity = 15.0; // pixels per opacity level
    let opacity_delta = (delta_y / sensitivity).round() as i32;
    let new_opacity = (drag.start_opacity as i32 + opacity_delta).clamp(0, 7) as u8;

    // Apply the new opacity
    state.set_component_opacity(drag.component_idx, new_opacity);

    // Draw slider track
    let track_x = popup_x + padding;
    let track_y = popup_y + padding + 16.0; // Leave room for label
    draw_rectangle(track_x, track_y, slider_width, slider_height, Color::from_rgba(25, 28, 35, 255));

    // Draw 8 segments (0 at top = visible, 7 at bottom = hidden)
    let segment_height = slider_height / 8.0;
    for i in 0..8u8 {
        let seg_y = track_y + i as f32 * segment_height;
        let is_active = i <= new_opacity;
        let brightness = if is_active {
            255 - (i * 28) // Darker as we go down
        } else {
            50
        };
        let color = Color::from_rgba(brightness, brightness, brightness, 255);
        draw_rectangle(track_x + 2.0, seg_y + 1.0, slider_width - 4.0, segment_height - 2.0, color);
    }

    // Draw current position indicator (horizontal line)
    let indicator_y = track_y + (new_opacity as f32 + 0.5) * segment_height;
    draw_rectangle(track_x - 2.0, indicator_y - 1.0, slider_width + 4.0, 3.0, ACCENT_COLOR);

    // Draw label at top
    let label = match new_opacity {
        0 => "Visible",
        7 => "Hidden",
        _ => "",
    };
    if !label.is_empty() {
        draw_text(label, popup_x + padding, popup_y + padding + 10.0, 12.0, TEXT_COLOR);
    } else {
        draw_text(&format!("{}%", ((7 - new_opacity) as f32 / 7.0 * 100.0) as u8), popup_x + padding, popup_y + padding + 10.0, 12.0, TEXT_COLOR);
    }

    // End drag on mouse release
    if !ctx.mouse.left_down {
        state.opacity_drag = None;
    }
}

/// Create a default component of the given type
fn create_default_component(type_name: &str) -> AssetComponent {
    use crate::asset::CollisionShapeDef;
    use crate::game::components::{EnemyType, ItemType};

    match type_name {
        "Mesh" => AssetComponent::Mesh {
            parts: Vec::new(),
        },
        "Collision" => AssetComponent::Collision {
            shape: CollisionShapeDef::FromMesh,
            is_trigger: false,
            collision_mesh: None, // Mesh part created at add site
        },
        "Light" => AssetComponent::Light {
            color: [255, 255, 200],
            intensity: 2.0,        // Strong enough to be visible over ambient
            radius: 2048.0,        // 2 meters - covers typical mesh
            offset: [0.0, 1024.0, 1024.0], // Above and in front of origin
        },
        "Trigger" => AssetComponent::Trigger {
            trigger_id: "trigger_1".to_string(),
            on_enter: None,
            on_exit: None,
        },
        "Pickup" => AssetComponent::Pickup {
            item_type: ItemType::HealthPickup { amount: 25 },
            respawn_time: Some(30.0),
        },
        "Enemy" => AssetComponent::Enemy {
            enemy_type: EnemyType::Grunt,
            health: 100,
            damage: 10,
            patrol_radius: 512.0,
        },
        "Door" => AssetComponent::Door {
            required_key: None,
            start_open: false,
        },
        "Audio" => AssetComponent::Audio {
            sound: "ambient".to_string(),
            volume: 1.0,
            radius: 512.0,
            looping: true,
        },
        "Particle" => AssetComponent::Particle {
            effect: "smoke".to_string(),
            offset: [0.0, 0.0, 0.0],
            emitter_def: None,
        },
        "CharacterController" => AssetComponent::CharacterController {
            height: 1536.0,
            radius: 384.0,
            step_height: 384.0,
        },
        "SpawnPoint" => AssetComponent::SpawnPoint {
            is_player: false,
            respawns: false,
        },
        "Skeleton" => {
            use super::state::RigBone;
            use crate::rasterizer::Vec3;
            AssetComponent::Skeleton {
                bones: vec![RigBone {
                    name: "Root".to_string(),
                    parent: None,
                    local_position: Vec3::ZERO,
                    local_rotation: Vec3::ZERO,
                    length: 200.0,
                    width: RigBone::DEFAULT_WIDTH,
                }],
            }
        },
        _ => AssetComponent::Collision {
            shape: CollisionShapeDef::FromMesh,
            is_trigger: false,
            collision_mesh: None,
        },
    }
}

/// Draw component editor (dispatcher to type-specific editors)
fn draw_component_editor(ctx: &mut UiContext, x: f32, y: &mut f32, width: f32, state: &mut ModelerState, icon_font: Option<&Font>) {
    let comp_idx = match state.selected_component {
        Some(idx) => idx,
        None => {
            draw_text("No component selected", x + 4.0, *y + 12.0, FONT_SIZE_HEADER, TEXT_DIM);
            *y += 18.0;
            return;
        }
    };

    // Get a clone of the component to avoid borrow issues during editing
    let mut component = match state.asset.components.get(comp_idx) {
        Some(c) => c.clone(),
        None => {
            state.selected_component = None;
            return;
        }
    };

    let modified = match &mut component {
        AssetComponent::Mesh { .. } => {
            // Mesh is handled specially by draw_mesh_editor_content, should not reach here
            return;
        }
        AssetComponent::Collision { is_trigger, collision_mesh, .. } => {
            draw_collision_editor(ctx, x, y, width, is_trigger, collision_mesh, icon_font)
        }
        AssetComponent::Light { color, intensity, radius, offset } => {
            draw_light_component_editor(ctx, x, y, width, color, intensity, radius, offset, &mut state.light_color_slider, icon_font)
        }
        AssetComponent::Trigger { trigger_id, on_enter, on_exit } => {
            draw_trigger_editor(ctx, x, y, width, trigger_id, on_enter, on_exit, icon_font)
        }
        AssetComponent::Pickup { item_type, respawn_time } => {
            draw_pickup_editor(ctx, x, y, width, item_type, respawn_time, icon_font)
        }
        AssetComponent::Enemy { enemy_type, health, damage, patrol_radius } => {
            draw_enemy_editor(ctx, x, y, width, enemy_type, health, damage, patrol_radius, icon_font)
        }
        AssetComponent::Door { required_key, start_open } => {
            draw_door_editor(ctx, x, y, width, required_key, start_open, icon_font)
        }
        AssetComponent::Audio { sound, volume, radius, looping } => {
            draw_audio_editor(ctx, x, y, width, sound, volume, radius, looping, icon_font)
        }
        AssetComponent::Particle { effect, offset, .. } => {
            draw_particle_editor(ctx, x, y, width, effect, offset, icon_font)
        }
        AssetComponent::CharacterController { height, radius, step_height } => {
            draw_character_controller_editor(ctx, x, y, width, height, radius, step_height, icon_font)
        }
        AssetComponent::SpawnPoint { is_player, respawns } => {
            draw_spawn_point_editor(ctx, x, y, width, is_player, respawns, icon_font)
        }
        AssetComponent::Skeleton { bones: _ } => {
            // Skeleton editing handled separately via bone tree in left panel
            // TODO: Implement skeleton editor
            false
        }
    };

    // Apply changes back to the asset
    if modified {
        if let Some(comp) = state.asset.components.get_mut(comp_idx) {
            *comp = component;
        }
    }
}

/// Draw mesh component content (object list + per-object properties)
fn draw_mesh_editor_content(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, icon_font: Option<&Font>) {
    let line_height = 18.0;
    let mut y = rect.y;
    let x = rect.x;
    let width = rect.w;

    // --- OBJECT LIST ---
    // Collect click actions first (to avoid borrow issues)
    let mut select_idx: Option<usize> = None;
    let mut toggle_vis_idx: Option<usize> = None;
    let mut rename_idx: Option<usize> = None;
    let mut delete_idx: Option<usize> = None;

    let obj_count = state.objects().len();

    // Calculate how much space for object list (leave room for properties if object selected)
    let has_selection = state.selected_object.is_some();
    let props_height = if has_selection { 80.0 } else { 0.0 };
    let list_height = (rect.h - props_height - 4.0).max(60.0);

    for idx in 0..obj_count {
        if y + line_height > rect.y + list_height {
            break;
        }

        let obj = match state.objects().get(idx) {
            Some(o) => o,
            None => continue,
        };
        let is_selected = state.selected_object == Some(idx);
        let item_rect = Rect::new(x, y, width, line_height);
        let is_hovered = ctx.mouse.inside(&item_rect);

        // Selection highlight
        if is_selected {
            draw_rectangle(item_rect.x, item_rect.y, item_rect.w, item_rect.h, Color::from_rgba(60, 80, 100, 255));
        } else if is_hovered {
            draw_rectangle(item_rect.x, item_rect.y, item_rect.w, item_rect.h, Color::from_rgba(50, 50, 55, 255));
        }

        // Visibility toggle (eye icon)
        let vis_rect = Rect::new(x + 2.0, y + 1.0, 16.0, 16.0);
        let vis_icon = if obj.visible { icon::EYE } else { icon::EYE_OFF };
        let vis_color = if obj.visible { TEXT_COLOR } else { TEXT_DIM };
        draw_icon_centered(icon_font, vis_icon, &vis_rect, 11.0, vis_color);

        if ctx.mouse.inside(&vis_rect) && ctx.mouse.left_pressed {
            toggle_vis_idx = Some(idx);
        }

        // Rename and delete icons (show when selected or hovered)
        let show_icons = is_selected || is_hovered;
        let icon_size = 14.0;
        let delete_rect = Rect::new(rect.right() - icon_size - 4.0, y + 2.0, icon_size, icon_size);
        let rename_rect = Rect::new(delete_rect.x - icon_size - 4.0, y + 2.0, icon_size, icon_size);

        if show_icons {
            // Rename icon (pencil)
            let rename_hover = ctx.mouse.inside(&rename_rect);
            let rename_color = if rename_hover { ACCENT_COLOR } else { TEXT_DIM };
            draw_icon_centered(icon_font, icon::PENCIL, &rename_rect, 11.0, rename_color);
            if rename_hover && ctx.mouse.left_pressed {
                rename_idx = Some(idx);
            }

            // Delete icon (trash)
            let delete_hover = ctx.mouse.inside(&delete_rect);
            let delete_color = if delete_hover { Color::from_rgba(255, 100, 100, 255) } else { TEXT_DIM };
            draw_icon_centered(icon_font, icon::TRASH, &delete_rect, 11.0, delete_color);
            if delete_hover && ctx.mouse.left_pressed {
                delete_idx = Some(idx);
            }
        }

        // Object name with face count
        let fc = obj.mesh.face_count();
        let name_color = poly_count_color(fc);
        draw_text(&format!("{} ({})", obj.name, fc), x + 20.0, y + 13.0, FONT_SIZE_HEADER, name_color);

        // Handle selection click (not on visibility toggle or icons)
        let name_rect = Rect::new(x + 20.0, y, width - 60.0, line_height);
        if ctx.mouse.inside(&name_rect) && ctx.mouse.left_pressed
            && toggle_vis_idx.is_none() && rename_idx.is_none() && delete_idx.is_none() {
            select_idx = Some(idx);
        }

        y += line_height;
    }

    // Apply actions after the loop
    if let Some(idx) = toggle_vis_idx {
        if let Some(obj) = state.objects_mut().and_then(|v| v.get_mut(idx)) {
            obj.visible = !obj.visible;
        }
    } else if let Some(idx) = rename_idx {
        let name = state.objects().get(idx).map(|o| o.name.clone()).unwrap_or_default();
        state.rename_dialog = Some((idx, TextInputState::new(name)));
    } else if let Some(idx) = delete_idx {
        state.delete_dialog = Some(idx);
    } else if let Some(idx) = select_idx {
        state.select_object(idx);
    }

    // --- PER-OBJECT PROPERTIES ---
    if let Some(selected_idx) = state.selected_object {
        y += 4.0;

        // Separator line
        draw_rectangle(x + 4.0, y, width - 8.0, 1.0, Color::from_rgba(60, 60, 70, 255));
        y += 4.0;

        // Get object data (capture values to avoid borrow issues)
        let (obj_name, double_sided, mirror, bone_index) = match state.objects().get(selected_idx) {
            Some(obj) => (obj.name.clone(), obj.double_sided, obj.mirror, obj.default_bone_index),
            None => return,
        };

        // Object name header
        draw_text(&obj_name, x + 4.0, y + 12.0, FONT_SIZE_HEADER, ACCENT_COLOR);
        y += line_height;

        // Double-Sided Toggle
        let toggle_size = 16.0;
        let ds_rect = Rect::new(x + 4.0, y, toggle_size, toggle_size);
        let ds_icon = if double_sided { icon::SQUARE_CHECK } else { icon::SQUARE };
        let ds_color = if double_sided { ACCENT_COLOR } else { TEXT_DIM };
        draw_icon_centered(icon_font, ds_icon, &ds_rect, 12.0, ds_color);
        draw_text("Double-Sided", x + 24.0, y + 12.0, FONT_SIZE_CONTENT, TEXT_COLOR);

        if ctx.mouse.inside(&Rect::new(x, y, width, line_height)) && ctx.mouse.left_pressed {
            if let Some(obj) = state.objects_mut().and_then(|v| v.get_mut(selected_idx)) {
                obj.double_sided = !double_sided;
            }
            state.dirty = true;
        }
        y += line_height;

        // Mirror Toggle + Axis
        let mirror_enabled = mirror.map(|m| m.enabled).unwrap_or(false);
        let mirror_axis = mirror.map(|m| m.axis).unwrap_or(Axis::X);

        let mir_rect = Rect::new(x + 4.0, y, toggle_size, toggle_size);
        let mir_icon = if mirror_enabled { icon::SQUARE_CHECK } else { icon::SQUARE };
        let mir_color = if mirror_enabled { ACCENT_COLOR } else { TEXT_DIM };
        draw_icon_centered(icon_font, mir_icon, &mir_rect, 12.0, mir_color);
        draw_text("Mirror", x + 24.0, y + 12.0, FONT_SIZE_CONTENT, TEXT_COLOR);

        if ctx.mouse.inside(&Rect::new(x, y, 70.0, line_height)) && ctx.mouse.left_pressed {
            let new_enabled = !mirror_enabled;
            if let Some(obj) = state.objects_mut().and_then(|v| v.get_mut(selected_idx)) {
                if new_enabled {
                    obj.mirror = Some(MirrorSettings {
                        enabled: true,
                        axis: mirror_axis,
                        threshold: 1.0,
                    });
                } else if let Some(ref mut m) = obj.mirror {
                    m.enabled = false;
                }
            }
            state.dirty = true;
        }

        // Axis buttons (X, Y, Z) - shown on same line when enabled
        if mirror_enabled {
            let btn_w = 20.0;
            let btn_h = 16.0;
            let mut btn_x = x + 75.0;

            for axis in [Axis::X, Axis::Y, Axis::Z] {
                let is_active = mirror_axis == axis;
                let btn_rect = Rect::new(btn_x, y, btn_w, btn_h);
                let bg_color = if is_active {
                    Color::from_rgba(60, 100, 140, 255)
                } else if ctx.mouse.inside(&btn_rect) {
                    Color::from_rgba(60, 60, 70, 255)
                } else {
                    Color::from_rgba(45, 45, 55, 255)
                };
                draw_rectangle(btn_rect.x, btn_rect.y, btn_rect.w, btn_rect.h, bg_color);
                draw_text(axis.label(), btn_x + 6.0, y + 12.0, FONT_SIZE_CONTENT, TEXT_COLOR);

                if ctx.mouse.inside(&btn_rect) && ctx.mouse.left_pressed {
                    if let Some(obj) = state.objects_mut().and_then(|v| v.get_mut(selected_idx)) {
                        if let Some(ref mut m) = obj.mirror {
                            m.axis = axis;
                        }
                    }
                    state.dirty = true;
                }
                btn_x += btn_w + 2.0;
            }
        }

        y += line_height;

        // Bone Assignment (only if skeleton exists)
        let skeleton = state.skeleton();
        if !skeleton.is_empty() {
            draw_text("Bone", x + 4.0, y + 12.0, FONT_SIZE_CONTENT, TEXT_DIM);

            // Get current bone name
            let bone_name = bone_index
                .and_then(|idx| skeleton.get(idx))
                .map(|b| b.name.as_str())
                .unwrap_or("(None)");

            // Draw dropdown trigger for bone selection
            let selector_rect = Rect::new(x + 50.0, y, width - 54.0, line_height);
            if draw_dropdown_trigger(ctx, selector_rect, bone_name, icon_font) {
                state.bone_picker_target_mesh = Some(selected_idx);
                state.dropdown.toggle("bone_picker", selector_rect);
            }

            y += line_height;
        }
    }
}

/// Draw skeleton component content (bone tree + per-bone properties)
fn draw_skeleton_editor_content(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, icon_font: Option<&Font>) {
    let line_height = 18.0;
    let mut y = rect.y;
    let x = rect.x;
    let width = rect.w;

    // --- BONE TREE ---
    let skeleton = state.skeleton();
    if skeleton.is_empty() {
        draw_text("No bones", x + 4.0, y + 12.0, FONT_SIZE_CONTENT, TEXT_DIM);
        draw_text("Add Skeleton component", x + 4.0, y + 26.0, FONT_SIZE_CONTENT, TEXT_DIM);
        draw_text("to create root bone", x + 4.0, y + 40.0, FONT_SIZE_CONTENT, TEXT_DIM);
        return;
    }

    // Calculate how much space for bone list (leave room for properties if bone selected)
    let has_selection = state.selected_bone.is_some();
    let props_height = if has_selection { 80.0 } else { 0.0 };
    let list_height = (rect.h - props_height - 4.0).max(60.0);

    // Collect click actions
    let mut select_idx: Option<usize> = None;
    let mut delete_idx: Option<usize> = None;
    let mut add_idx: Option<usize> = None;
    let mut rename_idx: Option<usize> = None;

    // Draw root bones and their children recursively
    fn draw_bone_recursive(
        ctx: &mut UiContext,
        state: &ModelerState,
        bone_idx: usize,
        depth: usize,
        x: f32,
        y: &mut f32,
        width: f32,
        line_height: f32,
        list_height: f32,
        rect_y: f32,
        icon_font: Option<&Font>,
        select_idx: &mut Option<usize>,
        delete_idx: &mut Option<usize>,
        add_idx: &mut Option<usize>,
        rename_idx: &mut Option<usize>,
    ) {
        if *y + line_height > rect_y + list_height {
            return;
        }

        let skeleton = state.skeleton();
        let bone = match skeleton.get(bone_idx) {
            Some(b) => b,
            None => return,
        };

        let is_selected = state.selected_bone == Some(bone_idx);
        let is_hovered_bone = state.hovered_bone == Some(bone_idx);
        let indent = depth as f32 * 12.0;
        let item_rect = Rect::new(x, *y, width, line_height);
        let is_hovered = ctx.mouse.inside(&item_rect);

        // Selection/hover highlight
        if is_selected {
            draw_rectangle(item_rect.x, item_rect.y, item_rect.w, item_rect.h, Color::from_rgba(60, 80, 100, 255));
        } else if is_hovered || is_hovered_bone {
            draw_rectangle(item_rect.x, item_rect.y, item_rect.w, item_rect.h, Color::from_rgba(50, 50, 55, 255));
        }

        // Bone icon
        let icon_rect = Rect::new(x + 2.0 + indent, *y + 1.0, 16.0, 16.0);
        let icon_color = if bone.parent.is_none() {
            Color::from_rgba(255, 220, 100, 255) // Yellow for root
        } else if is_selected {
            Color::from_rgba(80, 255, 80, 255) // Green when selected
        } else {
            TEXT_COLOR
        };
        draw_icon_centered(icon_font, icon::BONE, &icon_rect, 11.0, icon_color);

        // Action icons (show when selected or hovered): Delete, Rename, Add Child
        let show_icons = is_selected || is_hovered;
        let icon_size = 14.0;
        let icon_spacing = 2.0;
        let mut icon_x = x + width - icon_size - 4.0;

        if show_icons {
            // Delete icon (rightmost)
            let delete_rect = Rect::new(icon_x, *y + 2.0, icon_size, icon_size);
            let delete_hover = ctx.mouse.inside(&delete_rect);
            let delete_color = if delete_hover { Color::from_rgba(255, 100, 100, 255) } else { TEXT_DIM };
            draw_icon_centered(icon_font, icon::TRASH, &delete_rect, 11.0, delete_color);
            if delete_hover && ctx.mouse.left_pressed {
                *delete_idx = Some(bone_idx);
            }
            icon_x -= icon_size + icon_spacing;

            // Rename icon
            let rename_rect = Rect::new(icon_x, *y + 2.0, icon_size, icon_size);
            let rename_hover = ctx.mouse.inside(&rename_rect);
            let rename_color = if rename_hover { ACCENT_COLOR } else { TEXT_DIM };
            draw_icon_centered(icon_font, icon::PENCIL, &rename_rect, 11.0, rename_color);
            if rename_hover && ctx.mouse.left_pressed {
                *rename_idx = Some(bone_idx);
            }
            icon_x -= icon_size + icon_spacing;

            // Add child icon
            let add_rect = Rect::new(icon_x, *y + 2.0, icon_size, icon_size);
            let add_hover = ctx.mouse.inside(&add_rect);
            let add_color = if add_hover { Color::from_rgba(100, 255, 100, 255) } else { TEXT_DIM };
            draw_icon_centered(icon_font, icon::PLUS, &add_rect, 11.0, add_color);
            if add_hover && ctx.mouse.left_pressed {
                *add_idx = Some(bone_idx);
            }
        }

        // Bone name
        let name_color = if is_selected { ACCENT_COLOR } else { TEXT_COLOR };
        draw_text(&bone.name, x + 20.0 + indent, *y + 13.0, FONT_SIZE_HEADER, name_color);

        // Handle selection click (not on action icons)
        let icons_width = if show_icons { (icon_size + icon_spacing) * 3.0 + 4.0 } else { 0.0 };
        let name_rect = Rect::new(x + 20.0 + indent, *y, width - 24.0 - indent - icons_width, line_height);
        if ctx.mouse.inside(&name_rect) && ctx.mouse.left_pressed {
            *select_idx = Some(bone_idx);
        }

        *y += line_height;

        // Draw children
        let children = state.bone_children(bone_idx);
        for child_idx in children {
            draw_bone_recursive(
                ctx, state, child_idx, depth + 1, x, y, width, line_height,
                list_height, rect_y, icon_font, select_idx, delete_idx, add_idx, rename_idx
            );
        }
    }

    // Draw root bones (no parent)
    let root_bones = state.root_bones();
    for root_idx in root_bones {
        draw_bone_recursive(
            ctx, state, root_idx, 0, x, &mut y, width, line_height,
            list_height, rect.y, icon_font, &mut select_idx, &mut delete_idx, &mut add_idx, &mut rename_idx
        );
    }

    // Apply actions after the loop
    if let Some(idx) = delete_idx {
        state.save_undo_skeleton("Delete Bone");
        state.remove_bone(idx);
        // Cancel rename mode if deleting
        state.bone_rename_active = false;
        state.bone_rename_buffer.clear();
    } else if let Some(idx) = add_idx {
        // Add child bone to this bone
        create_child_bone(state, idx);
    } else if let Some(idx) = rename_idx {
        // Start rename mode for this bone
        state.selected_bone = Some(idx); // Select the bone being renamed
        if let Some(bone) = state.skeleton().get(idx) {
            state.bone_rename_buffer = bone.name.clone();
            state.bone_rename_active = true;
        }
    } else if let Some(idx) = select_idx {
        // Cancel rename mode when selecting different bone
        if state.selected_bone != Some(idx) {
            state.bone_rename_active = false;
            state.bone_rename_buffer.clear();
        }
        state.selected_bone = Some(idx);
        if let Some(bone) = state.skeleton().get(idx) {
            state.set_status(&format!("Selected bone: {}", bone.name), 1.0);
        }
    }

    // --- PER-BONE PROPERTIES ---
    if let Some(selected_idx) = state.selected_bone {
        y += 4.0;

        // Separator line
        draw_rectangle(x + 4.0, y, width - 8.0, 1.0, Color::from_rgba(60, 60, 70, 255));
        y += 4.0;

        // Get bone data
        let (bone_name, parent_name, length, bone_width) = {
            let skeleton = state.skeleton();
            let bone = match skeleton.get(selected_idx) {
                Some(b) => b,
                None => return,
            };
            let parent_name = bone.parent
                .and_then(|p| skeleton.get(p))
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "(root)".to_string());
            (bone.name.clone(), parent_name, bone.length, bone.width)
        };

        // Bone name (editable if rename mode active)
        if state.bone_rename_active {
            // Draw text input for rename
            let input_rect = Rect::new(x + 4.0, y, width - 8.0, line_height);
            draw_rectangle(input_rect.x, input_rect.y, input_rect.w, input_rect.h, Color::from_rgba(40, 45, 55, 255));
            draw_rectangle_lines(input_rect.x, input_rect.y, input_rect.w, input_rect.h, 1.0, ACCENT_COLOR);

            // Handle text input
            while let Some(ch) = get_char_pressed() {
                if ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == ' ' {
                    state.bone_rename_buffer.push(ch);
                }
            }
            if is_key_pressed(KeyCode::Backspace) && !state.bone_rename_buffer.is_empty() {
                state.bone_rename_buffer.pop();
            }

            // Draw the text with cursor
            let display_text = format!("{}|", state.bone_rename_buffer);
            draw_text(&display_text, x + 6.0, y + 13.0, FONT_SIZE_HEADER, ACCENT_COLOR);

            // Handle Enter to confirm or Escape to cancel
            if is_key_pressed(KeyCode::Enter) {
                if !state.bone_rename_buffer.is_empty() {
                    state.save_undo_skeleton("Rename Bone");
                    if let Some(bones) = state.asset.skeleton_mut() {
                        if let Some(bone) = bones.get_mut(selected_idx) {
                            bone.name = state.bone_rename_buffer.clone();
                        }
                    }
                }
                state.bone_rename_active = false;
                state.bone_rename_buffer.clear();
            } else if is_key_pressed(KeyCode::Escape) {
                state.bone_rename_active = false;
                state.bone_rename_buffer.clear();
            }
        } else {
            draw_text(&bone_name, x + 4.0, y + 12.0, FONT_SIZE_HEADER, ACCENT_COLOR);
        }
        y += line_height;

        // Parent info
        draw_text(&format!("Parent: {}", parent_name), x + 4.0, y + 12.0, FONT_SIZE_CONTENT, TEXT_DIM);
        y += line_height;

        // Length info
        draw_text(&format!("Length: {:.0}", length), x + 4.0, y + 12.0, FONT_SIZE_CONTENT, TEXT_DIM);
        y += line_height;

        // Width slider (drag left/right to adjust)
        {
            let label = format!("Width: {:.0}", bone_width);
            let label_w = 55.0;
            draw_text(&label, x + 4.0, y + 12.0, FONT_SIZE_CONTENT, TEXT_COLOR);

            // Slider bar
            let slider_x = x + label_w + 4.0;
            let slider_w = width - label_w - 12.0;
            let slider_rect = Rect::new(slider_x, y + 2.0, slider_w, line_height - 4.0);
            let slider_hover = ctx.mouse.inside(&slider_rect);

            // Draw slider background
            let bg_color = if slider_hover { Color::from_rgba(50, 55, 65, 255) } else { Color::from_rgba(40, 42, 50, 255) };
            draw_rectangle(slider_rect.x, slider_rect.y, slider_rect.w, slider_rect.h, bg_color);

            // Draw filled portion (5..200 range)
            let fill_ratio = ((bone_width - 5.0) / 195.0).clamp(0.0, 1.0);
            let fill_w = slider_rect.w * fill_ratio;
            draw_rectangle(slider_rect.x, slider_rect.y, fill_w, slider_rect.h, Color::from_rgba(70, 90, 110, 255));

            // Click to set width directly
            if slider_hover && ctx.mouse.left_down {
                let ratio = ((ctx.mouse.x - slider_rect.x) / slider_rect.w).clamp(0.0, 1.0);
                let new_width = (5.0 + ratio * 195.0).round();
                if let Some(bones) = state.asset.skeleton_mut() {
                    if let Some(bone) = bones.get_mut(selected_idx) {
                        bone.width = new_width;
                    }
                }
            }
        }
        y += line_height;

        // Hint
        draw_text("Drag tip to rotate", x + 4.0, y + 12.0, FONT_SIZE_CONTENT, Color::from_rgba(100, 150, 200, 255));
        y += line_height;

        // Show meshes attached to this bone
        let attached_meshes: Vec<String> = state.objects()
            .iter()
            .filter(|obj| obj.default_bone_index == Some(selected_idx))
            .map(|obj| obj.name.clone())
            .collect();

        if !attached_meshes.is_empty() {
            y += 4.0;
            draw_text("Attached:", x + 4.0, y + 12.0, FONT_SIZE_CONTENT, TEXT_DIM);
            y += line_height;

            for name in attached_meshes {
                draw_text(&format!("• {}", name), x + 8.0, y + 12.0, FONT_SIZE_CONTENT, TEXT_COLOR);
                y += line_height;
            }
        }

        // Per-vertex bone assignment info
        let vertex_count = state.count_vertices_for_bone(selected_idx);
        if vertex_count > 0 {
            y += 4.0;
            draw_text(&format!("Vertices: {}", vertex_count), x + 4.0, y + 12.0, FONT_SIZE_CONTENT, TEXT_DIM);

            // "Select" button to select all vertices for this bone
            let btn_rect = Rect::new(x + 70.0, y, 50.0, line_height - 2.0);
            let btn_hover = ctx.mouse.inside(&btn_rect);
            let btn_color = if btn_hover { Color::from_rgba(80, 100, 120, 255) } else { Color::from_rgba(50, 60, 70, 255) };
            draw_rectangle(btn_rect.x, btn_rect.y, btn_rect.w, btn_rect.h, btn_color);
            draw_text("Select", btn_rect.x + 6.0, btn_rect.y + 12.0, FONT_SIZE_CONTENT, if btn_hover { ACCENT_COLOR } else { TEXT_COLOR });

            if btn_hover && ctx.mouse.left_pressed {
                state.select_vertices_for_bone(selected_idx);
            }
            y += line_height;
        }
    }
}

/// Create a child bone attached to the given parent bone
fn create_child_bone(state: &mut ModelerState, parent_idx: usize) {
    use super::state::RigBone;
    use crate::rasterizer::Vec3;

    const DEFAULT_LENGTH: f32 = 200.0;

    // Save undo before creating bone
    state.save_undo_skeleton("Create Bone");

    // Get parent bone info
    let (parent_length, parent_rotation, parent_width) = match state.skeleton().get(parent_idx) {
        Some(b) => (b.length, b.local_rotation, b.display_width()),
        None => return,
    };

    // Child is positioned at parent's tip in parent's local space
    let new_bone = RigBone {
        name: state.generate_bone_name(),
        parent: Some(parent_idx),
        local_position: Vec3::new(0.0, parent_length, 0.0),
        local_rotation: parent_rotation, // Inherit parent's rotation
        length: DEFAULT_LENGTH,
        width: parent_width,
    };

    let bone_name = new_bone.name.clone();
    if let Some(new_idx) = state.add_bone(new_bone) {
        state.selected_bone = Some(new_idx);
        state.selection = super::state::ModelerSelection::Bones(vec![new_idx]);
        state.set_status(&format!("Created child bone: {}", bone_name), 1.0);
    }
}

/// Ensure a Skeleton component exists, creating one if needed
fn ensure_skeleton_component(state: &mut ModelerState) {
    use super::state::RigBone;
    use crate::rasterizer::Vec3;

    // Check if skeleton component already exists
    let has_skeleton = state.asset.components.iter().any(|c| c.is_skeleton());
    if has_skeleton {
        return;
    }

    // Create default root bone at origin, pointing up (Y+)
    let root_bone = RigBone {
        name: "Root".to_string(),
        parent: None,
        local_position: Vec3::new(0.0, 0.0, 0.0),
        local_rotation: Vec3::ZERO,
        length: 200.0,
        width: RigBone::DEFAULT_WIDTH,
    };

    // Create and add skeleton component with default root bone
    let skeleton = crate::asset::AssetComponent::Skeleton {
        bones: vec![root_bone],
    };
    state.asset.components.push(skeleton);

    // Select the new skeleton component and root bone
    state.selected_component = Some(state.asset.components.len() - 1);
    state.selected_bone = Some(0);
    state.selection = super::state::ModelerSelection::Bones(vec![0]);
    state.dirty = true;
    state.set_status("Created skeleton with Root bone", 1.0);
}

/// Create a bone at a sensible default position (Tab key handler for skeleton)
fn create_bone_at_default_position(state: &mut ModelerState) {
    use super::state::RigBone;
    use crate::rasterizer::Vec3;

    // Default bone properties
    const DEFAULT_LENGTH: f32 = 200.0;

    // Determine position and parent based on current selection
    // Check bone selection first, then fall back to selected_bone
    let parent_from_selection = state.selection.bones()
        .and_then(|bones| bones.first().copied());
    let parent_idx = parent_from_selection.or(state.selected_bone);

    let (local_position, parent, local_rotation) = if let Some(selected_idx) = parent_idx {
        // Create child bone at selected bone's tip, pointing same direction
        // local_position is in parent's local space, so Y = parent.length puts us at the tip
        let (parent_length, parent_rotation) = state.skeleton().get(selected_idx)
            .map(|b| (b.length, b.local_rotation))
            .unwrap_or((DEFAULT_LENGTH, Vec3::ZERO));
        (Vec3::new(0.0, parent_length, 0.0), Some(selected_idx), parent_rotation)
    } else {
        // Create root bone at origin, pointing up (Y+)
        (Vec3::new(0.0, 0.0, 0.0), None, Vec3::ZERO)
    };

    let new_bone = RigBone {
        name: state.generate_bone_name(),
        parent,
        local_position,
        local_rotation,
        length: DEFAULT_LENGTH,
        width: RigBone::DEFAULT_WIDTH,
    };

    let bone_name = new_bone.name.clone();
    if let Some(new_idx) = state.add_bone(new_bone) {
        // Update both selection systems
        state.selected_bone = Some(new_idx);
        state.selection = super::state::ModelerSelection::Bones(vec![new_idx]);
        state.set_status(&format!("Created bone: {} (G to move, drag tip to rotate)", bone_name), 2.0);
    } else {
        state.set_status("Failed to create bone", 1.5);
    }
}

/// Draw collision component editor
fn draw_collision_editor(
    ctx: &mut UiContext,
    x: f32,
    y: &mut f32,
    width: f32,
    is_trigger: &mut bool,
    collision_mesh: &mut Option<String>,
    _icon_font: Option<&Font>,
) -> bool {
    let mut modified = false;
    let line_height = 20.0;

    // Show linked mesh name
    if let Some(mesh_name) = collision_mesh {
        draw_text("Mesh:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
        draw_text(mesh_name, x + 50.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_COLOR);
    } else {
        draw_text("Legacy shape (no mesh)", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
    }
    *y += line_height;

    // Hint: use Tab wheel for shape, G/R/S for editing
    draw_text("Tab: change shape", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
    *y += line_height;
    draw_text("G/R/S: edit mesh", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
    *y += line_height;

    // Is Trigger toggle
    draw_text("Is Trigger:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);

    let toggle_x = x + width - 40.0;
    let toggle_rect = Rect::new(toggle_x, *y + 2.0, 32.0, 14.0);
    let toggle_color = if *is_trigger { ACCENT_COLOR } else { Color::from_rgba(60, 60, 65, 255) };
    draw_rectangle(toggle_rect.x, toggle_rect.y, toggle_rect.w, toggle_rect.h, toggle_color);
    draw_text(if *is_trigger { "ON" } else { "OFF" }, toggle_x + 6.0, *y + 13.0, 11.0, TEXT_COLOR);

    if ctx.mouse.inside(&toggle_rect) && ctx.mouse.left_pressed {
        *is_trigger = !*is_trigger;
        modified = true;
    }
    *y += line_height;

    modified
}

/// Draw light component editor
fn draw_light_component_editor(
    ctx: &mut UiContext,
    x: f32,
    y: &mut f32,
    width: f32,
    color: &mut [u8; 3],
    intensity: &mut f32,
    radius: &mut f32,
    offset: &mut [f32; 3],
    color_slider: &mut Option<usize>,
    _icon_font: Option<&Font>,
) -> bool {
    let mut modified = false;
    let line_height = 18.0;
    let slider_height = 10.0;
    let slider_x = x + 14.0;
    let slider_width = width - 40.0;
    let track_bg = Color::new(0.12, 0.12, 0.14, 1.0);

    // Color preview
    draw_text("Color:", x + 4.0, *y + 12.0, FONT_SIZE_CONTENT, TEXT_DIM);
    let preview_rect = Rect::new(x + 50.0, *y + 2.0, 40.0, 14.0);
    draw_rectangle(preview_rect.x, preview_rect.y, preview_rect.w, preview_rect.h,
        Color::from_rgba(color[0], color[1], color[2], 255));
    *y += line_height;

    // RGB sliders (0-31 display, stored as 0-255) - matching texture editor style
    let channels = [
        ("R", color[0] / 8, Color::new(0.7, 0.3, 0.3, 1.0), 0usize),
        ("G", color[1] / 8, Color::new(0.3, 0.7, 0.3, 1.0), 1usize),
        ("B", color[2] / 8, Color::new(0.3, 0.3, 0.7, 1.0), 2usize),
    ];

    for (label, value, tint, slider_idx) in channels {
        let track_rect = Rect::new(slider_x, *y, slider_width, slider_height);

        // Label in channel color
        draw_text(label, x + 4.0, *y + 9.0, 12.0, tint);

        // Track background
        draw_rectangle(track_rect.x, track_rect.y, track_rect.w, track_rect.h, track_bg);

        // Filled portion
        let fill_ratio = value as f32 / 31.0;
        draw_rectangle(track_rect.x, track_rect.y, track_rect.w * fill_ratio, slider_height, tint);

        // Handle/thumb
        let handle_x = track_rect.x + track_rect.w * fill_ratio - 2.0;
        draw_rectangle(handle_x.max(track_rect.x), track_rect.y, 4.0, slider_height, WHITE);

        // Value text
        draw_text(&format!("{}", value), track_rect.x + track_rect.w + 4.0, *y + 9.0, 11.0, TEXT_DIM);

        // Slider interaction - start drag
        if ctx.mouse.inside(&track_rect) && ctx.mouse.left_down && color_slider.is_none() {
            *color_slider = Some(slider_idx);
        }

        // Continue drag (even outside track)
        if *color_slider == Some(slider_idx) {
            if ctx.mouse.left_down {
                let rel_x = (ctx.mouse.x - track_rect.x).clamp(0.0, track_rect.w);
                let new_val_31 = ((rel_x / track_rect.w) * 31.0).round() as u8;
                let new_val_255 = (new_val_31 as u16 * 8).min(255) as u8;
                if color[slider_idx] != new_val_255 {
                    color[slider_idx] = new_val_255;
                    modified = true;
                }
            } else {
                // Mouse released
                *color_slider = None;
            }
        }

        *y += slider_height + 4.0;
    }

    // Intensity slider
    draw_text("Intensity:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
    let slider_x = x + 70.0;
    let slider_w = width - 110.0;
    let slider_rect = Rect::new(slider_x, *y + 4.0, slider_w, 10.0);
    draw_rectangle(slider_rect.x, slider_rect.y, slider_rect.w, slider_rect.h, Color::from_rgba(40, 40, 45, 255));

    let max_intensity = 5.0;
    let fill_w = (intensity.clamp(0.0, max_intensity) / max_intensity) * slider_w;
    draw_rectangle(slider_rect.x, slider_rect.y, fill_w, slider_rect.h, ACCENT_COLOR);

    draw_text(&format!("{:.1}", intensity), x + width - 35.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_COLOR);

    if ctx.mouse.inside(&slider_rect) && ctx.mouse.left_down {
        let t = ((ctx.mouse.x - slider_rect.x) / slider_w).clamp(0.0, 1.0);
        *intensity = t * max_intensity;
        modified = true;
    }
    *y += line_height;

    // Radius slider
    draw_text("Radius:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
    let slider_rect = Rect::new(slider_x, *y + 4.0, slider_w, 10.0);
    draw_rectangle(slider_rect.x, slider_rect.y, slider_rect.w, slider_rect.h, Color::from_rgba(40, 40, 45, 255));

    let max_radius = 8192.0; // 8 meters
    let fill_w = (radius.clamp(0.0, max_radius) / max_radius) * slider_w;
    draw_rectangle(slider_rect.x, slider_rect.y, fill_w, slider_rect.h, ACCENT_COLOR);

    draw_text(&format!("{:.0}", radius), x + width - 35.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_COLOR);

    if ctx.mouse.inside(&slider_rect) && ctx.mouse.left_down {
        let t = ((ctx.mouse.x - slider_rect.x) / slider_w).clamp(0.0, 1.0);
        *radius = t * max_radius;
        modified = true;
    }
    *y += line_height;

    // Offset XYZ
    draw_text("Offset:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
    draw_text(&format!("X:{:.0} Y:{:.0} Z:{:.0}", offset[0], offset[1], offset[2]),
        x + 50.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_COLOR);
    *y += line_height;

    modified
}

/// Draw trigger component editor
fn draw_trigger_editor(
    _ctx: &mut UiContext,
    x: f32,
    y: &mut f32,
    _width: f32,
    trigger_id: &mut String,
    on_enter: &mut Option<String>,
    on_exit: &mut Option<String>,
    _icon_font: Option<&Font>,
) -> bool {
    let line_height = 20.0;

    draw_text("Trigger ID:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
    draw_text(trigger_id, x + 70.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_COLOR);
    *y += line_height;

    draw_text("On Enter:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
    draw_text(on_enter.as_deref().unwrap_or("(none)"), x + 70.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_COLOR);
    *y += line_height;

    draw_text("On Exit:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
    draw_text(on_exit.as_deref().unwrap_or("(none)"), x + 70.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_COLOR);
    *y += line_height;

    // TODO: Add text input for editing
    false
}

/// Draw pickup component editor
fn draw_pickup_editor(
    ctx: &mut UiContext,
    x: f32,
    y: &mut f32,
    width: f32,
    item_type: &mut crate::game::components::ItemType,
    respawn_time: &mut Option<f32>,
    _icon_font: Option<&Font>,
) -> bool {
    use crate::game::components::ItemType;
    let mut modified = false;
    let line_height = 20.0;

    // Item type
    draw_text("Type:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
    let type_name = match item_type {
        ItemType::HealthPickup { amount } => format!("Health ({})", amount),
        ItemType::Currency { amount } => format!("Currency ({})", amount),
        ItemType::Key(_) => "Key".to_string(),
        ItemType::Upgrade => "Upgrade".to_string(),
    };
    draw_text(&type_name, x + 50.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_COLOR);
    *y += line_height;

    // Item type buttons (simplified)
    let btn_w = (width - 12.0) / 4.0;
    let types = [
        ("Health", ItemType::HealthPickup { amount: 25 }),
        ("Currency", ItemType::Currency { amount: 10 }),
        ("Key", ItemType::Key(crate::game::components::KeyType::Generic(1))),
        ("Upgrade", ItemType::Upgrade),
    ];

    for (i, (name, new_type)) in types.iter().enumerate() {
        let btn_x = x + 4.0 + i as f32 * btn_w;
        let btn_rect = Rect::new(btn_x, *y, btn_w - 2.0, 18.0);
        let is_active = std::mem::discriminant(item_type) == std::mem::discriminant(new_type);
        let hovered = ctx.mouse.inside(&btn_rect);

        let bg = if is_active {
            ACCENT_COLOR
        } else if hovered {
            Color::from_rgba(60, 60, 70, 255)
        } else {
            Color::from_rgba(45, 45, 50, 255)
        };
        draw_rectangle(btn_rect.x, btn_rect.y, btn_rect.w, btn_rect.h, bg);

        let text_color = if is_active { Color::from_rgba(20, 20, 25, 255) } else { TEXT_COLOR };
        draw_text(name, btn_x + 2.0, *y + 13.0, 10.0, text_color);

        if hovered && ctx.mouse.left_pressed && !is_active {
            *item_type = new_type.clone();
            modified = true;
        }
    }
    *y += line_height;

    // Respawn time
    draw_text("Respawn:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
    let respawn_text = respawn_time.map(|t| format!("{:.0}s", t)).unwrap_or("Never".to_string());
    draw_text(&respawn_text, x + 60.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_COLOR);
    *y += line_height;

    modified
}

/// Draw enemy component editor
fn draw_enemy_editor(
    ctx: &mut UiContext,
    x: f32,
    y: &mut f32,
    width: f32,
    enemy_type: &mut crate::game::components::EnemyType,
    health: &mut i32,
    damage: &mut i32,
    patrol_radius: &mut f32,
    _icon_font: Option<&Font>,
) -> bool {
    use crate::game::components::EnemyType;
    let mut modified = false;
    let line_height = 20.0;

    // Enemy type
    draw_text("Type:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
    let type_name = match enemy_type {
        EnemyType::Grunt => "Grunt",
        EnemyType::Archer => "Archer",
        EnemyType::Heavy => "Heavy",
        EnemyType::Swarm => "Swarm",
        EnemyType::Elite => "Elite",
        EnemyType::Boss => "Boss",
    };
    draw_text(type_name, x + 50.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_COLOR);
    *y += line_height;

    // Enemy type buttons (first row)
    let btn_w = (width - 12.0) / 3.0;
    let types_row1 = [
        ("Grunt", EnemyType::Grunt),
        ("Archer", EnemyType::Archer),
        ("Heavy", EnemyType::Heavy),
    ];

    for (i, (name, new_type)) in types_row1.iter().enumerate() {
        let btn_x = x + 4.0 + i as f32 * btn_w;
        let btn_rect = Rect::new(btn_x, *y, btn_w - 2.0, 18.0);
        let is_active = enemy_type == new_type;
        let hovered = ctx.mouse.inside(&btn_rect);

        let bg = if is_active {
            ACCENT_COLOR
        } else if hovered {
            Color::from_rgba(60, 60, 70, 255)
        } else {
            Color::from_rgba(45, 45, 50, 255)
        };
        draw_rectangle(btn_rect.x, btn_rect.y, btn_rect.w, btn_rect.h, bg);

        let text_color = if is_active { Color::from_rgba(20, 20, 25, 255) } else { TEXT_COLOR };
        draw_text(name, btn_x + 4.0, *y + 13.0, 11.0, text_color);

        if hovered && ctx.mouse.left_pressed && !is_active {
            *enemy_type = *new_type;
            modified = true;
        }
    }
    *y += line_height;

    // Enemy type buttons (second row)
    let types_row2 = [
        ("Swarm", EnemyType::Swarm),
        ("Elite", EnemyType::Elite),
        ("Boss", EnemyType::Boss),
    ];

    for (i, (name, new_type)) in types_row2.iter().enumerate() {
        let btn_x = x + 4.0 + i as f32 * btn_w;
        let btn_rect = Rect::new(btn_x, *y, btn_w - 2.0, 18.0);
        let is_active = enemy_type == new_type;
        let hovered = ctx.mouse.inside(&btn_rect);

        let bg = if is_active {
            ACCENT_COLOR
        } else if hovered {
            Color::from_rgba(60, 60, 70, 255)
        } else {
            Color::from_rgba(45, 45, 50, 255)
        };
        draw_rectangle(btn_rect.x, btn_rect.y, btn_rect.w, btn_rect.h, bg);

        let text_color = if is_active { Color::from_rgba(20, 20, 25, 255) } else { TEXT_COLOR };
        draw_text(name, btn_x + 4.0, *y + 13.0, 11.0, text_color);

        if hovered && ctx.mouse.left_pressed && !is_active {
            *enemy_type = *new_type;
            modified = true;
        }
    }
    *y += line_height;

    // Health
    draw_text("Health:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
    draw_text(&format!("{}", health), x + 60.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_COLOR);
    *y += line_height;

    // Damage
    draw_text("Damage:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
    draw_text(&format!("{}", damage), x + 60.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_COLOR);
    *y += line_height;

    // Patrol radius
    draw_text("Patrol:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
    draw_text(&format!("{:.0}", patrol_radius), x + 60.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_COLOR);
    *y += line_height;

    modified
}

/// Draw door component editor
fn draw_door_editor(
    ctx: &mut UiContext,
    x: f32,
    y: &mut f32,
    width: f32,
    required_key: &mut Option<String>,
    start_open: &mut bool,
    _icon_font: Option<&Font>,
) -> bool {
    let mut modified = false;
    let line_height = 20.0;

    // Required key
    draw_text("Key:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
    let key_text = required_key.as_deref().unwrap_or("(unlocked)");
    draw_text(key_text, x + 40.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_COLOR);
    *y += line_height;

    // Start open toggle
    draw_text("Start Open:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);

    let toggle_x = x + width - 40.0;
    let toggle_rect = Rect::new(toggle_x, *y + 2.0, 32.0, 14.0);
    let toggle_color = if *start_open { ACCENT_COLOR } else { Color::from_rgba(60, 60, 65, 255) };
    draw_rectangle(toggle_rect.x, toggle_rect.y, toggle_rect.w, toggle_rect.h, toggle_color);
    draw_text(if *start_open { "ON" } else { "OFF" }, toggle_x + 6.0, *y + 13.0, 11.0, TEXT_COLOR);

    if ctx.mouse.inside(&toggle_rect) && ctx.mouse.left_pressed {
        *start_open = !*start_open;
        modified = true;
    }
    *y += line_height;

    modified
}

/// Draw audio component editor
fn draw_audio_editor(
    ctx: &mut UiContext,
    x: f32,
    y: &mut f32,
    width: f32,
    sound: &mut String,
    volume: &mut f32,
    radius: &mut f32,
    looping: &mut bool,
    _icon_font: Option<&Font>,
) -> bool {
    let mut modified = false;
    let line_height = 20.0;

    // Sound name
    draw_text("Sound:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
    draw_text(sound, x + 50.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_COLOR);
    *y += line_height;

    // Volume slider
    draw_text("Volume:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
    let slider_x = x + 60.0;
    let slider_w = width - 100.0;
    let slider_rect = Rect::new(slider_x, *y + 4.0, slider_w, 10.0);
    draw_rectangle(slider_rect.x, slider_rect.y, slider_rect.w, slider_rect.h, Color::from_rgba(40, 40, 45, 255));

    let fill_w = volume.clamp(0.0, 1.0) * slider_w;
    draw_rectangle(slider_rect.x, slider_rect.y, fill_w, slider_rect.h, ACCENT_COLOR);

    draw_text(&format!("{:.0}%", *volume * 100.0), x + width - 35.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_COLOR);

    if ctx.mouse.inside(&slider_rect) && ctx.mouse.left_down {
        let t = ((ctx.mouse.x - slider_rect.x) / slider_w).clamp(0.0, 1.0);
        *volume = t;
        modified = true;
    }
    *y += line_height;

    // Radius slider
    draw_text("Radius:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
    let slider_rect = Rect::new(slider_x, *y + 4.0, slider_w, 10.0);
    draw_rectangle(slider_rect.x, slider_rect.y, slider_rect.w, slider_rect.h, Color::from_rgba(40, 40, 45, 255));

    let max_radius = 8192.0; // 8 meters
    let fill_w = (radius.clamp(0.0, max_radius) / max_radius) * slider_w;
    draw_rectangle(slider_rect.x, slider_rect.y, fill_w, slider_rect.h, ACCENT_COLOR);

    draw_text(&format!("{:.0}", radius), x + width - 35.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_COLOR);

    if ctx.mouse.inside(&slider_rect) && ctx.mouse.left_down {
        let t = ((ctx.mouse.x - slider_rect.x) / slider_w).clamp(0.0, 1.0);
        *radius = t * max_radius;
        modified = true;
    }
    *y += line_height;

    // Looping toggle
    draw_text("Looping:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);

    let toggle_x = x + width - 40.0;
    let toggle_rect = Rect::new(toggle_x, *y + 2.0, 32.0, 14.0);
    let toggle_color = if *looping { ACCENT_COLOR } else { Color::from_rgba(60, 60, 65, 255) };
    draw_rectangle(toggle_rect.x, toggle_rect.y, toggle_rect.w, toggle_rect.h, toggle_color);
    draw_text(if *looping { "ON" } else { "OFF" }, toggle_x + 6.0, *y + 13.0, 11.0, TEXT_COLOR);

    if ctx.mouse.inside(&toggle_rect) && ctx.mouse.left_pressed {
        *looping = !*looping;
        modified = true;
    }
    *y += line_height;

    modified
}

/// Draw particle component editor
fn draw_particle_editor(
    ctx: &mut UiContext,
    x: f32,
    y: &mut f32,
    width: f32,
    effect: &mut String,
    offset: &mut [f32; 3],
    _icon_font: Option<&Font>,
) -> bool {
    let line_height = 20.0;
    let mut modified = false;

    // Preset buttons
    draw_text("Preset:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
    *y += line_height;

    let presets = ["fire", "sparks", "dust", "blood"];
    let btn_w = (width - 12.0) / presets.len() as f32;
    for (i, preset_name) in presets.iter().enumerate() {
        let btn_x = x + 4.0 + i as f32 * btn_w;
        let btn_rect = Rect::new(btn_x, *y, btn_w - 2.0, 18.0);
        let is_active = effect.as_str() == *preset_name;
        let hovered = ctx.mouse.inside(&btn_rect);

        let bg = if is_active {
            ACCENT_COLOR
        } else if hovered {
            Color::from_rgba(60, 60, 70, 255)
        } else {
            Color::from_rgba(45, 45, 50, 255)
        };
        draw_rectangle(btn_rect.x, btn_rect.y, btn_rect.w, btn_rect.h, bg);

        let text_color = if is_active { Color::from_rgba(20, 20, 25, 255) } else { TEXT_COLOR };
        draw_text(preset_name, btn_x + 4.0, *y + 13.0, 11.0, text_color);

        if hovered && ctx.mouse.left_pressed && !is_active {
            *effect = preset_name.to_string();
            modified = true;
        }
    }
    *y += line_height;

    // Offset sliders
    let slider_x = x + 70.0;
    let slider_w = width - 110.0;
    let slider_h = 10.0;
    let track_bg = Color::from_rgba(40, 40, 45, 255);
    let max_offset = 512.0;

    let labels = ["Offset X:", "Offset Y:", "Offset Z:"];
    for (i, label) in labels.iter().enumerate() {
        draw_text(label, x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
        let sr = Rect::new(slider_x, *y + 4.0, slider_w, slider_h);
        draw_rectangle(sr.x, sr.y, sr.w, sr.h, track_bg);
        // Map [-max_offset, max_offset] to [0, 1]
        let norm = (offset[i] + max_offset) / (2.0 * max_offset);
        let fill = norm.clamp(0.0, 1.0) * slider_w;
        draw_rectangle(sr.x, sr.y, fill, sr.h, ACCENT_COLOR);
        draw_text(&format!("{:.0}", offset[i]), x + width - 35.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_COLOR);
        if ctx.mouse.inside(&sr) && ctx.mouse.left_down {
            let t = ((ctx.mouse.x - sr.x) / slider_w).clamp(0.0, 1.0);
            offset[i] = t * 2.0 * max_offset - max_offset;
            modified = true;
        }
        *y += line_height;
    }

    modified
}

/// Draw character controller component editor
fn draw_character_controller_editor(
    ctx: &mut UiContext,
    x: f32,
    y: &mut f32,
    width: f32,
    height: &mut f32,
    radius: &mut f32,
    step_height: &mut f32,
    _icon_font: Option<&Font>,
) -> bool {
    let mut modified = false;
    let line_height = 20.0;
    let slider_x = x + 70.0;
    let slider_w = width - 110.0;
    let max_val = 3072.0;

    // Height slider
    draw_text("Height:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
    let slider_rect = Rect::new(slider_x, *y + 4.0, slider_w, 10.0);
    draw_rectangle(slider_rect.x, slider_rect.y, slider_rect.w, slider_rect.h, Color::from_rgba(40, 40, 45, 255));

    let fill_w = (height.clamp(0.0, max_val) / max_val) * slider_w;
    draw_rectangle(slider_rect.x, slider_rect.y, fill_w, slider_rect.h, ACCENT_COLOR);

    draw_text(&format!("{:.0}", height), x + width - 35.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_COLOR);

    if ctx.mouse.inside(&slider_rect) && ctx.mouse.left_down {
        let t = ((ctx.mouse.x - slider_rect.x) / slider_w).clamp(0.0, 1.0);
        *height = t * max_val;
        modified = true;
    }
    *y += line_height;

    // Radius slider
    draw_text("Radius:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
    let slider_rect = Rect::new(slider_x, *y + 4.0, slider_w, 10.0);
    draw_rectangle(slider_rect.x, slider_rect.y, slider_rect.w, slider_rect.h, Color::from_rgba(40, 40, 45, 255));

    let fill_w = (radius.clamp(0.0, max_val) / max_val) * slider_w;
    draw_rectangle(slider_rect.x, slider_rect.y, fill_w, slider_rect.h, ACCENT_COLOR);

    draw_text(&format!("{:.0}", radius), x + width - 35.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_COLOR);

    if ctx.mouse.inside(&slider_rect) && ctx.mouse.left_down {
        let t = ((ctx.mouse.x - slider_rect.x) / slider_w).clamp(0.0, 1.0);
        *radius = t * max_val;
        modified = true;
    }
    *y += line_height;

    // Step height slider
    draw_text("Step:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
    let slider_rect = Rect::new(slider_x, *y + 4.0, slider_w, 10.0);
    draw_rectangle(slider_rect.x, slider_rect.y, slider_rect.w, slider_rect.h, Color::from_rgba(40, 40, 45, 255));

    let max_step = 1024.0;
    let fill_w = (step_height.clamp(0.0, max_step) / max_step) * slider_w;
    draw_rectangle(slider_rect.x, slider_rect.y, fill_w, slider_rect.h, ACCENT_COLOR);

    draw_text(&format!("{:.0}", step_height), x + width - 35.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_COLOR);

    if ctx.mouse.inside(&slider_rect) && ctx.mouse.left_down {
        let t = ((ctx.mouse.x - slider_rect.x) / slider_w).clamp(0.0, 1.0);
        *step_height = t * max_step;
        modified = true;
    }
    *y += line_height;

    modified
}

/// Draw spawn point component editor
fn draw_spawn_point_editor(
    ctx: &mut UiContext,
    x: f32,
    y: &mut f32,
    width: f32,
    is_player: &mut bool,
    respawns: &mut bool,
    _icon_font: Option<&Font>,
) -> bool {
    let mut modified = false;
    let line_height = 20.0;
    let toggle_x = x + width - 40.0;

    // Is player toggle
    draw_text("Player Start:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
    let toggle_rect = Rect::new(toggle_x, *y + 2.0, 32.0, 14.0);
    let toggle_color = if *is_player { ACCENT_COLOR } else { Color::from_rgba(60, 60, 65, 255) };
    draw_rectangle(toggle_rect.x, toggle_rect.y, toggle_rect.w, toggle_rect.h, toggle_color);
    draw_text(if *is_player { "ON" } else { "OFF" }, toggle_x + 6.0, *y + 13.0, 11.0, TEXT_COLOR);
    if ctx.mouse.inside(&toggle_rect) && ctx.mouse.left_pressed {
        *is_player = !*is_player;
        modified = true;
    }
    *y += line_height;

    // Respawns toggle
    draw_text("Respawns:", x + 4.0, *y + 14.0, FONT_SIZE_CONTENT, TEXT_DIM);
    let respawn_rect = Rect::new(toggle_x, *y + 2.0, 32.0, 14.0);
    let respawn_color = if *respawns { ACCENT_COLOR } else { Color::from_rgba(60, 60, 65, 255) };
    draw_rectangle(respawn_rect.x, respawn_rect.y, respawn_rect.w, respawn_rect.h, respawn_color);
    draw_text(if *respawns { "ON" } else { "OFF" }, toggle_x + 6.0, *y + 13.0, 11.0, TEXT_COLOR);
    if ctx.mouse.inside(&respawn_rect) && ctx.mouse.left_pressed {
        *respawns = !*respawns;
        modified = true;
    }
    *y += line_height;

    modified
}

/// Draw lights section - simple ambient slider (Light components add point lights on top)
fn draw_lights_section(ctx: &mut UiContext, x: f32, y: &mut f32, width: f32, state: &mut ModelerState, _icon_font: Option<&Font>) {
    let slider_height = 12.0;
    let label_width = 55.0;
    let value_width = 24.0;
    let slider_x = x + label_width;
    let slider_width = width - label_width - value_width - 12.0;

    let text_color = Color::from_rgba(204, 204, 204, 255);
    let track_bg = Color::from_rgba(38, 38, 46, 255);
    let tint = Color::from_rgba(230, 217, 102, 255); // Yellow/warm for light

    // Label
    draw_text("Ambient", x, *y + slider_height - 2.0, 11.0, text_color);

    // Convert ambient (0.0-1.0) to display value (0-31)
    let ambient = state.raster_settings.ambient;
    let ambient_31 = (ambient * 31.0).round() as u8;

    // Slider track background
    let track_rect = Rect::new(slider_x, *y, slider_width, slider_height);
    draw_rectangle(track_rect.x, track_rect.y, track_rect.w, track_rect.h, track_bg);

    // Filled portion
    let fill_ratio = ambient_31 as f32 / 31.0;
    let fill_width = fill_ratio * slider_width;
    draw_rectangle(track_rect.x, track_rect.y, fill_width, track_rect.h, tint);

    // Thumb indicator
    let thumb_x = track_rect.x + fill_width - 1.0;
    draw_rectangle(thumb_x, track_rect.y, 3.0, track_rect.h, WHITE);

    // Value text
    draw_text(&format!("{:2}", ambient_31), slider_x + slider_width + 4.0, *y + slider_height - 2.0, 11.0, text_color);

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
        if (state.raster_settings.ambient - new_ambient).abs() > 0.001 {
            state.raster_settings.ambient = new_ambient;
        }
    }

    // End dragging
    if state.ambient_slider_active && !ctx.mouse.left_down {
        state.ambient_slider_active = false;
    }

    *y += slider_height + 8.0;
}

// ============================================================================
// Right Panel (Atlas + UV Tools + Paint Tools + CLUT)
// ============================================================================

fn draw_right_panel(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, icon_font: Option<&Font>, storage: &Storage) {
    let mut y = rect.y + 4.0;

    // === UNIFIED TEXTURE EDITOR (collapsible) ===
    // Combines Paint + UV modes with tab-based switching
    let is_focused = state.active_panel == super::state::ActivePanel::TextureEditor;
    let (editor_expanded, header_clicked) = draw_collapsible_header(
        ctx, rect.x, &mut y, rect.w, "Texture",
        state.paint_section_expanded, is_focused, icon_font
    );

    // Set focus when clicking on the header
    if header_clicked {
        state.active_panel = super::state::ActivePanel::TextureEditor;
    }

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

        // Set focus when clicking anywhere in the texture editor content
        if ctx.mouse.inside(&editor_rect) && ctx.mouse.left_pressed {
            state.active_panel = super::state::ActivePanel::TextureEditor;
        }

        draw_paint_section(ctx, editor_rect, state, icon_font, storage);
    }
}

/// Draw a collapsible section header, returns (new_expanded_state, was_clicked)
/// Matches World Editor style: 20px header, 16pt font, triangle indicators
fn draw_collapsible_header(
    ctx: &mut UiContext,
    x: f32,
    y: &mut f32,
    width: f32,
    label: &str,
    expanded: bool,
    focused: bool,
    _icon_font: Option<&Font>,
) -> (bool, bool) {
    let header_h = 20.0;  // Match World Editor
    let header_rect = Rect::new(x, *y, width, header_h);
    let hovered = ctx.mouse.inside(&header_rect);

    // Background (matching World Editor colors)
    let bg = if hovered {
        Color::from_rgba(60, 60, 70, 255)
    } else {
        Color::from_rgba(50, 50, 60, 255)
    };
    draw_rectangle(header_rect.x, header_rect.y, header_rect.w, header_rect.h, bg);

    // Text/indicator color - cyan when focused
    let text_color = if focused { ACCENT_COLOR } else { WHITE };
    let indicator_color = if focused { ACCENT_COLOR } else { Color::from_rgba(180, 180, 180, 255) };

    // Triangle indicator (matching World Editor style)
    let indicator_x = x + 6.0;
    let indicator_y = *y + 10.0;
    let indicator_size = 5.0;

    if expanded {
        // Down-pointing triangle (expanded)
        draw_triangle(
            macroquad::math::Vec2::new(indicator_x - 2.0, indicator_y - 3.0),
            macroquad::math::Vec2::new(indicator_x + indicator_size + 2.0, indicator_y - 3.0),
            macroquad::math::Vec2::new(indicator_x + indicator_size / 2.0, indicator_y + 4.0),
            indicator_color,
        );
    } else {
        // Right-pointing triangle (collapsed)
        draw_triangle(
            macroquad::math::Vec2::new(indicator_x, indicator_y - indicator_size),
            macroquad::math::Vec2::new(indicator_x, indicator_y + indicator_size),
            macroquad::math::Vec2::new(indicator_x + indicator_size, indicator_y),
            indicator_color,
        );
    }

    // Label (font size 16 to match World Editor)
    draw_text(label, x + 16.0, *y + 14.0, 16.0, text_color);

    // Border (matching World Editor)
    draw_rectangle_lines(header_rect.x, header_rect.y, header_rect.w, header_rect.h, 1.0, Color::from_rgba(80, 80, 80, 255));

    *y += header_h;

    // Handle click
    let clicked = hovered && ctx.mouse.left_pressed;
    let new_expanded = if clicked { !expanded } else { expanded };
    (new_expanded, clicked)
}

/// Create a UserTexture for editing from the selected object's IndexedAtlas
fn create_editing_texture(state: &ModelerState) -> UserTexture {
    let indexed = state.atlas();
    // Get CLUT for palette colors
    let clut = state.clut_pool.get(indexed.default_clut)
        .or_else(|| state.clut_pool.iter().next())
        .cloned()
        .unwrap_or_else(|| Clut::new_4bit("default".to_string()));

    UserTexture {
        id: generate_texture_id(),
        name: "atlas".to_string(),
        width: indexed.width,
        height: indexed.height,
        depth: indexed.depth,
        indices: indexed.indices.clone(),
        palette: clut.colors.clone(),
        blend_mode: crate::rasterizer::BlendMode::Opaque,
        source: crate::texture::TextureSource::User,
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
fn draw_paint_section(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, icon_font: Option<&Font>, storage: &Storage) {
    // If editing a texture, show the texture editor
    if state.editing_indexed_atlas {
        draw_paint_texture_editor(ctx, rect, state, icon_font, storage);
    } else {
        // Show the texture browser with New/Edit buttons
        draw_paint_texture_browser(ctx, rect, state, icon_font);
    }

    // Draw import dialog (modal overlay) if active
    if let Some(action) = draw_import_dialog(ctx, &mut state.texture_editor.import_state, icon_font) {
        match action {
            ImportAction::Confirm => {
                // Create a new texture from the import state
                let import = &state.texture_editor.import_state;
                let name = state.user_textures.next_available_name();
                let (size_w, _) = import.target_size.dimensions();
                let texture = UserTexture::new_with_data(
                    name.clone(),
                    import.target_size,
                    import.depth,
                    import.preview_indices.clone(),
                    import.preview_palette.clone(),
                );
                state.user_textures.add(texture);
                // Save via storage (native only)
                #[cfg(not(target_arch = "wasm32"))]
                if let Err(e) = state.user_textures.save_texture_with_storage(&name, storage) {
                    eprintln!("Failed to save imported texture: {}", e);
                }
                state.set_status(&format!("Imported '{}' ({}x{})", name, size_w, size_w), 2.0);
                state.texture_editor.import_state.reset();
            }
            ImportAction::Cancel => {
                // Just reset the import state (already done by the dialog)
            }
        }
    }

    // Draw delete texture confirmation dialog (modal overlay) if active
    if let Some(action) = draw_delete_texture_dialog(ctx, state, icon_font) {
        match action {
            DeleteTextureAction::Confirm => {
                if let Some(name) = state.texture_pending_delete.take() {
                    // Delete the texture file and remove from library
                    match state.user_textures.delete_texture_with_storage(&name, storage) {
                        Ok(()) => {
                            state.set_status(&format!("Deleted '{}'", name), 2.0);
                            // Clear selection if we deleted the selected texture
                            if state.selected_user_texture.as_ref() == Some(&name) {
                                state.selected_user_texture = None;
                            }
                        }
                        Err(e) => {
                            state.set_status(&format!("Delete failed: {}", e), 3.0);
                        }
                    }
                }
            }
            DeleteTextureAction::Cancel => {
                state.texture_pending_delete = None;
            }
        }
    }

    // Draw unsaved texture changes dialog (modal overlay) if active
    if let Some(action) = draw_unsaved_texture_dialog(ctx, state, icon_font) {
        match action {
            UnsavedTextureAction::Save => {
                // Save the texture first, then switch
                if let Some(ref editing_tex) = state.editing_texture {
                    let tex_name = editing_tex.name.clone();
                    // Sync editing_texture to user_textures library
                    if let Some(lib_tex) = state.user_textures.get_mut(&tex_name) {
                        lib_tex.indices = editing_tex.indices.clone();
                        lib_tex.palette = editing_tex.palette.clone();
                        lib_tex.depth = editing_tex.depth;
                        lib_tex.width = editing_tex.width;
                        lib_tex.height = editing_tex.height;
                    }
                    // Save to storage
                    if let Err(e) = state.user_textures.save_texture_with_storage(&tex_name, storage) {
                        state.set_status(&format!("Failed to save: {}", e), 3.0);
                    } else {
                        let cloud_text = if storage.has_cloud() { " to cloud" } else { "" };
                        state.set_status(&format!("Saved '{}'{}", tex_name, cloud_text), 2.0);
                    }
                }
                // Now switch to pending object
                if let Some(pending_idx) = state.unsaved_texture_pending_switch {
                    state.force_select_object(pending_idx);
                }
            }
            UnsavedTextureAction::Discard => {
                // Discard changes and switch
                if let Some(pending_idx) = state.unsaved_texture_pending_switch {
                    state.force_select_object(pending_idx);
                }
            }
            UnsavedTextureAction::Cancel => {
                // Stay on current object, clear pending switch
                state.unsaved_texture_pending_switch = None;
            }
        }
    }
}

/// Action from delete texture confirmation dialog
enum DeleteTextureAction {
    Confirm,
    Cancel,
}

/// Draw the delete texture confirmation dialog (modal overlay)
fn draw_delete_texture_dialog(
    ctx: &mut UiContext,
    state: &ModelerState,
    _icon_font: Option<&Font>,
) -> Option<DeleteTextureAction> {
    let texture_name = state.texture_pending_delete.as_ref()?;

    // Dark overlay
    draw_rectangle(0.0, 0.0, screen_width(), screen_height(), Color::new(0.0, 0.0, 0.0, 0.6));

    // Dialog dimensions
    let dialog_w = 300.0;
    let dialog_h = 120.0;
    let dialog_x = (screen_width() - dialog_w) / 2.0;
    let dialog_y = (screen_height() - dialog_h) / 2.0;

    // Dialog background
    draw_rectangle(dialog_x, dialog_y, dialog_w, dialog_h, Color::from_rgba(45, 45, 55, 255));
    draw_rectangle_lines(dialog_x, dialog_y, dialog_w, dialog_h, 2.0, Color::from_rgba(80, 80, 90, 255));

    // Title
    draw_rectangle(dialog_x, dialog_y, dialog_w, 24.0, Color::from_rgba(60, 45, 45, 255));
    draw_text("Delete Texture", dialog_x + 8.0, dialog_y + 17.0, 16.0, WHITE);

    // Message
    let msg = format!("Delete '{}'?", texture_name);
    let msg_dims = measure_text(&msg, None, 14, 1.0);
    draw_text(&msg, dialog_x + (dialog_w - msg_dims.width) / 2.0, dialog_y + 55.0, 14.0, WHITE);
    draw_text("This cannot be undone.", dialog_x + (dialog_w - measure_text("This cannot be undone.", None, 12, 1.0).width) / 2.0, dialog_y + 75.0, 12.0, Color::from_rgba(180, 150, 150, 255));

    // Buttons
    let btn_w = 80.0;
    let btn_h = 28.0;
    let btn_y = dialog_y + dialog_h - btn_h - 10.0;
    let btn_spacing = 20.0;
    let total_btn_w = btn_w * 2.0 + btn_spacing;
    let btn_start_x = dialog_x + (dialog_w - total_btn_w) / 2.0;

    // Cancel button
    let cancel_rect = Rect::new(btn_start_x, btn_y, btn_w, btn_h);
    let cancel_hovered = ctx.mouse.inside(&cancel_rect);
    let cancel_bg = if cancel_hovered { Color::from_rgba(70, 70, 80, 255) } else { Color::from_rgba(55, 55, 65, 255) };
    draw_rectangle(cancel_rect.x, cancel_rect.y, cancel_rect.w, cancel_rect.h, cancel_bg);
    draw_rectangle_lines(cancel_rect.x, cancel_rect.y, cancel_rect.w, cancel_rect.h, 1.0, Color::from_rgba(80, 80, 90, 255));
    let cancel_text = "Cancel";
    let cancel_dims = measure_text(cancel_text, None, 14, 1.0);
    draw_text(cancel_text, cancel_rect.x + (cancel_rect.w - cancel_dims.width) / 2.0, cancel_rect.y + cancel_rect.h / 2.0 + 5.0, 14.0, if cancel_hovered { WHITE } else { Color::from_rgba(200, 200, 200, 255) });

    if ctx.mouse.clicked(&cancel_rect) {
        return Some(DeleteTextureAction::Cancel);
    }

    // Delete button
    let delete_rect = Rect::new(btn_start_x + btn_w + btn_spacing, btn_y, btn_w, btn_h);
    let delete_hovered = ctx.mouse.inside(&delete_rect);
    let delete_bg = if delete_hovered { Color::from_rgba(150, 60, 60, 255) } else { Color::from_rgba(120, 50, 50, 255) };
    draw_rectangle(delete_rect.x, delete_rect.y, delete_rect.w, delete_rect.h, delete_bg);
    draw_rectangle_lines(delete_rect.x, delete_rect.y, delete_rect.w, delete_rect.h, 1.0, Color::from_rgba(160, 80, 80, 255));
    let delete_text = "Delete";
    let delete_dims = measure_text(delete_text, None, 14, 1.0);
    draw_text(delete_text, delete_rect.x + (delete_rect.w - delete_dims.width) / 2.0, delete_rect.y + delete_rect.h / 2.0 + 5.0, 14.0, WHITE);

    if ctx.mouse.clicked(&delete_rect) {
        return Some(DeleteTextureAction::Confirm);
    }

    None
}

/// Action from unsaved texture confirmation dialog
#[derive(Debug, Clone, Copy)]
enum UnsavedTextureAction {
    Save,
    Discard,
    Cancel,
}

/// Draw the unsaved texture changes dialog (modal overlay)
fn draw_unsaved_texture_dialog(
    ctx: &mut UiContext,
    state: &ModelerState,
    _icon_font: Option<&Font>,
) -> Option<UnsavedTextureAction> {
    // Only show if there's a pending switch
    let _pending_idx = state.unsaved_texture_pending_switch?;

    // Get the texture name being edited
    let texture_name = state.editing_texture.as_ref()
        .map(|t| t.name.as_str())
        .unwrap_or("texture");

    // Dark overlay
    draw_rectangle(0.0, 0.0, screen_width(), screen_height(), Color::new(0.0, 0.0, 0.0, 0.6));

    // Dialog dimensions (wider to fit 3 buttons)
    let dialog_w = 360.0;
    let dialog_h = 130.0;
    let dialog_x = (screen_width() - dialog_w) / 2.0;
    let dialog_y = (screen_height() - dialog_h) / 2.0;

    // Dialog background
    draw_rectangle(dialog_x, dialog_y, dialog_w, dialog_h, Color::from_rgba(45, 45, 55, 255));
    draw_rectangle_lines(dialog_x, dialog_y, dialog_w, dialog_h, 2.0, Color::from_rgba(80, 80, 90, 255));

    // Title bar (warning color)
    draw_rectangle(dialog_x, dialog_y, dialog_w, 24.0, Color::from_rgba(120, 100, 50, 255));
    draw_text("Unsaved Changes", dialog_x + 8.0, dialog_y + 17.0, 16.0, WHITE);

    // Message
    let msg = format!("'{}' has unsaved changes.", texture_name);
    let msg_dims = measure_text(&msg, None, 14, 1.0);
    draw_text(&msg, dialog_x + (dialog_w - msg_dims.width) / 2.0, dialog_y + 55.0, 14.0, WHITE);
    let sub_msg = "Save before switching objects?";
    let sub_dims = measure_text(sub_msg, None, 12, 1.0);
    draw_text(sub_msg, dialog_x + (dialog_w - sub_dims.width) / 2.0, dialog_y + 75.0, 12.0, Color::from_rgba(180, 180, 180, 255));

    // Buttons (3 buttons: Cancel, Discard, Save)
    let btn_w = 80.0;
    let btn_h = 28.0;
    let btn_y = dialog_y + dialog_h - btn_h - 12.0;
    let btn_spacing = 15.0;
    let total_btn_w = btn_w * 3.0 + btn_spacing * 2.0;
    let btn_start_x = dialog_x + (dialog_w - total_btn_w) / 2.0;

    // Cancel button (leftmost)
    let cancel_rect = Rect::new(btn_start_x, btn_y, btn_w, btn_h);
    let cancel_hovered = ctx.mouse.inside(&cancel_rect);
    let cancel_bg = if cancel_hovered { Color::from_rgba(70, 70, 80, 255) } else { Color::from_rgba(55, 55, 65, 255) };
    draw_rectangle(cancel_rect.x, cancel_rect.y, cancel_rect.w, cancel_rect.h, cancel_bg);
    draw_rectangle_lines(cancel_rect.x, cancel_rect.y, cancel_rect.w, cancel_rect.h, 1.0, Color::from_rgba(80, 80, 90, 255));
    let cancel_text = "Cancel";
    let cancel_dims = measure_text(cancel_text, None, 14, 1.0);
    draw_text(cancel_text, cancel_rect.x + (cancel_rect.w - cancel_dims.width) / 2.0, cancel_rect.y + cancel_rect.h / 2.0 + 5.0, 14.0, if cancel_hovered { WHITE } else { Color::from_rgba(200, 200, 200, 255) });

    if ctx.mouse.clicked(&cancel_rect) {
        return Some(UnsavedTextureAction::Cancel);
    }

    // Discard button (middle, red-ish)
    let discard_rect = Rect::new(btn_start_x + btn_w + btn_spacing, btn_y, btn_w, btn_h);
    let discard_hovered = ctx.mouse.inside(&discard_rect);
    let discard_bg = if discard_hovered { Color::from_rgba(140, 70, 70, 255) } else { Color::from_rgba(100, 55, 55, 255) };
    draw_rectangle(discard_rect.x, discard_rect.y, discard_rect.w, discard_rect.h, discard_bg);
    draw_rectangle_lines(discard_rect.x, discard_rect.y, discard_rect.w, discard_rect.h, 1.0, Color::from_rgba(140, 80, 80, 255));
    let discard_text = "Discard";
    let discard_dims = measure_text(discard_text, None, 14, 1.0);
    draw_text(discard_text, discard_rect.x + (discard_rect.w - discard_dims.width) / 2.0, discard_rect.y + discard_rect.h / 2.0 + 5.0, 14.0, if discard_hovered { WHITE } else { Color::from_rgba(220, 180, 180, 255) });

    if ctx.mouse.clicked(&discard_rect) {
        return Some(UnsavedTextureAction::Discard);
    }

    // Save button (rightmost, green-ish)
    let save_rect = Rect::new(btn_start_x + (btn_w + btn_spacing) * 2.0, btn_y, btn_w, btn_h);
    let save_hovered = ctx.mouse.inside(&save_rect);
    let save_bg = if save_hovered { Color::from_rgba(70, 130, 70, 255) } else { Color::from_rgba(55, 100, 55, 255) };
    draw_rectangle(save_rect.x, save_rect.y, save_rect.w, save_rect.h, save_bg);
    draw_rectangle_lines(save_rect.x, save_rect.y, save_rect.w, save_rect.h, 1.0, Color::from_rgba(80, 140, 80, 255));
    let save_text = "Save";
    let save_dims = measure_text(save_text, None, 14, 1.0);
    draw_text(save_text, save_rect.x + (save_rect.w - save_dims.width) / 2.0, save_rect.y + save_rect.h / 2.0 + 5.0, 14.0, if save_hovered { WHITE } else { Color::from_rgba(180, 220, 180, 255) });

    if ctx.mouse.clicked(&save_rect) {
        return Some(UnsavedTextureAction::Save);
    }

    None
}

/// Draw the texture browser header with Import/New/Edit/Delete and Zoom buttons (unified icon toolbar)
fn draw_paint_header(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, icon_font: Option<&Font>) {
    use crate::ui::Toolbar;

    draw_rectangle(rect.x.floor(), rect.y.floor(), rect.w, rect.h, Color::from_rgba(40, 40, 45, 255));

    let mut toolbar = Toolbar::new(rect);

    // Import button - opens file picker
    if toolbar.icon_button(ctx, icon::FOLDER_OPEN, icon_font, "Import PNG") {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Images", &["png", "jpg", "jpeg", "bmp"])
                .pick_file()
            {
                match std::fs::read(&path) {
                    Ok(bytes) => {
                        if let Err(e) = load_png_to_import_state(&bytes, &mut state.texture_editor.import_state) {
                            state.set_status(&format!("Import failed: {}", e), 3.0);
                        }
                    }
                    Err(e) => {
                        state.set_status(&format!("Failed to read file: {}", e), 3.0);
                    }
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            state.set_status("Import not yet available in browser", 2.0);
        }
    }

    // New button - creates a new texture
    if toolbar.icon_button(ctx, icon::PLUS, icon_font, "New Texture") {
        let name = state.user_textures.next_available_name();
        let tex = UserTexture::new(&name, TextureSize::Size64x64, ClutDepth::Bpp4);
        state.user_textures.add(tex);
        state.editing_texture = Some(state.user_textures.get(&name).unwrap().clone());
        state.editing_indexed_atlas = true;
        state.texture_editor.reset();
    }

    // Edit button - edits the selected texture
    let has_selection = state.selected_user_texture.is_some();
    if has_selection {
        if toolbar.icon_button(ctx, icon::PENCIL, icon_font, "Edit Texture") {
            if let Some(name) = &state.selected_user_texture {
                if let Some(tex) = state.user_textures.get(name) {
                    state.editing_texture = Some(tex.clone());
                    state.editing_indexed_atlas = true;
                    state.texture_editor.reset();
                }
            }
        }
    } else {
        toolbar.icon_button_disabled(ctx, icon::PENCIL, icon_font, "Edit Texture (select a texture first)");
    }

    // Delete button - deletes the selected user texture (not samples)
    let is_user_texture = state.selected_user_texture.as_ref()
        .and_then(|name| state.user_textures.get(name))
        .map(|tex| tex.source == crate::texture::TextureSource::User)
        .unwrap_or(false);
    let delete_enabled = has_selection && is_user_texture;

    if delete_enabled {
        if toolbar.icon_button_danger(ctx, icon::TRASH, icon_font, "Delete Texture") {
            if let Some(name) = &state.selected_user_texture {
                state.texture_pending_delete = Some(name.clone());
            }
        }
    } else {
        let tooltip = if has_selection && !is_user_texture {
            "Cannot delete sample textures"
        } else {
            "Delete Texture (select a user texture first)"
        };
        toolbar.icon_button_danger_disabled(ctx, icon::TRASH, icon_font, tooltip);
    }

    toolbar.separator();

    // Zoom buttons
    if toolbar.icon_button(ctx, icon::ZOOM_OUT, icon_font, "Smaller Thumbnails") {
        state.paint_thumb_size = smaller_thumb_size(state.paint_thumb_size);
    }
    if toolbar.icon_button(ctx, icon::ZOOM_IN, icon_font, "Larger Thumbnails") {
        state.paint_thumb_size = larger_thumb_size(state.paint_thumb_size);
    }
}

/// Draw the texture browser grid with two sections: SAMPLES and MY TEXTURES (matches World Editor)
fn draw_paint_texture_browser(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, icon_font: Option<&Font>) {
    const HEADER_HEIGHT: f32 = 28.0;
    const THUMB_PADDING: f32 = 4.0;
    const SECTION_HEADER_HEIGHT: f32 = 24.0;

    // Get thumbnail size from state
    let thumb_size = state.paint_thumb_size;

    // Header with New/Edit buttons
    let header_rect = Rect::new(rect.x, rect.y, rect.w, HEADER_HEIGHT);
    draw_paint_header(ctx, header_rect, state, icon_font);

    // Content area for texture grid
    let content_rect = Rect::new(rect.x, rect.y + HEADER_HEIGHT, rect.w, rect.h - HEADER_HEIGHT);

    // Calculate columns
    let cols = ((content_rect.w - THUMB_PADDING) / (thumb_size + THUMB_PADDING)).floor() as usize;
    let cols = cols.max(1);

    // Collect texture names for both sections
    let sample_names: Vec<String> = state.user_textures.sample_names().map(|s| s.to_string()).collect();
    let user_names: Vec<String> = state.user_textures.user_names().map(|s| s.to_string()).collect();

    // Calculate content heights for each section
    let sample_rows = if state.paint_samples_collapsed { 0 } else { (sample_names.len() + cols - 1) / cols.max(1) };
    let user_rows = if state.paint_user_collapsed { 0 } else { (user_names.len() + cols - 1) / cols.max(1) };

    let sample_content_h = sample_rows as f32 * (thumb_size + THUMB_PADDING);
    let user_content_h = user_rows as f32 * (thumb_size + THUMB_PADDING);

    // Total scrollable height
    let total_height = SECTION_HEADER_HEIGHT * 2.0 + sample_content_h + user_content_h + THUMB_PADDING * 2.0;

    // Handle scrolling
    let max_scroll = (total_height - content_rect.h).max(0.0);
    state.paint_texture_scroll = state.paint_texture_scroll.clamp(0.0, max_scroll);

    if ctx.mouse.inside(&content_rect) {
        state.paint_texture_scroll -= ctx.mouse.scroll * 12.0;
        state.paint_texture_scroll = state.paint_texture_scroll.clamp(0.0, max_scroll);
    }

    // Draw scrollbar if needed
    if total_height > content_rect.h && max_scroll > 0.0 {
        let scrollbar_width = 8.0;
        let scrollbar_x = content_rect.right() - scrollbar_width - 2.0;
        let scrollbar_height = content_rect.h;
        let scroll_thumb_height = (content_rect.h / total_height * scrollbar_height).max(20.0);
        let thumb_y = content_rect.y + (state.paint_texture_scroll / max_scroll) * (scrollbar_height - scroll_thumb_height);

        draw_rectangle(scrollbar_x, content_rect.y, scrollbar_width, scrollbar_height, Color::from_rgba(15, 15, 20, 255));
        draw_rectangle(scrollbar_x, thumb_y, scrollbar_width, scroll_thumb_height, Color::from_rgba(80, 80, 90, 255));
    }

    // Colors
    let section_bg = Color::from_rgba(40, 40, 50, 255);
    let text_color = Color::from_rgba(200, 200, 200, 255);
    let text_dim = Color::from_rgba(140, 140, 140, 255);

    // Track clicked texture (with is_sample flag)
    let mut clicked_texture: Option<(String, bool)> = None;
    let mut double_clicked_texture: Option<(String, bool)> = None;

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

    let mut y = content_rect.y - state.paint_texture_scroll;

    // ═══════════════════════════════════════════════════════════════════════════
    // SAMPLES section
    // ═══════════════════════════════════════════════════════════════════════════
    let samples_header_rect = Rect::new(content_rect.x, y, content_rect.w, SECTION_HEADER_HEIGHT);
    if y + SECTION_HEADER_HEIGHT > content_rect.y && y < content_rect.bottom() {
        let draw_y = y.max(content_rect.y);
        let draw_h = SECTION_HEADER_HEIGHT.min(content_rect.bottom() - draw_y);
        draw_rectangle(content_rect.x, draw_y, content_rect.w, draw_h, section_bg);

        if y >= content_rect.y {
            let arrow = if state.paint_samples_collapsed { ">" } else { "v" };
            draw_text(
                &format!("{} SAMPLE TEXTURES ({})", arrow, sample_names.len()),
                content_rect.x + 8.0,
                y + 17.0,
                14.0,
                text_color,
            );
        }

        // Toggle collapse on click
        if ctx.mouse.inside(&samples_header_rect) && ctx.mouse.left_pressed && samples_header_rect.y >= content_rect.y {
            state.paint_samples_collapsed = !state.paint_samples_collapsed;
        }
    }
    y += SECTION_HEADER_HEIGHT;

    // Sample texture grid (if not collapsed)
    if !state.paint_samples_collapsed {
        if sample_names.is_empty() {
            if y + 20.0 > content_rect.y && y < content_rect.bottom() {
                draw_text("  (no sample textures)", content_rect.x + 8.0, y + 14.0, 12.0, text_dim);
            }
            y += 20.0;
        } else {
            for (i, name) in sample_names.iter().enumerate() {
                let col = i % cols;
                let row = i / cols;

                let x = content_rect.x + THUMB_PADDING + col as f32 * (thumb_size + THUMB_PADDING);
                let item_y = y + THUMB_PADDING + row as f32 * (thumb_size + THUMB_PADDING);

                // Skip if outside visible area
                if item_y + thumb_size < content_rect.y || item_y > content_rect.bottom() {
                    continue;
                }

                draw_modeler_texture_thumbnail(
                    ctx,
                    &content_rect,
                    state,
                    name,
                    x,
                    item_y,
                    thumb_size,
                    true, // is_sample
                    &mut clicked_texture,
                    &mut double_clicked_texture,
                );
            }
            y += sample_content_h;
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // MY TEXTURES section
    // ═══════════════════════════════════════════════════════════════════════════
    let user_header_rect = Rect::new(content_rect.x, y, content_rect.w, SECTION_HEADER_HEIGHT);
    if y + SECTION_HEADER_HEIGHT > content_rect.y && y < content_rect.bottom() {
        let draw_y = y.max(content_rect.y);
        let draw_h = SECTION_HEADER_HEIGHT.min(content_rect.bottom() - draw_y);
        draw_rectangle(content_rect.x, draw_y, content_rect.w, draw_h, section_bg);

        if y >= content_rect.y {
            let arrow = if state.paint_user_collapsed { ">" } else { "v" };
            draw_text(
                &format!("{} MY TEXTURES ({})", arrow, user_names.len()),
                content_rect.x + 8.0,
                y + 17.0,
                14.0,
                text_color,
            );
        }

        // Toggle collapse on click
        if ctx.mouse.inside(&user_header_rect) && ctx.mouse.left_pressed && user_header_rect.y >= content_rect.y {
            state.paint_user_collapsed = !state.paint_user_collapsed;
        }
    }
    y += SECTION_HEADER_HEIGHT;

    // User texture grid (if not collapsed)
    if !state.paint_user_collapsed {
        if user_names.is_empty() {
            if y + 20.0 > content_rect.y && y < content_rect.bottom() {
                draw_text("  (no user textures)", content_rect.x + 8.0, y + 14.0, 12.0, text_dim);
            }
            // y += 20.0; // Not needed since this is the last section
        } else {
            for (i, name) in user_names.iter().enumerate() {
                let col = i % cols;
                let row = i / cols;

                let x = content_rect.x + THUMB_PADDING + col as f32 * (thumb_size + THUMB_PADDING);
                let item_y = y + THUMB_PADDING + row as f32 * (thumb_size + THUMB_PADDING);

                // Skip if outside visible area
                if item_y + thumb_size < content_rect.y || item_y > content_rect.bottom() {
                    continue;
                }

                draw_modeler_texture_thumbnail(
                    ctx,
                    &content_rect,
                    state,
                    name,
                    x,
                    item_y,
                    thumb_size,
                    false, // is_sample
                    &mut clicked_texture,
                    &mut double_clicked_texture,
                );
            }
        }
    }

    // Disable scissor clipping
    unsafe {
        get_internal_gl().quad_gl.scissor(None);
    }

    // Handle single-click to select and assign texture to selected object
    if let Some((name, _is_sample)) = clicked_texture {
        state.selected_user_texture = Some(name.clone());

        // Clone texture data to avoid borrow issues
        let tex_data = state.user_textures.get(&name).cloned();
        if let Some(tex) = tex_data {
            // Create a new CLUT for this object with the texture's palette
            // This ensures each object has its own CLUT, not shared with other objects
            let obj_name = state.selected_object()
                .map(|o| o.name.clone())
                .unwrap_or_else(|| "object".to_string());
            let clut_name = format!("{}_clut", obj_name);

            let mut new_clut = Clut::new_4bit(&clut_name);
            new_clut.colors = tex.palette.clone();
            new_clut.depth = tex.depth;
            let new_clut_id = state.clut_pool.add_clut(new_clut);

            // Update the selected object's texture reference and atlas
            let tex_id = tex.id;
            if let Some(obj) = state.selected_object_mut() {
                // Set ID-based reference for persistence (survives texture edits)
                obj.texture_ref = TextureRef::Id(tex_id);
                // Also update atlas for runtime rendering
                obj.atlas.width = tex.width;
                obj.atlas.height = tex.height;
                obj.atlas.depth = tex.depth;
                obj.atlas.indices = tex.indices.clone();
                obj.atlas.default_clut = new_clut_id;
            }

            // Update the editing texture to match
            state.editing_texture = Some(tex);
            state.dirty = true;
        }
    }

    // Handle double-click to edit (also sets selection)
    // Note: Sample textures are read-only, double-click shows message
    if let Some((name, is_sample)) = double_clicked_texture {
        state.selected_user_texture = Some(name.clone());
        if is_sample {
            // Sample textures are read-only
            state.set_status("Sample textures are read-only. Use 'New' to create editable textures.", 3.0);
        } else if let Some(tex) = state.user_textures.get(&name) {
            state.editing_texture = Some(tex.clone());
            state.editing_indexed_atlas = true;
            state.texture_editor.reset();
        }
    }
}

/// Helper function to draw a single texture thumbnail (modeler version)
fn draw_modeler_texture_thumbnail(
    ctx: &UiContext,
    content_rect: &Rect,
    state: &ModelerState,
    name: &str,
    x: f32,
    y: f32,
    thumb_size: f32,
    is_sample: bool,
    clicked_texture: &mut Option<(String, bool)>,
    double_clicked_texture: &mut Option<(String, bool)>,
) {
    let thumb_rect = Rect::new(x, y, thumb_size, thumb_size);

    // Get texture for rendering
    if let Some(tex) = state.user_textures.get(name) {
        // Draw checkerboard background for transparency
        let check_size = (thumb_size / tex.width.max(tex.height) as f32 * 2.0).max(4.0);
        draw_checkerboard(x, y, thumb_size, thumb_size, check_size);

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
                *double_clicked_texture = Some((name.to_string(), is_sample));
            }
        } else if ctx.mouse.clicked(&visible_rect) {
            *clicked_texture = Some((name.to_string(), is_sample));
        }
    }

    // Check if this texture is selected
    let is_selected = state.selected_user_texture.as_deref() == Some(name);

    // Selection highlight (golden border for user textures, cyan for samples)
    if is_selected {
        let highlight_color = if is_sample {
            Color::from_rgba(100, 200, 255, 255) // Cyan for samples
        } else {
            Color::from_rgba(255, 200, 50, 255) // Gold for user textures
        };
        draw_rectangle_lines(x - 2.0, y - 2.0, thumb_size + 4.0, thumb_size + 4.0, 2.0, highlight_color);
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

/// Convert a UserTexture to a macroquad texture for display (with transparency)
fn user_texture_to_mq_texture(texture: &UserTexture) -> Texture2D {
    let mut pixels = Vec::with_capacity(texture.width * texture.height * 4);
    for y in 0..texture.height {
        for x in 0..texture.width {
            let idx = texture.indices[y * texture.width + x] as usize;
            let color = texture.palette.get(idx).copied().unwrap_or_default();
            // Index 0 is transparent
            let alpha = if idx == 0 { 0 } else { 255 };
            pixels.push(color.r8());
            pixels.push(color.g8());
            pixels.push(color.b8());
            pixels.push(alpha);
        }
    }

    let tex = Texture2D::from_rgba8(texture.width as u16, texture.height as u16, &pixels);
    tex.set_filter(FilterMode::Nearest);
    tex
}

/// Draw a checkerboard pattern for transparency display
fn draw_checkerboard(x: f32, y: f32, w: f32, h: f32, check_size: f32) {
    let cols = (w / check_size).ceil() as i32;
    let rows = (h / check_size).ceil() as i32;
    for row in 0..rows {
        for col in 0..cols {
            let c = if (row + col) % 2 == 0 {
                Color::new(0.25, 0.25, 0.28, 1.0)
            } else {
                Color::new(0.18, 0.18, 0.20, 1.0)
            };
            let cx = x + col as f32 * check_size;
            let cy = y + row as f32 * check_size;
            let cw = check_size.min(x + w - cx);
            let ch = check_size.min(y + h - cy);
            draw_rectangle(cx, cy, cw, ch, c);
        }
    }
}

/// Draw the texture editor panel (when editing a texture)
fn draw_paint_texture_editor(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, icon_font: Option<&Font>, storage: &Storage) {
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

    // Extract texture info before calculating layout (to avoid borrow conflicts later)
    let (tex_width_f, tex_height_f, tex_name, is_dirty) = {
        let tex = state.editing_texture.as_ref().unwrap();
        (tex.width as f32, tex.height as f32, tex.name.clone(), state.texture_editor.dirty)
    };

    // Header with texture name and buttons (match main toolbar sizing: 36px height, 32px buttons, 16px icons)
    let header_h = 36.0;
    let btn_size = 32.0;
    let icon_size = 16.0;
    let header_rect = Rect::new(rect.x, rect.y, rect.w, header_h);
    draw_rectangle(header_rect.x, header_rect.y, header_rect.w, header_rect.h, Color::from_rgba(45, 45, 55, 255));

    // Back button (arrow-big-left) - far right
    let back_rect = Rect::new(rect.right() - btn_size - 2.0, rect.y + 2.0, btn_size, btn_size);
    let back_hovered = ctx.mouse.inside(&back_rect);
    if back_hovered {
        draw_rectangle(back_rect.x, back_rect.y, back_rect.w, back_rect.h, Color::from_rgba(80, 60, 60, 255));
    }
    draw_icon_centered(icon_font, icon::ARROW_BIG_LEFT, &back_rect, icon_size, if back_hovered { WHITE } else { Color::from_rgba(200, 200, 200, 255) });

    if ctx.mouse.clicked(&back_rect) {
        state.editing_texture = None;
        state.editing_indexed_atlas = false;
        return;
    }

    // Save/Download button - only visible when dirty
    let mut save_clicked = false;
    if is_dirty {
        let save_rect = Rect::new(back_rect.x - btn_size - 2.0, rect.y + 2.0, btn_size, btn_size);
        let save_hovered = ctx.mouse.inside(&save_rect);

        // Highlight button to draw attention
        let save_bg = if save_hovered {
            Color::from_rgba(80, 100, 80, 255)
        } else {
            Color::from_rgba(60, 80, 60, 255)
        };
        draw_rectangle(save_rect.x, save_rect.y, save_rect.w, save_rect.h, save_bg);

        // Use SAVE icon on desktop, DOWNLOAD icon on WASM
        #[cfg(not(target_arch = "wasm32"))]
        let save_icon = icon::SAVE;
        #[cfg(target_arch = "wasm32")]
        let save_icon = icon::DOWNLOAD;

        draw_icon_centered(icon_font, save_icon, &save_rect, icon_size, if save_hovered { WHITE } else { Color::from_rgba(200, 200, 200, 255) });

        if ctx.mouse.clicked(&save_rect) {
            save_clicked = true;
        }
    }

    // Texture name with dirty indicator (vertically centered in header)
    let dirty_indicator = if is_dirty { " ●" } else { "" };
    let name_text = format!("Editing: {}{}", tex_name, dirty_indicator);
    let name_color = if is_dirty { Color::from_rgba(255, 200, 100, 255) } else { WHITE };
    draw_text(&name_text, (header_rect.x + 8.0).floor(), (header_rect.y + header_h / 2.0 + 4.0).floor(), 12.0, name_color);

    // Content area below header
    let content_rect_full = Rect::new(rect.x, rect.y + header_h, rect.w, rect.h - header_h);

    // Draw mode tabs (Paint / UV) at the top
    let content_rect = draw_mode_tabs(ctx, content_rect_full, &mut state.texture_editor);

    // Layout: Canvas (square, capped size) + Tool panel (right), Palette panel (below, gets remaining space)
    // This matches the World Editor's texture_palette.rs layout exactly
    let tool_panel_w = 66.0;  // 2-column layout: 2 * 28px buttons + 2px gap + 4px padding each side
    let canvas_w = content_rect.w - tool_panel_w;
    // Tool panel needs ~280px height (6 tools + undo/redo/zoom/grid + size/shape options)
    // Palette needs: depth buttons (~22) + gen row (~24) + grid (~65) + color editor (~60) + effect (~18) = ~190 base
    let min_canvas_h: f32 = 280.0;  // Minimum for tool panel to fit all buttons
    let min_palette_h: f32 = 190.0;  // Minimum palette panel height
    // Calculate canvas height: try to be squarish (canvas_w), but MUST leave room for palette
    let available_for_canvas = (content_rect.h - min_palette_h).max(0.0);
    // Canvas is at least min_canvas_h, but not more than available_for_canvas, and prefer squarish
    let canvas_h = available_for_canvas.min(canvas_w).max(min_canvas_h.min(available_for_canvas));
    let palette_panel_h = content_rect.h - canvas_h;

    let canvas_rect = Rect::new(content_rect.x, content_rect.y, canvas_w, canvas_h);
    let tool_rect = Rect::new(content_rect.x + canvas_w, content_rect.y, tool_panel_w, canvas_h);
    let palette_rect = Rect::new(content_rect.x, content_rect.y + canvas_h, content_rect.w, palette_panel_h);

    // Save undo BEFORE drawing if a new stroke is starting AND cursor is inside canvas
    if ctx.mouse.left_pressed
        && ctx.mouse.inside(&canvas_rect)
        && !state.texture_editor.drawing
        && !state.texture_editor.panning
        && state.texture_editor.tool.modifies_texture()
    {
        state.save_texture_undo();
    }

    // Now get mutable reference to the texture for drawing
    let tex = state.editing_texture.as_mut().unwrap();

    // Draw panels using the shared texture editor components
    draw_texture_canvas(ctx, canvas_rect, tex, &mut state.texture_editor, uv_data.as_ref());
    draw_tool_panel(ctx, tool_rect, &mut state.texture_editor, icon_font);
    draw_palette_panel_constrained(ctx, palette_rect, tex, &mut state.texture_editor, icon_font, Some(canvas_w));

    // Handle UV modal transforms (G/S/R) - apply to actual mesh vertices
    apply_uv_modal_transform(ctx, &canvas_rect, tex_width_f, tex_height_f, state);

    // Handle direct UV dragging (with pixel snapping)
    apply_uv_direct_drag(ctx, &canvas_rect, tex_width_f, tex_height_f, state);

    // Handle UV operations (flip/rotate/reset buttons)
    apply_uv_operation(tex_width_f, tex_height_f, state);

    // Handle undo save signals from texture editor (for non-paint actions like selection move)
    if state.texture_editor.undo_save_pending.take().is_some() {
        state.save_texture_undo();
    }

    // Handle UV undo save signals (for UV transforms - saves mesh, not texture)
    if let Some(description) = state.texture_editor.uv_undo_pending.take() {
        state.push_undo(&description);
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

    // Handle auto-unwrap button request from UV editor
    if state.texture_editor.auto_unwrap_requested {
        state.texture_editor.auto_unwrap_requested = false;
        auto_unwrap_selected_faces(state);
    }

    // Sync editing_texture back to ALL objects that use this texture (not just selected)
    // This ensures texture changes are visible on all objects sharing the same texture
    if state.editing_indexed_atlas {
        let editing_tex_data = state.editing_texture.clone();
        if let Some(editing_tex) = editing_tex_data {
            // Get the texture ID from the library to find all objects using it
            let tex_id = state.user_textures.get(&tex_name).map(|t| t.id);

            // Collect CLUT IDs that need updating (to avoid double borrow)
            let mut clut_ids_to_update = Vec::new();

            if let (Some(tex_id), Some(objects)) = (tex_id, state.objects_mut()) {
                for obj in objects.iter_mut() {
                    // Update all objects that reference this texture
                    if let TextureRef::Id(obj_tex_id) = obj.texture_ref {
                        if obj_tex_id == tex_id {
                            obj.atlas.width = editing_tex.width;
                            obj.atlas.height = editing_tex.height;
                            obj.atlas.depth = editing_tex.depth;
                            obj.atlas.indices = editing_tex.indices.clone();
                            clut_ids_to_update.push(obj.atlas.default_clut);
                        }
                    }
                }
            }

            // Update CLUTs after releasing objects borrow
            for clut_id in clut_ids_to_update {
                if let Some(clut) = state.clut_pool.get_mut(clut_id) {
                    clut.colors = editing_tex.palette.clone();
                    clut.depth = editing_tex.depth;
                }
            }
        }
    }

    // Handle save button click
    if save_clicked {
        // Sync editing_texture to user_textures library before saving
        if let Some(ref editing_tex) = state.editing_texture {
            if let Some(lib_tex) = state.user_textures.get_mut(&tex_name) {
                lib_tex.indices = editing_tex.indices.clone();
                lib_tex.palette = editing_tex.palette.clone();
                lib_tex.depth = editing_tex.depth;
                lib_tex.width = editing_tex.width;
                lib_tex.height = editing_tex.height;
            }
        }
        // Now save via storage
        if let Err(e) = state.user_textures.save_texture_with_storage(&tex_name, storage) {
            state.set_status(&format!("Failed to save: {}", e), 3.0);
        } else {
            // Clear dirty flag on successful save
            state.texture_editor.dirty = false;
            let cloud_text = if storage.has_cloud() { " to cloud" } else { "" };
            state.set_status(&format!("Saved '{}'{}", tex_name, cloud_text), 2.0);
            // Flag to sync with world editor
            state.pending_texture_refresh = true;
        }
        state.dirty = true;
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
    // Only apply transforms for active states (not None or ScalePending)
    if transform == UvModalTransform::None || transform == UvModalTransform::ScalePending {
        return;
    }

    // Extract all needed values before borrowing state mutably
    let zoom = state.texture_editor.zoom;
    let pan_x = state.texture_editor.pan_x;
    let pan_y = state.texture_editor.pan_y;
    let (start_mx, start_my) = state.texture_editor.uv_modal_start_mouse;
    let uv_modal_center = state.texture_editor.uv_modal_center;
    let start_uvs: Vec<(usize, RastVec2)> = state.texture_editor.uv_modal_start_uvs.iter()
        .map(|(vi, uv)| (*vi, *uv))
        .collect();

    // Calculate texture position on screen
    let canvas_cx = canvas_rect.x + canvas_rect.w / 2.0;
    let canvas_cy = canvas_rect.y + canvas_rect.h / 2.0;
    let _tex_x = canvas_cx - tex_width * zoom / 2.0 + pan_x;
    let _tex_y = canvas_cy - tex_height * zoom / 2.0 + pan_y;

    // Screen delta in UV space
    let delta_screen_x = ctx.mouse.x - start_mx;
    let delta_screen_y = ctx.mouse.y - start_my;
    let delta_u = delta_screen_x / (tex_width * zoom);
    let delta_v = -delta_screen_y / (tex_height * zoom); // Inverted Y

    // Get the mesh to modify
    let obj = match state.selected_object_mut() {
        Some(o) => o,
        None => return,
    };

    match transform {
        UvModalTransform::Grab => {
            // Move selected vertices by delta with pixel snapping
            for (vi, original_uv) in &start_uvs {
                if let Some(v) = obj.mesh.vertices.get_mut(*vi) {
                    let new_u = original_uv.x + delta_u;
                    let new_v = original_uv.y + delta_v;
                    // Snap to pixel boundaries
                    v.uv.x = (new_u * tex_width).round() / tex_width;
                    v.uv.y = (new_v * tex_height).round() / tex_height;
                }
            }
        }
        UvModalTransform::Scale => {
            // Scale around center - snap center to pixel boundary for consistent results
            let center = RastVec2::new(
                (uv_modal_center.x * tex_width).round() / tex_width,
                (uv_modal_center.y * tex_height).round() / tex_height,
            );
            // Scale factor based on horizontal mouse movement
            let scale = 1.0 + delta_screen_x * 0.01;
            let scale = scale.max(0.01); // Prevent negative/zero scale

            for (vi, original_uv) in &start_uvs {
                if let Some(v) = obj.mesh.vertices.get_mut(*vi) {
                    // Snap original UV to pixel boundary for consistent scaling
                    let snapped_orig = RastVec2::new(
                        (original_uv.x * tex_width).round() / tex_width,
                        (original_uv.y * tex_height).round() / tex_height,
                    );
                    let offset_x = snapped_orig.x - center.x;
                    let offset_y = snapped_orig.y - center.y;
                    let new_u = center.x + offset_x * scale;
                    let new_v = center.y + offset_y * scale;
                    // Snap to pixel boundaries
                    v.uv.x = (new_u * tex_width).round() / tex_width;
                    v.uv.y = (new_v * tex_height).round() / tex_height;
                }
            }
        }
        UvModalTransform::Rotate => {
            // Rotate around center with pixel snapping
            let center = uv_modal_center;
            // Rotation angle based on horizontal mouse movement
            let angle = delta_screen_x * 0.01; // Radians
            let cos_a = angle.cos();
            let sin_a = angle.sin();

            for (vi, original_uv) in &start_uvs {
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
        }
        UvModalTransform::HandleScale => {
            // Bounding box handle scale - apply pre-calculated UVs directly
            for (vi, new_uv) in &start_uvs {
                if let Some(v) = obj.mesh.vertices.get_mut(*vi) {
                    // Snap to pixel boundaries
                    v.uv.x = (new_uv.x * tex_width).round() / tex_width;
                    v.uv.y = (new_uv.y * tex_height).round() / tex_height;
                }
            }
        }
        UvModalTransform::None | UvModalTransform::ScalePending => {}
    }
    state.dirty = true;
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

    // Extract all needed values before borrowing state mutably
    let zoom = state.texture_editor.zoom;
    let (start_mx, start_my) = state.texture_editor.uv_drag_start;
    let drag_start_uvs: Vec<(usize, usize, RastVec2)> = state.texture_editor.uv_drag_start_uvs.iter()
        .map(|&(fi, vi, uv)| (fi, vi, uv))
        .collect();

    // Screen delta in UV space
    let delta_screen_x = ctx.mouse.x - start_mx;
    let delta_screen_y = ctx.mouse.y - start_my;
    let delta_u = delta_screen_x / (tex_width * zoom);
    let delta_v = -delta_screen_y / (tex_height * zoom); // Inverted Y

    // Get the mesh to modify
    let obj = match state.selected_object_mut() {
        Some(o) => o,
        None => return,
    };

    // Move selected vertices by delta with pixel snapping
    for &(_, vi, original_uv) in &drag_start_uvs {
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

    // Get selected vertex indices before mutably borrowing state
    let selected_vertices = state.texture_editor.uv_selection.clone();
    if selected_vertices.is_empty() {
        state.texture_editor.set_status("No vertices selected");
        return;
    }

    // Get the mesh to modify
    let obj = match state.selected_object_mut() {
        Some(o) => o,
        None => return,
    };

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
    let obj = state.selected_object()?;

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
    let atlas = state.atlas();
    let atlas_width = atlas.width;
    let atlas_height = atlas.height;

    // Get CLUT for rendering atlas preview (effective_clut logic)
    let clut = state.preview_clut
        .and_then(|id| state.clut_pool.get(id))
        .or_else(|| {
            state.objects().first()
                .filter(|obj| obj.atlas.default_clut.is_valid())
                .and_then(|obj| state.clut_pool.get(obj.atlas.default_clut))
        })
        .or_else(|| state.clut_pool.first_id().and_then(|id| state.clut_pool.get(id)));

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

    if let Some(obj) = state.selected_object() {
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

                    // Draw vertices (only in UV mode)
                    if state.texture_editor.mode == TextureEditorMode::Uv {
                        for (i, &vi) in face.vertices.iter().enumerate() {
                            if let Some((sx, sy)) = screen_uvs.get(i) {
                                let is_selected = state.texture_editor.uv_selection.contains(&vi);
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

    let atlas_width = state.atlas().width;
    let mut btn_x = x + 32.0;
    for (size, label) in sizes {
        let btn_w = label.len() as f32 * 7.0 + 6.0;
        let btn_rect = Rect::new(btn_x, *y, btn_w, btn_h);
        let is_current = atlas_width == size;
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
            if let Some(atlas) = state.atlas_mut() {
                atlas.resize(size, size);
            }
            state.dirty = true;
        }

        btn_x += btn_w + btn_spacing;
    }

    *y += btn_h + 4.0;
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
    draw_text("Blend:", x + 4.0, *y + 12.0, FONT_SIZE_HEADER, TEXT_DIM);

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
    draw_text("CLUT Pool", x + padding, cur_y + 10.0, FONT_SIZE_HEADER, TEXT_DIM);
    cur_y += LINE_HEIGHT;

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
        let clut = Clut::new_4bit(format!("CLUT {}", state.clut_pool.len() + 1));
        let id = state.clut_pool.add_clut(clut);
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
        let clut = Clut::new_8bit(format!("CLUT {}", state.clut_pool.len() + 1));
        let id = state.clut_pool.add_clut(clut);
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
    let clut_count = state.clut_pool.len();
    if clut_count == 0 {
        draw_text("(empty)", x + padding + 4.0, cur_y + 12.0, 12.0, TEXT_DIM);
    } else {
        let mut item_y = cur_y + 2.0;
        for clut in state.clut_pool.iter() {
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
            draw_text(badge_text, badge_x + 2.0, item_y + 11.0, FONT_SIZE_CONTENT, TEXT_DIM);

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
        if let Some(clut) = state.clut_pool.get(clut_id) {
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
                    if let Some(clut_mut) = state.clut_pool.get_mut(clut_id) {
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

                            if let Some(clut_mut) = state.clut_pool.get_mut(clut_id) {
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

/// Draw PicoCAD-style 4-panel viewport layout with resizable splits
/// ┌─────────────┬─────────────┐
/// │   3D View   │   Top (XZ)  │
/// │ (Perspective)│             │
/// ├─────────────┼─────────────┤
/// │  Front (XY) │  Side (YZ)  │
/// │             │             │
/// └─────────────┴─────────────┘
fn draw_4panel_viewport(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, fb: &mut Framebuffer) {
    let gap = 4.0; // Gap between panels (matches SplitPanel divider_size)
    let divider_hit_size = 8.0; // Hit area for dragging dividers

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

    // Handle divider dragging with proper state tracking
    let h_divider_rect = Rect::new(rect.x + left_w - divider_hit_size/2.0, rect.y, gap + divider_hit_size, rect.h);
    let v_divider_rect = Rect::new(rect.x, rect.y + top_h - divider_hit_size/2.0, rect.w, gap + divider_hit_size);

    let h_hovered = ctx.mouse.inside(&h_divider_rect);
    let v_hovered = ctx.mouse.inside(&v_divider_rect);

    // Start dragging on mouse press
    if ctx.mouse.left_pressed {
        if h_hovered {
            state.dragging_h_divider = true;
        }
        if v_hovered {
            state.dragging_v_divider = true;
        }
    }

    // Stop dragging on mouse release
    if !ctx.mouse.left_down {
        state.dragging_h_divider = false;
        state.dragging_v_divider = false;
    }

    // Update split positions while dragging (anywhere in viewport area)
    if state.dragging_h_divider {
        state.viewport_h_split = ((ctx.mouse.x - rect.x) / rect.w).clamp(0.15, 0.85);
    }
    if state.dragging_v_divider {
        state.viewport_v_split = ((ctx.mouse.y - rect.y) / rect.h).clamp(0.15, 0.85);
    }

    // Update active viewport/panel on click (not hover, matching World Editor)
    let on_divider = h_hovered || v_hovered || state.dragging_h_divider || state.dragging_v_divider;
    if !on_divider && ctx.mouse.left_pressed {
        for (id, vp_rect) in &viewports {
            if ctx.mouse.inside(vp_rect) {
                state.active_viewport = *id;
                state.active_panel = super::state::ActivePanel::Viewport;
                break;
            }
        }
    }

    // Draw each viewport
    for (id, vp_rect) in viewports {
        draw_single_viewport(ctx, vp_rect, state, fb, id);
    }

    // Draw dividers between viewports (matching SplitPanel style: 4px wide, darker color)
    let divider_color = Color::from_rgba(60, 60, 60, 255);
    let highlight_color = Color::from_rgba(100, 150, 255, 255); // Same as SplitPanel hover

    // Calculate divider positions (center of gap)
    let divider_y = rect.y + top_h;
    let divider_x = rect.x + left_w;

    // Horizontal divider (between top and bottom rows) - full gap height
    let h_divider_color = if v_hovered || state.dragging_v_divider { highlight_color } else { divider_color };
    draw_rectangle(rect.x, divider_y, rect.w, gap, h_divider_color);

    // Vertical divider (between left and right columns) - full gap width
    let v_divider_color = if h_hovered || state.dragging_h_divider { highlight_color } else { divider_color };
    draw_rectangle(divider_x, rect.y, gap, rect.h, v_divider_color);
}

/// Draw a single viewport with its header bar (matching World Editor style)
fn draw_single_viewport(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, fb: &mut Framebuffer, viewport_id: ViewportId) {
    // Viewport is active when both: panel focus is on viewports AND this specific viewport is selected
    let is_active = state.active_panel == super::state::ActivePanel::Viewport
        && state.active_viewport == viewport_id;
    let header_height = 20.0;

    // Header bar (matching World Editor panel style)
    let header_rect = Rect::new(rect.x, rect.y, rect.w, header_height);
    let header_color = Color::from_rgba(50, 50, 60, 255);
    draw_rectangle(header_rect.x, header_rect.y, header_rect.w, header_rect.h, header_color);

    // Header text - cyan when active, white when inactive (font size 16 to match World Editor)
    let label = viewport_id.label();
    let text_color = if is_active { ACCENT_COLOR } else { WHITE };
    draw_text(label, header_rect.x + 6.0, header_rect.y + 14.0, 16.0, text_color);

    // X-RAY label in header when enabled
    if state.xray_mode {
        let xray_label = "X-RAY";
        let xray_w = xray_label.len() as f32 * 7.0 + 8.0;
        let xray_x = header_rect.right() - xray_w - 4.0;
        draw_rectangle(xray_x, header_rect.y + 2.0, xray_w, header_height - 4.0, Color::from_rgba(180, 80, 80, 200));
        draw_text(xray_label, xray_x + 4.0, header_rect.y + 14.0, FONT_SIZE_CONTENT, WHITE);
    }

    // Content area below header
    let content_rect = Rect::new(rect.x, rect.y + header_height, rect.w, rect.h - header_height);

    // Background
    draw_rectangle(content_rect.x, content_rect.y, content_rect.w, content_rect.h, Color::from_rgba(25, 25, 30, 255));

    // Subtle border around entire viewport
    let border_color = Color::from_rgba(60, 60, 65, 255);
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, border_color);

    // Draw the actual 3D content
    draw_modeler_viewport_ext(ctx, content_rect, state, fb, viewport_id);
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
    let has_visible_geometry = state.objects().iter().any(|obj| obj.visible && !obj.mesh.vertices.is_empty())
        || (!mesh.vertices.is_empty() && state.selected_object.map_or(true, |i| state.objects().get(i).map_or(true, |o| o.visible)));

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

        let use_rgb555 = state.raster_settings.use_rgb555;

        // Fallback CLUT for objects with no assigned CLUT
        let default_clut = Clut::new_4bit("default");
        let fallback_clut = state.clut_pool.first_id()
            .and_then(|id| state.clut_pool.get(id))
            .unwrap_or(&default_clut);

        // Render all visible objects (each with its own texture atlas and CLUT)
        for (obj_idx, obj) in state.objects().iter().enumerate() {
            // Skip hidden objects
            if !obj.visible {
                continue;
            }

            // Get this object's CLUT from the shared pool (per-object atlas.default_clut)
            let obj_clut = if obj.atlas.default_clut.is_valid() {
                state.clut_pool.get(obj.atlas.default_clut).unwrap_or(fallback_clut)
            } else {
                fallback_clut
            };

            // Convert this object's atlas to rasterizer texture using its own CLUT
            let atlas_texture = obj.atlas.to_raster_texture(obj_clut, &format!("atlas_{}", obj_idx));
            let atlas_texture_15 = if use_rgb555 {
                Some(obj.atlas.to_texture15(obj_clut, &format!("atlas_{}", obj_idx)))
            } else {
                None
            };

            // Use project mesh directly (mesh() accessor returns selected object's mesh)
            let obj_mesh = &obj.mesh;

            // Dim non-selected objects slightly
            let base_color = if state.selected_object == Some(obj_idx) {
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
                        editor_alpha: 255,
                    });
                }
            }

            if !vertices.is_empty() && !faces.is_empty() {
                if use_rgb555 {
                    // RGB555 rendering path
                    if let Some(ref tex15) = atlas_texture_15 {
                        let textures_15 = [tex15.clone()];
                        render_mesh_15(
                            fb,
                            &vertices,
                            &faces,
                            &textures_15,
                            &ortho_camera,
                            &ortho_settings,
                            None,
                        );
                    }
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
        // Semi-transparent edge color for solid mode (always visible)
        let edge_overlay_color = Color::from_rgba(80, 80, 80, 191); // 75% opacity gray

        // Draw wireframe edges (n-gon edges)
        // Always draw all edges with semi-transparent overlay, then highlight hover/selected on top
        for (idx, face) in mesh.faces.iter().enumerate() {
            let is_hovered = state.hovered_face == Some(idx);
            let is_selected = matches!(&state.selection, super::state::ModelerSelection::Faces(f) if f.contains(&idx));

            // Choose color: hover/selected get bright colors, others get semi-transparent overlay
            let color = if is_hovered {
                hover_color
            } else if is_selected {
                select_color
            } else if wireframe_mode {
                wire_color
            } else {
                edge_overlay_color
            };
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

        // Draw vertices - always show all vertices with appropriate colors
        // Semi-transparent for solid mode, brighter for wireframe, highlighted for hover/select
        let vertex_overlay_color = Color::from_rgba(60, 60, 70, 140); // Semi-transparent dark
        for (idx, vert) in mesh.vertices.iter().enumerate() {
            let is_hovered = state.hovered_vertex == Some(idx);
            let is_selected = matches!(&state.selection, super::state::ModelerSelection::Vertices(v) if v.contains(&idx));

            let (x, y) = project_vertex(vert);

            // Only draw if in viewport
            if x >= rect.x && x <= rect.right() && y >= rect.y && y <= rect.bottom() {
                let color = if is_hovered {
                    hover_color
                } else if is_selected {
                    select_color
                } else if wireframe_mode {
                    vertex_color
                } else {
                    vertex_overlay_color
                };
                let radius = if is_hovered { 5.0 } else if is_selected { 4.0 } else { 3.0 };
                draw_circle(x, y, radius, color);
            }
        }
    }

    // =========================================================================
    // Draw transform gizmo in ortho views (2-axis version)
    // =========================================================================
    if !state.selection.is_empty() && state.tool_box.active_transform_tool().is_some() {
        if let Some(center) = state.compute_selection_center() {
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

                // Get orientation basis (local or global axes)
                let (local_x, local_y, local_z) = state.compute_orientation_basis();

                // Project local axes to screen coordinates for this ortho view
                // Returns (screen_dx, screen_dy) for each axis - how much the axis moves on screen
                let (axis1_screen, axis2_screen, axis1_world, axis2_world) = match viewport_id {
                    ViewportId::Top => {
                        // Top view: X-Z plane, screen X = world X, screen Y = -world Z
                        ((local_x.x, -local_x.z), (local_z.x, -local_z.z), super::state::Axis::X, super::state::Axis::Z)
                    }
                    ViewportId::Front => {
                        // Front view: X-Y plane, screen X = world X, screen Y = -world Y
                        ((local_x.x, -local_x.y), (local_y.x, -local_y.y), super::state::Axis::X, super::state::Axis::Y)
                    }
                    ViewportId::Side => {
                        // Side view: Z-Y plane, screen X = world Z, screen Y = -world Y
                        ((local_z.z, -local_z.y), (local_y.z, -local_y.y), super::state::Axis::Z, super::state::Axis::Y)
                    }
                    ViewportId::Perspective => ((1.0, 0.0), (0.0, -1.0), super::state::Axis::X, super::state::Axis::Y),
                };

                // Normalize and scale to gizmo length
                let normalize_and_scale = |dx: f32, dy: f32| -> (f32, f32) {
                    let len = (dx * dx + dy * dy).sqrt();
                    if len > 0.001 {
                        (dx / len * gizmo_length, dy / len * gizmo_length)
                    } else {
                        (gizmo_length, 0.0) // Fallback
                    }
                };

                let (dx1, dy1) = normalize_and_scale(axis1_screen.0, axis1_screen.1);
                let (dx2, dy2) = normalize_and_scale(axis2_screen.0, axis2_screen.1);

                let x_end = (cx + dx1, cy + dy1);
                let y_end = (cx + dx2, cy + dy2);

                // Axis colors based on which world axis they represent
                let axis1_color = match axis1_world {
                    super::state::Axis::X => RED,
                    super::state::Axis::Y => GREEN,
                    super::state::Axis::Z => BLUE,
                };
                let axis2_color = match axis2_world {
                    super::state::Axis::X => RED,
                    super::state::Axis::Y => GREEN,
                    super::state::Axis::Z => BLUE,
                };

                // Check which axis is hovered
                let dist_to_x = point_to_line_dist(ctx.mouse.x, ctx.mouse.y, cx, cy, x_end.0, x_end.1);
                let dist_to_y = point_to_line_dist(ctx.mouse.x, ctx.mouse.y, cx, cy, y_end.0, y_end.1);

                let hover_threshold = 8.0;
                let x_hovered = inside_viewport && dist_to_x < hover_threshold && dist_to_x < dist_to_y;
                let y_hovered = inside_viewport && dist_to_y < hover_threshold && dist_to_y < dist_to_x;

                // Update gizmo hover state for ortho views
                if x_hovered {
                    state.ortho_gizmo_hovered_axis = Some(axis1_world);
                } else if y_hovered {
                    state.ortho_gizmo_hovered_axis = Some(axis2_world);
                } else if inside_viewport {
                    state.ortho_gizmo_hovered_axis = None;
                }

                // Draw colors (brighten on hover)
                let x_draw_color = if x_hovered { YELLOW } else { Color::new(axis1_color.r * 0.8, axis1_color.g * 0.8, axis1_color.b * 0.8, 1.0) };
                let y_draw_color = if y_hovered { YELLOW } else { Color::new(axis2_color.r * 0.8, axis2_color.g * 0.8, axis2_color.b * 0.8, 1.0) };

                // Draw gizmo lines
                let line_thickness = 2.0;
                draw_line(cx, cy, x_end.0, x_end.1, line_thickness, x_draw_color);
                draw_line(cx, cy, y_end.0, y_end.1, line_thickness, y_draw_color);

                // Draw arrowheads for move gizmo
                if matches!(state.tool_box.active_transform_tool(), Some(ModelerToolId::Move)) {
                    let arrow_size = 8.0;
                    // Calculate arrow direction based on line direction
                    let draw_arrow = |end: (f32, f32), dx: f32, dy: f32, color: Color| {
                        let len = (dx * dx + dy * dy).sqrt();
                        if len > 0.001 {
                            let nx = dx / len;
                            let ny = dy / len;
                            // Perpendicular
                            let px = -ny;
                            let py = nx;
                            draw_triangle(
                                Vec2::new(end.0, end.1),
                                Vec2::new(end.0 - nx * arrow_size + px * arrow_size * 0.5, end.1 - ny * arrow_size + py * arrow_size * 0.5),
                                Vec2::new(end.0 - nx * arrow_size - px * arrow_size * 0.5, end.1 - ny * arrow_size - py * arrow_size * 0.5),
                                color,
                            );
                        }
                    };
                    draw_arrow(x_end, dx1, dy1, x_draw_color);
                    draw_arrow(y_end, dx2, dy2, y_draw_color);
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
    // Skip if radial menu is open - menu consumes clicks
    if inside_viewport && state.active_viewport == viewport_id && ctx.mouse.left_pressed && state.modal_transform == ModalTransform::None && !state.drag_manager.is_dragging() && state.ortho_gizmo_hovered_axis.is_none() && !state.radial_menu.is_open {
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

                        // Get bone rotation for world-to-local delta transformation (bone-bound meshes)
                        let bone_rotation = state.selected_object()
                            .and_then(|obj| obj.default_bone_index)
                            .map(|bone_idx| state.get_bone_world_transform(bone_idx).1);

                        // Get axis direction from orientation basis if axis is constrained
                        let axis_direction = state.ortho_drag_axis.map(|axis| {
                            let (basis_x, basis_y, basis_z) = state.compute_orientation_basis();
                            match axis {
                                crate::ui::drag_tracker::Axis::X => basis_x,
                                crate::ui::drag_tracker::Axis::Y => basis_y,
                                crate::ui::drag_tracker::Axis::Z => basis_z,
                            }
                        });

                        // Start move drag (constrained if gizmo axis was clicked)
                        state.drag_manager.start_move_with_bone(
                            center,
                            drag_start_mouse,
                            state.ortho_drag_axis, // Use captured axis constraint
                            axis_direction,
                            indices,
                            initial_positions,
                            state.snap_settings.enabled,
                            state.snap_settings.grid_size,
                            bone_rotation,
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
    // Skip if Mesh component is hidden - nothing to select
    let mesh_hidden = state.asset.components.iter()
        .enumerate()
        .find(|(_, c)| matches!(c, crate::asset::AssetComponent::Mesh { .. }))
        .map(|(idx, _)| state.is_component_hidden(idx))
        .unwrap_or(false);
    if mesh_hidden {
        return;
    }

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

    // Pre-compute all bone transforms for per-vertex skinning
    let bone_transforms: Vec<(crate::rasterizer::Vec3, crate::rasterizer::Vec3)> = (0..state.skeleton().len())
        .map(|i| state.get_bone_world_transform(i))
        .collect();

    // Get mesh's default bone index
    let default_bone_idx = state.selected_object().and_then(|obj| obj.default_bone_index);

    let mesh = state.mesh();

    // Helper to transform vertex position to world space (per-vertex bone)
    let get_world_pos = |v: &crate::rasterizer::Vertex| -> crate::rasterizer::Vec3 {
        let bone_idx = v.bone_index.or(default_bone_idx);
        let bone_transform = bone_idx.and_then(|idx| bone_transforms.get(idx)).copied();

        if let Some((bone_pos, bone_rot)) = bone_transform {
            rotate_by_euler(v.pos, bone_rot) + bone_pos
        } else {
            v.pos
        }
    };

    match state.select_mode {
        SelectMode::Vertex => {
            let mut selected = if add_to_selection {
                if let super::state::ModelerSelection::Vertices(v) = &state.selection { v.clone() } else { Vec::new() }
            } else {
                Vec::new()
            };

            for (idx, vert) in mesh.vertices.iter().enumerate() {
                let world_pos = get_world_pos(vert);
                if is_in_box(world_pos) && !selected.contains(&idx) {
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
                        // Check if edge center is in box (using transformed positions)
                        if let (Some(p0), Some(p1)) = (mesh.vertices.get(v0), mesh.vertices.get(v1)) {
                            let center = (get_world_pos(p0) + get_world_pos(p1)) * 0.5;
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
                // Calculate face center using transformed positions
                let world_positions: Vec<_> = face.vertices.iter()
                    .filter_map(|&vi| mesh.vertices.get(vi).map(|v| get_world_pos(v)))
                    .collect();
                if !world_positions.is_empty() {
                    let center = world_positions.iter().fold(crate::rasterizer::Vec3::ZERO, |acc, &p| acc + p)
                        * (1.0 / world_positions.len() as f32);
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
    let atlas = state.atlas();
    let atlas_width = atlas.width;
    let atlas_height = atlas.height;
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

    // Draw the actual texture atlas (effective_clut logic)
    let pixels_per_block = (1.0 / scale).max(1.0) as usize;
    let effective_clut = state.preview_clut
        .and_then(|id| state.clut_pool.get(id))
        .or_else(|| {
            state.objects().first()
                .filter(|obj| obj.atlas.default_clut.is_valid())
                .and_then(|obj| state.clut_pool.get(obj.atlas.default_clut))
        })
        .or_else(|| state.clut_pool.first_id().and_then(|id| state.clut_pool.get(id)));
    if let Some(clut) = effective_clut {
        for by in (0..atlas_height).step_by(pixels_per_block.max(1)) {
            for bx in (0..atlas_width).step_by(pixels_per_block.max(1)) {
                let pixel = state.atlas().get_color(bx, by, clut);
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
            if let Some(atlas) = state.atlas_mut() {
                for dy in 0..brush {
                    for dx in 0..brush {
                        atlas.set_index(px + dx, py + dy, index);
                    }
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
        super::state::ModelerSelection::Bones(bones) => {
            draw_text(&format!("{} bone(s)", bones.len()), rect.x, y + 14.0, 12.0, TEXT_COLOR);
        }
        super::state::ModelerSelection::BoneTips(tips) => {
            draw_text(&format!("{} bone tip(s)", tips.len()), rect.x, y + 14.0, 12.0, TEXT_COLOR);
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

    // Ambient light slider (Light components add point lights on top)
    if y + line_height * 2.0 < rect.bottom() {
        draw_text("Ambient:", rect.x, y + 14.0, 12.0, TEXT_DIM);
        y += line_height;

        let slider_height = 12.0;
        let slider_width = rect.w - 40.0;
        let text_color = Color::from_rgba(204, 204, 204, 255);
        let track_bg = Color::from_rgba(38, 38, 46, 255);
        let tint = Color::from_rgba(230, 217, 102, 255);

        let ambient = state.raster_settings.ambient;
        let ambient_31 = (ambient * 31.0).round() as u8;

        let track_rect = Rect::new(rect.x, y, slider_width, slider_height);
        draw_rectangle(track_rect.x, track_rect.y, track_rect.w, track_rect.h, track_bg);

        let fill_ratio = ambient_31 as f32 / 31.0;
        let fill_width = fill_ratio * slider_width;
        draw_rectangle(track_rect.x, track_rect.y, fill_width, track_rect.h, tint);

        let thumb_x = track_rect.x + fill_width - 1.0;
        draw_rectangle(thumb_x, track_rect.y, 3.0, track_rect.h, WHITE);

        draw_text(&format!("{:2}", ambient_31), rect.x + slider_width + 4.0, y + slider_height - 2.0, 11.0, text_color);

        // Handle slider interaction
        let hovered = ctx.mouse.inside(&track_rect);
        if hovered && ctx.mouse.left_pressed {
            state.ambient_slider_active = true;
        }
        if state.ambient_slider_active && ctx.mouse.left_down {
            let rel_x = (ctx.mouse.x - track_rect.x).clamp(0.0, slider_width);
            let new_val = ((rel_x / slider_width) * 31.0).round() as u8;
            let new_ambient = new_val as f32 / 31.0;
            if (state.raster_settings.ambient - new_ambient).abs() > 0.001 {
                state.raster_settings.ambient = new_ambient;
            }
        }
        if state.ambient_slider_active && !ctx.mouse.left_down {
            state.ambient_slider_active = false;
        }

        y += slider_height + 8.0;
    }
    let _ = y; // Silence unused warning
}

fn draw_timeline(_ctx: &mut UiContext, rect: Rect, _state: &mut ModelerState, _icon_font: Option<&Font>) {
    // Timeline disabled in mesh-only mode
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, HEADER_COLOR);
    draw_text("Timeline (disabled)", rect.x + 10.0, rect.y + 20.0, 14.0, TEXT_DIM);
}

fn draw_status_bar(rect: Rect, state: &ModelerState) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(40, 40, 45, 255));

    // Left side: Green temporary status message
    let status_end_x = if let Some(msg) = state.get_status() {
        let msg_dims = measure_text(msg, None, 14, 1.0);
        draw_text(msg, (rect.x + 10.0).floor(), (rect.y + 15.0).floor(), 14.0, Color::from_rgba(100, 255, 100, 255));
        rect.x + 10.0 + msg_dims.width + 20.0
    } else {
        rect.x + 10.0
    };

    // Right side: Context-sensitive shortcuts
    let mut shortcuts: Vec<&str> = Vec::new();

    // Mode-specific shortcuts
    match state.select_mode {
        SelectMode::Vertex => {
            shortcuts.push("[1] Vertex");
            if !state.selection.is_empty() {
                shortcuts.push("[Alt+M] Merge");
            }
        }
        SelectMode::Edge => {
            shortcuts.push("[2] Edge");
            if !state.selection.is_empty() {
                shortcuts.push("[Alt+L] Loop");
            }
        }
        SelectMode::Face => {
            shortcuts.push("[3] Face");
            if !state.selection.is_empty() {
                shortcuts.push("[E] Extrude");
                shortcuts.push("[Alt+L] Loop");
            }
        }
    }

    // Transform shortcuts when selection exists
    if !state.selection.is_empty() {
        shortcuts.push("[G] Grab");
        shortcuts.push("[R] Rotate");
        shortcuts.push("[T] Scale");
        shortcuts.push("[Del] Delete");
        shortcuts.push("[Tab] Menu");
    }

    // View shortcuts (always available)
    shortcuts.push("[Space] Fullscreen");

    // Vertex linking state
    if state.vertex_linking {
        shortcuts.push("[X] Unlink");
    } else {
        shortcuts.push("[X] Link");
    }

    if !shortcuts.is_empty() {
        let shortcuts_text = shortcuts.join("  ");
        let text_dims = measure_text(&shortcuts_text, None, FONT_SIZE_HEADER as u16, 1.0);
        let text_x = rect.right() - text_dims.width - 10.0;
        let text_y = rect.y + (rect.h + text_dims.height) / 2.0 - 2.0;

        if text_x > status_end_x {
            draw_text(&shortcuts_text, text_x.floor(), text_y.floor(), FONT_SIZE_HEADER, Color::from_rgba(180, 180, 190, 255));
        }
    }
}

// ============================================================================
// UV Transform Functions
// ============================================================================

/// Get UV vertices from selected faces
fn get_uv_vertices_from_selection(state: &ModelerState) -> Vec<usize> {
    let mut verts = std::collections::HashSet::new();
    if let Some(obj) = state.selected_object() {
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
    let obj = state.selected_object()?;
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

    let atlas_size = state.atlas().width as f32;

    state.push_undo(if flip_h { "Flip UV Horizontal" } else { "Flip UV Vertical" });

    if let Some(obj) = state.selected_object_mut() {
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

    // Asset is single source of truth
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

    let atlas_size = state.atlas().width as f32;

    state.push_undo("Rotate UV 90°");

    if let Some(obj) = state.selected_object_mut() {
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

    // Asset is single source of truth
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

        let atlas_size = state.atlas().width as f32;

        state.push_undo("Reset UVs");

        if let Some(obj) = state.selected_object_mut() {
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

/// Auto-unwrap selected faces preserving edge connectivity
fn auto_unwrap_selected_faces(state: &mut ModelerState) {
    if let super::state::ModelerSelection::Faces(faces) = &state.selection.clone() {
        if faces.is_empty() {
            state.set_status("No faces selected", 1.0);
            return;
        }

        let tex_width = state.atlas().width as f32;
        let tex_height = state.atlas().height as f32;

        state.push_undo("Auto Unwrap UVs");

        if let Some(obj) = state.selected_object_mut() {
            super::mesh_editor::auto_unwrap_faces(
                &mut obj.mesh,
                faces,
                tex_width,
                tex_height,
            );
        }

        state.dirty = true;
        state.set_status(&format!("Auto-unwrapped {} faces", faces.len()), 1.0);
    } else {
        state.set_status("Select faces to auto-unwrap", 1.0);
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
    // UV editor is focused when paint section is open and in UV mode
    let uv_editor_focused = state.paint_section_expanded
        && state.texture_editor.mode == crate::texture::TextureEditorMode::Uv;

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
        uv_editor_focused,
        state.clipboard.has_content(),
        state.selected_bone.is_some(),
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
    if actions.triggered("edit.copy", &ctx) {
        copy_selection(state);
    }
    if actions.triggered("edit.paste", &ctx) {
        paste_clipboard(state);
    }
    if actions.triggered("edit.duplicate", &ctx) {
        duplicate_selection(state);
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

    // Loop select (Alt+L) - extends selection along edge/face loops
    if actions.triggered("select.loop", &ctx) {
        select_loop(state);
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
                // Use 2x grid size for clearly visible extrusion
                let extrude_amount = state.snap_settings.grid_size * 2.0;
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
    if actions.triggered("transform.toggle_orientation", &ctx) {
        state.transform_orientation = state.transform_orientation.toggle();
        state.set_status(&format!("Transform orientation: {}", state.transform_orientation.label()), 1.5);
    }

    // ========================================================================
    // Mesh Cleanup Actions
    // ========================================================================
    if actions.triggered("mesh.merge_by_distance", &ctx) {
        let threshold = state.snap_settings.grid_size * 0.1; // 10% of grid size
        state.push_undo("Merge by Distance");
        let merged = if let Some(mesh) = state.mesh_mut() {
            mesh.merge_by_distance(threshold)
        } else {
            0
        };
        if merged > 0 {
            state.dirty = true;
            state.set_status(&format!("Merged {} vertices (threshold: {:.1})", merged, threshold), 2.0);
        } else {
            state.set_status("No overlapping vertices found", 1.5);
        }
    }

    if actions.triggered("mesh.merge_to_center", &ctx) {
        if let super::state::ModelerSelection::Vertices(vert_indices) = &state.selection {
            if vert_indices.len() >= 2 {
                let indices = vert_indices.clone();
                state.push_undo("Merge to Center");
                let result = if let Some(mesh) = state.mesh_mut() {
                    mesh.merge_to_center(&indices)
                } else {
                    None
                };
                if let Some(kept_idx) = result {
                    // Select the merged vertex
                    state.selection = super::state::ModelerSelection::Vertices(vec![kept_idx]);
                    // Clean up orphaned vertices
                    if let Some(mesh) = state.mesh_mut() {
                        mesh.compact_vertices();
                    }
                    state.dirty = true;
                    state.set_status(&format!("Merged {} vertices to center", indices.len()), 1.5);
                }
            } else {
                state.set_status("Select 2+ vertices to merge", 1.0);
            }
        } else {
            state.set_status("Switch to Vertex mode (1) to merge", 1.0);
        }
    }

    // ========================================================================
    // Skeleton / Bone Binding Actions
    // ========================================================================
    if actions.triggered("skeleton.bind_vertices_to_bone", &ctx) {
        if let Some(bone_idx) = state.selected_bone {
            state.assign_selected_vertices_to_bone(bone_idx);
        }
    }
    if actions.triggered("skeleton.unbind_vertices", &ctx) {
        state.unassign_selected_vertices();
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
    if actions.triggered("view.toggle_xray", &ctx) {
        state.xray_mode = !state.xray_mode;
        state.raster_settings.xray_mode = state.xray_mode;
        let mode = if state.xray_mode { "ON" } else { "OFF" };
        state.set_status(&format!("X-Ray: {}", mode), 1.0);
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
        // Toggle between UV and Paint modes
        if state.texture_editor.mode == TextureEditorMode::Paint {
            state.texture_editor.mode = TextureEditorMode::Uv;
            state.set_status("UV mode", 1.0);
        } else {
            state.texture_editor.mode = TextureEditorMode::Paint;
            state.texture_editor.uv_selection.clear();
            // Initialize editing texture when switching to paint
            if state.editing_texture.is_none() {
                state.editing_texture = Some(create_editing_texture(state));
                state.texture_editor.reset();
            }
            state.set_status("Paint mode", 1.0);
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
    if actions.triggered("uv.auto_unwrap", &ctx) {
        auto_unwrap_selected_faces(state);
    }

    // ========================================================================
    // Context Menu Actions
    // ========================================================================
    if actions.triggered("context.open_menu", &ctx) {
        if !state.radial_menu.is_open {
            // Tab key opens radial menu at mouse position (hold behavior)
            let (mx, my) = (ui_ctx.mouse.x, ui_ctx.mouse.y);
            open_radial_menu(state, mx, my);
        }
    }

    // Tab release: enter submenu if has children, otherwise close and select
    // Use is_key_released so this only fires once, not every frame
    if state.radial_menu.is_open && is_key_released(KeyCode::Tab) {
        // Check if highlighted item has children (submenu)
        let has_children = state.radial_menu.highlighted
            .and_then(|idx| state.radial_menu.items.get(idx))
            .map(|item| !item.children.is_empty())
            .unwrap_or(false);

        if has_children {
            // Enter submenu, keep menu open
            if let Some(idx) = state.radial_menu.highlighted {
                state.radial_menu.enter_submenu(idx);
            }
        } else if state.radial_menu.highlighted.is_none() {
            // In center zone - check for back vs exit in submenu
            let in_submenu = !state.radial_menu.menu_stack.is_empty();
            let (cx, cy) = state.radial_menu.center;
            let mouse_on_left = ui_ctx.mouse.x < cx;

            if in_submenu && mouse_on_left {
                // Back to parent menu
                state.radial_menu.back();
            } else {
                // Exit/cancel
                state.radial_menu.close(false);
            }
        } else {
            // Close and select
            if let Some(selected_id) = state.radial_menu.close(true) {
                handle_radial_menu_action(state, &selected_id);
            }
        }
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
        } else if !state.selection.is_empty() {
            // Clear selection if nothing else to cancel
            state.set_selection(super::state::ModelerSelection::None);
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

/// Select an edge or face loop based on current selection
fn select_loop(state: &mut ModelerState) {
    use std::collections::HashSet;

    let mesh = state.mesh();
    let selection = state.selection.clone();

    match selection {
        super::state::ModelerSelection::Vertices(verts) => {
            // If exactly 2 vertices selected, treat as an edge and select edge loop
            if verts.len() == 2 {
                let v0 = verts[0];
                let v1 = verts[1];

                // Check if these form an edge (are adjacent in some face)
                let is_edge = mesh.faces.iter().any(|face| {
                    let fv = &face.vertices;
                    let n = fv.len();
                    for i in 0..n {
                        let a = fv[i];
                        let b = fv[(i + 1) % n];
                        if (a == v0 && b == v1) || (a == v1 && b == v0) {
                            return true;
                        }
                    }
                    false
                });

                if is_edge {
                    let loop_edges = mesh.select_edge_loop(v0, v1);
                    let loop_verts = mesh.vertices_from_edge_loop(&loop_edges);
                    let count = loop_verts.len();
                    state.set_selection(super::state::ModelerSelection::Vertices(loop_verts));
                    state.set_status(&format!("Selected edge loop ({} vertices)", count), 1.5);
                } else {
                    state.set_status("Selected vertices don't form an edge", 1.5);
                }
            } else if verts.len() == 1 {
                // Single vertex: try to find a loop through connected edges
                // For now, select all vertices connected by edges to this one
                let v = verts[0];
                let mut connected: HashSet<usize> = HashSet::new();
                connected.insert(v);

                for face in &mesh.faces {
                    if face.vertices.contains(&v) {
                        for &fv in &face.vertices {
                            connected.insert(fv);
                        }
                    }
                }

                let connected_verts: Vec<usize> = connected.into_iter().collect();
                let count = connected_verts.len();
                state.set_selection(super::state::ModelerSelection::Vertices(connected_verts));
                state.set_status(&format!("Selected {} connected vertices", count), 1.5);
            } else {
                state.set_status("Select 2 adjacent vertices to select edge loop", 1.5);
            }
        }

        super::state::ModelerSelection::Edges(edges) => {
            if edges.len() == 1 {
                let (v0, v1) = edges[0];
                let loop_edges = mesh.select_edge_loop(v0, v1);
                let count = loop_edges.len();
                state.set_selection(super::state::ModelerSelection::Edges(loop_edges));
                state.set_status(&format!("Selected edge loop ({} edges)", count), 1.5);
            } else {
                state.set_status("Select a single edge to select edge loop", 1.5);
            }
        }

        super::state::ModelerSelection::Faces(faces) => {
            if faces.len() == 1 {
                let face_idx = faces[0];
                let face = &mesh.faces[face_idx];

                // For face loop, we need a direction. Use first edge of the face.
                if face.vertices.len() >= 2 {
                    let v0 = face.vertices[0];
                    let v1 = face.vertices[1];
                    let loop_faces = mesh.select_face_loop(face_idx, v0, v1);
                    let count = loop_faces.len();
                    state.set_selection(super::state::ModelerSelection::Faces(loop_faces));
                    state.set_status(&format!("Selected face loop ({} faces)", count), 1.5);
                } else {
                    state.set_status("Face has no edges", 1.5);
                }
            } else {
                state.set_status("Select a single face to select face loop", 1.5);
            }
        }

        super::state::ModelerSelection::None | super::state::ModelerSelection::Mesh | super::state::ModelerSelection::Bones(_) | super::state::ModelerSelection::BoneTips(_) => {
            state.set_status("No selection for loop select", 1.0);
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

    // Check if mesh is now empty and remove the object if so
    if let Some(idx) = state.selected_object {
        let is_empty = state.objects().get(idx)
            .map(|o| o.mesh.faces.is_empty())
            .unwrap_or(false);

        if is_empty {
            let name = state.objects().get(idx)
                .map(|o| o.name.clone())
                .unwrap_or_default();

            if let Some(objects) = state.objects_mut() {
                objects.remove(idx);
            }

            // Update selected_object to point to a valid object
            if state.objects().is_empty() {
                state.selected_object = None;
            } else if idx >= state.objects().len() {
                // Select the last object if we removed the last one
                state.selected_object = Some(state.objects().len() - 1);
            }
            // If idx is still valid, keep it (now points to the next object)

            state.set_status(&format!("Deleted object '{}'", name), 1.5);
        }
    }
}

/// Copy current selection to clipboard
fn copy_selection(state: &mut ModelerState) {
    let selection = state.selection.clone();
    let mesh = state.mesh().clone();

    match selection {
        super::state::ModelerSelection::Faces(face_indices) => {
            if face_indices.is_empty() {
                state.set_status("No faces selected to copy", 1.0);
                return;
            }
            state.clipboard.copy_faces(&mesh, &face_indices);
            state.set_status(&format!("Copied {} face(s)", face_indices.len()), 1.0);
        }
        super::state::ModelerSelection::Vertices(_) |
        super::state::ModelerSelection::Edges(_) => {
            // For vertices/edges, copy the entire mesh for now
            // Could be improved to copy just the affected geometry
            state.clipboard.copy_mesh(&mesh);
            state.set_status("Copied mesh", 1.0);
        }
        _ => {
            // No selection - copy entire mesh
            state.clipboard.copy_mesh(&mesh);
            state.set_status("Copied entire mesh", 1.0);
        }
    }
}

/// Paste clipboard contents as a new object
fn paste_clipboard(state: &mut ModelerState) {
    if !state.clipboard.has_content() {
        state.set_status("Clipboard empty", 1.0);
        return;
    }

    let clipboard_mesh = state.clipboard.mesh.clone().unwrap();
    let clipboard_center = state.clipboard.center;

    state.push_undo("Paste");

    // Create new mesh offset to a point in front of the camera
    let mut new_mesh = clipboard_mesh;
    let camera_pos = state.camera.position;
    let camera_forward = state.camera.basis_z;
    let paste_target = camera_pos + camera_forward * 500.0; // 500 units in front
    let offset = paste_target - clipboard_center;

    for vert in &mut new_mesh.vertices {
        vert.pos.x += offset.x;
        vert.pos.y += offset.y;
        vert.pos.z += offset.z;
    }

    let name = state.generate_unique_object_name("Pasted");
    let obj = MeshPart::with_mesh(&name, new_mesh);
    state.add_object(obj);
    state.set_status("Pasted as new object", 1.0);
}

/// Duplicate selection (copy + paste in one operation)
fn duplicate_selection(state: &mut ModelerState) {
    let selection = state.selection.clone();
    let mesh = state.mesh().clone();

    match selection {
        super::state::ModelerSelection::Faces(face_indices) => {
            if face_indices.is_empty() {
                state.set_status("No faces selected to duplicate", 1.0);
                return;
            }
            state.push_undo("Duplicate");

            // Copy faces to clipboard and paste immediately
            state.clipboard.copy_faces(&mesh, &face_indices);
            let clipboard_mesh = state.clipboard.mesh.clone().unwrap();

            // Offset slightly so duplicate is visible
            let mut new_mesh = clipboard_mesh;
            for vert in &mut new_mesh.vertices {
                vert.pos.x += 100.0;
                vert.pos.z += 100.0;
            }

            let name = state.generate_unique_object_name("Duplicate");
            let obj = MeshPart::with_mesh(&name, new_mesh);
            state.add_object(obj);
            state.set_status(&format!("Duplicated {} face(s)", face_indices.len()), 1.0);
        }
        _ => {
            // Duplicate entire mesh
            state.push_undo("Duplicate mesh");
            state.clipboard.copy_mesh(&mesh);
            let clipboard_mesh = state.clipboard.mesh.clone().unwrap();

            let mut new_mesh = clipboard_mesh;
            for vert in &mut new_mesh.vertices {
                vert.pos.x += 100.0;
                vert.pos.z += 100.0;
            }

            let name = state.generate_unique_object_name("Duplicate");
            let obj = MeshPart::with_mesh(&name, new_mesh);
            state.add_object(obj);
            state.set_status("Duplicated mesh", 1.0);
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

/// Draw the "Add Component" popup at the top level (above all panels)
fn draw_add_component_popup(ctx: &mut UiContext, _left_rect: Rect, state: &mut ModelerState, icon_font: Option<&Font>) {
    let trigger_rect = match state.dropdown.trigger_rect {
        Some(r) if state.dropdown.is_open("add_component") => r,
        _ => return,
    };

    // All component types that can be added
    let component_types = [
        ("Mesh", icon::BOX),
        ("Skeleton", icon::BONE),
        ("Collision", icon::SCAN),
        ("Light", icon::SUN),
        ("Trigger", icon::MAP_PIN),
        ("Pickup", icon::PLUS),
        ("Enemy", icon::PERSON_STANDING),
        ("Door", icon::DOOR_CLOSED),
        ("Audio", icon::MUSIC),
        ("Particle", icon::BLEND),
        ("CharacterController", icon::GAMEPAD_2),
        ("SpawnPoint", icon::FOOTPRINTS),
    ];

    let item_height = 20.0;
    let menu_rect = dropdown_menu_rect(trigger_rect, component_types.len(), item_height, Some(140.0));

    if !begin_dropdown(ctx, &mut state.dropdown, "add_component", menu_rect) {
        return;
    }

    let mut item_y = menu_rect.y + 2.0;
    for (type_name, icon_char) in component_types {
        let item_rect = Rect::new(menu_rect.x + 2.0, item_y, menu_rect.w - 4.0, item_height);

        if dropdown_item(ctx, item_rect, type_name, Some((icon_char, icon_font)), false) {
            let is_collision = type_name == "Collision";
            let is_skeleton_type = type_name == "Skeleton";
            let mut new_component = create_default_component(type_name);

            // For Collision, create a linked MeshPart (cube by default)
            if is_collision {
                let mesh_name = state.generate_unique_object_name("Collision");
                let collision_obj = MeshPart::with_mesh(&mesh_name, EditableMesh::cube(512.0));
                let obj_idx = state.add_object(collision_obj);
                // Update the component with the linked mesh name
                if let AssetComponent::Collision { collision_mesh, .. } = &mut new_component {
                    *collision_mesh = Some(mesh_name);
                }
                state.selected_object = Some(obj_idx);
            }

            state.asset.components.push(new_component);
            state.selected_component = Some(state.asset.components.len() - 1);
            state.dropdown.close();

            // For Skeleton, also select the default Root bone
            if is_skeleton_type {
                state.selected_bone = Some(0);
                state.selection = super::state::ModelerSelection::Bones(vec![0]);
                state.set_status("Created skeleton with Root bone", 1.0);
            }
            if is_collision {
                state.set_status("Added Collision with editable mesh", 1.0);
            }
        }

        item_y += item_height;
    }
}

/// Draw the bone picker popup for mesh-to-bone assignment
fn draw_bone_picker_popup(ctx: &mut UiContext, _left_rect: Rect, state: &mut ModelerState, icon_font: Option<&Font>) {
    let trigger_rect = match state.dropdown.trigger_rect {
        Some(r) if state.dropdown.is_open("bone_picker") => r,
        _ => return,
    };

    let target_mesh = match state.bone_picker_target_mesh {
        Some(idx) => idx,
        None => {
            state.dropdown.close();
            return;
        }
    };

    // Collect bone names first to avoid borrow issues
    let bone_names: Vec<String> = state.skeleton().iter().map(|b| b.name.clone()).collect();
    if bone_names.is_empty() {
        state.dropdown.close();
        return;
    }

    // Get current bone index for highlighting
    let current_bone_index = state.objects()
        .get(target_mesh)
        .and_then(|obj| obj.default_bone_index);

    // +1 for "(None)" option
    let item_height = 20.0;
    let menu_rect = dropdown_menu_rect(trigger_rect, bone_names.len() + 1, item_height, Some(140.0));

    if !begin_dropdown(ctx, &mut state.dropdown, "bone_picker", menu_rect) {
        return;
    }

    let mut item_y = menu_rect.y + 2.0;

    // "(None)" option first - unbind from bone
    {
        let item_rect = Rect::new(menu_rect.x + 2.0, item_y, menu_rect.w - 4.0, item_height);
        let is_selected = current_bone_index.is_none();

        if dropdown_item(ctx, item_rect, "(None)", None, is_selected) {
            // Get current bone transform BEFORE mutating, so we can convert vertices back to world space
            let old_bone_transform = current_bone_index.map(|idx| state.get_bone_world_transform(idx));

            state.push_undo("Unbind bone");
            if let Some(obj) = state.objects_mut().and_then(|v| v.get_mut(target_mesh)) {
                // Convert vertices from bone-local space back to world space
                if let Some((bone_pos, bone_rot)) = old_bone_transform {
                    for v in &mut obj.mesh.vertices {
                        // Rotate by bone rotation, then translate by bone position
                        let rotated = rotate_by_euler(v.pos, bone_rot);
                        v.pos = rotated + bone_pos;
                        // Rotate normal too (no translation for normals)
                        v.normal = rotate_by_euler(v.normal, bone_rot);
                    }
                }
                obj.default_bone_index = None;
            }
            state.dirty = true;
            state.dropdown.close();
            state.set_status("Unbound mesh from bone", 1.0);
        }

        item_y += item_height;
    }

    // List all bones
    for (bone_idx, bone_name) in bone_names.iter().enumerate() {
        let item_rect = Rect::new(menu_rect.x + 2.0, item_y, menu_rect.w - 4.0, item_height);
        let is_selected = current_bone_index == Some(bone_idx);

        if dropdown_item(ctx, item_rect, bone_name, Some((icon::BONE, icon_font)), is_selected) {
            // Get transforms BEFORE mutating state
            // Old bone transform (if previously bound to a different bone)
            let old_bone_transform = current_bone_index
                .filter(|&old_idx| old_idx != bone_idx)
                .map(|idx| state.get_bone_world_transform(idx));
            // New bone transform
            let (new_bone_pos, new_bone_rot) = state.get_bone_world_transform(bone_idx);

            state.push_undo("Assign bone");
            if let Some(obj) = state.objects_mut().and_then(|v| v.get_mut(target_mesh)) {
                // If already bound to a different bone, first convert to world space
                if let Some((old_pos, old_rot)) = old_bone_transform {
                    for v in &mut obj.mesh.vertices {
                        let rotated = rotate_by_euler(v.pos, old_rot);
                        v.pos = rotated + old_pos;
                        v.normal = rotate_by_euler(v.normal, old_rot);
                    }
                }

                // Now convert from world space to new bone-local space
                // Only if not already bound (or was bound to different bone, which we just converted above)
                if current_bone_index != Some(bone_idx) {
                    for v in &mut obj.mesh.vertices {
                        // Translate to bone origin, then inverse-rotate
                        let relative = v.pos - new_bone_pos;
                        v.pos = inverse_rotate_by_euler(relative, new_bone_rot);
                        // Inverse-rotate normal (no translation for normals)
                        v.normal = inverse_rotate_by_euler(v.normal, new_bone_rot);
                    }
                }

                obj.default_bone_index = Some(bone_idx);
            }
            state.dirty = true;
            state.dropdown.close();
            state.set_status(&format!("Bound mesh to '{}'", bone_name), 1.0);
        }

        item_y += item_height;
    }
}

/// Draw and handle context menu
fn draw_context_menu(ctx: &mut UiContext, state: &mut ModelerState) {
    use super::state::ContextMenuType;
    // Note: Tab/Escape shortcuts are now handled through ActionRegistry in handle_actions()

    let menu = match &state.context_menu {
        Some(m) => m.clone(),
        None => return,
    };

    // Dispatch based on menu type
    match menu.menu_type {
        ContextMenuType::Primitives => draw_primitives_context_menu(ctx, state, &menu),
        ContextMenuType::VertexOps => draw_vertex_ops_context_menu(ctx, state, &menu),
        ContextMenuType::FaceOps | ContextMenuType::EdgeOps => {
            // Face/edge modes share the vertex ops menu for bone assignment
            draw_vertex_ops_context_menu(ctx, state, &menu)
        }
    }
}

/// Draw vertex operations context menu (bone assignment, etc.)
fn draw_vertex_ops_context_menu(ctx: &mut UiContext, state: &mut ModelerState, menu: &super::state::ContextMenu) {
    let item_height = 24.0;
    let menu_width = 160.0;

    // Get skeleton bones for the submenu
    let skeleton = state.skeleton();
    let bone_count = skeleton.len();
    let has_bones = bone_count > 0;

    // Calculate menu height
    let header_height = item_height;
    let assign_item_height = if has_bones { item_height + (bone_count as f32 * item_height) } else { item_height };
    let unbind_height = item_height;
    let menu_height = header_height + assign_item_height + unbind_height + 16.0;

    // Keep menu on screen
    let menu_x = menu.x.min(screen_width() - menu_width - 5.0);
    let menu_y = menu.y.min(screen_height() - menu_height - 5.0);

    let menu_rect = Rect::new(menu_x, menu_y, menu_width, menu_height);

    // Draw menu background
    draw_rectangle(menu_rect.x - 1.0, menu_rect.y - 1.0, menu_rect.w + 2.0, menu_rect.h + 2.0, Color::from_rgba(80, 80, 85, 255));
    draw_rectangle(menu_rect.x, menu_rect.y, menu_rect.w, menu_rect.h, Color::from_rgba(45, 45, 50, 255));

    let mut y = menu_rect.y + 4.0;

    // Header showing vertex count (derived from faces/edges if needed)
    let vert_count = match &state.selection {
        super::state::ModelerSelection::Vertices(v) => v.len(),
        super::state::ModelerSelection::Faces(faces) => {
            let mesh = state.mesh();
            let mut verts: std::collections::HashSet<usize> = std::collections::HashSet::new();
            for &face_idx in faces {
                if let Some(face) = mesh.faces.get(face_idx) {
                    verts.extend(face.vertices.iter().copied());
                }
            }
            verts.len()
        }
        super::state::ModelerSelection::Edges(edges) => {
            let mut verts: std::collections::HashSet<usize> = std::collections::HashSet::new();
            for &(a, b) in edges {
                verts.insert(a);
                verts.insert(b);
            }
            verts.len()
        }
        _ => 0,
    };
    draw_text(&format!("{} vertices selected", vert_count), menu_rect.x + 8.0, y + 14.0, 12.0, TEXT_DIM);
    y += item_height;

    // Track actions
    let mut assign_to_bone: Option<usize> = None;
    let mut unbind_clicked = false;
    let mut new_hovered_bone: Option<usize> = None;

    if has_bones {
        // "Assign to Bone" section header
        draw_text("Assign to Bone:", menu_rect.x + 8.0, y + 14.0, 12.0, ACCENT_COLOR);
        y += item_height;

        // List all bones
        let skeleton = state.skeleton();
        for (idx, bone) in skeleton.iter().enumerate() {
            let item_rect = Rect::new(menu_rect.x + 2.0, y, menu_width - 4.0, item_height);

            // Hover highlight
            let is_hovered = ctx.mouse.inside(&item_rect);
            if is_hovered {
                draw_rectangle(item_rect.x, item_rect.y, item_rect.w, item_rect.h, Color::from_rgba(60, 80, 100, 255));
                new_hovered_bone = Some(idx);

                if ctx.mouse.left_pressed {
                    assign_to_bone = Some(idx);
                }
            }

            // Bone icon and name
            let icon_color = if bone.parent.is_none() {
                Color::from_rgba(255, 220, 100, 255) // Yellow for root
            } else {
                TEXT_COLOR
            };
            draw_text("◆", item_rect.x + 8.0, item_rect.y + 15.0, 10.0, icon_color);
            draw_text(&bone.name, item_rect.x + 22.0, item_rect.y + 16.0, 14.0, TEXT_COLOR);

            y += item_height;
        }
    } else {
        // No bones available
        let item_rect = Rect::new(menu_rect.x + 2.0, y, menu_width - 4.0, item_height);
        draw_text("No bones (add skeleton)", item_rect.x + 8.0, item_rect.y + 16.0, 12.0, TEXT_DIM);
        y += item_height;
    }

    // Separator
    y += 4.0;
    draw_line(menu_rect.x + 8.0, y, menu_rect.right() - 8.0, y, 1.0, Color::from_rgba(70, 70, 75, 255));
    y += 8.0;

    // "Unbind from Bone" option
    let unbind_rect = Rect::new(menu_rect.x + 2.0, y, menu_width - 4.0, item_height);
    if ctx.mouse.inside(&unbind_rect) {
        draw_rectangle(unbind_rect.x, unbind_rect.y, unbind_rect.w, unbind_rect.h, Color::from_rgba(60, 60, 70, 255));
        if ctx.mouse.left_pressed {
            unbind_clicked = true;
        }
    }
    draw_text("Unbind from Bone", unbind_rect.x + 8.0, unbind_rect.y + 16.0, 14.0, TEXT_COLOR);

    // Update hovered bone for viewport highlighting
    if let Some(cm) = &mut state.context_menu {
        cm.hovered_bone = new_hovered_bone;
    }
    // Also set hovered_bone on state for 3D highlighting
    state.hovered_bone = new_hovered_bone;

    // Handle actions
    if let Some(bone_idx) = assign_to_bone {
        state.assign_selected_vertices_to_bone(bone_idx);
        state.context_menu = None;
    }

    if unbind_clicked {
        state.unassign_selected_vertices();
        state.context_menu = None;
    }

    // Close if clicked outside menu
    if ctx.mouse.left_pressed && !ctx.mouse.inside(&menu_rect) {
        state.context_menu = None;
        state.hovered_bone = None;
    }
}

/// Draw primitives context menu (original functionality)
fn draw_primitives_context_menu(ctx: &mut UiContext, state: &mut ModelerState, menu: &super::state::ContextMenu) {

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
        let mut new_mesh = prim.create(size);

        // Offset mesh vertices to the clicked world position
        for vert in &mut new_mesh.vertices {
            vert.pos.x += menu.world_pos.x;
            vert.pos.y += menu.world_pos.y;
            vert.pos.z += menu.world_pos.z;
        }

        // Create new object with unique name
        let base_name = prim.label().split_whitespace().next().unwrap_or("Object");
        let name = state.generate_unique_object_name(base_name);
        let obj = MeshPart::with_mesh(&name, new_mesh);
        state.add_object(obj);
        state.set_status(&format!("Added {} as new object", prim.label()), 1.0);
        state.context_menu = None;
    }

    if clone_clicked {
        state.push_undo("Clone object");
        // Clone entire object at offset as a new object
        let offset = Vec3::new(
            state.snap_settings.grid_size * 2.0,
            0.0,
            state.snap_settings.grid_size * 2.0,
        );
        let mut clone = state.mesh().clone();
        // Apply offset to cloned mesh
        for vert in &mut clone.vertices {
            vert.pos.x += offset.x;
            vert.pos.y += offset.y;
            vert.pos.z += offset.z;
        }
        // Generate unique name based on source object
        let source_name = state.selected_object()
            .map(|o| o.name.as_str())
            .unwrap_or("Object");
        let name = state.generate_unique_object_name(source_name);
        let obj = MeshPart::with_mesh(&name, clone);
        state.add_object(obj);
        state.set_status("Cloned as new object", 1.0);
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

/// Draw rename and delete dialogs for objects
fn draw_object_dialogs(ctx: &mut UiContext, state: &mut ModelerState, icon_font: Option<&Font>) {
    // Handle rename dialog
    if state.rename_dialog.is_some() {
        let dialog_w = 280.0;
        let dialog_h = 120.0;
        let dialog_x = (screen_width() - dialog_w) / 2.0;
        let dialog_y = (screen_height() - dialog_h) / 2.0;

        // Background
        draw_rectangle(dialog_x, dialog_y, dialog_w, dialog_h, Color::from_rgba(45, 45, 50, 255));
        draw_rectangle_lines(dialog_x, dialog_y, dialog_w, dialog_h, 2.0, Color::from_rgba(80, 80, 90, 255));

        // Title
        draw_text("Rename Object", dialog_x + 12.0, dialog_y + 22.0, 16.0, WHITE);

        // Text input field - use the new widget
        let input_rect = Rect::new(dialog_x + 12.0, dialog_y + 40.0, dialog_w - 24.0, 28.0);
        if let Some((_, ref mut input_state)) = state.rename_dialog {
            draw_text_input(input_rect, input_state, 14.0);
        }

        // Buttons
        let btn_w = 80.0;
        let btn_h = 28.0;
        let btn_y = dialog_y + dialog_h - btn_h - 12.0;

        // Cancel button
        let cancel_rect = Rect::new(dialog_x + dialog_w - btn_w * 2.0 - 20.0, btn_y, btn_w, btn_h);
        let cancel_hover = ctx.mouse.inside(&cancel_rect);
        draw_rectangle(cancel_rect.x, cancel_rect.y, cancel_rect.w, cancel_rect.h,
            if cancel_hover { Color::from_rgba(70, 70, 75, 255) } else { Color::from_rgba(55, 55, 60, 255) });
        draw_text("Cancel", cancel_rect.x + 18.0, cancel_rect.y + 18.0, 14.0, TEXT_COLOR);

        // Confirm button
        let confirm_rect = Rect::new(dialog_x + dialog_w - btn_w - 12.0, btn_y, btn_w, btn_h);
        let confirm_hover = ctx.mouse.inside(&confirm_rect);
        draw_rectangle(confirm_rect.x, confirm_rect.y, confirm_rect.w, confirm_rect.h,
            if confirm_hover { Color::from_rgba(60, 100, 140, 255) } else { ACCENT_COLOR });
        draw_text("Rename", confirm_rect.x + 14.0, confirm_rect.y + 18.0, 14.0, WHITE);

        // Handle button clicks
        if ctx.mouse.clicked(&cancel_rect) || is_key_pressed(KeyCode::Escape) {
            state.rename_dialog = None;
        } else if ctx.mouse.clicked(&confirm_rect) || is_key_pressed(KeyCode::Enter) {
            // Apply the rename
            if let Some((idx, ref input_state)) = state.rename_dialog {
                let name = input_state.text.clone();
                if !name.is_empty() && idx < state.objects().len() {
                    if let Some(obj) = state.objects_mut().and_then(|v| v.get_mut(idx)) {
                        obj.name = name.clone();
                    }
                    state.set_status(&format!("Renamed to '{}'", name), 1.0);
                }
            }
            state.rename_dialog = None;
        }
    }

    // Handle delete confirmation dialog
    if let Some(idx) = state.delete_dialog {
        let obj_name = state.objects().get(idx)
            .map(|o| o.name.clone())
            .unwrap_or_default();

        let dialog_w = 300.0;
        let dialog_h = 120.0;
        let dialog_x = (screen_width() - dialog_w) / 2.0;
        let dialog_y = (screen_height() - dialog_h) / 2.0;
        let _dialog_rect = Rect::new(dialog_x, dialog_y, dialog_w, dialog_h);

        // Background
        draw_rectangle(dialog_x, dialog_y, dialog_w, dialog_h, Color::from_rgba(45, 45, 50, 255));
        draw_rectangle_lines(dialog_x, dialog_y, dialog_w, dialog_h, 2.0, Color::from_rgba(100, 60, 60, 255));

        // Warning icon and title
        let icon_rect = Rect::new(dialog_x + 12.0, dialog_y + 12.0, 20.0, 20.0);
        draw_icon_centered(icon_font, icon::TRASH, &icon_rect, 16.0, Color::from_rgba(255, 100, 100, 255));
        draw_text("Delete Object?", dialog_x + 36.0, dialog_y + 26.0, 16.0, WHITE);

        // Message
        draw_text(&format!("Delete '{}'?", obj_name), dialog_x + 12.0, dialog_y + 55.0, 14.0, TEXT_COLOR);
        draw_text("This cannot be undone.", dialog_x + 12.0, dialog_y + 72.0, 12.0, TEXT_DIM);

        // Buttons
        let btn_w = 80.0;
        let btn_h = 28.0;
        let btn_y = dialog_y + dialog_h - btn_h - 12.0;

        // Cancel button
        let cancel_rect = Rect::new(dialog_x + dialog_w - btn_w * 2.0 - 20.0, btn_y, btn_w, btn_h);
        let cancel_hover = ctx.mouse.inside(&cancel_rect);
        draw_rectangle(cancel_rect.x, cancel_rect.y, cancel_rect.w, cancel_rect.h,
            if cancel_hover { Color::from_rgba(70, 70, 75, 255) } else { Color::from_rgba(55, 55, 60, 255) });
        draw_text("Cancel", cancel_rect.x + 18.0, cancel_rect.y + 18.0, 14.0, TEXT_COLOR);

        // Delete button (red)
        let delete_rect = Rect::new(dialog_x + dialog_w - btn_w - 12.0, btn_y, btn_w, btn_h);
        let delete_hover = ctx.mouse.inside(&delete_rect);
        draw_rectangle(delete_rect.x, delete_rect.y, delete_rect.w, delete_rect.h,
            if delete_hover { Color::from_rgba(180, 60, 60, 255) } else { Color::from_rgba(140, 50, 50, 255) });
        draw_text("Delete", delete_rect.x + 18.0, delete_rect.y + 18.0, 14.0, WHITE);

        // Handle button clicks (use clicked() for buttons, not left_pressed)
        if ctx.mouse.clicked(&cancel_rect) {
            state.delete_dialog = None;
        } else if ctx.mouse.clicked(&delete_rect) {
            // Delete the object
            if idx < state.objects().len() {
                if let Some(objects) = state.objects_mut() {
                    objects.remove(idx);
                }
                // Update selected_object
                if state.objects().is_empty() {
                    state.selected_object = None;
                } else if let Some(sel) = state.selected_object {
                    if sel >= state.objects().len() {
                        state.selected_object = Some(state.objects().len() - 1);
                    } else if sel > idx {
                        state.selected_object = Some(sel - 1);
                    }
                }
                state.selection.clear();
                state.dirty = true;
                state.set_status(&format!("Deleted '{}'", obj_name), 1.0);
            }
            state.delete_dialog = None;
        }

        // Handle Escape key
        if is_key_pressed(KeyCode::Escape) {
            state.delete_dialog = None;
        }
    }

    // Handle component delete confirmation dialog
    if let Some(idx) = state.delete_component_dialog {
        let comp_name = state.asset.components.get(idx)
            .map(|c| c.type_name().to_string())
            .unwrap_or_default();

        let dialog_w = 300.0;
        let dialog_h = 120.0;
        let dialog_x = (screen_width() - dialog_w) / 2.0;
        let dialog_y = (screen_height() - dialog_h) / 2.0;

        // Background
        draw_rectangle(dialog_x, dialog_y, dialog_w, dialog_h, Color::from_rgba(45, 45, 50, 255));
        draw_rectangle_lines(dialog_x, dialog_y, dialog_w, dialog_h, 2.0, Color::from_rgba(100, 60, 60, 255));

        // Warning icon and title
        let icon_rect = Rect::new(dialog_x + 12.0, dialog_y + 12.0, 20.0, 20.0);
        draw_icon_centered(icon_font, icon::TRASH, &icon_rect, 16.0, Color::from_rgba(255, 100, 100, 255));
        draw_text("Delete Component?", dialog_x + 36.0, dialog_y + 26.0, 16.0, WHITE);

        // Message
        draw_text(&format!("Delete '{}' component?", comp_name), dialog_x + 12.0, dialog_y + 55.0, 14.0, TEXT_COLOR);
        draw_text("This cannot be undone.", dialog_x + 12.0, dialog_y + 72.0, 12.0, TEXT_DIM);

        // Buttons
        let btn_w = 80.0;
        let btn_h = 28.0;
        let btn_y = dialog_y + dialog_h - btn_h - 12.0;

        // Cancel button
        let cancel_rect = Rect::new(dialog_x + dialog_w - btn_w * 2.0 - 20.0, btn_y, btn_w, btn_h);
        let cancel_hover = ctx.mouse.inside(&cancel_rect);
        draw_rectangle(cancel_rect.x, cancel_rect.y, cancel_rect.w, cancel_rect.h,
            if cancel_hover { Color::from_rgba(70, 70, 75, 255) } else { Color::from_rgba(55, 55, 60, 255) });
        draw_text("Cancel", cancel_rect.x + 18.0, cancel_rect.y + 18.0, 14.0, TEXT_COLOR);

        // Delete button (red)
        let delete_rect = Rect::new(dialog_x + dialog_w - btn_w - 12.0, btn_y, btn_w, btn_h);
        let delete_hover = ctx.mouse.inside(&delete_rect);
        draw_rectangle(delete_rect.x, delete_rect.y, delete_rect.w, delete_rect.h,
            if delete_hover { Color::from_rgba(180, 60, 60, 255) } else { Color::from_rgba(140, 50, 50, 255) });
        draw_text("Delete", delete_rect.x + 18.0, delete_rect.y + 18.0, 14.0, WHITE);

        // Handle button clicks
        if ctx.mouse.clicked(&cancel_rect) {
            state.delete_component_dialog = None;
        } else if ctx.mouse.clicked(&delete_rect) {
            // Delete the component
            if idx < state.asset.components.len() {
                state.asset.components.remove(idx);

                // Update selected_component
                if state.asset.components.is_empty() {
                    state.selected_component = None;
                } else if let Some(sel) = state.selected_component {
                    if sel >= state.asset.components.len() {
                        state.selected_component = Some(state.asset.components.len() - 1);
                    } else if sel > idx {
                        state.selected_component = Some(sel - 1);
                    }
                }

                // Update component_opacity (remove deleted entry)
                if idx < state.component_opacity.len() {
                    state.component_opacity.remove(idx);
                }

                state.dirty = true;
                state.set_status(&format!("Deleted '{}' component", comp_name), 1.0);
            }
            state.delete_component_dialog = None;
        }

        // Handle Escape key
        if is_key_pressed(KeyCode::Escape) {
            state.delete_component_dialog = None;
        }
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

/// Draw the snap grid size dropdown menu
fn draw_snap_menu(ctx: &mut UiContext, state: &mut ModelerState) {
    let trigger_rect = match state.dropdown.trigger_rect {
        Some(r) if state.dropdown.is_open("snap_menu") => r,
        _ => return,
    };

    // Preset snap sizes (in world units)
    const SNAP_SIZES: &[f32] = &[8.0, 16.0, 32.0, 64.0, 128.0, 256.0, 512.0];

    let item_height = 22.0;
    let menu_rect = dropdown_menu_rect(trigger_rect, SNAP_SIZES.len(), item_height, Some(80.0));

    // Keep menu on screen
    let menu_x = menu_rect.x.min(screen_width() - menu_rect.w - 5.0);
    let menu_y = menu_rect.y.min(screen_height() - menu_rect.h - 5.0);
    let menu_rect = Rect::new(menu_x, menu_y, menu_rect.w, menu_rect.h);

    if !begin_dropdown(ctx, &mut state.dropdown, "snap_menu", menu_rect) {
        return;
    }

    let mut y = menu_rect.y + 2.0;

    for &size in SNAP_SIZES {
        let item_rect = Rect::new(menu_rect.x + 2.0, y, menu_rect.w - 4.0, item_height);
        let is_current = (state.snap_settings.grid_size - size).abs() < 0.1;
        let label = format!("{}", size as i32);

        if dropdown_item(ctx, item_rect, &label, None, is_current) {
            state.snap_settings.grid_size = size;
            state.dropdown.close();
            state.set_status(&format!("Snap Grid: {} units", size as i32), 1.5);
        }

        y += item_height;
    }
}

// ============================================================================
// Radial Menu (Hold-Tab Context Menu)
// ============================================================================

/// Open the radial menu at the given position with context-appropriate items
fn open_radial_menu(state: &mut ModelerState, x: f32, y: f32) {
    use super::radial_menu::{RadialMenuItem, build_context_items, ComponentContext};

    // Determine what's selected
    let has_vertex_selection = matches!(&state.selection, super::state::ModelerSelection::Vertices(v) if !v.is_empty());
    let has_face_selection = matches!(&state.selection, super::state::ModelerSelection::Faces(f) if !f.is_empty());
    let has_edge_selection = matches!(&state.selection, super::state::ModelerSelection::Edges(e) if !e.is_empty());

    // Get bone names if skeleton exists
    let bone_names: Vec<String> = state.skeleton().iter().map(|b| b.name.clone()).collect();

    // Determine component context from selected component
    let component_ctx = state.selected_component
        .and_then(|idx| state.asset.components.get(idx))
        .map(|comp| match comp {
            crate::asset::AssetComponent::Collision { collision_mesh: Some(_), .. } => ComponentContext::Collision,
            crate::asset::AssetComponent::Particle { .. } => ComponentContext::Particle,
            _ => ComponentContext::None,
        })
        .unwrap_or(ComponentContext::None);

    // Build context-sensitive items
    let items = build_context_items(has_vertex_selection, has_face_selection, has_edge_selection, &bone_names, component_ctx);

    state.radial_menu.open(x, y, items);
}

/// Draw and handle the radial menu
fn draw_and_handle_radial_menu(ctx: &mut UiContext, state: &mut ModelerState) {
    use super::radial_menu::{draw_radial_menu, RadialMenuConfig};

    if !state.radial_menu.is_open {
        return;
    }

    // Close on Escape
    if is_key_pressed(KeyCode::Escape) {
        state.radial_menu.close(false);
        return;
    }

    // Draw and handle the menu
    let config = RadialMenuConfig::default();
    if let Some(selected_id) = draw_radial_menu(&mut state.radial_menu, &config, ctx.mouse.x, ctx.mouse.y) {
        handle_radial_menu_action(state, &selected_id);
    }

    // Close if clicked outside (check after drawing so we know the menu bounds)
    // The draw function handles click-to-select internally
}

/// Handle a radial menu action by ID
fn handle_radial_menu_action(state: &mut ModelerState, action_id: &str) {
    // Handle bone assignment (bone_0, bone_1, etc.)
    if let Some(idx_str) = action_id.strip_prefix("bone_") {
        if let Ok(bone_idx) = idx_str.parse::<usize>() {
            state.assign_selected_vertices_to_bone(bone_idx);
            return;
        }
    }

    match action_id {
        "unbind" => {
            state.unassign_selected_vertices();
        }
        "merge" => {
            // TODO: Implement merge vertices
            state.set_status("Merge vertices (not yet implemented)", 1.5);
        }
        "split" => {
            // TODO: Implement split
            state.set_status("Split (not yet implemented)", 1.5);
        }
        "extrude" => {
            // TODO: Trigger extrude
            state.set_status("Extrude (not yet implemented)", 1.5);
        }
        "inset" => {
            state.set_status("Inset (not yet implemented)", 1.5);
        }
        "flip" => {
            state.set_status("Flip normal (not yet implemented)", 1.5);
        }
        "prim_cube" => {
            add_primitive_at_origin(state, PrimitiveType::Cube);
        }
        "prim_plane" => {
            add_primitive_at_origin(state, PrimitiveType::Plane);
        }
        "prim_cylinder" => {
            add_primitive_at_origin(state, PrimitiveType::Cylinder);
        }
        "prim_prism" => {
            add_primitive_at_origin(state, PrimitiveType::Prism);
        }
        // Collision shape replacement (replaces the selected collision mesh)
        "coll_cube" => replace_collision_mesh(state, PrimitiveType::Cube),
        "coll_cylinder" => replace_collision_mesh(state, PrimitiveType::Cylinder),
        "coll_prism" => replace_collision_mesh(state, PrimitiveType::Prism),
        "coll_plane" => replace_collision_mesh(state, PrimitiveType::Plane),
        "coll_pyramid" => replace_collision_mesh(state, PrimitiveType::Pyramid),
        "coll_hex" => replace_collision_mesh(state, PrimitiveType::Hex),
        // Particle preset selection
        "part_fire" | "part_sparks" | "part_dust" | "part_blood" => {
            let preset = action_id.strip_prefix("part_").unwrap_or("fire");
            if let Some(comp_idx) = state.selected_component {
                if let Some(comp) = state.asset.components.get_mut(comp_idx) {
                    if let AssetComponent::Particle { effect, .. } = comp {
                        *effect = preset.to_string();
                        state.set_status(&format!("Set particle preset: {}", preset), 1.0);
                    }
                }
            }
        }
        _ => {
            // Unknown action
        }
    }
}

/// Replace the collision mesh with a new primitive shape
fn replace_collision_mesh(state: &mut ModelerState, prim: PrimitiveType) {
    // Find the collision mesh name from the selected component
    let mesh_name = state.selected_component
        .and_then(|idx| state.asset.components.get(idx))
        .and_then(|comp| {
            if let AssetComponent::Collision { collision_mesh: Some(name), .. } = comp {
                Some(name.clone())
            } else {
                None
            }
        });

    if let Some(mesh_name) = mesh_name {
        // Push undo before mutating
        state.push_undo(&format!("Change collision to {}", prim.label()));
        let new_mesh = prim.create(512.0);
        // Find the mesh part and replace its mesh
        if let Some(objects) = state.objects_mut() {
            if let Some(obj) = objects.iter_mut().find(|o| o.name == mesh_name) {
                obj.mesh = new_mesh;
            }
        }
        state.set_status(&format!("Collision shape: {}", prim.label()), 1.0);
    }
}

/// Add a primitive at origin as a new object
fn add_primitive_at_origin(state: &mut ModelerState, prim: PrimitiveType) {
    state.push_undo(&format!("Add {}", prim.label()));
    let size = 512.0;
    let new_mesh = prim.create(size);
    let base_name = prim.label().split_whitespace().next().unwrap_or("Object");
    let name = state.generate_unique_object_name(base_name);
    let obj = MeshPart::with_mesh(&name, new_mesh);
    state.add_object(obj);
    state.set_status(&format!("Added {}", prim.label()), 1.0);
}
