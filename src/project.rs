//! Project Data
//!
//! Single source of truth for all game assets.
//! All editor tools reference ProjectData rather than owning copies.
//! This enables live editing: changes in any editor are immediately
//! visible in all other views including the game preview.

// Allow unused - project structure for future use
#![allow(dead_code)]

use crate::world::Level;
use crate::modeler::{RiggedModel, EditableMesh};
use crate::tracker::Song;

/// Central container for all project data.
///
/// This is the shared state that all editor tools reference.
/// The World Editor, Modeler, Tracker, and Game tool all read/write
/// to this same data, enabling seamless live editing.
pub struct ProjectData {
    /// The level being edited (rooms, sectors, geometry)
    pub level: Level,

    /// Rigged character/object models (skeleton + mesh parts)
    pub models: Vec<RiggedModel>,

    /// Standalone editable meshes (not yet rigged)
    pub meshes: Vec<EditableMesh>,

    /// Music tracks
    pub songs: Vec<Song>,
}

impl ProjectData {
    /// Create a new empty project
    pub fn new() -> Self {
        Self {
            level: Level::new(),
            models: Vec::new(),
            meshes: Vec::new(),
            songs: Vec::new(),
        }
    }

    /// Create a project with a starter level
    pub fn with_starter_level() -> Self {
        Self {
            level: crate::world::create_empty_level(),
            models: Vec::new(),
            meshes: Vec::new(),
            songs: Vec::new(),
        }
    }
}

impl Default for ProjectData {
    fn default() -> Self {
        Self::new()
    }
}
