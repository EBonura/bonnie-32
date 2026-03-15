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
                        ui.label(format!("Room {}", index));
                        ui.separator();
                        ui.label("Room properties will appear here");
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
