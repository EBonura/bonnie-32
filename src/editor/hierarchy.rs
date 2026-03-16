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

                // Collect room info to avoid borrow conflicts
                let room_info: Vec<(usize, usize)> = editor.current_level.as_ref()
                    .map(|level| {
                        level.rooms.iter().enumerate()
                            .map(|(i, room)| (i, room.iter_sectors().count()))
                            .collect()
                    })
                    .unwrap_or_default();

                ui.collapsing(format!("Rooms ({})", room_info.len()), |ui| {
                    if room_info.is_empty() {
                        ui.weak("No rooms. Open a level from the Content Browser.");
                    }
                    for (i, sector_count) in &room_info {
                        let selected = editor.selection == Selection::Room(*i);
                        let label = format!("Room {} ({} sectors)", i, sector_count);
                        if ui.selectable_label(selected, &label).clicked() {
                            editor.select(Selection::Room(*i));
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
