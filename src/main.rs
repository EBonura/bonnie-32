//! Bonnie Engine: PS1-style software rasterizer engine
//!
//! A souls-like game engine with authentic PlayStation 1 rendering:
//! - Affine texture mapping (warpy textures)
//! - Vertex snapping (jittery vertices)
//! - Gouraud shading
//! - Low resolution (320x240)
//! - TR1-style room-based levels with portal culling

/// Version from Cargo.toml
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

mod rasterizer;
mod world;
mod ui;
mod editor;
mod landing;
mod app;

use macroquad::prelude::*;
use rasterizer::{Framebuffer, RasterSettings, Texture, HEIGHT, WIDTH};
use world::{create_empty_level, load_level, save_level};
use ui::{UiContext, MouseState, Rect, draw_fixed_tabs, layout as tab_layout};
use editor::{EditorAction, draw_editor};
use app::{AppState, Tool};
use std::path::PathBuf;

fn window_conf() -> Conf {
    Conf {
        window_title: format!("Bonnie Engine v{}", VERSION),
        window_width: WIDTH as i32 * 3,
        window_height: HEIGHT as i32 * 3,
        window_resizable: true,
        high_dpi: true,
        icon: Some(miniquad::conf::Icon {
            small: *include_bytes!("../assets/icons/icon16.rgba"),
            medium: *include_bytes!("../assets/icons/icon32.rgba"),
            big: *include_bytes!("../assets/icons/icon64.rgba"),
        }),
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    // Initialize framebuffer (used by 3D viewport in editor)
    let mut fb = Framebuffer::new(WIDTH, HEIGHT);

    // Load level from file, fall back to empty level
    let level = match load_level("assets/levels/test.ron") {
        Ok(l) => {
            println!("Loaded level from assets/levels/test.ron");
            l
        }
        Err(e) => {
            println!("Failed to load level: {}, using empty level", e);
            create_empty_level()
        }
    };

    // Load textures (used by editor via texture packs)
    let _textures = {
        let loaded = Texture::load_directory("assets/textures/SAMPLE");
        if loaded.is_empty() {
            println!("No textures found, using checkerboard fallbacks");
            Vec::<Texture>::new()
        } else {
            println!("Loaded {} textures", loaded.len());
            loaded
        }
    };

    // Rasterizer settings
    let settings = RasterSettings::default();

    // Mouse state tracking
    let mut last_left_down = false;

    // UI context
    let mut ui_ctx = UiContext::new();

    // Load icon font (Lucide)
    let icon_font = match load_ttf_font("assets/fonts/lucide.ttf").await {
        Ok(font) => {
            println!("Loaded Lucide icon font");
            Some(font)
        }
        Err(e) => {
            println!("Failed to load Lucide font: {}, icons will be missing", e);
            None
        }
    };

    // App state with all tools
    let initial_file = if std::path::Path::new("assets/levels/test.ron").exists() {
        Some(PathBuf::from("assets/levels/test.ron"))
    } else {
        None
    };
    let mut app = AppState::new(level, initial_file, icon_font);

    // Load textures from manifest (WASM needs async loading)
    #[cfg(target_arch = "wasm32")]
    {
        use editor::TexturePack;
        app.world_editor.editor_state.texture_packs = TexturePack::load_from_manifest().await;
        println!("WASM: Loaded {} texture packs", app.world_editor.editor_state.texture_packs.len());
    }

    println!("=== Bonnie Engine ===");
    println!("Click tabs to switch between tools");

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

        let screen_w = screen_width();
        let screen_h = screen_height();

        // Clear background
        clear_background(Color::from_rgba(30, 30, 35, 255));

        // Draw tab bar at top
        let tab_bar_rect = Rect::new(0.0, 0.0, screen_w, tab_layout::BAR_HEIGHT);
        let labels = Tool::labels();
        if let Some(clicked) = draw_fixed_tabs(&mut ui_ctx, tab_bar_rect, &labels, app.active_tool_index()) {
            if let Some(tool) = Tool::from_index(clicked) {
                app.set_active_tool(tool);
            }
        }

        // Content area below tab bar
        let content_rect = Rect::new(0.0, tab_layout::BAR_HEIGHT, screen_w, screen_h - tab_layout::BAR_HEIGHT);

        // Draw active tool content
        match app.active_tool {
            Tool::Home => {
                landing::draw_landing(content_rect, &mut app.landing);
            }

            Tool::WorldEditor => {
                let ws = &mut app.world_editor;

                // Check for pending import from browser (WASM only)
                #[cfg(target_arch = "wasm32")]
                {
                    extern "C" {
                        fn bonnie_check_import() -> i32;
                        fn bonnie_get_import_data_len() -> usize;
                        fn bonnie_get_import_filename_len() -> usize;
                        fn bonnie_copy_import_data(ptr: *mut u8, max_len: usize) -> usize;
                        fn bonnie_copy_import_filename(ptr: *mut u8, max_len: usize) -> usize;
                        fn bonnie_clear_import();
                    }

                    let has_import = unsafe { bonnie_check_import() };

                    if has_import != 0 {
                        let data_len = unsafe { bonnie_get_import_data_len() };
                        let filename_len = unsafe { bonnie_get_import_filename_len() };

                        let mut data_buf = vec![0u8; data_len];
                        let mut filename_buf = vec![0u8; filename_len];

                        unsafe {
                            bonnie_copy_import_data(data_buf.as_mut_ptr(), data_len);
                            bonnie_copy_import_filename(filename_buf.as_mut_ptr(), filename_len);
                            bonnie_clear_import();
                        }

                        let data = String::from_utf8_lossy(&data_buf).to_string();
                        let filename = String::from_utf8_lossy(&filename_buf).to_string();

                        match ron::from_str::<world::Level>(&data) {
                            Ok(level) => {
                                ws.editor_layout.apply_config(&level.editor_layout);
                                ws.editor_state.load_level(level, PathBuf::from(&filename));
                                ws.editor_state.set_status(&format!("Uploaded {}", filename), 3.0);
                            }
                            Err(e) => {
                                ws.editor_state.set_status(&format!("Upload failed: {}", e), 5.0);
                            }
                        }
                    }
                }

                // Build textures array from texture packs
                let editor_textures: Vec<Texture> = ws.editor_state.texture_packs
                    .iter()
                    .flat_map(|pack| &pack.textures)
                    .cloned()
                    .collect();

                // Draw editor UI
                let action = draw_editor(
                    &mut ui_ctx,
                    &mut ws.editor_layout,
                    &mut ws.editor_state,
                    &editor_textures,
                    &mut fb,
                    &settings,
                    content_rect,
                    app.icon_font.as_ref(),
                );

                // Handle editor actions
                handle_editor_action(action, ws);
            }

            Tool::SoundDesigner => {
                draw_placeholder(content_rect, "Sound Designer", Color::from_rgba(25, 30, 35, 255));
            }

            Tool::Tracker => {
                draw_placeholder(content_rect, "Tracker", Color::from_rgba(30, 25, 35, 255));
            }

            Tool::Game => {
                draw_placeholder(content_rect, "Game Preview", Color::from_rgba(20, 20, 25, 255));
            }
        }

        // Draw tooltips last (on top of everything)
        ui_ctx.draw_tooltip();

        next_frame().await;
    }
}

fn draw_placeholder(rect: Rect, name: &str, bg_color: Color) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, bg_color);
    let text = format!("{} - Coming Soon", name);
    let text_width = measure_text(&text, None, 24, 1.0).width;
    draw_text(
        &text,
        rect.x + (rect.w - text_width) * 0.5,
        rect.y + rect.h * 0.5,
        24.0,
        Color::from_rgba(100, 100, 100, 255),
    );
}

fn handle_editor_action(action: EditorAction, ws: &mut app::WorldEditorState) {
    match action {
        EditorAction::Play => {
            ws.editor_state.set_status("Game preview coming soon", 2.0);
        }
        EditorAction::New => {
            let new_level = create_empty_level();
            ws.editor_state = editor::EditorState::new(new_level);
            ws.editor_layout.apply_config(&ws.editor_state.level.editor_layout);
            ws.editor_state.set_status("Created new level", 3.0);
        }
        EditorAction::Save => {
            ws.editor_state.level.editor_layout = ws.editor_layout.to_config();

            if let Some(path) = &ws.editor_state.current_file.clone() {
                match save_level(&ws.editor_state.level, path) {
                    Ok(()) => {
                        ws.editor_state.dirty = false;
                        ws.editor_state.set_status(&format!("Saved to {}", path.display()), 3.0);
                    }
                    Err(e) => {
                        ws.editor_state.set_status(&format!("Save failed: {}", e), 5.0);
                    }
                }
            } else {
                let default_path = PathBuf::from("assets/levels/untitled.ron");
                if let Some(parent) = default_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match save_level(&ws.editor_state.level, &default_path) {
                    Ok(()) => {
                        ws.editor_state.current_file = Some(default_path.clone());
                        ws.editor_state.dirty = false;
                        ws.editor_state.set_status(&format!("Saved to {}", default_path.display()), 3.0);
                    }
                    Err(e) => {
                        ws.editor_state.set_status(&format!("Save failed: {}", e), 5.0);
                    }
                }
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        EditorAction::SaveAs => {
            ws.editor_state.level.editor_layout = ws.editor_layout.to_config();
            let default_dir = PathBuf::from("assets/levels");
            let _ = std::fs::create_dir_all(&default_dir);

            let dialog = rfd::FileDialog::new()
                .add_filter("RON Level", &["ron"])
                .set_directory(&default_dir)
                .set_file_name("level.ron");

            if let Some(save_path) = dialog.save_file() {
                match save_level(&ws.editor_state.level, &save_path) {
                    Ok(()) => {
                        ws.editor_state.current_file = Some(save_path.clone());
                        ws.editor_state.dirty = false;
                        ws.editor_state.set_status(&format!("Saved as {}", save_path.display()), 3.0);
                    }
                    Err(e) => {
                        ws.editor_state.set_status(&format!("Save failed: {}", e), 5.0);
                    }
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        EditorAction::SaveAs => {
            ws.editor_state.set_status("Save As not available in browser", 3.0);
        }
        #[cfg(not(target_arch = "wasm32"))]
        EditorAction::PromptLoad => {
            let default_dir = PathBuf::from("assets/levels");
            let _ = std::fs::create_dir_all(&default_dir);

            let dialog = rfd::FileDialog::new()
                .add_filter("RON Level", &["ron"])
                .set_directory(&default_dir);

            if let Some(path) = dialog.pick_file() {
                match load_level(&path) {
                    Ok(level) => {
                        ws.editor_layout.apply_config(&level.editor_layout);
                        ws.editor_state.load_level(level, path.clone());
                        ws.editor_state.set_status(&format!("Loaded {}", path.display()), 3.0);
                    }
                    Err(e) => {
                        ws.editor_state.set_status(&format!("Load failed: {}", e), 5.0);
                    }
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        EditorAction::PromptLoad => {
            ws.editor_state.set_status("Open not available in browser - use Upload", 3.0);
        }
        #[cfg(target_arch = "wasm32")]
        EditorAction::Export => {
            ws.editor_state.level.editor_layout = ws.editor_layout.to_config();

            match ron::ser::to_string_pretty(&ws.editor_state.level, ron::ser::PrettyConfig::default()) {
                Ok(ron_str) => {
                    let filename = ws.editor_state.current_file
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "level.ron".to_string());

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

                    ws.editor_state.dirty = false;
                    ws.editor_state.set_status(&format!("Downloaded {}", filename), 3.0);
                }
                Err(e) => {
                    ws.editor_state.set_status(&format!("Export failed: {}", e), 5.0);
                }
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        EditorAction::Export => {
            ws.editor_state.set_status("Export is for browser - use Save As", 3.0);
        }
        #[cfg(target_arch = "wasm32")]
        EditorAction::Import => {
            extern "C" {
                fn bonnie_import_file();
            }
            unsafe {
                bonnie_import_file();
            }
            ws.editor_state.set_status("Select a .ron file to import...", 3.0);
        }
        #[cfg(not(target_arch = "wasm32"))]
        EditorAction::Import => {
            ws.editor_state.set_status("Import is for browser - use Open", 3.0);
        }
        EditorAction::Load(path_str) => {
            let path = PathBuf::from(&path_str);
            match load_level(&path) {
                Ok(level) => {
                    ws.editor_layout.apply_config(&level.editor_layout);
                    ws.editor_state.load_level(level, path.clone());
                    ws.editor_state.set_status(&format!("Loaded {}", path.display()), 3.0);
                }
                Err(e) => {
                    ws.editor_state.set_status(&format!("Load failed: {}", e), 5.0);
                }
            }
        }
        EditorAction::Exit | EditorAction::None => {}
    }
}
