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
mod game;
mod project;
mod input;

use macroquad::prelude::*;
use rasterizer::{Framebuffer, Texture, HEIGHT, WIDTH};
use world::{create_empty_level, load_level, save_level};
use ui::{UiContext, MouseState, Rect, draw_fixed_tabs, TabEntry, layout as tab_layout, icon};
use editor::{EditorAction, draw_editor, draw_example_browser, BrowserAction, discover_examples};
use modeler::{ModelerAction, ModelBrowserAction, MeshBrowserAction, draw_model_browser, draw_mesh_browser, discover_models, discover_meshes, ObjImporter};
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

    // Load textures from manifest (WASM needs async loading)
    #[cfg(target_arch = "wasm32")]
    {
        use editor::TexturePack;
        app.world_editor.editor_state.texture_packs = TexturePack::load_from_manifest().await;
        println!("WASM: Loaded {} texture packs", app.world_editor.editor_state.texture_packs.len());
    }

    println!("=== Bonnie Engine ===");

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
                landing::draw_landing(content_rect, &mut app.landing);
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
                    &app.input,
                );

                // Handle editor actions (including opening example browser)
                handle_editor_action(action, ws);

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
                );
            }

            Tool::Modeler => {
                let ms = &mut app.modeler;

                // Block background input if any browser is open
                let real_mouse_modeler = mouse_state;
                if ms.model_browser.open || ms.mesh_browser.open {
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
                handle_modeler_action(action, &mut ms.modeler_state, &mut ms.model_browser, &mut ms.mesh_browser);

                // Draw model browser overlay if open (native only - uses filesystem)
                #[cfg(not(target_arch = "wasm32"))]
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
                            if let Some(model_info) = ms.model_browser.models.get(index) {
                                let path = model_info.path.clone();
                                match modeler::MeshProject::load_from_file(&path) {
                                    Ok(project) => {
                                        ms.model_browser.set_preview(project);
                                    }
                                    Err(e) => {
                                        eprintln!("Failed to load model: {}", e);
                                        ms.modeler_state.set_status(&format!("Failed to load: {}", e), 3.0);
                                    }
                                }
                            }
                        }
                        ModelBrowserAction::OpenModel => {
                            if let Some(project) = ms.model_browser.preview_project.take() {
                                let path = ms.model_browser.selected_model()
                                    .map(|m| m.path.clone())
                                    .unwrap_or_else(|| PathBuf::from("assets/models/untitled.ron"));
                                ms.modeler_state.project = project;
                                ms.modeler_state.sync_mesh_from_project();
                                ms.modeler_state.current_file = Some(path.clone());
                                ms.modeler_state.dirty = false;
                                ms.modeler_state.selection = modeler::ModelerSelection::None;
                                ms.modeler_state.set_status(&format!("Opened: {}", path.display()), 3.0);
                                ms.model_browser.close();
                            }
                        }
                        ModelBrowserAction::NewModel => {
                            ms.modeler_state.new_mesh();
                            ms.model_browser.close();
                        }
                        ModelBrowserAction::Cancel => {
                            ms.model_browser.close();
                        }
                        ModelBrowserAction::None => {}
                    }
                }

                // Draw mesh browser overlay if open (native only - uses filesystem)
                #[cfg(not(target_arch = "wasm32"))]
                if ms.mesh_browser.open {
                    ui_ctx.end_modal(real_mouse_modeler);

                    let browser_action = draw_mesh_browser(
                        &mut ui_ctx,
                        &mut ms.mesh_browser,
                        app.icon_font.as_ref(),
                        &mut fb,
                    );

                    match browser_action {
                        MeshBrowserAction::SelectPreview(index) => {
                            if let Some(mesh_info) = ms.mesh_browser.meshes.get(index) {
                                let path = mesh_info.path.clone();
                                match ObjImporter::load_from_file(&path) {
                                    Ok(mut mesh) => {
                                        // Compute normals for shading in preview
                                        ObjImporter::compute_face_normals(&mut mesh);
                                        ms.mesh_browser.set_preview(mesh);
                                    }
                                    Err(e) => {
                                        eprintln!("Failed to load mesh: {}", e);
                                        ms.modeler_state.set_status(&format!("Failed to load: {}", e), 3.0);
                                    }
                                }
                            }
                        }
                        MeshBrowserAction::OpenMesh => {
                            if let Some(mut mesh) = ms.mesh_browser.preview_mesh.take() {
                                let path = ms.mesh_browser.selected_mesh()
                                    .map(|m| m.path.clone())
                                    .unwrap_or_else(|| PathBuf::from("assets/meshes/untitled.obj"));

                                // Compute normals if the OBJ didn't have them (required for shading)
                                ObjImporter::compute_face_normals(&mut mesh);

                                // Apply scale to mesh vertices
                                let scale = ms.mesh_browser.import_scale;
                                for vertex in &mut mesh.vertices {
                                    vertex.pos.x *= scale;
                                    vertex.pos.y *= scale;
                                    vertex.pos.z *= scale;
                                }

                                // Set the editable mesh
                                ms.modeler_state.mesh = mesh;
                                ms.modeler_state.current_file = Some(path.clone());
                                ms.modeler_state.dirty = false;
                                ms.modeler_state.selection = modeler::ModelerSelection::None;

                                // Reset camera to fit the scaled mesh
                                ms.modeler_state.orbit_target = crate::rasterizer::Vec3::new(0.0, 50.0, 0.0);
                                ms.modeler_state.orbit_distance = scale * 3.0;
                                ms.modeler_state.sync_camera_from_orbit();

                                ms.modeler_state.set_status(&format!("Opened mesh: {} (scale {}x)", path.display(), scale), 3.0);
                                ms.mesh_browser.close();
                            }
                        }
                        MeshBrowserAction::Cancel => {
                            ms.mesh_browser.close();
                        }
                        MeshBrowserAction::None => {}
                    }
                }
            }

            Tool::Tracker => {
                // Update playback timing
                let delta = get_frame_time() as f64;
                app.tracker.update_playback(delta);

                // Draw tracker UI
                tracker::draw_tracker(&mut ui_ctx, content_rect, &mut app.tracker, app.icon_font.as_ref());
            }

            Tool::InputTest => {
                // Draw controller debug view
                input::draw_controller_debug(content_rect, &app.input);
            }
        }

        // Draw tooltips last (on top of everything)
        ui_ctx.draw_tooltip();

        // Handle pending async level load (WASM) - after all drawing is complete
        #[cfg(target_arch = "wasm32")]
        if let Tool::WorldEditor = app.active_tool {
            let ws = &mut app.world_editor;
            if let Some(path) = ws.example_browser.pending_load_path.take() {
                use editor::load_example_level;
                if let Some(level) = load_example_level(&path).await {
                    ws.example_browser.set_preview(level);
                } else {
                    ws.editor_state.set_status("Failed to load level preview", 3.0);
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

/// Find the next available model filename with format "model_001", "model_002", etc.
fn next_available_model_name() -> PathBuf {
    let models_dir = PathBuf::from("assets/models");

    // Create directory if it doesn't exist
    let _ = std::fs::create_dir_all(&models_dir);

    // Find the highest existing model_XXX number
    let mut highest = 0;
    if let Ok(entries) = std::fs::read_dir(&models_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                // Check for pattern "model_XXX"
                if let Some(num_str) = stem.strip_prefix("model_") {
                    if let Ok(num) = num_str.parse::<u32>() {
                        highest = highest.max(num);
                    }
                }
            }
        }
    }

    // Generate next filename
    let next_num = highest + 1;
    models_dir.join(format!("model_{:03}.ron", next_num))
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
            ws.editor_state.set_status("Browse levels", 2.0);
        }
        EditorAction::Exit | EditorAction::None => {}
    }
}

fn handle_modeler_action(
    action: ModelerAction,
    state: &mut modeler::ModelerState,
    model_browser: &mut modeler::ModelBrowser,
    mesh_browser: &mut modeler::MeshBrowser,
) {
    match action {
        ModelerAction::New => {
            state.new_mesh();
        }
        ModelerAction::BrowseModels => {
            let models = discover_models();
            model_browser.open(models);
            state.set_status("Browse models", 2.0);
        }
        ModelerAction::BrowseMeshes => {
            let meshes = discover_meshes();
            mesh_browser.open(meshes);
            state.set_status("Browse meshes", 2.0);
        }
        #[cfg(not(target_arch = "wasm32"))]
        ModelerAction::Save => {
            if let Some(path) = state.current_file.clone() {
                if let Err(e) = state.save_project(&path) {
                    state.set_status(&format!("Save failed: {}", e), 5.0);
                }
            } else {
                // No current file - save with auto-generated name (model_001, model_002, etc.)
                let default_path = next_available_model_name();
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
            let default_dir = PathBuf::from("assets/models");
            let _ = std::fs::create_dir_all(&default_dir);

            let dialog = rfd::FileDialog::new()
                .add_filter("RON Model", &["ron"])
                .set_directory(&default_dir)
                .set_file_name("model.ron");

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
            let default_dir = PathBuf::from("assets/models");
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
            match ron::ser::to_string_pretty(&state.mesh, ron::ser::PrettyConfig::default()) {
                Ok(ron_str) => {
                    let filename = state.current_file
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "mesh.ron".to_string());

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
                fn bonnie_import_file();
            }
            unsafe {
                bonnie_import_file();
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
