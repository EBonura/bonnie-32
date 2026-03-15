pub mod manifest;

use std::path::{Path, PathBuf};

use crate::asset::{AssetManager, AssetSource, AssetType};
use manifest::ProjectManifest;

pub use manifest::ProjectManifest as Manifest;

pub struct Project {
    pub manifest: ProjectManifest,
    pub assets: AssetManager,
    root: PathBuf,
    manifest_path: PathBuf,
}

impl Project {
    /// Create a new project in the given directory.
    /// Creates the directory structure and manifest file on disk.
    pub fn create(
        root: PathBuf,
        name: impl Into<String>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let name = name.into();
        let manifest = ProjectManifest::new(&name);

        std::fs::create_dir_all(&root)?;

        // Create subdirectories
        for dir in &["levels", "assets", "textures", "songs", "scripts", "meshes"] {
            std::fs::create_dir_all(root.join(dir))?;
        }

        let manifest_path = root.join(ProjectManifest::manifest_filename(&name));
        manifest.save(&manifest_path)?;

        let assets = AssetManager::with_project(root.clone())?;
        assets.save_registry()?;

        Ok(Self {
            manifest,
            assets,
            root,
            manifest_path,
        })
    }

    /// Open an existing project from a .b32 manifest file.
    pub fn open(manifest_path: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let manifest = ProjectManifest::load(&manifest_path)?;
        let root = manifest_path
            .parent()
            .ok_or("Manifest has no parent directory")?
            .to_path_buf();

        let assets = AssetManager::with_project(root.clone())?;

        Ok(Self {
            manifest,
            assets,
            root,
            manifest_path,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    /// Save manifest and asset registry to disk.
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.manifest.save(&self.manifest_path)?;
        self.assets.save_registry()?;
        Ok(())
    }

    /// Scan all standard directories for unregistered assets and add them to the registry.
    pub fn scan_assets(&mut self) -> Vec<crate::asset::AssetHandle> {
        let mut found = Vec::new();

        let scans: &[(AssetType, &str)] = &[
            (AssetType::Level, "levels"),
            (AssetType::Model, "assets"),
            (AssetType::Song, "songs"),
            (AssetType::Script, "scripts"),
        ];

        for &(asset_type, dir_name) in scans {
            let dir = self.root.join(dir_name);
            if dir.exists() {
                let handles = self.assets.scan_directory(&dir, asset_type, AssetSource::Project);
                found.extend(handles);
            }
        }

        found
    }

    /// Register bundled sample assets from the engine's assets/samples/ directory.
    pub fn register_bundled_assets(&mut self, bundled_root: &Path) -> Vec<crate::asset::AssetHandle> {
        let mut found = Vec::new();

        let scans: &[(AssetType, &str)] = &[
            (AssetType::Level, "levels"),
            (AssetType::Model, "assets"),
            (AssetType::Song, "songs"),
        ];

        for &(asset_type, dir_name) in scans {
            let dir = bundled_root.join(dir_name);
            if dir.exists() {
                let handles = self.assets.scan_directory(&dir, asset_type, AssetSource::Bundled);
                found.extend(handles);
            }
        }

        found
    }

    /// Get the full path for a project-relative path.
    pub fn resolve(&self, relative: &Path) -> PathBuf {
        self.root.join(relative)
    }
}
