//! Game Renderer
//!
//! Renders the game view from project data and ECS world.
//! Combines static level geometry with dynamic entities.

use std::collections::HashMap;
use macroquad::prelude::*;
use crate::rasterizer::{
    Framebuffer, Texture as RasterTexture, render_mesh,
    Light, RasterSettings, ShadingMode, Color as RasterColor,
    Vec3, WIDTH, HEIGHT,
};
use crate::ui::Rect;
use crate::world::Level;
use super::runtime::GameToolState;

/// Draw the game viewport
pub fn draw_game_viewport(
    rect: Rect,
    game: &mut GameToolState,
    level: &Level,
    textures: &[RasterTexture],
    fb: &mut Framebuffer,
) {
    // Resize framebuffer to match game resolution
    fb.resize(WIDTH, HEIGHT);

    // Initialize camera from level's player start (only once)
    game.init_from_level(level);

    // Handle camera input (orbit camera for now)
    handle_camera_input(game, &rect);

    // Clear framebuffer to dark gray
    fb.clear(RasterColor::new(20, 22, 28));

    // Build texture map for looking up textures by (pack, name)
    let mut texture_map: HashMap<(String, String), usize> = HashMap::new();
    // Build from textures array - we don't have pack info here, so match by name only
    for (idx, tex) in textures.iter().enumerate() {
        texture_map.insert((String::new(), tex.name.clone()), idx);
    }

    // Texture resolver closure
    let resolve_texture = |tex_ref: &crate::world::TextureRef| -> Option<usize> {
        if !tex_ref.is_valid() {
            return Some(0); // Fallback to first texture
        }
        // Try finding by name in the textures array directly
        textures.iter().position(|t| t.name == tex_ref.name)
    };

    // Build lighting settings
    let render_settings = if game.raster_settings.shading != ShadingMode::None {
        // Collect all enabled lights from all rooms
        let mut lights = Vec::new();
        for room in &level.rooms {
            for room_light in &room.lights {
                if room_light.enabled {
                    // Convert room-local position to world position
                    let world_pos = Vec3::new(
                        room.position.x + room_light.position.x,
                        room.position.y + room_light.position.y,
                        room.position.z + room_light.position.z,
                    );
                    let mut light = Light::point(world_pos, room_light.radius, room_light.intensity);
                    light.color = room_light.color;
                    lights.push(light);
                }
            }
        }

        // Use ambient from first room (or default)
        let ambient = level.rooms.first()
            .map(|r| r.ambient)
            .unwrap_or(0.5);

        RasterSettings {
            lights,
            ambient,
            ..game.raster_settings.clone()
        }
    } else {
        game.raster_settings.clone()
    };

    // Render all rooms
    for room in &level.rooms {
        let (vertices, faces) = room.to_render_data_with_textures(&resolve_texture);
        render_mesh(fb, &vertices, &faces, textures, &game.camera, &render_settings);
    }

    // Convert framebuffer to texture and draw to viewport
    let texture = Texture2D::from_rgba8(fb.width as u16, fb.height as u16, &fb.pixels);
    texture.set_filter(FilterMode::Nearest);

    // Calculate draw area maintaining aspect ratio (4:3 for PS1)
    let fb_aspect = fb.width as f32 / fb.height as f32;
    let rect_aspect = rect.w / rect.h;
    let (draw_w, draw_h, draw_x, draw_y) = if fb_aspect > rect_aspect {
        let w = rect.w;
        let h = rect.w / fb_aspect;
        (w, h, rect.x, rect.y + (rect.h - h) * 0.5)
    } else {
        let h = rect.h;
        let w = rect.h * fb_aspect;
        (w, h, rect.x + (rect.w - w) * 0.5, rect.y)
    };

    // Draw letterbox bars
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(10, 10, 12, 255));

    // Draw the rendered frame
    draw_texture_ex(
        &texture,
        draw_x,
        draw_y,
        WHITE,
        DrawTextureParams {
            dest_size: Some(vec2(draw_w, draw_h)),
            ..Default::default()
        },
    );

    // Draw play/pause indicator
    let status = if game.playing { "PLAYING" } else { "PAUSED (Space to play)" };
    let status_dims = measure_text(status, None, 14, 1.0);
    draw_text(
        status,
        rect.x + (rect.w - status_dims.width) / 2.0,
        rect.y + rect.h - 20.0,
        14.0,
        Color::from_rgba(150, 150, 160, 200),
    );

    // Draw controls hint
    let hint = "Right-click drag: Rotate | Scroll: Zoom";
    let hint_dims = measure_text(hint, None, 12, 1.0);
    draw_text(
        hint,
        rect.x + (rect.w - hint_dims.width) / 2.0,
        rect.y + 20.0,
        12.0,
        Color::from_rgba(100, 100, 110, 180),
    );
}

/// Handle camera input for the game viewport
fn handle_camera_input(game: &mut GameToolState, rect: &Rect) {
    let mouse_pos = mouse_position();
    let inside = mouse_pos.0 >= rect.x
        && mouse_pos.0 < rect.x + rect.w
        && mouse_pos.1 >= rect.y
        && mouse_pos.1 < rect.y + rect.h;

    // Toggle play with space
    if inside && is_key_pressed(KeyCode::Space) {
        game.toggle_playing();
    }

    // Orbit camera controls (right-click drag)
    if inside && is_mouse_button_down(MouseButton::Right) {
        let dx = mouse_pos.0 - game.viewport_last_mouse.0;
        let dy = mouse_pos.1 - game.viewport_last_mouse.1;

        game.orbit_azimuth -= dx * 0.005;
        game.orbit_elevation = (game.orbit_elevation + dy * 0.005)
            .clamp(-1.4, 1.4);

        game.sync_camera_from_orbit();
        game.viewport_mouse_captured = true;
    } else {
        game.viewport_mouse_captured = false;
    }

    // Zoom with scroll wheel
    if inside {
        let scroll = mouse_wheel().1;
        if scroll.abs() > 0.1 {
            game.orbit_distance *= 1.0 - scroll * 0.1;
            game.orbit_distance = game.orbit_distance.clamp(500.0, 20000.0);
            game.sync_camera_from_orbit();
        }
    }

    game.viewport_last_mouse = mouse_pos;
}
