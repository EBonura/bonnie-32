use crate::asset::{AssetSource, AssetType};
use super::context::{EditorAction, EditorContext, Selection};

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
    fn matches(&self, asset_type: AssetType) -> bool {
        match self {
            TypeFilter::All => true,
            TypeFilter::Levels => asset_type == AssetType::Level,
            TypeFilter::Models => asset_type == AssetType::Model,
            TypeFilter::Textures => matches!(asset_type, AssetType::Texture | AssetType::TexturePack),
            TypeFilter::Songs => asset_type == AssetType::Song,
            TypeFilter::Scripts => asset_type == AssetType::Script,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            TypeFilter::All => "All",
            TypeFilter::Levels => "Levels",
            TypeFilter::Models => "Models",
            TypeFilter::Textures => "Textures",
            TypeFilter::Songs => "Songs",
            TypeFilter::Scripts => "Scripts",
        }
    }
}

pub struct ContentBrowser {
    filter: TypeFilter,
    search: String,
}

impl ContentBrowser {
    pub fn new() -> Self {
        Self {
            filter: TypeFilter::All,
            search: String::new(),
        }
    }

    pub fn draw(&mut self, ctx: &egui::Context, editor: &mut EditorContext) {
        egui::TopBottomPanel::bottom("content_browser")
            .resizable(true)
            .min_height(120.0)
            .default_height(200.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("Content Browser");
                    ui.separator();

                    // Type filter tabs
                    let filters = [
                        TypeFilter::All,
                        TypeFilter::Levels,
                        TypeFilter::Models,
                        TypeFilter::Textures,
                        TypeFilter::Songs,
                        TypeFilter::Scripts,
                    ];
                    for f in filters {
                        if ui.selectable_label(self.filter == f, f.label()).clicked() {
                            self.filter = f;
                        }
                    }

                    ui.separator();

                    // Search box
                    ui.label("Search:");
                    ui.text_edit_singleline(&mut self.search);
                });

                ui.separator();

                let Some(project) = editor.project.as_ref() else {
                    ui.centered_and_justified(|ui| {
                        ui.label("No project open. Use File > New Project to get started.");
                    });
                    return;
                };

                // Collect assets matching filter + search
                let search_lower = self.search.to_lowercase();
                let mut bundled = Vec::new();
                let mut project_assets = Vec::new();

                for (handle, entry) in project.assets.registry.iter() {
                    if !self.filter.matches(entry.asset_type) {
                        continue;
                    }
                    if !self.search.is_empty() && !entry.name.to_lowercase().contains(&search_lower) {
                        continue;
                    }

                    match entry.source {
                        AssetSource::Bundled => bundled.push((handle, entry.clone())),
                        AssetSource::Project => project_assets.push((handle, entry.clone())),
                    }
                }

                bundled.sort_by(|a, b| a.1.name.cmp(&b.1.name));
                project_assets.sort_by(|a, b| a.1.name.cmp(&b.1.name));

                egui::ScrollArea::vertical().show(ui, |ui| {
                    // Bundled section
                    if !bundled.is_empty() {
                        ui.collapsing(
                            egui::RichText::new(format!("Bundled ({})", bundled.len()))
                                .color(egui::Color32::from_rgb(140, 140, 180)),
                            |ui| {
                                for (handle, entry) in &bundled {
                                    let selected = editor.selection == Selection::Asset(*handle);
                                    let label = format!("{} [{}]", entry.name, entry.asset_type.label());
                                    let response = ui.selectable_label(selected, &label);

                                    if response.clicked() {
                                        editor.select(Selection::Asset(*handle));
                                    }
                                    if response.double_clicked() {
                                        editor.request_action(EditorAction::OpenAsset(*handle));
                                    }
                                }
                            },
                        );
                    }

                    // Project section
                    let project_label = if project_assets.is_empty() {
                        "Project (empty)".to_string()
                    } else {
                        format!("Project ({})", project_assets.len())
                    };

                    let header = egui::CollapsingHeader::new(
                        egui::RichText::new(&project_label)
                            .color(egui::Color32::from_rgb(180, 180, 140)),
                    )
                    .default_open(true);

                    header.show(ui, |ui| {
                        if project_assets.is_empty() {
                            ui.label(
                                egui::RichText::new("Drop files into the project directories or use File > Import")
                                    .weak(),
                            );
                        }
                        for (handle, entry) in &project_assets {
                            let selected = editor.selection == Selection::Asset(*handle);
                            let label = format!("{} [{}]", entry.name, entry.asset_type.label());
                            let response = ui.selectable_label(selected, &label);

                            if response.clicked() {
                                editor.select(Selection::Asset(*handle));
                            }
                            if response.double_clicked() {
                                editor.request_action(EditorAction::OpenAsset(*handle));
                            }
                        }
                    });
                });
            });
    }
}

impl Default for ContentBrowser {
    fn default() -> Self {
        Self::new()
    }
}
