use super::context::{EditorContext, Selection};

pub struct HierarchyPanel;

impl HierarchyPanel {
    pub fn new() -> Self {
        Self
    }

    pub fn draw(&mut self, ctx: &egui::Context, editor: &mut EditorContext) {
        egui::SidePanel::left("hierarchy")
            .resizable(true)
            .default_width(180.0)
            .min_width(120.0)
            .show(ctx, |ui| {
                ui.strong("Hierarchy");
                ui.separator();

                if !editor.has_project() {
                    ui.weak("No project");
                    return;
                }

                // Placeholder room hierarchy
                // Will be populated when Scene system is ported in Phase 4
                ui.collapsing("Rooms", |ui| {
                    let room_count = 0; // TODO: get from scene
                    if room_count == 0 {
                        ui.weak("No rooms. Open a level from the Content Browser.");
                    }
                    for i in 0..room_count {
                        let selected = editor.selection == Selection::Room(i);
                        if ui.selectable_label(selected, format!("Room {}", i)).clicked() {
                            editor.select(Selection::Room(i));
                        }
                    }
                });
            });
    }
}

impl Default for HierarchyPanel {
    fn default() -> Self {
        Self::new()
    }
}
