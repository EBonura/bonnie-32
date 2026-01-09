//! Texture library - discovery and caching of user textures
//!
//! Manages the collection of user-created indexed textures stored in
//! `assets/textures-user/`. Handles both native filesystem discovery
//! and WASM manifest-based loading.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::user_texture::{TextureError, UserTexture};

/// Directory where user textures are stored
pub const TEXTURES_USER_DIR: &str = "assets/textures-user";

/// Manifest file for WASM texture loading
pub const MANIFEST_FILE: &str = "manifest.txt";

/// A library of user-created textures
///
/// Provides discovery, loading, and caching of textures from the
/// `assets/textures-user/` directory.
#[derive(Debug, Default)]
pub struct TextureLibrary {
    /// Loaded textures keyed by name (without extension)
    textures: HashMap<String, UserTexture>,
    /// List of discovered texture names (for iteration order)
    texture_names: Vec<String>,
    /// Base directory for textures
    base_dir: PathBuf,
}

impl TextureLibrary {
    /// Create a new empty texture library
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
            texture_names: Vec::new(),
            base_dir: PathBuf::from(TEXTURES_USER_DIR),
        }
    }

    /// Create a library with a custom base directory
    pub fn with_dir(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            textures: HashMap::new(),
            texture_names: Vec::new(),
            base_dir: base_dir.into(),
        }
    }

    /// Discover and load all textures from the base directory
    #[cfg(not(target_arch = "wasm32"))]
    pub fn discover(&mut self) -> Result<usize, TextureError> {
        self.textures.clear();
        self.texture_names.clear();

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

        let mut loaded = 0;
        for path in entries {
            match UserTexture::load(&path) {
                Ok(tex) => {
                    let name = tex.name.clone();
                    self.texture_names.push(name.clone());
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

    /// Load textures from manifest (for WASM)
    ///
    /// The manifest file should contain one texture filename per line (without path).
    #[cfg(target_arch = "wasm32")]
    pub async fn discover_from_manifest(&mut self) -> Result<usize, TextureError> {
        use macroquad::prelude::load_string;

        self.textures.clear();
        self.texture_names.clear();

        let manifest_path = format!("{}/{}", TEXTURES_USER_DIR, MANIFEST_FILE);
        let manifest = match load_string(&manifest_path).await {
            Ok(m) => m,
            Err(_) => {
                // No manifest, no textures
                return Ok(0);
            }
        };

        let mut loaded = 0;
        for line in manifest.lines() {
            let filename = line.trim();
            if filename.is_empty() || filename.starts_with('#') {
                continue;
            }

            let path = format!("{}/{}", TEXTURES_USER_DIR, filename);
            match macroquad::prelude::load_file(&path).await {
                Ok(bytes) => match UserTexture::load_from_bytes(&bytes) {
                    Ok(tex) => {
                        let name = tex.name.clone();
                        self.texture_names.push(name.clone());
                        self.textures.insert(name, tex);
                        loaded += 1;
                    }
                    Err(e) => {
                        eprintln!("Failed to parse texture {}: {}", filename, e);
                    }
                },
                Err(e) => {
                    eprintln!("Failed to load texture file {}: {}", filename, e);
                }
            }
        }

        Ok(loaded)
    }

    /// Get a texture by name
    pub fn get(&self, name: &str) -> Option<&UserTexture> {
        self.textures.get(name)
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
    pub fn add(&mut self, texture: UserTexture) {
        let name = texture.name.clone();
        if !self.textures.contains_key(&name) {
            self.texture_names.push(name.clone());
        }
        self.textures.insert(name, texture);
    }

    /// Remove a texture by name
    pub fn remove(&mut self, name: &str) -> Option<UserTexture> {
        if let Some(tex) = self.textures.remove(name) {
            self.texture_names.retain(|n| n != name);
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

    /// Iterate over texture names in discovery order
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.texture_names.iter().map(|s| s.as_str())
    }

    /// Iterate over all textures
    pub fn iter(&self) -> impl Iterator<Item = (&str, &UserTexture)> {
        self.texture_names
            .iter()
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

    /// Save a texture to disk (native only)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_texture(&self, name: &str) -> Result<(), TextureError> {
        let tex = self
            .textures
            .get(name)
            .ok_or_else(|| TextureError::ValidationError(format!("texture '{}' not found", name)))?;

        // Ensure directory exists
        std::fs::create_dir_all(&self.base_dir)?;

        let path = self.base_dir.join(format!("{}.ron", name));
        tex.save(&path)
    }

    /// Save all textures to disk (native only)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_all(&self) -> Result<usize, TextureError> {
        std::fs::create_dir_all(&self.base_dir)?;

        let mut saved = 0;
        for (name, tex) in &self.textures {
            let path = self.base_dir.join(format!("{}.ron", name));
            tex.save(&path)?;
            saved += 1;
        }
        Ok(saved)
    }

    /// Delete a texture file from disk (native only)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn delete_texture_file(&mut self, name: &str) -> Result<(), TextureError> {
        let path = self.base_dir.join(format!("{}.ron", name));
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        self.remove(name);
        Ok(())
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
    /// Creates a manifest.txt file listing all textures for WASM loading.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn regenerate_manifest(&self) -> Result<(), TextureError> {
        std::fs::create_dir_all(&self.base_dir)?;

        let manifest_path = self.base_dir.join(MANIFEST_FILE);
        let mut manifest = String::new();

        for name in &self.texture_names {
            manifest.push_str(&format!("{}.ron\n", name));
        }

        std::fs::write(manifest_path, manifest)?;
        Ok(())
    }

    /// Get the base directory path
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
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
}
