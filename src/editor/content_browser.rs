//! Content Browser — the always-present home tab.
//!
//! Two modes:
//!   - No project: welcome screen with New/Open buttons and recent list.
//!   - Project open: type-filter sidebar + search + asset list.
//!
//! Assets are discovered by scanning the project's subdirectories — no registry.
//! Double-clicking an asset opens the appropriate editor tab.

use std::path::PathBuf;
use crate::app::{AppState, Tab};
use crate::asset::AssetType;
use crate::store::projects_root;
use super::icons::icon;
use super::radial_menu::{RadialItem, WheelOut, WheelSession};

// ---------------------------------------------------------------------------
// Type filter
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypeFilter {
    All,
    Levels,
    Models,
    Textures,
    Songs,
    Scripts,
}

impl TypeFilter {
    fn all() -> &'static [TypeFilter] {
        &[Self::All, Self::Levels, Self::Models, Self::Textures, Self::Songs, Self::Scripts]
    }

    fn label(self) -> &'static str {
        match self {
            Self::All      => "All",
            Self::Levels   => "Levels",
            Self::Models   => "Models",
            Self::Textures => "Textures",
            Self::Songs    => "Songs",
            Self::Scripts  => "Scripts",
        }
    }

    fn icon(self) -> char {
        match self {
            Self::All      => icon::LAYERS,
            Self::Levels   => icon::GLOBE,
            Self::Models   => icon::BOX,
            Self::Textures => icon::PALETTE,
            Self::Songs    => icon::MUSIC,
            Self::Scripts  => icon::BOOK_OPEN,
        }
    }

    fn matches(self, asset_type: AssetType) -> bool {
        match self {
            Self::All      => true,
            Self::Levels   => asset_type == AssetType::Level,
            Self::Models   => asset_type == AssetType::Model,
            Self::Textures => matches!(asset_type, AssetType::Texture | AssetType::TexturePack),
            Self::Songs    => asset_type == AssetType::Song,
            Self::Scripts  => asset_type == AssetType::Script,
        }
    }
}

// ---------------------------------------------------------------------------
// ContentBrowserPanel
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq)]
enum ContentAction {
    NewLevel,
    NewSong,
    NewModel,
    NewScript,
    Import,
}

pub struct ContentBrowserPanel {
    filter: TypeFilter,
    search: String,
    selected: Option<PathBuf>,

    // New-level dialog
    show_new_level_dialog: bool,
    new_level_name: String,

    // New-project dialog (shown after folder is picked)
    pending_project_dir: Option<PathBuf>,
    new_project_name: String,

    // Content creation wheel
    content_wheel: Option<WheelSession<ContentAction>>,
}

impl ContentBrowserPanel {
    pub fn new() -> Self {
        Self {
            filter: TypeFilter::All,
            search: String::new(),
            selected: None,
            show_new_level_dialog: false,
            new_level_name: String::new(),
            pending_project_dir: None,
            new_project_name: String::new(),
            content_wheel: None,
        }
    }

    pub fn draw(&mut self, egui_ctx: &egui::Context, state: &mut AppState) {
        self.draw_new_level_dialog(egui_ctx, state);
        self.draw_new_project_dialog(egui_ctx, state);

        egui::CentralPanel::default().show(egui_ctx, |ui| {
            self.draw_content(ui, state);
        });
    }

    /// Inline variant — draws the content directly into `ui` (for split panes).
    /// Dialogs (New Level, New Project) are still rooted at the egui context via `ui.ctx()`.
    pub fn draw_content(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let ctx = ui.ctx().clone();
        self.draw_new_level_dialog(&ctx, state);
        self.draw_new_project_dialog(&ctx, state);

        if state.store.has_project() {
            self.draw_project_view(ui, state);
        } else {
            self.draw_welcome_view(ui, state);
        }

        // Content wheel floats above browser contents
        self.tick_content_wheel(&ctx, state);
    }

    // ---- Welcome (no project) -----------------------------------------------

    fn draw_welcome_view(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
            ui.add_space(60.0);
            ui.label(
                egui::RichText::new("BONNIE-32")
                    .size(36.0)
                    .strong()
                    .color(egui::Color32::from_rgb(200, 200, 220)),
            );
            ui.label(
                egui::RichText::new("PS1-era fantasy console")
                    .size(14.0)
                    .color(egui::Color32::from_rgb(120, 120, 140)),
            );
            ui.add_space(40.0);

            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() - 280.0).max(0.0) / 2.0);
                let btn = egui::vec2(130.0, 40.0);
                if ui.add_sized(btn, egui::Button::new("New Project")).clicked() {
                    self.open_new_project_dialog();
                }
                ui.add_space(20.0);
                if ui.add_sized(btn, egui::Button::new("Open Project")).clicked() {
                    self.open_project_file_dialog(state);
                }
            });

            ui.add_space(40.0);
            ui.separator();
            ui.add_space(16.0);

            let recents: Vec<_> = state.store.recent_projects.clone();
            if recents.is_empty() {
                ui.label(
                    egui::RichText::new("No recent projects")
                        .color(egui::Color32::from_rgb(80, 80, 95)),
                );
            } else {
                ui.label(
                    egui::RichText::new("Recent Projects")
                        .size(13.0)
                        .color(egui::Color32::from_rgb(140, 140, 160)),
                );
                ui.add_space(8.0);
                for recent in &recents {
                    let exists = recent.path.exists();
                    ui.horizontal(|ui| {
                        let label = egui::RichText::new(&recent.name).color(
                            if exists { egui::Color32::from_rgb(180, 200, 220) }
                            else      { egui::Color32::from_rgb(90, 90, 105)  }
                        );
                        let resp = ui.add(egui::Label::new(label).sense(egui::Sense::click()));
                        ui.label(
                            egui::RichText::new(recent.path.to_string_lossy().as_ref())
                                .size(11.0)
                                .color(if exists {
                                    egui::Color32::from_rgb(80, 80, 95)
                                } else {
                                    egui::Color32::from_rgb(130, 60, 60)
                                }),
                        );
                        if resp.clicked() && exists {
                            let path = recent.path.clone();
                            if let Err(e) = state.store.open_project(path) {
                                log::error!("Failed to open recent project: {}", e);
                            } else {
                                state.open_tab(Tab::ProjectComposer);
                            }
                        }
                    });
                    ui.add_space(4.0);
                }
            }
        });
    }

    // ---- Project view -------------------------------------------------------

    fn draw_project_view(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.horizontal(|ui| {
            let name = state.store.project_name().unwrap_or("Project").to_string();
            ui.label(
                egui::RichText::new(&name)
                    .strong()
                    .color(egui::Color32::from_rgb(180, 200, 220)),
            );
            ui.add_space(16.0);
            ui.label("Search:");
            ui.text_edit_singleline(&mut self.search);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Close").on_hover_text("Close project").clicked() {
                    state.store.close_project();
                    state.close_all_editors = true;
                }
                let plus_resp = ui.button(
                    egui::RichText::new(icon::PLUS.to_string()).size(15.0)
                ).on_hover_text("Create new asset  (+)");
                if plus_resp.clicked() || (!ui.ctx().wants_keyboard_input()
                    && ui.ctx().input(|i| i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals)))
                {
                    if self.content_wheel.is_none() {
                        self.content_wheel = Some(WheelSession::open(
                            ui.ctx(), "content_wheel", Self::content_wheel_items(),
                        ));
                    }
                }
            });
        });

        ui.separator();

        ui.horizontal(|ui| {
            // Left sidebar: type filter
            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                ui.set_width(100.0);
                ui.add_space(4.0);
                let icon_font = egui::FontId::new(14.0, egui::FontFamily::Name("lucide".into()));
                for &f in TypeFilter::all() {
                    let active = self.filter == f;
                    let color = if active {
                        egui::Color32::from_rgb(80, 160, 230)
                    } else {
                        egui::Color32::from_rgb(140, 140, 160)
                    };
                    let resp = ui.horizontal(|ui| {
                        ui.add(egui::Label::new(
                            egui::RichText::new(f.icon().to_string())
                                .font(icon_font.clone())
                                .color(color),
                        ).sense(egui::Sense::hover()));
                        ui.add(egui::Label::new(
                            egui::RichText::new(f.label()).color(color),
                        ).sense(egui::Sense::click()))
                    }).inner;
                    if resp.clicked() { self.filter = f; }
                    ui.add_space(2.0);
                }
            });

            ui.separator();

            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                self.draw_asset_list(ui, state);
            });
        });
    }

    fn draw_asset_list(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let search_lower = self.search.to_lowercase();
        let filter = self.filter;

        // Collect all matching assets by scanning project directories
        let mut assets: Vec<(PathBuf, AssetType)> = Vec::new();

        if let Some(project) = &state.store.project {
            for &asset_type in AssetType::all() {
                if !filter.matches(asset_type) { continue; }
                for path in project.scan(asset_type) {
                    let name = path.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("");
                    if !search_lower.is_empty()
                        && !name.to_lowercase().contains(&search_lower)
                    {
                        continue;
                    }
                    assets.push((path, asset_type));
                }
            }
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            if assets.is_empty() {
                ui.label(
                    egui::RichText::new("No assets yet. Use the + buttons to create some.")
                        .weak(),
                );
                return;
            }
            for (path, asset_type) in &assets {
                self.draw_asset_row(ui, state, path, *asset_type);
            }
        });
    }

    fn draw_asset_row(
        &mut self,
        ui: &mut egui::Ui,
        state: &mut AppState,
        path: &PathBuf,
        asset_type: AssetType,
    ) {
        let selected = self.selected.as_deref() == Some(path);
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("?");

        let icon_ch  = type_icon(asset_type);
        let icon_font = egui::FontId::new(13.0, egui::FontFamily::Name("lucide".into()));

        let resp = ui.horizontal(|ui| {
            ui.add(egui::Label::new(
                egui::RichText::new(icon_ch.to_string())
                    .font(icon_font)
                    .color(egui::Color32::from_rgb(120, 140, 160)),
            ).sense(egui::Sense::hover()));
            let label_color = if selected {
                egui::Color32::from_rgb(100, 180, 255)
            } else {
                egui::Color32::from_rgb(200, 200, 210)
            };
            let r = ui.add(egui::Label::new(
                egui::RichText::new(name).color(label_color),
            ).sense(egui::Sense::click()));
            ui.add(egui::Label::new(
                egui::RichText::new(asset_type.label())
                    .size(11.0)
                    .color(egui::Color32::from_rgb(100, 100, 120)),
            ).sense(egui::Sense::hover()));
            r
        }).inner;

        if resp.clicked()        { self.selected = Some(path.clone()); }
        if resp.double_clicked() { self.open_asset(state, path.clone(), asset_type); }
    }

    // ---- Open asset --------------------------------------------------------

    fn open_asset(&self, state: &mut AppState, path: PathBuf, asset_type: AssetType) {
        match asset_type {
            AssetType::Level => {
                if let Err(e) = state.store.open_level(path.clone()) {
                    log::error!("Failed to open level: {}", e);
                    return;
                }
                state.open_tab(Tab::LevelEditor { path, last_seen_version: 0 });
            }
            AssetType::Song   => { state.open_tab(Tab::MusicTracker { path }); }
            AssetType::Model  => { state.open_tab(Tab::AssetEditor  { path }); }
            AssetType::Script => { state.open_tab(Tab::ScriptEditor  { path }); }
            _ => {}
        }
    }

    // ---- Dialogs -----------------------------------------------------------

    fn draw_new_level_dialog(&mut self, egui_ctx: &egui::Context, state: &mut AppState) {
        if !self.show_new_level_dialog { return; }

        let mut confirmed = false;
        let mut cancelled = false;

        egui::Window::new("New Level")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(egui_ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    let resp = ui.text_edit_singleline(&mut self.new_level_name);
                    if resp.lost_focus()
                        && egui_ctx.input(|i| i.key_pressed(egui::Key::Enter))
                    {
                        confirmed = true;
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Create").clicked() { confirmed = true; }
                    if ui.button("Cancel").clicked() { cancelled = true; }
                });
            });

        if confirmed && !self.new_level_name.is_empty() {
            let name = self.new_level_name.clone();
            if let Some(path) = state.store.create_level(&name) {
                state.open_tab(Tab::LevelEditor { path, last_seen_version: 0 });
            }
            self.show_new_level_dialog = false;
            self.new_level_name.clear();
        } else if cancelled {
            self.show_new_level_dialog = false;
            self.new_level_name.clear();
        }
    }

    fn draw_new_project_dialog(&mut self, egui_ctx: &egui::Context, state: &mut AppState) {
        if self.pending_project_dir.is_none() { return; }

        let dir_label = self.pending_project_dir
            .as_ref()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default();

        let mut confirmed = false;
        let mut cancelled = false;

        egui::Window::new("New Project")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(egui_ctx, |ui| {
                ui.label(
                    egui::RichText::new(&dir_label)
                        .size(11.0)
                        .color(egui::Color32::from_rgb(120, 120, 140)),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Project name:");
                    let resp = ui.text_edit_singleline(&mut self.new_project_name);
                    if resp.lost_focus()
                        && egui_ctx.input(|i| i.key_pressed(egui::Key::Enter))
                    {
                        confirmed = true;
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Create").clicked() { confirmed = true; }
                    if ui.button("Cancel").clicked() { cancelled = true; }
                });
            });

        if confirmed && !self.new_project_name.is_empty() {
            if let Some(dir) = self.pending_project_dir.take() {
                let name = self.new_project_name.clone();
                let root = dir.join(&name);
                if let Err(e) = state.store.create_project(root, &name) {
                    log::error!("Failed to create project: {}", e);
                } else {
                    state.open_tab(Tab::ProjectComposer);
                }
            }
            self.new_project_name.clear();
        } else if cancelled {
            self.pending_project_dir = None;
            self.new_project_name.clear();
        }
    }

    // ---- File dialogs -------------------------------------------------------

    fn open_new_project_dialog(&mut self) {
        let start = projects_root();
        if let Some(dir) = rfd::FileDialog::new()
            .set_title("Choose project location")
            .set_directory(&start)
            .pick_folder()
        {
            let name = dir.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("New Project")
                .to_string();
            self.new_project_name = name;
            self.pending_project_dir = Some(dir);
        }
    }

    fn open_project_file_dialog(&self, state: &mut AppState) {
        let start = projects_root();
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Open project")
            .set_directory(&start)
            .add_filter("Bonnie-32 project", &["b32"])
            .pick_file()
        {
            if let Err(e) = state.store.open_project(path) {
                log::error!("Failed to open project: {}", e);
            } else {
                state.open_tab(Tab::ProjectComposer);
            }
        }
    }

    // ---- Content wheel -----------------------------------------------------

    fn content_wheel_items() -> Vec<RadialItem<ContentAction>> {
        vec![
            RadialItem::new(icon::GLOBE,    "New Level",  ContentAction::NewLevel),
            RadialItem::new(icon::MUSIC,    "New Song",   ContentAction::NewSong),
            RadialItem::new(icon::BOX,      "New Model",  ContentAction::NewModel),
            RadialItem::new(icon::PENCIL,   "New Script", ContentAction::NewScript),
            RadialItem::new(icon::DOWNLOAD, "Import",     ContentAction::Import),
        ]
    }

    fn tick_content_wheel(&mut self, ctx: &egui::Context, state: &mut AppState) {
        let Some(session) = self.content_wheel.as_mut() else { return };
        match session.show(ctx) {
            WheelOut::Open => {}
            WheelOut::Dismissed => { self.content_wheel = None; }
            WheelOut::Selected(action) => {
                self.content_wheel = None;
                self.handle_content_action(action, state);
            }
        }
    }

    fn handle_content_action(&mut self, action: ContentAction, state: &mut AppState) {
        match action {
            ContentAction::NewLevel => {
                self.new_level_name = "New Level".to_string();
                self.show_new_level_dialog = true;
            }
            ContentAction::NewSong => {
                // New song: open tracker with a blank song
                state.open_tab(Tab::MusicTracker {
                    path: state.store.project.as_ref()
                        .map(|p| p.root().join("songs").join("new_song.ron"))
                        .unwrap_or_default(),
                });
            }
            ContentAction::NewModel => {
                state.open_tab(Tab::AssetEditor {
                    path: state.store.project.as_ref()
                        .map(|p| p.root().join("models").join("new_model.ron"))
                        .unwrap_or_default(),
                });
            }
            ContentAction::NewScript => {
                state.open_tab(Tab::ScriptEditor {
                    path: state.store.project.as_ref()
                        .map(|p| p.root().join("scripts").join("new_script.lua"))
                        .unwrap_or_default(),
                });
            }
            ContentAction::Import => {
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Import Asset")
                    .add_filter("All Supported", &["obj", "ron", "lua", "png", "jpg", "bmp"])
                    .pick_file()
                {
                    log::info!("Import: {:?}", path);
                    // TODO: copy into project assets dir and refresh
                }
            }
        }
    }
}

impl Default for ContentBrowserPanel {
    fn default() -> Self { Self::new() }
}

fn type_icon(asset_type: AssetType) -> char {
    match asset_type {
        AssetType::Level                    => icon::GLOBE,
        AssetType::Model                    => icon::BOX,
        AssetType::Texture | AssetType::TexturePack => icon::PALETTE,
        AssetType::Song                     => icon::MUSIC,
        AssetType::Script                   => icon::BOOK_OPEN,
    }
}
