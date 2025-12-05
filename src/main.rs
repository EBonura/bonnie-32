//! bonnie-rs: PS1-style software rasterizer engine
//!
//! A souls-like game engine with authentic PlayStation 1 rendering:
//! - Affine texture mapping (warpy textures)
//! - Vertex snapping (jittery vertices)
//! - Gouraud shading
//! - Low resolution (320x240)
//! - TR1-style room-based levels with portal culling

mod rasterizer;
mod world;
mod ui;
mod editor;

use macroquad::prelude::*;
use rasterizer::{
    Camera, Color as RasterColor, Framebuffer, RasterSettings, ShadingMode, Texture,
    render_mesh, HEIGHT, WIDTH,
};
use world::{Level, create_test_level, create_empty_level, load_level, save_level};
use ui::{UiContext, MouseState};
use editor::{EditorState, EditorLayout, EditorAction, draw_editor};
use std::path::PathBuf;

/// Application mode
#[derive(Debug, Clone, Copy, PartialEq)]
enum AppMode {
    Game,
    Editor,
}

/// Convert our framebuffer to a macroquad texture
fn framebuffer_to_texture(fb: &Framebuffer) -> Texture2D {
    let texture = Texture2D::from_rgba8(fb.width as u16, fb.height as u16, &fb.pixels);
    texture.set_filter(FilterMode::Nearest); // No filtering - crispy pixels!
    texture
}

fn window_conf() -> Conf {
    Conf {
        window_title: "bonnie-rs :: PS1 Software Rasterizer".to_owned(),
        window_width: WIDTH as i32 * 3,  // 960x720 window
        window_height: HEIGHT as i32 * 3,
        window_resizable: true,
        ..Default::default()
    }
}

/// Render all rooms in a level
fn render_level(
    fb: &mut Framebuffer,
    level: &Level,
    textures: &[Texture],
    camera: &Camera,
    settings: &RasterSettings,
) {
    for room in &level.rooms {
        let (vertices, faces) = room.to_render_data();
        render_mesh(fb, &vertices, &faces, textures, camera, settings);
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    // Initialize framebuffer
    let mut fb = Framebuffer::new(WIDTH, HEIGHT);

    // Initialize camera - position inside first room
    let mut camera = Camera::new();
    camera.position = rasterizer::Vec3::new(0.0, 1.5, 0.0);

    // Load level from file, fall back to hardcoded test level
    let level = match load_level("assets/levels/test.ron") {
        Ok(l) => {
            println!("Loaded level from assets/levels/test.ron");
            l
        }
        Err(e) => {
            println!("Failed to load level: {}, using hardcoded test level", e);
            create_test_level()
        }
    };

    // Load textures from assets/textures/SAMPLE, fall back to checkerboards
    let mut textures = Texture::load_directory("assets/textures/SAMPLE");
    if textures.is_empty() {
        println!("No textures found in assets/textures/SAMPLE, using checkerboard fallbacks");
        textures = vec![
            Texture::checkerboard(32, 32, RasterColor::new(200, 100, 50), RasterColor::new(50, 100, 200)),
            Texture::checkerboard(32, 32, RasterColor::new(50, 200, 100), RasterColor::new(150, 50, 200)),
        ];
    } else {
        println!("Loaded {} textures", textures.len());
    }

    // Rasterizer settings
    let mut settings = RasterSettings::default();

    // Mouse state for camera control
    let mut last_mouse_pos = mouse_position();
    let mut mouse_captured = false;
    let mut last_left_down = false;

    // Track which room camera is in
    let mut current_room: Option<usize> = Some(0);

    // App mode (game or editor) - start in editor
    let mut mode = AppMode::Editor;

    // Editor state - track the file we loaded from
    let initial_file = if std::path::Path::new("assets/levels/test.ron").exists() {
        Some(PathBuf::from("assets/levels/test.ron"))
    } else {
        None
    };
    let mut editor_state = if let Some(path) = initial_file {
        EditorState::with_file(level.clone(), path)
    } else {
        EditorState::new(level.clone())
    };
    let mut editor_layout = EditorLayout::new();
    let mut ui_ctx = UiContext::new();

    println!("=== bonnie-rs ===");
    println!("Controls:");
    println!("  Editor: Click 'Play' to test level");
    println!("  Game: Press Esc to return to editor");
    println!("  Right-click + drag: Look around");
    println!("  WASD: Move camera");
    println!("  Q/E: Move up/down");
    println!("  1/2/3: Shading mode (None/Flat/Gouraud)");
    println!("  P: Toggle perspective correction");
    println!("  J: Toggle vertex jitter");
    println!("  Z: Toggle Z-buffer");

    loop {
        // Update UI context with mouse state
        let mouse_pos = mouse_position();
        let left_down = is_mouse_button_down(MouseButton::Left);
        let mouse_state = MouseState {
            x: mouse_pos.0,
            y: mouse_pos.1,
            left_down,
            right_down: is_mouse_button_down(MouseButton::Right),
            left_pressed: left_down && !last_left_down,
            left_released: !left_down && last_left_down,
            scroll: mouse_wheel().1,
        };
        last_left_down = left_down;
        ui_ctx.begin_frame(mouse_state);

        match mode {
            AppMode::Game => {
                // Toggle settings
                if is_key_pressed(KeyCode::Key1) {
                    settings.shading = ShadingMode::None;
                    println!("Shading: None");
                }
                if is_key_pressed(KeyCode::Key2) {
                    settings.shading = ShadingMode::Flat;
                    println!("Shading: Flat");
                }
                if is_key_pressed(KeyCode::Key3) {
                    settings.shading = ShadingMode::Gouraud;
                    println!("Shading: Gouraud");
                }
                if is_key_pressed(KeyCode::P) {
                    settings.affine_textures = !settings.affine_textures;
                    println!(
                        "Textures: {}",
                        if settings.affine_textures { "Affine (PS1)" } else { "Perspective-correct" }
                    );
                }
                if is_key_pressed(KeyCode::J) {
                    settings.vertex_snap = !settings.vertex_snap;
                    println!(
                        "Vertex snap: {}",
                        if settings.vertex_snap { "ON (PS1 jitter)" } else { "OFF (smooth)" }
                    );
                }
                if is_key_pressed(KeyCode::Z) {
                    settings.use_zbuffer = !settings.use_zbuffer;
                    println!(
                        "Z-buffer: {}",
                        if settings.use_zbuffer { "ON" } else { "OFF (painter's)" }
                    );
                }

                // Camera rotation with right mouse button
                if is_mouse_button_down(MouseButton::Right) {
                    if mouse_captured {
                        // Note: negated dx for non-inverted vertical look
                        let dx = -(mouse_pos.1 - last_mouse_pos.1) * 0.005;
                        let dy = (mouse_pos.0 - last_mouse_pos.0) * 0.005;
                        camera.rotate(dx, dy);
                    }
                    mouse_captured = true;
                } else {
                    mouse_captured = false;
                }
                last_mouse_pos = mouse_pos;

                // Camera movement (WASD + Q/E for vertical)
                let move_speed = 0.05;
                if is_key_down(KeyCode::W) {
                    camera.position = camera.position + camera.basis_z * move_speed;
                }
                if is_key_down(KeyCode::S) {
                    camera.position = camera.position - camera.basis_z * move_speed;
                }
                if is_key_down(KeyCode::A) {
                    camera.position = camera.position - camera.basis_x * move_speed;
                }
                if is_key_down(KeyCode::D) {
                    camera.position = camera.position + camera.basis_x * move_speed;
                }
                if is_key_down(KeyCode::Q) {
                    camera.position = camera.position - camera.basis_y * move_speed;
                }
                if is_key_down(KeyCode::E) {
                    camera.position = camera.position + camera.basis_y * move_speed;
                }

                // Update current room (with hint for faster lookup)
                let new_room = level.find_room_at_with_hint(camera.position, current_room);
                if new_room != current_room {
                    if let Some(room_id) = new_room {
                        println!("Entered room {}", room_id);
                    }
                    current_room = new_room;
                }

                // Clear framebuffer
                fb.clear(RasterColor::new(20, 20, 30));

                // Render the level (all rooms for now - portal culling comes later)
                render_level(&mut fb, &level, &textures, &camera, &settings);

                // Convert framebuffer to macroquad texture
                let texture = framebuffer_to_texture(&fb);

                // Draw to screen (scaled up)
                clear_background(BLACK);

                // Calculate scaled size maintaining aspect ratio
                let screen_w = screen_width();
                let screen_h = screen_height();
                let scale = (screen_w / WIDTH as f32).min(screen_h / HEIGHT as f32);
                let draw_w = WIDTH as f32 * scale;
                let draw_h = HEIGHT as f32 * scale;
                let draw_x = (screen_w - draw_w) / 2.0;
                let draw_y = (screen_h - draw_h) / 2.0;

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

                // Draw HUD
                let room_text = match current_room {
                    Some(id) => format!("Room: {}", id),
                    None => "Room: outside".to_string(),
                };
                draw_text(&format!("FPS: {}", get_fps()), 10.0, 20.0, 20.0, WHITE);
                draw_text(&room_text, 10.0, 40.0, 20.0, WHITE);
                draw_text(
                    &format!("Pos: ({:.1}, {:.1}, {:.1})", camera.position.x, camera.position.y, camera.position.z),
                    10.0, 60.0, 20.0, WHITE
                );
                draw_text("[Esc] Editor", 10.0, 80.0, 16.0, Color::from_rgba(150, 150, 150, 255));

                // Return to editor with Escape
                if is_key_pressed(KeyCode::Escape) {
                    mode = AppMode::Editor;
                    println!("Switched to Editor mode");
                }
            }

            AppMode::Editor => {
                clear_background(Color::from_rgba(30, 30, 35, 255));

                // Check for pending import from browser (WASM only)
                #[cfg(target_arch = "wasm32")]
                {
                    // Check if import is ready via quad-storage (set by JS in index.html)
                    if quad_storage::STORAGE.lock().unwrap().get("_bonnie_import_ready") == Some("true".to_string()) {
                        if let Some(data) = quad_storage::STORAGE.lock().unwrap().get("_bonnie_import_data") {
                            let filename = quad_storage::STORAGE.lock().unwrap()
                                .get("_bonnie_import_filename")
                                .unwrap_or_else(|| "imported.ron".to_string());

                            // Clear the import flags
                            {
                                let mut storage = quad_storage::STORAGE.lock().unwrap();
                                storage.remove("_bonnie_import_ready");
                                storage.remove("_bonnie_import_data");
                                storage.remove("_bonnie_import_filename");
                            }

                            // Parse the level data
                            match ron::from_str::<Level>(&data) {
                                Ok(level) => {
                                    editor_state = EditorState::with_file(level, PathBuf::from(&filename));
                                    editor_state.set_status(&format!("Imported {}", filename), 3.0);
                                }
                                Err(e) => {
                                    editor_state.set_status(&format!("Import failed: {}", e), 5.0);
                                }
                            }
                        }
                    }
                }

                // Draw editor UI
                let action = draw_editor(
                    &mut ui_ctx,
                    &mut editor_layout,
                    &mut editor_state,
                    &textures,
                    &mut fb,
                    &settings,
                );

                // Handle editor actions
                match action {
                    EditorAction::Play => {
                        mode = AppMode::Game;
                        println!("Switched to Game mode");
                    }
                    EditorAction::New => {
                        // Create a new empty level with one room
                        let new_level = create_empty_level();
                        editor_state = EditorState::new(new_level);
                        editor_state.set_status("Created new level", 3.0);
                        println!("Created new level");
                    }
                    EditorAction::Save => {
                        // Save to current file, or prompt for Save As if no file
                        if let Some(path) = &editor_state.current_file.clone() {
                            match save_level(&editor_state.level, path) {
                                Ok(()) => {
                                    editor_state.dirty = false;
                                    editor_state.set_status(&format!("Saved to {}", path.display()), 3.0);
                                    println!("Saved level to {}", path.display());
                                }
                                Err(e) => {
                                    editor_state.set_status(&format!("Save failed: {}", e), 5.0);
                                    eprintln!("Failed to save: {}", e);
                                }
                            }
                        } else {
                            // No current file - save to default location
                            let default_path = PathBuf::from("assets/levels/untitled.ron");
                            // Ensure directory exists
                            if let Some(parent) = default_path.parent() {
                                let _ = std::fs::create_dir_all(parent);
                            }
                            match save_level(&editor_state.level, &default_path) {
                                Ok(()) => {
                                    editor_state.current_file = Some(default_path.clone());
                                    editor_state.dirty = false;
                                    editor_state.set_status(&format!("Saved to {}", default_path.display()), 3.0);
                                    println!("Saved level to {}", default_path.display());
                                }
                                Err(e) => {
                                    editor_state.set_status(&format!("Save failed: {}", e), 5.0);
                                    eprintln!("Failed to save: {}", e);
                                }
                            }
                        }
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    EditorAction::SaveAs => {
                        // Show native save dialog (blocking on macOS)
                        let default_dir = PathBuf::from("assets/levels");
                        let _ = std::fs::create_dir_all(&default_dir);

                        let dialog = rfd::FileDialog::new()
                            .add_filter("RON Level", &["ron"])
                            .set_directory(&default_dir)
                            .set_file_name("level.ron");

                        if let Some(save_path) = dialog.save_file() {
                            match save_level(&editor_state.level, &save_path) {
                                Ok(()) => {
                                    editor_state.current_file = Some(save_path.clone());
                                    editor_state.dirty = false;
                                    editor_state.set_status(&format!("Saved as {}", save_path.display()), 3.0);
                                    println!("Saved level as {}", save_path.display());
                                }
                                Err(e) => {
                                    editor_state.set_status(&format!("Save failed: {}", e), 5.0);
                                    eprintln!("Failed to save: {}", e);
                                }
                            }
                        } else {
                            editor_state.set_status("Save cancelled", 2.0);
                        }
                    }
                    #[cfg(target_arch = "wasm32")]
                    EditorAction::SaveAs => {
                        editor_state.set_status("Save As not available in browser", 3.0);
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    EditorAction::PromptLoad => {
                        // Show native open dialog (blocking on macOS)
                        let default_dir = PathBuf::from("assets/levels");
                        let _ = std::fs::create_dir_all(&default_dir);

                        let dialog = rfd::FileDialog::new()
                            .add_filter("RON Level", &["ron"])
                            .set_directory(&default_dir);

                        if let Some(path) = dialog.pick_file() {
                            match load_level(&path) {
                                Ok(level) => {
                                    editor_state = EditorState::with_file(level, path.clone());
                                    editor_state.set_status(&format!("Loaded {}", path.display()), 3.0);
                                    println!("Loaded level from {}", path.display());
                                }
                                Err(e) => {
                                    editor_state.set_status(&format!("Load failed: {}", e), 5.0);
                                    eprintln!("Failed to load: {}", e);
                                }
                            }
                        } else {
                            editor_state.set_status("Open cancelled", 2.0);
                        }
                    }
                    #[cfg(target_arch = "wasm32")]
                    EditorAction::PromptLoad => {
                        editor_state.set_status("Open not available in browser - use Import", 3.0);
                    }
                    #[cfg(target_arch = "wasm32")]
                    EditorAction::Export => {
                        // Serialize level to RON string
                        match ron::ser::to_string_pretty(&editor_state.level, ron::ser::PrettyConfig::default()) {
                            Ok(ron_str) => {
                                // Trigger browser download via JS
                                let filename = editor_state.current_file
                                    .as_ref()
                                    .and_then(|p| p.file_name())
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| "level.ron".to_string());

                                // Store data in localStorage, then call JS function to download
                                {
                                    let mut storage = quad_storage::STORAGE.lock().unwrap();
                                    storage.set("_bonnie_export_data", &ron_str);
                                    storage.set("_bonnie_export_filename", &filename);
                                }

                                // Call JS to trigger the download
                                extern "C" {
                                    fn bonnie_export_file();
                                }
                                unsafe {
                                    bonnie_export_file();
                                }

                                editor_state.dirty = false;
                                editor_state.set_status(&format!("Exported {}", filename), 3.0);
                            }
                            Err(e) => {
                                editor_state.set_status(&format!("Export failed: {}", e), 5.0);
                            }
                        }
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    EditorAction::Export => {
                        editor_state.set_status("Export is for browser - use Save As", 3.0);
                    }
                    #[cfg(target_arch = "wasm32")]
                    EditorAction::Import => {
                        // Trigger the file input via JS (defined in index.html)
                        // The import is handled asynchronously at the start of the editor loop
                        extern "C" {
                            fn bonnie_import_file();
                        }
                        unsafe {
                            bonnie_import_file();
                        }
                        editor_state.set_status("Select a .ron file to import...", 3.0);
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    EditorAction::Import => {
                        editor_state.set_status("Import is for browser - use Open", 3.0);
                    }
                    EditorAction::Load(path_str) => {
                        let path = PathBuf::from(&path_str);
                        match load_level(&path) {
                            Ok(level) => {
                                editor_state = EditorState::with_file(level, path.clone());
                                editor_state.set_status(&format!("Loaded {}", path.display()), 3.0);
                                println!("Loaded level from {}", path.display());
                            }
                            Err(e) => {
                                editor_state.set_status(&format!("Load failed: {}", e), 5.0);
                                eprintln!("Failed to load {}: {}", path.display(), e);
                            }
                        }
                    }
                    EditorAction::None => {}
                }
            }
        }

        next_frame().await;
    }
}
