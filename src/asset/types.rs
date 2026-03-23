use serde::{Deserialize, Serialize};

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
    pub fn label(self) -> &'static str {
        match self {
            AssetType::Level       => "Level",
            AssetType::Model       => "Model",
            AssetType::Texture     => "Texture",
            AssetType::TexturePack => "Texture Pack",
            AssetType::Song        => "Song",
            AssetType::Script      => "Script",
        }
    }

    /// Subdirectory name inside a project folder.
    pub fn directory(self) -> &'static str {
        match self {
            AssetType::Level       => "levels",
            AssetType::Model       => "meshes",
            AssetType::Texture
            | AssetType::TexturePack => "textures",
            AssetType::Song        => "songs",
            AssetType::Script      => "scripts",
        }
    }

    /// File extensions that belong to this type.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            AssetType::Level       => &["ron"],
            AssetType::Model       => &["obj"],
            AssetType::Texture     => &["png", "jpg", "jpeg", "bmp"],
            AssetType::TexturePack => &["ron"],
            AssetType::Song        => &["ron"],
            AssetType::Script      => &["lua"],
        }
    }

    /// Infer type from file extension alone (best-effort; directory wins when ambiguous).
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "lua"                    => Some(AssetType::Script),
            "obj"                    => Some(AssetType::Model),
            "png" | "jpg" | "jpeg" | "bmp" => Some(AssetType::Texture),
            _                        => None,
        }
    }

    /// All asset types, in display order.
    pub fn all() -> &'static [AssetType] {
        &[
            AssetType::Level,
            AssetType::Model,
            AssetType::Texture,
            AssetType::Song,
            AssetType::Script,
        ]
    }
}
