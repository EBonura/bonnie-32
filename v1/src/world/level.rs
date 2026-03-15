//! Level loading and saving
//!
//! Uses RON (Rusty Object Notation) for human-readable level files.
//! Supports both compressed (brotli) and uncompressed RON files.
//! - Reading: Auto-detects format by checking for valid RON start
//! - Writing: Always uses brotli compression

use std::fs;
use std::io::Cursor;
use std::path::Path;
use super::{Level, Room, Sector, HorizontalFace, VerticalFace, TextureRef};

/// Validation limits to prevent resource exhaustion from malicious files
pub mod limits {
    /// Maximum number of rooms in a level
    pub const MAX_ROOMS: usize = 256;
    /// Maximum grid dimension (width or depth) for a room
    pub const MAX_ROOM_SIZE: usize = 128;
    /// Maximum walls per sector edge
    pub const MAX_WALLS_PER_EDGE: usize = 16;
    /// Maximum string length for texture names
    pub const MAX_STRING_LEN: usize = 256;
    /// Maximum coordinate value (prevents overflow issues)
    pub const MAX_COORD: f32 = 1_000_000.0;
}

/// Error type for level loading
#[derive(Debug)]
pub enum LevelError {
    IoError(std::io::Error),
    ParseError(ron::error::SpannedError),
    SerializeError(ron::Error),
    ValidationError(String),
}

impl From<std::io::Error> for LevelError {
    fn from(e: std::io::Error) -> Self {
        LevelError::IoError(e)
    }
}

impl From<ron::error::SpannedError> for LevelError {
    fn from(e: ron::error::SpannedError) -> Self {
        LevelError::ParseError(e)
    }
}

impl From<ron::Error> for LevelError {
    fn from(e: ron::Error) -> Self {
        LevelError::SerializeError(e)
    }
}

impl std::fmt::Display for LevelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LevelError::IoError(e) => write!(f, "IO error: {}", e),
            LevelError::ParseError(e) => write!(f, "Parse error: {}", e),
            LevelError::SerializeError(e) => write!(f, "Serialize error: {}", e),
            LevelError::ValidationError(e) => write!(f, "Validation error: {}", e),
        }
    }
}

/// Check if a float is valid (not NaN or Inf)
fn is_valid_float(f: f32) -> bool {
    f.is_finite() && f.abs() <= limits::MAX_COORD
}

/// Check if a portal coordinate is valid (allows infinity for unbounded portals)
/// Portal Y coordinates can be infinite for open-air sectors with no ceiling
fn is_valid_portal_coord(f: f32) -> bool {
    !f.is_nan() && (f.is_finite() && f.abs() <= limits::MAX_COORD || f.is_infinite())
}

/// Validate a texture reference
fn validate_texture_ref(tex: &TextureRef, context: &str) -> Result<(), String> {
    if tex.pack.len() > limits::MAX_STRING_LEN {
        return Err(format!("{}: texture pack name too long ({} > {})",
            context, tex.pack.len(), limits::MAX_STRING_LEN));
    }
    if tex.name.len() > limits::MAX_STRING_LEN {
        return Err(format!("{}: texture name too long ({} > {})",
            context, tex.name.len(), limits::MAX_STRING_LEN));
    }
    Ok(())
}

/// Validate a horizontal face (floor/ceiling)
fn validate_horizontal_face(face: &HorizontalFace, context: &str) -> Result<(), String> {
    for (i, h) in face.heights.iter().enumerate() {
        if !is_valid_float(*h) {
            return Err(format!("{}: invalid height[{}] = {}", context, i, h));
        }
    }
    validate_texture_ref(&face.texture, context)?;
    Ok(())
}

/// Validate a vertical face (wall)
fn validate_vertical_face(face: &VerticalFace, context: &str) -> Result<(), String> {
    for (i, h) in face.heights.iter().enumerate() {
        if !is_valid_float(*h) {
            return Err(format!("{}: invalid height[{}] = {}", context, i, h));
        }
    }
    validate_texture_ref(&face.texture, context)?;
    Ok(())
}

/// Validate a sector
fn validate_sector(sector: &Sector, context: &str) -> Result<(), String> {
    if let Some(floor) = &sector.floor {
        validate_horizontal_face(floor, &format!("{} floor", context))?;
    }
    if let Some(ceiling) = &sector.ceiling {
        validate_horizontal_face(ceiling, &format!("{} ceiling", context))?;
    }

    // Check wall counts
    if sector.walls_north.len() > limits::MAX_WALLS_PER_EDGE {
        return Err(format!("{}: too many north walls ({} > {})",
            context, sector.walls_north.len(), limits::MAX_WALLS_PER_EDGE));
    }
    if sector.walls_east.len() > limits::MAX_WALLS_PER_EDGE {
        return Err(format!("{}: too many east walls ({} > {})",
            context, sector.walls_east.len(), limits::MAX_WALLS_PER_EDGE));
    }
    if sector.walls_south.len() > limits::MAX_WALLS_PER_EDGE {
        return Err(format!("{}: too many south walls ({} > {})",
            context, sector.walls_south.len(), limits::MAX_WALLS_PER_EDGE));
    }
    if sector.walls_west.len() > limits::MAX_WALLS_PER_EDGE {
        return Err(format!("{}: too many west walls ({} > {})",
            context, sector.walls_west.len(), limits::MAX_WALLS_PER_EDGE));
    }

    // Validate each wall
    for (i, wall) in sector.walls_north.iter().enumerate() {
        validate_vertical_face(wall, &format!("{} walls_north[{}]", context, i))?;
    }
    for (i, wall) in sector.walls_east.iter().enumerate() {
        validate_vertical_face(wall, &format!("{} walls_east[{}]", context, i))?;
    }
    for (i, wall) in sector.walls_south.iter().enumerate() {
        validate_vertical_face(wall, &format!("{} walls_south[{}]", context, i))?;
    }
    for (i, wall) in sector.walls_west.iter().enumerate() {
        validate_vertical_face(wall, &format!("{} walls_west[{}]", context, i))?;
    }

    Ok(())
}

/// Validate a room
fn validate_room(room: &Room, room_idx: usize, total_rooms: usize) -> Result<(), String> {
    let context = format!("room[{}]", room_idx);

    // Check room dimensions
    if room.width > limits::MAX_ROOM_SIZE {
        return Err(format!("{}: width too large ({} > {})",
            context, room.width, limits::MAX_ROOM_SIZE));
    }
    if room.depth > limits::MAX_ROOM_SIZE {
        return Err(format!("{}: depth too large ({} > {})",
            context, room.depth, limits::MAX_ROOM_SIZE));
    }

    // Check position is valid
    if !is_valid_float(room.position.x) || !is_valid_float(room.position.y) || !is_valid_float(room.position.z) {
        return Err(format!("{}: invalid position ({}, {}, {})",
            context, room.position.x, room.position.y, room.position.z));
    }

    // Check sectors array matches dimensions
    if room.sectors.len() != room.width {
        return Err(format!("{}: sectors array width mismatch ({} != {})",
            context, room.sectors.len(), room.width));
    }
    for (x, col) in room.sectors.iter().enumerate() {
        if col.len() != room.depth {
            return Err(format!("{}: sectors[{}] depth mismatch ({} != {})",
                context, x, col.len(), room.depth));
        }
    }

    // Validate portals (no count limit - portals are dynamically calculated)
    for (i, portal) in room.portals.iter().enumerate() {
        if portal.target_room >= total_rooms {
            return Err(format!("{} portal[{}]: invalid target_room {} (only {} rooms)",
                context, i, portal.target_room, total_rooms));
        }
        // Validate portal vertices (Y can be infinite for open-air sectors)
        for (j, v) in portal.vertices.iter().enumerate() {
            if !is_valid_portal_coord(v.x) || !is_valid_portal_coord(v.y) || !is_valid_portal_coord(v.z) {
                return Err(format!("{} portal[{}] vertex[{}]: invalid coordinates ({}, {}, {})",
                    context, i, j, v.x, v.y, v.z));
            }
        }
        // Validate portal normal
        if !is_valid_float(portal.normal.x) || !is_valid_float(portal.normal.y) || !is_valid_float(portal.normal.z) {
            return Err(format!("{} portal[{}]: invalid normal", context, i));
        }
    }

    // Validate ambient
    if !is_valid_float(room.ambient) {
        return Err(format!("{}: invalid ambient {}", context, room.ambient));
    }

    // Validate each sector
    for (x, col) in room.sectors.iter().enumerate() {
        for (z, sector_opt) in col.iter().enumerate() {
            if let Some(sector) = sector_opt {
                validate_sector(sector, &format!("{} sector[{},{}]", context, x, z))?;
            }
        }
    }

    Ok(())
}

/// Validate an entire level
pub fn validate_level(level: &Level) -> Result<(), LevelError> {
    // Check room count
    if level.rooms.len() > limits::MAX_ROOMS {
        return Err(LevelError::ValidationError(format!(
            "too many rooms ({} > {})", level.rooms.len(), limits::MAX_ROOMS
        )));
    }

    // Validate each room
    for (i, room) in level.rooms.iter().enumerate() {
        validate_room(room, i, level.rooms.len())
            .map_err(LevelError::ValidationError)?;
    }

    Ok(())
}

/// Load a level from a RON file (supports both compressed and uncompressed)
pub fn load_level<P: AsRef<Path>>(path: P) -> Result<Level, LevelError> {
    let path = path.as_ref();
    let bytes = fs::read(path)?;

    // Detect format: RON files start with '(' or whitespace, brotli is binary
    let is_plain_ron = bytes.first().map(|&b| b == b'(' || b == b' ' || b == b'\n' || b == b'\r' || b == b'\t').unwrap_or(false);

    let contents = if is_plain_ron {
        // Plain RON text
        String::from_utf8(bytes)
            .map_err(|e| LevelError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid UTF-8: {}", e)
            )))?
    } else {
        // Brotli compressed - decompress first
        let mut decompressed = Vec::new();
        brotli::BrotliDecompress(&mut Cursor::new(&bytes), &mut decompressed)
            .map_err(|e| LevelError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("brotli decompression failed: {}", e)
            )))?;
        String::from_utf8(decompressed)
            .map_err(|e| LevelError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid UTF-8 after decompression: {}", e)
            )))?
    };

    let mut level: Level = match ron::from_str(&contents) {
        Ok(l) => l,
        Err(e) => {
            // Log detailed error with context
            eprintln!("RON parse error in {}: {}", path.display(), e);
            let pos = e.position;
            // Show context around the error
            let lines: Vec<&str> = contents.lines().collect();
            let line_idx = pos.line.saturating_sub(1);
            if line_idx < lines.len() {
                let line = lines[line_idx];
                eprintln!("  Line {}: {}", pos.line, line);
                if pos.col > 0 && pos.col <= line.len() {
                    // Show surrounding characters
                    let start = pos.col.saturating_sub(20);
                    let end = (pos.col + 30).min(line.len());
                    eprintln!("  Context: ...{}...", &line[start..end]);
                }
            }
            return Err(e.into());
        }
    };

    // Validate level to prevent malicious files
    validate_level(&level)?;

    // Strip legacy objects (objects without asset_id) - migration to asset-based system
    for room in &mut level.rooms {
        room.objects.retain(|obj| obj.asset_id != 0);
    }

    // Recalculate bounds for all rooms (not serialized)
    for room in &mut level.rooms {
        room.recalculate_bounds();
    }

    Ok(level)
}

/// Save a level to a compressed RON file (brotli)
pub fn save_level<P: AsRef<Path>>(level: &Level, path: P) -> Result<(), LevelError> {
    let config = ron::ser::PrettyConfig::new()
        .depth_limit(4)
        .indentor("  ".to_string());

    let ron_string = ron::ser::to_string_pretty(level, config)?;

    // Compress with brotli (quality 6, window 22 - good balance of speed/ratio)
    let mut compressed = Vec::new();
    brotli::BrotliCompress(&mut Cursor::new(ron_string.as_bytes()), &mut compressed, &brotli::enc::BrotliEncoderParams {
        quality: 6,
        lgwin: 22,
        ..Default::default()
    }).map_err(|e| LevelError::IoError(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!("brotli compression failed: {}", e)
    )))?;

    fs::write(path, compressed)?;
    Ok(())
}

/// Load a level from a RON string (for embedded levels or testing)
pub fn load_level_from_str(s: &str) -> Result<Level, LevelError> {
    let mut level: Level = ron::from_str(s)?;

    // Validate level to prevent malicious files
    validate_level(&level)?;

    // Strip legacy objects (objects without asset_id) - migration to asset-based system
    for room in &mut level.rooms {
        room.objects.retain(|obj| obj.asset_id != 0);
    }

    for room in &mut level.rooms {
        room.recalculate_bounds();
    }

    Ok(level)
}

// ─────────────────────────────────────────────────────────────────────────────
// Storage-aware methods (use Storage abstraction for I/O)
// ─────────────────────────────────────────────────────────────────────────────

use crate::storage::Storage;

/// Load a level using the storage backend
pub fn load_level_with_storage(path: &str, storage: &Storage) -> Result<Level, LevelError> {
    let bytes = storage
        .read_sync(path)
        .map_err(|e| LevelError::IoError(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        )))?;

    // Detect format: RON files start with '(' or whitespace, brotli is binary
    let is_plain_ron = bytes.first().map(|&b| b == b'(' || b == b' ' || b == b'\n' || b == b'\r' || b == b'\t').unwrap_or(false);

    let contents = if is_plain_ron {
        // Plain RON text
        String::from_utf8(bytes)
            .map_err(|e| LevelError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid UTF-8: {}", e)
            )))?
    } else {
        // Brotli compressed - decompress first
        let mut decompressed = Vec::new();
        brotli::BrotliDecompress(&mut Cursor::new(&bytes), &mut decompressed)
            .map_err(|e| LevelError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("brotli decompression failed: {}", e)
            )))?;
        String::from_utf8(decompressed)
            .map_err(|e| LevelError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid UTF-8 after decompression: {}", e)
            )))?
    };

    let mut level: Level = ron::from_str(&contents)?;

    // Validate level to prevent malicious files
    validate_level(&level)?;

    // Strip legacy objects (objects without asset_id) - migration to asset-based system
    for room in &mut level.rooms {
        room.objects.retain(|obj| obj.asset_id != 0);
    }

    // Recalculate bounds for all rooms (not serialized)
    for room in &mut level.rooms {
        room.recalculate_bounds();
    }

    Ok(level)
}

/// Parse level data from bytes (for async loading)
pub fn parse_level_data(bytes: &[u8]) -> Result<Level, LevelError> {
    // Detect format: RON files start with '(' or whitespace, brotli is binary
    let is_plain_ron = bytes.first().map(|&b| b == b'(' || b == b' ' || b == b'\n' || b == b'\r' || b == b'\t').unwrap_or(false);

    let contents = if is_plain_ron {
        // Plain RON text
        String::from_utf8(bytes.to_vec())
            .map_err(|e| LevelError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid UTF-8: {}", e)
            )))?
    } else {
        // Brotli compressed - decompress first
        let mut decompressed = Vec::new();
        brotli::BrotliDecompress(&mut Cursor::new(bytes), &mut decompressed)
            .map_err(|e| LevelError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("brotli decompression failed: {}", e)
            )))?;
        String::from_utf8(decompressed)
            .map_err(|e| LevelError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid UTF-8 after decompression: {}", e)
            )))?
    };

    let mut level: Level = ron::from_str(&contents)?;

    // Validate level to prevent malicious files
    validate_level(&level)?;

    // Strip legacy objects (objects without asset_id) - migration to asset-based system
    for room in &mut level.rooms {
        room.objects.retain(|obj| obj.asset_id != 0);
    }

    // Recalculate bounds for all rooms (not serialized)
    for room in &mut level.rooms {
        room.recalculate_bounds();
    }

    Ok(level)
}

/// Save a level using the storage backend
pub fn save_level_with_storage(level: &Level, path: &str, storage: &Storage) -> Result<(), LevelError> {
    let data = serialize_level(level)?;
    storage
        .write_sync(path, &data)
        .map_err(|e| LevelError::IoError(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        )))
}

/// Serialize a level to compressed bytes (for async saving)
pub fn serialize_level(level: &Level) -> Result<Vec<u8>, LevelError> {
    let config = ron::ser::PrettyConfig::new()
        .depth_limit(4)
        .indentor("  ".to_string());

    let ron_string = ron::ser::to_string_pretty(level, config)?;

    // Compress with brotli (quality 6, window 22 - good balance of speed/ratio)
    let mut compressed = Vec::new();
    brotli::BrotliCompress(&mut Cursor::new(ron_string.as_bytes()), &mut compressed, &brotli::enc::BrotliEncoderParams {
        quality: 6,
        lgwin: 22,
        ..Default::default()
    }).map_err(|e| LevelError::IoError(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!("brotli compression failed: {}", e)
    )))?;

    Ok(compressed)
}
