use super::context::{EditorAction, EditorContext, EditorMode};
use super::icons::icon;

pub fn draw_toolbar(ctx: &egui::Context, editor: &mut EditorContext) {
    egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            // File menu
            ui.menu_button("File", |ui| {
                if ui.button("New Project...").clicked() {
                    editor.request_action(EditorAction::NewProject);
                    ui.close_menu();
                }
                if ui.button("Open Project...").clicked() {
                    if let Some(path) = pick_project_file() {
                        editor.request_action(EditorAction::OpenProject(path));
                    }
                    ui.close_menu();
                }
                ui.separator();
                let save_enabled = editor.has_project();
                if ui.add_enabled(save_enabled, egui::Button::new("Save Project")).clicked() {
                    editor.request_action(EditorAction::SaveProject);
                    ui.close_menu();
                }

                ui.separator();

                let import_enabled = editor.has_project();
                if ui.add_enabled(import_enabled, egui::Button::new("Import Asset...")).clicked() {
                    if let Some(path) = pick_import_file() {
                        editor.request_action(EditorAction::ImportAsset(path));
                    }
                    ui.close_menu();
                }
            });

            // Edit menu (when in world editor)
            if editor.mode == EditorMode::WorldEditor {
                ui.menu_button("Level", |ui| {
                    if ui.button("New Level").clicked() {
                        editor.request_action(EditorAction::NewLevel);
                        ui.close_menu();
                    }
                    let has_level = editor.current_level.is_some();
                    if ui.add_enabled(has_level, egui::Button::new("Save Level")).clicked() {
                        editor.request_action(EditorAction::SaveLevel);
                        ui.close_menu();
                    }
                });
            }

            // Song menu (when in tracker)
            if editor.mode == EditorMode::Tracker {
                ui.menu_button("Song", |ui| {
                    if ui.button("New Song").clicked() {
                        editor.request_action(EditorAction::NewSong);
                        ui.close_menu();
                    }
                    if ui.button("Open Song...").clicked() {
                        if let Some(path) = pick_song_file() {
                            editor.request_action(EditorAction::OpenSong(path));
                        }
                        ui.close_menu();
                    }
                    if ui.button("Save Song").clicked() {
                        editor.request_action(EditorAction::SaveSong);
                        ui.close_menu();
                    }
                    if ui.button("Save Song As...").clicked() {
                        if let Some(path) = save_song_dialog() {
                            editor.request_action(EditorAction::SaveSongAs(path));
                        }
                        ui.close_menu();
                    }
                });
            }

            ui.separator();

            // Mode tabs with icons
            let modes: &[(EditorMode, char, &str)] = &[
                (EditorMode::Project,      icon::HOUSE,         "Project"),
                (EditorMode::WorldEditor,  icon::GLOBE,         "World"),
                (EditorMode::Modeler,      icon::PERSON_STANDING,"Modeler"),
                (EditorMode::Tracker,      icon::MUSIC,         "Tracker"),
                (EditorMode::ScriptEditor, icon::PENCIL,        "Script"),
                (EditorMode::Test,         icon::PLAY,          "Test"),
            ];

            for (mode, ic, label) in modes {
                let selected = editor.mode == *mode;
                let text = egui::RichText::new(format!("{} {label}", ic));
                if ui.selectable_label(selected, text).clicked() && !selected {
                    editor.request_action(EditorAction::SwitchMode(*mode));
                }
            }

            // Right-aligned project name + FPS
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(editor.project_name())
                        .small()
                        .color(egui::Color32::GRAY),
                );
            });
        });
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn pick_project_file() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .set_title("Open Project")
        .add_filter("Bonnie-32 Project", &["b32"])
        .pick_file()
}

#[cfg(target_arch = "wasm32")]
fn pick_project_file() -> Option<std::path::PathBuf> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
fn pick_import_file() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .set_title("Import Asset")
        .add_filter("All Supported", &["obj", "ron", "lua", "png", "jpg", "bmp"])
        .add_filter("3D Models", &["obj"])
        .add_filter("Levels", &["ron"])
        .add_filter("Scripts", &["lua"])
        .add_filter("Images", &["png", "jpg", "bmp"])
        .pick_file()
}

#[cfg(target_arch = "wasm32")]
fn pick_import_file() -> Option<std::path::PathBuf> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
fn pick_song_file() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .set_title("Open Song")
        .add_filter("Song Files", &["ron"])
        .pick_file()
}

#[cfg(target_arch = "wasm32")]
fn pick_song_file() -> Option<std::path::PathBuf> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
fn save_song_dialog() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .set_title("Save Song As")
        .add_filter("Song Files", &["ron"])
        .save_file()
}

#[cfg(target_arch = "wasm32")]
fn save_song_dialog() -> Option<std::path::PathBuf> {
    None
}
