//! Asset Library - Discovery and caching of assets
//!
//! Manages the collection of assets from two directories:
//! - `assets/samples/assets/` - Bundled sample assets (read-only)
//! - `assets/userdata/assets/` - User-created assets (editable, cloud-synced)
//!
//! Handles both native filesystem discovery and WASM manifest-based loading.

use std::collections::HashMap;
use std::path::Path;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

use super::asset::{Asset, AssetError};
use crate::storage::Storage;

/// Directory for bundled sample assets (read-only)
pub const SAMPLES_ASSETS_DIR: &str = "assets/samples/assets";

/// Directory for user-created assets (editable, cloud-synced)
pub const USER_ASSETS_DIR: &str = "assets/userdata/assets";

/// Legacy constant for backwards compatibility
pub const ASSETS_DIR: &str = USER_ASSETS_DIR;

/// Manifest file for WASM asset loading
pub const MANIFEST_FILE: &str = "manifest.txt";

/// Source/origin of an asset (determines editability and storage location)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AssetSource {
    /// Bundled sample asset from assets/samples/assets/ (read-only)
    Sample,
    /// User-created asset from assets/userdata/assets/ (editable, cloud-synced)
    #[default]
    User,
}

impl AssetSource {
    /// Get the prefix for this source (used in library keys)
    pub fn prefix(&self) -> &'static str {
        match self {
            AssetSource::Sample => "sample:",
            AssetSource::User => "user:",
        }
    }
}

/// Create a namespaced key for the asset library
/// This prevents name collisions between sample and user assets
fn make_asset_key(source: AssetSource, name: &str) -> String {
    format!("{}{}", source.prefix(), name)
}

/// A library of assets
///
/// Provides discovery, loading, and caching of assets from two directories:
/// - `assets/samples/assets/` - Bundled sample assets (read-only)
/// - `assets/userdata/assets/` - User-created assets (editable, cloud-synced)
#[derive(Debug, Default)]
pub struct AssetLibrary {
    /// Loaded assets keyed by name (without extension)
    assets: HashMap<String, Asset>,
    /// List of discovered sample asset names (for iteration order)
    sample_names: Vec<String>,
    /// List of discovered user asset names (for iteration order)
    user_names: Vec<String>,
    /// Asset ID -> asset name mapping for ID-based lookups
    by_id: HashMap<u64, String>,
}

impl AssetLibrary {
    /// Create a new empty asset library
    pub fn new() -> Self {
        Self {
            assets: HashMap::new(),
            sample_names: Vec::new(),
            user_names: Vec::new(),
            by_id: HashMap::new(),
        }
    }

    /// Discover and load all assets from both directories (native only)
    ///
    /// Scans:
    /// - `assets/samples/assets/` for bundled sample assets (read-only)
    /// - `assets/userdata/assets/` for user-created assets (editable)
    ///
    /// On WASM, this is a no-op - use upload functionality instead.
    /// Assets are keyed by filename (without extension).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn discover(&mut self) -> Result<usize, AssetError> {
        self.assets.clear();
        self.sample_names.clear();
        self.user_names.clear();
        self.by_id.clear();

        let mut count = 0;

        // Discover sample assets (read-only)
        count += self.discover_from_dir(SAMPLES_ASSETS_DIR, AssetSource::Sample)?;

        // Discover user assets (editable)
        count += self.discover_from_dir(USER_ASSETS_DIR, AssetSource::User)?;

        Ok(count)
    }

    /// Discover assets from a specific directory
    #[cfg(not(target_arch = "wasm32"))]
    fn discover_from_dir(&mut self, dir: &str, source: AssetSource) -> Result<usize, AssetError> {
        let base_dir = PathBuf::from(dir);

        if !base_dir.exists() {
            // Create user directory if it doesn't exist, skip samples if missing
            if source == AssetSource::User {
                std::fs::create_dir_all(&base_dir)?;
            }
            return Ok(0);
        }

        let mut entries: Vec<_> = std::fs::read_dir(&base_dir)?
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

        let mut count = 0;
        for path in entries {
            match Asset::load(&path) {
                Ok(mut asset) => {
                    // Set the source
                    asset.source = source;

                    // Use filename (without extension) as the base name
                    let base_name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(&asset.name)
                        .to_string();
                    let id = asset.id;

                    // Create namespaced key to prevent collisions between sources
                    let key = make_asset_key(source, &base_name);

                    // Track in the appropriate list (using base name for display)
                    match source {
                        AssetSource::Sample => self.sample_names.push(base_name.clone()),
                        AssetSource::User => self.user_names.push(base_name.clone()),
                    }

                    self.by_id.insert(id, key.clone());
                    self.assets.insert(key, asset);
                    count += 1;
                }
                Err(e) => {
                    eprintln!("Failed to load asset {:?}: {}", path, e);
                }
            }
        }

        Ok(count)
    }

    /// Discover assets (WASM stub - no filesystem access)
    ///
    /// On WASM, assets must be uploaded by the user.
    /// Use `add()` to add uploaded assets to the library.
    #[cfg(target_arch = "wasm32")]
    pub fn discover(&mut self) -> Result<usize, AssetError> {
        self.assets.clear();
        self.sample_names.clear();
        self.user_names.clear();
        self.by_id.clear();
        Ok(0)
    }

    /// Reload a single asset from disk by name (for hot-reload)
    ///
    /// Re-reads the asset file and updates the library entry.
    /// Useful for picking up changes made in the modeler.
    /// Supports both namespaced keys and plain names.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn reload_asset(&mut self, name: &str) -> Result<(), AssetError> {
        // Determine the actual key and source
        let (key, source, base_name) = if let Some(stripped) = name.strip_prefix("sample:") {
            (name.to_string(), AssetSource::Sample, stripped.to_string())
        } else if let Some(stripped) = name.strip_prefix("user:") {
            (name.to_string(), AssetSource::User, stripped.to_string())
        } else {
            // Plain name - check if exists, default to user
            let user_key = make_asset_key(AssetSource::User, name);
            if self.assets.contains_key(&user_key) {
                (user_key, AssetSource::User, name.to_string())
            } else {
                let sample_key = make_asset_key(AssetSource::Sample, name);
                if self.assets.contains_key(&sample_key) {
                    (sample_key, AssetSource::Sample, name.to_string())
                } else {
                    // Default to user directory for new assets
                    (make_asset_key(AssetSource::User, name), AssetSource::User, name.to_string())
                }
            }
        };

        // Determine directory from source
        let dir = match source {
            AssetSource::Sample => SAMPLES_ASSETS_DIR,
            AssetSource::User => USER_ASSETS_DIR,
        };

        let path = PathBuf::from(dir).join(format!("{}.ron", base_name));
        let mut asset = Asset::load(&path)?;
        asset.source = source;

        // Update ID index
        if let Some(old_asset) = self.assets.get(&key) {
            self.by_id.remove(&old_asset.id);
        }
        self.by_id.insert(asset.id, key.clone());

        // Update the asset
        self.assets.insert(key, asset);
        Ok(())
    }

    /// Reload all assets from disk (for hot-reload)
    ///
    /// Re-reads all known assets from disk. Returns the number of
    /// successfully reloaded assets.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn reload_all(&mut self) -> Result<usize, AssetError> {
        let names: Vec<String> = self.all_names().map(|s| s.to_string()).collect();
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
    /// Loads from both samples and user assets directories.
    #[cfg(target_arch = "wasm32")]
    pub async fn discover_from_manifest(&mut self) -> Result<usize, AssetError> {
        

        self.assets.clear();
        self.sample_names.clear();
        self.user_names.clear();
        self.by_id.clear();

        let mut count = 0;

        // Load sample assets
        count += self.load_manifest_from_dir(SAMPLES_ASSETS_DIR, AssetSource::Sample).await?;

        // Load user assets
        count += self.load_manifest_from_dir(USER_ASSETS_DIR, AssetSource::User).await?;

        Ok(count)
    }

    /// Load assets from manifest in a specific directory (WASM)
    #[cfg(target_arch = "wasm32")]
    async fn load_manifest_from_dir(&mut self, dir: &str, source: AssetSource) -> Result<usize, AssetError> {
        use macroquad::prelude::load_string;

        let manifest_path = format!("{}/{}", dir, MANIFEST_FILE);
        let manifest = match load_string(&manifest_path).await {
            Ok(m) => m,
            Err(_) => {
                // No manifest for this directory
                return Ok(0);
            }
        };

        let mut count = 0;
        for line in manifest.lines() {
            let filename = line.trim();
            if filename.is_empty() || filename.starts_with('#') {
                continue;
            }

            let path = format!("{}/{}", dir, filename);
            match macroquad::prelude::load_file(&path).await {
                Ok(bytes) => match Asset::load_from_bytes(&bytes) {
                    Ok(mut asset) => {
                        asset.source = source;

                        // Use filename (without extension) as the key
                        let name = filename
                            .strip_suffix(".ron")
                            .unwrap_or(filename)
                            .to_string();
                        let id = asset.id;

                        match source {
                            AssetSource::Sample => self.sample_names.push(name.clone()),
                            AssetSource::User => self.user_names.push(name.clone()),
                        }

                        self.by_id.insert(id, name.clone());
                        self.assets.insert(name, asset);
                        count += 1;
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

        Ok(count)
    }

    /// Get an asset by name
    ///
    /// Supports both namespaced keys ("sample:asset_003", "user:asset_003")
    /// and plain names ("asset_003"). For plain names, tries user first, then sample.
    pub fn get(&self, name: &str) -> Option<&Asset> {
        // If already namespaced, look up directly
        if name.starts_with("sample:") || name.starts_with("user:") {
            return self.assets.get(name);
        }
        // Try user first (user assets take precedence for editing), then sample
        self.assets.get(&make_asset_key(AssetSource::User, name))
            .or_else(|| self.assets.get(&make_asset_key(AssetSource::Sample, name)))
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
    ///
    /// Supports both namespaced keys and plain names (tries user first, then sample)
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Asset> {
        // If already namespaced, look up directly
        if name.starts_with("sample:") || name.starts_with("user:") {
            return self.assets.get_mut(name);
        }
        // Try user first, then sample
        let user_key = make_asset_key(AssetSource::User, name);
        if self.assets.contains_key(&user_key) {
            return self.assets.get_mut(&user_key);
        }
        let sample_key = make_asset_key(AssetSource::Sample, name);
        self.assets.get_mut(&sample_key)
    }

    /// Check if an asset with the given name exists
    ///
    /// Supports both namespaced keys and plain names
    pub fn contains(&self, name: &str) -> bool {
        if name.starts_with("sample:") || name.starts_with("user:") {
            return self.assets.contains_key(name);
        }
        self.assets.contains_key(&make_asset_key(AssetSource::User, name))
            || self.assets.contains_key(&make_asset_key(AssetSource::Sample, name))
    }

    /// Add an asset to the library
    ///
    /// If an asset with the same name AND source exists, it will be replaced.
    /// Also updates the ID index. New assets are added to the appropriate
    /// list based on their source field.
    pub fn add(&mut self, asset: Asset) {
        let base_name = asset.name.clone();
        let id = asset.id;
        let source = asset.source;
        let key = make_asset_key(source, &base_name);

        // If replacing, remove old ID mapping
        if let Some(old_asset) = self.assets.get(&key) {
            self.by_id.remove(&old_asset.id);
        }

        // Add to appropriate list (using base name)
        match source {
            AssetSource::Sample => {
                if !self.sample_names.contains(&base_name) {
                    self.sample_names.push(base_name.clone());
                }
            }
            AssetSource::User => {
                if !self.user_names.contains(&base_name) {
                    self.user_names.push(base_name);
                }
            }
        }

        self.by_id.insert(id, key.clone());
        self.assets.insert(key, asset);
    }

    /// Remove an asset by name (supports namespaced or plain names)
    pub fn remove(&mut self, name: &str) -> Option<Asset> {
        // Determine the actual key to use
        let key = if name.starts_with("sample:") || name.starts_with("user:") {
            name.to_string()
        } else {
            // Try user first, then sample
            let user_key = make_asset_key(AssetSource::User, name);
            if self.assets.contains_key(&user_key) {
                user_key
            } else {
                make_asset_key(AssetSource::Sample, name)
            }
        };

        if let Some(asset) = self.assets.remove(&key) {
            // Extract base name from key for list cleanup
            let base_name = key.strip_prefix("sample:")
                .or_else(|| key.strip_prefix("user:"))
                .unwrap_or(&key);

            // Remove from appropriate list
            match asset.source {
                AssetSource::Sample => self.sample_names.retain(|n| n != base_name),
                AssetSource::User => self.user_names.retain(|n| n != base_name),
            }
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

    /// Get the number of sample assets
    pub fn sample_count(&self) -> usize {
        self.sample_names.len()
    }

    /// Get the number of user assets
    pub fn user_count(&self) -> usize {
        self.user_names.len()
    }

    /// Check if there are any sample assets
    pub fn has_samples(&self) -> bool {
        !self.sample_names.is_empty()
    }

    /// Check if there are any user assets
    pub fn has_user_assets(&self) -> bool {
        !self.user_names.is_empty()
    }

    /// Iterate over sample asset names in discovery order
    pub fn sample_names(&self) -> impl Iterator<Item = &str> {
        self.sample_names.iter().map(|s| s.as_str())
    }

    /// Iterate over user asset names in discovery order
    pub fn user_asset_names(&self) -> impl Iterator<Item = &str> {
        self.user_names.iter().map(|s| s.as_str())
    }

    /// Iterate over all asset names (samples first, then user assets)
    pub fn all_names(&self) -> impl Iterator<Item = &str> {
        self.sample_names.iter().chain(self.user_names.iter()).map(|s| s.as_str())
    }

    /// Iterate over asset names in discovery order (alias for all_names for backwards compatibility)
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.all_names()
    }

    /// Iterate over sample assets (returns base name, not namespaced key)
    pub fn samples(&self) -> impl Iterator<Item = (&str, &Asset)> {
        self.sample_names
            .iter()
            .filter_map(|name| {
                let key = make_asset_key(AssetSource::Sample, name);
                self.assets.get(&key).map(|asset| (name.as_str(), asset))
            })
    }

    /// Iterate over user assets (returns base name, not namespaced key)
    pub fn user_assets(&self) -> impl Iterator<Item = (&str, &Asset)> {
        self.user_names
            .iter()
            .filter_map(|name| {
                let key = make_asset_key(AssetSource::User, name);
                self.assets.get(&key).map(|asset| (name.as_str(), asset))
            })
    }

    /// Iterate over all assets with their full namespaced keys
    /// Returns ("sample:name", asset) or ("user:name", asset)
    pub fn iter_with_keys(&self) -> impl Iterator<Item = (String, &Asset)> {
        let samples = self.sample_names.iter().filter_map(|name| {
            let key = make_asset_key(AssetSource::Sample, name);
            self.assets.get(&key).map(|asset| (key, asset))
        });
        let users = self.user_names.iter().filter_map(|name| {
            let key = make_asset_key(AssetSource::User, name);
            self.assets.get(&key).map(|asset| (key, asset))
        });
        samples.chain(users)
    }

    /// Iterate over all assets (samples first, then user assets)
    /// Returns base name for display, use iter_with_keys() for library keys
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Asset)> {
        self.samples().chain(self.user_assets())
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
    ///
    /// Assets are saved to their appropriate directory based on source:
    /// - Sample assets cannot be saved (read-only)
    /// - User assets are saved to assets/userdata/assets/
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_asset(&self, name: &str) -> Result<(), AssetError> {
        let asset = self
            .assets
            .get(name)
            .ok_or_else(|| AssetError::ValidationError(format!("asset '{}' not found", name)))?;

        // Sample assets are read-only
        if asset.source == AssetSource::Sample {
            return Err(AssetError::ValidationError(
                "cannot save sample assets (read-only)".into(),
            ));
        }

        // Ensure user directory exists
        let base_dir = PathBuf::from(USER_ASSETS_DIR);
        std::fs::create_dir_all(&base_dir)?;

        let path = base_dir.join(format!("{}.ron", name));
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

    /// Save all user assets to disk (native only)
    ///
    /// Only saves user assets (sample assets are read-only).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_all(&self) -> Result<usize, AssetError> {
        let base_dir = PathBuf::from(USER_ASSETS_DIR);
        std::fs::create_dir_all(&base_dir)?;

        let mut saved = 0;
        for name in &self.user_names {
            if let Some(asset) = self.assets.get(name) {
                let path = base_dir.join(format!("{}.ron", name));
                asset.save(&path)?;
                saved += 1;
            }
        }
        Ok(saved)
    }

    /// Delete an asset file from disk (native only)
    ///
    /// Only user assets can be deleted (sample assets are read-only).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn delete_asset_file(&mut self, name: &str) -> Result<(), AssetError> {
        // Check if asset exists and is a user asset
        if let Some(asset) = self.assets.get(name) {
            if asset.source == AssetSource::Sample {
                return Err(AssetError::ValidationError(
                    "cannot delete sample assets (read-only)".into(),
                ));
            }
        }

        let path = PathBuf::from(USER_ASSETS_DIR).join(format!("{}.ron", name));
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        self.remove(name);
        Ok(())
    }

    /// Generate the next available asset name with format "asset_001", "asset_002", etc.
    ///
    /// Follows the same numbering convention as levels and textures.
    /// Checks both sample and user assets to avoid conflicts.
    pub fn next_available_name(&self) -> String {
        // Find the highest existing asset_XXX number across all assets
        let mut highest = 0u32;
        for name in self.sample_names.iter().chain(self.user_names.iter()) {
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

    /// Regenerate the manifest files (native only)
    ///
    /// Creates manifest.txt files listing all assets for WASM loading.
    /// Creates separate manifests for samples and user directories.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn regenerate_manifest(&self) -> Result<(), AssetError> {
        // Regenerate sample manifest
        let samples_dir = PathBuf::from(SAMPLES_ASSETS_DIR);
        if samples_dir.exists() {
            let manifest_path = samples_dir.join(MANIFEST_FILE);
            let mut manifest = String::new();
            for name in &self.sample_names {
                manifest.push_str(&format!("{}.ron\n", name));
            }
            std::fs::write(manifest_path, manifest)?;
        }

        // Regenerate user manifest
        let user_dir = PathBuf::from(USER_ASSETS_DIR);
        std::fs::create_dir_all(&user_dir)?;
        let manifest_path = user_dir.join(MANIFEST_FILE);
        let mut manifest = String::new();
        for name in &self.user_names {
            manifest.push_str(&format!("{}.ron\n", name));
        }
        std::fs::write(manifest_path, manifest)?;

        Ok(())
    }

    /// Get the user assets directory path
    pub fn user_dir(&self) -> &Path {
        Path::new(USER_ASSETS_DIR)
    }

    /// Get the samples assets directory path
    pub fn samples_dir(&self) -> &Path {
        Path::new(SAMPLES_ASSETS_DIR)
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
    /// Discovers from both samples and user directories.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn discover_with_storage(&mut self, storage: &Storage) -> Result<usize, AssetError> {
        self.assets.clear();
        self.sample_names.clear();
        self.user_names.clear();
        self.by_id.clear();

        let mut count = 0;

        // Discover sample assets (read-only, local only)
        count += self.discover_from_dir_with_storage(SAMPLES_ASSETS_DIR, AssetSource::Sample, storage)?;

        // Discover user assets (editable, may be cloud-synced)
        count += self.discover_from_dir_with_storage(USER_ASSETS_DIR, AssetSource::User, storage)?;

        Ok(count)
    }

    /// Discover assets from a specific directory using storage backend
    #[cfg(not(target_arch = "wasm32"))]
    fn discover_from_dir_with_storage(
        &mut self,
        dir: &str,
        source: AssetSource,
        storage: &Storage,
    ) -> Result<usize, AssetError> {
        use crate::storage::StorageError;

        // List all files in the directory
        let files = match storage.list_sync(dir) {
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

        let mut count = 0;
        // Load each asset
        for filename in ron_files {
            // Handle both full paths (from cloud) and filenames (from local)
            let path = if filename.contains('/') {
                filename.clone()
            } else {
                format!("{}/{}", dir, filename)
            };

            match storage.read_sync(&path) {
                Ok(bytes) => {
                    match Asset::load_from_bytes(&bytes) {
                        Ok(mut asset) => {
                            asset.source = source;

                            // Use filename (without extension) as the key
                            let name = filename
                                .rsplit('/')
                                .next()
                                .unwrap_or(&filename)
                                .strip_suffix(".ron")
                                .unwrap_or(&filename)
                                .to_string();
                            let id = asset.id;

                            match source {
                                AssetSource::Sample => self.sample_names.push(name.clone()),
                                AssetSource::User => self.user_names.push(name.clone()),
                            }

                            self.by_id.insert(id, name.clone());
                            self.assets.insert(name, asset);
                            count += 1;
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

        Ok(count)
    }

    /// Discover assets using storage (WASM stub)
    #[cfg(target_arch = "wasm32")]
    pub fn discover_with_storage(&mut self, _storage: &Storage) -> Result<usize, AssetError> {
        // On WASM, use manifest-based discovery or cloud storage
        self.assets.clear();
        self.sample_names.clear();
        self.user_names.clear();
        self.by_id.clear();
        Ok(0)
    }

    /// Save an asset using the storage backend
    ///
    /// Only user assets can be saved (sample assets are read-only).
    pub fn save_asset_with_storage(&self, name: &str, storage: &Storage) -> Result<(), AssetError> {
        let asset = self
            .assets
            .get(name)
            .ok_or_else(|| AssetError::Io(format!("asset '{}' not found", name)))?;

        // Sample assets are read-only
        if asset.source == AssetSource::Sample {
            return Err(AssetError::ValidationError(
                "cannot save sample assets (read-only)".into(),
            ));
        }

        let path = format!("{}/{}.ron", USER_ASSETS_DIR, name);

        // Serialize and compress the asset
        let bytes = asset.to_bytes()?;

        storage
            .write_sync(&path, &bytes)
            .map_err(|e| AssetError::Io(e.to_string()))
    }

    /// Delete an asset file using the storage backend
    ///
    /// Only user assets can be deleted (sample assets are read-only).
    pub fn delete_asset_with_storage(&mut self, name: &str, storage: &Storage) -> Result<(), AssetError> {
        // Check if asset exists and is a user asset
        if let Some(asset) = self.assets.get(name) {
            if asset.source == AssetSource::Sample {
                return Err(AssetError::ValidationError(
                    "cannot delete sample assets (read-only)".into(),
                ));
            }
        }

        let path = format!("{}/{}.ron", USER_ASSETS_DIR, name);

        storage
            .delete_sync(&path)
            .map_err(|e| AssetError::Io(e.to_string()))?;

        self.remove(name);
        Ok(())
    }

    /// Reload a single asset using storage backend
    #[cfg(not(target_arch = "wasm32"))]
    pub fn reload_asset_with_storage(&mut self, name: &str, storage: &Storage) -> Result<(), AssetError> {
        // Determine directory from existing asset source
        let dir = if let Some(asset) = self.assets.get(name) {
            match asset.source {
                AssetSource::Sample => SAMPLES_ASSETS_DIR,
                AssetSource::User => USER_ASSETS_DIR,
            }
        } else {
            USER_ASSETS_DIR // Default to user directory
        };

        let path = format!("{}/{}.ron", dir, name);

        let bytes = storage
            .read_sync(&path)
            .map_err(|e| AssetError::Io(e.to_string()))?;

        let mut asset = Asset::load_from_bytes(&bytes)?;

        // Preserve the source
        if let Some(old_asset) = self.assets.get(name) {
            asset.source = old_asset.source;
            self.by_id.remove(&old_asset.id);
        }
        self.by_id.insert(asset.id, name.to_string());

        self.assets.insert(name.to_string(), asset);
        Ok(())
    }

    /// Regenerate manifest using storage backend
    ///
    /// Only regenerates the user assets manifest (sample manifest is read-only).
    pub fn regenerate_manifest_with_storage(&self, storage: &Storage) -> Result<(), AssetError> {
        let manifest_path = format!("{}/{}", USER_ASSETS_DIR, MANIFEST_FILE);

        let mut manifest = String::new();
        for name in &self.user_names {
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
