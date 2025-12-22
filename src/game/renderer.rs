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
use crate::input::{InputState, Action};
use super::runtime::{GameToolState, CameraMode};

/// Draw the test viewport (full area, no properties panel)
/// Player settings are now edited in the World Editor properties panel when PlayerStart is selected.
pub fn draw_test_viewport(
    rect: Rect,
    game: &mut GameToolState,
    level: &Level,
    textures: &[RasterTexture],
    fb: &mut Framebuffer,
    input: &InputState,
) {
    // Resize framebuffer to match game resolution
    fb.resize(WIDTH, HEIGHT);

    // Initialize camera from level's player start (only once)
    game.init_from_level(level);

    // Check for options menu toggle (Start button / Escape)
    if input.action_pressed(Action::OpenMenu) {
        game.options_menu_open = !game.options_menu_open;
    }

    // Handle input (camera, player movement) when menu is closed
    if !game.options_menu_open {
        if game.playing {
            match game.camera_mode {
                CameraMode::Character => {
                    // Third-person camera follows player
                    game.update_camera_follow_player(level);
                    // Handle Elden Ring style player input
                    handle_player_input(game, level, &rect, input);
                }
                CameraMode::FreeFly => {
                    // Free-fly noclip camera
                    handle_freefly_input(game, &rect, input);
                }
            }
        } else {
            // Orbit camera for preview mode
            handle_camera_input(game, &rect, input);
        }
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

    // Collect all lights from room objects
    let lights: Vec<Light> = if game.raster_settings.shading != ShadingMode::None {
        level.rooms.iter()
            .flat_map(|room| {
                room.objects.iter()
                    .filter(|obj| obj.enabled)
                    .filter_map(|obj| {
                        if let crate::world::ObjectType::Light { color, intensity, radius } = &obj.object_type {
                            let world_pos = obj.world_position(room);
                            let mut light = Light::point(world_pos, *radius, *intensity);
                            light.color = *color;
                            Some(light)
                        } else {
                            None
                        }
                    })
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

    // Draw options menu if open
    if game.options_menu_open {
        draw_options_menu(game, &rect, input);
    }

    // Draw play/pause indicator
    let status = if game.options_menu_open {
        "OPTIONS MENU"
    } else if game.playing {
        match game.camera_mode {
            CameraMode::Character => "PLAYING - Character Mode",
            CameraMode::FreeFly => "PLAYING - Free-Fly Mode",
        }
    } else {
        "PAUSED (Space to play)"
    };
    let status_dims = measure_text(status, None, 14, 1.0);
    draw_text(
        status,
        rect.x + (rect.w - status_dims.width) / 2.0,
        rect.y + rect.h - 20.0,
        14.0,
        Color::from_rgba(150, 150, 160, 200),
    );

    // Draw controls hint
    let has_gamepad = input.has_gamepad();
    let hint = if game.options_menu_open {
        if has_gamepad {
            "D-Pad: Select | A: Confirm | B/Start: Close"
        } else {
            "Up/Down: Select | Enter: Confirm | Escape: Close"
        }
    } else if game.playing {
        match game.camera_mode {
            CameraMode::Character => {
                if has_gamepad {
                    "LS: Move | RS: Look | B: Dodge | Start: Options"
                } else {
                    "WASD: Move | Shift: Run | RMB: Look | Esc: Options"
                }
            }
            CameraMode::FreeFly => {
                if has_gamepad {
                    "LS: Move | RS: Look | LB/LT: Up/Down | Start: Options"
                } else {
                    "WASD: Move | QE: Up/Down | RMB: Look | Esc: Options"
                }
            }
        }
    } else {
        if has_gamepad {
            "RS: Rotate | Start: Play"
        } else {
            "RMB: Rotate | Scroll: Zoom | Space: Play"
        }
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
fn handle_camera_input(game: &mut GameToolState, rect: &Rect, input: &InputState) {
    let mouse_pos = mouse_position();
    let inside = mouse_pos.0 >= rect.x
        && mouse_pos.0 < rect.x + rect.w
        && mouse_pos.1 >= rect.y
        && mouse_pos.1 < rect.y + rect.h;

    // Toggle play with space or gamepad Start
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

    // Gamepad right stick for orbit rotation
    let right_stick = input.right_stick();
    if right_stick.length() > 0.0 {
        let delta = get_frame_time();
        game.orbit_azimuth -= right_stick.x * 2.0 * delta;
        game.orbit_elevation = (game.orbit_elevation + right_stick.y * 1.5 * delta)
            .clamp(-1.4, 1.4);
        game.sync_camera_from_orbit();
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

/// Handle player input during gameplay (Dark Souls style character controls)
/// Camera orbits around player with right stick, movement is relative to camera direction.
fn handle_player_input(game: &mut GameToolState, level: &Level, rect: &Rect, input: &InputState) {
    let mouse_pos = mouse_position();
    let inside = mouse_pos.0 >= rect.x
        && mouse_pos.0 < rect.x + rect.w
        && mouse_pos.1 >= rect.y
        && mouse_pos.1 < rect.y + rect.h;

    let delta = get_frame_time();
    let settings = &level.player_settings;
    let look_sensitivity = 2.5;

    // Mouse look to rotate camera around player (RMB drag)
    if inside && is_mouse_button_down(MouseButton::Right) {
        let dx = mouse_pos.0 - game.viewport_last_mouse.0;
        let dy = mouse_pos.1 - game.viewport_last_mouse.1;

        game.char_cam_yaw -= dx * 0.005;
        game.char_cam_pitch = (game.char_cam_pitch + dy * 0.005)
            .clamp(settings.camera_pitch_min, settings.camera_pitch_max);

        game.viewport_mouse_captured = true;
    } else {
        game.viewport_mouse_captured = false;
    }

    // Gamepad right stick: orbit camera around player (Y inverted for natural feel)
    let right_stick = input.right_stick();
    if right_stick.length() > 0.0 {
        game.char_cam_yaw -= right_stick.x * look_sensitivity * delta;
        game.char_cam_pitch = (game.char_cam_pitch - right_stick.y * look_sensitivity * delta)
            .clamp(settings.camera_pitch_min, settings.camera_pitch_max);
    }

    // Get camera-relative directions for movement
    let cam_forward = game.get_camera_forward_xz();
    let cam_right = game.get_camera_right_xz();

    // Movement input: combine keyboard WASD with gamepad left stick
    let left_stick = input.left_stick();
    if let Some(player) = game.player_entity {
        let mut move_dir = Vec3::ZERO;

        // Movement relative to camera direction (Dark Souls style)
        if left_stick.length() > 0.1 {
            // Forward/back relative to where camera is facing
            move_dir = move_dir + cam_forward * left_stick.y;
            // Strafe left/right relative to camera (X inverted for natural feel)
            move_dir = move_dir + cam_right * -left_stick.x;
        }

        // Apply movement to velocity
        let move_len = move_dir.len();
        if move_len > 0.1 {
            move_dir = move_dir.normalize();

            // Update player facing to match movement direction (Dark Souls: character turns to face movement)
            if let Some(controller) = game.world.controllers.get_mut(player) {
                let target_facing = move_dir.x.atan2(move_dir.z);
                // Smooth rotation toward movement direction
                let facing_diff = (target_facing - controller.facing).rem_euclid(std::f32::consts::TAU);
                let facing_diff = if facing_diff > std::f32::consts::PI {
                    facing_diff - std::f32::consts::TAU
                } else {
                    facing_diff
                };
                controller.facing += facing_diff * 10.0 * delta; // Smooth turn speed
            }

            // Sprint when Dodge held + moving (Elden Ring: hold B to run)
            let sprinting = input.action_down(Action::Dodge) && move_len > 0.1;
            let speed = if sprinting {
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

    game.viewport_last_mouse = mouse_pos;
}

/// Handle free-fly camera input (noclip spectator mode)
fn handle_freefly_input(game: &mut GameToolState, rect: &Rect, input: &InputState) {
    let mouse_pos = mouse_position();
    let inside = mouse_pos.0 >= rect.x
        && mouse_pos.0 < rect.x + rect.w
        && mouse_pos.1 >= rect.y
        && mouse_pos.1 < rect.y + rect.h;

    let delta = get_frame_time();
    let fly_speed = 1500.0; // Units per second
    let look_sensitivity = 2.5;

    // Mouse look (RMB drag)
    if inside && is_mouse_button_down(MouseButton::Right) {
        let dx = mouse_pos.0 - game.viewport_last_mouse.0;
        let dy = mouse_pos.1 - game.viewport_last_mouse.1;

        game.freefly_yaw -= dx * 0.005;
        game.freefly_pitch = (game.freefly_pitch + dy * 0.005).clamp(-1.5, 1.5);
        game.viewport_mouse_captured = true;
    } else {
        game.viewport_mouse_captured = false;
    }

    // Gamepad right stick: look around (Y inverted for natural feel)
    let right_stick = input.right_stick();
    if right_stick.length() > 0.0 {
        game.freefly_yaw -= right_stick.x * look_sensitivity * delta;
        game.freefly_pitch = (game.freefly_pitch - right_stick.y * look_sensitivity * delta)
            .clamp(-1.5, 1.5);
    }

    // Calculate forward/right vectors from yaw/pitch
    let forward = Vec3::new(
        game.freefly_pitch.cos() * game.freefly_yaw.sin(),
        -game.freefly_pitch.sin(),
        game.freefly_pitch.cos() * game.freefly_yaw.cos(),
    ).normalize();
    let right = Vec3::new(
        game.freefly_yaw.cos(),
        0.0,
        -game.freefly_yaw.sin(),
    );

    // Movement input
    let left_stick = input.left_stick();
    let mut move_delta = Vec3::ZERO;

    // Gamepad left stick: move forward/back, strafe left/right (X inverted for natural feel)
    if left_stick.length() > 0.1 {
        move_delta = move_delta + forward * left_stick.y * fly_speed * delta;
        move_delta = move_delta + right * -left_stick.x * fly_speed * delta;
    }

    // Vertical movement: LB up, LT down (or Q/E on keyboard)
    if input.action_down(Action::FlyUp) {
        move_delta.y += fly_speed * delta;
    }
    if input.action_down(Action::FlyDown) {
        move_delta.y -= fly_speed * delta;
    }

    // Apply movement
    game.camera.position = game.camera.position + move_delta;

    // Update camera orientation
    game.camera.rotation_y = game.freefly_yaw;
    game.camera.rotation_x = game.freefly_pitch;
    game.camera.update_basis();

    game.viewport_last_mouse = mouse_pos;
}

/// Draw the options menu overlay
fn draw_options_menu(game: &mut GameToolState, rect: &Rect, input: &InputState) {
    // Dark semi-transparent overlay
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(0, 0, 0, 180));

    // Menu box dimensions
    let menu_w = 300.0;
    let menu_h = 120.0;
    let menu_x = rect.x + (rect.w - menu_w) / 2.0;
    let menu_y = rect.y + (rect.h - menu_h) / 2.0;

    // Menu background
    draw_rectangle(menu_x, menu_y, menu_w, menu_h, Color::from_rgba(30, 32, 38, 240));
    draw_rectangle_lines(menu_x, menu_y, menu_w, menu_h, 2.0, Color::from_rgba(80, 85, 95, 255));

    // Title
    let title = "OPTIONS";
    let title_dims = measure_text(title, None, 18, 1.0);
    draw_text(
        title,
        menu_x + (menu_w - title_dims.width) / 2.0,
        menu_y + 30.0,
        18.0,
        Color::from_rgba(200, 200, 210, 255),
    );

    // Camera mode option
    let mode_label = "Camera Mode:";
    draw_text(
        mode_label,
        menu_x + 20.0,
        menu_y + 65.0,
        14.0,
        Color::from_rgba(150, 150, 160, 255),
    );

    // Mode buttons
    let char_selected = game.camera_mode == CameraMode::Character;
    let fly_selected = game.camera_mode == CameraMode::FreeFly;

    let char_color = if char_selected {
        Color::from_rgba(100, 180, 255, 255)
    } else {
        Color::from_rgba(100, 100, 110, 255)
    };
    let fly_color = if fly_selected {
        Color::from_rgba(100, 180, 255, 255)
    } else {
        Color::from_rgba(100, 100, 110, 255)
    };

    draw_text("[Character]", menu_x + 120.0, menu_y + 65.0, 14.0, char_color);
    draw_text("[Free-Fly]", menu_x + 210.0, menu_y + 65.0, 14.0, fly_color);

    // Instructions
    let close_hint = "Press Escape or Start to close";
    let close_dims = measure_text(close_hint, None, 11, 1.0);
    draw_text(
        close_hint,
        menu_x + (menu_w - close_dims.width) / 2.0,
        menu_y + menu_h - 15.0,
        11.0,
        Color::from_rgba(80, 80, 90, 255),
    );

    // Handle input to toggle camera mode
    // D-Pad left/right or keyboard left/right arrows
    if input.action_pressed(Action::SwitchLeftWeapon) || is_key_pressed(KeyCode::Left) {
        game.camera_mode = CameraMode::Character;
    }
    if input.action_pressed(Action::SwitchRightWeapon) || is_key_pressed(KeyCode::Right) {
        game.camera_mode = CameraMode::FreeFly;
    }

    // Also allow A/Enter to toggle between modes
    if input.action_pressed(Action::Jump) || is_key_pressed(KeyCode::Enter) {
        game.camera_mode = match game.camera_mode {
            CameraMode::Character => CameraMode::FreeFly,
            CameraMode::FreeFly => CameraMode::Character,
        };
    }

    // B button closes menu (in addition to Start which is handled in main function)
    if input.action_pressed(Action::Dodge) {
        game.options_menu_open = false;
    }
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

