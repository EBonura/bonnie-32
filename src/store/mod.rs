//! DocumentStore — single source of truth for all open documents.
//!
//! Documents are keyed by their absolute path on disk.
//! Mutations bump a version counter so tabs can detect stale cached renders.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::project::Project;
use crate::scene::{load_level, save_level, Level};

// ---------------------------------------------------------------------------
// Projects root
// ---------------------------------------------------------------------------

/// The canonical root for all projects: `{current_dir}/projects/`.
/// Created on first call if it doesn't exist yet.
pub fn projects_root() -> PathBuf {
    let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let root = base.join("projects");
    std::fs::create_dir_all(&root).ok();
    root
}

// ---------------------------------------------------------------------------
// RecentProject
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RecentProject {
    pub name: String,
    pub path: PathBuf,
}

// ---------------------------------------------------------------------------
// LoadedLevel
// ---------------------------------------------------------------------------

struct LoadedLevel {
    data: Level,
    /// Bumped on every mutation. Tabs cache their last-seen version.
    version: u64,
    dirty: bool,
}

// ---------------------------------------------------------------------------
// DocumentStore
// ---------------------------------------------------------------------------

pub struct DocumentStore {
    pub project: Option<Project>,
    /// Keyed by absolute path to the .ron file.
    levels: HashMap<PathBuf, LoadedLevel>,
    pub recent_projects: Vec<RecentProject>,
}

impl DocumentStore {
    pub fn new() -> Self {
        Self {
            project: None,
            levels: HashMap::new(),
            recent_projects: Self::load_recent(),
        }
    }

    // ---- Project management ------------------------------------------------

    pub fn open_project(&mut self, manifest_path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
        let project = Project::open(manifest_path.clone())?;
        let name = project.name().to_string();
        self.levels.clear();
        self.project = Some(project);
        self.add_recent(name, manifest_path);
        Ok(())
    }

    pub fn create_project(&mut self, root: PathBuf, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let project = Project::create(root, name)?;
        let path = project.manifest_path().to_path_buf();
        let display_name = project.name().to_string();
        self.levels.clear();
        self.project = Some(project);
        self.add_recent(display_name, path);
        Ok(())
    }

    pub fn close_project(&mut self) {
        self.project = None;
        self.levels.clear();
    }

    pub fn project_name(&self) -> Option<&str> {
        self.project.as_ref().map(|p| p.name())
    }

    pub fn has_project(&self) -> bool {
        self.project.is_some()
    }

    // ---- Level CRUD --------------------------------------------------------

    /// Create a new empty level in the project's levels/ directory.
    /// Returns the absolute path of the new file.
    pub fn create_level(&mut self, name: &str) -> Option<PathBuf> {
        let project = self.project.as_ref()?;
        let levels_dir = project.root().join("levels");
        std::fs::create_dir_all(&levels_dir).ok()?;

        // Use name as filename, sanitised
        let filename = sanitise_filename(name);
        let path = levels_dir.join(format!("{}.ron", filename));

        let mut level = Level::new();
        level.name = name.to_string();

        if let Err(e) = save_level(&level, &path) {
            log::error!("Failed to write new level {:?}: {}", path, e);
            return None;
        }

        self.levels.insert(path.clone(), LoadedLevel {
            data: level,
            version: 0,
            dirty: false,
        });

        Some(path)
    }

    /// Load a level from disk. If already loaded, returns immediately.
    pub fn open_level(&mut self, path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
        if self.levels.contains_key(&path) { return Ok(()); }
        let level = load_level(&path)?;
        self.levels.insert(path, LoadedLevel { data: level, version: 0, dirty: false });
        Ok(())
    }

    /// Unload a level from memory. Does not delete from disk.
    pub fn close_level(&mut self, path: &Path) {
        self.levels.remove(path);
    }

    /// Save a loaded level to disk.
    pub fn save_level(&mut self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let data = {
            let loaded = self.levels.get(path).ok_or("Level not loaded")?;
            loaded.data.clone()
        };
        save_level(&data, path)?;
        if let Some(loaded) = self.levels.get_mut(path) {
            loaded.dirty = false;
        }
        Ok(())
    }

    // ---- Level accessors ---------------------------------------------------

    pub fn get_level(&self, path: &Path) -> Option<&Level> {
        self.levels.get(path).map(|l| &l.data)
    }

    /// Mutable access. Bumps version and marks dirty.
    pub fn mutate_level(&mut self, path: &Path) -> Option<&mut Level> {
        let loaded = self.levels.get_mut(path)?;
        loaded.version += 1;
        loaded.dirty = true;
        Some(&mut loaded.data)
    }

    pub fn level_version(&self, path: &Path) -> u64 {
        self.levels.get(path).map(|l| l.version).unwrap_or(0)
    }

    pub fn level_dirty(&self, path: &Path) -> bool {
        self.levels.get(path).map(|l| l.dirty).unwrap_or(false)
    }

    pub fn is_level_loaded(&self, path: &Path) -> bool {
        self.levels.contains_key(path)
    }

    // ---- Recent projects ---------------------------------------------------

    fn recent_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".bonnie32").join("recent.ron")
    }

    fn load_recent() -> Vec<RecentProject> {
        let path = Self::recent_path();
        let Ok(text) = std::fs::read_to_string(&path) else { return Vec::new() };
        ron::from_str(&text).unwrap_or_default()
    }

    fn save_recent(&self) {
        let path = Self::recent_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        if let Ok(text) = ron::ser::to_string_pretty(
            &self.recent_projects,
            ron::ser::PrettyConfig::default(),
        ) {
            std::fs::write(path, text).ok();
        }
    }

    fn add_recent(&mut self, name: String, path: PathBuf) {
        self.recent_projects.retain(|r| r.path != path);
        self.recent_projects.insert(0, RecentProject { name, path });
        self.recent_projects.truncate(10);
        self.save_recent();
    }
}

impl Default for DocumentStore {
    fn default() -> Self { Self::new() }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sanitise_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}
