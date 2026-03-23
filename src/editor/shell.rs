//! Shell — top-level application UI.
//!
//! Uses egui_dock for VS Code-style draggable/splittable tabs.
//! ShellViewer implements TabViewer and owns all editor panels.
//! The Project Wheel (` key) floats over everything.

use std::path::PathBuf;

use egui_dock::{DockArea, DockState, TabViewer};

use crate::app::{AppState, EditorAction, Tab};
use crate::config::AppConfig;
use crate::scene::Level;

use super::about::AboutPanel;
use super::content_browser::ContentBrowserPanel;
use super::level_edit::LevelEditState;
use super::hierarchy::HierarchyPanel;
use super::inspector::InspectorPanel;
use super::modeler::ModelerPanel;
use super::radial_menu::{RadialItem, WheelOut, WheelSession};
use super::tracker::TrackerPanel;
use super::viewport::ViewportPanel;
use super::world_editor::WorldEditorPanel;
use super::icons::icon;

// ---------------------------------------------------------------------------
// Project wheel actions
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq)]
enum ProjectAction {
    NewProject,
    OpenProject,
    SaveProject,
    CloseProject,
}

fn project_wheel_items(has_project: bool) -> Vec<RadialItem<ProjectAction>> {
    vec![
        RadialItem::new(icon::FILE_PLUS,   "New",   ProjectAction::NewProject),
        RadialItem::new(icon::FOLDER_OPEN, "Open",  ProjectAction::OpenProject),
        if has_project {
            RadialItem::new(icon::SAVE,    "Save",  ProjectAction::SaveProject)
        } else {
            RadialItem::new(icon::SAVE,    "Save",  ProjectAction::SaveProject).disabled()
        },
        if has_project {
            RadialItem::new(icon::CIRCLE_X, "Close", ProjectAction::CloseProject)
        } else {
            RadialItem::new(icon::CIRCLE_X, "Close", ProjectAction::CloseProject).disabled()
        },
    ]
}

// ---------------------------------------------------------------------------
// ShellViewer — owns all panels, implements egui_dock::TabViewer
// ---------------------------------------------------------------------------

pub struct ShellViewer {
    pub state: AppState,

    pub level_edit: LevelEditState,
    pub viewport: ViewportPanel,
    pub world_editor: WorldEditorPanel,
    hierarchy: HierarchyPanel,
    inspector: InspectorPanel,
    modeler: ModelerPanel,
    tracker: TrackerPanel,

    about: AboutPanel,
    content_browser: ContentBrowserPanel,

    loaded_level_path: Option<PathBuf>,

    /// Active tab from the last frame (used by render_3d / process_actions).
    pub focused_tab: Option<Tab>,

    /// Set by on_add (the tab-bar + button) to open the project wheel next frame.
    pub pending_project_wheel: bool,
}

impl ShellViewer {
    fn new() -> Self {
        Self {
            state: AppState::new(),
            level_edit: LevelEditState::new(),
            viewport: ViewportPanel::new(),
            world_editor: WorldEditorPanel::new(),
            hierarchy: HierarchyPanel::new(),
            inspector: InspectorPanel::new(),
            modeler: ModelerPanel::new(),
            tracker: TrackerPanel::new(),
            about: AboutPanel::new(),
            content_browser: ContentBrowserPanel::new(),
            loaded_level_path: None,
            focused_tab: None,
            pending_project_wheel: false,
        }
    }

    fn sync_ctx_from_tab(&mut self, tab: &Tab) {
        let active_path = match tab {
            Tab::LevelEditor { path, .. } => Some(path.clone()),
            _ => None,
        };

        if active_path != self.loaded_level_path {
            if let Some(ref p) = active_path {
                if let Some(level) = self.state.store.get_level(p) {
                    let cloned = level.clone();
                    self.level_edit.current_level = Some(cloned);
                    self.level_edit.level_undo.clear();
                    self.level_edit.level_dirty = false;
                    self.rebuild_viewport_from_ctx();
                    if let Some(l) = self.level_edit.current_level.as_ref() {
                        self.world_editor.center_on_level(l);
                    }
                    self.world_editor.selected_face   = None;
                    self.world_editor.hovered_face    = None;
                    self.world_editor.selected_sector = None;
                    self.world_editor.hovered_sector  = None;
                }
            }
            self.loaded_level_path = active_path;
        }
    }

    fn sync_world_editor_rebuild(&mut self) {
        if self.world_editor.needs_viewport_rebuild {
            self.world_editor.needs_viewport_rebuild = false;
            if let Some(level) = self.level_edit.current_level.clone() {
                self.rebuild_viewport_from_level(&level);
            }
        }
    }

    fn rebuild_viewport_from_ctx(&mut self) {
        if let Some(level) = self.level_edit.current_level.clone() {
            self.rebuild_viewport_from_level(&level);
        }
    }

    fn rebuild_viewport_from_level(&mut self, level: &Level) {
        let root = self.state.store.project.as_ref().map(|p| p.root().to_path_buf());
        self.viewport.rebuild_from_level_with_textures(level, root.as_deref());
    }

    fn flush_level_to_store(&mut self) {
        if self.level_edit.level_dirty {
            if let (Some(path), Some(level)) =
                (self.loaded_level_path.clone(), self.level_edit.current_level.clone())
            {
                if let Some(stored) = self.state.store.mutate_level(&path) {
                    *stored = level;
                }
            }
        }
    }

    fn add_room(&mut self) {
        use crate::rasterizer::Vec3;
        use crate::scene::{Room, TextureRef};

        if self.level_edit.current_level.is_none() { return; }
        self.level_edit.push_level_undo();
        let Some(level) = self.level_edit.current_level.as_mut() else { return };
        let id = level.rooms.len();
        let offset_x = id as f32 * 4.0 * crate::scene::SECTOR_SIZE;
        let mut room = Room::new(id, Vec3::new(offset_x, 0.0, 0.0), 4, 4);
        let tex = TextureRef::new("_DEFAULT", "checkerboard");
        for x in 0..4 { for z in 0..4 {
            room.set_sector(x, z, crate::scene::Sector::with_floor(0.0, tex.clone()));
        }}
        level.rooms.push(room);
        let snap = level.clone();
        self.rebuild_viewport_from_level(&snap);
    }
}

// ---------------------------------------------------------------------------
// TabViewer
// ---------------------------------------------------------------------------

impl TabViewer for ShellViewer {
    type Tab = Tab;

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        tab.title(&self.state.store).into()
    }

    fn closeable(&mut self, tab: &mut Tab) -> bool {
        tab.can_close()
    }

    fn on_add(&mut self, _surface: egui_dock::SurfaceIndex, _node: egui_dock::NodeIndex) {
        self.pending_project_wheel = true;
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        self.focused_tab = Some(tab.clone());
        self.sync_ctx_from_tab(tab);

        match tab {
            Tab::About => {
                self.about.draw_content(ui);
            }
            Tab::ContentBrowser => {
                self.content_browser.draw_content(ui, &mut self.state);
            }
            Tab::ProjectComposer => {
                ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                    ui.add_space(60.0);
                    let name = self.state.store.project_name().unwrap_or("Project").to_string();
                    ui.label(
                        egui::RichText::new(name)
                            .size(24.0).strong()
                            .color(egui::Color32::from_rgb(200, 200, 220)),
                    );
                    ui.add_space(16.0);
                    ui.label(
                        egui::RichText::new("Project Composer — level flow graph coming soon")
                            .color(egui::Color32::from_rgb(100, 100, 120)),
                    );
                });
            }
            Tab::LevelEditor { .. } => {
                self.world_editor.draw_central_inside(ui, &mut self.level_edit, &mut self.state);
                self.flush_level_to_store();
                self.sync_world_editor_rebuild();
            }
            Tab::AssetEditor { .. } => {
                self.modeler.draw_inside(ui);
            }
            Tab::MusicTracker { .. } => {
                self.tracker.draw_inside(ui);
            }
            Tab::ScriptEditor { .. } => {
                ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                    ui.add_space(80.0);
                    ui.label(
                        egui::RichText::new("Script Editor")
                            .size(20.0).strong()
                            .color(egui::Color32::from_rgb(180, 180, 200)),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("Lua scripting — coming soon")
                            .color(egui::Color32::from_rgb(100, 100, 120)),
                    );
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Shell
// ---------------------------------------------------------------------------

pub struct Shell {
    dock_state: DockState<Tab>,
    viewer: ShellViewer,
    config: AppConfig,
    project_wheel: Option<WheelSession<ProjectAction>>,
}

impl Shell {
    pub fn new() -> Self {
        Self {
            dock_state: DockState::new(vec![Tab::About, Tab::ContentBrowser]),
            viewer: ShellViewer::new(),
            config: AppConfig::load(),
            project_wheel: None,
        }
    }

    pub fn draw(&mut self, egui_ctx: &egui::Context) {
        self.handle_global_shortcuts(egui_ctx);
        self.process_pending_tabs();

        // Level editor global panels (hierarchy, inspector, toolbars) must be
        // allocated before DockArea consumes the central area.
        if let Some(Tab::LevelEditor { .. }) = &self.viewer.focused_tab {
            self.viewer.world_editor.draw_panels(egui_ctx, &mut self.viewer.level_edit, &mut self.viewer.state);
            self.viewer.hierarchy.draw(egui_ctx, &self.viewer.level_edit, &mut self.viewer.state);
            self.viewer.inspector.draw(egui_ctx, &self.viewer.level_edit, &mut self.viewer.state);
        }

        DockArea::new(&mut self.dock_state)
            .show_leaf_collapse_buttons(false)
            .show_add_buttons(true)
            .show_add_popup(false)
            .show(egui_ctx, &mut self.viewer);

        // Drain the flag set by on_add (the + button in the tab bar).
        if self.viewer.pending_project_wheel {
            self.viewer.pending_project_wheel = false;
            self.open_project_wheel(egui_ctx);
        }

        self.draw_project_wheel(egui_ctx);
    }

    pub fn render_3d(&mut self, dt: f32, rotation: &mut f32) {
        match &self.viewer.focused_tab.clone().unwrap_or(Tab::About) {
            Tab::LevelEditor { .. } => {
                self.viewer.sync_world_editor_rebuild();
                let level = self.viewer.level_edit.current_level.as_ref();
                self.viewer.world_editor.render_frame(&mut self.viewer.viewport, level);
            }
            Tab::AssetEditor { .. } => {
                self.viewer.modeler.render_frame(dt);
            }
            _ => {
                self.viewer.viewport.render_frame(dt, rotation);
            }
        }
    }

    pub fn active_framebuffer(&self) -> &crate::rasterizer::Framebuffer {
        match &self.viewer.focused_tab {
            Some(Tab::AssetEditor { .. }) => &self.viewer.modeler.framebuffer,
            _ => &self.viewer.viewport.framebuffer,
        }
    }

    pub fn tick(&mut self, dt: f64) {
        self.viewer.tracker.tick(dt);
    }

    pub fn process_actions(&mut self) {
        let Some(action) = self.viewer.state.take_action() else { return };
        self.handle_action(action);
    }

    // ---- Project wheel -----------------------------------------------------

    fn draw_project_wheel(&mut self, ctx: &egui::Context) {
        let Some(session) = self.project_wheel.as_mut() else { return };

        match session.show(ctx) {
            WheelOut::Open => {}
            WheelOut::Dismissed => {
                self.project_wheel = None;
            }
            WheelOut::Selected(action) => {
                self.project_wheel = None;
                self.handle_project_action(action, ctx);
            }
        }
    }

    fn handle_project_action(&mut self, action: ProjectAction, _ctx: &egui::Context) {
        match action {
            ProjectAction::NewProject => {
                self.viewer.state.request_action(EditorAction::NewProject);
            }
            ProjectAction::OpenProject => {
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Open Project")
                    .add_filter("Bonnie-32 Project", &["b32"])
                    .pick_file()
                {
                    self.config.push_recent(path.clone());
                    self.viewer.state.request_action(EditorAction::OpenProject(path));
                }
            }
            ProjectAction::SaveProject => {
                self.viewer.state.request_action(EditorAction::SaveProject);
            }
            ProjectAction::CloseProject => {
                self.viewer.state.store.close_project();
                self.viewer.state.close_all_editors = true;
            }
        }
    }

    // ---- Pending tab management --------------------------------------------

    fn process_pending_tabs(&mut self) {
        if self.viewer.state.close_all_editors {
            self.viewer.state.close_all_editors = false;
            self.dock_state.retain_tabs(|t| !t.can_close());
        }

        for tab in self.viewer.state.pending_tabs.drain(..).collect::<Vec<_>>() {
            if let Some(pos) = self.dock_state.find_tab(&tab) {
                self.dock_state.set_active_tab(pos);
            } else {
                self.dock_state.push_to_first_leaf(tab);
            }
        }
    }

    // ---- Project wheel helpers --------------------------------------------

    fn open_project_wheel(&mut self, ctx: &egui::Context) {
        if self.project_wheel.is_some() { return; }
        let has_project = self.viewer.state.store.has_project();
        let items = project_wheel_items(has_project);
        self.project_wheel = Some(WheelSession::open(ctx, "project_wheel", items));
    }

    // ---- Global shortcuts --------------------------------------------------

    fn handle_global_shortcuts(&mut self, egui_ctx: &egui::Context) {
        // Backtick opens the project wheel (Tab is consumed by egui for focus traversal).
        let backtick = egui_ctx.input(|i| i.key_pressed(egui::Key::Backtick));
        if backtick { self.open_project_wheel(egui_ctx); }

        if egui_ctx.wants_keyboard_input() { return; }

        egui_ctx.input(|i| {
            if i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::Z) {
                self.viewer.state.request_action(EditorAction::Undo);
            }
            if i.modifiers.command
                && (i.key_pressed(egui::Key::Y)
                    || (i.modifiers.shift && i.key_pressed(egui::Key::Z)))
            {
                self.viewer.state.request_action(EditorAction::Redo);
            }
            if i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::S) {
                match &self.viewer.focused_tab {
                    Some(Tab::MusicTracker { .. }) => {
                        self.viewer.state.request_action(EditorAction::SaveSong);
                    }
                    Some(Tab::LevelEditor { path, .. }) => {
                        let path = path.clone();
                        if let Some(level) = &self.viewer.level_edit.current_level {
                            let level = level.clone();
                            if let Some(stored) = self.viewer.state.store.mutate_level(&path) {
                                *stored = level;
                            }
                        }
                        if let Err(e) = self.viewer.state.store.save_level(&path) {
                            log::error!("Failed to save level: {}", e);
                        } else {
                            self.viewer.level_edit.level_dirty = false;
                        }
                    }
                    _ => {}
                }
            }
        });
    }

    // ---- Action handler ----------------------------------------------------

    fn handle_action(&mut self, action: EditorAction) {
        match action {
            EditorAction::Undo => {
                if let Some(Tab::LevelEditor { .. }) = &self.viewer.focused_tab {
                    if let Some(prev) = self.viewer.level_edit.level_undo.undo(
                        self.viewer.level_edit.current_level.clone().unwrap_or_default(),
                    ) {
                        self.viewer.rebuild_viewport_from_level(&prev);
                        self.viewer.level_edit.current_level = Some(prev);
                    }
                } else if let Some(Tab::MusicTracker { .. }) = &self.viewer.focused_tab {
                    self.viewer.tracker.state.do_undo();
                }
            }
            EditorAction::Redo => {
                if let Some(Tab::LevelEditor { .. }) = &self.viewer.focused_tab {
                    if let Some(next) = self.viewer.level_edit.level_undo.redo(
                        self.viewer.level_edit.current_level.clone().unwrap_or_default(),
                    ) {
                        self.viewer.rebuild_viewport_from_level(&next);
                        self.viewer.level_edit.current_level = Some(next);
                    }
                } else if let Some(Tab::MusicTracker { .. }) = &self.viewer.focused_tab {
                    self.viewer.tracker.state.do_redo();
                }
            }
            EditorAction::SaveSong      => { self.viewer.tracker.state.save_song().ok(); }
            EditorAction::SaveSongAs(p) => {
                self.viewer.tracker.state.current_file = Some(p);
                self.viewer.tracker.state.save_song().ok();
            }
            EditorAction::OpenSong(p)   => {
                self.viewer.tracker.state.load_song(&p).ok();
            }
            EditorAction::NewSong => { self.viewer.tracker.state.new_song(); }
            EditorAction::AddRoom => { self.viewer.add_room(); }
            EditorAction::SaveLevel => {
                if let Some(path) = self.viewer.loaded_level_path.clone() {
                    if let Some(level) = &self.viewer.level_edit.current_level {
                        let level = level.clone();
                        if let Some(stored) = self.viewer.state.store.mutate_level(&path) {
                            *stored = level;
                        }
                    }
                    if let Err(e) = self.viewer.state.store.save_level(&path) {
                        log::error!("Failed to save level: {}", e);
                    } else {
                        self.viewer.level_edit.level_dirty = false;
                    }
                }
            }
            EditorAction::NewProject
            | EditorAction::OpenProject(_)
            | EditorAction::SaveProject
            | EditorAction::ImportAsset(_)
            | EditorAction::OpenAsset(_)
            | EditorAction::NewLevel => {}
        }
    }
}

impl Default for Shell {
    fn default() -> Self { Self::new() }
}
