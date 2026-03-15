use super::context::{EditorAction, EditorContext, EditorMode};

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
                    // TODO: file dialog
                    ui.close_menu();
                }
                ui.separator();
                let save_enabled = editor.has_project();
                if ui.add_enabled(save_enabled, egui::Button::new("Save Project")).clicked() {
                    editor.request_action(EditorAction::SaveProject);
                    ui.close_menu();
                }
            });

            ui.separator();

            // Mode tabs
            let modes = [
                (EditorMode::Project, "Project"),
                (EditorMode::WorldEditor, "World"),
                (EditorMode::Modeler, "Modeler"),
                (EditorMode::Tracker, "Tracker"),
                (EditorMode::ScriptEditor, "Script"),
                (EditorMode::Test, "Test"),
            ];

            for (mode, label) in modes {
                let selected = editor.mode == mode;
                if ui.selectable_label(selected, label).clicked() && !selected {
                    editor.request_action(EditorAction::SwitchMode(mode));
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
