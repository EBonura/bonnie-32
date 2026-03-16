pub mod context;
pub mod icons;
pub mod theme;
pub mod toolbar;
pub mod content_browser;
pub mod hierarchy;
pub mod inspector;
pub mod modeler;
pub mod viewport;
pub mod tracker;
pub mod world_editor;

use crate::asset::{AssetHandle, AssetType};

use context::{EditorAction, EditorContext, EditorMode};
use content_browser::ContentBrowser;
use hierarchy::HierarchyPanel;
use inspector::InspectorPanel;
use modeler::ModelerPanel;
use viewport::ViewportPanel;
use tracker::TrackerPanel;
use world_editor::WorldEditorPanel;

pub struct Editor {
    pub ctx: EditorContext,
    content_browser: ContentBrowser,
    hierarchy: HierarchyPanel,
    inspector: InspectorPanel,
    modeler: ModelerPanel,
    pub viewport: ViewportPanel,
    tracker: TrackerPanel,
    world_editor: WorldEditorPanel,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            ctx: EditorContext::new(),
            content_browser: ContentBrowser::new(),
            hierarchy: HierarchyPanel::new(),
            inspector: InspectorPanel::new(),
            modeler: ModelerPanel::new(),
            viewport: ViewportPanel::new(),
            tracker: TrackerPanel::new(),
            world_editor: WorldEditorPanel::new(),
        }
    }

    /// Draw all editor UI. Call this inside egui_ctx.run().
    pub fn draw(&mut self, egui_ctx: &egui::Context) {
        toolbar::draw_toolbar(egui_ctx, &mut self.ctx);

        match self.ctx.mode {
            EditorMode::Tracker => {
                self.tracker.draw(egui_ctx);
            }
            EditorMode::Modeler => {
                self.modeler.draw(egui_ctx);
            }
            EditorMode::WorldEditor => {
                self.world_editor.draw(egui_ctx, &mut self.ctx);
            }
            _ => {
                // Project / Script / Test modes: content browser + inspector
                self.content_browser.draw(egui_ctx, &mut self.ctx);
                self.inspector.draw(egui_ctx, &mut self.ctx);
                self.viewport.draw(egui_ctx, &mut self.ctx);
            }
        }
    }

    /// Process any pending editor actions.
    pub fn process_actions(&mut self) {
        let Some(action) = self.ctx.take_action() else {
            return;
        };

        match action {
            EditorAction::NewProject => {
                self.create_default_project();
            }
            EditorAction::OpenProject(path) => {
                match crate::project::Project::open(path) {
                    Ok(project) => {
                        self.ctx.project = Some(project);
                        self.ctx.mode = EditorMode::WorldEditor;
                    }
                    Err(e) => {
                        log::error!("Failed to open project: {}", e);
                    }
                }
            }
            EditorAction::SaveProject => {
                if let Some(project) = self.ctx.project.as_ref() {
                    if let Err(e) = project.save() {
                        log::error!("Failed to save project: {}", e);
                    }
                }
            }
            EditorAction::ImportAsset(path) => {
                self.import_asset(&path);
            }
            EditorAction::OpenAsset(handle) => {
                self.open_asset(handle);
            }
            EditorAction::SwitchMode(mode) => {
                self.ctx.mode = mode;
            }
            EditorAction::NewLevel => {
                self.new_level();
            }
            EditorAction::SaveLevel => {
                self.save_level();
            }
            EditorAction::NewSong => {
                self.tracker.state.new_song();
            }
            EditorAction::OpenSong(path) => {
                if let Err(e) = self.tracker.state.load_song(&path) {
                    log::error!("Failed to load song: {}", e);
                }
            }
            EditorAction::SaveSong => {
                if let Err(e) = self.tracker.state.save_song() {
                    log::error!("Failed to save song: {}", e);
                }
            }
            EditorAction::SaveSongAs(path) => {
                self.tracker.state.current_file = Some(path);
                if let Err(e) = self.tracker.state.save_song() {
                    log::error!("Failed to save song: {}", e);
                }
            }
        }
    }

    /// Render the 3D scene to the active framebuffer
    pub fn render_3d(&mut self, dt: f32, rotation: &mut f32) {
        match self.ctx.mode {
            EditorMode::Modeler => {
                self.modeler.render_frame(dt);
            }
            _ => {
                self.viewport.render_frame(dt, rotation);
            }
        }
    }

    /// Get the active framebuffer for GPU upload
    pub fn active_framebuffer(&self) -> &crate::rasterizer::Framebuffer {
        match self.ctx.mode {
            EditorMode::Modeler => &self.modeler.framebuffer,
            _ => &self.viewport.framebuffer,
        }
    }

    /// Tick tracker playback
    pub fn tick(&mut self, dt: f64) {
        self.tracker.tick(dt);
    }

    fn import_asset(&mut self, path: &std::path::Path) {
        let Some(project) = self.ctx.project.as_mut() else { return };

        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let Some(asset_type) = AssetType::from_extension(ext) else {
            log::warn!("Unknown file type: .{}", ext);
            return;
        };

        match project.assets.import(path, asset_type) {
            Ok(handle) => {
                log::info!("Imported asset: {:?} -> {}", path.file_name().unwrap_or_default(), handle);
                if let Err(e) = project.save() {
                    log::error!("Failed to save project after import: {}", e);
                }
            }
            Err(e) => {
                log::error!("Failed to import {:?}: {}", path, e);
            }
        }
    }

    fn new_level(&mut self) {
        use crate::rasterizer::Vec3;
        use crate::scene::{Room, Level, Sector, TextureRef};

        let mut room = Room::new(0, Vec3::new(0.0, 0.0, 0.0), 4, 4);
        let floor_tex = TextureRef::new("_DEFAULT", "checkerboard");
        for x in 0..4 {
            for z in 0..4 {
                room.set_sector(x, z, Sector::with_floor(0.0, floor_tex.clone()));
            }
        }

        let mut level = Level::new();
        level.rooms.push(room);

        self.viewport.rebuild_from_level(&level);
        self.ctx.current_level = Some(level);
        self.ctx.mode = EditorMode::WorldEditor;
    }

    fn save_level(&mut self) {
        let Some(level) = &self.ctx.current_level else { return };
        let Some(project) = &self.ctx.project else { return };

        let levels_dir = project.root().join("levels");
        std::fs::create_dir_all(&levels_dir).ok();

        // Use a default name if no file is associated yet
        let path = levels_dir.join("level.ron");

        match crate::scene::save_level(level, &path) {
            Ok(()) => {
                log::info!("Level saved to {:?}", path);
            }
            Err(e) => {
                log::error!("Failed to save level: {}", e);
            }
        }
    }

    fn open_asset(&mut self, handle: AssetHandle) {
        let Some(project) = self.ctx.project.as_ref() else { return };
        let Some(entry) = project.assets.registry.get(&handle) else { return };
        let asset_type = entry.asset_type;

        match asset_type {
            AssetType::Level => {
                if let Some(path) = project.assets.resolve_path(&handle) {
                    match crate::scene::load_level(&path) {
                        Ok(level) => {
                            self.viewport.rebuild_from_level(&level);
                            self.ctx.current_level = Some(level);
                            self.ctx.mode = EditorMode::WorldEditor;
                        }
                        Err(e) => {
                            log::error!("Failed to load level: {}", e);
                        }
                    }
                }
            }
            AssetType::Song => {
                self.ctx.mode = EditorMode::Tracker;
            }
            AssetType::Model => {
                self.ctx.mode = EditorMode::Modeler;
            }
            AssetType::Script => {
                self.ctx.mode = EditorMode::ScriptEditor;
            }
            _ => {}
        }
    }

    fn create_default_project(&mut self) {
        let home = dirs_next().unwrap_or_else(|| std::path::PathBuf::from("."));
        let project_root = home.join("bonnie-32-projects").join("New Project");

        match crate::project::Project::create(project_root, "New Project") {
            Ok(mut project) => {
                let bundled_path = std::path::PathBuf::from("assets/samples");
                if bundled_path.exists() {
                    project.register_bundled_assets(&bundled_path);
                }
                project.save().ok();
                self.ctx.project = Some(project);
                self.ctx.mode = EditorMode::WorldEditor;
            }
            Err(e) => {
                log::error!("Failed to create project: {}", e);
            }
        }
    }
}

fn dirs_next() -> Option<std::path::PathBuf> {
    std::env::var("HOME").ok().map(std::path::PathBuf::from)
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}
