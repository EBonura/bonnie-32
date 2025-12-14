//! Level loading and saving
//!
//! Uses RON (Rusty Object Notation) for human-readable level files.

use std::fs;
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
    /// Maximum portals per room
    pub const MAX_PORTALS: usize = 64;
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

    // Validate portals
    if room.portals.len() > limits::MAX_PORTALS {
        return Err(format!("{}: too many portals ({} > {})",
            context, room.portals.len(), limits::MAX_PORTALS));
    }
    for (i, portal) in room.portals.iter().enumerate() {
        if portal.target_room >= total_rooms {
            return Err(format!("{} portal[{}]: invalid target_room {} (only {} rooms)",
                context, i, portal.target_room, total_rooms));
        }
        // Validate portal vertices
        for (j, v) in portal.vertices.iter().enumerate() {
            if !is_valid_float(v.x) || !is_valid_float(v.y) || !is_valid_float(v.z) {
                return Err(format!("{} portal[{}] vertex[{}]: invalid coordinates", context, i, j));
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

/// Load a level from a RON file
pub fn load_level<P: AsRef<Path>>(path: P) -> Result<Level, LevelError> {
    let contents = fs::read_to_string(path)?;
    let mut level: Level = ron::from_str(&contents)?;

    // Validate level to prevent malicious files
    validate_level(&level)?;

    // Recalculate bounds for all rooms (not serialized)
    for room in &mut level.rooms {
        room.recalculate_bounds();
    }

    Ok(level)
}

/// Save a level to a RON file
pub fn save_level<P: AsRef<Path>>(level: &Level, path: P) -> Result<(), LevelError> {
    let config = ron::ser::PrettyConfig::new()
        .depth_limit(4)
        .indentor("  ".to_string());

    let contents = ron::ser::to_string_pretty(level, config)?;
    fs::write(path, contents)?;
    Ok(())
}

/// Load a level from a RON string (for embedded levels or testing)
pub fn load_level_from_str(s: &str) -> Result<Level, LevelError> {
    let mut level: Level = ron::from_str(s)?;

    // Validate level to prevent malicious files
    validate_level(&level)?;

    for room in &mut level.rooms {
        room.recalculate_bounds();
    }

    Ok(level)
}
