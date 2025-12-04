//! 3D Viewport - Software rendered preview

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext};
use crate::rasterizer::{Framebuffer, Texture as RasterTexture, RasterSettings, render_mesh, Color as RasterColor};
use super::EditorState;

/// Draw the 3D viewport using the software rasterizer
pub fn draw_viewport_3d(
    ctx: &mut UiContext,
    rect: Rect,
    state: &mut EditorState,
    textures: &[RasterTexture],
    fb: &mut Framebuffer,
    settings: &RasterSettings,
) {
    let mouse_pos = (ctx.mouse.x, ctx.mouse.y);
    let inside_viewport = ctx.mouse.inside(&rect);

    // Camera rotation with right mouse button (same as game mode)
    if ctx.mouse.right_down && inside_viewport {
        if state.viewport_mouse_captured {
            // Calculate delta from last position
            let dx = -(mouse_pos.1 - state.viewport_last_mouse.1) * 0.005;
            let dy = (mouse_pos.0 - state.viewport_last_mouse.0) * 0.005;
            state.camera_3d.rotate(dx, dy);
        }
        state.viewport_mouse_captured = true;
    } else {
        state.viewport_mouse_captured = false;
    }
    state.viewport_last_mouse = mouse_pos;

    // Keyboard camera movement (WASD + Q/E) - only when viewport focused (right click or inside)
    let move_speed = 0.1;
    if inside_viewport || state.viewport_mouse_captured {
        if is_key_down(KeyCode::W) {
            state.camera_3d.position = state.camera_3d.position + state.camera_3d.basis_z * move_speed;
        }
        if is_key_down(KeyCode::S) {
            state.camera_3d.position = state.camera_3d.position - state.camera_3d.basis_z * move_speed;
        }
        if is_key_down(KeyCode::A) {
            state.camera_3d.position = state.camera_3d.position - state.camera_3d.basis_x * move_speed;
        }
        if is_key_down(KeyCode::D) {
            state.camera_3d.position = state.camera_3d.position + state.camera_3d.basis_x * move_speed;
        }
        if is_key_down(KeyCode::Q) {
            state.camera_3d.position = state.camera_3d.position - state.camera_3d.basis_y * move_speed;
        }
        if is_key_down(KeyCode::E) {
            state.camera_3d.position = state.camera_3d.position + state.camera_3d.basis_y * move_speed;
        }
    }

    // Clear framebuffer
    fb.clear(RasterColor::new(30, 30, 40));

    // Render all rooms
    for room in &state.level.rooms {
        let (vertices, faces) = room.to_render_data();
        render_mesh(fb, &vertices, &faces, textures, &state.camera_3d, settings);
    }

    // Convert framebuffer to texture and draw to viewport
    let texture = Texture2D::from_rgba8(fb.width as u16, fb.height as u16, &fb.pixels);
    texture.set_filter(FilterMode::Nearest);

    // Calculate aspect-correct scaling
    let fb_aspect = fb.width as f32 / fb.height as f32;
    let rect_aspect = rect.w / rect.h;

    let (draw_w, draw_h, draw_x, draw_y) = if fb_aspect > rect_aspect {
        // Framebuffer is wider - fit to width
        let w = rect.w;
        let h = rect.w / fb_aspect;
        (w, h, rect.x, rect.y + (rect.h - h) * 0.5)
    } else {
        // Framebuffer is taller - fit to height
        let h = rect.h;
        let w = rect.h * fb_aspect;
        (w, h, rect.x + (rect.w - w) * 0.5, rect.y)
    };

    draw_texture_ex(
        &texture,
        draw_x,
        draw_y,
        WHITE,
        DrawTextureParams {
            dest_size: Some(Vec2::new(draw_w, draw_h)),
            ..Default::default()
        },
    );

    // Draw viewport border
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, Color::from_rgba(60, 60, 60, 255));

    // Draw camera info
    draw_text(
        &format!(
            "Cam: ({:.1}, {:.1}, {:.1})",
            state.camera_3d.position.x,
            state.camera_3d.position.y,
            state.camera_3d.position.z
        ),
        rect.x + 5.0,
        rect.bottom() - 5.0,
        12.0,
        Color::from_rgba(150, 150, 150, 255),
    );
}
