pub mod manifest;

use std::path::{Path, PathBuf};

use crate::asset::AssetType;
use manifest::ProjectManifest;

pub use manifest::ProjectManifest as Manifest;

pub struct Project {
    pub manifest: ProjectManifest,
    root: PathBuf,
    manifest_path: PathBuf,
}

impl Project {
    /// Create a new project in the given directory.
    pub fn create(
        root: PathBuf,
        name: impl Into<String>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let name = name.into();
        let manifest = ProjectManifest::new(&name);

        std::fs::create_dir_all(&root)?;

        for dir in &["levels", "meshes", "textures", "songs", "scripts"] {
            std::fs::create_dir_all(root.join(dir))?;
        }

        let manifest_path = root.join(ProjectManifest::manifest_filename(&name));
        manifest.save(&manifest_path)?;

        Ok(Self { manifest, root, manifest_path })
    }

    /// Open an existing project from a .b32 manifest file.
    pub fn open(manifest_path: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let manifest = ProjectManifest::load(&manifest_path)?;
        let root = manifest_path
            .parent()
            .ok_or("Manifest has no parent directory")?
            .to_path_buf();

        Ok(Self { manifest, root, manifest_path })
    }

    pub fn root(&self) -> &Path { &self.root }
    pub fn manifest_path(&self) -> &Path { &self.manifest_path }
    pub fn name(&self) -> &str { &self.manifest.name }

    /// Save manifest to disk.
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.manifest.save(&self.manifest_path)?;
        Ok(())
    }

    /// Scan one of the project's standard directories and return all matching paths.
    /// Paths are absolute.
    pub fn scan(&self, asset_type: AssetType) -> Vec<PathBuf> {
        scan_dir(&self.root.join(asset_type.directory()), asset_type)
    }

    /// Absolute path to a project-relative path.
    pub fn resolve(&self, relative: &Path) -> PathBuf {
        self.root.join(relative)
    }
}

/// Scan `dir` for files whose extension matches `asset_type`.
/// Returns absolute paths, sorted by filename.
pub fn scan_dir(dir: &Path, asset_type: AssetType) -> Vec<PathBuf> {
    let exts = asset_type.extensions();
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };

    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| exts.contains(&e.to_lowercase().as_str()))
                    .unwrap_or(false)
        })
        .collect();

    paths.sort_by(|a, b| {
        a.file_name().cmp(&b.file_name())
    });

    paths
}
