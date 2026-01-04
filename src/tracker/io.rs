//! Song file I/O for the tracker
//!
//! Saves and loads songs in RON format (.ron extension)

use std::fs;
use std::path::Path;

use super::pattern::Song;

/// Save a song to a file in RON format
pub fn save_song(song: &Song, path: &Path) -> Result<(), String> {
    let config = ron::ser::PrettyConfig::new()
        .depth_limit(8)
        .indentor("  ".to_string());

    let contents = ron::ser::to_string_pretty(song, config)
        .map_err(|e| format!("Failed to serialize song: {}", e))?;

    fs::write(path, contents).map_err(|e| format!("Failed to write file: {}", e))?;

    Ok(())
}

/// Load a song from a RON file
pub fn load_song(path: &Path) -> Result<Song, String> {
    let contents =
        fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;

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
