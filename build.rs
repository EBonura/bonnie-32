//! Build script to generate manifests for WASM builds
//!
//! Scans asset directories and creates manifests listing all files, since WASM
//! can't enumerate directories at runtime.

use std::fs;
use std::io::Write;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=assets/samples/textures");
    println!("cargo:rerun-if-changed=assets/samples/levels");
    println!("cargo:rerun-if-changed=assets/samples/assets");
    println!("cargo:rerun-if-changed=assets/samples/meshes");
    println!("cargo:rerun-if-changed=assets/samples/songs");

    generate_texture_manifest();
    generate_levels_manifest();
    generate_models_manifest();
    generate_meshes_manifest();
    generate_songs_manifest();
}

/// Generate manifest for texture packs
fn generate_texture_manifest() {
    let textures_dir = Path::new("assets/samples/textures");
    let manifest_path = Path::new("assets/samples/textures/manifest.txt");

    let mut manifest = String::new();

    if textures_dir.exists() {
        let mut packs: Vec<_> = fs::read_dir(textures_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();

        packs.sort_by_key(|e| e.file_name());

        for pack_entry in packs {
            let pack_path = pack_entry.path();
            let pack_name = pack_entry.file_name().to_string_lossy().to_string();

            // Get all PNG files in the pack
            let mut textures: Vec<_> = fs::read_dir(&pack_path)
                .unwrap()
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path()
                        .extension()
                        .map(|ext| ext.to_ascii_lowercase() == "png")
                        .unwrap_or(false)
                })
                .collect();

            textures.sort_by_key(|e| e.file_name());

            if !textures.is_empty() {
                // Pack header: [pack_name]
                manifest.push_str(&format!("[{}]\n", pack_name));

                for tex_entry in textures {
                    let tex_name = tex_entry.file_name().to_string_lossy().to_string();
                    manifest.push_str(&format!("{}\n", tex_name));
                }

                manifest.push('\n');
            }
        }
    }

    // Write manifest file
    if let Some(parent) = manifest_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut file = fs::File::create(manifest_path).unwrap();
    file.write_all(manifest.as_bytes()).unwrap();
}

/// Generate manifest for levels (for WASM builds)
fn generate_levels_manifest() {
    let levels_dir = Path::new("assets/samples/levels");
    let manifest_path = Path::new("assets/samples/levels/manifest.txt");

    let mut manifest = String::new();

    if levels_dir.exists() {
        let mut levels: Vec<_> = fs::read_dir(levels_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                // Only include .ron files, skip directories
                path.is_file() && path
                    .extension()
                    .map(|ext| ext.to_ascii_lowercase() == "ron")
                    .unwrap_or(false)
            })
            .collect();

        levels.sort_by_key(|e| e.file_name());

        for level_entry in levels {
            let level_name = level_entry.file_name().to_string_lossy().to_string();
            manifest.push_str(&format!("{}\n", level_name));
        }
    }

    // Write manifest file
    if let Some(parent) = manifest_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut file = fs::File::create(manifest_path).unwrap();
    file.write_all(manifest.as_bytes()).unwrap();
}

/// Generate manifest for models/assets (for WASM builds)
fn generate_models_manifest() {
    let models_dir = Path::new("assets/samples/assets");
    let manifest_path = Path::new("assets/samples/assets/manifest.txt");

    let mut manifest = String::new();

    if models_dir.exists() {
        let mut models: Vec<_> = fs::read_dir(models_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                // Only include .ron files, skip directories
                path.is_file()
                    && path
                        .extension()
                        .map(|ext| ext.to_ascii_lowercase() == "ron")
                        .unwrap_or(false)
            })
            .collect();

        models.sort_by_key(|e| e.file_name());

        for model_entry in models {
            let model_name = model_entry.file_name().to_string_lossy().to_string();
            manifest.push_str(&format!("{}\n", model_name));
        }
    }

    // Write manifest file
    if let Some(parent) = manifest_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut file = fs::File::create(manifest_path).unwrap();
    file.write_all(manifest.as_bytes()).unwrap();
}

/// Generate manifest for meshes (for WASM builds)
fn generate_meshes_manifest() {
    let meshes_dir = Path::new("assets/samples/meshes");
    let manifest_path = Path::new("assets/samples/meshes/manifest.txt");

    let mut manifest = String::new();

    if meshes_dir.exists() {
        let mut meshes: Vec<_> = fs::read_dir(meshes_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                // Only include .obj files, skip directories
                path.is_file()
                    && path
                        .extension()
                        .map(|ext| ext.to_ascii_lowercase() == "obj")
                        .unwrap_or(false)
            })
            .collect();

        meshes.sort_by_key(|e| e.file_name());

        for mesh_entry in meshes {
            let mesh_name = mesh_entry.file_name().to_string_lossy().to_string();
            manifest.push_str(&format!("{}\n", mesh_name));
        }
    }

    // Write manifest file
    if let Some(parent) = manifest_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut file = fs::File::create(manifest_path).unwrap();
    file.write_all(manifest.as_bytes()).unwrap();
}

/// Generate manifest for songs (for WASM builds)
fn generate_songs_manifest() {
    let songs_dir = Path::new("assets/samples/songs");
    let manifest_path = Path::new("assets/samples/songs/manifest.txt");

    let mut manifest = String::new();

    if songs_dir.exists() {
        let mut songs: Vec<_> = fs::read_dir(songs_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                // Only include .ron files, skip directories
                path.is_file()
                    && path
                        .extension()
                        .map(|ext| ext.to_ascii_lowercase() == "ron")
                        .unwrap_or(false)
            })
            .collect();

        songs.sort_by_key(|e| e.file_name());

        for song_entry in songs {
            let song_name = song_entry.file_name().to_string_lossy().to_string();
            manifest.push_str(&format!("{}\n", song_name));
        }
    }

    // Write manifest file (create directory if needed)
    if let Some(parent) = manifest_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut file = fs::File::create(manifest_path).unwrap();
    file.write_all(manifest.as_bytes()).unwrap();
}
