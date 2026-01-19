//! Texture pack loading for the editor
//!
//! Handles loading texture packs from disk (native) or via JavaScript cache (WASM).

use std::path::PathBuf;
use crate::rasterizer::Texture;

/// A texture pack loaded from a folder
pub struct TexturePack {
    pub name: String,
    pub path: PathBuf,
    pub textures: Vec<Texture>,
}

impl TexturePack {
    /// Load a texture pack from a directory (native only)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_directory(path: PathBuf) -> Option<Self> {
        let name = path.file_name()?.to_string_lossy().to_string();
        let mut textures = Texture::load_directory(&path);

        // Quantize all textures to PS1 15-bit color depth
        for tex in &mut textures {
            tex.quantize_15bit();
        }

        if textures.is_empty() {
            // Try loading from subdirectories (some packs have nested folders)
            let mut all_textures = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&path) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let entry_path = entry.path();
                    if entry_path.is_dir() {
                        all_textures.extend(Texture::load_directory(&entry_path));
                    }
                }
            }
            // Quantize subdirectory textures too
            for tex in &mut all_textures {
                tex.quantize_15bit();
            }
            if all_textures.is_empty() {
                return None;
            }
            Some(Self { name, path, textures: all_textures })
        } else {
            Some(Self { name, path, textures })
        }
    }

    /// Discover all texture packs in the assets/samples/textures directory (native only)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn discover_all() -> Vec<Self> {
        let textures_dir = PathBuf::from("assets/samples/textures");
        let mut packs = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&textures_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(pack) = Self::from_directory(path) {
                        packs.push(pack);
                    }
                }
            }
        }

        packs.sort_by(|a, b| a.name.cmp(&b.name));
        packs
    }

    /// Discover all texture packs from manifest (WASM stub - returns empty, loaded async later)
    #[cfg(target_arch = "wasm32")]
    pub fn discover_all() -> Vec<Self> {
        Vec::new()
    }

    /// Load texture packs from manifest asynchronously.
    /// On WASM: JavaScript prefetches and decodes PNGs in parallel, Rust just copies raw RGBA.
    /// On native: Falls back to load_file + PNG decoding.
    /// Note: Primarily used in WASM builds, may appear unused in native builds
    #[allow(dead_code)]
    pub async fn load_from_manifest() -> Vec<Self> {
        use macroquad::prelude::*;

        // Load and parse manifest
        let manifest = match load_string("assets/samples/textures/manifest.txt").await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to load texture manifest: {}", e);
                wasm::hide_loading();
                return Vec::new();
            }
        };

        let pack_files = parse_manifest(&manifest);
        let mut packs = Vec::new();

        for (pack_name, files) in pack_files {
            wasm::set_status(&format!("Loading {}...", pack_name));

            let mut textures = Vec::with_capacity(files.len());
            for filename in &files {
                if let Some(mut tex) = load_single_texture(&pack_name, filename).await {
                    // Quantize to PS1 15-bit color depth
                    tex.quantize_15bit();
                    textures.push(tex);
                }
            }

            if !textures.is_empty() {
                packs.push(TexturePack {
                    name: pack_name.clone(),
                    path: PathBuf::from(format!("assets/samples/textures/{}", pack_name)),
                    textures,
                });
            }
        }

        println!("Loaded {} texture packs from manifest", packs.len());
        wasm::hide_loading();
        packs
    }
}

/// Parse manifest file into (pack_name, filenames) pairs
#[allow(dead_code)]
fn parse_manifest(manifest: &str) -> Vec<(String, Vec<String>)> {
    let mut result = Vec::new();
    let mut current_pack: Option<String> = None;
    let mut current_files = Vec::new();

    for line in manifest.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            // Save previous pack
            if let Some(name) = current_pack.take() {
                if !current_files.is_empty() {
                    result.push((name, std::mem::take(&mut current_files)));
                }
            }
            current_pack = Some(line[1..line.len() - 1].to_string());
        } else if current_pack.is_some() {
            current_files.push(line.to_string());
        }
    }

    // Don't forget last pack
    if let Some(name) = current_pack {
        if !current_files.is_empty() {
            result.push((name, current_files));
        }
    }

    result
}

/// Load a single texture from pack
#[allow(dead_code)]
async fn load_single_texture(pack_name: &str, filename: &str) -> Option<Texture> {
    let tex_path = format!("assets/samples/textures/{}/{}", pack_name, filename);
    let tex_name = filename
        .strip_suffix(".png")
        .or_else(|| filename.strip_suffix(".PNG"))
        .unwrap_or(filename)
        .to_string();

    #[cfg(target_arch = "wasm32")]
    {
        wasm::load_cached_texture(&tex_path, tex_name)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        use macroquad::prelude::load_file;
        match load_file(&tex_path).await {
            Ok(bytes) => Texture::from_bytes(&bytes, tex_name).ok(),
            Err(_) => None,
        }
    }
}

/// WASM-specific FFI and helpers
mod wasm {
    #[cfg(target_arch = "wasm32")]
    use crate::rasterizer::{Color, Texture};

    #[cfg(target_arch = "wasm32")]
    extern "C" {
        fn b32_set_loading_status(ptr: *const u8, len: usize);
        fn b32_hide_loading();
        fn b32_get_cached_texture_info(path_ptr: *const u8, path_len: usize) -> u32;
        fn b32_copy_cached_texture(
            path_ptr: *const u8,
            path_len: usize,
            dest_ptr: *mut u8,
            max_len: usize,
        ) -> usize;
    }

    /// Update loading status text
    #[cfg(target_arch = "wasm32")]
    pub fn set_status(msg: &str) {
        unsafe { b32_set_loading_status(msg.as_ptr(), msg.len()) }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_status(_msg: &str) {}

    /// Hide the loading overlay
    #[cfg(target_arch = "wasm32")]
    pub fn hide_loading() {
        unsafe { b32_hide_loading() }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn hide_loading() {}

    /// Load texture from JavaScript cache (pre-decoded RGBA)
    #[cfg(target_arch = "wasm32")]
    pub fn load_cached_texture(path: &str, name: String) -> Option<Texture> {
        /// Maximum texture dimension to prevent memory exhaustion
        const MAX_TEXTURE_DIM: usize = 4096;

        unsafe {
            let info = b32_get_cached_texture_info(path.as_ptr(), path.len());
            if info == 0 {
                return None;
            }

            let width = (info >> 16) as usize;
            let height = (info & 0xFFFF) as usize;

            // Security: Validate dimensions before allocation
            if width == 0 || height == 0 || width > MAX_TEXTURE_DIM || height > MAX_TEXTURE_DIM {
                eprintln!("Invalid texture dimensions {}x{} for {}", width, height, path);
                return None;
            }

            let rgba_size = width * height * 4;

            let mut rgba_buffer = vec![0u8; rgba_size];
            let copied = b32_copy_cached_texture(
                path.as_ptr(),
                path.len(),
                rgba_buffer.as_mut_ptr(),
                rgba_size,
            );

            if copied != rgba_size {
                return None;
            }

            let pixels: Vec<Color> = rgba_buffer
                .chunks_exact(4)
                .map(|c| {
                    // Convert alpha to blend mode: 0 = transparent, otherwise opaque
                    let blend = if c[3] == 0 { crate::rasterizer::BlendMode::Erase } else { crate::rasterizer::BlendMode::Opaque };
                    Color::with_blend(c[0], c[1], c[2], blend)
                })
                .collect();

            Some(Texture {
                width,
                height,
                pixels,
                name,
                blend_mode: crate::rasterizer::BlendMode::Opaque,
            })
        }
    }
}
