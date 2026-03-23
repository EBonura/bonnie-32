use crate::app::Selection;
use super::level_edit::LevelEditState;

pub struct InspectorPanel;

impl InspectorPanel {
    pub fn new() -> Self {
        Self
    }

    pub fn draw(&mut self, ctx: &egui::Context, level: &LevelEditState, state: &crate::app::AppState) {
        egui::SidePanel::right("inspector")
            .resizable(true)
            .default_width(220.0)
            .min_width(150.0)
            .show(ctx, |ui| {
                ui.strong("Inspector");
                ui.separator();

                match &state.selection {
                    Selection::None => {
                        ui.weak("Nothing selected");
                    }
                    Selection::Asset(path) => {
                        let name = path.file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("?");
                        ui.label(egui::RichText::new(name).strong().size(16.0));
                        ui.label(
                            egui::RichText::new(path.to_string_lossy().as_ref())
                                .small()
                                .color(egui::Color32::GRAY),
                        );
                    }
                    Selection::Room(index) => {
                        ui.label(
                            egui::RichText::new(format!("Room {}", index)).strong().size(16.0),
                        );
                        ui.separator();

                        if let Some(level) = &level.current_level {
                            if let Some(room) = level.rooms.get(*index) {
                                ui.label(format!("Position: ({:.1}, {:.1}, {:.1})",
                                    room.position.x, room.position.y, room.position.z));
                                ui.label(format!("Grid: {}x{}", room.width, room.depth));
                                ui.label(format!("Sectors: {}", room.iter_sectors().count()));
                                ui.label(format!("Portals: {}", room.portals.len()));
                                ui.label(format!("Objects: {}", room.objects.len()));
                                ui.label(format!("Ambient: {:.0}%", room.ambient * 100.0));

                                if room.fog.enabled {
                                    ui.separator();
                                    ui.label("Fog: enabled");
                                    ui.label(format!("  Start: {:.1}", room.fog.start));
                                    ui.label(format!("  Falloff: {:.1}", room.fog.falloff));
                                }
                            }
                        }
                    }
                    Selection::Entity { room, index } => {
                        ui.label(format!("Entity {} in Room {}", index, room));
                        ui.separator();
                        ui.label("Entity components will appear here");
                    }
                }
            });
    }
}

impl Default for InspectorPanel {
    fn default() -> Self {
        Self::new()
    }
}
