//! Texture library - discovery and caching of user textures
//!
//! Manages the collection of indexed textures stored in two directories:
//! - `assets/samples/textures/` - Bundled sample textures (read-only)
//! - `assets/userdata/textures/` - User-created textures (editable, cloud-synced)
//!
//! Handles both native filesystem discovery and WASM manifest-based loading.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::user_texture::{TextureError, UserTexture};
use crate::storage::Storage;

/// Directory where sample textures are stored (read-only)
pub const SAMPLES_TEXTURES_DIR: &str = "assets/samples/textures";

/// Directory where user textures are stored (editable, cloud-synced)
pub const USER_TEXTURES_DIR: &str = "assets/userdata/textures";

/// Legacy alias for USER_TEXTURES_DIR
pub const TEXTURES_USER_DIR: &str = "assets/userdata/textures";

/// Manifest file for WASM texture loading
pub const MANIFEST_FILE: &str = "manifest.txt";

/// Source/origin of a texture (sample vs user-created)
///
/// Determines where the texture came from and whether it's editable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextureSource {
    /// Bundled sample texture from assets/samples/textures/ (read-only)
    Sample,
    /// User-created texture from assets/userdata/textures/ (editable, cloud-synced)
    #[default]
    User,
}

/// A library of textures
///
/// Provides discovery, loading, and caching of textures from two directories:
/// - `assets/samples/textures/` - Bundled sample textures (read-only)
/// - `assets/userdata/textures/` - User-created textures (editable, cloud-synced)
#[derive(Debug, Default)]
pub struct TextureLibrary {
    /// Loaded textures keyed by name (without extension)
    textures: HashMap<String, UserTexture>,
    /// List of discovered sample texture names (for iteration order)
    sample_names: Vec<String>,
    /// List of discovered user texture names (for iteration order)
    user_names: Vec<String>,
    /// Texture ID → texture name mapping for ID-based lookups
    /// Unlike content hash, IDs are stable across edits
    by_id: HashMap<u64, String>,
}

impl TextureLibrary {
    /// Create a new empty texture library
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
            sample_names: Vec::new(),
            user_names: Vec::new(),
            by_id: HashMap::new(),
        }
    }

    /// Discover and load all textures from both directories (native only)
    ///
    /// Scans:
    /// - `assets/samples/textures/` for bundled sample textures (read-only)
    /// - `assets/userdata/textures/` for user-created textures (editable)
    ///
    /// On WASM, this is a no-op - use upload functionality instead.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn discover(&mut self) -> Result<usize, TextureError> {
        self.textures.clear();
        self.sample_names.clear();
        self.user_names.clear();
        self.by_id.clear();

        let mut count = 0;

        // Discover sample textures (read-only)
        count += self.discover_from_dir(SAMPLES_TEXTURES_DIR, TextureSource::Sample)?;

        // Discover user textures (editable)
        count += self.discover_from_dir(USER_TEXTURES_DIR, TextureSource::User)?;

        Ok(count)
    }

    /// Discover textures from a specific directory
    #[cfg(not(target_arch = "wasm32"))]
    fn discover_from_dir(&mut self, dir: &str, source: TextureSource) -> Result<usize, TextureError> {
        let base_dir = PathBuf::from(dir);

        if !base_dir.exists() {
            // Create user directory if it doesn't exist, skip samples if missing
            if source == TextureSource::User {
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

        let mut loaded = 0;
        for path in entries {
            match UserTexture::load(&path) {
                Ok(mut tex) => {
                    // Set the source
                    tex.source = source;

                    let name = tex.name.clone();
                    let id = tex.id;

                    // Track in the appropriate list
                    match source {
                        TextureSource::Sample => self.sample_names.push(name.clone()),
                        TextureSource::User => self.user_names.push(name.clone()),
                    }

                    self.by_id.insert(id, name.clone());
                    self.textures.insert(name, tex);
                    loaded += 1;
                }
                Err(e) => {
                    eprintln!("Failed to load texture {:?}: {}", path, e);
                }
            }
        }

        Ok(loaded)
    }

    /// Discover textures (WASM stub - no filesystem access)
    ///
    /// On WASM, textures must be uploaded by the user. This returns Ok(0).
    /// Use `add()` to add uploaded textures to the library.
    #[cfg(target_arch = "wasm32")]
    pub fn discover(&mut self) -> Result<usize, TextureError> {
        self.textures.clear();
        self.sample_names.clear();
        self.user_names.clear();
        self.by_id.clear();
        Ok(0)
    }

    /// Load textures from manifest (for WASM)
    ///
    /// The manifest file should contain one texture filename per line (without path).
    /// Loads from both samples and user directories.
    #[cfg(target_arch = "wasm32")]
    pub async fn discover_from_manifest(&mut self) -> Result<usize, TextureError> {
        self.textures.clear();
        self.sample_names.clear();
        self.user_names.clear();
        self.by_id.clear();

        let mut count = 0;

        // Load sample textures (RON UserTexture definitions)
        count += self.load_manifest_from_dir(SAMPLES_TEXTURES_DIR, TextureSource::Sample).await?;

        // Load user textures
        count += self.load_manifest_from_dir(USER_TEXTURES_DIR, TextureSource::User).await?;

        Ok(count)
    }

    /// Load textures from manifest in a specific directory (WASM)
    ///
    /// The manifest supports section headers like `[subdirectory]` to organize
    /// textures into subdirectories. Files listed after a section header are
    /// loaded from that subdirectory.
    #[cfg(target_arch = "wasm32")]
    async fn load_manifest_from_dir(&mut self, dir: &str, source: TextureSource) -> Result<usize, TextureError> {
        use macroquad::prelude::load_string;

        let manifest_path = format!("{}/{}", dir, MANIFEST_FILE);
        let manifest = match load_string(&manifest_path).await {
            Ok(m) => m,
            Err(_) => {
                // No manifest for this directory
                return Ok(0);
            }
        };

        let mut loaded = 0;
        let mut current_subdir: Option<String> = None;

        for line in manifest.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Check for section header like [subdirectory]
            if line.starts_with('[') && line.ends_with(']') {
                let subdir = &line[1..line.len()-1];
                current_subdir = if subdir.is_empty() {
                    None
                } else {
                    Some(subdir.to_string())
                };
                continue;
            }

            // Build the full path including subdirectory if present
            let path = match &current_subdir {
                Some(subdir) => format!("{}/{}/{}", dir, subdir, line),
                None => format!("{}/{}", dir, line),
            };

            match macroquad::prelude::load_file(&path).await {
                Ok(bytes) => match UserTexture::load_from_bytes(&bytes) {
                    Ok(mut tex) => {
                        tex.source = source;

                        let name = tex.name.clone();
                        let id = tex.id;

                        match source {
                            TextureSource::Sample => self.sample_names.push(name.clone()),
                            TextureSource::User => self.user_names.push(name.clone()),
                        }

                        self.by_id.insert(id, name.clone());
                        self.textures.insert(name, tex);
                        loaded += 1;
                    }
                    Err(e) => {
                        eprintln!("Failed to parse texture {}: {}", path, e);
                    }
                },
                Err(e) => {
                    eprintln!("Failed to load texture file {}: {}", path, e);
                }
            }
        }

        Ok(loaded)
    }

    /// Get a texture by name
    pub fn get(&self, name: &str) -> Option<&UserTexture> {
        self.textures.get(name)
    }

    /// Get a texture by its stable ID
    ///
    /// Returns the texture with the given ID, if any.
    /// IDs are stable across edits, unlike content hashes.
    pub fn get_by_id(&self, id: u64) -> Option<&UserTexture> {
        self.by_id
            .get(&id)
            .and_then(|name| self.textures.get(name))
    }

    /// Get texture name by stable ID
    ///
    /// Returns the name of the texture with the given ID.
    pub fn get_name_by_id(&self, id: u64) -> Option<&str> {
        self.by_id.get(&id).map(|s| s.as_str())
    }

    /// Get a mutable reference to a texture by name
    pub fn get_mut(&mut self, name: &str) -> Option<&mut UserTexture> {
        self.textures.get_mut(name)
    }

    /// Check if a texture with the given name exists
    pub fn contains(&self, name: &str) -> bool {
        self.textures.contains_key(name)
    }

    /// Add a texture to the library
    ///
    /// If a texture with the same name exists, it will be replaced.
    /// Also updates the ID index. New textures are added to the appropriate
    /// list based on their source field.
    pub fn add(&mut self, texture: UserTexture) {
        let name = texture.name.clone();
        let id = texture.id;
        let source = texture.source;

        // If replacing, remove old ID mapping and from old list
        if let Some(old_tex) = self.textures.get(&name) {
            self.by_id.remove(&old_tex.id);
            // Remove from appropriate list
            match old_tex.source {
                TextureSource::Sample => self.sample_names.retain(|n| n != &name),
                TextureSource::User => self.user_names.retain(|n| n != &name),
            }
        }

        // Add to appropriate list
        match source {
            TextureSource::Sample => {
                if !self.sample_names.contains(&name) {
                    self.sample_names.push(name.clone());
                }
            }
            TextureSource::User => {
                if !self.user_names.contains(&name) {
                    self.user_names.push(name.clone());
                }
            }
        }

        self.by_id.insert(id, name.clone());
        self.textures.insert(name, texture);
    }

    /// Remove a texture by name
    pub fn remove(&mut self, name: &str) -> Option<UserTexture> {
        if let Some(tex) = self.textures.remove(name) {
            // Remove from appropriate list
            match tex.source {
                TextureSource::Sample => self.sample_names.retain(|n| n != name),
                TextureSource::User => self.user_names.retain(|n| n != name),
            }
            // Clean up ID index
            self.by_id.remove(&tex.id);
            Some(tex)
        } else {
            None
        }
    }

    /// Get the number of textures in the library
    pub fn len(&self) -> usize {
        self.textures.len()
    }

    /// Check if the library is empty
    pub fn is_empty(&self) -> bool {
        self.textures.is_empty()
    }

    /// Get the number of sample textures
    pub fn sample_count(&self) -> usize {
        self.sample_names.len()
    }

    /// Get the number of user textures
    pub fn user_count(&self) -> usize {
        self.user_names.len()
    }

    /// Check if there are any sample textures
    pub fn has_samples(&self) -> bool {
        !self.sample_names.is_empty()
    }

    /// Check if there are any user textures
    pub fn has_user_textures(&self) -> bool {
        !self.user_names.is_empty()
    }

    /// Iterate over sample texture names in discovery order
    pub fn sample_names(&self) -> impl Iterator<Item = &str> {
        self.sample_names.iter().map(|s| s.as_str())
    }

    /// Iterate over user texture names in discovery order
    pub fn user_names(&self) -> impl Iterator<Item = &str> {
        self.user_names.iter().map(|s| s.as_str())
    }

    /// Iterate over all texture names (samples first, then user textures)
    pub fn all_names(&self) -> impl Iterator<Item = &str> {
        self.sample_names.iter().chain(self.user_names.iter()).map(|s| s.as_str())
    }

    /// Iterate over texture names in discovery order (alias for all_names for backwards compatibility)
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.all_names()
    }

    /// Iterate over sample textures
    pub fn samples(&self) -> impl Iterator<Item = (&str, &UserTexture)> {
        self.sample_names
            .iter()
            .filter_map(|name| self.textures.get(name).map(|tex| (name.as_str(), tex)))
    }

    /// Iterate over user textures
    pub fn user_textures(&self) -> impl Iterator<Item = (&str, &UserTexture)> {
        self.user_names
            .iter()
            .filter_map(|name| self.textures.get(name).map(|tex| (name.as_str(), tex)))
    }

    /// Iterate over all textures (samples first, then user textures)
    pub fn iter(&self) -> impl Iterator<Item = (&str, &UserTexture)> {
        self.sample_names
            .iter()
            .chain(self.user_names.iter())
            .filter_map(|name| self.textures.get(name).map(|tex| (name.as_str(), tex)))
    }

    /// Iterate over all textures mutably
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&str, &mut UserTexture)> {
        self.textures
            .iter_mut()
            .map(|(name, tex)| (name.as_str(), tex))
    }

    /// Get textures usable in the World Editor (64x64 only)
    pub fn world_editor_textures(&self) -> impl Iterator<Item = (&str, &UserTexture)> {
        self.iter().filter(|(_, tex)| tex.usable_in_world_editor())
    }

    /// Get user textures usable in the World Editor (64x64 only)
    pub fn world_editor_user_textures(&self) -> impl Iterator<Item = (&str, &UserTexture)> {
        self.user_textures().filter(|(_, tex)| tex.usable_in_world_editor())
    }

    /// Get sample textures usable in the World Editor (64x64 only)
    pub fn world_editor_sample_textures(&self) -> impl Iterator<Item = (&str, &UserTexture)> {
        self.samples().filter(|(_, tex)| tex.usable_in_world_editor())
    }

    /// Save a texture to disk (native only)
    ///
    /// Only user textures can be saved. Sample textures are read-only.
    /// Returns an error if the texture is a sample.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_texture(&self, name: &str) -> Result<(), TextureError> {
        let tex = self
            .textures
            .get(name)
            .ok_or_else(|| TextureError::ValidationError(format!("texture '{}' not found", name)))?;

        // Check if this is a sample texture (read-only)
        if tex.source == TextureSource::Sample {
            return Err(TextureError::ValidationError(
                "cannot save sample texture - it is read-only".to_string(),
            ));
        }

        // Ensure directory exists
        std::fs::create_dir_all(USER_TEXTURES_DIR)?;

        let path = PathBuf::from(USER_TEXTURES_DIR).join(format!("{}.ron", name));
        tex.save(&path)
    }

    /// Save a texture (WASM stub - use download instead)
    ///
    /// On WASM, textures cannot be saved to filesystem. Use the download
    /// functionality to export textures as .ron files.
    #[cfg(target_arch = "wasm32")]
    pub fn save_texture(&self, _name: &str) -> Result<(), TextureError> {
        // No filesystem on WASM - use download functionality instead
        Ok(())
    }

    /// Save all user textures to disk (native only)
    ///
    /// Only saves user textures - sample textures are read-only.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_all(&self) -> Result<usize, TextureError> {
        std::fs::create_dir_all(USER_TEXTURES_DIR)?;

        let mut saved = 0;
        for (name, tex) in self.user_textures() {
            let path = PathBuf::from(USER_TEXTURES_DIR).join(format!("{}.ron", name));
            tex.save(&path)?;
            saved += 1;
        }
        Ok(saved)
    }

    /// Delete a texture file from disk (native only)
    ///
    /// Only user textures can be deleted. Sample textures are read-only.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn delete_texture_file(&mut self, name: &str) -> Result<(), TextureError> {
        // Check if this is a sample texture (read-only)
        if let Some(tex) = self.textures.get(name) {
            if tex.source == TextureSource::Sample {
                return Err(TextureError::ValidationError(
                    "cannot delete sample texture - it is read-only".to_string(),
                ));
            }
        }

        let path = PathBuf::from(USER_TEXTURES_DIR).join(format!("{}.ron", name));
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        self.remove(name);
        Ok(())
    }

    /// Generate the next available texture name with format "texture_001", "texture_002", etc.
    ///
    /// Follows the same numbering convention as levels and models.
    pub fn next_available_name(&self) -> String {
        // Find the highest existing texture_XXX number across all textures
        let mut highest = 0u32;
        for name in self.sample_names.iter().chain(self.user_names.iter()) {
            if let Some(num_str) = name.strip_prefix("texture_") {
                if let Ok(num) = num_str.parse::<u32>() {
                    highest = highest.max(num);
                }
            }
        }

        // Generate next name
        format!("texture_{:03}", highest + 1)
    }

    /// Generate a unique name based on a base name (legacy, use next_available_name for new textures)
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

    /// Regenerate manifest files for both directories (native only)
    ///
    /// Creates manifest.txt files listing textures for WASM loading.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn regenerate_manifest(&self) -> Result<(), TextureError> {
        // Generate sample manifest
        if !self.sample_names.is_empty() {
            let samples_dir = PathBuf::from(SAMPLES_TEXTURES_DIR);
            if samples_dir.exists() {
                let manifest_path = samples_dir.join(MANIFEST_FILE);
                let mut manifest = String::new();
                for name in &self.sample_names {
                    manifest.push_str(&format!("{}.ron\n", name));
                }
                std::fs::write(manifest_path, manifest)?;
            }
        }

        // Generate user manifest
        std::fs::create_dir_all(USER_TEXTURES_DIR)?;
        let user_dir = PathBuf::from(USER_TEXTURES_DIR);
        let manifest_path = user_dir.join(MANIFEST_FILE);
        let mut manifest = String::new();
        for name in &self.user_names {
            manifest.push_str(&format!("{}.ron\n", name));
        }
        std::fs::write(manifest_path, manifest)?;

        Ok(())
    }

    /// Regenerate user manifest only (native only)
    ///
    /// Creates manifest.txt for user textures directory.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn regenerate_user_manifest(&self) -> Result<(), TextureError> {
        std::fs::create_dir_all(USER_TEXTURES_DIR)?;
        let user_dir = PathBuf::from(USER_TEXTURES_DIR);
        let manifest_path = user_dir.join(MANIFEST_FILE);
        let mut manifest = String::new();
        for name in &self.user_names {
            manifest.push_str(&format!("{}.ron\n", name));
        }
        std::fs::write(manifest_path, manifest)?;
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Storage-aware methods (use Storage abstraction for I/O)
    // ─────────────────────────────────────────────────────────────────────────

    /// Discover and load all textures using the storage backend
    #[cfg(not(target_arch = "wasm32"))]
    pub fn discover_with_storage(&mut self, storage: &Storage) -> Result<usize, TextureError> {
        self.textures.clear();
        self.sample_names.clear();
        self.user_names.clear();
        self.by_id.clear();

        let mut count = 0;

        // Discover sample textures
        count += self.discover_from_dir_with_storage(SAMPLES_TEXTURES_DIR, TextureSource::Sample, storage)?;

        // Discover user textures
        count += self.discover_from_dir_with_storage(USER_TEXTURES_DIR, TextureSource::User, storage)?;

        Ok(count)
    }

    /// Discover textures from a specific directory using storage backend
    #[cfg(not(target_arch = "wasm32"))]
    fn discover_from_dir_with_storage(
        &mut self,
        dir: &str,
        source: TextureSource,
        storage: &Storage,
    ) -> Result<usize, TextureError> {
        use crate::storage::StorageError;

        // List all files in the directory
        let files = match storage.list_sync(dir) {
            Ok(files) => files,
            Err(StorageError::NotFound(_)) => {
                // Directory doesn't exist - nothing to discover
                return Ok(0);
            }
            Err(e) => return Err(TextureError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))),
        };

        // Filter for .ron files and sort
        let mut ron_files: Vec<_> = files
            .into_iter()
            .filter(|f| f.ends_with(".ron"))
            .collect();
        ron_files.sort();

        // Load each texture
        let mut loaded = 0;
        for filename in ron_files {
            let path = format!("{}/{}", dir, filename);
            match storage.read_sync(&path) {
                Ok(bytes) => {
                    match UserTexture::load_from_bytes(&bytes) {
                        Ok(mut tex) => {
                            tex.source = source;
                            let name = tex.name.clone();
                            let id = tex.id;

                            match source {
                                TextureSource::Sample => self.sample_names.push(name.clone()),
                                TextureSource::User => self.user_names.push(name.clone()),
                            }

                            self.by_id.insert(id, name.clone());
                            self.textures.insert(name, tex);
                            loaded += 1;
                        }
                        Err(e) => {
                            eprintln!("Failed to parse texture {}: {}", filename, e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to read texture {}: {}", filename, e);
                }
            }
        }

        Ok(loaded)
    }

    /// Discover textures using storage (WASM stub)
    #[cfg(target_arch = "wasm32")]
    pub fn discover_with_storage(&mut self, _storage: &Storage) -> Result<usize, TextureError> {
        self.textures.clear();
        self.sample_names.clear();
        self.user_names.clear();
        self.by_id.clear();
        Ok(0)
    }

    /// Save a texture using the storage backend
    ///
    /// Only user textures can be saved. Sample textures are read-only.
    pub fn save_texture_with_storage(&self, name: &str, storage: &Storage) -> Result<(), TextureError> {
        let tex = self
            .textures
            .get(name)
            .ok_or_else(|| TextureError::ValidationError(format!("texture '{}' not found", name)))?;

        // Check if this is a sample texture (read-only)
        if tex.source == TextureSource::Sample {
            return Err(TextureError::ValidationError(
                "cannot save sample texture - it is read-only".to_string(),
            ));
        }

        let path = format!("{}/{}.ron", USER_TEXTURES_DIR, name);

        // Serialize the texture
        let content = tex.to_ron_string()?;

        storage
            .write_sync(&path, content.as_bytes())
            .map_err(|e| TextureError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            )))
    }

    /// Delete a texture file using the storage backend
    ///
    /// Only user textures can be deleted. Sample textures are read-only.
    pub fn delete_texture_with_storage(&mut self, name: &str, storage: &Storage) -> Result<(), TextureError> {
        // Check if this is a sample texture (read-only)
        if let Some(tex) = self.textures.get(name) {
            if tex.source == TextureSource::Sample {
                return Err(TextureError::ValidationError(
                    "cannot delete sample texture - it is read-only".to_string(),
                ));
            }
        }

        let path = format!("{}/{}.ron", USER_TEXTURES_DIR, name);

        storage
            .delete_sync(&path)
            .map_err(|e| TextureError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            )))?;

        self.remove(name);
        Ok(())
    }

    /// Regenerate user manifest using storage backend
    ///
    /// Only regenerates the user textures manifest, not samples.
    pub fn regenerate_manifest_with_storage(&self, storage: &Storage) -> Result<(), TextureError> {
        let manifest_path = format!("{}/{}", USER_TEXTURES_DIR, MANIFEST_FILE);

        let mut manifest = String::new();
        for name in &self.user_names {
            manifest.push_str(&format!("{}.ron\n", name));
        }

        storage
            .write_sync(&manifest_path, manifest.as_bytes())
            .map_err(|e| TextureError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rasterizer::ClutDepth;
    use crate::texture::TextureSize;

    #[test]
    fn test_library_operations() {
        let mut lib = TextureLibrary::new();

        // Add a texture
        let tex = UserTexture::new("test_texture", TextureSize::Size64x64, ClutDepth::Bpp4);
        lib.add(tex);

        assert_eq!(lib.len(), 1);
        assert!(lib.contains("test_texture"));
        assert!(lib.get("test_texture").is_some());

        // Remove it
        let removed = lib.remove("test_texture");
        assert!(removed.is_some());
        assert_eq!(lib.len(), 0);
    }

    #[test]
    fn test_unique_name_generation() {
        let mut lib = TextureLibrary::new();

        // First name should be used as-is
        assert_eq!(lib.generate_unique_name("texture"), "texture");

        // Add it
        lib.add(UserTexture::new(
            "texture",
            TextureSize::Size64x64,
            ClutDepth::Bpp4,
        ));

        // Now it should generate a unique name
        assert_eq!(lib.generate_unique_name("texture"), "texture_1");

        lib.add(UserTexture::new(
            "texture_1",
            TextureSize::Size64x64,
            ClutDepth::Bpp4,
        ));
        assert_eq!(lib.generate_unique_name("texture"), "texture_2");
    }

    #[test]
    fn test_next_available_name() {
        let mut lib = TextureLibrary::new();

        // Empty library should start at texture_001
        assert_eq!(lib.next_available_name(), "texture_001");

        // Add texture_001
        lib.add(UserTexture::new(
            "texture_001",
            TextureSize::Size64x64,
            ClutDepth::Bpp4,
        ));
        assert_eq!(lib.next_available_name(), "texture_002");

        // Add texture_005 (gap)
        lib.add(UserTexture::new(
            "texture_005",
            TextureSize::Size64x64,
            ClutDepth::Bpp4,
        ));
        // Should use highest + 1, so texture_006
        assert_eq!(lib.next_available_name(), "texture_006");

        // Non-numbered textures should be ignored
        lib.add(UserTexture::new(
            "my_custom_texture",
            TextureSize::Size64x64,
            ClutDepth::Bpp4,
        ));
        assert_eq!(lib.next_available_name(), "texture_006");
    }
}
