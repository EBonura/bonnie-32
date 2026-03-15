//! Level loading and saving
//!
//! Uses RON (Rusty Object Notation) for human-readable level files.

use std::fs;
use std::path::Path;
use super::geometry::*;

pub mod limits {
    pub const MAX_ROOMS: usize = 256;
    pub const MAX_ROOM_SIZE: usize = 128;
    pub const MAX_WALLS_PER_EDGE: usize = 16;
    pub const MAX_STRING_LEN: usize = 256;
    pub const MAX_COORD: f32 = 1_000_000.0;
}

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

fn is_valid_float(f: f32) -> bool {
    f.is_finite() && f.abs() <= limits::MAX_COORD
}

fn validate_texture_ref(tex: &TextureRef, context: &str) -> Result<(), String> {
    if tex.pack.len() > limits::MAX_STRING_LEN {
        return Err(format!("{}: texture pack name too long", context));
    }
    if tex.name.len() > limits::MAX_STRING_LEN {
        return Err(format!("{}: texture name too long", context));
    }
    Ok(())
}

fn validate_horizontal_face(face: &HorizontalFace, context: &str) -> Result<(), String> {
    for (i, h) in face.heights.iter().enumerate() {
        if !is_valid_float(*h) {
            return Err(format!("{}: invalid height[{}] = {}", context, i, h));
        }
    }
    validate_texture_ref(&face.texture, context)?;
    Ok(())
}

fn validate_vertical_face(face: &VerticalFace, context: &str) -> Result<(), String> {
    for (i, h) in face.heights.iter().enumerate() {
        if !is_valid_float(*h) {
            return Err(format!("{}: invalid height[{}] = {}", context, i, h));
        }
    }
    validate_texture_ref(&face.texture, context)?;
    Ok(())
}

fn validate_sector(sector: &Sector, context: &str) -> Result<(), String> {
    if let Some(floor) = &sector.floor {
        validate_horizontal_face(floor, &format!("{} floor", context))?;
    }
    if let Some(ceiling) = &sector.ceiling {
        validate_horizontal_face(ceiling, &format!("{} ceiling", context))?;
    }
    for dir_walls in [&sector.walls_north, &sector.walls_east, &sector.walls_south, &sector.walls_west] {
        if dir_walls.len() > limits::MAX_WALLS_PER_EDGE {
            return Err(format!("{}: too many walls on edge", context));
        }
        for (i, wall) in dir_walls.iter().enumerate() {
            validate_vertical_face(wall, &format!("{} wall[{}]", context, i))?;
        }
    }
    Ok(())
}

fn validate_room(room: &Room, room_idx: usize, total_rooms: usize) -> Result<(), String> {
    let context = format!("room[{}]", room_idx);
    if room.width > limits::MAX_ROOM_SIZE {
        return Err(format!("{}: width too large", context));
    }
    if room.depth > limits::MAX_ROOM_SIZE {
        return Err(format!("{}: depth too large", context));
    }
    if !is_valid_float(room.position.x) || !is_valid_float(room.position.y) || !is_valid_float(room.position.z) {
        return Err(format!("{}: invalid position", context));
    }
    if room.sectors.len() != room.width {
        return Err(format!("{}: sectors array width mismatch", context));
    }
    for (x, col) in room.sectors.iter().enumerate() {
        if col.len() != room.depth {
            return Err(format!("{}: sectors[{}] depth mismatch", context, x));
        }
    }
    for (i, portal) in room.portals.iter().enumerate() {
        if portal.target_room >= total_rooms {
            return Err(format!("{} portal[{}]: invalid target_room", context, i));
        }
    }
    for (x, col) in room.sectors.iter().enumerate() {
        for (z, sector_opt) in col.iter().enumerate() {
            if let Some(sector) = sector_opt {
                validate_sector(sector, &format!("{} sector[{},{}]", context, x, z))?;
            }
        }
    }
    Ok(())
}

pub fn validate_level(level: &Level) -> Result<(), LevelError> {
    if level.rooms.len() > limits::MAX_ROOMS {
        return Err(LevelError::ValidationError(format!(
            "too many rooms ({} > {})", level.rooms.len(), limits::MAX_ROOMS
        )));
    }
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
    validate_level(&level)?;

    // Strip legacy objects (objects without asset_id)
    for room in &mut level.rooms {
        room.objects.retain(|obj| obj.asset_id != 0);
    }

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
    let ron_string = ron::ser::to_string_pretty(level, config)?;
    fs::write(path, ron_string)?;
    Ok(())
}

/// Load a level from a RON string (for testing)
pub fn load_level_from_str(s: &str) -> Result<Level, LevelError> {
    let mut level: Level = ron::from_str(s)?;
    validate_level(&level)?;
    for room in &mut level.rooms {
        room.objects.retain(|obj| obj.asset_id != 0);
    }
    for room in &mut level.rooms {
        room.recalculate_bounds();
    }
    Ok(level)
}
