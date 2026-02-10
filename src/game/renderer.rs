//! Test Renderer
//!
//! Renders the test view from project data and ECS world.
//! Combines static level geometry with dynamic entities.

use macroquad::prelude::*;
use crate::rasterizer::{
    Framebuffer, Texture as RasterTexture,
    Light, RasterTimings, ShadingMode, Color as RasterColor,
    Vec3, project, perspective_transform,
    WIDTH, HEIGHT, WIDTH_HI, HEIGHT_HI,
};
use crate::ui::Rect;
use crate::world::Level;
use crate::input::{InputState, Action};
use super::runtime::{GameToolState, CameraMode, FrameTimings};

/// Draw the test viewport (full area, no properties panel)
/// Player settings are now edited in the World Editor properties panel when PlayerStart is selected.
pub fn draw_test_viewport(
    rect: Rect,
    game: &mut GameToolState,
    level: &Level,
    textures: &[RasterTexture],
    fb: &mut Framebuffer,
    input: &InputState,
    ctx: &crate::ui::UiContext,
    asset_library: &crate::asset::AssetLibrary,
    user_textures: &crate::texture::TextureLibrary,
) {
    let frame_start = FrameTimings::start();

    // Resize framebuffer based on resolution and aspect ratio settings
    let (fb_w, fb_h) = if game.raster_settings.stretch_to_fill {
        // Stretch mode: keep vertical resolution fixed, scale horizontal to match viewport aspect ratio
        // This maintains consistent pixel size while utilizing full screen width
        let base_h = if game.raster_settings.low_resolution { HEIGHT } else { HEIGHT_HI };
        let viewport_aspect = rect.w / rect.h;
        let scaled_w = (base_h as f32 * viewport_aspect) as usize;
        (scaled_w.max(1), base_h)
    } else {
        // 4:3 mode: fixed PS1 resolution
        if game.raster_settings.low_resolution {
            (WIDTH, HEIGHT)       // 320x240 PS1 native
        } else {
            (WIDTH_HI, HEIGHT_HI) // 640x480 high res
        }
    };
    fb.resize(fb_w, fb_h);

    // Initialize camera from level's player start (only once)
    game.init_from_level(level, asset_library);

    // Check for options menu toggle (Start button / Escape)
    if input.action_pressed(Action::OpenMenu) {
        game.options_menu_open = !game.options_menu_open;
    }

    // Auto-start playing when entering game tab
    if !game.playing {
        game.toggle_playing();
    }

    // === INPUT PHASE ===
    let input_start = FrameTimings::start();

    // Handle input (camera, player movement) - blocked when debug menu is open
    if !game.options_menu_open {
        match game.camera_mode {
            CameraMode::Character => {
                // Third-person camera follows player
                game.update_camera_follow_player(level);
                // Handle Dark Souls style player input
                handle_player_input(game, level, &rect, input, ctx);
            }
            CameraMode::FreeFly => {
                // Free-fly noclip camera
                handle_freefly_input(game, &rect, input, ctx);
            }
        }
    }

    let input_ms = FrameTimings::elapsed_ms(input_start);

    // === CLEAR PHASE ===
    let clear_start = FrameTimings::start();

    // Clear framebuffer - if skybox, render 3D sphere; otherwise solid color
    if let Some(skybox) = &level.skybox {
        // Clear to black first, then render 3D skybox sphere
        fb.clear(RasterColor::new(0, 0, 0));
        let time = macroquad::prelude::get_time() as f32;
        fb.render_skybox(skybox, &game.camera, time);
    } else {
        fb.clear(RasterColor::new(20, 22, 28));
    }

    let clear_ms = FrameTimings::elapsed_ms(clear_start);

    // === RENDER PHASE ===
    let render_start = FrameTimings::start();

    // Texture resolver closure - returns (texture_id, texture_width)
    let resolve_texture = |tex_ref: &crate::world::TextureRef| -> Option<(usize, u32)> {
        if !tex_ref.is_valid() {
            return Some((0, 64)); // Fallback to first texture with default 64x64 size
        }
        // Try finding by name in the textures array directly
        textures.iter().enumerate()
            .find(|(_, t)| t.name == tex_ref.name)
            .map(|(idx, t)| (idx, t.width as u32))
    };

    // --- Sub-timing: Light collection ---
    let lights_start = FrameTimings::start();
    let lights: Vec<Light> = if game.raster_settings.shading != ShadingMode::None {
        crate::scene::collect_scene_lights(&level.rooms, asset_library)
    } else {
        Vec::new()
    };
    let render_lights_ms = FrameTimings::elapsed_ms(lights_start);

    // --- Sub-timing: Mesh generation and rasterization ---
    let render_meshgen_ms = 0.0;
    let mut render_raster_ms = 0.0;
    let raster_timings = RasterTimings::default();

    // --- Sub-timing: Texture conversion (RGB888 to RGB555) ---
    let texconv_start = FrameTimings::start();
    let use_rgb555 = game.raster_settings.use_rgb555;
    if use_rgb555 && game.textures_15_cache.len() != textures.len() {
        game.textures_15_cache = textures.iter().map(|t| t.to_15()).collect();
    }
    let render_texconv_ms = FrameTimings::elapsed_ms(texconv_start);

    // Render rooms + asset meshes
    crate::scene::render_scene(
        fb,
        &level.rooms,
        asset_library,
        user_textures,
        &game.camera,
        &game.raster_settings,
        &lights,
        textures,
        &game.textures_15_cache,
        &resolve_texture,
        &crate::scene::SceneRenderOptions {
            use_fog: true,
            render_assets: true,
            skip_rooms: &[],
        },
    );

    // Render blob shadows under entities
    if game.playing {
        super::shadows::render_blob_shadows(fb, &game.camera, &game.world, level);
    }

    // Render particles
    if game.playing {
        game.particles.render(fb, &game.camera);
    }

    // Render player wireframe cylinder if playing
    if game.playing {
        if let Some(player_pos) = game.get_player_position() {
            let settings = &level.player_settings;
            let raster_start = FrameTimings::start();
            draw_wireframe_cylinder(
                fb,
                &game.camera,
                player_pos,
                settings.radius,
                settings.height,
                12, // segments
                RasterColor::new(80, 255, 80), // Bright green wireframe
            );
            render_raster_ms += FrameTimings::elapsed_ms(raster_start);
        }
    }

    let render_ms = FrameTimings::elapsed_ms(render_start);

    // --- Sub-timing: Texture upload ---
    let upload_start = FrameTimings::start();

    // Convert framebuffer to texture and draw to viewport
    let texture = Texture2D::from_rgba8(fb.width as u16, fb.height as u16, &fb.pixels);
    texture.set_filter(FilterMode::Nearest);

    // Calculate draw area - framebuffer matches viewport in stretch mode, needs letterboxing in 4:3
    let (draw_w, draw_h, draw_x, draw_y) = if game.raster_settings.stretch_to_fill {
        // Framebuffer already sized to viewport aspect, draw at full size
        (rect.w, rect.h, rect.x, rect.y)
    } else {
        // Maintain aspect ratio (4:3 for PS1) with letterboxing
        let fb_aspect = fb.width as f32 / fb.height as f32;
        let rect_aspect = rect.w / rect.h;
        if fb_aspect > rect_aspect {
            let w = rect.w;
            let h = rect.w / fb_aspect;
            (w, h, rect.x, rect.y + (rect.h - h) * 0.5)
        } else {
            let h = rect.h;
            let w = rect.h * fb_aspect;
            (w, h, rect.x + (rect.w - w) * 0.5, rect.y)
        }
    };

    // Draw letterbox bars (background for non-rendered area)
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

    let render_upload_ms = FrameTimings::elapsed_ms(upload_start);

    // === UI PHASE ===
    let ui_start = FrameTimings::start();

    // Draw debug overlay HUD if enabled (top-right, always visible during gameplay)
    if game.show_debug_overlay {
        draw_debug_overlay(game, &rect, input, level);
    }

    // Draw debug menu overlay if open (top-left, blocks gameplay for D-pad navigation)
    if game.options_menu_open {
        draw_debug_menu(game, &rect, input, level, asset_library);
    } else {
        // Show collapsed hint when menu is closed
        let hint = "[ESC] Menu";
        let hint_w = 70.0;
        let hint_h = 16.0;
        let hint_x = rect.x + 4.0;
        let hint_y = rect.y + 4.0;
        draw_rectangle(hint_x, hint_y, hint_w, hint_h, Color::from_rgba(0, 0, 0, 120));
        draw_text(hint, hint_x + 4.0, hint_y + 12.0, 11.0, Color::from_rgba(180, 180, 180, 200));
    }

    // Show warning if no player start exists in level
    if level.get_player_start(asset_library).is_none() {
        let msg = "No Player Start in level";
        let hint = "Add a PlayerStart spawn point in World Editor";
        let font_size = 16.0;
        let hint_size = 12.0;

        // Center the message
        let msg_width = msg.len() as f32 * font_size * 0.5;
        let hint_width = hint.len() as f32 * hint_size * 0.5;
        let center_x = rect.x + rect.w / 2.0;
        let center_y = rect.y + rect.h / 2.0;

        // Draw semi-transparent background
        let bg_w = msg_width.max(hint_width) + 40.0;
        let bg_h = 60.0;
        draw_rectangle(
            center_x - bg_w / 2.0,
            center_y - bg_h / 2.0,
            bg_w,
            bg_h,
            Color::from_rgba(0, 0, 0, 180),
        );

        // Draw warning text
        draw_text(
            msg,
            center_x - msg_width / 2.0,
            center_y - 5.0,
            font_size,
            Color::from_rgba(255, 200, 50, 255),
        );
        draw_text(
            hint,
            center_x - hint_width / 2.0,
            center_y + 18.0,
            hint_size,
            Color::from_rgba(180, 180, 180, 255),
        );
    }

    let ui_ms = FrameTimings::elapsed_ms(ui_start);

    // Store frame timings for display
    game.frame_timings = FrameTimings {
        input_ms,
        logic_ms: 0.0, // Logic is run separately in tick()
        clear_ms,
        render_ms,
        ui_ms,
        total_ms: FrameTimings::elapsed_ms(frame_start),
        // Render sub-timings
        render_lights_ms,
        render_texconv_ms,
        render_meshgen_ms,
        render_raster_ms,
        render_upload_ms,
        // Raster sub-timings (breakdown of render_raster_ms)
        raster_transform_ms: raster_timings.transform_ms,
        raster_fog_ms: raster_timings.fog_ms,
        raster_cull_ms: raster_timings.cull_ms,
        raster_sort_ms: raster_timings.sort_ms,
        raster_draw_ms: raster_timings.draw_ms,
        raster_wireframe_ms: raster_timings.wireframe_ms,
        triangles_drawn: raster_timings.triangles_drawn,
    };
}

/// Handle player input during gameplay (Dark Souls style character controls)
/// Camera orbits around player with right stick, movement is relative to camera direction.
fn handle_player_input(game: &mut GameToolState, level: &Level, rect: &Rect, input: &InputState, ctx: &crate::ui::UiContext) {
    let mouse_pos = (ctx.mouse.x, ctx.mouse.y);
    let inside = mouse_pos.0 >= rect.x
        && mouse_pos.0 < rect.x + rect.w
        && mouse_pos.1 >= rect.y
        && mouse_pos.1 < rect.y + rect.h;

    let delta = get_frame_time();
    let settings = &level.player_settings;
    let look_sensitivity = 2.5;

    // Mouse look to rotate camera around player (RMB drag)
    if inside && ctx.mouse.right_down {
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

        // Check sprint state (Elden Ring: hold B to run)
        let move_len = move_dir.len();
        let sprinting = input.action_down(Action::Dodge) && move_len > 0.1;

        // Apply movement to velocity
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

        // Jump (Elden Ring: A button / Space key)
        // Can only jump when grounded
        if input.action_pressed(Action::Jump) {
            if let Some(controller) = game.world.controllers.get_mut(player) {
                if controller.grounded {
                    // Calculate jump velocity (sprint-jump is higher)
                    let jump_vel = if sprinting {
                        settings.jump_velocity * settings.sprint_jump_multiplier
                    } else {
                        settings.jump_velocity
                    };
                    controller.vertical_velocity = jump_vel;
                    controller.grounded = false; // Immediately leave ground
                }
            }
        }
    }

    game.viewport_last_mouse = mouse_pos;
}

/// Handle free-fly camera input (noclip spectator mode)
fn handle_freefly_input(game: &mut GameToolState, rect: &Rect, input: &InputState, ctx: &crate::ui::UiContext) {
    let mouse_pos = (ctx.mouse.x, ctx.mouse.y);
    let inside = mouse_pos.0 >= rect.x
        && mouse_pos.0 < rect.x + rect.w
        && mouse_pos.1 >= rect.y
        && mouse_pos.1 < rect.y + rect.h;

    let delta = get_frame_time();
    let fly_speed = 1500.0; // Units per second
    let look_sensitivity = 2.5;

    // Mouse look (RMB drag)
    if inside && ctx.mouse.right_down {
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

/// Draw compact debug menu overlay (top-left, blocks gameplay for D-pad navigation)
fn draw_debug_menu(game: &mut GameToolState, rect: &Rect, input: &InputState, level: &Level, asset_library: &crate::asset::AssetLibrary) {
    let menu_x = rect.x + 10.0;
    let menu_y = rect.y + 10.0;
    let menu_w = 180.0;
    let row_height = 20.0;

    // Menu items: Camera, Overlay, PS1 features, Reset
    let items = [
        "Camera",        // 0
        "Overlay",       // 1
        "---",           // 2 - Separator
        "Affine UV",     // 3 - PS1 texture warping
        "Fixed-Point",   // 4 - PS1 fixed-point math (jitter)
        "Low Res",       // 5 - 320x240
        "4:3 Aspect",    // 6 - 4:3 aspect ratio (vs stretch to fill)
        "RGB555",        // 7 - PS1 15-bit color
        "Dithering",     // 8 - PS1 dithering
        "Shading",       // 9 - None/Flat/Gouraud
        "FPS",           // 10 - 30/60/Unlocked
        "---",           // 11 - Separator
        "Reset",         // 12
    ];
    let menu_h = 20.0 + items.len() as f32 * row_height + 14.0;
    let selected = game.debug_menu_selection;

    // Semi-transparent background
    draw_rectangle(menu_x, menu_y, menu_w, menu_h, Color::from_rgba(20, 22, 28, 220));
    draw_rectangle_lines(menu_x, menu_y, menu_w, menu_h, 1.0, Color::from_rgba(60, 65, 75, 255));

    // Handle D-Pad up/down navigation (skip separators)
    if input.action_pressed(Action::SwitchSpell) || is_key_pressed(KeyCode::Up) {
        let mut new_sel = game.debug_menu_selection.saturating_sub(1);
        // Skip separators
        while new_sel > 0 && items[new_sel] == "---" {
            new_sel = new_sel.saturating_sub(1);
        }
        game.debug_menu_selection = new_sel;
    }
    if input.action_pressed(Action::SwitchItem) || is_key_pressed(KeyCode::Down) {
        let mut new_sel = (game.debug_menu_selection + 1).min(items.len() - 1);
        // Skip separators
        while new_sel < items.len() - 1 && items[new_sel] == "---" {
            new_sel = (new_sel + 1).min(items.len() - 1);
        }
        game.debug_menu_selection = new_sel;
    }

    // Draw each menu item
    for (i, item) in items.iter().enumerate() {
        let y = menu_y + 18.0 + i as f32 * row_height;
        let is_selected = i == selected;

        // Separator line
        if *item == "---" {
            draw_line(
                menu_x + 8.0,
                y - 6.0,
                menu_x + menu_w - 8.0,
                y - 6.0,
                1.0,
                Color::from_rgba(60, 65, 75, 255),
            );
            continue;
        }

        // Selection indicator
        let label_color = if is_selected {
            Color::from_rgba(255, 255, 255, 255)
        } else {
            Color::from_rgba(120, 120, 130, 255)
        };

        if is_selected {
            draw_text(">", menu_x + 4.0, y, 12.0, Color::from_rgba(100, 180, 255, 255));
        }

        draw_text(item, menu_x + 16.0, y, 12.0, label_color);

        // Item-specific value/action
        match i {
            0 => {
                // Camera mode
                let mode_name = match game.camera_mode {
                    CameraMode::Character => "Character",
                    CameraMode::FreeFly => "Free-Fly",
                };
                draw_text(mode_name, menu_x + 100.0, y, 12.0, Color::from_rgba(100, 180, 255, 255));

                if is_selected {
                    if input.action_pressed(Action::SwitchLeftWeapon) || is_key_pressed(KeyCode::Left) {
                        game.camera_mode = CameraMode::Character;
                    }
                    if input.action_pressed(Action::SwitchRightWeapon) || is_key_pressed(KeyCode::Right) {
                        game.camera_mode = CameraMode::FreeFly;
                    }
                    if input.action_pressed(Action::Jump) || is_key_pressed(KeyCode::Enter) {
                        game.camera_mode = match game.camera_mode {
                            CameraMode::Character => CameraMode::FreeFly,
                            CameraMode::FreeFly => CameraMode::Character,
                        };
                    }
                }
            }
            1 => {
                // Debug overlay toggle
                draw_toggle(menu_x, y, game.show_debug_overlay);
                if is_selected && toggle_pressed(input) {
                    game.show_debug_overlay = !game.show_debug_overlay;
                }
            }
            3 => {
                // Affine textures (PS1 UV warping)
                draw_toggle(menu_x, y, game.raster_settings.affine_textures);
                if is_selected && toggle_pressed(input) {
                    game.raster_settings.affine_textures = !game.raster_settings.affine_textures;
                }
            }
            4 => {
                // Fixed-point math (PS1 jitter)
                draw_toggle(menu_x, y, game.raster_settings.use_fixed_point);
                if is_selected && toggle_pressed(input) {
                    game.raster_settings.use_fixed_point = !game.raster_settings.use_fixed_point;
                }
            }
            5 => {
                // Low resolution (320x240)
                draw_toggle(menu_x, y, game.raster_settings.low_resolution);
                if is_selected && toggle_pressed(input) {
                    game.raster_settings.low_resolution = !game.raster_settings.low_resolution;
                }
            }
            6 => {
                // 4:3 aspect ratio (vs stretch to fill)
                // Note: toggle shows ON when NOT stretching (i.e., maintaining 4:3)
                draw_toggle(menu_x, y, !game.raster_settings.stretch_to_fill);
                if is_selected && toggle_pressed(input) {
                    game.raster_settings.stretch_to_fill = !game.raster_settings.stretch_to_fill;
                }
            }
            7 => {
                // RGB555 (PS1 15-bit color)
                draw_toggle(menu_x, y, game.raster_settings.use_rgb555);
                if is_selected && toggle_pressed(input) {
                    game.raster_settings.use_rgb555 = !game.raster_settings.use_rgb555;
                }
            }
            8 => {
                // Dithering (PS1 ordered dithering)
                draw_toggle(menu_x, y, game.raster_settings.dithering);
                if is_selected && toggle_pressed(input) {
                    game.raster_settings.dithering = !game.raster_settings.dithering;
                }
            }
            9 => {
                // Shading mode (cycle: None -> Flat -> Gouraud)
                let mode_name = match game.raster_settings.shading {
                    ShadingMode::None => "None",
                    ShadingMode::Flat => "Flat",
                    ShadingMode::Gouraud => "Gouraud",
                };
                draw_text(mode_name, menu_x + 100.0, y, 12.0, Color::from_rgba(100, 180, 255, 255));

                if is_selected {
                    if input.action_pressed(Action::SwitchLeftWeapon) || is_key_pressed(KeyCode::Left) {
                        game.raster_settings.shading = match game.raster_settings.shading {
                            ShadingMode::None => ShadingMode::Gouraud,
                            ShadingMode::Flat => ShadingMode::None,
                            ShadingMode::Gouraud => ShadingMode::Flat,
                        };
                    }
                    if input.action_pressed(Action::SwitchRightWeapon) || is_key_pressed(KeyCode::Right)
                        || input.action_pressed(Action::Jump) || is_key_pressed(KeyCode::Enter)
                    {
                        game.raster_settings.shading = match game.raster_settings.shading {
                            ShadingMode::None => ShadingMode::Flat,
                            ShadingMode::Flat => ShadingMode::Gouraud,
                            ShadingMode::Gouraud => ShadingMode::None,
                        };
                    }
                }
            }
            10 => {
                // FPS limit (cycle: 30 -> 60 -> Unlocked)
                draw_text(game.fps_limit.label(), menu_x + 100.0, y, 12.0, Color::from_rgba(100, 180, 255, 255));

                if is_selected {
                    if input.action_pressed(Action::SwitchLeftWeapon) || is_key_pressed(KeyCode::Left) {
                        game.fps_limit = game.fps_limit.prev();
                    }
                    if input.action_pressed(Action::SwitchRightWeapon) || is_key_pressed(KeyCode::Right)
                        || input.action_pressed(Action::Jump) || is_key_pressed(KeyCode::Enter)
                    {
                        game.fps_limit = game.fps_limit.next();
                    }
                }
            }
            12 => {
                // Reset game
                draw_text("[Press A]", menu_x + 100.0, y, 12.0, Color::from_rgba(80, 80, 90, 255));

                if is_selected {
                    if input.action_pressed(Action::Jump) || is_key_pressed(KeyCode::Enter) {
                        game.reset();
                        game.options_menu_open = false;
                        if let Some((room_idx, spawn)) = level.get_player_start(asset_library) {
                            if let Some(room) = level.rooms.get(room_idx) {
                                let spawn_pos = spawn.world_position(room);
                                game.spawn_player(spawn_pos, level);
                            }
                        }
                        game.playing = true;
                    }
                }
            }
            _ => {}
        }
    }

    // Hint at bottom
    draw_text("D-Pad: Navigate  A: Toggle", menu_x + 8.0, menu_y + menu_h - 8.0, 10.0, Color::from_rgba(80, 80, 90, 255));
}

/// Helper: draw ON/OFF toggle at position
fn draw_toggle(menu_x: f32, y: f32, enabled: bool) {
    let state = if enabled { "ON" } else { "OFF" };
    let color = if enabled {
        Color::from_rgba(100, 255, 100, 255)
    } else {
        Color::from_rgba(100, 180, 255, 255)
    };
    draw_text(state, menu_x + 100.0, y, 12.0, color);
}

/// Helper: check if toggle action was pressed
fn toggle_pressed(input: &InputState) -> bool {
    input.action_pressed(Action::Jump) || is_key_pressed(KeyCode::Enter)
        || input.action_pressed(Action::SwitchLeftWeapon) || is_key_pressed(KeyCode::Left)
        || input.action_pressed(Action::SwitchRightWeapon) || is_key_pressed(KeyCode::Right)
}

/// Draw debug overlay HUD (top-right, shows player/collision stats)
fn draw_debug_overlay(game: &GameToolState, rect: &Rect, input: &InputState, level: &Level) {
    // Scale factor for the entire overlay (1.5x for compact display)
    let scale = 1.5;

    let line_height = 12.0 * scale;
    let text_size = 10.0 * scale;
    let overlay_w = 160.0 * scale;
    let overlay_x = rect.x + rect.w - overlay_w - 10.0;
    let overlay_y = rect.y + 10.0;

    let label_color = Color::from_rgba(120, 120, 130, 255);
    let value_color = Color::from_rgba(200, 200, 210, 255);
    let good_color = Color::from_rgba(100, 255, 100, 255);
    let warn_color = Color::from_rgba(255, 180, 80, 255);

    // Performance bar colors
    let input_color = Color::from_rgba(100, 180, 255, 255);  // Blue - input
    let clear_color = Color::from_rgba(180, 100, 255, 255);  // Purple - clear
    let render_color = Color::from_rgba(255, 100, 100, 255); // Red - render (usually biggest)
    let ui_color = Color::from_rgba(255, 200, 100, 255);     // Orange - UI

    // Render sub-timing colors (shades of red)
    let lights_color = Color::from_rgba(255, 150, 150, 255);  // Light red - lights
    let texconv_color = Color::from_rgba(255, 100, 180, 255); // Pink - texture conversion
    let meshgen_color = Color::from_rgba(255, 80, 80, 255);   // Medium red - mesh gen
    let raster_color = Color::from_rgba(200, 50, 50, 255);    // Dark red - rasterization
    let upload_color = Color::from_rgba(255, 120, 80, 255);   // Red-orange - upload

    // Raster sub-timing colors (shades of maroon/brown)
    let transform_color = Color::from_rgba(180, 100, 100, 255); // Transform
    let fog_timing_color = Color::from_rgba(170, 90, 130, 255); // Fog (purple tint)
    let cull_color = Color::from_rgba(160, 80, 80, 255);        // Cull
    let sort_color = Color::from_rgba(140, 60, 60, 255);        // Sort
    let draw_color = Color::from_rgba(120, 40, 40, 255);        // Draw (main)
    let wireframe_color = Color::from_rgba(100, 70, 70, 255);   // Wireframe

    let mut lines: Vec<(String, Color)> = Vec::new();

    // FPS
    let fps = get_fps();
    let fps_color = if fps >= 55 { good_color } else if fps >= 30 { warn_color } else { Color::from_rgba(255, 100, 100, 255) };
    lines.push((format!("FPS: {}", fps), fps_color));

    // Player state
    if let Some(player) = game.player_entity {
        // Position
        if let Some(transform) = game.world.transforms.get(player) {
            let p = transform.position;
            lines.push((format!("Pos: {:.0}, {:.0}, {:.0}", p.x, p.y, p.z), value_color));
        }

        // Velocity
        if let Some(velocity) = game.world.velocities.get(player) {
            let v = velocity.0;
            let speed = (v.x * v.x + v.z * v.z).sqrt();
            lines.push((format!("Speed: {:.0}", speed), value_color));
            lines.push((format!("Vel Y: {:.1}", v.y), value_color));
        }

        // Controller state
        if let Some(ctrl) = game.world.controllers.get(player) {
            // Grounded
            let grounded_str = if ctrl.grounded { "YES" } else { "NO" };
            let grounded_color = if ctrl.grounded { good_color } else { warn_color };
            lines.push((format!("Grounded: {}", grounded_str), grounded_color));

            // Vertical velocity (gravity accumulation)
            lines.push((format!("Vert Vel: {:.1}", ctrl.vertical_velocity), value_color));

            // Room
            lines.push((format!("Room: {}", ctrl.current_room), value_color));

            // Facing
            let facing_deg = ctrl.facing.to_degrees();
            lines.push((format!("Facing: {:.0}°", facing_deg), value_color));
        }

        // Floor height at player position
        if let Some(transform) = game.world.transforms.get(player) {
            if let Some(floor) = level.get_floor_height(transform.position, None) {
                lines.push((format!("Floor: {:.0}", floor), value_color));
            }
        }
    } else {
        lines.push(("No Player".to_string(), warn_color));
    }

    // Separator
    lines.push(("---".to_string(), label_color));

    // Input state
    let left_stick = input.left_stick();
    lines.push((format!("L Stick: {:.2}, {:.2}", left_stick.x, left_stick.y), value_color));

    let right_stick = input.right_stick();
    lines.push((format!("R Stick: {:.2}, {:.2}", right_stick.x, right_stick.y), value_color));

    // Movement state - show B button status for debugging
    let b_down = input.action_down(Action::Dodge);
    if b_down {
        lines.push(("B: DOWN".to_string(), good_color));
    }

    let sprinting = b_down && left_stick.length() > 0.1;
    if sprinting {
        lines.push(("SPRINTING".to_string(), good_color));
    }

    // Check if player is jumping (not grounded and positive vertical velocity)
    if let Some(player) = game.player_entity {
        if let Some(ctrl) = game.world.controllers.get(player) {
            if !ctrl.grounded && ctrl.vertical_velocity > 0.0 {
                lines.push(("JUMPING".to_string(), Color::from_rgba(255, 200, 100, 255)));
            }
        }
    }

    // Calculate overlay height
    let padding = 8.0 * scale;
    let overlay_h = padding + lines.len() as f32 * line_height + 4.0 * scale;

    // Draw background
    draw_rectangle(overlay_x, overlay_y, overlay_w, overlay_h, Color::from_rgba(20, 22, 28, 200));
    draw_rectangle_lines(overlay_x, overlay_y, overlay_w, overlay_h, 1.0, Color::from_rgba(60, 65, 75, 255));

    // Draw lines
    for (i, (text, color)) in lines.iter().enumerate() {
        let y = overlay_y + 12.0 * scale + i as f32 * line_height;
        draw_text(text, overlay_x + 6.0 * scale, y, text_size, *color);
    }

    // === PERFORMANCE BAR ===
    // Draw a horizontal bar showing frame time breakdown
    let bar_y = overlay_y + overlay_h + 6.0 * scale;
    let bar_h = 12.0 * scale;
    let bar_w = overlay_w - 12.0 * scale;
    let bar_x = overlay_x + 6.0 * scale;

    // Background (taller to fit stacked legend with render breakdown + raster breakdown + triangle count)
    let legend_height = 250.0 * scale; // 4 main + 5 render + 6 raster + 2 headers + triangle count + padding
    draw_rectangle(overlay_x, bar_y - 4.0 * scale, overlay_w, bar_h + legend_height, Color::from_rgba(20, 22, 28, 200));
    draw_rectangle_lines(overlay_x, bar_y - 4.0 * scale, overlay_w, bar_h + legend_height, 1.0, Color::from_rgba(60, 65, 75, 255));

    // Calculate bar segments proportionally
    // Target: 16.67ms = 60fps, show bar relative to that
    let target_ms = 16.67;
    let t = &game.frame_timings;
    let total = t.total_ms.max(0.001); // Avoid division by zero

    // Draw bar segments (stacked horizontally)
    let mut x = bar_x;
    let segments = [
        (t.input_ms, input_color, "I"),
        (t.clear_ms, clear_color, "C"),
        (t.render_ms, render_color, "R"),
        (t.ui_ms, ui_color, "U"),
    ];

    for (ms, color, _label) in segments.iter() {
        let seg_w = (*ms / total) * bar_w;
        if seg_w > 0.5 {
            draw_rectangle(x, bar_y, seg_w, bar_h, *color);
            x += seg_w;
        }
    }

    // Draw target line (16.67ms = 60fps)
    let target_x = bar_x + (target_ms / total.max(target_ms)) * bar_w;
    if target_x < bar_x + bar_w {
        draw_line(target_x, bar_y - 2.0 * scale, target_x, bar_y + bar_h + 2.0 * scale, 1.0, Color::from_rgba(255, 255, 255, 150));
    }

    // Draw timing text below bar
    draw_text(
        &format!("{:.1}ms", t.total_ms),
        bar_x,
        bar_y + bar_h + 10.0 * scale,
        text_size,
        value_color,
    );

    // Legend (stacked vertically with full names and times)
    let legend_y = bar_y + bar_h + 20.0 * scale;
    let legend_line_height = 12.0 * scale;
    let legend_text_size = 9.0 * scale;
    let legend_box_size = 6.0 * scale;

    // Main phases
    let legend_items: [(Color, &str, f32); 4] = [
        (input_color, "Input", t.input_ms),
        (clear_color, "Clear", t.clear_ms),
        (render_color, "Render", t.render_ms),
        (ui_color, "UI", t.ui_ms),
    ];

    for (i, (color, name, ms)) in legend_items.iter().enumerate() {
        let y = legend_y + i as f32 * legend_line_height;
        draw_rectangle(bar_x, y - legend_box_size * 0.5, legend_box_size, legend_box_size, *color);
        draw_text(name, bar_x + 10.0 * scale, y + legend_box_size * 0.3, legend_text_size, label_color);
        draw_text(&format!("{:.2}ms", ms), bar_x + 55.0 * scale, y + legend_box_size * 0.3, legend_text_size, value_color);
    }

    // Render breakdown (indented)
    let render_y = legend_y + 4.0 * legend_line_height + 4.0 * scale;
    draw_text("Render breakdown:", bar_x, render_y, legend_text_size * 0.9, label_color);

    let render_items: [(Color, &str, f32); 5] = [
        (lights_color, "Lights", t.render_lights_ms),
        (texconv_color, "TexConv", t.render_texconv_ms),
        (meshgen_color, "MeshGen", t.render_meshgen_ms),
        (raster_color, "Raster", t.render_raster_ms),
        (upload_color, "Upload", t.render_upload_ms),
    ];

    let indent = 8.0 * scale;
    for (i, (color, name, ms)) in render_items.iter().enumerate() {
        let y = render_y + (i as f32 + 1.0) * legend_line_height;
        draw_rectangle(bar_x + indent, y - legend_box_size * 0.5, legend_box_size, legend_box_size, *color);
        draw_text(name, bar_x + indent + 10.0 * scale, y + legend_box_size * 0.3, legend_text_size, label_color);
        draw_text(&format!("{:.2}ms", ms), bar_x + indent + 55.0 * scale, y + legend_box_size * 0.3, legend_text_size, value_color);
    }

    // Raster breakdown (further indented)
    let raster_y = render_y + 6.0 * legend_line_height + 4.0 * scale;
    draw_text("Raster breakdown:", bar_x + indent, raster_y, legend_text_size * 0.9, label_color);

    let raster_items: [(Color, &str, f32); 6] = [
        (transform_color, "Transform", t.raster_transform_ms),
        (fog_timing_color, "Fog", t.raster_fog_ms),
        (cull_color, "Cull", t.raster_cull_ms),
        (sort_color, "Sort", t.raster_sort_ms),
        (draw_color, "Draw", t.raster_draw_ms),
        (wireframe_color, "Wireframe", t.raster_wireframe_ms),
    ];

    let indent2 = 16.0 * scale;
    for (i, (color, name, ms)) in raster_items.iter().enumerate() {
        let y = raster_y + (i as f32 + 1.0) * legend_line_height;
        draw_rectangle(bar_x + indent2, y - legend_box_size * 0.5, legend_box_size, legend_box_size, *color);
        draw_text(name, bar_x + indent2 + 10.0 * scale, y + legend_box_size * 0.3, legend_text_size, label_color);
        draw_text(&format!("{:.2}ms", ms), bar_x + indent2 + 55.0 * scale, y + legend_box_size * 0.3, legend_text_size, value_color);
    }

    // Triangle count (below raster breakdown)
    let tris_y = raster_y + 7.0 * legend_line_height + 4.0 * scale;
    draw_text(&format!("Triangles: {}", t.triangles_drawn), bar_x + indent, tris_y, legend_text_size, value_color);
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

        let proj = project(cam, fb.width, fb.height);
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

