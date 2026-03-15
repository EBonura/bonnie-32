use std::collections::HashMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use super::handle::AssetHandle;
use super::registry::AssetRegistry;
use super::types::{AssetSource, AssetType};

#[derive(Debug)]
pub struct CachedAsset {
    pub data: AssetData,
    pub loaded_from: PathBuf,
}

#[derive(Debug)]
pub enum AssetData {
    Raw(Vec<u8>),
    Ron(String),
}

pub struct AssetManager {
    pub registry: AssetRegistry,
    cache: HashMap<Uuid, CachedAsset>,
    project_root: Option<PathBuf>,
}

impl AssetManager {
    pub fn new() -> Self {
        Self {
            registry: AssetRegistry::new(),
            cache: HashMap::new(),
            project_root: None,
        }
    }

    pub fn with_project(project_root: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let registry = if AssetRegistry::registry_path(&project_root).exists() {
            AssetRegistry::load(&project_root)?
        } else {
            AssetRegistry::new()
        };

        Ok(Self {
            registry,
            cache: HashMap::new(),
            project_root: Some(project_root),
        })
    }

    pub fn project_root(&self) -> Option<&Path> {
        self.project_root.as_deref()
    }

    pub fn set_project_root(&mut self, root: PathBuf) {
        self.project_root = Some(root);
        self.cache.clear();
    }

    pub fn register(
        &mut self,
        path: PathBuf,
        asset_type: AssetType,
        source: AssetSource,
    ) -> AssetHandle {
        self.registry.register(path, asset_type, source)
    }

    pub fn import(
        &mut self,
        source_path: &Path,
        asset_type: AssetType,
    ) -> Result<AssetHandle, Box<dyn std::error::Error>> {
        let project_root = self
            .project_root
            .as_ref()
            .ok_or("No project root set")?
            .clone();

        let dest_dir = project_root.join(asset_type.directory());
        std::fs::create_dir_all(&dest_dir)?;

        let filename = source_path
            .file_name()
            .ok_or("Source path has no filename")?;
        let dest_path = dest_dir.join(filename);

        std::fs::copy(source_path, &dest_path)?;

        let relative = dest_path
            .strip_prefix(&project_root)
            .unwrap_or(&dest_path)
            .to_path_buf();

        let handle = self.registry.register(relative, asset_type, AssetSource::Project);
        Ok(handle)
    }

    pub fn resolve_path(&self, handle: &AssetHandle) -> Option<PathBuf> {
        let entry = self.registry.get(handle)?;
        match entry.source {
            AssetSource::Project => {
                let root = self.project_root.as_ref()?;
                Some(root.join(&entry.path))
            }
            AssetSource::Bundled => Some(PathBuf::from("assets/samples").join(&entry.path)),
        }
    }

    pub fn load_raw(&mut self, handle: &AssetHandle) -> Option<&[u8]> {
        let uuid = handle.0;

        if !self.cache.contains_key(&uuid) {
            let full_path = self.resolve_path(handle)?;
            let data = std::fs::read(&full_path).ok()?;
            self.cache.insert(uuid, CachedAsset {
                data: AssetData::Raw(data),
                loaded_from: full_path,
            });
        }

        match &self.cache.get(&uuid)?.data {
            AssetData::Raw(bytes) => Some(bytes.as_slice()),
            _ => None,
        }
    }

    pub fn load_ron(&mut self, handle: &AssetHandle) -> Option<&str> {
        let uuid = handle.0;

        if !self.cache.contains_key(&uuid) {
            let full_path = self.resolve_path(handle)?;
            let text = std::fs::read_to_string(&full_path).ok()?;
            self.cache.insert(uuid, CachedAsset {
                data: AssetData::Ron(text),
                loaded_from: full_path,
            });
        }

        match &self.cache.get(&uuid)?.data {
            AssetData::Ron(text) => Some(text.as_str()),
            _ => None,
        }
    }

    pub fn evict(&mut self, handle: &AssetHandle) {
        self.cache.remove(&handle.0);
    }

    pub fn evict_all(&mut self) {
        self.cache.clear();
    }

    pub fn save_registry(&self) -> Result<(), Box<dyn std::error::Error>> {
        let root = self.project_root.as_ref().ok_or("No project root set")?;
        self.registry.save(root)
    }

    pub fn scan_directory(
        &mut self,
        dir: &Path,
        asset_type: AssetType,
        source: AssetSource,
    ) -> Vec<AssetHandle> {
        let mut handles = Vec::new();
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return handles,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let relative = if let Some(root) = &self.project_root {
                path.strip_prefix(root).unwrap_or(&path).to_path_buf()
            } else {
                path.clone()
            };

            if self.registry.find_by_path(&relative).is_none() {
                let handle = self.registry.register(relative, asset_type, source);
                handles.push(handle);
            }
        }
        handles
    }
}

impl Default for AssetManager {
    fn default() -> Self {
        Self::new()
    }
}
