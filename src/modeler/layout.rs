//! Modeler UI layout and rendering

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext, SplitPanel, draw_panel, panel_content_rect, Toolbar, icon, icon_button};
use crate::rasterizer::Framebuffer;
use super::state::{ModelerState, DataContext, InteractionMode, RigSubMode, SelectMode, TransformTool};
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

    // Timeline at bottom of content (only in Rig/Animate mode)
    let show_timeline = state.data_context == DataContext::Rig && state.rig_sub_mode == RigSubMode::Animate;
    let (panels_rect, timeline_rect) = if show_timeline {
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

    // Draw panels based on data context
    let left_top_label = match (state.data_context, state.rig_sub_mode) {
        (DataContext::Rig, RigSubMode::Animate) => "Dopesheet",
        (DataContext::Rig, RigSubMode::Skeleton) => "Skeleton",
        (DataContext::Rig, RigSubMode::Parts) => "Parts",
        (DataContext::Spine, _) => "Segments",
        (DataContext::Mesh, _) => "Mesh",
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

    // Data context selector (Spine/Mesh/Rig)
    toolbar.label("Context:");
    for context in DataContext::ALL {
        let is_active = state.data_context == context;
        let icon_char = match context {
            DataContext::Spine => icon::GIT_BRANCH,
            DataContext::Mesh => icon::BOX,
            DataContext::Rig => icon::BONE,
        };
        if toolbar.icon_button_active(ctx, icon_char, icon_font, context.label(), is_active) {
            state.set_data_context(context);
        }
    }

    toolbar.separator();

    // Interaction mode toggle (Object/Edit)
    let mode_icon = match state.interaction_mode {
        InteractionMode::Object => icon::BOX,        // Object mode - whole objects
        InteractionMode::Edit => icon::CIRCLE_DOT,   // Edit mode - vertices/details
    };
    if toolbar.icon_button_active(ctx, mode_icon, icon_font, &format!("{} Mode (Tab)", state.interaction_mode.label()), true) {
        state.toggle_interaction_mode();
    }

    toolbar.separator();

    // Rig sub-mode (only in Rig context)
    if state.data_context == DataContext::Rig {
        for sub_mode in RigSubMode::ALL {
            let is_active = state.rig_sub_mode == sub_mode;
            let icon_char = match sub_mode {
                RigSubMode::Skeleton => icon::GIT_BRANCH,
                RigSubMode::Parts => icon::LAYERS,
                RigSubMode::Animate => icon::PLAY,
            };
            if toolbar.icon_button_active(ctx, icon_char, icon_font, sub_mode.label(), is_active) {
                state.set_rig_sub_mode(sub_mode);
            }
        }
        toolbar.separator();
    }

    // Selection mode (only in Edit mode with valid modes)
    let valid_modes = SelectMode::valid_modes(state.data_context, state.interaction_mode);
    if !valid_modes.is_empty() {
        for mode in &valid_modes {
            let is_active = state.select_mode == *mode;
            let icon_char = match mode {
                SelectMode::Segment => icon::LAYERS,
                SelectMode::Joint => icon::CIRCLE_DOT,
                SelectMode::SpineBone => icon::BONE,
                SelectMode::Vertex => icon::CIRCLE_DOT,
                SelectMode::Edge => icon::MINUS,
                SelectMode::Face => icon::SCAN,
                SelectMode::Part => icon::BOX,
                SelectMode::RigBone => icon::BONE,
            };
            if toolbar.icon_button_active(ctx, icon_char, icon_font, mode.label(), is_active) {
                if state.select_mode != *mode {
                    state.select_mode = *mode;
                    state.selection.clear(); // Clear selection when changing select mode
                }
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

    // Context-specific stats
    let stats = match state.data_context {
        DataContext::Spine => {
            if let Some(spine) = &state.spine_model {
                format!("Segs:{} Joints:{}", spine.segments.len(),
                    spine.segments.iter().map(|s| s.joints.len()).sum::<usize>())
            } else {
                "No spine".to_string()
            }
        }
        DataContext::Mesh => {
            if let Some(mesh) = &state.editable_mesh {
                format!("Verts:{} Faces:{}", mesh.vertex_count(), mesh.face_count())
            } else {
                "No mesh".to_string()
            }
        }
        DataContext::Rig => {
            if let Some(rig) = &state.rigged_model {
                format!("Parts:{} Bones:{}", rig.parts.len(), rig.skeleton.len())
            } else {
                "No rig".to_string()
            }
        }
    };
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

fn draw_hierarchy_panel(_ctx: &mut UiContext, rect: Rect, state: &ModelerState) {
    let mut y = rect.y;
    let line_height = 20.0;

    // Draw hierarchy based on current data context
    match state.data_context {
        DataContext::Spine => {
            if let Some(spine) = &state.spine_model {
                for (i, segment) in spine.segments.iter().enumerate() {
                    if y > rect.bottom() - line_height { break; }
                    draw_text(
                        &format!("▼ {} ({} joints)", segment.name, segment.joints.len()),
                        rect.x,
                        y + 14.0,
                        14.0,
                        TEXT_COLOR,
                    );
                    y += line_height;

                    // Show joints if selected
                    if matches!(&state.selection, super::state::ModelerSelection::SpineJoints(joints) if joints.iter().any(|(s, _)| *s == i)) {
                        for (j, joint) in segment.joints.iter().enumerate() {
                            if y > rect.bottom() - line_height { break; }
                            draw_text(
                                &format!("  └ Joint {} (r={:.0})", j, joint.radius),
                                rect.x,
                                y + 14.0,
                                12.0,
                                TEXT_DIM,
                            );
                            y += line_height * 0.8;
                        }
                    }
                }
            } else {
                draw_text("No spine model", rect.x, y + 14.0, 14.0, TEXT_DIM);
            }
        }
        DataContext::Mesh => {
            if let Some(mesh) = &state.editable_mesh {
                draw_text(&format!("Mesh: {} verts, {} faces", mesh.vertex_count(), mesh.face_count()), rect.x, y + 14.0, 14.0, TEXT_COLOR);
            } else {
                draw_text("No mesh loaded", rect.x, y + 14.0, 14.0, TEXT_DIM);
            }
        }
        DataContext::Rig => {
            if let Some(rig) = &state.rigged_model {
                // Show skeleton
                draw_text("Skeleton:", rect.x, y + 14.0, 12.0, TEXT_DIM);
                y += line_height;

                for (_i, bone) in rig.skeleton.iter().enumerate() {
                    if y > rect.bottom() - line_height { break; }
                    let prefix = if bone.parent.is_some() { "  └ " } else { "▼ " };
                    draw_text(&format!("{}{}", prefix, bone.name), rect.x, y + 14.0, 12.0, TEXT_COLOR);
                    y += line_height * 0.8;
                }

                y += line_height * 0.5;

                // Show parts
                draw_text("Parts:", rect.x, y + 14.0, 12.0, TEXT_DIM);
                y += line_height;

                for part in &rig.parts {
                    if y > rect.bottom() - line_height { break; }
                    let bone_name = part.bone_index
                        .and_then(|i| rig.skeleton.get(i))
                        .map(|b| b.name.as_str())
                        .unwrap_or("unassigned");
                    draw_text(&format!("  {} → {}", part.name, bone_name), rect.x, y + 14.0, 12.0, TEXT_COLOR);
                    y += line_height * 0.8;
                }
            } else {
                draw_text("No rigged model", rect.x, y + 14.0, 14.0, TEXT_DIM);
            }
        }
    }
}

fn draw_uv_editor(_ctx: &mut UiContext, rect: Rect, _state: &ModelerState) {
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

    // UV editor placeholder
    let atlas_dim = 128.0;
    let padding = 10.0;
    let available = rect.w.min(rect.h) - padding * 2.0;
    let scale = available / atlas_dim;

    let atlas_x = rect.x + (rect.w - atlas_dim * scale) * 0.5;
    let atlas_y = rect.y + (rect.h - atlas_dim * scale) * 0.5;

    draw_rectangle(atlas_x, atlas_y, atlas_dim * scale, atlas_dim * scale, Color::from_rgba(100, 100, 100, 255));
    draw_rectangle_lines(atlas_x, atlas_y, atlas_dim * scale, atlas_dim * scale, 1.0, Color::from_rgba(80, 80, 85, 255));

    draw_text(
        "UV Editor (placeholder)",
        rect.x + 4.0,
        rect.y + 14.0,
        12.0,
        TEXT_DIM,
    );
}

fn draw_viewport(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, fb: &mut Framebuffer) {
    draw_modeler_viewport(ctx, rect, state, fb);
}

fn draw_atlas_panel(_ctx: &mut UiContext, rect: Rect, _state: &ModelerState) {
    let atlas_dim = 128.0;

    // Scale to fit panel
    let padding = 4.0;
    let available = rect.w.min(rect.h - 30.0) - padding * 2.0;
    let scale = available / atlas_dim;

    let atlas_x = rect.x + (rect.w - atlas_dim * scale) * 0.5;
    let atlas_y = rect.y + padding;

    // Draw atlas placeholder
    draw_rectangle(atlas_x, atlas_y, atlas_dim * scale, atlas_dim * scale, Color::from_rgba(100, 100, 100, 255));
    draw_rectangle_lines(atlas_x, atlas_y, atlas_dim * scale, atlas_dim * scale, 1.0, Color::from_rgba(80, 80, 85, 255));

    // Size label below
    draw_text(
        "128x128",
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
        super::state::ModelerSelection::SpineSegments(segs) => {
            draw_text(&format!("{} segment(s)", segs.len()), rect.x, y + 14.0, 12.0, TEXT_COLOR);
        }
        super::state::ModelerSelection::SpineJoints(joints) => {
            draw_text(&format!("{} joint(s)", joints.len()), rect.x, y + 14.0, 12.0, TEXT_COLOR);
        }
        super::state::ModelerSelection::SpineBones(bones) => {
            draw_text(&format!("{} bone(s)", bones.len()), rect.x, y + 14.0, 12.0, TEXT_COLOR);
        }
        super::state::ModelerSelection::Mesh => {
            draw_text("Mesh (whole)", rect.x, y + 14.0, 12.0, TEXT_COLOR);
        }
        super::state::ModelerSelection::MeshVertices(verts) => {
            draw_text(&format!("{} vertex(es)", verts.len()), rect.x, y + 14.0, 12.0, TEXT_COLOR);
        }
        super::state::ModelerSelection::MeshEdges(edges) => {
            draw_text(&format!("{} edge(s)", edges.len()), rect.x, y + 14.0, 12.0, TEXT_COLOR);
        }
        super::state::ModelerSelection::MeshFaces(faces) => {
            draw_text(&format!("{} face(s)", faces.len()), rect.x, y + 14.0, 12.0, TEXT_COLOR);
        }
        super::state::ModelerSelection::RigParts(parts) => {
            draw_text(&format!("{} part(s)", parts.len()), rect.x, y + 14.0, 12.0, TEXT_COLOR);
        }
        super::state::ModelerSelection::RigBones(bones) => {
            draw_text(&format!("{} bone(s)", bones.len()), rect.x, y + 14.0, 12.0, TEXT_COLOR);
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
        y += line_height;
    }

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

fn draw_timeline(ctx: &mut UiContext, rect: Rect, state: &mut ModelerState, icon_font: Option<&Font>) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, HEADER_COLOR);

    // Handle playback animation
    if state.playing {
        let fps = state.get_animation_fps() as f64;
        state.playback_time += get_frame_time() as f64;
        let new_frame = (state.playback_time * fps) as u32;
        if new_frame != state.current_frame {
            state.current_frame = new_frame;
            let last_frame = state.get_animation_last_frame();
            if state.is_animation_looping() && state.current_frame > last_frame && last_frame > 0 {
                state.current_frame = 0;
                state.playback_time = 0.0;
            }
            state.apply_animation_pose();
        }
    }

    // Transport controls
    let mut toolbar = Toolbar::new(Rect::new(rect.x, rect.y, 200.0, 32.0));

    if toolbar.icon_button(ctx, icon::SKIP_BACK, icon_font, "Stop & Rewind") {
        state.stop_playback();
        state.apply_animation_pose();
    }

    let play_icon = if state.playing { icon::PAUSE } else { icon::PLAY };
    if toolbar.icon_button(ctx, play_icon, icon_font, if state.playing { "Pause" } else { "Play" }) {
        state.toggle_playback();
    }

    toolbar.separator();

    // Frame counter - use actual animation last frame
    let last_frame = state.get_animation_last_frame().max(60);
    toolbar.label(&format!("Frame: {:03}/{:03}", state.current_frame, last_frame));

    // Show keyframe indicator
    if state.has_keyframe_at_current_frame() {
        toolbar.label(" [K]");
    }

    toolbar.separator();

    // Timeline scrubber area
    let scrub_rect = Rect::new(rect.x + 10.0, rect.y + 40.0, rect.w - 20.0, 30.0);
    draw_rectangle(scrub_rect.x, scrub_rect.y, scrub_rect.w, scrub_rect.h, Color::from_rgba(20, 20, 25, 255));

    // Draw frame markers
    let frames_visible = last_frame.max(60) as usize;
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

    // Draw keyframe markers (diamonds)
    let keyframe_frames = state.get_keyframe_frames();
    for kf_frame in keyframe_frames {
        let kf_x = scrub_rect.x + kf_frame as f32 * frame_width;
        // Draw diamond shape
        let size = 4.0;
        let cy = scrub_rect.y + 20.0;
        draw_triangle(
            vec2(kf_x, cy - size),        // top
            vec2(kf_x - size, cy),        // left
            vec2(kf_x + size, cy),        // right
            ACCENT_COLOR,
        );
        draw_triangle(
            vec2(kf_x - size, cy),        // left
            vec2(kf_x, cy + size),        // bottom
            vec2(kf_x + size, cy),        // right
            ACCENT_COLOR,
        );
    }

    // Draw playhead
    let playhead_x = scrub_rect.x + state.current_frame as f32 * frame_width;
    draw_line(playhead_x, scrub_rect.y, playhead_x, scrub_rect.bottom(), 2.0, Color::from_rgba(255, 100, 100, 255));

    // Handle timeline click to scrub
    if ctx.mouse.inside(&scrub_rect) && is_mouse_button_pressed(MouseButton::Left) {
        let click_x = ctx.mouse.x - scrub_rect.x;
        let new_frame = ((click_x / scrub_rect.w) * frames_visible as f32) as u32;
        if new_frame != state.current_frame {
            state.current_frame = new_frame;
            state.apply_animation_pose();
        }
    }
}

fn draw_status_bar(rect: Rect, state: &ModelerState) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(40, 40, 45, 255));

    // Status message
    if let Some(msg) = state.get_status() {
        let center_x = rect.x + rect.w * 0.5 - (msg.len() as f32 * 4.0);
        draw_text(msg, center_x, rect.y + 15.0, 14.0, Color::from_rgba(100, 255, 100, 255));
    }

    // Keyboard hints based on data context and interaction mode
    let hints = match (state.data_context, state.interaction_mode) {
        (DataContext::Spine, InteractionMode::Edit) => "1/2/3:Vert/Edge/Face E:Extrude X:Del W:Subdivide Tab:Object",
        (DataContext::Spine, InteractionMode::Object) => "Tab:Edit Ctrl+1/2/3:Spine/Mesh/Rig",
        (DataContext::Mesh, InteractionMode::Edit) => "1/2/3:Vert/Edge/Face G:Move R:Rotate S:Scale Tab:Object",
        (DataContext::Mesh, InteractionMode::Object) => "Tab:Edit Ctrl+1/2/3:Spine/Mesh/Rig",
        (DataContext::Rig, _) => match state.rig_sub_mode {
            RigSubMode::Skeleton => "E:Extrude N:Root X:Delete Shift+1/2/3:Skel/Parts/Anim",
            RigSubMode::Parts => "Ctrl+1-9:Assign to bone Shift+1/2/3:Skel/Parts/Anim",
            RigSubMode::Animate => "Space:Play R:Rotate I:Key K:Delete Key",
        },
    };
    draw_text(hints, rect.right() - (hints.len() as f32 * 6.0) - 8.0, rect.y + 15.0, 12.0, TEXT_DIM);
}

fn handle_keyboard(state: &mut ModelerState) {
    let ctrl = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl)
             || is_key_down(KeyCode::LeftSuper) || is_key_down(KeyCode::RightSuper);
    let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);

    // File shortcuts (Ctrl+N, Ctrl+S, etc.) are now handled in draw_toolbar
    // to properly return ModelerAction

    // Tab = Toggle interaction mode (Object <-> Edit)
    if is_key_pressed(KeyCode::Tab) {
        state.toggle_interaction_mode();
    }

    // Blender-style shortcuts:
    // 1/2/3 = Vertex/Edge/Face selection modes (in Edit mode)
    // Ctrl+1/2/3 = Switch data context (Spine/Mesh/Rig)
    // Shift+1/2/3 = Switch rig sub-mode (when in Rig context)

    // Note: Ctrl+1-9 in Rig/Parts mode is handled in viewport.rs for part-to-bone assignment
    let in_rig_parts = state.data_context == DataContext::Rig && state.rig_sub_mode == RigSubMode::Parts;

    if is_key_pressed(KeyCode::Key1) {
        if ctrl && !in_rig_parts {
            // Ctrl+1 = Spine context (not in Rig/Parts where it assigns to bone)
            state.set_data_context(DataContext::Spine);
        } else if shift && state.data_context == DataContext::Rig {
            // Shift+1 = Skeleton sub-mode (in Rig context)
            state.set_rig_sub_mode(RigSubMode::Skeleton);
        } else if !ctrl && state.interaction_mode == InteractionMode::Edit {
            // 1 = Vertex mode (Blender-style, Edit mode only)
            state.select_mode = SelectMode::Vertex;
            state.selection = super::state::ModelerSelection::None;
            state.set_status("Vertex mode", 1.0);
        }
    }

    if is_key_pressed(KeyCode::Key2) {
        if ctrl && !in_rig_parts {
            // Ctrl+2 = Mesh context (not in Rig/Parts where it assigns to bone)
            state.set_data_context(DataContext::Mesh);
        } else if shift && state.data_context == DataContext::Rig {
            // Shift+2 = Parts sub-mode (in Rig context)
            state.set_rig_sub_mode(RigSubMode::Parts);
        } else if !ctrl && state.interaction_mode == InteractionMode::Edit {
            // 2 = Edge mode (Blender-style, Edit mode only)
            state.select_mode = SelectMode::Edge;
            state.selection = super::state::ModelerSelection::None;
            state.set_status("Edge mode", 1.0);
        }
    }

    if is_key_pressed(KeyCode::Key3) {
        if ctrl && !in_rig_parts {
            // Ctrl+3 = Rig context (not in Rig/Parts where it assigns to bone)
            state.set_data_context(DataContext::Rig);
        } else if shift && state.data_context == DataContext::Rig {
            // Shift+3 = Animate sub-mode (in Rig context)
            state.set_rig_sub_mode(RigSubMode::Animate);
        } else if !ctrl && state.interaction_mode == InteractionMode::Edit {
            // 3 = Face mode (Blender-style, Edit mode only)
            state.select_mode = SelectMode::Face;
            state.selection = super::state::ModelerSelection::None;
            state.set_status("Face mode", 1.0);
        }
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
        state.tool = TransformTool::Extrude;
        state.set_status("Extrude", 1.0);
    }

    // Animation controls (in Rig/Animate sub-mode)
    if state.data_context == DataContext::Rig && state.rig_sub_mode == RigSubMode::Animate {
        if is_key_pressed(KeyCode::Space) {
            state.toggle_playback();
        }
        if is_key_pressed(KeyCode::Left) {
            if state.current_frame > 0 {
                state.current_frame -= 1;
                state.apply_animation_pose();
            }
        }
        if is_key_pressed(KeyCode::Right) {
            state.current_frame += 1;
            state.apply_animation_pose();
        }
        if is_key_pressed(KeyCode::Home) {
            state.current_frame = 0;
            state.apply_animation_pose();
        }

        // I key: Insert keyframe at current frame
        if is_key_pressed(KeyCode::I) {
            state.insert_keyframe();
        }

        // K key: Delete keyframe at current frame
        if is_key_pressed(KeyCode::K) {
            state.delete_keyframe();
        }
    }
}
