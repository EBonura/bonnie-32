//! Level browser for the editor
//!
//! Handles loading bundled levels from disk (native) or via manifest (WASM).

use std::path::PathBuf;
use crate::world::{Level, load_level};

#[cfg(target_arch = "wasm32")]
use crate::world::load_level_from_str;

/// Metadata about a level (without loading the full level)
#[derive(Debug, Clone)]
pub struct ExampleLevelInfo {
    /// Display name (filename without extension)
    pub name: String,
    /// Full path to the level file
    pub path: PathBuf,
}

/// Discover all levels in the levels directory (native)
#[cfg(not(target_arch = "wasm32"))]
pub fn discover_examples() -> Vec<ExampleLevelInfo> {
    let levels_dir = PathBuf::from("assets/userdata/levels");
    let mut levels = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&levels_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            // Only include .ron files, skip directories
            if path.is_file() && path.extension().map(|e| e == "ron").unwrap_or(false) {
                let name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unnamed".to_string());
                levels.push(ExampleLevelInfo { name, path });
            }
        }
    }

    levels.sort_by(|a, b| a.name.cmp(&b.name));
    levels
}

/// Discover all levels from manifest (WASM)
#[cfg(target_arch = "wasm32")]
pub fn discover_examples() -> Vec<ExampleLevelInfo> {
    // On WASM, we return empty here and load async later
    Vec::new()
}

/// Load level list from manifest asynchronously (for WASM)
pub async fn load_example_list() -> Vec<ExampleLevelInfo> {
    use macroquad::prelude::*;

    // Load and parse manifest
    let manifest = match load_string("assets/userdata/levels/manifest.txt").await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to load levels manifest: {}", e);
            return Vec::new();
        }
    };

    let mut levels = Vec::new();

    for line in manifest.lines() {
        let line = line.trim();
        if line.is_empty() || !line.ends_with(".ron") {
            continue;
        }

        let name = line
            .strip_suffix(".ron")
            .unwrap_or(line)
            .to_string();
        let path = PathBuf::from(format!("assets/userdata/levels/{}", line));

        levels.push(ExampleLevelInfo { name, path });
    }

    levels
}

/// Load a specific example level by path
pub async fn load_example_level(path: &PathBuf) -> Option<Level> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        load_level(path).ok()
    }

    #[cfg(target_arch = "wasm32")]
    {
        use macroquad::prelude::*;
        use std::io::Cursor;

        // Convert path to string for fetch - ensure forward slashes
        let path_str = path.to_string_lossy().replace('\\', "/");

        // Load as binary to support both compressed and uncompressed
        let bytes = match load_file(&path_str).await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Failed to load level file {}: {}", path_str, e);
                return None;
            }
        };

        // Detect format: RON files start with '(' or whitespace, brotli is binary
        let is_plain_ron = bytes.first().map(|&b| b == b'(' || b == b' ' || b == b'\n' || b == b'\r' || b == b'\t').unwrap_or(false);

        let contents = if is_plain_ron {
            match String::from_utf8(bytes) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Invalid UTF-8 in level file {}: {}", path_str, e);
                    return None;
                }
            }
        } else {
            // Brotli compressed - decompress first
            let mut decompressed = Vec::new();
            match brotli::BrotliDecompress(&mut Cursor::new(&bytes), &mut decompressed) {
                Ok(_) => match String::from_utf8(decompressed) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Invalid UTF-8 after decompression {}: {}", path_str, e);
                        return None;
                    }
                },
                Err(e) => {
                    eprintln!("Failed to decompress level {}: {}", path_str, e);
                    return None;
                }
            }
        };

        match load_level_from_str(&contents) {
            Ok(level) => Some(level),
            Err(e) => {
                eprintln!("Failed to parse level {}: {}", path_str, e);
                None
            }
        }
    }
}

/// Get level statistics without fully loading (for preview info)
pub fn get_level_stats(level: &Level) -> LevelStats {
    let room_count = level.rooms.len();
    let mut sector_count = 0;
    let mut floor_count = 0;
    let mut wall_count = 0;

    for room in &level.rooms {
        for row in &room.sectors {
            for sector_opt in row {
                if let Some(sector) = sector_opt {
                    sector_count += 1;
                    if sector.floor.is_some() {
                        floor_count += 1;
                    }
                    wall_count += sector.walls_north.len();
                    wall_count += sector.walls_east.len();
                    wall_count += sector.walls_south.len();
                    wall_count += sector.walls_west.len();
                }
            }
        }
    }

    LevelStats {
        room_count,
        sector_count,
        floor_count,
        wall_count,
    }
}

/// Statistics about a level
#[derive(Debug, Clone)]
pub struct LevelStats {
    pub room_count: usize,
    pub sector_count: usize,
    pub floor_count: usize,
    pub wall_count: usize,
}
