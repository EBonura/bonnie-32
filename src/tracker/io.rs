//! Song file I/O for the tracker
//!
//! Saves and loads songs in RON format (.ron extension).
//! Supports both compressed (zstd) and uncompressed RON files.
//! - Reading: Auto-detects format by magic bytes
//! - Writing: Always uses zstd compression

use std::fs;
use std::io::Cursor;
use std::path::Path;

use super::pattern::Song;

/// Zstd magic bytes: 0x28 0xB5 0x2F 0xFD
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

/// Save a song to a file in compressed RON format (zstd)
pub fn save_song(song: &Song, path: &Path) -> Result<(), String> {
    let config = ron::ser::PrettyConfig::new()
        .depth_limit(8)
        .indentor("  ".to_string());

    let contents = ron::ser::to_string_pretty(song, config)
        .map_err(|e| format!("Failed to serialize song: {}", e))?;

    // Compress with zstd (level 3 is a good balance of speed/ratio)
    let compressed = zstd::encode_all(Cursor::new(contents.as_bytes()), 3)
        .map_err(|e| format!("Failed to compress: {}", e))?;

    fs::write(path, compressed).map_err(|e| format!("Failed to write file: {}", e))?;

    Ok(())
}

/// Load a song from a RON file (supports both compressed and uncompressed)
pub fn load_song(path: &Path) -> Result<Song, String> {
    let bytes = fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;

    // Detect format by magic bytes: zstd vs plain RON text
    let contents = if bytes.starts_with(&ZSTD_MAGIC) {
        // Zstd compressed - decompress first
        let decompressed = zstd::decode_all(Cursor::new(&bytes))
            .map_err(|e| format!("Failed to decompress: {}", e))?;
        String::from_utf8(decompressed)
            .map_err(|e| format!("Invalid UTF-8 after decompression: {}", e))?
    } else {
        // Plain RON text
        String::from_utf8(bytes)
            .map_err(|e| format!("Invalid UTF-8: {}", e))?
    };

    load_song_from_str(&contents)
}

/// Load a song from a RON string (for WASM async loading)
pub fn load_song_from_str(contents: &str) -> Result<Song, String> {
    let song: Song =
        ron::from_str(contents).map_err(|e| format!("Failed to parse song: {}", e))?;

    Ok(song)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_save_and_load_song() {
        let song = Song::new();

        // Save to temp file
        let mut temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_path_buf();

        save_song(&song, &path).unwrap();

        // Load back
        let loaded = load_song(&path).unwrap();

        assert_eq!(loaded.name, song.name);
        assert_eq!(loaded.bpm, song.bpm);
        assert_eq!(loaded.patterns.len(), song.patterns.len());
    }

    #[test]
    fn test_load_invalid_file() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "not valid ron data").unwrap();

        let result = load_song(temp_file.path());
        assert!(result.is_err());
    }
}
