use crate::asset::AssetType;
use super::context::{EditorContext, Selection};

pub struct InspectorPanel;

impl InspectorPanel {
    pub fn new() -> Self {
        Self
    }

    pub fn draw(&mut self, ctx: &egui::Context, editor: &mut EditorContext) {
        egui::SidePanel::right("inspector")
            .resizable(true)
            .default_width(220.0)
            .min_width(150.0)
            .show(ctx, |ui| {
                ui.strong("Inspector");
                ui.separator();

                match &editor.selection {
                    Selection::None => {
                        ui.weak("Nothing selected");
                    }
                    Selection::Asset(handle) => {
                        if let Some(project) = editor.project.as_ref() {
                            if let Some(entry) = project.assets.registry.get(handle) {
                                ui.label(
                                    egui::RichText::new(&entry.name).strong().size(16.0),
                                );
                                ui.label(format!("Type: {}", entry.asset_type.label()));
                                ui.label(format!("Source: {:?}", entry.source));
                                ui.label(format!("Path: {}", entry.path.display()));
                                ui.label(
                                    egui::RichText::new(format!("UUID: {}", handle))
                                        .small()
                                        .color(egui::Color32::GRAY),
                                );

                                ui.separator();

                                // Type-specific info
                                match entry.asset_type {
                                    AssetType::Level => {
                                        ui.label("Level properties will appear here");
                                    }
                                    AssetType::Model => {
                                        ui.label("Model components will appear here");
                                    }
                                    AssetType::Song => {
                                        ui.label("Song properties will appear here");
                                    }
                                    AssetType::Script => {
                                        ui.label("Script info will appear here");
                                    }
                                    _ => {}
                                }
                            } else {
                                ui.colored_label(
                                    egui::Color32::RED,
                                    "Asset not found in registry",
                                );
                            }
                        }
                    }
                    Selection::Room(index) => {
                        ui.label(
                            egui::RichText::new(format!("Room {}", index)).strong().size(16.0),
                        );
                        ui.separator();

                        if let Some(level) = &editor.current_level {
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
