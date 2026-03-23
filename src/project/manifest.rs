use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MANIFEST_EXTENSION: &str = "b32";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectManifest {
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default = "default_resolution")]
    pub resolution: (u32, u32),
    /// Level that loads first when the game runs (project-relative path)
    #[serde(default)]
    pub start_level: Option<PathBuf>,
}

fn default_resolution() -> (u32, u32) { (320, 240) }

impl ProjectManifest {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            author: String::new(),
            resolution: default_resolution(),
            start_level: None,
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let ron_str = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())?;
        std::fs::write(path, ron_str)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(path)?;
        let manifest: Self = ron::from_str(&contents)?;
        Ok(manifest)
    }

    pub fn manifest_filename(name: &str) -> String {
        format!("{}.{}", name, MANIFEST_EXTENSION)
    }

    pub fn is_manifest_file(path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e == MANIFEST_EXTENSION)
            .unwrap_or(false)
    }
}

impl Default for ProjectManifest {
    fn default() -> Self { Self::new("Untitled") }
}
