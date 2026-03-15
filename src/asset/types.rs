use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssetType {
    Level,
    Model,
    Texture,
    TexturePack,
    Song,
    Script,
}

impl AssetType {
    pub fn label(&self) -> &'static str {
        match self {
            AssetType::Level => "Level",
            AssetType::Model => "Model",
            AssetType::Texture => "Texture",
            AssetType::TexturePack => "Texture Pack",
            AssetType::Song => "Song",
            AssetType::Script => "Script",
        }
    }

    pub fn directory(&self) -> &'static str {
        match self {
            AssetType::Level => "levels",
            AssetType::Model => "assets",
            AssetType::Texture => "textures",
            AssetType::TexturePack => "textures",
            AssetType::Song => "songs",
            AssetType::Script => "scripts",
        }
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "lua" => Some(AssetType::Script),
            "obj" => Some(AssetType::Model),
            "png" | "jpg" | "jpeg" | "bmp" => Some(AssetType::Texture),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetSource {
    Bundled,
    Project,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    pub path: PathBuf,
    pub asset_type: AssetType,
    pub source: AssetSource,
    #[serde(default)]
    pub name: String,
}
