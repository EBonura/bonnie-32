pub mod context;
pub mod toolbar;
pub mod content_browser;
pub mod hierarchy;
pub mod inspector;
pub mod viewport;
pub mod tracker;

use context::{EditorAction, EditorContext, EditorMode};
use content_browser::ContentBrowser;
use hierarchy::HierarchyPanel;
use inspector::InspectorPanel;
use viewport::ViewportPanel;
use tracker::TrackerPanel;

pub struct Editor {
    pub ctx: EditorContext,
    content_browser: ContentBrowser,
    hierarchy: HierarchyPanel,
    inspector: InspectorPanel,
    pub viewport: ViewportPanel,
    tracker: TrackerPanel,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            ctx: EditorContext::new(),
            content_browser: ContentBrowser::new(),
            hierarchy: HierarchyPanel::new(),
            inspector: InspectorPanel::new(),
            viewport: ViewportPanel::new(),
            tracker: TrackerPanel::new(),
        }
    }

    /// Draw all editor UI. Call this inside egui_ctx.run().
    pub fn draw(&mut self, egui_ctx: &egui::Context) {
        toolbar::draw_toolbar(egui_ctx, &mut self.ctx);

        match self.ctx.mode {
            EditorMode::Tracker => {
                self.tracker.draw(egui_ctx);
            }
            _ => {
                // World editor / other modes
                self.content_browser.draw(egui_ctx, &mut self.ctx);
                self.hierarchy.draw(egui_ctx, &mut self.ctx);
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
            EditorAction::OpenAsset(_handle) => {
                // TODO: navigate to appropriate editor for asset type
            }
            EditorAction::SwitchMode(mode) => {
                self.ctx.mode = mode;
            }
        }
    }

    /// Tick tracker playback
    pub fn tick(&mut self, dt: f64) {
        self.tracker.tick(dt);
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
