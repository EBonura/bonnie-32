//! Texture Palette - Grid of available textures with folder selection

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext, icon, draw_icon_centered};
use crate::rasterizer::Texture as RasterTexture;
use super::EditorState;

/// Size of texture thumbnails in the palette
const THUMB_SIZE: f32 = 48.0;
const THUMB_PADDING: f32 = 4.0;
const HEADER_HEIGHT: f32 = 28.0;

/// Draw the texture palette
pub fn draw_texture_palette(
    ctx: &mut UiContext,
    rect: Rect,
    state: &mut EditorState,
    icon_font: Option<&Font>,
) {
    // Background
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(25, 25, 30, 255));

    // Draw folder selector header
    let header_rect = Rect::new(rect.x, rect.y, rect.w, HEADER_HEIGHT);
    draw_folder_selector(ctx, header_rect, state, icon_font);

    // Content area (below header)
    let content_rect = Rect::new(rect.x, rect.y + HEADER_HEIGHT, rect.w, rect.h - HEADER_HEIGHT);

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
    let cols = ((content_rect.w - THUMB_PADDING) / (THUMB_SIZE + THUMB_PADDING)).floor() as usize;
    let cols = cols.max(1);
    let rows = (texture_count + cols - 1) / cols;
    let total_height = rows as f32 * (THUMB_SIZE + THUMB_PADDING) + THUMB_PADDING;

    // Handle scrolling
    if ctx.mouse.inside(&content_rect) {
        state.texture_scroll -= ctx.mouse.scroll * 12.0;
        // Clamp scroll
        let max_scroll = (total_height - content_rect.h).max(0.0);
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

    // Draw texture grid by index to avoid borrowing issues
    for i in 0..texture_count {
        let col = i % cols;
        let row = i / cols;

        let x = content_rect.x + THUMB_PADDING + col as f32 * (THUMB_SIZE + THUMB_PADDING);
        let y = content_rect.y + THUMB_PADDING + row as f32 * (THUMB_SIZE + THUMB_PADDING) - texture_scroll;

        // Skip if outside visible area
        if y + THUMB_SIZE < content_rect.y || y > content_rect.bottom() {
            continue;
        }

        let thumb_rect = Rect::new(x, y, THUMB_SIZE, THUMB_SIZE);

        // Clip drawing to content area
        if y < content_rect.y {
            continue; // Skip partial textures at top
        }

        // Get texture and pack from state
        let (texture, pack_name) = match state.texture_packs.get(selected_pack) {
            Some(pack) => match pack.textures.get(i) {
                Some(tex) => (tex, &pack.name),
                None => continue,
            },
            None => continue,
        };

        // Check for click (only if fully visible)
        if y >= content_rect.y && y + THUMB_SIZE <= content_rect.bottom() {
            if ctx.mouse.clicked(&thumb_rect) {
                clicked_texture = Some(crate::world::TextureRef::new(pack_name.clone(), texture.name.clone()));
            }
        }

        // Draw texture thumbnail
        let mq_texture = raster_to_mq_texture(texture);
        draw_texture_ex(
            &mq_texture,
            x,
            y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(Vec2::new(THUMB_SIZE, THUMB_SIZE)),
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
                THUMB_SIZE + 4.0,
                THUMB_SIZE + 4.0,
                2.0,
                Color::from_rgba(255, 200, 50, 255),
            );
        }

        // Hover highlight
        if ctx.mouse.inside(&thumb_rect) && !is_selected {
            draw_rectangle_lines(
                x - 1.0,
                y - 1.0,
                THUMB_SIZE + 2.0,
                THUMB_SIZE + 2.0,
                1.0,
                Color::from_rgba(150, 150, 200, 255),
            );
        }

        // Texture index
        draw_text(
            &format!("{}", i),
            (x + 2.0).floor(),
            (y + THUMB_SIZE - 2.0).floor(),
            12.0,
            Color::from_rgba(255, 255, 255, 200),
        );
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

            // Apply texture to all selections
            for sel in all_selections {
                apply_texture_to_selection(&mut state.level, sel, tex_ref.clone());
            }
        }
    }
}

/// Apply a texture to a single selection
fn apply_texture_to_selection(level: &mut crate::world::Level, selection: super::Selection, tex_ref: crate::world::TextureRef) {
    match selection {
        // Single face selected (from 3D view) - apply to that face only
        super::Selection::SectorFace { room, x, z, face } => {
            if let Some(r) = level.rooms.get_mut(room) {
                if let Some(sector) = r.get_sector_mut(x, z) {
                    match face {
                        super::SectorFace::Floor => {
                            if let Some(floor) = &mut sector.floor {
                                floor.texture = tex_ref;
                            }
                        }
                        super::SectorFace::Ceiling => {
                            if let Some(ceiling) = &mut sector.ceiling {
                                ceiling.texture = tex_ref;
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
                    }
                }
            }
        }
        // Whole sector selected (from 2D view) - apply to all faces
        super::Selection::Sector { room, x, z } => {
            if let Some(r) = level.rooms.get_mut(room) {
                if let Some(sector) = r.get_sector_mut(x, z) {
                    // Apply to floor if it exists
                    if let Some(floor) = &mut sector.floor {
                        floor.texture = tex_ref.clone();
                    }
                    // Apply to ceiling if it exists
                    if let Some(ceiling) = &mut sector.ceiling {
                        ceiling.texture = tex_ref.clone();
                    }
                    // Apply to all walls
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

    // Next button - flat icon style
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

    // Pack name in center - properly centered vertically
    let name = state.current_pack_name();
    let pack_count = state.texture_packs.len();
    let label = format!("{} ({}/{})", name, state.selected_pack + 1, pack_count);
    let font_size = 14.0;
    let text_dims = measure_text(&label, None, font_size as u16, 1.0);
    let text_x = (rect.x + (rect.w - text_dims.width) * 0.5).round();
    let text_y = (rect.y + (rect.h + text_dims.height) * 0.5).round();
    draw_text(&label, text_x, text_y, font_size, WHITE);
}

/// Convert a raster texture to a macroquad texture
fn raster_to_mq_texture(texture: &RasterTexture) -> Texture2D {
    // Convert RGBA pixels
    let mut pixels = Vec::with_capacity(texture.width * texture.height * 4);
    for y in 0..texture.height {
        for x in 0..texture.width {
            let color = texture.get_pixel(x, y);
            pixels.push(color.r);
            pixels.push(color.g);
            pixels.push(color.b);
            pixels.push(color.a);
        }
    }

    let tex = Texture2D::from_rgba8(texture.width as u16, texture.height as u16, &pixels);
    tex.set_filter(FilterMode::Nearest);
    tex
}
