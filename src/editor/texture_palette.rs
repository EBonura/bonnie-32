//! Texture Palette - Grid of available textures with folder selection
//!
//! Supports two modes:
//! - Source PNGs: Read-only textures from assets/textures/
//! - User Textures: Editable indexed textures from assets/textures-user/

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext, icon, draw_icon_centered};
use crate::rasterizer::{Texture as RasterTexture, ClutDepth};
use crate::texture::{UserTexture, TextureSize, draw_texture_canvas, draw_tool_panel, draw_palette_panel};
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
    // Background
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(25, 25, 30, 255));

    // If editing a texture, show the texture editor instead
    if state.editing_texture.is_some() {
        draw_texture_editor_panel(ctx, rect, state, icon_font);
        return;
    }

    // Mode toggle tabs (Source PNGs | User Textures)
    let mode_rect = Rect::new(rect.x, rect.y, rect.w, MODE_TOGGLE_HEIGHT);
    draw_mode_toggle(ctx, mode_rect, state);

    // Header area (folder selector for source PNGs, action buttons for user textures)
    let header_rect = Rect::new(rect.x, rect.y + MODE_TOGGLE_HEIGHT, rect.w, HEADER_HEIGHT);

    if state.texture_palette_user_mode {
        draw_user_texture_header(ctx, header_rect, state, icon_font);
        let content_rect = Rect::new(rect.x, rect.y + MODE_TOGGLE_HEIGHT + HEADER_HEIGHT, rect.w, rect.h - MODE_TOGGLE_HEIGHT - HEADER_HEIGHT);
        draw_user_texture_grid(ctx, content_rect, state);
    } else {
        draw_folder_selector(ctx, header_rect, state, icon_font);
        let content_rect = Rect::new(rect.x, rect.y + MODE_TOGGLE_HEIGHT + HEADER_HEIGHT, rect.w, rect.h - MODE_TOGGLE_HEIGHT - HEADER_HEIGHT);
        draw_source_texture_grid(ctx, content_rect, state);
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
        // Create a new 64x64 texture with default name
        let name = state.user_textures.generate_unique_name("texture");
        let tex = UserTexture::new(&name, TextureSize::Size64x64, ClutDepth::Bpp4);
        state.user_textures.add(tex);
        state.editing_texture = Some(name.clone());
        // Reset texture editor state for new texture
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
    let content_rect = Rect::new(rect.x, rect.y + header_h, rect.w, rect.h - header_h);

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
    // Canvas should be square based on available width, but leave minimum space for palette
    let min_palette_h = 120.0;  // Minimum palette panel height
    let max_canvas_h = (content_rect.h - min_palette_h).max(100.0);  // Leave room for palette
    let canvas_h = canvas_w.min(max_canvas_h);  // Square, but capped to leave palette space
    let palette_panel_h = content_rect.h - canvas_h;  // Remaining space goes to palette

    let canvas_rect = Rect::new(content_rect.x, content_rect.y, canvas_w, canvas_h);
    let tool_rect = Rect::new(content_rect.x + canvas_w, content_rect.y, tool_panel_w, canvas_h);
    let palette_rect = Rect::new(content_rect.x, content_rect.y + canvas_h, content_rect.w, palette_panel_h);

    // Draw panels
    draw_texture_canvas(ctx, canvas_rect, tex, &mut state.texture_editor);
    draw_tool_panel(ctx, tool_rect, &mut state.texture_editor, icon_font);
    draw_palette_panel(ctx, palette_rect, tex, &mut state.texture_editor, icon_font);

    // Handle undo/redo requests
    if state.texture_editor.undo_requested {
        state.texture_editor.undo_requested = false;
        state.texture_editor.undo(tex);
    }
    if state.texture_editor.redo_requested {
        state.texture_editor.redo_requested = false;
        state.texture_editor.redo(tex);
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
