//! Texture Palette - Grid of available textures

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext};
use crate::rasterizer::Texture as RasterTexture;
use super::EditorState;

/// Size of texture thumbnails in the palette
const THUMB_SIZE: f32 = 48.0;
const THUMB_PADDING: f32 = 4.0;

/// Draw the texture palette
pub fn draw_texture_palette(
    ctx: &mut UiContext,
    rect: Rect,
    state: &mut EditorState,
    textures: &[RasterTexture],
) {
    // Background
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(25, 25, 30, 255));

    if textures.is_empty() {
        draw_text("No textures", rect.x + 10.0, rect.y + 20.0, 14.0, Color::from_rgba(100, 100, 100, 255));
        return;
    }

    // Calculate grid layout
    let cols = ((rect.w - THUMB_PADDING) / (THUMB_SIZE + THUMB_PADDING)).floor() as usize;
    let cols = cols.max(1);

    // Draw texture grid
    for (i, texture) in textures.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;

        let x = rect.x + THUMB_PADDING + col as f32 * (THUMB_SIZE + THUMB_PADDING);
        let y = rect.y + THUMB_PADDING + row as f32 * (THUMB_SIZE + THUMB_PADDING);

        // Skip if outside visible area
        if y > rect.bottom() {
            break;
        }
        if y + THUMB_SIZE < rect.y {
            continue;
        }

        let thumb_rect = Rect::new(x, y, THUMB_SIZE, THUMB_SIZE);

        // Check for click
        if ctx.mouse.clicked(&thumb_rect) {
            state.selected_texture = i;
        }

        // Draw texture thumbnail
        // Convert raster texture to macroquad texture
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

        // Selection highlight
        if i == state.selected_texture {
            draw_rectangle_lines(x - 2.0, y - 2.0, THUMB_SIZE + 4.0, THUMB_SIZE + 4.0, 2.0, Color::from_rgba(255, 200, 50, 255));
        }

        // Hover highlight
        if ctx.mouse.inside(&thumb_rect) && i != state.selected_texture {
            draw_rectangle_lines(x - 1.0, y - 1.0, THUMB_SIZE + 2.0, THUMB_SIZE + 2.0, 1.0, Color::from_rgba(150, 150, 200, 255));
        }

        // Texture index
        draw_text(&format!("{}", i), x + 2.0, y + THUMB_SIZE - 2.0, 10.0, Color::from_rgba(255, 255, 255, 200));
    }
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
