use std::collections::HashMap;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::handle::AssetHandle;
use super::types::{AssetSource, AssetType, RegistryEntry};

const REGISTRY_FILENAME: &str = "asset_registry.ron";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AssetRegistry {
    entries: HashMap<Uuid, RegistryEntry>,
}

impl AssetRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        path: PathBuf,
        asset_type: AssetType,
        source: AssetSource,
    ) -> AssetHandle {
        // Check if this path is already registered
        if let Some((&uuid, _)) = self.entries.iter().find(|(_, e)| e.path == path) {
            return AssetHandle::from_uuid(uuid);
        }

        let handle = AssetHandle::new();
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unnamed")
            .to_string();

        self.entries.insert(
            handle.0,
            RegistryEntry {
                path,
                asset_type,
                source,
                name,
            },
        );
        handle
    }

    pub fn get(&self, handle: &AssetHandle) -> Option<&RegistryEntry> {
        self.entries.get(&handle.0)
    }

    pub fn get_mut(&mut self, handle: &AssetHandle) -> Option<&mut RegistryEntry> {
        self.entries.get_mut(&handle.0)
    }

    pub fn remove(&mut self, handle: &AssetHandle) -> Option<RegistryEntry> {
        self.entries.remove(&handle.0)
    }

    pub fn rename(&mut self, handle: &AssetHandle, new_path: PathBuf) -> bool {
        if let Some(entry) = self.entries.get_mut(&handle.0) {
            entry.name = new_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unnamed")
                .to_string();
            entry.path = new_path;
            true
        } else {
            false
        }
    }

    pub fn find_by_path(&self, path: &Path) -> Option<AssetHandle> {
        self.entries
            .iter()
            .find(|(_, e)| e.path == path)
            .map(|(&uuid, _)| AssetHandle::from_uuid(uuid))
    }

    pub fn iter(&self) -> impl Iterator<Item = (AssetHandle, &RegistryEntry)> {
        self.entries
            .iter()
            .map(|(&uuid, entry)| (AssetHandle::from_uuid(uuid), entry))
    }

    pub fn iter_by_type(&self, asset_type: AssetType) -> impl Iterator<Item = (AssetHandle, &RegistryEntry)> {
        self.entries
            .iter()
            .filter(move |(_, e)| e.asset_type == asset_type)
            .map(|(&uuid, entry)| (AssetHandle::from_uuid(uuid), entry))
    }

    pub fn iter_by_source(&self, source: AssetSource) -> impl Iterator<Item = (AssetHandle, &RegistryEntry)> {
        self.entries
            .iter()
            .filter(move |(_, e)| e.source == source)
            .map(|(&uuid, entry)| (AssetHandle::from_uuid(uuid), entry))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn save(&self, project_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let path = project_root.join(REGISTRY_FILENAME);
        let ron_str = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())?;
        std::fs::write(path, ron_str)?;
        Ok(())
    }

    pub fn load(project_root: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let path = project_root.join(REGISTRY_FILENAME);
        let contents = std::fs::read_to_string(path)?;
        let registry: Self = ron::from_str(&contents)?;
        Ok(registry)
    }

    pub fn registry_path(project_root: &Path) -> PathBuf {
        project_root.join(REGISTRY_FILENAME)
    }
}
