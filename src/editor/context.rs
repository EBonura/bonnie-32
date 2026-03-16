use crate::asset::AssetHandle;
use crate::project::Project;
use crate::scene::Level;
use super::undo::UndoStack;

/// Shared editor state, accessible by all panels.
/// Follows Hazel's pattern: EditorLayer owns this, panels borrow it.
pub struct EditorContext {
    pub project: Option<Project>,
    pub selection: Selection,
    pub mode: EditorMode,
    pub pending_action: Option<EditorAction>,
    pub current_level: Option<Level>,
    /// Undo/redo for level geometry edits
    pub level_undo: UndoStack<Level>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Selection {
    None,
    Asset(AssetHandle),
    Room(usize),
    Entity { room: usize, index: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorMode {
    Project,
    WorldEditor,
    Modeler,
    Tracker,
    ScriptEditor,
    Test,
}

#[derive(Debug, Clone)]
pub enum EditorAction {
    NewProject,
    OpenProject(std::path::PathBuf),
    SaveProject,
    ImportAsset(std::path::PathBuf),
    OpenAsset(AssetHandle),
    SwitchMode(EditorMode),
    // Level actions
    NewLevel,
    SaveLevel,
    // Song actions
    NewSong,
    OpenSong(std::path::PathBuf),
    SaveSong,
    SaveSongAs(std::path::PathBuf),
    // Undo/redo
    Undo,
    Redo,
}

impl EditorContext {
    pub fn new() -> Self {
        Self {
            project: None,
            selection: Selection::None,
            mode: EditorMode::Project,
            pending_action: None,
            current_level: None,
            level_undo: UndoStack::new(),
        }
    }

    /// Call before mutating the level to record a snapshot for undo.
    pub fn push_level_undo(&mut self) {
        if let Some(level) = &self.current_level {
            self.level_undo.push(level.clone());
        }
    }

    pub fn has_project(&self) -> bool {
        self.project.is_some()
    }

    pub fn project_name(&self) -> &str {
        self.project
            .as_ref()
            .map(|p| p.name())
            .unwrap_or("No Project")
    }

    pub fn select(&mut self, selection: Selection) {
        self.selection = selection;
    }

    pub fn clear_selection(&mut self) {
        self.selection = Selection::None;
    }

    pub fn request_action(&mut self, action: EditorAction) {
        self.pending_action = Some(action);
    }

    pub fn take_action(&mut self) -> Option<EditorAction> {
        self.pending_action.take()
    }
}

impl Default for EditorContext {
    fn default() -> Self {
        Self::new()
    }
}
