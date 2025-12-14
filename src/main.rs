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
mod modeler;
mod tracker;
mod app;

use macroquad::prelude::*;
use rasterizer::{Framebuffer, Texture, HEIGHT, WIDTH};
use world::{create_empty_level, load_level, save_level};
use ui::{UiContext, MouseState, Rect, draw_fixed_tabs, TabEntry, layout as tab_layout, icon};
use editor::{EditorAction, draw_editor, draw_example_browser, BrowserAction, discover_examples};
use app::{AppState, Tool};
use std::path::PathBuf;

fn window_conf() -> Conf {
    Conf {
        window_title: format!("Bonnie Engine v{}", VERSION),
        window_width: WIDTH as i32 * 3,
        window_height: HEIGHT as i32 * 3,
        window_resizable: true,
        high_dpi: true,
        // Start fullscreen on native, windowed on WASM (browser handles sizing)
        #[cfg(not(target_arch = "wasm32"))]
        fullscreen: true,
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

    // Start with empty level (user can open levels via browser)
    let level = create_empty_level();

    // Mouse state tracking
    let mut last_left_down = false;
    let mut last_right_down = false;
    let mut last_click_time = 0.0f64;
    let mut last_click_pos = (0.0f32, 0.0f32);

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
    let mut app = AppState::new(level, None, icon_font);

    // Track if this is the first time opening World Editor (to show browser)
    let mut world_editor_first_open = true;

    // Load textures and level list from manifest (WASM needs async loading)
    #[cfg(target_arch = "wasm32")]
    let preloaded_levels = {
        use editor::TexturePack;
        use editor::load_example_list;
        app.world_editor.editor_state.texture_packs = TexturePack::load_from_manifest().await;
        println!("WASM: Loaded {} texture packs", app.world_editor.editor_state.texture_packs.len());
        let levels = load_example_list().await;
        println!("WASM: Loaded {} example levels from manifest", levels.len());
        levels
    };
    #[cfg(not(target_arch = "wasm32"))]
    let preloaded_levels = discover_examples();

    println!("=== Bonnie Engine ===");

    loop {
        // Update UI context with mouse state
        let mouse_pos = mouse_position();
        let left_down = is_mouse_button_down(MouseButton::Left);
        // Detect double-click (300ms window, 10px radius)
        let left_pressed = left_down && !last_left_down;
        let current_time = get_time();
        let double_click_threshold = 0.3; // 300ms
        let double_click_radius = 10.0;
        let double_clicked = if left_pressed {
            let time_delta = current_time - last_click_time;
            let dx = mouse_pos.0 - last_click_pos.0;
            let dy = mouse_pos.1 - last_click_pos.1;
            let dist = (dx * dx + dy * dy).sqrt();
            let is_double = time_delta < double_click_threshold && dist < double_click_radius;
            last_click_time = current_time;
            last_click_pos = mouse_pos;
            is_double
        } else {
            false
        };

        let right_down = is_mouse_button_down(MouseButton::Right);
        let right_pressed = right_down && !last_right_down;
        let mouse_state = MouseState {
            x: mouse_pos.0,
            y: mouse_pos.1,
            left_down,
            right_down,
            left_pressed,
            left_released: !left_down && last_left_down,
            right_pressed,
            scroll: mouse_wheel().1,
            double_clicked,
        };
        last_left_down = left_down;
        last_right_down = right_down;
        ui_ctx.begin_frame(mouse_state);

        // Block background input if example browser modal is open
        // Save the real mouse state so we can restore it for the modal
        let real_mouse = mouse_state;
        if app.world_editor.example_browser.open {
            ui_ctx.begin_modal();
        }

        let screen_w = screen_width();
        let screen_h = screen_height();

        // Clear background
        clear_background(Color::from_rgba(30, 30, 35, 255));

        // Draw tab bar at top
        let tab_bar_rect = Rect::new(0.0, 0.0, screen_w, tab_layout::BAR_HEIGHT);
        let tabs = [
            TabEntry::new(icon::HOUSE, "Home"),
            TabEntry::new(icon::GLOBE, "World"),
            TabEntry::new(icon::PERSON_STANDING, "Assets"),
            TabEntry::new(icon::MUSIC, "Music"),
        ];
        if let Some(clicked) = draw_fixed_tabs(&mut ui_ctx, tab_bar_rect, &tabs, app.active_tool_index(), app.icon_font.as_ref()) {
            if let Some(tool) = Tool::from_index(clicked) {
                // Open browser on first World Editor visit
                if tool == Tool::WorldEditor && world_editor_first_open {
                    world_editor_first_open = false;
                    app.world_editor.example_browser.open(preloaded_levels.clone());
                }
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
                    /// Maximum import file size (10 MB) to prevent memory exhaustion
                    const MAX_IMPORT_SIZE: usize = 10 * 1024 * 1024;
                    /// Maximum filename length
                    const MAX_FILENAME_LEN: usize = 256;

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

                        // Security: Check sizes before allocation to prevent memory exhaustion
                        if data_len > MAX_IMPORT_SIZE {
                            unsafe { bonnie_clear_import(); }
                            ws.editor_state.set_status("Import failed: file too large (max 10MB)", 5.0);
                        } else if filename_len > MAX_FILENAME_LEN {
                            unsafe { bonnie_clear_import(); }
                            ws.editor_state.set_status("Import failed: filename too long", 5.0);
                        } else {
                            let mut data_buf = vec![0u8; data_len];
                            let mut filename_buf = vec![0u8; filename_len];

                            unsafe {
                                bonnie_copy_import_data(data_buf.as_mut_ptr(), data_len);
                                bonnie_copy_import_filename(filename_buf.as_mut_ptr(), filename_len);
                                bonnie_clear_import();
                            }

                            let data = String::from_utf8_lossy(&data_buf).to_string();
                            let filename = String::from_utf8_lossy(&filename_buf).to_string();

                            // Use load_level_from_str which includes validation
                            match world::load_level_from_str(&data) {
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
                    content_rect,
                    app.icon_font.as_ref(),
                );

                // Handle editor actions (including opening example browser)
                handle_editor_action(action, ws, &preloaded_levels);

                // Draw example browser overlay if open
                if ws.example_browser.open {
                    // End modal blocking so the browser itself can receive input
                    ui_ctx.end_modal(real_mouse);

                    let browser_action = draw_example_browser(
                        &mut ui_ctx,
                        &mut ws.example_browser,
                        app.icon_font.as_ref(),
                        &ws.editor_state.texture_packs,
                        &mut fb,
                    );

                    match browser_action {
                        BrowserAction::SelectPreview(index) => {
                            // Load the preview synchronously
                            if let Some(example) = ws.example_browser.examples.get(index) {
                                let path = example.path.clone();
                                #[cfg(not(target_arch = "wasm32"))]
                                {
                                    match load_level(&path) {
                                        Ok(level) => {
                                            println!("Loaded example level with {} rooms", level.rooms.len());
                                            ws.example_browser.set_preview(level);
                                        }
                                        Err(e) => {
                                            eprintln!("Failed to load example {}: {}", path.display(), e);
                                            ws.editor_state.set_status(&format!("Failed to load: {}", e), 3.0);
                                        }
                                    }
                                }
                            }
                        }
                        BrowserAction::OpenLevel => {
                            // Load the selected level with its file path preserved
                            if let Some(level) = ws.example_browser.preview_level.take() {
                                let (name, path) = ws.example_browser.selected_example()
                                    .map(|e| (e.name.clone(), e.path.clone()))
                                    .unwrap_or_else(|| ("example".to_string(), PathBuf::from("assets/levels/untitled.ron")));
                                ws.editor_layout.apply_config(&level.editor_layout);
                                // Use with_file to preserve the file path for saving
                                ws.editor_state = editor::EditorState::with_file(level, path);
                                ws.editor_state.set_status(&format!("Opened: {}", name), 3.0);
                                ws.example_browser.close();
                            }
                        }
                        BrowserAction::NewLevel => {
                            // Start with a fresh empty level
                            let new_level = create_empty_level();
                            ws.editor_state = editor::EditorState::new(new_level);
                            ws.editor_layout.apply_config(&ws.editor_state.level.editor_layout);
                            ws.editor_state.set_status("New level created", 3.0);
                            ws.example_browser.close();
                        }
                        BrowserAction::Cancel => {
                            ws.example_browser.close();
                        }
                        BrowserAction::None => {}
                    }
                }
            }

            Tool::Modeler => {
                // Update animation playback
                let delta = get_frame_time() as f64;
                app.modeler.modeler_state.update_playback(delta);

                // Draw modeler UI
                let _action = modeler::draw_modeler(
                    &mut ui_ctx,
                    &mut app.modeler.modeler_layout,
                    &mut app.modeler.modeler_state,
                    &mut fb,
                    content_rect,
                    app.icon_font.as_ref(),
                );

                // TODO: Handle modeler actions (New, Save, Load, Export, Import)
            }

            Tool::Tracker => {
                // Update playback timing
                let delta = get_frame_time() as f64;
                app.tracker.update_playback(delta);

                // Draw tracker UI
                tracker::draw_tracker(&mut ui_ctx, content_rect, &mut app.tracker, app.icon_font.as_ref());
            }
        }

        // Draw tooltips last (on top of everything)
        ui_ctx.draw_tooltip();

        next_frame().await;
    }
}


fn handle_editor_action(action: EditorAction, ws: &mut app::WorldEditorState, preloaded_levels: &[editor::ExampleLevelInfo]) {
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
        EditorAction::BrowseExamples => {
            // Open the level browser
            ws.example_browser.open(preloaded_levels.to_vec());
            ws.editor_state.set_status("Browse levels", 2.0);
        }
        EditorAction::Exit | EditorAction::None => {}
    }
}
