//! Build script to generate texture manifest for WASM builds
//!
//! Scans assets/textures/ and creates a manifest listing all texture packs
//! and their files, since WASM can't enumerate directories at runtime.

use std::fs;
use std::io::Write;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=assets/textures");

    let textures_dir = Path::new("assets/textures");
    let manifest_path = Path::new("assets/textures/manifest.txt");

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
    let mut file = fs::File::create(manifest_path).unwrap();
    file.write_all(manifest.as_bytes()).unwrap();
}
