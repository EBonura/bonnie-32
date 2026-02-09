//! BONNIE-32: A fantasy console for PS1-era 3D games
//!
//! Like PICO-8, but for low-poly 3D. Authentic PlayStation 1 rendering:
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
mod game;
mod project;
mod input;
mod texture;
mod asset;
mod storage;
mod auth;
mod scene;

use macroquad::prelude::*;
use rasterizer::{Framebuffer, Texture, HEIGHT, WIDTH};
use world::{create_empty_level, load_level_with_storage, serialize_level, save_level_with_storage};
use storage::{save_async, list_async, load_async, Storage};
use ui::{UiContext, MouseState, Rect, draw_fixed_tabs_with_auth, TabBarAction, TabEntry, layout as tab_layout, icon};
use editor::{EditorAction, draw_editor, draw_level_browser, BrowserAction, LevelCategory, discover_sample_levels, discover_user_levels};
use modeler::{ModelerAction, ModelBrowserAction, ObjImportAction, draw_model_browser, draw_obj_importer, discover_models, discover_meshes, ObjImporter, TextureImportResult};
use app::{AppState, Tool};
use std::path::PathBuf;

fn window_conf() -> Conf {
    Conf {
        window_title: format!("BONNIE-32 v{}", VERSION),
        // Request oversized dimensions so macOS clamps to screen bounds (pseudo-maximize)
        window_width: 3840,
        window_height: 2160,
        window_resizable: true,
        high_dpi: true,
        // Start windowed on all platforms (WASM: browser handles sizing)
        #[cfg(not(target_arch = "wasm32"))]
        fullscreen: false,
        icon: Some(miniquad::conf::Icon {
            small: *include_bytes!("../assets/runtime/icons/icon16.rgba"),
            medium: *include_bytes!("../assets/runtime/icons/icon32.rgba"),
            big: *include_bytes!("../assets/runtime/icons/icon64.rgba"),
        }),
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    // Initialize crash logging FIRST (before any other code)
    #[cfg(not(target_arch = "wasm32"))]
    crashlog::setup!(crashlog::cargo_metadata!().capitalized(), false);

    // Note: console_error_panic_hook was removed because it requires wasm-bindgen
    // which conflicts with macroquad's JS bundle. Panics still show in browser console.

    // Initialize framebuffer (used by 3D viewport in editor)
    let mut fb = Framebuffer::new(WIDTH, HEIGHT);

    // Start with empty level (user can open levels via browser)
    let level = create_empty_level();

    // Mouse state tracking
    // (mouse edge-detection now uses macroquad's event-based is_mouse_button_pressed/released)
    let mut last_click_time = 0.0f64;
    let mut last_click_pos = (0.0f32, 0.0f32);

    // Version highlight state (easter egg - click to toggle!)
    let mut version_highlighted = false;

    // UI context
    let mut ui_ctx = UiContext::new();

    // Load icon font (Lucide)
    let icon_font = match load_ttf_font("assets/runtime/fonts/lucide.ttf").await {
        Ok(font) => {
            println!("Loaded Lucide icon font");
            Some(font)
        }
        Err(e) => {
            println!("Failed to load Lucide font: {}, icons will be missing", e);
            None
        }
    };

    // Load logo texture
    let logo_texture = match load_texture("assets/runtime/branding/logo.png").await {
        Ok(tex) => {
            tex.set_filter(FilterMode::Linear);
            println!("Loaded logo texture");
            Some(tex)
        }
        Err(e) => {
            println!("Failed to load logo: {}", e);
            None
        }
    };

    // App state with all tools
    let mut app = AppState::new(level, None, icon_font, logo_texture);

    // Initialize GCP authentication (loads Google Identity Services on WASM)
    auth::init();

    // Track if this is the first time opening World Editor (to show browser)
    let mut world_editor_first_open = true;

    // Load textures from manifest (WASM needs async loading)
    #[cfg(target_arch = "wasm32")]
    {
        use editor::TexturePack;
        app.world_editor.editor_state.texture_packs = TexturePack::load_from_manifest().await;
        println!("WASM: Loaded {} texture packs", app.world_editor.editor_state.texture_packs.len());

        // Load user textures from manifest (for both editor and modeler)
        match app.world_editor.editor_state.user_textures.discover_from_manifest().await {
            Ok(count) => println!("WASM: Loaded {} user textures for editor", count),
            Err(e) => eprintln!("WASM: Failed to load user textures for editor: {}", e),
        }
        match app.modeler.modeler_state.user_textures.discover_from_manifest().await {
            Ok(count) => println!("WASM: Loaded {} user textures for modeler", count),
            Err(e) => eprintln!("WASM: Failed to load user textures for modeler: {}", e),
        }
    }

    println!("=== BONNIE-32 ===");

    loop {
        // Track frame start time for FPS limiting
        let frame_start = get_time();

        // Update authentication state (checks for sign-in/sign-out)
        // When auth state changes, refresh browser's user levels to avoid stale data
        if app.update_auth() {
            let browser = &mut app.world_editor.level_browser;
            // Clear any stale preview if it was from cloud
            if browser.selected_category == Some(LevelCategory::User) {
                browser.preview_level = None;
                browser.preview_stats = None;
                browser.pending_preview_load = None;
            }
            // Cancel any pending user list operation
            browser.pending_user_list = None;

            #[cfg(not(target_arch = "wasm32"))]
            {
                if app.storage.has_cloud() {
                    // Now authenticated: start async cloud discovery
                    browser.pending_user_list = Some(list_async("assets/userdata/levels".to_string()));
                } else {
                    // Now unauthenticated: load local user levels (sync, fast)
                    browser.user_levels = discover_user_levels(&app.storage);
                }
            }
            #[cfg(target_arch = "wasm32")]
            {
                if app.storage.has_cloud() {
                    // WASM + auth: trigger async list load
                    browser.pending_load_list = true;
                } else {
                    // WASM + no auth: no user levels available
                    browser.user_levels = Vec::new();
                }
            }

            // Also refresh asset browsers when auth changes
            // Modeler's asset browser
            {
                let mb = &mut app.modeler.model_browser;
                if mb.selected_category == Some(modeler::AssetCategory::User) {
                    mb.preview_asset = None;
                    mb.pending_preview_load = None;
                }
                mb.pending_user_list = None;
                #[cfg(target_arch = "wasm32")]
                {
                    if app.storage.has_cloud() {
                        mb.pending_load_list = true;
                    } else {
                        mb.user_assets = Vec::new();
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    if app.storage.has_cloud() {
                        mb.pending_user_list = Some(list_async("assets/userdata/assets".to_string()));
                    } else {
                        mb.user_assets = modeler::discover_user_assets();
                    }
                }
            }
            // World editor's asset browser
            {
                let ab = &mut app.world_editor.editor_state.asset_browser;
                if ab.selected_category == Some(modeler::AssetCategory::User) {
                    ab.preview_asset = None;
                    ab.pending_preview_load = None;
                }
                ab.pending_user_list = None;
                #[cfg(target_arch = "wasm32")]
                {
                    if app.storage.has_cloud() {
                        ab.pending_load_list = true;
                    } else {
                        ab.user_assets = Vec::new();
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    if app.storage.has_cloud() {
                        ab.pending_user_list = Some(list_async("assets/userdata/assets".to_string()));
                    } else {
                        ab.user_assets = modeler::discover_user_assets();
                    }
                }
            }
            // Tracker's song browser
            {
                let sb = &mut app.tracker.song_browser;
                if sb.selected_category == Some(tracker::SongCategory::User) {
                    sb.preview_song = None;
                    sb.pending_preview_load = None;
                }
                sb.pending_user_list = None;
                if app.storage.has_cloud() {
                    sb.user_songs.clear();
                    sb.pending_user_list = Some(list_async("assets/userdata/songs".to_string()));
                } else {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        sb.user_songs = tracker::discover_songs_from_dir(
                            tracker::USER_SONGS_DIR,
                            tracker::SongCategory::User,
                        );
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        sb.user_songs = Vec::new();
                    }
                }
            }
            // World editor's texture library
            {
                let es = &mut app.world_editor.editor_state;
                es.pending_user_texture_list = None;
                es.pending_texture_loads.clear();
                if app.storage.has_cloud() {
                    es.user_textures.clear_user_textures();
                    es.pending_user_texture_list = Some(list_async("assets/userdata/textures".to_string()));
                } else {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        if let Err(e) = es.user_textures.discover() {
                            eprintln!("Failed to discover textures: {}", e);
                        }
                    }
                }
            }
            // Modeler's texture library
            {
                let ms = &mut app.modeler.modeler_state;
                ms.pending_user_texture_list = None;
                ms.pending_texture_loads.clear();
                if app.storage.has_cloud() {
                    ms.user_textures.clear_user_textures();
                    ms.pending_user_texture_list = Some(list_async("assets/userdata/textures".to_string()));
                } else {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        if let Err(e) = ms.user_textures.discover() {
                            eprintln!("Failed to discover textures: {}", e);
                        }
                    }
                }
            }
        }

        // Poll pending async operations (save, load)
        poll_pending_ops(&mut app);

        // Update UI context with mouse state
        // Use macroquad's event-based press/release detection (won't miss fast clicks)
        let mouse_pos = mouse_position();
        let left_down = is_mouse_button_down(MouseButton::Left);
        let left_pressed = is_mouse_button_pressed(MouseButton::Left);
        // Detect double-click (300ms window, 10px radius)
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
        let mouse_state = MouseState {
            x: mouse_pos.0,
            y: mouse_pos.1,
            left_down,
            right_down,
            left_pressed,
            left_released: is_mouse_button_released(MouseButton::Left),
            right_pressed: is_mouse_button_pressed(MouseButton::Right),
            scroll: mouse_wheel().1,
            double_clicked,
        };
        ui_ctx.begin_frame(mouse_state);

        // Poll gamepad input
        app.input.poll();

        // Block background input if level browser modal is open
        // Save the real mouse state so we can restore it for the modal
        let real_mouse = mouse_state;
        if app.world_editor.level_browser.open {
            ui_ctx.begin_modal();
        }

        let screen_w = screen_width();
        let screen_h = screen_height();

        // Clear background
        clear_background(Color::from_rgba(30, 30, 35, 255));

        // Tab bar rect and tabs (drawn last so it's on top of any overflow)
        let tab_bar_rect = Rect::new(0.0, 0.0, screen_w, tab_layout::BAR_HEIGHT);
        let tabs = [
            TabEntry::new(icon::HOUSE, "Home"),
            TabEntry::new(icon::GLOBE, "World"),
            TabEntry::new(icon::PLAY, "Game"),
            TabEntry::new(icon::PERSON_STANDING, "Assets"),
            TabEntry::new(icon::MUSIC, "Music"),
            TabEntry::new(icon::GAMEPAD_2, "Input"),
        ];

        // Keyboard shortcuts for tab cycling: Cmd+] (next), Cmd+[ (previous)
        let cmd = is_key_down(KeyCode::LeftSuper) || is_key_down(KeyCode::RightSuper);
        let bracket_left = is_key_pressed(KeyCode::LeftBracket);
        let bracket_right = is_key_pressed(KeyCode::RightBracket);

        if cmd && (bracket_left || bracket_right) {
            let num_tabs = tabs.len();
            let current = app.active_tool_index();
            let next_index = if bracket_left {
                // Previous tab (wrap around)
                if current == 0 { num_tabs - 1 } else { current - 1 }
            } else {
                // Next tab (wrap around)
                (current + 1) % num_tabs
            };
            if let Some(tool) = Tool::from_index(next_index) {
                // Handle special cases for certain tabs
                if tool == Tool::WorldEditor && world_editor_first_open {
                    world_editor_first_open = false;
                    let samples = discover_sample_levels();
                    // Open immediately with samples, user levels load async
                    app.world_editor.level_browser.open_with_levels(samples, Vec::new());
                    #[cfg(target_arch = "wasm32")]
                    {
                        app.world_editor.level_browser.pending_load_list = true;
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        // Start async user level discovery if cloud is enabled
                        if app.storage.has_cloud() {
                            app.world_editor.level_browser.pending_user_list = Some(list_async("assets/userdata/levels".to_string()));
                        } else {
                            // Local storage is fast, use sync
                            app.world_editor.level_browser.user_levels = discover_user_levels(&app.storage);
                        }
                    }
                }
                if tool == Tool::Test {
                    app.game.reset();
                }
                app.set_active_tool(tool);
            }
        }

        // Content area below tab bar
        let content_rect = Rect::new(0.0, tab_layout::BAR_HEIGHT, screen_w, screen_h - tab_layout::BAR_HEIGHT);

        // Sync level from World Editor to ProjectData for live editing
        // This ensures Game tab always sees the current editor state
        app.project.level = app.world_editor.editor_state.level.clone();

        // Draw active tool content
        match app.active_tool {
            Tool::Home => {
                landing::draw_landing(content_rect, &mut app.landing, &ui_ctx);
            }

            Tool::WorldEditor => {
                let ws = &mut app.world_editor;

                // Recalculate portals if geometry changed
                if ws.editor_state.portals_dirty {
                    ws.editor_state.level.recalculate_portals();
                    ws.editor_state.portals_dirty = false;
                }

                // Check for pending import from browser (WASM only)
                #[cfg(target_arch = "wasm32")]
                {
                    /// Maximum import file size (10 MB) to prevent memory exhaustion
                    const MAX_IMPORT_SIZE: usize = 10 * 1024 * 1024;
                    /// Maximum filename length
                    const MAX_FILENAME_LEN: usize = 256;

                    extern "C" {
                        fn b32_check_import() -> i32;
                        fn b32_get_import_data_len() -> usize;
                        fn b32_get_import_filename_len() -> usize;
                        fn b32_copy_import_data(ptr: *mut u8, max_len: usize) -> usize;
                        fn b32_copy_import_filename(ptr: *mut u8, max_len: usize) -> usize;
                        fn b32_clear_import();
                    }

                    let has_import = unsafe { b32_check_import() };

                    if has_import != 0 {
                        let data_len = unsafe { b32_get_import_data_len() };
                        let filename_len = unsafe { b32_get_import_filename_len() };

                        // Security: Check sizes before allocation to prevent memory exhaustion
                        if data_len > MAX_IMPORT_SIZE {
                            unsafe { b32_clear_import(); }
                            ws.editor_state.set_status("Import failed: file too large (max 10MB)", 5.0);
                        } else if filename_len > MAX_FILENAME_LEN {
                            unsafe { b32_clear_import(); }
                            ws.editor_state.set_status("Import failed: filename too long", 5.0);
                        } else {
                            let mut data_buf = vec![0u8; data_len];
                            let mut filename_buf = vec![0u8; filename_len];

                            unsafe {
                                b32_copy_import_data(data_buf.as_mut_ptr(), data_len);
                                b32_copy_import_filename(filename_buf.as_mut_ptr(), filename_len);
                                b32_clear_import();
                            }

                            let data = String::from_utf8_lossy(&data_buf).to_string();
                            let filename = String::from_utf8_lossy(&filename_buf).to_string();

                            // Use load_level_from_str which includes validation
                            match world::load_level_from_str(&data) {
                                Ok(level) => {
                                    ws.editor_layout.apply_config(&level.editor_layout);
                                    ws.editor_state.grid_offset_x = level.editor_layout.grid_offset_x;
                                    ws.editor_state.grid_offset_y = level.editor_layout.grid_offset_y;
                                    ws.editor_state.grid_zoom = level.editor_layout.grid_zoom;
                                    ws.editor_state.orbit_target = rasterizer::Vec3::new(
                                        level.editor_layout.orbit_target_x,
                                        level.editor_layout.orbit_target_y,
                                        level.editor_layout.orbit_target_z,
                                    );
                                    ws.editor_state.orbit_distance = level.editor_layout.orbit_distance;
                                    ws.editor_state.orbit_azimuth = level.editor_layout.orbit_azimuth;
                                    ws.editor_state.orbit_elevation = level.editor_layout.orbit_elevation;
                                    ws.editor_state.sync_camera_from_orbit();
                                    ws.editor_state.load_level(level, PathBuf::from(&filename));
                                    // Reset game state for the new level
                                    app.game.reset_for_new_level();
                                    ws.editor_state.set_status(&format!("Uploaded {}", filename), 3.0);
                                }
                                Err(e) => {
                                    ws.editor_state.set_status(&format!("Upload failed: {}", e), 5.0);
                                }
                            }
                        }
                    }
                }

                // Build textures array from texture packs + user textures
                let mut editor_textures: Vec<Texture> = ws.editor_state.texture_packs
                    .iter()
                    .flat_map(|pack| &pack.textures)
                    .cloned()
                    .collect();

                // Append user textures (they'll be indexed after pack textures)
                // These are updated in real-time when editing, so the 3D view shows live changes
                for name in ws.editor_state.user_textures.names() {
                    if let Some(user_tex) = ws.editor_state.user_textures.get(name) {
                        editor_textures.push(user_tex.to_raster_texture());
                    }
                }

                // Draw editor UI
                let action = draw_editor(
                    &mut ui_ctx,
                    &mut ws.editor_layout,
                    &mut ws.editor_state,
                    &editor_textures,
                    &mut fb,
                    content_rect,
                    app.icon_font.as_ref(),
                    &app.input,
                    &app.storage,
                );

                // Handle editor actions (including opening level browser)
                handle_editor_action(action, &mut app);

                // Reborrow after handle_editor_action
                let ws = &mut app.world_editor;

                // Draw level browser overlay if open
                if ws.level_browser.open {
                    // End modal blocking so the browser itself can receive input
                    ui_ctx.end_modal(real_mouse);

                    let browser_action = draw_level_browser(
                        &mut ui_ctx,
                        &mut ws.level_browser,
                        &app.storage,
                        app.icon_font.as_ref(),
                        &ws.editor_state.texture_packs,
                        &ws.editor_state.asset_library,
                        &ws.editor_state.user_textures,
                    );

                    match browser_action {
                        BrowserAction::SelectPreview(category, index) => {
                            // Get level from the appropriate list
                            let level_info = match category {
                                LevelCategory::Sample => ws.level_browser.samples.get(index),
                                LevelCategory::User => ws.level_browser.user_levels.get(index),
                            };
                            if let Some(info) = level_info {
                                let path = info.path.clone();
                                #[cfg(not(target_arch = "wasm32"))]
                                {
                                    let path_str = path.to_string_lossy().to_string();
                                    // Check if this is a cloud path (userdata + authenticated)
                                    if Storage::is_userdata_path(&path_str) && app.storage.has_cloud() {
                                        // Clear existing preview so loading indicator shows
                                        ws.level_browser.preview_level = None;
                                        ws.level_browser.preview_stats = None;
                                        // Async load for cloud paths
                                        ws.level_browser.pending_preview_load = Some(load_async(path));
                                        ws.editor_state.set_status("Loading preview...", 2.0);
                                    } else {
                                        // Sync load for local paths
                                        match load_level_with_storage(&path_str, &app.storage) {
                                            Ok(level) => {
                                                println!("Loaded level with {} rooms", level.rooms.len());
                                                ws.level_browser.set_preview(level);
                                            }
                                            Err(e) => {
                                                eprintln!("Failed to load level {}: {}", path.display(), e);
                                                ws.editor_state.set_status(&format!("Failed to load: {}", e), 3.0);
                                            }
                                        }
                                    }
                                }
                                #[cfg(target_arch = "wasm32")]
                                {
                                    // Clear existing preview so loading indicator shows
                                    ws.level_browser.preview_level = None;
                                    ws.level_browser.preview_stats = None;
                                    // WASM: set pending path for async load (handled after drawing)
                                    ws.level_browser.pending_load_path = Some(path);
                                }
                            }
                        }
                        BrowserAction::OpenLevel => {
                            // Load the selected level, preserving texture packs and other state
                            if let Some(level) = ws.level_browser.preview_level.take() {
                                let (name, path) = ws.level_browser.selected_level()
                                    .map(|e| (e.name.clone(), e.path.clone()))
                                    .unwrap_or_else(|| ("level".to_string(), PathBuf::from("assets/userdata/levels/untitled.ron")));
                                ws.editor_layout.apply_config(&level.editor_layout);
                                ws.editor_state.grid_offset_x = level.editor_layout.grid_offset_x;
                                ws.editor_state.grid_offset_y = level.editor_layout.grid_offset_y;
                                ws.editor_state.grid_zoom = level.editor_layout.grid_zoom;
                                ws.editor_state.orbit_target = rasterizer::Vec3::new(
                                    level.editor_layout.orbit_target_x,
                                    level.editor_layout.orbit_target_y,
                                    level.editor_layout.orbit_target_z,
                                );
                                ws.editor_state.orbit_distance = level.editor_layout.orbit_distance;
                                ws.editor_state.orbit_azimuth = level.editor_layout.orbit_azimuth;
                                ws.editor_state.orbit_elevation = level.editor_layout.orbit_elevation;
                                ws.editor_state.sync_camera_from_orbit();
                                // Use load_level to preserve texture packs (important for WASM)
                                ws.editor_state.load_level(level, path);
                                // Reset game state for the new level
                                app.game.reset_for_new_level();
                                ws.editor_state.set_status(&format!("Opened: {}", name), 3.0);
                                ws.level_browser.close();
                            }
                        }
                        BrowserAction::OpenCopy => {
                            // Copy sample level to user levels and open it
                            if let Some(level) = ws.level_browser.preview_level.take() {
                                let name = ws.level_browser.selected_level()
                                    .map(|e| e.name.clone())
                                    .unwrap_or_else(|| "copy".to_string());
                                // Generate a new path in userdata
                                let new_path = PathBuf::from(format!("assets/userdata/levels/{}-copy.ron", name));
                                ws.editor_layout.apply_config(&level.editor_layout);
                                ws.editor_state.grid_offset_x = level.editor_layout.grid_offset_x;
                                ws.editor_state.grid_offset_y = level.editor_layout.grid_offset_y;
                                ws.editor_state.grid_zoom = level.editor_layout.grid_zoom;
                                ws.editor_state.orbit_target = rasterizer::Vec3::new(
                                    level.editor_layout.orbit_target_x,
                                    level.editor_layout.orbit_target_y,
                                    level.editor_layout.orbit_target_z,
                                );
                                ws.editor_state.orbit_distance = level.editor_layout.orbit_distance;
                                ws.editor_state.orbit_azimuth = level.editor_layout.orbit_azimuth;
                                ws.editor_state.orbit_elevation = level.editor_layout.orbit_elevation;
                                ws.editor_state.sync_camera_from_orbit();
                                ws.editor_state.load_level(level, new_path.clone());
                                // Mark as unsaved (no current file) so user must save
                                ws.editor_state.current_file = None;
                                app.game.reset_for_new_level();
                                ws.editor_state.set_status(&format!("Copied: {} (save to keep)", name), 3.0);
                                ws.level_browser.close();
                            }
                        }
                        BrowserAction::DeleteLevel => {
                            // Delete user level (only enabled for user levels)
                            if let Some(info) = ws.level_browser.selected_level() {
                                let path_str = info.path.to_string_lossy().to_string();
                                let name = info.name.clone();
                                match app.storage.delete_sync(&path_str) {
                                    Ok(()) => {
                                        ws.editor_state.set_status(&format!("Deleted: {}", name), 3.0);
                                        // Clear selection
                                        ws.level_browser.selected_category = None;
                                        ws.level_browser.selected_index = None;
                                        ws.level_browser.preview_level = None;
                                        ws.level_browser.preview_stats = None;
                                        // Refresh user levels list (async if cloud)
                                        #[cfg(not(target_arch = "wasm32"))]
                                        {
                                            if app.storage.has_cloud() {
                                                ws.level_browser.user_levels.clear();
                                                ws.level_browser.pending_user_list = Some(list_async("assets/userdata/levels".to_string()));
                                            } else {
                                                ws.level_browser.user_levels = discover_user_levels(&app.storage);
                                            }
                                        }
                                        #[cfg(target_arch = "wasm32")]
                                        {
                                            ws.level_browser.user_levels = discover_user_levels(&app.storage);
                                        }
                                    }
                                    Err(e) => {
                                        ws.editor_state.set_status(&format!("Delete failed: {}", e), 3.0);
                                    }
                                }
                            }
                        }
                        BrowserAction::RenameLevel => {
                            // Rename level file (user or sample)
                            if let Some(info) = ws.level_browser.selected_level() {
                                if let Some(ref input_state) = ws.level_browser.rename_dialog {
                                    let new_name = input_state.text.trim().to_string();
                                    let old_path = info.path.clone();
                                    let old_name = info.name.clone();
                                    let is_sample = info.category == LevelCategory::Sample;

                                    if new_name.is_empty() {
                                        ws.editor_state.set_status("Name cannot be empty", 3.0);
                                    } else if new_name.contains('/') || new_name.contains('\\') || new_name.contains(':') {
                                        ws.editor_state.set_status("Name contains invalid characters", 3.0);
                                    } else if new_name == old_name {
                                        // No change
                                    } else {
                                        let new_path = old_path.with_file_name(format!("{}.ron", new_name));
                                        #[cfg(not(target_arch = "wasm32"))]
                                        {
                                            if new_path.exists() {
                                                ws.editor_state.set_status(&format!("'{}' already exists", new_name), 3.0);
                                            } else if !is_sample && app.storage.has_cloud() {
                                                // Cloud user level: read → write new → delete old
                                                let old_str = old_path.to_string_lossy().to_string();
                                                let new_str = new_path.to_string_lossy().to_string();
                                                match app.storage.read_sync(&old_str) {
                                                    Ok(bytes) => {
                                                        if let Err(e) = app.storage.write_sync(&new_str, &bytes) {
                                                            ws.editor_state.set_status(&format!("Rename failed: {}", e), 3.0);
                                                        } else {
                                                            let _ = app.storage.delete_sync(&old_str);
                                                            if ws.editor_state.current_file.as_ref() == Some(&old_path) {
                                                                ws.editor_state.current_file = Some(new_path);
                                                            }
                                                            ws.editor_state.set_status(&format!("Renamed to '{}'", new_name), 2.0);
                                                            ws.level_browser.selected_category = None;
                                                            ws.level_browser.selected_index = None;
                                                            ws.level_browser.preview_level = None;
                                                            ws.level_browser.preview_stats = None;
                                                            ws.level_browser.user_levels.clear();
                                                            ws.level_browser.pending_user_list = Some(list_async("assets/userdata/levels".to_string()));
                                                        }
                                                    }
                                                    Err(e) => {
                                                        ws.editor_state.set_status(&format!("Rename failed: {}", e), 3.0);
                                                    }
                                                }
                                            } else {
                                                // Local rename (user levels without cloud, or sample levels)
                                                match std::fs::rename(&old_path, &new_path) {
                                                    Ok(()) => {
                                                        if ws.editor_state.current_file.as_ref() == Some(&old_path) {
                                                            ws.editor_state.current_file = Some(new_path);
                                                        }
                                                        ws.editor_state.set_status(&format!("Renamed to '{}'", new_name), 2.0);
                                                        ws.level_browser.selected_category = None;
                                                        ws.level_browser.selected_index = None;
                                                        ws.level_browser.preview_level = None;
                                                        ws.level_browser.preview_stats = None;
                                                        if is_sample {
                                                            ws.level_browser.samples = discover_sample_levels();
                                                        } else {
                                                            ws.level_browser.user_levels = discover_user_levels(&app.storage);
                                                        }
                                                    }
                                                    Err(e) => {
                                                        ws.editor_state.set_status(&format!("Rename failed: {}", e), 3.0);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            ws.level_browser.rename_dialog = None;
                        }
                        BrowserAction::NewLevel => {
                            // Start with a fresh empty level, preserving texture packs
                            let new_level = create_empty_level();
                            ws.editor_layout.apply_config(&new_level.editor_layout);
                            ws.editor_state.grid_offset_x = new_level.editor_layout.grid_offset_x;
                            ws.editor_state.grid_offset_y = new_level.editor_layout.grid_offset_y;
                            ws.editor_state.grid_zoom = new_level.editor_layout.grid_zoom;
                            ws.editor_state.orbit_target = rasterizer::Vec3::new(
                                new_level.editor_layout.orbit_target_x,
                                new_level.editor_layout.orbit_target_y,
                                new_level.editor_layout.orbit_target_z,
                            );
                            ws.editor_state.orbit_distance = new_level.editor_layout.orbit_distance;
                            ws.editor_state.orbit_azimuth = new_level.editor_layout.orbit_azimuth;
                            ws.editor_state.orbit_elevation = new_level.editor_layout.orbit_elevation;
                            ws.editor_state.sync_camera_from_orbit();
                            ws.editor_state.load_level(new_level, PathBuf::from("assets/userdata/levels/untitled.ron"));
                            ws.editor_state.current_file = None; // New level has no file yet
                            // Reset game state for the new level
                            app.game.reset_for_new_level();
                            ws.editor_state.set_status("New level created", 3.0);
                            ws.level_browser.close();
                        }
                        BrowserAction::Refresh => {
                            // Refresh level lists from storage
                            ws.level_browser.samples = discover_sample_levels();
                            ws.level_browser.selected_category = None;
                            ws.level_browser.selected_index = None;
                            ws.level_browser.preview_level = None;
                            ws.level_browser.preview_stats = None;
                            // Refresh user levels (async if cloud)
                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                if app.storage.has_cloud() {
                                    ws.level_browser.user_levels.clear();
                                    ws.level_browser.pending_user_list = Some(list_async("assets/userdata/levels".to_string()));
                                    ws.editor_state.set_status("Refreshing...", 2.0);
                                } else {
                                    ws.level_browser.user_levels = discover_user_levels(&app.storage);
                                    ws.editor_state.set_status("Level list refreshed", 2.0);
                                }
                            }
                            #[cfg(target_arch = "wasm32")]
                            {
                                // WASM: trigger async reload (handled in main loop)
                                ws.level_browser.pending_load_list = true;
                                ws.editor_state.set_status("Refreshing...", 2.0);
                            }
                        }
                        BrowserAction::Cancel => {
                            ws.level_browser.close();
                        }
                        BrowserAction::None => {}
                    }
                }
            }

            Tool::Test => {
                // Build textures array from World Editor texture packs
                let game_textures: Vec<Texture> = app.world_editor.editor_state.texture_packs
                    .iter()
                    .flat_map(|pack| &pack.textures)
                    .cloned()
                    .collect();

                // Spawn player if playing and no player exists
                if app.game.playing && app.game.player_entity.is_none() {
                    if let Some((room_idx, spawn)) = app.project.level.get_player_start(&app.world_editor.editor_state.asset_library) {
                        if let Some(room) = app.project.level.rooms.get(room_idx) {
                            let pos = spawn.world_position(room);
                            app.game.spawn_player(pos, &app.project.level);
                        }
                    }
                }

                // Run game simulation
                let delta = get_frame_time();
                app.game.tick(&app.project.level, delta);

                // Render the test viewport (player settings edited in World Editor)
                game::draw_test_viewport(
                    content_rect,
                    &mut app.game,
                    &app.project.level,
                    &game_textures,
                    &mut fb,
                    &app.input,
                    &ui_ctx,
                    &app.world_editor.editor_state.asset_library,
                    &app.world_editor.editor_state.user_textures,
                );
            }

            Tool::Modeler => {
                // Block background input if any browser is open
                let real_mouse_modeler = mouse_state;
                if app.modeler.model_browser.open || app.modeler.obj_importer.open {
                    ui_ctx.begin_modal();
                }

                // Draw modeler UI
                let action = modeler::draw_modeler(
                    &mut ui_ctx,
                    &mut app.modeler.modeler_layout,
                    &mut app.modeler.modeler_state,
                    &mut fb,
                    content_rect,
                    app.icon_font.as_ref(),
                    &app.storage,
                );

                // Handle modeler actions
                // Intercept Save and BrowseModels for cloud storage support
                if matches!(action, ModelerAction::Save) {
                    handle_modeler_save_action(&mut app);
                } else if matches!(action, ModelerAction::BrowseModels) {
                    // Handle inline with full app access (like World Editor does)
                    let ms = &mut app.modeler;
                    let samples = modeler::discover_sample_assets();

                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        if app.storage.has_cloud() {
                            ms.model_browser.open_with_assets(samples, Vec::new());
                            ms.model_browser.pending_user_list = Some(list_async("assets/userdata/assets".to_string()));
                        } else {
                            let users = modeler::discover_user_assets();
                            ms.model_browser.open_with_assets(samples, users);
                        }
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        ms.model_browser.open_with_assets(samples, Vec::new());
                        ms.model_browser.pending_load_list = true;
                    }
                    ms.modeler_state.set_status("Browse assets", 2.0);
                } else {
                    let ms = &mut app.modeler;
                    handle_modeler_action(action, &mut ms.modeler_state, &mut ms.model_browser, &mut ms.obj_importer);
                }
                let ms = &mut app.modeler;

                // Draw model browser overlay if open
                if ms.model_browser.open {
                    ui_ctx.end_modal(real_mouse_modeler);

                    let browser_action = draw_model_browser(
                        &mut ui_ctx,
                        &mut ms.model_browser,
                        &app.storage,
                        app.icon_font.as_ref(),
                        &ms.modeler_state.user_textures,
                    );

                    match browser_action {
                        ModelBrowserAction::SelectPreview(category, index) => {
                            // Get path based on category
                            let asset_info = match category {
                                modeler::AssetCategory::Sample => ms.model_browser.samples.get(index),
                                modeler::AssetCategory::User => ms.model_browser.user_assets.get(index),
                            };
                            if let Some(asset_info) = asset_info {
                                let path = asset_info.path.clone();
                                #[cfg(not(target_arch = "wasm32"))]
                                {
                                    match asset::Asset::load(&path) {
                                        Ok(asset) => {
                                            ms.model_browser.set_preview(asset, &ms.modeler_state.user_textures);
                                        }
                                        Err(e) => {
                                            eprintln!("Failed to load asset: {}", e);
                                            ms.modeler_state.set_status(&format!("Failed to load: {}", e), 3.0);
                                        }
                                    }
                                }
                                #[cfg(target_arch = "wasm32")]
                                {
                                    ms.model_browser.pending_load_path = Some(path);
                                }
                            }
                        }
                        ModelBrowserAction::OpenAsset => {
                            if let Some(asset) = ms.model_browser.preview_asset.take() {
                                let path = ms.model_browser.selected_asset()
                                    .map(|a| a.path.clone())
                                    .unwrap_or_else(|| PathBuf::from("assets/userdata/assets/untitled.ron"));
                                // Set the asset directly in the modeler
                                ms.modeler_state.asset = asset;
                                ms.modeler_state.selected_object = if ms.modeler_state.objects().is_empty() { None } else { Some(0) };
                                // Auto-select first mesh component so it's visible
                                ms.modeler_state.selected_component = ms.modeler_state.asset.components.iter()
                                    .position(|c| c.is_mesh());
                                // Resolve ID-based texture refs using the texture library
                                ms.modeler_state.resolve_all_texture_refs();
                                ms.modeler_state.current_file = Some(path.clone());
                                ms.modeler_state.dirty = false;
                                ms.modeler_state.selection = modeler::ModelerSelection::None;
                                ms.modeler_state.set_status(&format!("Opened: {}", path.display()), 3.0);
                                ms.model_browser.close();
                            }
                        }
                        ModelBrowserAction::OpenCopy => {
                            // Copy sample asset to user asset
                            if let Some(mut asset) = ms.model_browser.preview_asset.take() {
                                // Generate new name with _copy suffix and save to user directory
                                let new_name = format!("{}_copy", asset.name);
                                asset.name = new_name.clone();
                                asset.id = asset::generate_asset_id();
                                let path = PathBuf::from(format!("{}/{}.ron", asset::USER_ASSETS_DIR, new_name));

                                ms.modeler_state.asset = asset;
                                ms.modeler_state.selected_object = if ms.modeler_state.objects().is_empty() { None } else { Some(0) };
                                // Auto-select first mesh component so it's visible
                                ms.modeler_state.selected_component = ms.modeler_state.asset.components.iter()
                                    .position(|c| c.is_mesh());
                                ms.modeler_state.resolve_all_texture_refs();
                                ms.modeler_state.current_file = Some(path.clone());
                                ms.modeler_state.dirty = true; // Mark as dirty so it gets saved
                                ms.modeler_state.selection = modeler::ModelerSelection::None;
                                ms.modeler_state.set_status(&format!("Copied as: {}", new_name), 3.0);
                                ms.model_browser.close();
                            }
                        }
                        ModelBrowserAction::DeleteAsset => {
                            // Delete user asset
                            if let Some(asset_info) = ms.model_browser.selected_asset() {
                                let path = asset_info.path.clone();
                                #[cfg(not(target_arch = "wasm32"))]
                                {
                                    if let Err(e) = std::fs::remove_file(&path) {
                                        eprintln!("Failed to delete asset: {}", e);
                                        ms.modeler_state.set_status(&format!("Failed to delete: {}", e), 3.0);
                                    } else {
                                        ms.modeler_state.set_status("Asset deleted", 2.0);
                                        // Refresh the browser
                                        ms.model_browser.user_assets = modeler::discover_user_assets();
                                        ms.model_browser.preview_asset = None;
                                        ms.model_browser.selected_category = None;
                                        ms.model_browser.selected_index = None;
                                    }
                                }
                            }
                        }
                        ModelBrowserAction::RenameAsset => {
                            // Rename user asset file and update internal name
                            if let Some(asset_info) = ms.model_browser.selected_asset() {
                                if let Some(ref input_state) = ms.model_browser.rename_dialog {
                                    let new_name = input_state.text.trim().to_string();
                                    let old_path = asset_info.path.clone();
                                    let old_name = asset_info.name.clone();

                                    if new_name.is_empty() {
                                        ms.modeler_state.set_status("Name cannot be empty", 3.0);
                                    } else if new_name.contains('/') || new_name.contains('\\') || new_name.contains(':') {
                                        ms.modeler_state.set_status("Name contains invalid characters", 3.0);
                                    } else if new_name == old_name {
                                        // No change
                                    } else {
                                        let new_path = old_path.with_file_name(format!("{}.ron", new_name));
                                        #[cfg(not(target_arch = "wasm32"))]
                                        {
                                            if new_path.exists() {
                                                ms.modeler_state.set_status(&format!("'{}' already exists", new_name), 3.0);
                                            } else {
                                                // Read, update name, write new, delete old
                                                match std::fs::read(&old_path) {
                                                    Ok(bytes) => {
                                                        match asset::Asset::load_from_bytes(&bytes) {
                                                            Ok(mut asset) => {
                                                                asset.name = new_name.clone();
                                                                match asset.to_bytes() {
                                                                    Ok(new_bytes) => {
                                                                        if let Err(e) = std::fs::write(&new_path, &new_bytes) {
                                                                            ms.modeler_state.set_status(&format!("Rename failed: {}", e), 3.0);
                                                                        } else {
                                                                            let _ = std::fs::remove_file(&old_path);
                                                                            // Update current_file if this asset is open
                                                                            if ms.modeler_state.current_file.as_ref() == Some(&old_path) {
                                                                                ms.modeler_state.current_file = Some(new_path);
                                                                                ms.modeler_state.asset.name = new_name.clone();
                                                                            }
                                                                            ms.modeler_state.set_status(&format!("Renamed to '{}'", new_name), 2.0);
                                                                            ms.model_browser.user_assets = modeler::discover_user_assets();
                                                                            ms.model_browser.preview_asset = None;
                                                                            ms.model_browser.selected_category = None;
                                                                            ms.model_browser.selected_index = None;
                                                                        }
                                                                    }
                                                                    Err(e) => {
                                                                        ms.modeler_state.set_status(&format!("Rename failed: {}", e), 3.0);
                                                                    }
                                                                }
                                                            }
                                                            Err(e) => {
                                                                ms.modeler_state.set_status(&format!("Rename failed: {}", e), 3.0);
                                                            }
                                                        }
                                                    }
                                                    Err(e) => {
                                                        ms.modeler_state.set_status(&format!("Rename failed: {}", e), 3.0);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            ms.model_browser.rename_dialog = None;
                        }
                        ModelBrowserAction::Refresh => {
                            // Refresh both sample and user asset lists
                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                ms.model_browser.samples = modeler::discover_sample_assets();
                                // User assets: check if cloud storage is available
                                if app.storage.has_cloud() {
                                    ms.model_browser.user_assets.clear();
                                    ms.model_browser.pending_user_list = Some(list_async("assets/userdata/assets".to_string()));
                                    ms.modeler_state.set_status("Refreshing...", 2.0);
                                } else {
                                    ms.model_browser.user_assets = modeler::discover_user_assets();
                                    ms.modeler_state.set_status("Asset list refreshed", 2.0);
                                }
                            }
                            #[cfg(target_arch = "wasm32")]
                            {
                                // WASM: trigger async reload
                                ms.model_browser.pending_load_list = true;
                                ms.modeler_state.set_status("Refreshing...", 2.0);
                            }
                            ms.model_browser.preview_asset = None;
                            ms.model_browser.selected_category = None;
                            ms.model_browser.selected_index = None;
                        }
                        ModelBrowserAction::NewAsset => {
                            ms.modeler_state.new_mesh();
                            ms.model_browser.close();
                        }
                        ModelBrowserAction::Cancel => {
                            ms.model_browser.close();
                        }
                        ModelBrowserAction::None => {}
                    }
                }

                // Draw mesh browser overlay if open
                if ms.obj_importer.open {
                    ui_ctx.end_modal(real_mouse_modeler);

                    let browser_action = draw_obj_importer(
                        &mut ui_ctx,
                        &mut ms.obj_importer,
                        app.icon_font.as_ref(),
                        &mut fb,
                    );

                    match browser_action {
                        ObjImportAction::SelectPreview(idx) => {
                            // Load preview for selected mesh
                            let index = idx;

                            if let Some(mesh_info) = ms.obj_importer.meshes.get(index) {
                                let path = mesh_info.path.clone();
                                let texture_path = mesh_info.texture_path.clone();
                                let additional_textures = mesh_info.additional_textures.clone();
                                let scale = ms.obj_importer.import_scale;
                                let flip = ms.obj_importer.flip_normals;

                                #[cfg(not(target_arch = "wasm32"))]
                                {
                                    match ObjImporter::load_from_file(&path) {
                                        Ok(mut mesh) => {
                                            // Apply scale to preview
                                            for vertex in &mut mesh.vertices {
                                                vertex.pos = vertex.pos * scale;
                                            }

                                            // Compute normals for shading in preview
                                            ObjImporter::compute_face_normals(&mut mesh);

                                            // Flip normals if requested
                                            if flip {
                                                // Flip vertex normals
                                                for vertex in &mut mesh.vertices {
                                                    vertex.normal = vertex.normal * -1.0;
                                                }
                                                // Swap v1 and v2 to flip winding order
                                                for face in &mut mesh.faces {
                                                    face.vertices.reverse();
                                                }
                                            }

                                            ms.obj_importer.set_preview(mesh);

                                            // Load all textures (primary + additional)
                                            let mut textures = Vec::new();

                                            // Load primary texture first (auto-quantize to indexed)
                                            if let Some(tex_path) = texture_path {
                                                match ObjImporter::load_png_to_indexed(&tex_path, "preview") {
                                                    Ok((indexed, clut, color_count)) => textures.push(TextureImportResult { indexed, clut, color_count }),
                                                    Err(e) => eprintln!("Failed to load primary texture: {}", e),
                                                }
                                            }

                                            // Load additional textures (_tex0.png, _tex1.png, etc.)
                                            for tex_path in additional_textures {
                                                match ObjImporter::load_png_to_indexed(&tex_path, "preview") {
                                                    Ok((indexed, clut, color_count)) => textures.push(TextureImportResult { indexed, clut, color_count }),
                                                    Err(e) => eprintln!("Failed to load texture {:?}: {}", tex_path, e),
                                                }
                                            }

                                            ms.obj_importer.set_preview_textures(textures);
                                        }
                                        Err(e) => {
                                            eprintln!("Failed to load mesh: {}", e);
                                            ms.modeler_state.set_status(&format!("Failed to load: {}", e), 3.0);
                                        }
                                    }
                                }
                                #[cfg(target_arch = "wasm32")]
                                {
                                    ms.obj_importer.pending_load_path = Some(path);
                                }
                            }
                        }
                        ObjImportAction::ReloadPreview => {
                            // Reload with current scale/flip settings
                            if let Some(index) = ms.obj_importer.selected_index {
                                if let Some(mesh_info) = ms.obj_importer.meshes.get(index) {
                                    let path = mesh_info.path.clone();

                                    #[cfg(not(target_arch = "wasm32"))]
                                    {
                                        let scale = ms.obj_importer.import_scale;
                                        let flip_normals = ms.obj_importer.flip_normals;
                                        let flip_h = ms.obj_importer.flip_horizontal;
                                        let flip_v = ms.obj_importer.flip_vertical;

                                        if let Ok(mut mesh) = ObjImporter::load_from_file(&path) {
                                            // Apply scale to preview
                                            for vertex in &mut mesh.vertices {
                                                vertex.pos = vertex.pos * scale;
                                            }

                                            // Compute normals for shading in preview
                                            ObjImporter::compute_face_normals(&mut mesh);

                                            // Flip normals if requested
                                            if flip_normals {
                                                for vertex in &mut mesh.vertices {
                                                    vertex.normal = vertex.normal * -1.0;
                                                }
                                                for face in &mut mesh.faces {
                                                    face.vertices.reverse();
                                                }
                                            }

                                            // Flip horizontal (mirror X)
                                            if flip_h {
                                                modeler::apply_mesh_flip_horizontal(&mut mesh);
                                            }

                                            // Flip vertical (mirror Y)
                                            if flip_v {
                                                modeler::apply_mesh_flip_vertical(&mut mesh);
                                            }

                                            // Use update_preview to preserve camera angles
                                            ms.obj_importer.update_preview(mesh);
                                        }
                                    }
                                    #[cfg(target_arch = "wasm32")]
                                    {
                                        // Queue async reload with new settings
                                        ms.obj_importer.pending_load_path = Some(path);
                                    }
                                }
                            }
                        }
                        ObjImportAction::OpenMesh => {
                            let path = ms.obj_importer.selected_mesh()
                                .map(|m| m.path.clone())
                                .unwrap_or_else(|| PathBuf::from("assets/samples/meshes/untitled.obj"));
                            let scale = ms.obj_importer.import_scale;
                            let flip_normals = ms.obj_importer.flip_normals;
                            let flip_h = ms.obj_importer.flip_horizontal;
                            let flip_v = ms.obj_importer.flip_vertical;
                            let clut_depth_override = ms.obj_importer.clut_depth_override;

                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                // Import with texture - either auto-detect or forced CLUT depth
                                let import_result = if let Some(depth) = clut_depth_override {
                                    // Force specific CLUT depth
                                    ObjImporter::import_with_texture(&path, scale, Some(depth))
                                } else {
                                    // Auto-detect optimal CLUT depth
                                    ObjImporter::import_with_auto_quantize(&path, scale)
                                };
                                match import_result {
                                    Ok(mut result) => {
                                        // Flip normals if requested
                                        if flip_normals {
                                            // Flip vertex normals
                                            for vertex in &mut result.mesh.vertices {
                                                vertex.normal = vertex.normal * -1.0;
                                            }
                                            // Swap v1 and v2 to flip winding order
                                            for face in &mut result.mesh.faces {
                                                face.vertices.reverse();
                                            }
                                        }

                                        // Flip horizontal (mirror X)
                                        if flip_h {
                                            modeler::apply_mesh_flip_horizontal(&mut result.mesh);
                                        }

                                        // Flip vertical (mirror Y)
                                        if flip_v {
                                            modeler::apply_mesh_flip_vertical(&mut result.mesh);
                                        }

                                        // Set the editable mesh directly in project (single source of truth)
                                        if let Some(mesh) = ms.modeler_state.mesh_mut() {
                                            *mesh = result.mesh;
                                        }
                                        // Don't set current_file to OBJ path - this is an IMPORT, not opening a project
                                        // User must "Save As" to create a .ron project file
                                        ms.modeler_state.current_file = None;
                                        ms.modeler_state.dirty = true;  // Needs saving
                                        ms.modeler_state.selection = modeler::ModelerSelection::None;

                                        // Handle texture import
                                        let mut texture_status = String::new();
                                        if let Some(tex_result) = result.texture {
                                            let TextureImportResult { mut indexed, clut, color_count } = tex_result;
                                            // Clear existing CLUTs and add only the imported one
                                            ms.modeler_state.clut_pool.clear();
                                            let clut_id = ms.modeler_state.clut_pool.add_clut(clut);
                                            indexed.default_clut = clut_id;
                                            let depth_label = indexed.depth.short_label();
                                            // Set the indexed atlas on the selected object
                                            if let Some(atlas) = ms.modeler_state.atlas_mut() {
                                                *atlas = indexed;
                                            }
                                            ms.modeler_state.selected_clut = Some(clut_id);
                                            // Show "(forced)" if user manually selected the depth
                                            let forced = if clut_depth_override.is_some() { " forced" } else { "" };
                                            texture_status = format!(" + CLUT {}{} ({} colors)", depth_label, forced, color_count);
                                        }

                                        // Reset camera to fit the scaled mesh
                                        ms.modeler_state.orbit_target = crate::rasterizer::Vec3::new(0.0, 50.0, 0.0);
                                        ms.modeler_state.orbit_distance = scale * 3.0;
                                        ms.modeler_state.sync_camera_from_orbit();

                                        // Build flip status string
                                        let mut flips = Vec::new();
                                        if flip_normals { flips.push("N"); }
                                        if flip_h { flips.push("H"); }
                                        if flip_v { flips.push("V"); }
                                        let flip_status = if flips.is_empty() {
                                            String::new()
                                        } else {
                                            format!(" (flip: {})", flips.join("+"))
                                        };
                                        ms.modeler_state.set_status(
                                            &format!("Imported: {} ({}x){}{}", path.display(), scale, texture_status, flip_status),
                                            3.0
                                        );
                                    }
                                    Err(e) => {
                                        ms.modeler_state.set_status(&format!("Import failed: {}", e), 3.0);
                                    }
                                }
                            }

                            #[cfg(target_arch = "wasm32")]
                            {
                                // WASM fallback - just use preview mesh (already has scale/flip applied from preview)
                                if let Some(imported_mesh) = ms.obj_importer.preview_mesh.take() {
                                    // Set mesh directly in project (single source of truth)
                                    if let Some(mesh) = ms.modeler_state.mesh_mut() {
                                        *mesh = imported_mesh;
                                    }
                                    // Don't set current_file to OBJ path - this is an IMPORT
                                    ms.modeler_state.current_file = None;
                                    ms.modeler_state.dirty = true;  // Needs saving
                                    ms.modeler_state.selection = modeler::ModelerSelection::None;
                                    ms.modeler_state.orbit_target = crate::rasterizer::Vec3::new(0.0, 50.0, 0.0);
                                    ms.modeler_state.orbit_distance = scale * 3.0;
                                    ms.modeler_state.sync_camera_from_orbit();
                                    ms.modeler_state.set_status(&format!("Imported mesh: {} ({}x)", path.display(), scale), 3.0);
                                }
                            }

                            ms.obj_importer.close();
                        }
                        ObjImportAction::Cancel => {
                            ms.obj_importer.close();
                        }
                        ObjImportAction::None => {}
                    }
                }
            }

            Tool::Tracker => {
                // Update playback timing
                let delta = get_frame_time() as f64;
                app.tracker.update_playback(delta);

                // Block background input if song browser is open
                let real_mouse_tracker = mouse_state;
                if app.tracker.song_browser.open {
                    ui_ctx.begin_modal();
                }

                // Draw tracker UI
                tracker::draw_tracker(&mut ui_ctx, content_rect, &mut app.tracker, app.icon_font.as_ref(), &app.storage);

                // Draw song browser overlay if open
                if app.tracker.song_browser.open {
                    // End modal blocking so the browser itself can receive input
                    ui_ctx.end_modal(real_mouse_tracker);

                    let _browser_action = tracker::draw_song_browser(
                        &mut ui_ctx,
                        &mut app.tracker,
                        app.icon_font.as_ref(),
                        &app.storage,
                    );
                    // Actions are handled internally by draw_song_browser
                }
            }

            Tool::InputTest => {
                // Draw controller debug view
                input::draw_controller_debug(content_rect, &mut app.input);
            }
        }

        // Draw tooltips last (on top of everything)
        ui_ctx.draw_tooltip();

        // Handle pending async level load (WASM) - after all drawing is complete
        #[cfg(target_arch = "wasm32")]
        if let Tool::WorldEditor = app.active_tool {
            let ws = &mut app.world_editor;
            // Load sample list from manifest if pending
            if ws.level_browser.pending_load_list {
                ws.level_browser.pending_load_list = false;
                use editor::load_sample_list;
                let samples = load_sample_list().await;
                ws.level_browser.samples = samples;
                // Start async user level discovery if authenticated
                if crate::auth::is_authenticated() {
                    ws.level_browser.pending_user_list = Some(list_async("assets/userdata/levels".to_string()));
                }
            }
            // Load individual level preview if pending
            if let Some(path) = ws.level_browser.pending_load_path.take() {
                let path_str = path.to_string_lossy().to_string();
                // Check if this is a user level (cloud storage) or sample (web server)
                if Storage::is_userdata_path(&path_str) && crate::auth::is_authenticated() {
                    // User level: load from cloud storage
                    ws.level_browser.pending_preview_load = Some(load_async(path));
                } else {
                    // Sample level: load from web server
                    use editor::load_sample_level;
                    if let Some(level) = load_sample_level(&path).await {
                        ws.level_browser.set_preview(level);
                    } else {
                        ws.editor_state.set_status("Failed to load level preview", 3.0);
                    }
                }
            }
            // Load asset browser list if pending (for world editor's asset picker)
            if ws.editor_state.asset_browser.pending_load_list {
                ws.editor_state.asset_browser.pending_load_list = false;
                use modeler::load_sample_asset_list;
                ws.editor_state.asset_browser.samples = load_sample_asset_list().await;
                // Start async user asset discovery if authenticated (cloud storage)
                if crate::auth::is_authenticated() {
                    ws.editor_state.asset_browser.pending_user_list = Some(list_async("assets/userdata/assets".to_string()));
                }
            }
        }

        // Handle pending async model/mesh load (WASM) - after all drawing is complete
        #[cfg(target_arch = "wasm32")]
        if let Tool::Modeler = app.active_tool {
            let ms = &mut app.modeler;
            // Load model list from manifest if pending
            if ms.model_browser.pending_load_list {
                ms.model_browser.pending_load_list = false;
                use modeler::load_sample_asset_list;
                ms.model_browser.samples = load_sample_asset_list().await;
                // Start async user asset discovery if authenticated (cloud storage)
                if crate::auth::is_authenticated() {
                    ms.model_browser.pending_user_list = Some(list_async("assets/userdata/assets".to_string()));
                }
            }
            // Load individual model preview if pending
            if let Some(path) = ms.model_browser.pending_load_path.take() {
                let path_str = path.to_string_lossy().to_string();
                // Check if this is a user asset (cloud storage) or sample (web server)
                if Storage::is_userdata_path(&path_str) && crate::auth::is_authenticated() {
                    // User asset: load from cloud storage
                    ms.model_browser.pending_preview_load = Some(load_async(path));
                } else {
                    // Sample asset: load from web server
                    use modeler::load_model;
                    if let Some(project) = load_model(&path).await {
                        ms.model_browser.set_preview(project, &ms.modeler_state.user_textures);
                    } else {
                        ms.modeler_state.set_status("Failed to load model preview", 3.0);
                    }
                }
            }
            // Load mesh list from manifest if pending
            if ms.obj_importer.pending_load_list {
                ms.obj_importer.pending_load_list = false;
                use modeler::load_mesh_list;
                let meshes = load_mesh_list().await;
                ms.obj_importer.meshes = meshes;
            }
            // Load individual mesh preview if pending
            if let Some(path) = ms.obj_importer.pending_load_path.take() {
                use modeler::load_mesh;
                use modeler::{apply_mesh_flip_horizontal, apply_mesh_flip_vertical};

                if let Some(mut mesh) = load_mesh(&path).await {
                    // Get transform settings from browser
                    let scale = ms.obj_importer.import_scale;
                    let flip_normals = ms.obj_importer.flip_normals;
                    let flip_h = ms.obj_importer.flip_horizontal;
                    let flip_v = ms.obj_importer.flip_vertical;

                    // Apply scale to preview
                    for vertex in &mut mesh.vertices {
                        vertex.pos = vertex.pos * scale;
                    }

                    // Compute normals for shading in preview
                    ObjImporter::compute_face_normals(&mut mesh);

                    // Flip normals if requested
                    if flip_normals {
                        for vertex in &mut mesh.vertices {
                            vertex.normal = vertex.normal * -1.0;
                        }
                        for face in &mut mesh.faces {
                            face.vertices.reverse();
                        }
                    }

                    // Apply horizontal/vertical flips
                    if flip_h {
                        apply_mesh_flip_horizontal(&mut mesh);
                    }
                    if flip_v {
                        apply_mesh_flip_vertical(&mut mesh);
                    }

                    // Update MeshInfo counts (since WASM loads async with initial 0 counts)
                    if let Some(idx) = ms.obj_importer.selected_index {
                        if let Some(info) = ms.obj_importer.meshes.get_mut(idx) {
                            info.vertex_count = mesh.vertices.len();
                            info.face_count = mesh.faces.len();
                        }
                    }

                    ms.obj_importer.set_preview(mesh);
                } else {
                    ms.modeler_state.set_status("Failed to load mesh preview", 3.0);
                }
            }
        }

        // Handle pending async song load (WASM) - after all drawing is complete
        #[cfg(target_arch = "wasm32")]
        if let Tool::Tracker = app.active_tool {
            let ts = &mut app.tracker;
            // Load song list from manifest if pending
            if ts.song_browser.pending_load_list {
                ts.song_browser.pending_load_list = false;
                // Load samples from bundled manifest
                use tracker::load_song_list;
                let (samples, _manifest_user_songs) = load_song_list().await;
                ts.song_browser.samples = samples;
                // User songs: load from cloud if authenticated, otherwise empty
                if crate::auth::is_authenticated() {
                    ts.song_browser.pending_user_list = Some(list_async("assets/userdata/songs".to_string()));
                } else {
                    ts.song_browser.user_songs = Vec::new();
                }
            }
            // Load individual song preview if pending
            if let Some(path) = ts.song_browser.pending_load_path.take() {
                let path_str = path.to_string_lossy().to_string();
                if path_str.contains("userdata") && crate::auth::is_authenticated() {
                    // User song from cloud: load via async API
                    ts.song_browser.pending_preview_load = Some(load_async(path_str.into()));
                } else {
                    // Sample song: load from web server
                    use tracker::load_song_async;
                    if let Some(song) = load_song_async(&path).await {
                        ts.song_browser.set_preview(song);
                    } else {
                        ts.set_status("Failed to load song preview", 3.0);
                    }
                }
            }
            // Load song from sample when "Open" is clicked (not preview, the actual song)
            if let Some(path) = ts.pending_song_load_path.take() {
                use tracker::load_song_async;
                if let Some(song) = load_song_async(&path).await {
                    let name = path.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("song")
                        .to_string();
                    ts.apply_song(song, Some(path));
                    ts.set_status(&format!("Loaded '{}'", name), 2.0);
                } else {
                    ts.set_status("Failed to load song", 3.0);
                }
            }
        }

        // Draw tab bar LAST so it covers any content overflow (e.g., landing page scroll)
        let tab_action = draw_fixed_tabs_with_auth(
            &mut ui_ctx,
            tab_bar_rect,
            &tabs,
            app.active_tool_index(),
            app.icon_font.as_ref(),
            Some(VERSION),
            &mut version_highlighted,
            app.storage.mode(),
            app.storage.can_write(),
            app.auth.authenticated,
        );

        match tab_action {
            TabBarAction::SwitchTab(clicked) => {
                if let Some(tool) = Tool::from_index(clicked) {
                    // Handle special cases for certain tabs
                    if tool == Tool::WorldEditor && world_editor_first_open {
                        world_editor_first_open = false;
                        let samples = discover_sample_levels();
                        // Open immediately with samples, user levels load async
                        app.world_editor.level_browser.open_with_levels(samples, Vec::new());
                        #[cfg(target_arch = "wasm32")]
                        {
                            app.world_editor.level_browser.pending_load_list = true;
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            // Start async user level discovery if cloud is enabled
                            if app.storage.has_cloud() {
                                app.world_editor.level_browser.pending_user_list = Some(list_async("assets/userdata/levels".to_string()));
                            } else {
                                // Local storage is fast, use sync
                                app.world_editor.level_browser.user_levels = discover_user_levels(&app.storage);
                            }
                        }
                    }
                    if tool == Tool::Test {
                        app.game.reset();
                    }
                    // Close all modals when switching tabs to prevent orphaned modal state
                    app.world_editor.level_browser.open = false;
                    app.modeler.model_browser.open = false;
                    app.modeler.obj_importer.open = false;
                    app.tracker.song_browser.open = false;
                    app.set_active_tool(tool);
                }
            }
            TabBarAction::SignIn => auth::sign_in(),
            TabBarAction::SignOut => auth::sign_out(),
            TabBarAction::None => {}
        }

        // FPS limiting (only when in game tab)
        if let Tool::Test = app.active_tool {
            if let Some(target_frame_time) = app.game.fps_limit.frame_time() {
                let elapsed = get_time() - frame_start;
                let remaining = target_frame_time - elapsed;

                if remaining > 0.0 {
                    // Native: use sleep for bulk, then spin-wait for precision
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        let spin_margin = 0.002; // 2ms
                        while get_time() - frame_start + spin_margin < target_frame_time {
                            std::thread::sleep(std::time::Duration::from_millis(1));
                        }
                        // Spin-wait for precise timing
                        while get_time() - frame_start < target_frame_time {
                            std::hint::spin_loop();
                        }
                    }
                    // WASM: just spin-wait (no thread::sleep available)
                    #[cfg(target_arch = "wasm32")]
                    {
                        while get_time() - frame_start < target_frame_time {
                            // Busy wait - browser will handle frame pacing
                        }
                    }
                }
            }
        }

        next_frame().await;
    }
}

/// Poll pending async operations and update state when complete
fn poll_pending_ops(app: &mut AppState) {
    // Poll pending save operation
    if let Some(mut pending) = app.pending_ops.save.take() {
        if pending.op.is_complete() {
            match pending.op.take() {
                Some(Ok(())) => {
                    app.world_editor.editor_state.dirty = false;
                    let mode_label = app.storage.mode().label();
                    app.world_editor.editor_state.set_status(
                        &format!("Saved ({}) {}", mode_label, pending.path.display()),
                        3.0,
                    );
                }
                Some(Err(e)) => {
                    app.world_editor.editor_state.set_status(&format!("Save failed: {}", e), 5.0);
                }
                None => {
                    app.world_editor.editor_state.set_status("Save failed: unknown error", 5.0);
                }
            }
            app.pending_ops.status_message = None;
        } else {
            // Still pending, put it back
            app.pending_ops.save = Some(pending);
        }
    }

    // Poll pending modeler (asset) save operation
    if let Some(mut pending) = app.pending_ops.modeler_save.take() {
        if pending.op.is_complete() {
            match pending.op.take() {
                Some(Ok(())) => {
                    app.modeler.modeler_state.dirty = false;
                    let mode_label = app.storage.mode().label();
                    app.modeler.modeler_state.set_status(
                        &format!("Saved ({}) {}", mode_label, pending.path.display()),
                        3.0,
                    );
                }
                Some(Err(e)) => {
                    app.modeler.modeler_state.set_status(&format!("Save failed: {}", e), 5.0);
                }
                None => {
                    app.modeler.modeler_state.set_status("Save failed: unknown error", 5.0);
                }
            }
            app.pending_ops.status_message = None;
        } else {
            // Still pending, put it back
            app.pending_ops.modeler_save = Some(pending);
        }
    }

    // Poll pending load operation
    if let Some(mut pending) = app.pending_ops.load.take() {
        if pending.op.is_complete() {
            match pending.op.take() {
                Some(Ok(data)) => {
                    // Parse the loaded data
                    match world::parse_level_data(&data) {
                        Ok(level) => {
                            app.world_editor.editor_state.load_level(level, pending.path.clone());
                            let mode_label = app.storage.mode().label();
                            app.world_editor.editor_state.set_status(
                                &format!("Loaded ({}) {}", mode_label, pending.path.display()),
                                3.0,
                            );
                        }
                        Err(e) => {
                            app.world_editor.editor_state.set_status(&format!("Load failed: {}", e), 5.0);
                        }
                    }
                }
                Some(Err(e)) => {
                    app.world_editor.editor_state.set_status(&format!("Load failed: {}", e), 5.0);
                }
                None => {
                    app.world_editor.editor_state.set_status("Load failed: unknown error", 5.0);
                }
            }
            app.pending_ops.status_message = None;
        } else {
            // Still pending, put it back
            app.pending_ops.load = Some(pending);
        }
    }

    // Poll pending browser preview load (for async cloud loads)
    if let Some(mut pending) = app.world_editor.level_browser.pending_preview_load.take() {
        if pending.op.is_complete() {
            match pending.op.take() {
                Some(Ok(data)) => {
                    // Parse the loaded data
                    match world::parse_level_data(&data) {
                        Ok(level) => {
                            app.world_editor.level_browser.set_preview(level);
                            app.world_editor.editor_state.set_status("Preview loaded", 2.0);
                        }
                        Err(e) => {
                            app.world_editor.editor_state.set_status(&format!("Preview failed: {}", e), 3.0);
                        }
                    }
                }
                Some(Err(e)) => {
                    app.world_editor.editor_state.set_status(&format!("Preview failed: {}", e), 3.0);
                }
                None => {
                    app.world_editor.editor_state.set_status("Preview failed: unknown error", 3.0);
                }
            }
        } else {
            // Still pending, put it back
            app.world_editor.level_browser.pending_preview_load = Some(pending);
        }
    }

    // Poll pending modeler asset browser preview load (for async cloud loads)
    if let Some(mut pending) = app.modeler.model_browser.pending_preview_load.take() {
        if pending.op.is_complete() {
            match pending.op.take() {
                Some(Ok(data)) => {
                    // Parse the loaded asset data
                    match asset::Asset::load_from_bytes(&data) {
                        Ok(asset) => {
                            app.modeler.model_browser.set_preview(asset, &app.modeler.modeler_state.user_textures);
                            app.modeler.modeler_state.set_status("Preview loaded", 2.0);
                        }
                        Err(e) => {
                            app.modeler.modeler_state.set_status(&format!("Preview failed: {}", e), 3.0);
                        }
                    }
                }
                Some(Err(e)) => {
                    app.modeler.modeler_state.set_status(&format!("Preview failed: {}", e), 3.0);
                }
                None => {
                    app.modeler.modeler_state.set_status("Preview failed: unknown error", 3.0);
                }
            }
        } else {
            // Still pending, put it back
            app.modeler.model_browser.pending_preview_load = Some(pending);
        }
    }

    // Poll pending world editor asset browser preview load (for async cloud loads)
    if let Some(mut pending) = app.world_editor.editor_state.asset_browser.pending_preview_load.take() {
        if pending.op.is_complete() {
            match pending.op.take() {
                Some(Ok(data)) => {
                    // Parse the loaded asset data
                    match asset::Asset::load_from_bytes(&data) {
                        Ok(asset) => {
                            app.world_editor.editor_state.asset_browser.set_preview(asset, &app.world_editor.editor_state.user_textures);
                            app.world_editor.editor_state.set_status("Preview loaded", 2.0);
                        }
                        Err(e) => {
                            app.world_editor.editor_state.set_status(&format!("Preview failed: {}", e), 3.0);
                        }
                    }
                }
                Some(Err(e)) => {
                    app.world_editor.editor_state.set_status(&format!("Preview failed: {}", e), 3.0);
                }
                None => {
                    app.world_editor.editor_state.set_status("Preview failed: unknown error", 3.0);
                }
            }
        } else {
            // Still pending, put it back
            app.world_editor.editor_state.asset_browser.pending_preview_load = Some(pending);
        }
    }

    // Poll pending browser user level list (for async cloud discovery)
    if let Some(mut pending) = app.world_editor.level_browser.pending_user_list.take() {
        if pending.op.is_complete() {
            const USER_LEVELS_DIR: &str = "assets/userdata/levels";
            match pending.op.take() {
                Some(Ok(files)) => {
                    // Convert file list to LevelInfo
                    // Cloud API returns full paths, local storage returns just filenames
                    let levels: Vec<_> = files
                        .iter()
                        .filter(|f| f.ends_with(".ron"))
                        .map(|f| {
                            let full_path = if f.contains('/') {
                                f.clone()
                            } else {
                                format!("{}/{}", USER_LEVELS_DIR, f)
                            };
                            let name = full_path
                                .rsplit('/')
                                .next()
                                .and_then(|n| n.strip_suffix(".ron"))
                                .unwrap_or(&full_path)
                                .to_string();
                            editor::LevelInfo {
                                name,
                                path: PathBuf::from(full_path),
                                category: editor::LevelCategory::User,
                            }
                        })
                        .collect();
                    app.world_editor.level_browser.user_levels = levels;
                    app.world_editor.editor_state.set_status("Levels loaded", 2.0);
                }
                Some(Err(e)) => {
                    app.world_editor.editor_state.set_status(&format!("Failed to list levels: {}", e), 3.0);
                }
                None => {
                    // No result
                }
            }
        } else {
            // Still pending, put it back
            app.world_editor.level_browser.pending_user_list = Some(pending);
        }
    }

    // Poll pending modeler asset browser user asset list (for async cloud discovery)
    if let Some(mut pending) = app.modeler.model_browser.pending_user_list.take() {
        if pending.op.is_complete() {
            const USER_ASSETS_DIR: &str = "assets/userdata/assets";
            match pending.op.take() {
                Some(Ok(files)) => {
                    // Convert file list to AssetInfo
                    let assets: Vec<_> = files
                        .iter()
                        .filter(|f| f.ends_with(".ron"))
                        .map(|f| {
                            let full_path = if f.contains('/') {
                                f.clone()
                            } else {
                                format!("{}/{}", USER_ASSETS_DIR, f)
                            };
                            let name = full_path
                                .rsplit('/')
                                .next()
                                .and_then(|n| n.strip_suffix(".ron"))
                                .unwrap_or(&full_path)
                                .to_string();
                            modeler::AssetInfo {
                                name,
                                path: PathBuf::from(full_path),
                                category: modeler::AssetCategory::User,
                            }
                        })
                        .collect();
                    app.modeler.model_browser.user_assets = assets;
                    app.modeler.modeler_state.set_status("Assets loaded", 2.0);
                }
                Some(Err(e)) => {
                    app.modeler.modeler_state.set_status(&format!("Failed to list assets: {}", e), 3.0);
                }
                None => {
                    // No result
                }
            }
        } else {
            // Still pending, put it back
            app.modeler.model_browser.pending_user_list = Some(pending);
        }
    }

    // Handle world editor asset browser pending refresh (native only)
    // This is triggered when Refresh is clicked but storage access wasn't available in layout.rs
    if app.world_editor.editor_state.asset_browser.pending_refresh {
        app.world_editor.editor_state.asset_browser.pending_refresh = false;
        if app.storage.has_cloud() {
            app.world_editor.editor_state.asset_browser.user_assets.clear();
            app.world_editor.editor_state.asset_browser.pending_user_list = Some(list_async("assets/userdata/assets".to_string()));
            app.world_editor.editor_state.set_status("Refreshing assets...", 2.0);
        } else {
            app.world_editor.editor_state.asset_browser.user_assets = modeler::discover_user_assets();
            app.world_editor.editor_state.set_status("Asset list refreshed", 2.0);
        }
    }

    // Poll pending world editor asset browser user asset list (for async cloud discovery)
    if let Some(mut pending) = app.world_editor.editor_state.asset_browser.pending_user_list.take() {
        if pending.op.is_complete() {
            const USER_ASSETS_DIR: &str = "assets/userdata/assets";
            match pending.op.take() {
                Some(Ok(files)) => {
                    // Convert file list to AssetInfo
                    let assets: Vec<_> = files
                        .iter()
                        .filter(|f| f.ends_with(".ron"))
                        .map(|f| {
                            let full_path = if f.contains('/') {
                                f.clone()
                            } else {
                                format!("{}/{}", USER_ASSETS_DIR, f)
                            };
                            let name = full_path
                                .rsplit('/')
                                .next()
                                .and_then(|n| n.strip_suffix(".ron"))
                                .unwrap_or(&full_path)
                                .to_string();
                            modeler::AssetInfo {
                                name,
                                path: PathBuf::from(full_path),
                                category: modeler::AssetCategory::User,
                            }
                        })
                        .collect();
                    app.world_editor.editor_state.asset_browser.user_assets = assets;
                    app.world_editor.editor_state.set_status("Assets loaded", 2.0);
                }
                Some(Err(e)) => {
                    app.world_editor.editor_state.set_status(&format!("Failed to list assets: {}", e), 3.0);
                }
                None => {
                    // No result
                }
            }
        } else {
            // Still pending, put it back
            app.world_editor.editor_state.asset_browser.pending_user_list = Some(pending);
        }
    }

    // Handle song browser pending refresh (native only)
    if app.tracker.song_browser.pending_refresh {
        app.tracker.song_browser.pending_refresh = false;
        // Refresh sample songs (always local/sync)
        #[cfg(not(target_arch = "wasm32"))]
        {
            app.tracker.song_browser.samples = tracker::discover_songs_from_dir(
                tracker::SAMPLES_SONGS_DIR,
                tracker::SongCategory::Sample,
            );
        }
        // User songs: check if cloud storage is available
        if app.storage.has_cloud() {
            app.tracker.song_browser.user_songs.clear();
            app.tracker.song_browser.pending_user_list = Some(list_async("assets/userdata/songs".to_string()));
        } else {
            #[cfg(not(target_arch = "wasm32"))]
            {
                app.tracker.song_browser.user_songs = tracker::discover_songs_from_dir(
                    tracker::USER_SONGS_DIR,
                    tracker::SongCategory::User,
                );
            }
            app.tracker.set_status("Song list refreshed", 2.0);
        }
    }

    // Poll pending song browser preview load (for async cloud loads)
    if let Some(mut pending) = app.tracker.song_browser.pending_preview_load.take() {
        if pending.op.is_complete() {
            match pending.op.take() {
                Some(Ok(data)) => {
                    // Parse the loaded song data
                    match parse_song_data(&data) {
                        Ok(song) => {
                            app.tracker.song_browser.set_preview(song);
                            app.tracker.set_status("Preview loaded", 2.0);
                        }
                        Err(e) => {
                            app.tracker.set_status(&format!("Preview failed: {}", e), 3.0);
                        }
                    }
                }
                Some(Err(e)) => {
                    app.tracker.set_status(&format!("Preview failed: {}", e), 3.0);
                }
                None => {
                    app.tracker.set_status("Preview failed: unknown error", 3.0);
                }
            }
        } else {
            // Still pending, put it back
            app.tracker.song_browser.pending_preview_load = Some(pending);
        }
    }

    // Poll pending song browser user songs list (for async cloud discovery)
    if let Some(mut pending) = app.tracker.song_browser.pending_user_list.take() {
        if pending.op.is_complete() {
            const USER_SONGS_DIR: &str = "assets/userdata/songs";
            match pending.op.take() {
                Some(Ok(files)) => {
                    // Convert file list to SongInfo
                    let songs: Vec<_> = files
                        .iter()
                        .filter(|f| f.ends_with(".ron"))
                        .map(|f| {
                            let full_path = if f.contains('/') {
                                f.clone()
                            } else {
                                format!("{}/{}", USER_SONGS_DIR, f)
                            };
                            let name = full_path
                                .rsplit('/')
                                .next()
                                .and_then(|n| n.strip_suffix(".ron"))
                                .unwrap_or(&full_path)
                                .to_string();
                            tracker::SongInfo {
                                name,
                                path: PathBuf::from(full_path),
                                category: tracker::SongCategory::User,
                            }
                        })
                        .collect();
                    app.tracker.song_browser.user_songs = songs;
                    app.tracker.set_status("Songs loaded", 2.0);
                }
                Some(Err(e)) => {
                    app.tracker.set_status(&format!("Failed to list songs: {}", e), 3.0);
                }
                None => {
                    // No result
                }
            }
        } else {
            // Still pending, put it back
            app.tracker.song_browser.pending_user_list = Some(pending);
        }
    }

    // Poll pending song load (for async cloud/WASM song loading)
    if let Some(mut pending) = app.tracker.pending_song_load.take() {
        if pending.op.is_complete() {
            let load_path = app.tracker.pending_song_path.take();
            match pending.op.take() {
                Some(Ok(data)) => {
                    match parse_song_data(&data) {
                        Ok(song) => {
                            let name = load_path
                                .as_ref()
                                .and_then(|p| p.file_stem())
                                .and_then(|s| s.to_str())
                                .unwrap_or("song")
                                .to_string();
                            app.tracker.apply_song(song, load_path);
                            app.tracker.set_status(&format!("Loaded '{}'", name), 2.0);
                        }
                        Err(e) => {
                            app.tracker.set_status(&format!("Failed to parse song: {}", e), 3.0);
                        }
                    }
                }
                Some(Err(e)) => {
                    app.tracker.set_status(&format!("Failed to load song: {}", e), 3.0);
                }
                None => {}
            }
        } else {
            // Still pending, put it back
            app.tracker.pending_song_load = Some(pending);
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // TEXTURE LIBRARY ASYNC LOADING
    // ═══════════════════════════════════════════════════════════════════════════

    // Poll world editor's pending user texture list
    if let Some(mut pending) = app.world_editor.editor_state.pending_user_texture_list.take() {
        if pending.op.is_complete() {
            const USER_TEXTURES_DIR: &str = "assets/userdata/textures";
            match pending.op.take() {
                Some(Ok(files)) => {
                    // Extract texture names from file list
                    let names: Vec<String> = files
                        .iter()
                        .filter(|f| f.ends_with(".ron"))
                        .map(|f| {
                            let full_path = if f.contains('/') {
                                f.clone()
                            } else {
                                format!("{}/{}", USER_TEXTURES_DIR, f)
                            };
                            full_path
                                .rsplit('/')
                                .next()
                                .and_then(|n| n.strip_suffix(".ron"))
                                .unwrap_or(&full_path)
                                .to_string()
                        })
                        .collect();

                    // Set the names first (so they show in the UI)
                    app.world_editor.editor_state.user_textures.set_user_texture_names(names.clone());

                    // Queue all textures for loading
                    for name in names {
                        let path = format!("{}/{}.ron", USER_TEXTURES_DIR, name);
                        app.world_editor.editor_state.pending_texture_loads.push(
                            (name, load_async(path.into()))
                        );
                    }
                    app.world_editor.editor_state.set_status("Loading textures...", 2.0);
                }
                Some(Err(e)) => {
                    app.world_editor.editor_state.set_status(&format!("Failed to list textures: {}", e), 3.0);
                }
                None => {}
            }
        } else {
            app.world_editor.editor_state.pending_user_texture_list = Some(pending);
        }
    }

    // Poll world editor's pending texture loads (load one at a time)
    if !app.world_editor.editor_state.pending_texture_loads.is_empty() {
        if let Some((_, pending)) = app.world_editor.editor_state.pending_texture_loads.first_mut() {
            if pending.op.is_complete() {
                let (name, pending) = app.world_editor.editor_state.pending_texture_loads.remove(0);
                match pending.op.take() {
                    Some(Ok(data)) => {
                        match parse_texture_data(&data) {
                            Ok(mut texture) => {
                                texture.name = name.clone();
                                texture.source = texture::TextureSource::User;
                                app.world_editor.editor_state.user_textures.add(texture);
                            }
                            Err(e) => {
                                eprintln!("Failed to parse texture '{}': {}", name, e);
                            }
                        }
                    }
                    Some(Err(e)) => {
                        eprintln!("Failed to load texture '{}': {}", name, e);
                    }
                    None => {}
                }
                // Show completion status when all done
                if app.world_editor.editor_state.pending_texture_loads.is_empty() {
                    app.world_editor.editor_state.set_status("Textures loaded", 2.0);
                }
            }
        }
    }

    // Poll modeler's pending user texture list
    if let Some(mut pending) = app.modeler.modeler_state.pending_user_texture_list.take() {
        if pending.op.is_complete() {
            const USER_TEXTURES_DIR: &str = "assets/userdata/textures";
            match pending.op.take() {
                Some(Ok(files)) => {
                    let names: Vec<String> = files
                        .iter()
                        .filter(|f| f.ends_with(".ron"))
                        .map(|f| {
                            let full_path = if f.contains('/') {
                                f.clone()
                            } else {
                                format!("{}/{}", USER_TEXTURES_DIR, f)
                            };
                            full_path
                                .rsplit('/')
                                .next()
                                .and_then(|n| n.strip_suffix(".ron"))
                                .unwrap_or(&full_path)
                                .to_string()
                        })
                        .collect();

                    app.modeler.modeler_state.user_textures.set_user_texture_names(names.clone());

                    for name in names {
                        let path = format!("{}/{}.ron", USER_TEXTURES_DIR, name);
                        app.modeler.modeler_state.pending_texture_loads.push(
                            (name, load_async(path.into()))
                        );
                    }
                    app.modeler.modeler_state.set_status("Loading textures...", 2.0);
                }
                Some(Err(e)) => {
                    app.modeler.modeler_state.set_status(&format!("Failed to list textures: {}", e), 3.0);
                }
                None => {}
            }
        } else {
            app.modeler.modeler_state.pending_user_texture_list = Some(pending);
        }
    }

    // Poll modeler's pending texture loads
    if !app.modeler.modeler_state.pending_texture_loads.is_empty() {
        if let Some((_, pending)) = app.modeler.modeler_state.pending_texture_loads.first_mut() {
            if pending.op.is_complete() {
                let (name, pending) = app.modeler.modeler_state.pending_texture_loads.remove(0);
                match pending.op.take() {
                    Some(Ok(data)) => {
                        match parse_texture_data(&data) {
                            Ok(mut texture) => {
                                texture.name = name.clone();
                                texture.source = texture::TextureSource::User;
                                app.modeler.modeler_state.user_textures.add(texture);
                            }
                            Err(e) => {
                                eprintln!("Failed to parse texture '{}': {}", name, e);
                            }
                        }
                    }
                    Some(Err(e)) => {
                        eprintln!("Failed to load texture '{}': {}", name, e);
                    }
                    None => {}
                }
                if app.modeler.modeler_state.pending_texture_loads.is_empty() {
                    app.modeler.modeler_state.set_status("Textures loaded", 2.0);
                }
            }
        }
    }

    // Handle texture refresh syncing between editor and modeler
    // When one saves a texture, the other should reload to see the changes
    if app.world_editor.editor_state.pending_texture_refresh {
        app.world_editor.editor_state.pending_texture_refresh = false;
        // Reload modeler's textures
        if app.storage.has_cloud() {
            app.modeler.modeler_state.user_textures.clear_user_textures();
            app.modeler.modeler_state.pending_user_texture_list =
                Some(list_async("assets/userdata/textures".to_string()));
        } else {
            #[cfg(not(target_arch = "wasm32"))]
            {
                if let Err(e) = app.modeler.modeler_state.user_textures.discover() {
                    eprintln!("Failed to refresh modeler textures: {}", e);
                }
            }
        }
    }
    if app.modeler.modeler_state.pending_texture_refresh {
        app.modeler.modeler_state.pending_texture_refresh = false;
        // Reload editor's textures
        if app.storage.has_cloud() {
            app.world_editor.editor_state.user_textures.clear_user_textures();
            app.world_editor.editor_state.pending_user_texture_list =
                Some(list_async("assets/userdata/textures".to_string()));
        } else {
            #[cfg(not(target_arch = "wasm32"))]
            {
                if let Err(e) = app.world_editor.editor_state.user_textures.discover() {
                    eprintln!("Failed to refresh editor textures: {}", e);
                }
            }
        }
    }
}

/// Parse texture data from bytes
fn parse_texture_data(data: &[u8]) -> Result<texture::UserTexture, String> {
    texture::UserTexture::load_from_bytes(data)
        .map_err(|e| format!("Parse error: {}", e))
}

/// Parse song data from bytes (handles both compressed and uncompressed)
fn parse_song_data(data: &[u8]) -> Result<tracker::Song, String> {
    use std::io::Cursor;

    // Detect format: RON files start with '(' or whitespace, brotli is binary
    let is_plain_ron = data.first()
        .map(|&b| b == b'(' || b == b' ' || b == b'\n' || b == b'\r' || b == b'\t')
        .unwrap_or(false);

    let contents = if is_plain_ron {
        String::from_utf8(data.to_vec())
            .map_err(|e| format!("Invalid UTF-8: {}", e))?
    } else {
        // Brotli compressed - decompress first
        let mut decompressed = Vec::new();
        brotli::BrotliDecompress(&mut Cursor::new(data), &mut decompressed)
            .map_err(|e| format!("Failed to decompress: {}", e))?;
        String::from_utf8(decompressed)
            .map_err(|e| format!("Invalid UTF-8 after decompression: {}", e))?
    };

    tracker::load_song_from_str(&contents)
}

/// Find the next available level filename with format "level_001", "level_002", etc.
fn next_available_level_name() -> PathBuf {
    let levels_dir = PathBuf::from("assets/userdata/levels");

    // Find the highest existing level_XXX number
    let mut highest = 0;
    if let Ok(entries) = std::fs::read_dir(&levels_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                // Check for pattern "level_XXX"
                if let Some(num_str) = stem.strip_prefix("level_") {
                    if let Ok(num) = num_str.parse::<u32>() {
                        highest = highest.max(num);
                    }
                }
            }
        }
    }

    // Generate next filename
    let next_num = highest + 1;
    levels_dir.join(format!("level_{:03}.ron", next_num))
}

/// Find the next available asset filename with format "asset_001", "asset_002", etc.
fn next_available_asset_path() -> PathBuf {
    let assets_dir = PathBuf::from(asset::ASSETS_DIR);

    // Create directory if it doesn't exist
    let _ = std::fs::create_dir_all(&assets_dir);

    // Find the highest existing asset_XXX number
    let mut highest = 0;
    if let Ok(entries) = std::fs::read_dir(&assets_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                // Check for pattern "asset_XXX"
                if let Some(num_str) = stem.strip_prefix("asset_") {
                    if let Ok(num) = num_str.parse::<u32>() {
                        highest = highest.max(num);
                    }
                }
            }
        }
    }

    // Generate next filename
    let next_num = highest + 1;
    assets_dir.join(format!("asset_{:03}.ron", next_num))
}

/// Handle save action with async support for cloud storage
fn handle_save_action(app: &mut AppState) {
    // Don't start a new save if one is already in progress
    if app.pending_ops.save.is_some() {
        app.world_editor.editor_state.set_status("Save already in progress...", 1.0);
        return;
    }

    let ws = &mut app.world_editor;

    // Save editor layout state
    ws.editor_state.level.editor_layout = ws.editor_layout.to_config(
        ws.editor_state.grid_offset_x,
        ws.editor_state.grid_offset_y,
        ws.editor_state.grid_zoom,
        ws.editor_state.orbit_target,
        ws.editor_state.orbit_distance,
        ws.editor_state.orbit_azimuth,
        ws.editor_state.orbit_elevation,
    );

    // Determine save path
    let save_path = if let Some(path) = &ws.editor_state.current_file {
        path.clone()
    } else {
        // Generate next available level_XXX name
        let default_path = next_available_level_name();
        if let Some(parent) = default_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        ws.editor_state.current_file = Some(default_path.clone());
        default_path
    };

    // Serialize level to bytes
    let data = match serialize_level(&ws.editor_state.level) {
        Ok(data) => data,
        Err(e) => {
            ws.editor_state.set_status(&format!("Save failed: {}", e), 5.0);
            return;
        }
    };

    let storage = &app.storage;
    let path_str = save_path.to_string_lossy().to_string();

    // Use async save for cloud storage, sync for local
    if storage.has_cloud() && storage::Storage::is_userdata_path(&path_str) {
        // Start async save
        app.world_editor.editor_state.set_status("Saving...", 30.0);
        app.pending_ops.save = Some(save_async(save_path.clone(), data));
        app.pending_ops.status_message = Some("Saving...".to_string());
    } else {
        // Sync save for local storage
        match storage.write_sync(&path_str, &data) {
            Ok(()) => {
                app.world_editor.editor_state.dirty = false;
                let mode_label = storage.mode().label();
                app.world_editor.editor_state.set_status(
                    &format!("Saved ({}) {}", mode_label, save_path.display()),
                    3.0,
                );
            }
            Err(e) => {
                app.world_editor.editor_state.set_status(&format!("Save failed: {}", e), 5.0);
            }
        }
    }
}

/// Handle modeler save action with async support for cloud storage
fn handle_modeler_save_action(app: &mut AppState) {
    // Don't start a new save if one is already in progress
    if app.pending_ops.modeler_save.is_some() {
        app.modeler.modeler_state.set_status("Save already in progress...", 1.0);
        return;
    }

    let state = &mut app.modeler.modeler_state;

    // Determine save path
    let save_path = if let Some(path) = &state.current_file {
        path.clone()
    } else {
        // Generate next available asset_XXX name
        let default_path = next_available_asset_path();
        state.current_file = Some(default_path.clone());
        default_path
    };

    // Serialize asset to bytes
    let data = match state.asset.to_bytes() {
        Ok(data) => data,
        Err(e) => {
            state.set_status(&format!("Save failed: {}", e), 5.0);
            return;
        }
    };

    let storage = &app.storage;
    let path_str = save_path.to_string_lossy().to_string();

    // Use async save for cloud storage, sync for local
    if storage.has_cloud() && storage::Storage::is_userdata_path(&path_str) {
        // Start async save
        app.modeler.modeler_state.set_status("Saving...", 30.0);
        app.pending_ops.modeler_save = Some(save_async(save_path.clone(), data));
        app.pending_ops.status_message = Some("Saving...".to_string());
    } else {
        // Sync save for local storage
        match storage.write_sync(&path_str, &data) {
            Ok(()) => {
                app.modeler.modeler_state.dirty = false;
                let mode_label = storage.mode().label();
                app.modeler.modeler_state.set_status(
                    &format!("Saved ({}) {}", mode_label, save_path.display()),
                    3.0,
                );
            }
            Err(e) => {
                app.modeler.modeler_state.set_status(&format!("Save failed: {}", e), 5.0);
            }
        }
    }
}

fn handle_editor_action(action: EditorAction, app: &mut AppState) {
    let storage = &app.storage;
    let ws = &mut app.world_editor;
    let game = &mut app.game;

    match action {
        EditorAction::Play => {
            ws.editor_state.set_status("Game preview coming soon", 2.0);
        }
        EditorAction::New => {
            let new_level = create_empty_level();
            ws.editor_state = editor::EditorState::new(new_level);
            ws.editor_layout.apply_config(&ws.editor_state.level.editor_layout);
            // Reset grid view to defaults for new level
            ws.editor_state.grid_offset_x = ws.editor_state.level.editor_layout.grid_offset_x;
            ws.editor_state.grid_offset_y = ws.editor_state.level.editor_layout.grid_offset_y;
            ws.editor_state.grid_zoom = ws.editor_state.level.editor_layout.grid_zoom;
            // Reset orbit camera to defaults
            ws.editor_state.orbit_target = rasterizer::Vec3::new(
                ws.editor_state.level.editor_layout.orbit_target_x,
                ws.editor_state.level.editor_layout.orbit_target_y,
                ws.editor_state.level.editor_layout.orbit_target_z,
            );
            ws.editor_state.orbit_distance = ws.editor_state.level.editor_layout.orbit_distance;
            ws.editor_state.orbit_azimuth = ws.editor_state.level.editor_layout.orbit_azimuth;
            ws.editor_state.orbit_elevation = ws.editor_state.level.editor_layout.orbit_elevation;
            ws.editor_state.sync_camera_from_orbit();
            ws.editor_state.set_status("Created new level", 3.0);
        }
        EditorAction::Save => {
            // Handle save - access app fields directly to avoid borrow conflicts
            handle_save_action(app);
        }
        #[cfg(not(target_arch = "wasm32"))]
        EditorAction::SaveAs => {
            ws.editor_state.level.editor_layout = ws.editor_layout.to_config(
                ws.editor_state.grid_offset_x,
                ws.editor_state.grid_offset_y,
                ws.editor_state.grid_zoom,
                ws.editor_state.orbit_target,
                ws.editor_state.orbit_distance,
                ws.editor_state.orbit_azimuth,
                ws.editor_state.orbit_elevation,
            );
            let default_dir = PathBuf::from("assets/userdata/levels");
            let _ = std::fs::create_dir_all(&default_dir);

            let dialog = rfd::FileDialog::new()
                .add_filter("RON Level", &["ron"])
                .set_directory(&default_dir)
                .set_file_name("level.ron");

            if let Some(save_path) = dialog.save_file() {
                let path_str = save_path.to_string_lossy();
                match save_level_with_storage(&ws.editor_state.level, &path_str, storage) {
                    Ok(()) => {
                        ws.editor_state.current_file = Some(save_path.clone());
                        ws.editor_state.dirty = false;
                        let mode_label = storage.mode().label();
                        ws.editor_state.set_status(&format!("Saved ({}) {}", mode_label, save_path.display()), 3.0);
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
            let default_dir = PathBuf::from("assets/userdata/levels");
            let _ = std::fs::create_dir_all(&default_dir);

            let dialog = rfd::FileDialog::new()
                .add_filter("RON Level", &["ron"])
                .set_directory(&default_dir);

            if let Some(path) = dialog.pick_file() {
                let path_str = path.to_string_lossy();
                match load_level_with_storage(&path_str, storage) {
                    Ok(level) => {
                        ws.editor_layout.apply_config(&level.editor_layout);
                        ws.editor_state.grid_offset_x = level.editor_layout.grid_offset_x;
                        ws.editor_state.grid_offset_y = level.editor_layout.grid_offset_y;
                        ws.editor_state.grid_zoom = level.editor_layout.grid_zoom;
                        ws.editor_state.orbit_target = rasterizer::Vec3::new(
                            level.editor_layout.orbit_target_x,
                            level.editor_layout.orbit_target_y,
                            level.editor_layout.orbit_target_z,
                        );
                        ws.editor_state.orbit_distance = level.editor_layout.orbit_distance;
                        ws.editor_state.orbit_azimuth = level.editor_layout.orbit_azimuth;
                        ws.editor_state.orbit_elevation = level.editor_layout.orbit_elevation;
                        ws.editor_state.sync_camera_from_orbit();
                        ws.editor_state.load_level(level, path.clone());
                        // Reset game state for the new level
                        game.reset_for_new_level();
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
            ws.editor_state.level.editor_layout = ws.editor_layout.to_config(
                ws.editor_state.grid_offset_x,
                ws.editor_state.grid_offset_y,
                ws.editor_state.grid_zoom,
                ws.editor_state.orbit_target,
                ws.editor_state.orbit_distance,
                ws.editor_state.orbit_azimuth,
                ws.editor_state.orbit_elevation,
            );

            match ron::ser::to_string_pretty(&ws.editor_state.level, ron::ser::PrettyConfig::default()) {
                Ok(ron_str) => {
                    let filename = ws.editor_state.current_file
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "level.ron".to_string());

                    extern "C" {
                        fn b32_set_export_data(ptr: *const u8, len: usize);
                        fn b32_set_export_filename(ptr: *const u8, len: usize);
                        fn b32_trigger_download();
                    }
                    unsafe {
                        b32_set_export_data(ron_str.as_ptr(), ron_str.len());
                        b32_set_export_filename(filename.as_ptr(), filename.len());
                        b32_trigger_download();
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
                fn b32_import_file();
            }
            unsafe {
                b32_import_file();
            }
            ws.editor_state.set_status("Select a .ron file to import...", 3.0);
        }
        #[cfg(not(target_arch = "wasm32"))]
        EditorAction::Import => {
            ws.editor_state.set_status("Import is for browser - use Open", 3.0);
        }
        EditorAction::Load(path_str) => {
            let path = PathBuf::from(&path_str);
            match load_level_with_storage(&path_str, storage) {
                Ok(level) => {
                    ws.editor_layout.apply_config(&level.editor_layout);
                    ws.editor_state.grid_offset_x = level.editor_layout.grid_offset_x;
                    ws.editor_state.grid_offset_y = level.editor_layout.grid_offset_y;
                    ws.editor_state.grid_zoom = level.editor_layout.grid_zoom;
                    ws.editor_state.orbit_target = rasterizer::Vec3::new(
                        level.editor_layout.orbit_target_x,
                        level.editor_layout.orbit_target_y,
                        level.editor_layout.orbit_target_z,
                    );
                    ws.editor_state.orbit_distance = level.editor_layout.orbit_distance;
                    ws.editor_state.orbit_azimuth = level.editor_layout.orbit_azimuth;
                    ws.editor_state.orbit_elevation = level.editor_layout.orbit_elevation;
                    ws.editor_state.sync_camera_from_orbit();
                    ws.editor_state.load_level(level, path.clone());
                    // Reset game state for the new level
                    game.reset_for_new_level();
                    ws.editor_state.set_status(&format!("Loaded {}", path.display()), 3.0);
                }
                Err(e) => {
                    ws.editor_state.set_status(&format!("Load failed: {}", e), 5.0);
                }
            }
        }
        EditorAction::OpenLevelBrowser => {
            // Open the level browser immediately, user levels load async
            let samples = discover_sample_levels();
            ws.level_browser.open_with_levels(samples, Vec::new());
            #[cfg(target_arch = "wasm32")]
            {
                ws.level_browser.pending_load_list = true;
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                // Start async user level discovery if cloud is enabled
                if storage.has_cloud() {
                    ws.level_browser.pending_user_list = Some(list_async("assets/userdata/levels".to_string()));
                } else {
                    // Local storage is fast, use sync
                    ws.level_browser.user_levels = discover_user_levels(storage);
                }
            }
            ws.editor_state.set_status("Browse levels", 2.0);
        }
        EditorAction::SwitchToModeler => {
            // Switch to Asset Editor and create a new asset
            app.active_tool = Tool::Modeler;
            app.modeler.modeler_state.new_mesh();
            app.modeler.modeler_state.set_status("New asset created", 2.0);
        }
        EditorAction::Exit | EditorAction::None => {}
    }
}

fn handle_modeler_action(
    action: ModelerAction,
    state: &mut modeler::ModelerState,
    model_browser: &mut modeler::ModelBrowser,
    obj_importer: &mut modeler::ObjImportBrowser,
) {
    match action {
        ModelerAction::New => {
            state.new_mesh();
        }
        ModelerAction::BrowseModels => {
            let models = discover_models();
            model_browser.open(models);
            // On WASM, trigger async load of model list
            #[cfg(target_arch = "wasm32")]
            {
                model_browser.pending_load_list = true;
            }
            state.set_status("Browse assets", 2.0);
        }
        ModelerAction::ImportObj => {
            let meshes = discover_meshes();
            obj_importer.open(meshes);
            // On WASM, trigger async load of mesh list
            #[cfg(target_arch = "wasm32")]
            {
                obj_importer.pending_load_list = true;
            }
            state.set_status("Import OBJ", 2.0);
        }
        ModelerAction::Save => {
            // Handled by handle_modeler_save_action before this function is called
            // This arm exists for completeness but should not be reached
        }
        #[cfg(not(target_arch = "wasm32"))]
        ModelerAction::SaveAs => {
            let default_dir = PathBuf::from(asset::ASSETS_DIR);
            let _ = std::fs::create_dir_all(&default_dir);

            let dialog = rfd::FileDialog::new()
                .add_filter("RON Asset", &["ron"])
                .set_directory(&default_dir)
                .set_file_name("asset.ron");

            if let Some(save_path) = dialog.save_file() {
                if let Err(e) = state.save_project(&save_path) {
                    state.set_status(&format!("Save failed: {}", e), 5.0);
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        ModelerAction::SaveAs => {
            state.set_status("Save As not available in browser", 3.0);
        }
        #[cfg(not(target_arch = "wasm32"))]
        ModelerAction::PromptLoad => {
            let default_dir = PathBuf::from(asset::ASSETS_DIR);
            let _ = std::fs::create_dir_all(&default_dir);

            let dialog = rfd::FileDialog::new()
                .add_filter("RON Model", &["ron"])
                .set_directory(&default_dir);

            if let Some(path) = dialog.pick_file() {
                if let Err(e) = state.load_project(&path) {
                    eprintln!("Load failed: {}", e);
                    state.set_status(&format!("Load failed: {}", e), 5.0);
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        ModelerAction::PromptLoad => {
            state.set_status("Open not available in browser - use Upload", 3.0);
        }
        #[cfg(not(target_arch = "wasm32"))]
        ModelerAction::Load(path_str) => {
            let path = PathBuf::from(&path_str);
            if let Err(e) = state.load_project(&path) {
                eprintln!("Load failed: {}", e);
                state.set_status(&format!("Load failed: {}", e), 5.0);
            }
        }
        #[cfg(target_arch = "wasm32")]
        ModelerAction::Load(_path_str) => {
            state.set_status("Load not available in browser - use Upload", 3.0);
        }
        #[cfg(target_arch = "wasm32")]
        ModelerAction::Export => {
            match ron::ser::to_string_pretty(state.mesh(), ron::ser::PrettyConfig::default()) {
                Ok(ron_str) => {
                    let filename = state.current_file
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "mesh.ron".to_string());

                    extern "C" {
                        fn b32_set_export_data(ptr: *const u8, len: usize);
                        fn b32_set_export_filename(ptr: *const u8, len: usize);
                        fn b32_trigger_download();
                    }
                    unsafe {
                        b32_set_export_data(ron_str.as_ptr(), ron_str.len());
                        b32_set_export_filename(filename.as_ptr(), filename.len());
                        b32_trigger_download();
                    }

                    state.dirty = false;
                    state.set_status(&format!("Downloaded {}", filename), 3.0);
                }
                Err(e) => {
                    state.set_status(&format!("Export failed: {}", e), 5.0);
                }
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        ModelerAction::Export => {
            state.set_status("Export is for browser - use Save As", 3.0);
        }
        #[cfg(target_arch = "wasm32")]
        ModelerAction::Import => {
            extern "C" {
                fn b32_import_file();
            }
            unsafe {
                b32_import_file();
            }
            state.set_status("Select a .ron file to import...", 3.0);
        }
        #[cfg(not(target_arch = "wasm32"))]
        ModelerAction::Import => {
            state.set_status("Import is for browser - use Open", 3.0);
        }
        ModelerAction::None => {}
    }
}
