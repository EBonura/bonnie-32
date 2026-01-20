//! Asset Library - Discovery and caching of assets
//!
//! Manages the collection of assets stored in `assets/userdata/assets/`.
//! Handles both native filesystem discovery and WASM manifest-based loading.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::asset::{Asset, AssetError};
use crate::storage::Storage;

/// Directory where assets are stored
pub const ASSETS_DIR: &str = "assets/userdata/assets";

/// Manifest file for WASM asset loading
pub const MANIFEST_FILE: &str = "manifest.txt";

/// A library of assets
///
/// Provides discovery, loading, and caching of assets from the
/// `assets/userdata/assets/` directory.
#[derive(Debug, Default)]
pub struct AssetLibrary {
    /// Loaded assets keyed by name (without extension)
    assets: HashMap<String, Asset>,
    /// List of discovered asset names (for iteration order)
    asset_names: Vec<String>,
    /// Asset ID -> asset name mapping for ID-based lookups
    by_id: HashMap<u64, String>,
    /// Base directory for assets
    base_dir: PathBuf,
}

impl AssetLibrary {
    /// Create a new empty asset library
    pub fn new() -> Self {
        Self {
            assets: HashMap::new(),
            asset_names: Vec::new(),
            by_id: HashMap::new(),
            base_dir: PathBuf::from(ASSETS_DIR),
        }
    }

    /// Create a library with a custom base directory
    pub fn with_dir(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            assets: HashMap::new(),
            asset_names: Vec::new(),
            by_id: HashMap::new(),
            base_dir: base_dir.into(),
        }
    }

    /// Discover and load all assets from the base directory (native only)
    ///
    /// On WASM, this is a no-op - use upload functionality instead.
    /// Assets are keyed by filename (without extension).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn discover(&mut self) -> Result<usize, AssetError> {
        self.assets.clear();
        self.asset_names.clear();
        self.by_id.clear();

        if !self.base_dir.exists() {
            // Create directory if it doesn't exist
            std::fs::create_dir_all(&self.base_dir)?;
            return Ok(0);
        }

        let mut entries: Vec<_> = std::fs::read_dir(&self.base_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .map(|ext| ext.to_ascii_lowercase() == "ron")
                    .unwrap_or(false)
            })
            .collect();

        // Sort by filename for consistent ordering
        entries.sort();

        for path in entries {
            match Asset::load(&path) {
                Ok(asset) => {
                    // Use filename (without extension) as the key
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(&asset.name)
                        .to_string();
                    let id = asset.id;
                    self.asset_names.push(name.clone());
                    self.by_id.insert(id, name.clone());
                    self.assets.insert(name, asset);
                }
                Err(e) => {
                    eprintln!("Failed to load asset {:?}: {}", path, e);
                }
            }
        }

        Ok(self.assets.len())
    }

    /// Discover assets (WASM stub - no filesystem access)
    ///
    /// On WASM, assets must be uploaded by the user.
    /// Use `add()` to add uploaded assets to the library.
    #[cfg(target_arch = "wasm32")]
    pub fn discover(&mut self) -> Result<usize, AssetError> {
        self.assets.clear();
        self.asset_names.clear();
        self.by_id.clear();
        Ok(0)
    }

    /// Reload a single asset from disk by name (for hot-reload)
    ///
    /// Re-reads the asset file and updates the library entry.
    /// Useful for picking up changes made in the modeler.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn reload_asset(&mut self, name: &str) -> Result<(), AssetError> {
        let path = self.base_dir.join(format!("{}.ron", name));
        let asset = Asset::load(&path)?;

        // Update by_id mapping (in case ID changed, though unlikely)
        if let Some(old_asset) = self.assets.get(name) {
            self.by_id.remove(&old_asset.id);
        }
        self.by_id.insert(asset.id, name.to_string());

        // Update the asset
        self.assets.insert(name.to_string(), asset);
        Ok(())
    }

    /// Reload all assets from disk (for hot-reload)
    ///
    /// Re-reads all known assets from disk. Returns the number of
    /// successfully reloaded assets.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn reload_all(&mut self) -> Result<usize, AssetError> {
        let names: Vec<String> = self.asset_names.clone();
        let mut count = 0;
        for name in names {
            if self.reload_asset(&name).is_ok() {
                count += 1;
            }
        }
        Ok(count)
    }

    /// Load assets from manifest (for WASM)
    ///
    /// The manifest file should contain one asset filename per line (without path).
    /// Assets are keyed by filename (without extension).
    #[cfg(target_arch = "wasm32")]
    pub async fn discover_from_manifest(&mut self) -> Result<usize, AssetError> {
        use macroquad::prelude::load_string;

        self.assets.clear();
        self.asset_names.clear();
        self.by_id.clear();

        let manifest_path = format!("{}/{}", ASSETS_DIR, MANIFEST_FILE);
        let manifest = match load_string(&manifest_path).await {
            Ok(m) => m,
            Err(_) => {
                // No manifest
                return Ok(0);
            }
        };

        for line in manifest.lines() {
            let filename = line.trim();
            if filename.is_empty() || filename.starts_with('#') {
                continue;
            }

            let path = format!("{}/{}", ASSETS_DIR, filename);
            match macroquad::prelude::load_file(&path).await {
                Ok(bytes) => match Asset::load_from_bytes(&bytes) {
                    Ok(asset) => {
                        // Use filename (without extension) as the key
                        let name = filename
                            .strip_suffix(".ron")
                            .unwrap_or(filename)
                            .to_string();
                        let id = asset.id;
                        self.asset_names.push(name.clone());
                        self.by_id.insert(id, name.clone());
                        self.assets.insert(name, asset);
                    }
                    Err(e) => {
                        eprintln!("Failed to parse asset {}: {}", filename, e);
                    }
                },
                Err(e) => {
                    eprintln!("Failed to load asset file {}: {}", filename, e);
                }
            }
        }

        Ok(self.assets.len())
    }

    /// Get an asset by name
    pub fn get(&self, name: &str) -> Option<&Asset> {
        self.assets.get(name)
    }

    /// Get an asset by its stable ID
    ///
    /// Returns the asset with the given ID, if any.
    /// IDs are stable across edits, unlike content hashes.
    pub fn get_by_id(&self, id: u64) -> Option<&Asset> {
        self.by_id.get(&id).and_then(|name| self.assets.get(name))
    }

    /// Get asset name by stable ID
    ///
    /// Returns the name of the asset with the given ID.
    pub fn get_name_by_id(&self, id: u64) -> Option<&str> {
        self.by_id.get(&id).map(|s| s.as_str())
    }

    /// Get a mutable reference to an asset by name
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Asset> {
        self.assets.get_mut(name)
    }

    /// Check if an asset with the given name exists
    pub fn contains(&self, name: &str) -> bool {
        self.assets.contains_key(name)
    }

    /// Add an asset to the library
    ///
    /// If an asset with the same name exists, it will be replaced.
    /// Also updates the ID index.
    pub fn add(&mut self, asset: Asset) {
        let name = asset.name.clone();
        let id = asset.id;

        // If replacing, remove old ID mapping
        if let Some(old_asset) = self.assets.get(&name) {
            self.by_id.remove(&old_asset.id);
        } else {
            self.asset_names.push(name.clone());
        }

        self.by_id.insert(id, name.clone());
        self.assets.insert(name, asset);
    }

    /// Remove an asset by name
    pub fn remove(&mut self, name: &str) -> Option<Asset> {
        if let Some(asset) = self.assets.remove(name) {
            self.asset_names.retain(|n| n != name);
            // Clean up ID index
            self.by_id.remove(&asset.id);
            Some(asset)
        } else {
            None
        }
    }

    /// Get the number of assets in the library
    pub fn len(&self) -> usize {
        self.assets.len()
    }

    /// Check if the library is empty
    pub fn is_empty(&self) -> bool {
        self.assets.is_empty()
    }

    /// Iterate over asset names in discovery order
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.asset_names.iter().map(|s| s.as_str())
    }

    /// Iterate over all assets
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Asset)> {
        self.asset_names
            .iter()
            .filter_map(|name| self.assets.get(name).map(|asset| (name.as_str(), asset)))
    }

    /// Iterate over all assets mutably
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&str, &mut Asset)> {
        self.assets
            .iter_mut()
            .map(|(name, asset)| (name.as_str(), asset))
    }

    /// Filter assets by category
    pub fn by_category<'a>(&'a self, category: &'a str) -> impl Iterator<Item = &'a Asset> {
        self.iter()
            .filter(move |(_, asset)| asset.category == category)
            .map(|(_, asset)| asset)
    }

    /// Filter assets that have a Mesh component
    pub fn with_mesh(&self) -> impl Iterator<Item = &Asset> {
        self.iter().filter(|(_, a)| a.has_mesh()).map(|(_, a)| a)
    }

    /// Filter assets that have a Collision component
    pub fn with_collision(&self) -> impl Iterator<Item = &Asset> {
        self.iter()
            .filter(|(_, a)| a.has_collision())
            .map(|(_, a)| a)
    }

    /// Filter assets that have an Enemy component
    pub fn with_enemy(&self) -> impl Iterator<Item = &Asset> {
        self.iter().filter(|(_, a)| a.has_enemy()).map(|(_, a)| a)
    }

    /// Filter assets that have a Light component
    pub fn with_light(&self) -> impl Iterator<Item = &Asset> {
        self.iter().filter(|(_, a)| a.has_light()).map(|(_, a)| a)
    }

    /// Save an asset to disk (native only)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_asset(&self, name: &str) -> Result<(), AssetError> {
        let asset = self
            .assets
            .get(name)
            .ok_or_else(|| AssetError::ValidationError(format!("asset '{}' not found", name)))?;

        // Ensure directory exists
        std::fs::create_dir_all(&self.base_dir)?;

        let path = self.base_dir.join(format!("{}.ron", name));
        asset.save(&path)
    }

    /// Save an asset (WASM stub - use download instead)
    ///
    /// On WASM, assets cannot be saved to filesystem. Use the download
    /// functionality to export assets as .ron files.
    #[cfg(target_arch = "wasm32")]
    pub fn save_asset(&self, _name: &str) -> Result<(), AssetError> {
        // No filesystem on WASM - use download functionality instead
        Ok(())
    }

    /// Save all assets to disk (native only)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_all(&self) -> Result<usize, AssetError> {
        std::fs::create_dir_all(&self.base_dir)?;

        let mut saved = 0;
        for (name, asset) in &self.assets {
            let path = self.base_dir.join(format!("{}.ron", name));
            asset.save(&path)?;
            saved += 1;
        }
        Ok(saved)
    }

    /// Delete an asset file from disk (native only)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn delete_asset_file(&mut self, name: &str) -> Result<(), AssetError> {
        let path = self.base_dir.join(format!("{}.ron", name));
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        self.remove(name);
        Ok(())
    }

    /// Generate the next available asset name with format "asset_001", "asset_002", etc.
    ///
    /// Follows the same numbering convention as levels and textures.
    pub fn next_available_name(&self) -> String {
        // Find the highest existing asset_XXX number
        let mut highest = 0u32;
        for name in self.asset_names.iter() {
            if let Some(num_str) = name.strip_prefix("asset_") {
                if let Ok(num) = num_str.parse::<u32>() {
                    highest = highest.max(num);
                }
            }
        }

        // Generate next name
        format!("asset_{:03}", highest + 1)
    }

    /// Generate a unique name based on a base name
    pub fn generate_unique_name(&self, base: &str) -> String {
        if !self.contains(base) {
            return base.to_string();
        }

        let mut counter = 1;
        loop {
            let name = format!("{}_{}", base, counter);
            if !self.contains(&name) {
                return name;
            }
            counter += 1;
        }
    }

    /// Regenerate the manifest file (native only)
    ///
    /// Creates a manifest.txt file listing all assets for WASM loading.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn regenerate_manifest(&self) -> Result<(), AssetError> {
        std::fs::create_dir_all(&self.base_dir)?;

        let manifest_path = self.base_dir.join(MANIFEST_FILE);
        let mut manifest = String::new();

        for name in &self.asset_names {
            manifest.push_str(&format!("{}.ron\n", name));
        }

        std::fs::write(manifest_path, manifest)?;
        Ok(())
    }

    /// Get the base directory path
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Get all unique categories in the library
    pub fn categories(&self) -> Vec<&str> {
        let mut cats: Vec<_> = self
            .assets
            .values()
            .filter(|a| !a.category.is_empty())
            .map(|a| a.category.as_str())
            .collect();
        cats.sort();
        cats.dedup();
        cats
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Storage-aware methods (use Storage abstraction for I/O)
    // ─────────────────────────────────────────────────────────────────────────

    /// Discover and load all assets using the storage backend
    ///
    /// This method uses the Storage abstraction for I/O, allowing it to work
    /// with both local filesystem and cloud storage backends.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn discover_with_storage(&mut self, storage: &Storage) -> Result<usize, AssetError> {
        use crate::storage::StorageError;

        self.assets.clear();
        self.asset_names.clear();
        self.by_id.clear();

        let base_dir_str = self.base_dir.to_string_lossy().to_string();

        // List all files in the directory
        let files = match storage.list_sync(&base_dir_str) {
            Ok(files) => files,
            Err(StorageError::NotFound(_)) => {
                // Directory doesn't exist - nothing to discover
                return Ok(0);
            }
            Err(e) => return Err(AssetError::Io(e.to_string())),
        };

        // Filter for .ron files and sort
        let mut ron_files: Vec<_> = files
            .into_iter()
            .filter(|f| f.ends_with(".ron"))
            .collect();
        ron_files.sort();

        // Load each asset
        for filename in ron_files {
            let path = format!("{}/{}", base_dir_str, filename);
            match storage.read_sync(&path) {
                Ok(bytes) => {
                    match Asset::load_from_bytes(&bytes) {
                        Ok(asset) => {
                            // Use filename (without extension) as the key
                            let name = filename
                                .strip_suffix(".ron")
                                .unwrap_or(&filename)
                                .to_string();
                            let id = asset.id;
                            self.asset_names.push(name.clone());
                            self.by_id.insert(id, name.clone());
                            self.assets.insert(name, asset);
                        }
                        Err(e) => {
                            eprintln!("Failed to parse asset {}: {}", filename, e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to read asset {}: {}", filename, e);
                }
            }
        }

        Ok(self.assets.len())
    }

    /// Discover assets using storage (WASM stub)
    #[cfg(target_arch = "wasm32")]
    pub fn discover_with_storage(&mut self, _storage: &Storage) -> Result<usize, AssetError> {
        // On WASM, use manifest-based discovery or cloud storage
        self.assets.clear();
        self.asset_names.clear();
        self.by_id.clear();
        Ok(0)
    }

    /// Save an asset using the storage backend
    pub fn save_asset_with_storage(&self, name: &str, storage: &Storage) -> Result<(), AssetError> {
        let asset = self
            .assets
            .get(name)
            .ok_or_else(|| AssetError::Io(format!("asset '{}' not found", name)))?;

        let path = format!("{}/{}.ron", self.base_dir.to_string_lossy(), name);

        // Serialize and compress the asset
        let bytes = asset.to_bytes()?;

        storage
            .write_sync(&path, &bytes)
            .map_err(|e| AssetError::Io(e.to_string()))
    }

    /// Delete an asset file using the storage backend
    pub fn delete_asset_with_storage(&mut self, name: &str, storage: &Storage) -> Result<(), AssetError> {
        let path = format!("{}/{}.ron", self.base_dir.to_string_lossy(), name);

        storage
            .delete_sync(&path)
            .map_err(|e| AssetError::Io(e.to_string()))?;

        self.remove(name);
        Ok(())
    }

    /// Reload a single asset using storage backend
    #[cfg(not(target_arch = "wasm32"))]
    pub fn reload_asset_with_storage(&mut self, name: &str, storage: &Storage) -> Result<(), AssetError> {
        let path = format!("{}/{}.ron", self.base_dir.to_string_lossy(), name);

        let bytes = storage
            .read_sync(&path)
            .map_err(|e| AssetError::Io(e.to_string()))?;

        let asset = Asset::load_from_bytes(&bytes)?;

        // Update by_id mapping (in case ID changed)
        if let Some(old_asset) = self.assets.get(name) {
            self.by_id.remove(&old_asset.id);
        }
        self.by_id.insert(asset.id, name.to_string());

        self.assets.insert(name.to_string(), asset);
        Ok(())
    }

    /// Regenerate manifest using storage backend
    pub fn regenerate_manifest_with_storage(&self, storage: &Storage) -> Result<(), AssetError> {
        let manifest_path = format!("{}/{}", self.base_dir.to_string_lossy(), MANIFEST_FILE);

        let mut manifest = String::new();
        for name in &self.asset_names {
            manifest.push_str(&format!("{}.ron\n", name));
        }

        storage
            .write_sync(&manifest_path, manifest.as_bytes())
            .map_err(|e| AssetError::Io(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::AssetComponent;

    #[test]
    fn test_library_operations() {
        let mut lib = AssetLibrary::new();

        // Add an asset
        let asset = Asset::new("test_asset");
        lib.add(asset);

        assert_eq!(lib.len(), 1);
        assert!(lib.contains("test_asset"));
        assert!(lib.get("test_asset").is_some());

        // Remove it
        let removed = lib.remove("test_asset");
        assert!(removed.is_some());
        assert_eq!(lib.len(), 0);
    }

    #[test]
    fn test_unique_name_generation() {
        let mut lib = AssetLibrary::new();

        // First name should be used as-is
        assert_eq!(lib.generate_unique_name("my_asset"), "my_asset");

        // Add it
        lib.add(Asset::new("my_asset"));

        // Now it should generate a unique name
        assert_eq!(lib.generate_unique_name("my_asset"), "my_asset_1");

        lib.add(Asset::new("my_asset_1"));
        assert_eq!(lib.generate_unique_name("my_asset"), "my_asset_2");
    }

    #[test]
    fn test_next_available_name() {
        let mut lib = AssetLibrary::new();

        // Empty library should start at asset_001
        assert_eq!(lib.next_available_name(), "asset_001");

        // Add asset_001
        lib.add(Asset::new("asset_001"));
        assert_eq!(lib.next_available_name(), "asset_002");

        // Add asset_005 (gap)
        lib.add(Asset::new("asset_005"));
        // Should use highest + 1, so asset_006
        assert_eq!(lib.next_available_name(), "asset_006");

        // Non-numbered assets should be ignored
        lib.add(Asset::new("my_custom_asset"));
        assert_eq!(lib.next_available_name(), "asset_006");
    }

    #[test]
    fn test_id_lookup() {
        let mut lib = AssetLibrary::new();

        let asset = Asset::new("test_asset");
        let id = asset.id;
        lib.add(asset);

        // Should find by ID
        assert!(lib.get_by_id(id).is_some());
        assert_eq!(lib.get_name_by_id(id), Some("test_asset"));

        // Should not find invalid ID
        assert!(lib.get_by_id(12345).is_none());
    }

    #[test]
    fn test_category_filter() {
        let mut lib = AssetLibrary::new();

        let mut enemy = Asset::new("grunt");
        enemy.category = "enemies".to_string();
        lib.add(enemy);

        let mut prop = Asset::new("crate");
        prop.category = "props".to_string();
        lib.add(prop);

        let enemies: Vec<_> = lib.by_category("enemies").collect();
        assert_eq!(enemies.len(), 1);
        assert_eq!(enemies[0].name, "grunt");

        let props: Vec<_> = lib.by_category("props").collect();
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].name, "crate");
    }
}
