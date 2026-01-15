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

use macroquad::prelude::*;
use rasterizer::{Framebuffer, Texture, HEIGHT, WIDTH};
use world::{create_empty_level, load_level, save_level};
use ui::{UiContext, MouseState, Rect, draw_fixed_tabs, TabEntry, layout as tab_layout, icon};
use editor::{EditorAction, draw_editor, draw_example_browser, BrowserAction, discover_examples};
use modeler::{ModelerAction, ModelBrowserAction, ObjImportAction, draw_model_browser, draw_obj_importer, discover_models, discover_meshes, ObjImporter, TextureImportResult};
use app::{AppState, Tool};
use std::path::PathBuf;

fn window_conf() -> Conf {
    Conf {
        window_title: format!("BONNIE-32 v{}", VERSION),
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
        match app.modeler.user_textures.discover_from_manifest().await {
            Ok(count) => println!("WASM: Loaded {} user textures for modeler", count),
            Err(e) => eprintln!("WASM: Failed to load user textures for modeler: {}", e),
        }
    }

    println!("=== BONNIE-32 ===");

    loop {
        // Track frame start time for FPS limiting
        let frame_start = get_time();

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

        // Poll gamepad input
        app.input.poll();

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
            TabEntry::new(icon::PLAY, "Game"),
            TabEntry::new(icon::PERSON_STANDING, "Assets"),
            TabEntry::new(icon::MUSIC, "Music"),
            TabEntry::new(icon::GAMEPAD_2, "Input"),
        ];
        if let Some(clicked) = draw_fixed_tabs(&mut ui_ctx, tab_bar_rect, &tabs, app.active_tool_index(), app.icon_font.as_ref()) {
            if let Some(tool) = Tool::from_index(clicked) {
                // Open browser on first World Editor visit
                if tool == Tool::WorldEditor && world_editor_first_open {
                    world_editor_first_open = false;
                    // Fresh scan to pick up any newly saved levels
                    app.world_editor.example_browser.open(discover_examples());
                    // On WASM, trigger async load of example list
                    #[cfg(target_arch = "wasm32")]
                    {
                        app.world_editor.example_browser.pending_load_list = true;
                    }
                }
                // Reset game state when switching to Test tab
                // This ensures player spawns fresh from current level's PlayerStart
                if tool == Tool::Test {
                    app.game.reset();
                }
                app.set_active_tool(tool);
            }
        }

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
                    app.world_editor.example_browser.open(discover_examples());
                    #[cfg(target_arch = "wasm32")]
                    {
                        app.world_editor.example_browser.pending_load_list = true;
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
                );

                // Handle editor actions (including opening example browser)
                handle_editor_action(action, ws, &mut app.game);

                // Draw example browser overlay if open
                if ws.example_browser.open {
                    // End modal blocking so the browser itself can receive input
                    ui_ctx.end_modal(real_mouse);

                    let browser_action = draw_example_browser(
                        &mut ui_ctx,
                        &mut ws.example_browser,
                        app.icon_font.as_ref(),
                        &ws.editor_state.texture_packs,
                    );

                    match browser_action {
                        BrowserAction::SelectPreview(index) => {
                            if let Some(example) = ws.example_browser.examples.get(index) {
                                let path = example.path.clone();
                                #[cfg(not(target_arch = "wasm32"))]
                                {
                                    // Native: load synchronously
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
                                #[cfg(target_arch = "wasm32")]
                                {
                                    // WASM: set pending path for async load (handled after drawing)
                                    ws.example_browser.pending_load_path = Some(path);
                                }
                            }
                        }
                        BrowserAction::OpenLevel => {
                            // Load the selected level, preserving texture packs and other state
                            if let Some(level) = ws.example_browser.preview_level.take() {
                                let (name, path) = ws.example_browser.selected_example()
                                    .map(|e| (e.name.clone(), e.path.clone()))
                                    .unwrap_or_else(|| ("example".to_string(), PathBuf::from("assets/levels/untitled.ron")));
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
                                ws.example_browser.close();
                            }
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
                            ws.editor_state.load_level(new_level, PathBuf::from("assets/levels/untitled.ron"));
                            ws.editor_state.current_file = None; // New level has no file yet
                            // Reset game state for the new level
                            app.game.reset_for_new_level();
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

            Tool::Test => {
                // Build textures array from World Editor texture packs
                let game_textures: Vec<Texture> = app.world_editor.editor_state.texture_packs
                    .iter()
                    .flat_map(|pack| &pack.textures)
                    .cloned()
                    .collect();

                // Spawn player if playing and no player exists
                if app.game.playing && app.game.player_entity.is_none() {
                    if let Some((room_idx, spawn)) = app.project.level.get_player_start() {
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
                );
            }

            Tool::Modeler => {
                let ms = &mut app.modeler;

                // Block background input if any browser is open
                let real_mouse_modeler = mouse_state;
                if ms.model_browser.open || ms.obj_importer.open {
                    ui_ctx.begin_modal();
                }

                // Draw modeler UI
                let action = modeler::draw_modeler(
                    &mut ui_ctx,
                    &mut ms.modeler_layout,
                    &mut ms.modeler_state,
                    &mut fb,
                    content_rect,
                    app.icon_font.as_ref(),
                );

                // Handle modeler actions
                handle_modeler_action(action, &mut ms.modeler_state, &mut ms.model_browser, &mut ms.obj_importer);

                // Draw model browser overlay if open
                if ms.model_browser.open {
                    ui_ctx.end_modal(real_mouse_modeler);

                    let browser_action = draw_model_browser(
                        &mut ui_ctx,
                        &mut ms.model_browser,
                        app.icon_font.as_ref(),
                        &mut fb,
                    );

                    match browser_action {
                        ModelBrowserAction::SelectPreview(index) => {
                            if let Some(asset_info) = ms.model_browser.assets.get(index) {
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
                                    .unwrap_or_else(|| PathBuf::from("assets/assets/untitled.ron"));
                                // Set the asset directly in the modeler
                                ms.modeler_state.asset = asset;
                                ms.modeler_state.selected_object = if ms.modeler_state.objects().is_empty() { None } else { Some(0) };
                                // Resolve ID-based texture refs using the texture library
                                ms.modeler_state.resolve_all_texture_refs();
                                ms.modeler_state.current_file = Some(path.clone());
                                ms.modeler_state.dirty = false;
                                ms.modeler_state.selection = modeler::ModelerSelection::None;
                                ms.modeler_state.set_status(&format!("Opened: {}", path.display()), 3.0);
                                ms.model_browser.close();
                            }
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
                                .unwrap_or_else(|| PathBuf::from("assets/meshes/untitled.obj"));
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
                tracker::draw_tracker(&mut ui_ctx, content_rect, &mut app.tracker, app.icon_font.as_ref());

                // Draw song browser overlay if open
                if app.tracker.song_browser.open {
                    // End modal blocking so the browser itself can receive input
                    ui_ctx.end_modal(real_mouse_tracker);

                    let _browser_action = tracker::draw_song_browser(
                        &mut ui_ctx,
                        &mut app.tracker,
                        app.icon_font.as_ref(),
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
            // Load example list from manifest if pending
            if ws.example_browser.pending_load_list {
                ws.example_browser.pending_load_list = false;
                use editor::load_example_list;
                let examples = load_example_list().await;
                ws.example_browser.examples = examples;
            }
            // Load individual level preview if pending
            if let Some(path) = ws.example_browser.pending_load_path.take() {
                use editor::load_example_level;
                if let Some(level) = load_example_level(&path).await {
                    ws.example_browser.set_preview(level);
                } else {
                    ws.editor_state.set_status("Failed to load level preview", 3.0);
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
                use modeler::load_model_list;
                let models = load_model_list().await;
                ms.model_browser.models = models;
            }
            // Load individual model preview if pending
            if let Some(path) = ms.model_browser.pending_load_path.take() {
                use modeler::load_model;
                if let Some(project) = load_model(&path).await {
                    ms.model_browser.set_preview(project);
                } else {
                    ms.modeler_state.set_status("Failed to load model preview", 3.0);
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
                use tracker::load_song_list;
                let songs = load_song_list().await;
                ts.song_browser.songs = songs;
            }
            // Load individual song preview if pending
            if let Some(path) = ts.song_browser.pending_load_path.take() {
                use tracker::load_song_async;
                if let Some(song) = load_song_async(&path).await {
                    ts.song_browser.set_preview(song);
                } else {
                    ts.set_status("Failed to load song preview", 3.0);
                }
            }
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


/// Find the next available level filename with format "level_001", "level_002", etc.
fn next_available_level_name() -> PathBuf {
    let levels_dir = PathBuf::from("assets/levels");

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

fn handle_editor_action(action: EditorAction, ws: &mut app::WorldEditorState, game: &mut game::GameToolState) {
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
            ws.editor_state.level.editor_layout = ws.editor_layout.to_config(
                ws.editor_state.grid_offset_x,
                ws.editor_state.grid_offset_y,
                ws.editor_state.grid_zoom,
                ws.editor_state.orbit_target,
                ws.editor_state.orbit_distance,
                ws.editor_state.orbit_azimuth,
                ws.editor_state.orbit_elevation,
            );

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
                // Generate next available level_XXX name
                let default_path = next_available_level_name();
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
            ws.editor_state.level.editor_layout = ws.editor_layout.to_config(
                ws.editor_state.grid_offset_x,
                ws.editor_state.grid_offset_y,
                ws.editor_state.grid_zoom,
                ws.editor_state.orbit_target,
                ws.editor_state.orbit_distance,
                ws.editor_state.orbit_azimuth,
                ws.editor_state.orbit_elevation,
            );
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
            match load_level(&path) {
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
        EditorAction::BrowseExamples => {
            // Open the level browser - fresh scan to pick up newly saved levels
            ws.example_browser.open(discover_examples());
            // On WASM, trigger async load of example list
            #[cfg(target_arch = "wasm32")]
            {
                ws.example_browser.pending_load_list = true;
            }
            ws.editor_state.set_status("Browse levels", 2.0);
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
        ModelerAction::BrowseMeshes => {
            let meshes = discover_meshes();
            obj_importer.open(meshes);
            // On WASM, trigger async load of mesh list
            #[cfg(target_arch = "wasm32")]
            {
                obj_importer.pending_load_list = true;
            }
            state.set_status("Browse meshes", 2.0);
        }
        #[cfg(not(target_arch = "wasm32"))]
        ModelerAction::Save => {
            if let Some(path) = state.current_file.clone() {
                if let Err(e) = state.save_project(&path) {
                    state.set_status(&format!("Save failed: {}", e), 5.0);
                }
            } else {
                // No current file - save with auto-generated name (asset_001, asset_002, etc.)
                let default_path = next_available_asset_path();
                if let Err(e) = state.save_project(&default_path) {
                    state.set_status(&format!("Save failed: {}", e), 5.0);
                } else {
                    state.current_file = Some(default_path);
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        ModelerAction::Save => {
            state.set_status("Save not available in browser - use Export", 3.0);
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
