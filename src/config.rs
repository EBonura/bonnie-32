use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AppConfig {
    /// Recently opened project files (.b32), most-recent first.
    pub recent_projects: Vec<PathBuf>,
}

impl AppConfig {
    const MAX_RECENT: usize = 10;

    fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("bonnie-32").join("config.ron"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::config_path() else { return Self::default() };
        let Ok(text) = std::fs::read_to_string(path) else { return Self::default() };
        ron::from_str(&text).unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = Self::config_path() else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(text) = ron::to_string(self) {
            let _ = std::fs::write(path, text);
        }
    }

    /// Record a newly opened project path, deduplicating and capping at MAX_RECENT.
    pub fn push_recent(&mut self, project_path: PathBuf) {
        self.recent_projects.retain(|p| p != &project_path);
        self.recent_projects.insert(0, project_path);
        self.recent_projects.truncate(Self::MAX_RECENT);
        self.save();
    }
}
