//! Test Renderer
//!
//! Renders the test view from project data and ECS world.
//! Combines static level geometry with dynamic entities.

use macroquad::prelude::*;
use crate::rasterizer::{
    Framebuffer, Texture as RasterTexture, render_mesh,
    Light, RasterSettings, ShadingMode, Color as RasterColor,
    Vec3, project, perspective_transform, WIDTH, HEIGHT,
};
use crate::ui::Rect;
use crate::world::Level;
use super::runtime::GameToolState;

/// Draw the test viewport (full area, no properties panel)
/// Player settings are now edited in the World Editor properties panel when PlayerStart is selected.
pub fn draw_test_viewport(
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

    // Handle camera based on play state
    if game.playing {
        // Third-person camera follows player
        game.update_camera_follow_player(level);
        // Still allow mouse look to rotate player facing
        handle_player_input(game, level, &rect);
    } else {
        // Orbit camera for preview mode
        handle_camera_input(game, &rect);
    }

    // Clear framebuffer to dark gray
    fb.clear(RasterColor::new(20, 22, 28));

    // Texture resolver closure
    let resolve_texture = |tex_ref: &crate::world::TextureRef| -> Option<usize> {
        if !tex_ref.is_valid() {
            return Some(0); // Fallback to first texture
        }
        // Try finding by name in the textures array directly
        textures.iter().position(|t| t.name == tex_ref.name)
    };

    // Build lighting settings
    // Collect all lights from level objects (shared across rooms)
    let lights: Vec<Light> = if game.raster_settings.shading != ShadingMode::None {
        level.objects.iter()
            .filter_map(|obj| {
                if let crate::world::ObjectType::Light { color, intensity, radius } = &obj.object_type {
                    level.rooms.get(obj.room).map(|room| {
                        let world_pos = obj.world_position(room);
                        let mut light = Light::point(world_pos, *radius, *intensity);
                        light.color = *color;
                        light
                    })
                } else {
                    None
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    // Render each room with its own ambient setting
    for room in &level.rooms {
        let render_settings = RasterSettings {
            lights: lights.clone(),
            ambient: room.ambient,
            ..game.raster_settings.clone()
        };
        let (vertices, faces) = room.to_render_data_with_textures(&resolve_texture);
        render_mesh(fb, &vertices, &faces, textures, &game.camera, &render_settings);
    }

    // Render player wireframe cylinder if playing
    if game.playing {
        if let Some(player_pos) = game.get_player_position() {
            let settings = &level.player_settings;
            draw_wireframe_cylinder(
                fb,
                &game.camera,
                player_pos,
                settings.radius,
                settings.height,
                12, // segments
                RasterColor::new(80, 255, 80), // Bright green wireframe
            );
        }
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
    let hint = if game.playing {
        "WASD: Move | Shift: Run | RMB: Look | Space: Stop"
    } else {
        "RMB: Rotate | Scroll: Zoom | Space: Play"
    };
    let hint_dims = measure_text(hint, None, 12, 1.0);
    draw_text(
        hint,
        rect.x + (rect.w - hint_dims.width) / 2.0,
        rect.y + 20.0,
        12.0,
        Color::from_rgba(100, 100, 110, 180),
    );
}

/// Handle camera input for the preview mode (orbit camera)
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

/// Handle player input during gameplay
fn handle_player_input(game: &mut GameToolState, level: &Level, rect: &Rect) {
    let mouse_pos = mouse_position();
    let inside = mouse_pos.0 >= rect.x
        && mouse_pos.0 < rect.x + rect.w
        && mouse_pos.1 >= rect.y
        && mouse_pos.1 < rect.y + rect.h;

    // Toggle play with space
    if inside && is_key_pressed(KeyCode::Space) {
        game.toggle_playing();
        return;
    }

    // Mouse look to rotate player facing
    if inside && is_mouse_button_down(MouseButton::Right) {
        let dx = mouse_pos.0 - game.viewport_last_mouse.0;

        // Update player facing direction
        if let Some(player) = game.player_entity {
            if let Some(controller) = game.world.controllers.get_mut(player) {
                controller.facing -= dx * 0.005;
            }
        }
        game.viewport_mouse_captured = true;
    } else {
        game.viewport_mouse_captured = false;
    }

    // Get player settings from level
    let settings = &level.player_settings;

    // WASD movement
    if let Some(player) = game.player_entity {
        if let Some(controller) = game.world.controllers.get(player) {
            let facing = controller.facing;
            let mut move_dir = Vec3::ZERO;

            if is_key_down(KeyCode::W) {
                move_dir.x += facing.sin();
                move_dir.z += facing.cos();
            }
            if is_key_down(KeyCode::S) {
                move_dir.x -= facing.sin();
                move_dir.z -= facing.cos();
            }
            if is_key_down(KeyCode::A) {
                move_dir.x += facing.cos();
                move_dir.z -= facing.sin();
            }
            if is_key_down(KeyCode::D) {
                move_dir.x -= facing.cos();
                move_dir.z += facing.sin();
            }

            // Apply movement to velocity
            let move_len = move_dir.len();
            if move_len > 0.1 {
                move_dir = move_dir.normalize();
                let speed = if is_key_down(KeyCode::LeftShift) {
                    settings.run_speed
                } else {
                    settings.walk_speed
                };

                if let Some(velocity) = game.world.velocities.get_mut(player) {
                    velocity.0.x = move_dir.x * speed;
                    velocity.0.z = move_dir.z * speed;
                }
            } else {
                // No input: stop horizontal movement
                if let Some(velocity) = game.world.velocities.get_mut(player) {
                    velocity.0.x = 0.0;
                    velocity.0.z = 0.0;
                }
            }
        }
    }

    game.viewport_last_mouse = mouse_pos;
}

/// Draw a wireframe cylinder in the 3D view
fn draw_wireframe_cylinder(
    fb: &mut Framebuffer,
    camera: &crate::rasterizer::Camera,
    center: Vec3,
    radius: f32,
    height: f32,
    segments: usize,
    color: RasterColor,
) {
    use std::f32::consts::PI;

    // Generate circle points at bottom and top
    let mut bottom_points: Vec<Vec3> = Vec::with_capacity(segments);
    let mut top_points: Vec<Vec3> = Vec::with_capacity(segments);

    for i in 0..segments {
        let angle = (i as f32 / segments as f32) * 2.0 * PI;
        let x = center.x + radius * angle.cos();
        let z = center.z + radius * angle.sin();

        bottom_points.push(Vec3::new(x, center.y, z));
        top_points.push(Vec3::new(x, center.y + height, z));
    }

    // Project all points to screen space
    let project_point = |p: Vec3| -> Option<(i32, i32, f32)> {
        let rel = p - camera.position;
        let cam = perspective_transform(rel, camera.basis_x, camera.basis_y, camera.basis_z);

        // Behind camera check
        if cam.z < 0.1 {
            return None;
        }

        let proj = project(cam, false, fb.width, fb.height);
        Some((proj.x as i32, proj.y as i32, cam.z))
    };

    let bottom_screen: Vec<_> = bottom_points.iter().filter_map(|p| project_point(*p)).collect();
    let top_screen: Vec<_> = top_points.iter().filter_map(|p| project_point(*p)).collect();

    // Draw bottom circle
    for i in 0..bottom_screen.len() {
        let next = (i + 1) % bottom_screen.len();
        let (x0, y0, z0) = bottom_screen[i];
        let (x1, y1, z1) = bottom_screen[next];
        fb.draw_line_3d(x0, y0, z0, x1, y1, z1, color);
    }

    // Draw top circle
    for i in 0..top_screen.len() {
        let next = (i + 1) % top_screen.len();
        let (x0, y0, z0) = top_screen[i];
        let (x1, y1, z1) = top_screen[next];
        fb.draw_line_3d(x0, y0, z0, x1, y1, z1, color);
    }

    // Draw vertical lines connecting top and bottom (every other segment for cleaner look)
    let skip = if segments > 8 { 2 } else { 1 };
    for i in (0..segments).step_by(skip) {
        if i < bottom_screen.len() && i < top_screen.len() {
            let (x0, y0, z0) = bottom_screen[i];
            let (x1, y1, z1) = top_screen[i];
            fb.draw_line_3d(x0, y0, z0, x1, y1, z1, color);
        }
    }
}

