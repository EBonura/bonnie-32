//! Texture Palette - Grid of available textures with folder selection
//!
//! Supports two modes:
//! - Source PNGs: Read-only textures from assets/textures/
//! - User Textures: Editable indexed textures from assets/textures-user/

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext, icon, draw_icon_centered};
use crate::rasterizer::{Texture as RasterTexture, ClutDepth};
use crate::texture::{
    UserTexture, TextureSize, draw_texture_canvas, draw_tool_panel, draw_palette_panel,
    draw_mode_tabs, TextureEditorMode, UvOverlayData, UvVertex, UvFace,
    draw_import_dialog, ImportAction, load_png_to_import_state,
};
use crate::rasterizer::Vec2 as RastVec2;
use super::EditorState;

const THUMB_PADDING: f32 = 4.0;
const HEADER_HEIGHT: f32 = 28.0;
const MODE_TOGGLE_HEIGHT: f32 = 24.0;

/// Draw the texture palette
pub fn draw_texture_palette(
    ctx: &mut UiContext,
    rect: Rect,
    state: &mut EditorState,
    icon_font: Option<&Font>,
) {
    // Set focus on click anywhere in this panel
    if ctx.mouse.inside(&rect) && ctx.mouse.left_pressed {
        state.active_panel = super::ActivePanel::TexturePalette;
    }

    // Background
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(25, 25, 30, 255));

    // Draw panel title bar with focus color
    let title_height = 20.0;
    draw_rectangle(rect.x, rect.y, rect.w, title_height, Color::from_rgba(50, 50, 60, 255));
    let title_color = if state.active_panel == super::ActivePanel::TexturePalette {
        Color::from_rgba(80, 180, 255, 255) // Cyan when focused
    } else {
        WHITE
    };
    draw_text("Textures", rect.x + 5.0, rect.y + 14.0, 16.0, title_color);

    // Adjust content rect to account for title
    let content_rect = Rect::new(rect.x, rect.y + title_height, rect.w, rect.h - title_height);

    // If editing a texture, show the texture editor instead
    if state.editing_texture.is_some() {
        draw_texture_editor_panel(ctx, content_rect, state, icon_font);
        return;
    }

    // Mode toggle tabs (Source PNGs | User Textures)
    let mode_rect = Rect::new(content_rect.x, content_rect.y, content_rect.w, MODE_TOGGLE_HEIGHT);
    draw_mode_toggle(ctx, mode_rect, state);

    // Header area (folder selector for source PNGs, action buttons for user textures)
    let header_rect = Rect::new(content_rect.x, content_rect.y + MODE_TOGGLE_HEIGHT, content_rect.w, HEADER_HEIGHT);

    if state.texture_palette_user_mode {
        draw_user_texture_header(ctx, header_rect, state, icon_font);
        let grid_rect = Rect::new(content_rect.x, content_rect.y + MODE_TOGGLE_HEIGHT + HEADER_HEIGHT, content_rect.w, content_rect.h - MODE_TOGGLE_HEIGHT - HEADER_HEIGHT);
        draw_user_texture_grid(ctx, grid_rect, state);
    } else {
        draw_folder_selector(ctx, header_rect, state, icon_font);
        let grid_rect = Rect::new(content_rect.x, content_rect.y + MODE_TOGGLE_HEIGHT + HEADER_HEIGHT, content_rect.w, content_rect.h - MODE_TOGGLE_HEIGHT - HEADER_HEIGHT);
        draw_source_texture_grid(ctx, grid_rect, state);
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
                state.set_status(&format!("Imported '{}' ({}x{})", name, size_w, size_w), 2.0);
                state.texture_editor.import_state.reset();
            }
            ImportAction::Cancel => {
                // Just reset the import state (already done by the dialog)
            }
        }
    }
}

/// Draw mode toggle tabs
fn draw_mode_toggle(ctx: &mut UiContext, rect: Rect, state: &mut EditorState) {
    let half_w = rect.w / 2.0;
    let active_bg = Color::from_rgba(50, 50, 60, 255);
    let inactive_bg = Color::from_rgba(35, 35, 40, 255);
    let active_text = WHITE;
    let inactive_text = Color::from_rgba(150, 150, 150, 255);

    // Source PNGs tab
    let source_rect = Rect::new(rect.x, rect.y, half_w, rect.h);
    let source_bg = if !state.texture_palette_user_mode { active_bg } else { inactive_bg };
    let source_text_color = if !state.texture_palette_user_mode { active_text } else { inactive_text };
    draw_rectangle(source_rect.x, source_rect.y, source_rect.w, source_rect.h, source_bg);
    let source_label = "Source";
    let source_dims = measure_text(source_label, None, 12, 1.0);
    draw_text(
        source_label,
        (source_rect.x + (source_rect.w - source_dims.width) / 2.0).floor(),
        (source_rect.y + (source_rect.h + source_dims.height) / 2.0).floor(),
        12.0,
        source_text_color,
    );
    if ctx.mouse.clicked(&source_rect) {
        state.texture_palette_user_mode = false;
    }

    // Paint Textures tab (editable indexed textures)
    let paint_rect = Rect::new(rect.x + half_w, rect.y, half_w, rect.h);
    let paint_bg = if state.texture_palette_user_mode { active_bg } else { inactive_bg };
    let paint_text_color = if state.texture_palette_user_mode { active_text } else { inactive_text };
    draw_rectangle(paint_rect.x, paint_rect.y, paint_rect.w, paint_rect.h, paint_bg);
    let paint_label = "Paint";
    let paint_dims = measure_text(paint_label, None, 12, 1.0);
    draw_text(
        paint_label,
        (paint_rect.x + (paint_rect.w - paint_dims.width) / 2.0).floor(),
        (paint_rect.y + (paint_rect.h + paint_dims.height) / 2.0).floor(),
        12.0,
        paint_text_color,
    );
    if ctx.mouse.clicked(&paint_rect) {
        state.texture_palette_user_mode = true;
    }

    // Separator line
    draw_line(rect.x, rect.bottom() - 1.0, rect.right(), rect.bottom() - 1.0, 1.0, Color::from_rgba(60, 60, 70, 255));
}

/// Draw the source texture grid (original implementation)
fn draw_source_texture_grid(
    ctx: &mut UiContext,
    content_rect: Rect,
    state: &mut EditorState,
) {
    // Store actual width for scroll_to_texture calculations
    state.texture_palette_width = content_rect.w;

    // Get thumbnail size from state
    let thumb_size = state.source_thumb_size;

    // Get texture count without borrowing state
    let texture_count = state.texture_packs
        .get(state.selected_pack)
        .map(|p| p.textures.len())
        .unwrap_or(0);

    if texture_count == 0 {
        draw_text(
            "No textures in this pack",
            (content_rect.x + 10.0).floor(),
            (content_rect.y + 20.0).floor(),
            16.0,
            Color::from_rgba(100, 100, 100, 255),
        );
        return;
    }

    // Calculate grid layout
    let cols = ((content_rect.w - THUMB_PADDING) / (thumb_size + THUMB_PADDING)).floor() as usize;
    let cols = cols.max(1);
    let rows = (texture_count + cols - 1) / cols;
    let total_height = rows as f32 * (thumb_size + THUMB_PADDING) + THUMB_PADDING;

    // Calculate max scroll and always clamp (handles programmatic scroll changes)
    let max_scroll = (total_height - content_rect.h).max(0.0);
    state.texture_scroll = state.texture_scroll.clamp(0.0, max_scroll);

    // Handle mouse wheel scrolling
    if ctx.mouse.inside(&content_rect) {
        state.texture_scroll -= ctx.mouse.scroll * 12.0;
        state.texture_scroll = state.texture_scroll.clamp(0.0, max_scroll);
    }

    // Draw scrollbar if needed
    if total_height > content_rect.h {
        let scrollbar_width = 8.0;
        let scrollbar_x = content_rect.right() - scrollbar_width - 2.0;
        let scrollbar_height = content_rect.h;
        let thumb_height = (content_rect.h / total_height * scrollbar_height).max(20.0);
        let max_scroll = total_height - content_rect.h;
        let thumb_y = content_rect.y + (state.texture_scroll / max_scroll) * (scrollbar_height - thumb_height);

        // Scrollbar track
        draw_rectangle(
            scrollbar_x,
            content_rect.y,
            scrollbar_width,
            scrollbar_height,
            Color::from_rgba(15, 15, 20, 255),
        );
        // Scrollbar thumb
        draw_rectangle(
            scrollbar_x,
            thumb_y,
            scrollbar_width,
            thumb_height,
            Color::from_rgba(80, 80, 90, 255),
        );
    }

    // Track clicked texture to update after loop
    let mut clicked_texture: Option<crate::world::TextureRef> = None;
    let selected_pack = state.selected_pack;
    let selected_texture = &state.selected_texture;
    let texture_scroll = state.texture_scroll;

    // Enable scissor clipping to content area for partial textures
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

    // Draw texture grid by index to avoid borrowing issues
    for i in 0..texture_count {
        let col = i % cols;
        let row = i / cols;

        let x = content_rect.x + THUMB_PADDING + col as f32 * (thumb_size + THUMB_PADDING);
        let y = content_rect.y + THUMB_PADDING + row as f32 * (thumb_size + THUMB_PADDING) - texture_scroll;

        // Skip if completely outside visible area
        if y + thumb_size < content_rect.y || y > content_rect.bottom() {
            continue;
        }

        let thumb_rect = Rect::new(x, y, thumb_size, thumb_size);

        // Get texture and pack from state
        let (texture, pack_name) = match state.texture_packs.get(selected_pack) {
            Some(pack) => match pack.textures.get(i) {
                Some(tex) => (tex, &pack.name),
                None => continue,
            },
            None => continue,
        };

        // Check for click - use intersection of thumb_rect with content_rect for partial visibility
        let visible_rect = Rect::new(
            thumb_rect.x,
            thumb_rect.y.max(content_rect.y),
            thumb_rect.w,
            (thumb_rect.bottom().min(content_rect.bottom()) - thumb_rect.y.max(content_rect.y)).max(0.0),
        );
        if visible_rect.h > 0.0 && ctx.mouse.clicked(&visible_rect) {
            clicked_texture = Some(crate::world::TextureRef::new(pack_name.clone(), texture.name.clone()));
        }

        // Draw texture thumbnail (use cached GPU texture to prevent memory leak)
        let cache_key = (selected_pack, i);
        let mq_texture = state.gpu_texture_cache.entry(cache_key).or_insert_with(|| {
            raster_to_mq_texture(texture)
        });
        draw_texture_ex(
            mq_texture,
            x,
            y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(Vec2::new(thumb_size, thumb_size)),
                ..Default::default()
            },
        );

        // Check if this texture is selected
        let is_selected = selected_texture.is_valid()
            && selected_texture.pack == *pack_name
            && selected_texture.name == texture.name;

        // Selection highlight
        if is_selected {
            draw_rectangle_lines(
                x - 2.0,
                y - 2.0,
                thumb_size + 4.0,
                thumb_size + 4.0,
                2.0,
                Color::from_rgba(255, 200, 50, 255),
            );
        }

        // Hover highlight - check visible portion
        if ctx.mouse.inside(&visible_rect) && !is_selected {
            draw_rectangle_lines(
                x - 1.0,
                y - 1.0,
                thumb_size + 2.0,
                thumb_size + 2.0,
                1.0,
                Color::from_rgba(150, 150, 200, 255),
            );
        }

        // Texture index (only draw if visible)
        if y + thumb_size - 2.0 >= content_rect.y && y + thumb_size - 2.0 <= content_rect.bottom() {
            draw_text(
                &format!("{}", i),
                (x + 2.0).floor(),
                (y + thumb_size - 2.0).floor(),
                12.0,
                Color::from_rgba(255, 255, 255, 200),
            );
        }
    }

    // Disable scissor clipping
    unsafe {
        get_internal_gl().quad_gl.scissor(None);
    }

    // Apply clicked texture after loop
    if let Some(tex_ref) = clicked_texture {
        state.selected_texture = tex_ref.clone();

        // Collect all selections to apply texture to (primary + multi-selection)
        let mut all_selections: Vec<super::Selection> = vec![state.selection.clone()];
        all_selections.extend(state.multi_selection.clone());

        // Check if we have any valid selections
        let has_valid_selection = all_selections.iter().any(|sel| !matches!(sel, super::Selection::None));

        if has_valid_selection {
            state.save_undo();

            // Apply texture to all selections, respecting triangle selection for horizontal faces
            let triangle_sel = state.selected_triangle;
            for sel in all_selections {
                apply_texture_to_selection(&mut state.level, sel, tex_ref.clone(), triangle_sel);
            }
        }
    }
}

/// Apply a texture to a single selection
fn apply_texture_to_selection(
    level: &mut crate::world::Level,
    selection: super::Selection,
    tex_ref: crate::world::TextureRef,
    triangle_sel: super::TriangleSelection,
) {
    match selection {
        // Single face selected (from 3D view) - apply to that face only
        super::Selection::SectorFace { room, x, z, face } => {
            if let Some(r) = level.rooms.get_mut(room) {
                if let Some(sector) = r.get_sector_mut(x, z) {
                    match face {
                        super::SectorFace::Floor => {
                            if let Some(floor) = &mut sector.floor {
                                apply_texture_to_horizontal_face(floor, tex_ref, triangle_sel);
                            }
                        }
                        super::SectorFace::Ceiling => {
                            if let Some(ceiling) = &mut sector.ceiling {
                                apply_texture_to_horizontal_face(ceiling, tex_ref, triangle_sel);
                            }
                        }
                        super::SectorFace::WallNorth(i) => {
                            if let Some(wall) = sector.walls_north.get_mut(i) {
                                wall.texture = tex_ref;
                            }
                        }
                        super::SectorFace::WallEast(i) => {
                            if let Some(wall) = sector.walls_east.get_mut(i) {
                                wall.texture = tex_ref;
                            }
                        }
                        super::SectorFace::WallSouth(i) => {
                            if let Some(wall) = sector.walls_south.get_mut(i) {
                                wall.texture = tex_ref;
                            }
                        }
                        super::SectorFace::WallWest(i) => {
                            if let Some(wall) = sector.walls_west.get_mut(i) {
                                wall.texture = tex_ref;
                            }
                        }
                        super::SectorFace::WallNwSe(i) => {
                            if let Some(wall) = sector.walls_nwse.get_mut(i) {
                                wall.texture = tex_ref;
                            }
                        }
                        super::SectorFace::WallNeSw(i) => {
                            if let Some(wall) = sector.walls_nesw.get_mut(i) {
                                wall.texture = tex_ref;
                            }
                        }
                    }
                }
            }
        }
        // Whole sector selected (from 2D view) - apply to all faces
        super::Selection::Sector { room, x, z } => {
            if let Some(r) = level.rooms.get_mut(room) {
                if let Some(sector) = r.get_sector_mut(x, z) {
                    // Apply to floor if it exists (respecting triangle selection)
                    if let Some(floor) = &mut sector.floor {
                        apply_texture_to_horizontal_face(floor, tex_ref.clone(), triangle_sel);
                    }
                    // Apply to ceiling if it exists (respecting triangle selection)
                    if let Some(ceiling) = &mut sector.ceiling {
                        apply_texture_to_horizontal_face(ceiling, tex_ref.clone(), triangle_sel);
                    }
                    // Apply to all walls (walls don't have triangle selection)
                    for wall in &mut sector.walls_north {
                        wall.texture = tex_ref.clone();
                    }
                    for wall in &mut sector.walls_east {
                        wall.texture = tex_ref.clone();
                    }
                    for wall in &mut sector.walls_south {
                        wall.texture = tex_ref.clone();
                    }
                    for wall in &mut sector.walls_west {
                        wall.texture = tex_ref.clone();
                    }
                }
            }
        }
        _ => {}
    }
}

/// Apply texture to a horizontal face, respecting triangle selection
fn apply_texture_to_horizontal_face(
    face: &mut crate::world::HorizontalFace,
    tex_ref: crate::world::TextureRef,
    triangle_sel: super::TriangleSelection,
) {
    match triangle_sel {
        super::TriangleSelection::Both => {
            // Apply to both triangles (keep them linked)
            face.texture = tex_ref;
            face.texture_2 = None;
        }
        super::TriangleSelection::Tri1 => {
            // Apply only to triangle 1
            face.texture = tex_ref;
            // Don't touch texture_2 - if it was set, keep it
        }
        super::TriangleSelection::Tri2 => {
            // Apply only to triangle 2
            face.texture_2 = Some(tex_ref);
        }
    }
}

/// Available thumbnail sizes
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

/// Draw the folder selector dropdown
fn draw_folder_selector(ctx: &mut UiContext, rect: Rect, state: &mut EditorState, icon_font: Option<&Font>) {
    // Background
    draw_rectangle(rect.x.floor(), rect.y.floor(), rect.w, rect.h, Color::from_rgba(40, 40, 45, 255));

    if state.texture_packs.is_empty() {
        draw_text("No texture packs found", (rect.x + 5.0).floor(), (rect.y + 18.0).floor(), 14.0, Color::from_rgba(150, 150, 150, 255));
        return;
    }

    let btn_size = (rect.h - 8.0).round();

    // Previous button - flat icon style
    let prev_rect = Rect::new((rect.x + 4.0).round(), (rect.y + 4.0).round(), btn_size, btn_size);
    let prev_hovered = ctx.mouse.inside(&prev_rect);
    if prev_hovered {
        draw_rectangle(prev_rect.x, prev_rect.y, prev_rect.w, prev_rect.h, Color::from_rgba(60, 60, 70, 255));
    }
    let prev_color = if prev_hovered { WHITE } else { Color::from_rgba(180, 180, 180, 255) };
    draw_icon_centered(icon_font, icon::CIRCLE_CHEVRON_LEFT, &prev_rect, 14.0, prev_color);
    if ctx.mouse.clicked(&prev_rect) && state.selected_pack > 0 {
        state.selected_pack -= 1;
        state.selected_texture = crate::world::TextureRef::none();
        state.texture_scroll = 0.0;
    }

    // Next button - far right
    let next_rect = Rect::new((rect.right() - btn_size - 4.0).round(), (rect.y + 4.0).round(), btn_size, btn_size);
    let next_hovered = ctx.mouse.inside(&next_rect);
    if next_hovered {
        draw_rectangle(next_rect.x, next_rect.y, next_rect.w, next_rect.h, Color::from_rgba(60, 60, 70, 255));
    }
    let next_color = if next_hovered { WHITE } else { Color::from_rgba(180, 180, 180, 255) };
    draw_icon_centered(icon_font, icon::CIRCLE_CHEVRON_RIGHT, &next_rect, 14.0, next_color);
    if ctx.mouse.clicked(&next_rect) && state.selected_pack < state.texture_packs.len() - 1 {
        state.selected_pack += 1;
        state.selected_texture = crate::world::TextureRef::none();
        state.texture_scroll = 0.0;
    }

    // Zoom buttons - before next button
    let zoom_x = next_rect.x - (btn_size * 2.0 + 2.0) - 8.0;
    let (zoom_out, zoom_in) = draw_zoom_buttons(ctx, zoom_x, (rect.y + 4.0).round(), btn_size, icon_font);
    if zoom_out {
        state.source_thumb_size = smaller_thumb_size(state.source_thumb_size);
    }
    if zoom_in {
        state.source_thumb_size = larger_thumb_size(state.source_thumb_size);
    }

    // Pack name in center (between prev and zoom buttons)
    let name = state.current_pack_name();
    let pack_count = state.texture_packs.len();
    let label = format!("{} ({}/{})", name, state.selected_pack + 1, pack_count);
    let font_size = 14.0;
    let text_dims = measure_text(&label, None, font_size as u16, 1.0);
    // Center between prev button and zoom buttons
    let text_area_start = prev_rect.right() + 4.0;
    let text_area_end = zoom_x - 4.0;
    let text_x = (text_area_start + (text_area_end - text_area_start - text_dims.width) * 0.5).round();
    let text_y = (rect.y + (rect.h + text_dims.height) * 0.5).round();
    draw_text(&label, text_x, text_y, font_size, WHITE);
}

/// Convert a raster texture to a macroquad texture
fn raster_to_mq_texture(texture: &RasterTexture) -> Texture2D {
    // Convert RGBA pixels (use to_bytes which handles blend mode -> alpha conversion)
    let mut pixels = Vec::with_capacity(texture.width * texture.height * 4);
    for y in 0..texture.height {
        for x in 0..texture.width {
            let color = texture.get_pixel(x, y);
            let bytes = color.to_bytes();
            pixels.push(bytes[0]);
            pixels.push(bytes[1]);
            pixels.push(bytes[2]);
            pixels.push(bytes[3]);
        }
    }

    let tex = Texture2D::from_rgba8(texture.width as u16, texture.height as u16, &pixels);
    tex.set_filter(FilterMode::Nearest);
    tex
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

/// Draw header for user texture mode (New + Edit buttons)
fn draw_user_texture_header(
    ctx: &mut UiContext,
    rect: Rect,
    state: &mut EditorState,
    icon_font: Option<&Font>,
) {
    draw_rectangle(rect.x.floor(), rect.y.floor(), rect.w, rect.h, Color::from_rgba(40, 40, 45, 255));

    let btn_h = rect.h - 8.0;
    let btn_w = 60.0;
    let btn_y = rect.y + 4.0;
    let mut btn_x = rect.x + 4.0;

    // "Import" button - square icon button at the start
    let import_btn_w = btn_h;
    let import_rect = Rect::new(btn_x, btn_y, import_btn_w, btn_h);
    let import_hovered = ctx.mouse.inside(&import_rect);
    let import_bg = if import_hovered {
        Color::from_rgba(70, 70, 80, 255)
    } else {
        Color::from_rgba(55, 55, 65, 255)
    };
    draw_rectangle(import_rect.x, import_rect.y, import_rect.w, import_rect.h, import_bg);
    draw_rectangle_lines(import_rect.x, import_rect.y, import_rect.w, import_rect.h, 1.0, Color::from_rgba(80, 80, 90, 255));

    let import_icon_color = if import_hovered { WHITE } else { Color::from_rgba(200, 200, 200, 255) };
    draw_icon_centered(icon_font, icon::FOLDER_OPEN, &import_rect, 14.0, import_icon_color);

    // Tooltip on hover
    if import_hovered {
        let tooltip = "Import PNG";
        let tip_dims = measure_text(tooltip, None, 12, 1.0);
        let tip_x = import_rect.x + import_rect.w / 2.0 - tip_dims.width / 2.0;
        let tip_y = import_rect.bottom() + 4.0;
        draw_rectangle(tip_x - 4.0, tip_y - 2.0, tip_dims.width + 8.0, tip_dims.height + 6.0, Color::from_rgba(30, 30, 35, 240));
        draw_text(tooltip, tip_x, tip_y + tip_dims.height, 12.0, WHITE);
    }

    // Import button opens file picker
    if ctx.mouse.clicked(&import_rect) {
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

    btn_x += import_btn_w + 4.0;

    // "New" button
    let new_rect = Rect::new(btn_x, btn_y, btn_w, btn_h);
    let new_hovered = ctx.mouse.inside(&new_rect);
    let new_bg = if new_hovered { Color::from_rgba(70, 70, 80, 255) } else { Color::from_rgba(55, 55, 65, 255) };
    draw_rectangle(new_rect.x, new_rect.y, new_rect.w, new_rect.h, new_bg);
    draw_rectangle_lines(new_rect.x, new_rect.y, new_rect.w, new_rect.h, 1.0, Color::from_rgba(80, 80, 90, 255));

    // Draw plus icon and "New" text
    let icon_rect = Rect::new(new_rect.x + 2.0, new_rect.y, 16.0, new_rect.h);
    draw_icon_centered(icon_font, icon::PLUS, &icon_rect, 12.0, if new_hovered { WHITE } else { Color::from_rgba(200, 200, 200, 255) });
    draw_text("New", (new_rect.x + 18.0).floor(), (new_rect.y + new_rect.h / 2.0 + 4.0).floor(), 12.0, if new_hovered { WHITE } else { Color::from_rgba(200, 200, 200, 255) });

    if ctx.mouse.clicked(&new_rect) {
        // Create a new 64x64 texture with auto-numbered name (texture_001, texture_002, etc.)
        let name = state.user_textures.next_available_name();
        let tex = UserTexture::new(&name, TextureSize::Size64x64, ClutDepth::Bpp4);
        state.user_textures.add(tex);
        state.editing_texture = Some(name.clone());
        // Reset texture editor state for new texture
        state.texture_editor.reset();
    }

    btn_x += btn_w + 4.0;

    // "Edit" button - edits the selected texture
    let has_selection = state.selected_user_texture.is_some();
    let edit_rect = Rect::new(btn_x, btn_y, btn_w, btn_h);
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
            state.editing_texture = Some(name.clone());
            state.texture_editor.reset();
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
    let btn_size = 20.0;
    let zoom_x = count_x - (btn_size * 2.0 + 2.0) - 8.0;
    let (zoom_out, zoom_in) = draw_zoom_buttons(ctx, zoom_x, (rect.y + 4.0).round(), btn_size, icon_font);
    if zoom_out {
        state.paint_thumb_size = smaller_thumb_size(state.paint_thumb_size);
    }
    if zoom_in {
        state.paint_thumb_size = larger_thumb_size(state.paint_thumb_size);
    }
}

/// Draw the user texture grid
fn draw_user_texture_grid(
    ctx: &mut UiContext,
    content_rect: Rect,
    state: &mut EditorState,
) {
    // Get thumbnail size from state
    let thumb_size = state.paint_thumb_size;

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

    // Use a separate scroll for user textures (reuse texture_scroll for simplicity)
    let max_scroll = (total_height - content_rect.h).max(0.0);
    state.texture_scroll = state.texture_scroll.clamp(0.0, max_scroll);

    // Handle mouse wheel scrolling
    if ctx.mouse.inside(&content_rect) {
        state.texture_scroll -= ctx.mouse.scroll * 12.0;
        state.texture_scroll = state.texture_scroll.clamp(0.0, max_scroll);
    }

    // Draw scrollbar if needed
    if total_height > content_rect.h && max_scroll > 0.0 {
        let scrollbar_width = 8.0;
        let scrollbar_x = content_rect.right() - scrollbar_width - 2.0;
        let scrollbar_height = content_rect.h;
        let thumb_height = (content_rect.h / total_height * scrollbar_height).max(20.0);
        let thumb_y = content_rect.y + (state.texture_scroll / max_scroll) * (scrollbar_height - thumb_height);

        draw_rectangle(scrollbar_x, content_rect.y, scrollbar_width, scrollbar_height, Color::from_rgba(15, 15, 20, 255));
        draw_rectangle(scrollbar_x, thumb_y, scrollbar_width, thumb_height, Color::from_rgba(80, 80, 90, 255));
    }

    // Collect texture names first to avoid borrow issues
    let texture_names: Vec<String> = state.user_textures.names().map(|s| s.to_string()).collect();

    // Track clicked texture
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
        let y = content_rect.y + THUMB_PADDING + row as f32 * (thumb_size + THUMB_PADDING) - state.texture_scroll;

        // Skip if outside visible area
        if y + thumb_size < content_rect.y || y > content_rect.bottom() {
            continue;
        }

        let thumb_rect = Rect::new(x, y, thumb_size, thumb_size);

        // Get texture for rendering
        if let Some(tex) = state.user_textures.get(name) {
            // Draw checkerboard background for transparency
            let check_size = (thumb_size / tex.width.max(tex.height) as f32 * 2.0).max(4.0);
            draw_checkerboard(x, y, thumb_size, thumb_size, check_size);

            // Draw texture thumbnail with alpha
            let mq_tex = user_texture_to_mq_texture(tex);
            draw_texture_ex(
                &mq_tex,
                x,
                y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(Vec2::new(thumb_size, thumb_size)),
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
            draw_text(display_name, (x + 2.0).floor(), (y + thumb_size - 2.0).floor(), 10.0, Color::from_rgba(255, 255, 255, 200));
        }
    }

    // Disable scissor clipping
    unsafe {
        get_internal_gl().quad_gl.scissor(None);
    }

    // Handle single-click to select AND assign to face (same as source textures)
    if let Some(name) = clicked_texture {
        state.selected_user_texture = Some(name.clone());

        // Create TextureRef for user texture
        let tex_ref = crate::world::TextureRef::user(&name);

        // Collect all selections to apply texture to (primary + multi-selection)
        let mut all_selections: Vec<super::Selection> = vec![state.selection.clone()];
        all_selections.extend(state.multi_selection.clone());

        // Check if we have any valid selections
        let has_valid_selection = all_selections.iter().any(|sel| !matches!(sel, super::Selection::None));

        if has_valid_selection {
            state.save_undo();

            // Apply texture to all selections, respecting triangle selection for horizontal faces
            let triangle_sel = state.selected_triangle;
            for sel in all_selections {
                apply_texture_to_selection(&mut state.level, sel, tex_ref.clone(), triangle_sel);
            }
        }
    }

    // Handle double-click to edit (also sets selection)
    if let Some(name) = double_clicked_texture {
        state.selected_user_texture = Some(name.clone());
        state.editing_texture = Some(name);
        state.texture_editor.reset();
    }
}

/// Draw the texture editor panel (when editing a texture)
fn draw_texture_editor_panel(
    ctx: &mut UiContext,
    rect: Rect,
    state: &mut EditorState,
    icon_font: Option<&Font>,
) {
    let texture_name = match &state.editing_texture {
        Some(name) => name.clone(),
        None => return,
    };

    // Header with texture name and buttons (match main toolbar sizing: 36px height, 32px buttons, 16px icons)
    let header_h = 36.0;
    let btn_size = 32.0;
    let icon_size = 16.0;
    let header_rect = Rect::new(rect.x, rect.y, rect.w, header_h);
    draw_rectangle(header_rect.x, header_rect.y, header_rect.w, header_rect.h, Color::from_rgba(45, 45, 55, 255));

    let is_dirty = state.texture_editor.dirty;

    // Back button (arrow-big-left) - far right
    let back_rect = Rect::new(rect.right() - btn_size - 2.0, rect.y + 2.0, btn_size, btn_size);
    let back_hovered = ctx.mouse.inside(&back_rect);
    if back_hovered {
        draw_rectangle(back_rect.x, back_rect.y, back_rect.w, back_rect.h, Color::from_rgba(80, 60, 60, 255));
    }
    draw_icon_centered(icon_font, icon::ARROW_BIG_LEFT, &back_rect, icon_size, if back_hovered { WHITE } else { Color::from_rgba(200, 200, 200, 255) });

    if ctx.mouse.clicked(&back_rect) {
        state.editing_texture = None;
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

        if save_hovered {
            #[cfg(not(target_arch = "wasm32"))]
            ctx.set_tooltip("Save texture", ctx.mouse.x, ctx.mouse.y);
            #[cfg(target_arch = "wasm32")]
            ctx.set_tooltip("Download texture", ctx.mouse.x, ctx.mouse.y);
        }
    }

    // Texture name with dirty indicator (vertically centered in header)
    let dirty_indicator = if is_dirty { " ●" } else { "" };
    let name_text = format!("{}{}", texture_name, dirty_indicator);
    let name_color = if is_dirty { Color::from_rgba(255, 200, 100, 255) } else { WHITE };
    draw_text(&name_text, (header_rect.x + 8.0).floor(), (header_rect.y + header_h / 2.0 + 4.0).floor(), 12.0, name_color);

    // Content area below header
    let full_content_rect = Rect::new(rect.x, rect.y + header_h, rect.w, rect.h - header_h);

    // Draw mode tabs (Paint/UV)
    let tabs_rect = Rect::new(full_content_rect.x, full_content_rect.y, full_content_rect.w, 24.0);
    let content_rect = draw_mode_tabs(ctx, tabs_rect, &mut state.texture_editor);

    // Build UV overlay data from selected face when in UV mode
    let uv_data = if state.texture_editor.mode == TextureEditorMode::Uv {
        build_uv_overlay_from_selection(state)
    } else {
        None
    };

    // Get mutable texture reference - need to split borrows
    let tex = match state.user_textures.get_mut(&texture_name) {
        Some(t) => t,
        None => {
            state.editing_texture = None;
            return;
        }
    };

    // Layout: Canvas (square, expands with sidebar) + Tool panel (right), Palette panel (below)
    let tool_panel_w = 66.0;  // 2-column layout: 2 * 28px buttons + 2px gap + 4px padding each side
    let canvas_w = content_rect.w - tool_panel_w;
    // Tool panel needs ~280px height (6 tools + undo/redo/zoom/grid + size/shape options)
    // Palette needs: depth buttons (~22) + gen row (~24) + grid (~65) + color editor (~60) + effect (~18) = ~190
    let min_canvas_h = 280.0;  // Minimum for tool panel to fit all buttons
    let min_palette_h = 190.0;  // Minimum palette panel height
    let max_canvas_h = (content_rect.h - min_palette_h).max(min_canvas_h);  // Leave room for palette
    let canvas_h = canvas_w.min(max_canvas_h).max(min_canvas_h);  // Square when possible, but enforce minimum for tool panel
    let palette_panel_h = (content_rect.h - canvas_h).max(min_palette_h);  // Remaining space goes to palette

    let canvas_rect = Rect::new(content_rect.x, content_rect.y, canvas_w, canvas_h);
    let tool_rect = Rect::new(content_rect.x + canvas_w, content_rect.y, tool_panel_w, canvas_h);
    let palette_rect = Rect::new(content_rect.x, content_rect.y + canvas_h, content_rect.w, palette_panel_h);

    // Store texture dimensions for UV operations
    let tex_width = tex.width as f32;
    let tex_height = tex.height as f32;

    // Draw panels
    draw_texture_canvas(ctx, canvas_rect, tex, &mut state.texture_editor, uv_data.as_ref());
    draw_tool_panel(ctx, tool_rect, &mut state.texture_editor, icon_font);
    draw_palette_panel(ctx, palette_rect, tex, &mut state.texture_editor, icon_font);

    // Handle UV direct drag (applies changes to face when dragging UV vertices)
    apply_uv_direct_drag_to_face(ctx, tex_width, tex_height, state);

    // Handle UV modal transforms (G/T/R keys)
    apply_uv_modal_transform_to_face(ctx, tex_width, tex_height, state);

    // Handle UV operations (flip/rotate/reset buttons)
    apply_uv_operation_to_face(tex_width, tex_height, state);

    // Handle undo save signals from texture editor (save BEFORE the action is applied)
    if state.texture_editor.undo_save_pending.take().is_some() {
        state.save_texture_undo(&texture_name);
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

    // Forward status messages from texture editor to main editor
    if let Some(msg) = state.texture_editor.take_status() {
        state.set_status(&msg, 2.0);
    }

    // Increment texture generation when dirty (for 3D view cache invalidation)
    if state.texture_editor.dirty {
        state.texture_generation = state.texture_generation.wrapping_add(1);
    }

    // Handle save button click
    if save_clicked {
        #[cfg(not(target_arch = "wasm32"))]
        {
            // Native: save to disk
            if let Err(e) = state.user_textures.save_texture(&texture_name) {
                eprintln!("Failed to save texture: {}", e);
            } else {
                state.texture_editor.dirty = false;
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            // WASM: trigger download
            if let Some(tex) = state.user_textures.get(&texture_name) {
                if let Ok(ron_str) = tex.to_ron_string() {
                    let filename = format!("{}.ron", texture_name);
                    extern "C" {
                        fn bonnie_set_export_data(ptr: *const u8, len: usize);
                        fn bonnie_set_export_filename(ptr: *const u8, len: usize);
                        fn bonnie_trigger_download();
                    }
                    unsafe {
                        bonnie_set_export_data(ron_str.as_ptr(), ron_str.len());
                        bonnie_set_export_filename(filename.as_ptr(), filename.len());
                        bonnie_trigger_download();
                    }
                    state.texture_editor.dirty = false;
                }
            }
        }
    }
}

/// Build UV overlay data from all selected faces in the world editor
/// UVs are offset based on sector grid position so adjacent faces appear adjacent in UV space
fn build_uv_overlay_from_selection(state: &EditorState) -> Option<UvOverlayData> {
    // Collect all SectorFace selections (primary + multi-selection)
    let mut face_selections: Vec<(usize, usize, usize, super::SectorFace)> = Vec::new();

    // Add primary selection if it's a SectorFace
    if let super::Selection::SectorFace { room, x, z, face } = &state.selection {
        face_selections.push((*room, *x, *z, face.clone()));
    }

    // Add all multi-selected faces
    for sel in &state.multi_selection {
        if let super::Selection::SectorFace { room, x, z, face } = sel {
            // Avoid duplicates
            let key = (*room, *x, *z, face.clone());
            if !face_selections.iter().any(|s| s.0 == key.0 && s.1 == key.1 && s.2 == key.2 && s.3 == key.3) {
                face_selections.push(key);
            }
        }
    }

    if face_selections.is_empty() {
        return None;
    }

    // Find minimum x,z to use as origin for UV offset calculation
    // This way face at min position has UVs at 0,0 and others are offset
    let min_x = face_selections.iter().map(|(_, x, _, _)| *x).min().unwrap_or(0);
    let min_z = face_selections.iter().map(|(_, _, z, _)| *z).min().unwrap_or(0);

    // Build combined UV overlay from all selected faces
    let mut vertices = Vec::new();
    let mut faces = Vec::new();

    for (face_idx, (room, x, z, face)) in face_selections.iter().enumerate() {
        let r = match state.level.rooms.get(*room) {
            Some(r) => r,
            None => continue,
        };
        let sector = match r.get_sector(*x, *z) {
            Some(s) => s,
            None => continue,
        };

        let face_uvs: Option<[crate::rasterizer::Vec2; 4]> = match face {
            super::SectorFace::Floor => {
                sector.floor.as_ref().and_then(|f| f.uv)
            }
            super::SectorFace::Ceiling => {
                sector.ceiling.as_ref().and_then(|f| f.uv)
            }
            super::SectorFace::WallNorth(i) => {
                sector.walls_north.get(*i).and_then(|w| w.uv)
            }
            super::SectorFace::WallEast(i) => {
                sector.walls_east.get(*i).and_then(|w| w.uv)
            }
            super::SectorFace::WallSouth(i) => {
                sector.walls_south.get(*i).and_then(|w| w.uv)
            }
            super::SectorFace::WallWest(i) => {
                sector.walls_west.get(*i).and_then(|w| w.uv)
            }
            super::SectorFace::WallNwSe(i) => {
                sector.walls_nwse.get(*i).and_then(|w| w.uv)
            }
            super::SectorFace::WallNeSw(i) => {
                sector.walls_nesw.get(*i).and_then(|w| w.uv)
            }
        };

        // Use default UVs if face has None (0-1 range)
        let base_uvs = face_uvs.unwrap_or([
            RastVec2::new(0.0, 0.0),
            RastVec2::new(1.0, 0.0),
            RastVec2::new(1.0, 1.0),
            RastVec2::new(0.0, 1.0),
        ]);

        // Calculate UV offset based on sector position relative to minimum
        // For horizontal faces (floor/ceiling), use sector x,z position
        let (offset_u, offset_v) = match face {
            super::SectorFace::Floor | super::SectorFace::Ceiling => {
                let dx = (*x - min_x) as f32;
                let dz = (*z - min_z) as f32;
                (dx, dz)
            }
            // For walls, we could offset based on position but for now keep them at origin
            // since wall alignment is more complex (depends on direction)
            _ => (0.0, 0.0),
        };

        // Add 4 vertices for this face with offset applied
        // vertex_index encodes both face index and corner: face_idx * 4 + corner
        let base_idx = vertices.len();
        for (corner, uv) in base_uvs.iter().enumerate() {
            vertices.push(UvVertex {
                uv: RastVec2::new(uv.x + offset_u, uv.y + offset_v),
                vertex_index: face_idx * 4 + corner,
            });
        }

        // Add face referencing these vertices
        faces.push(UvFace {
            vertex_indices: vec![base_idx, base_idx + 1, base_idx + 2, base_idx + 3],
        });
    }

    if faces.is_empty() {
        return None;
    }

    // All faces are selected
    let selected_faces: Vec<usize> = (0..faces.len()).collect();

    Some(UvOverlayData {
        vertices,
        faces,
        selected_faces,
    })
}

/// Apply UV direct drag to all selected faces
fn apply_uv_direct_drag_to_face(
    ctx: &UiContext,
    tex_width: f32,
    tex_height: f32,
    state: &mut EditorState,
) {
    if !state.texture_editor.uv_drag_active {
        return;
    }

    // Collect all face selections (same as build_uv_overlay_from_selection)
    let mut face_selections: Vec<(usize, usize, usize, super::SectorFace)> = Vec::new();
    if let super::Selection::SectorFace { room, x, z, face } = &state.selection {
        face_selections.push((*room, *x, *z, face.clone()));
    }
    for sel in &state.multi_selection {
        if let super::Selection::SectorFace { room, x, z, face } = sel {
            let key = (*room, *x, *z, face.clone());
            if !face_selections.iter().any(|s| s.0 == key.0 && s.1 == key.1 && s.2 == key.2 && s.3 == key.3) {
                face_selections.push(key);
            }
        }
    }

    if face_selections.is_empty() {
        return;
    }

    // Calculate min x,z for offset (same as build_uv_overlay_from_selection)
    let min_x = face_selections.iter().map(|(_, x, _, _)| *x).min().unwrap_or(0);
    let min_z = face_selections.iter().map(|(_, _, z, _)| *z).min().unwrap_or(0);

    // Calculate UV delta from drag
    let zoom = state.texture_editor.zoom;
    let (start_mx, start_my) = state.texture_editor.uv_drag_start;
    let delta_screen_x = ctx.mouse.x - start_mx;
    let delta_screen_y = ctx.mouse.y - start_my;
    let delta_u = delta_screen_x / (tex_width * zoom);
    let delta_v = -delta_screen_y / (tex_height * zoom); // Inverted Y

    // Group dragged vertices by face index
    // vertex_index encodes face_idx * 4 + corner
    let mut face_changes: std::collections::HashMap<usize, Vec<(usize, RastVec2)>> = std::collections::HashMap::new();
    for &(_, vi, original_uv) in &state.texture_editor.uv_drag_start_uvs {
        let face_idx = vi / 4;
        let corner = vi % 4;
        face_changes.entry(face_idx).or_default().push((corner, original_uv));
    }

    // Apply changes to each affected face
    for (face_idx, changes) in face_changes {
        if face_idx >= face_selections.len() {
            continue;
        }
        let (room, x, z, face) = &face_selections[face_idx];

        // Calculate the offset that was applied to this face's UVs in the display
        let (offset_u, offset_v) = match face {
            super::SectorFace::Floor | super::SectorFace::Ceiling => {
                let dx = (*x - min_x) as f32;
                let dz = (*z - min_z) as f32;
                (dx, dz)
            }
            _ => (0.0, 0.0),
        };

        // Get current face UVs
        let current_uvs = if let Some(r) = state.level.rooms.get(*room) {
            if let Some(sector) = r.get_sector(*x, *z) {
                match face {
                    super::SectorFace::Floor => sector.floor.as_ref().and_then(|f| f.uv),
                    super::SectorFace::Ceiling => sector.ceiling.as_ref().and_then(|f| f.uv),
                    super::SectorFace::WallNorth(i) => sector.walls_north.get(*i).and_then(|w| w.uv),
                    super::SectorFace::WallEast(i) => sector.walls_east.get(*i).and_then(|w| w.uv),
                    super::SectorFace::WallSouth(i) => sector.walls_south.get(*i).and_then(|w| w.uv),
                    super::SectorFace::WallWest(i) => sector.walls_west.get(*i).and_then(|w| w.uv),
                    super::SectorFace::WallNwSe(i) => sector.walls_nwse.get(*i).and_then(|w| w.uv),
                    super::SectorFace::WallNeSw(i) => sector.walls_nesw.get(*i).and_then(|w| w.uv),
                }
            } else {
                None
            }
        } else {
            None
        };

        // Start with current UVs or defaults
        let mut new_uvs = current_uvs.unwrap_or([
            RastVec2::new(0.0, 0.0),
            RastVec2::new(1.0, 0.0),
            RastVec2::new(1.0, 1.0),
            RastVec2::new(0.0, 1.0),
        ]);

        // Apply delta to dragged corners with pixel snapping
        // The original_uv includes the display offset, so we need to subtract it
        for (corner, original_uv) in changes {
            if corner < 4 {
                // original_uv has offset applied, add delta, then subtract offset for storage
                let new_u = original_uv.x + delta_u - offset_u;
                let new_v = original_uv.y + delta_v - offset_v;
                new_uvs[corner].x = (new_u * tex_width).round() / tex_width;
                new_uvs[corner].y = (new_v * tex_height).round() / tex_height;
            }
        }

        // Update the face's UV field
        if let Some(r) = state.level.rooms.get_mut(*room) {
            if let Some(sector) = r.get_sector_mut(*x, *z) {
                match face {
                    super::SectorFace::Floor => {
                        if let Some(f) = &mut sector.floor {
                            f.uv = Some(new_uvs);
                        }
                    }
                    super::SectorFace::Ceiling => {
                        if let Some(f) = &mut sector.ceiling {
                            f.uv = Some(new_uvs);
                        }
                    }
                    super::SectorFace::WallNorth(i) => {
                        if let Some(w) = sector.walls_north.get_mut(*i) {
                            w.uv = Some(new_uvs);
                        }
                    }
                    super::SectorFace::WallEast(i) => {
                        if let Some(w) = sector.walls_east.get_mut(*i) {
                            w.uv = Some(new_uvs);
                        }
                    }
                    super::SectorFace::WallSouth(i) => {
                        if let Some(w) = sector.walls_south.get_mut(*i) {
                            w.uv = Some(new_uvs);
                        }
                    }
                    super::SectorFace::WallWest(i) => {
                        if let Some(w) = sector.walls_west.get_mut(*i) {
                            w.uv = Some(new_uvs);
                        }
                    }
                    super::SectorFace::WallNwSe(i) => {
                        if let Some(w) = sector.walls_nwse.get_mut(*i) {
                            w.uv = Some(new_uvs);
                        }
                    }
                    super::SectorFace::WallNeSw(i) => {
                        if let Some(w) = sector.walls_nesw.get_mut(*i) {
                            w.uv = Some(new_uvs);
                        }
                    }
                }
            }
        }
    }
}

/// Apply UV operation (flip/rotate/reset) to all selected faces
fn apply_uv_operation_to_face(
    tex_width: f32,
    tex_height: f32,
    state: &mut EditorState,
) {
    use crate::texture::UvOperation;

    let operation = match state.texture_editor.uv_operation_pending.take() {
        Some(op) => op,
        None => return,
    };

    // Collect all face selections
    let mut face_selections: Vec<(usize, usize, usize, super::SectorFace)> = Vec::new();
    if let super::Selection::SectorFace { room, x, z, face } = &state.selection {
        face_selections.push((*room, *x, *z, face.clone()));
    }
    for sel in &state.multi_selection {
        if let super::Selection::SectorFace { room, x, z, face } = sel {
            let key = (*room, *x, *z, face.clone());
            if !face_selections.iter().any(|s| s.0 == key.0 && s.1 == key.1 && s.2 == key.2 && s.3 == key.3) {
                face_selections.push(key);
            }
        }
    }

    if face_selections.is_empty() {
        return;
    }

    let face_count = face_selections.len();

    // Apply operation to each face
    for (room, x, z, face) in face_selections {
        // Get current face UVs
        let current_uvs = if let Some(r) = state.level.rooms.get(room) {
            if let Some(sector) = r.get_sector(x, z) {
                match &face {
                    super::SectorFace::Floor => sector.floor.as_ref().and_then(|f| f.uv),
                    super::SectorFace::Ceiling => sector.ceiling.as_ref().and_then(|f| f.uv),
                    super::SectorFace::WallNorth(i) => sector.walls_north.get(*i).and_then(|w| w.uv),
                    super::SectorFace::WallEast(i) => sector.walls_east.get(*i).and_then(|w| w.uv),
                    super::SectorFace::WallSouth(i) => sector.walls_south.get(*i).and_then(|w| w.uv),
                    super::SectorFace::WallWest(i) => sector.walls_west.get(*i).and_then(|w| w.uv),
                    super::SectorFace::WallNwSe(i) => sector.walls_nwse.get(*i).and_then(|w| w.uv),
                    super::SectorFace::WallNeSw(i) => sector.walls_nesw.get(*i).and_then(|w| w.uv),
                }
            } else {
                None
            }
        } else {
            None
        };

        let uvs = current_uvs.unwrap_or([
            RastVec2::new(0.0, 0.0),
            RastVec2::new(1.0, 0.0),
            RastVec2::new(1.0, 1.0),
            RastVec2::new(0.0, 1.0),
        ]);

        // Calculate center for operations
        let center_u = (uvs[0].x + uvs[1].x + uvs[2].x + uvs[3].x) / 4.0;
        let center_v = (uvs[0].y + uvs[1].y + uvs[2].y + uvs[3].y) / 4.0;

        let new_uvs: [RastVec2; 4] = match operation {
            UvOperation::FlipHorizontal => {
                [
                    RastVec2::new(2.0 * center_u - uvs[0].x, uvs[0].y),
                    RastVec2::new(2.0 * center_u - uvs[1].x, uvs[1].y),
                    RastVec2::new(2.0 * center_u - uvs[2].x, uvs[2].y),
                    RastVec2::new(2.0 * center_u - uvs[3].x, uvs[3].y),
                ]
            }
            UvOperation::FlipVertical => {
                [
                    RastVec2::new(uvs[0].x, 2.0 * center_v - uvs[0].y),
                    RastVec2::new(uvs[1].x, 2.0 * center_v - uvs[1].y),
                    RastVec2::new(uvs[2].x, 2.0 * center_v - uvs[2].y),
                    RastVec2::new(uvs[3].x, 2.0 * center_v - uvs[3].y),
                ]
            }
            UvOperation::RotateCW => {
                let mut result = [RastVec2::new(0.0, 0.0); 4];
                for i in 0..4 {
                    let dx = uvs[i].x - center_u;
                    let dy = uvs[i].y - center_v;
                    let new_u = center_u + dy;
                    let new_v = center_v - dx;
                    result[i].x = (new_u * tex_width).round() / tex_width;
                    result[i].y = (new_v * tex_height).round() / tex_height;
                }
                result
            }
            UvOperation::ResetUVs => {
                [
                    RastVec2::new(0.0, 0.0),
                    RastVec2::new(1.0, 0.0),
                    RastVec2::new(1.0, 1.0),
                    RastVec2::new(0.0, 1.0),
                ]
            }
        };

        // Update the face's UV field
        if let Some(r) = state.level.rooms.get_mut(room) {
            if let Some(sector) = r.get_sector_mut(x, z) {
                match &face {
                    super::SectorFace::Floor => {
                        if let Some(f) = &mut sector.floor {
                            f.uv = Some(new_uvs);
                        }
                    }
                    super::SectorFace::Ceiling => {
                        if let Some(f) = &mut sector.ceiling {
                            f.uv = Some(new_uvs);
                        }
                    }
                    super::SectorFace::WallNorth(i) => {
                        if let Some(w) = sector.walls_north.get_mut(*i) {
                            w.uv = Some(new_uvs);
                        }
                    }
                    super::SectorFace::WallEast(i) => {
                        if let Some(w) = sector.walls_east.get_mut(*i) {
                            w.uv = Some(new_uvs);
                        }
                    }
                    super::SectorFace::WallSouth(i) => {
                        if let Some(w) = sector.walls_south.get_mut(*i) {
                            w.uv = Some(new_uvs);
                        }
                    }
                    super::SectorFace::WallWest(i) => {
                        if let Some(w) = sector.walls_west.get_mut(*i) {
                            w.uv = Some(new_uvs);
                        }
                    }
                    super::SectorFace::WallNwSe(i) => {
                        if let Some(w) = sector.walls_nwse.get_mut(*i) {
                            w.uv = Some(new_uvs);
                        }
                    }
                    super::SectorFace::WallNeSw(i) => {
                        if let Some(w) = sector.walls_nesw.get_mut(*i) {
                            w.uv = Some(new_uvs);
                        }
                    }
                }
            }
        }
    }

    state.texture_editor.set_status(&format!("{:?} applied to {} face(s)", operation, face_count));
}

/// Apply UV modal transforms (G/T/R) to all selected faces
fn apply_uv_modal_transform_to_face(
    ctx: &UiContext,
    tex_width: f32,
    tex_height: f32,
    state: &mut EditorState,
) {
    use crate::texture::UvModalTransform;

    let transform = state.texture_editor.uv_modal_transform;
    // Only apply transforms for active states (not None or ScalePending)
    if transform == UvModalTransform::None || transform == UvModalTransform::ScalePending {
        return;
    }

    // Collect all face selections
    let mut face_selections: Vec<(usize, usize, usize, super::SectorFace)> = Vec::new();
    if let super::Selection::SectorFace { room, x, z, face } = &state.selection {
        face_selections.push((*room, *x, *z, face.clone()));
    }
    for sel in &state.multi_selection {
        if let super::Selection::SectorFace { room, x, z, face } = sel {
            let key = (*room, *x, *z, face.clone());
            if !face_selections.iter().any(|s| s.0 == key.0 && s.1 == key.1 && s.2 == key.2 && s.3 == key.3) {
                face_selections.push(key);
            }
        }
    }

    if face_selections.is_empty() {
        return;
    }

    // Calculate min x,z for offset (same as build_uv_overlay_from_selection)
    let min_x = face_selections.iter().map(|(_, x, _, _)| *x).min().unwrap_or(0);
    let min_z = face_selections.iter().map(|(_, _, z, _)| *z).min().unwrap_or(0);

    // Calculate transform parameters
    let zoom = state.texture_editor.zoom;
    let (start_mx, start_my) = state.texture_editor.uv_modal_start_mouse;

    // Screen delta in UV space
    let delta_screen_x = ctx.mouse.x - start_mx;
    let delta_screen_y = ctx.mouse.y - start_my;
    let delta_u = delta_screen_x / (tex_width * zoom);
    let delta_v = -delta_screen_y / (tex_height * zoom); // Inverted Y

    // Group dragged vertices by face index
    // vertex_index encodes face_idx * 4 + corner
    let mut face_changes: std::collections::HashMap<usize, Vec<(usize, RastVec2)>> = std::collections::HashMap::new();
    for &(vi, original_uv) in &state.texture_editor.uv_modal_start_uvs {
        let face_idx = vi / 4;
        let corner = vi % 4;
        face_changes.entry(face_idx).or_default().push((corner, original_uv));
    }

    // Apply transform to each affected face
    for (face_idx, changes) in face_changes {
        if face_idx >= face_selections.len() {
            continue;
        }
        let (room, x, z, face) = &face_selections[face_idx];

        // Calculate the offset that was applied to this face's UVs in the display
        let (offset_u, offset_v) = match face {
            super::SectorFace::Floor | super::SectorFace::Ceiling => {
                let dx = (*x - min_x) as f32;
                let dz = (*z - min_z) as f32;
                (dx, dz)
            }
            _ => (0.0, 0.0),
        };

        // Get current face UVs
        let current_uvs = if let Some(r) = state.level.rooms.get(*room) {
            if let Some(sector) = r.get_sector(*x, *z) {
                match face {
                    super::SectorFace::Floor => sector.floor.as_ref().and_then(|f| f.uv),
                    super::SectorFace::Ceiling => sector.ceiling.as_ref().and_then(|f| f.uv),
                    super::SectorFace::WallNorth(i) => sector.walls_north.get(*i).and_then(|w| w.uv),
                    super::SectorFace::WallEast(i) => sector.walls_east.get(*i).and_then(|w| w.uv),
                    super::SectorFace::WallSouth(i) => sector.walls_south.get(*i).and_then(|w| w.uv),
                    super::SectorFace::WallWest(i) => sector.walls_west.get(*i).and_then(|w| w.uv),
                    super::SectorFace::WallNwSe(i) => sector.walls_nwse.get(*i).and_then(|w| w.uv),
                    super::SectorFace::WallNeSw(i) => sector.walls_nesw.get(*i).and_then(|w| w.uv),
                }
            } else {
                None
            }
        } else {
            None
        };

        // Start with current UVs or defaults
        let mut new_uvs = current_uvs.unwrap_or([
            RastVec2::new(0.0, 0.0),
            RastVec2::new(1.0, 0.0),
            RastVec2::new(1.0, 1.0),
            RastVec2::new(0.0, 1.0),
        ]);

        // Apply transform based on type
        match transform {
            UvModalTransform::Grab => {
                // Move selected vertices by delta with pixel snapping
                for (corner, original_uv) in changes {
                    if corner < 4 {
                        // original_uv has offset applied, add delta, then subtract offset for storage
                        let new_u = original_uv.x + delta_u - offset_u;
                        let new_v = original_uv.y + delta_v - offset_v;
                        new_uvs[corner].x = (new_u * tex_width).round() / tex_width;
                        new_uvs[corner].y = (new_v * tex_height).round() / tex_height;
                    }
                }
            }
            UvModalTransform::Scale => {
                // Scale around center - snap center to pixel boundary for consistent results
                let raw_center = state.texture_editor.uv_modal_center;
                let center = RastVec2::new(
                    (raw_center.x * tex_width).round() / tex_width,
                    (raw_center.y * tex_height).round() / tex_height,
                );
                // Scale factor based on horizontal mouse movement
                let scale = 1.0 + delta_screen_x * 0.01;
                let scale = scale.max(0.01); // Prevent negative/zero scale

                for (corner, original_uv) in changes {
                    if corner < 4 {
                        // Snap original UV to pixel boundary for consistent scaling
                        let snapped_orig = RastVec2::new(
                            (original_uv.x * tex_width).round() / tex_width,
                            (original_uv.y * tex_height).round() / tex_height,
                        );
                        let offset_x = snapped_orig.x - center.x;
                        let offset_y = snapped_orig.y - center.y;
                        let scaled_u = center.x + offset_x * scale - offset_u;
                        let scaled_v = center.y + offset_y * scale - offset_v;
                        new_uvs[corner].x = (scaled_u * tex_width).round() / tex_width;
                        new_uvs[corner].y = (scaled_v * tex_height).round() / tex_height;
                    }
                }
            }
            UvModalTransform::Rotate => {
                // Rotate around center with pixel snapping
                let center = state.texture_editor.uv_modal_center;
                // Rotation angle based on horizontal mouse movement
                let angle = delta_screen_x * 0.01; // Radians
                let cos_a = angle.cos();
                let sin_a = angle.sin();

                for (corner, original_uv) in changes {
                    if corner < 4 {
                        let offset_x = original_uv.x - center.x;
                        let offset_y = original_uv.y - center.y;
                        let rotated_u = center.x + offset_x * cos_a - offset_y * sin_a - offset_u;
                        let rotated_v = center.y + offset_x * sin_a + offset_y * cos_a - offset_v;
                        new_uvs[corner].x = (rotated_u * tex_width).round() / tex_width;
                        new_uvs[corner].y = (rotated_v * tex_height).round() / tex_height;
                    }
                }
            }
            UvModalTransform::None | UvModalTransform::ScalePending => {}
        }

        // Update the face's UV field
        if let Some(r) = state.level.rooms.get_mut(*room) {
            if let Some(sector) = r.get_sector_mut(*x, *z) {
                match face {
                    super::SectorFace::Floor => {
                        if let Some(f) = &mut sector.floor {
                            f.uv = Some(new_uvs);
                        }
                    }
                    super::SectorFace::Ceiling => {
                        if let Some(f) = &mut sector.ceiling {
                            f.uv = Some(new_uvs);
                        }
                    }
                    super::SectorFace::WallNorth(i) => {
                        if let Some(w) = sector.walls_north.get_mut(*i) {
                            w.uv = Some(new_uvs);
                        }
                    }
                    super::SectorFace::WallEast(i) => {
                        if let Some(w) = sector.walls_east.get_mut(*i) {
                            w.uv = Some(new_uvs);
                        }
                    }
                    super::SectorFace::WallSouth(i) => {
                        if let Some(w) = sector.walls_south.get_mut(*i) {
                            w.uv = Some(new_uvs);
                        }
                    }
                    super::SectorFace::WallWest(i) => {
                        if let Some(w) = sector.walls_west.get_mut(*i) {
                            w.uv = Some(new_uvs);
                        }
                    }
                    super::SectorFace::WallNwSe(i) => {
                        if let Some(w) = sector.walls_nwse.get_mut(*i) {
                            w.uv = Some(new_uvs);
                        }
                    }
                    super::SectorFace::WallNeSw(i) => {
                        if let Some(w) = sector.walls_nesw.get_mut(*i) {
                            w.uv = Some(new_uvs);
                        }
                    }
                }
            }
        }
    }
}
