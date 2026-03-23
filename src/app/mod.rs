//! AppState — top-level application state.
//!
//! Owns the DocumentStore (data) and pending tab operations.
//! Tab layout is managed by egui_dock::DockState in Shell.

use std::path::{Path, PathBuf};
use crate::store::DocumentStore;

// ---------------------------------------------------------------------------
// Tab
// ---------------------------------------------------------------------------

/// One open editor tab. Each variant carries the absolute path to the document
/// it edits (except About, ContentBrowser, and ProjectComposer, which are singletons).
#[derive(Debug, Clone, PartialEq)]
pub enum Tab {
    About,
    ContentBrowser,
    ProjectComposer,
    LevelEditor   { path: PathBuf, last_seen_version: u64 },
    AssetEditor   { path: PathBuf },
    MusicTracker  { path: PathBuf },
    ScriptEditor  { path: PathBuf },
}

impl Tab {
    /// Human-readable title shown in the tab bar.
    pub fn title(&self, store: &DocumentStore) -> String {
        match self {
            Tab::About          => "Home".to_string(),
            Tab::ContentBrowser => "Browser".to_string(),
            Tab::ProjectComposer => {
                store.project_name()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "Project".to_string())
            }
            Tab::LevelEditor { path, .. } => {
                let name = stem(path);
                if store.level_dirty(path) { format!("{}*", name) } else { name }
            }
            Tab::AssetEditor  { path } => stem(path),
            Tab::MusicTracker { path } => stem(path),
            Tab::ScriptEditor { path } => stem(path),
        }
    }

    pub fn can_close(&self) -> bool {
        !matches!(self, Tab::About | Tab::ContentBrowser)
    }

    /// Two tabs are the same document if they have the same (variant, path).
    pub fn is_same(&self, other: &Tab) -> bool {
        self == other
    }
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("?")
        .to_string()
}

// ---------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Selection {
    None,
    Asset(PathBuf),
    Room(usize),
    Entity { room: usize, index: usize },
}

// ---------------------------------------------------------------------------
// EditorAction
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum EditorAction {
    NewProject,
    OpenProject(PathBuf),
    SaveProject,
    ImportAsset(PathBuf),
    OpenAsset(PathBuf),
    // Level actions
    NewLevel,
    AddRoom,
    SaveLevel,
    // Song actions
    NewSong,
    OpenSong(PathBuf),
    SaveSong,
    SaveSongAs(PathBuf),
    // Undo/redo
    Undo,
    Redo,
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

pub struct AppState {
    pub store: DocumentStore,
    pub selection: Selection,
    pub pending_action: Option<EditorAction>,
    /// Tabs waiting to be opened in the DockState (processed by Shell::draw).
    pub pending_tabs: Vec<Tab>,
    /// When true, Shell closes all closeable tabs (e.g. when a project is closed).
    pub close_all_editors: bool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            store: DocumentStore::new(),
            selection: Selection::None,
            pending_action: None,
            pending_tabs: Vec::new(),
            close_all_editors: false,
        }
    }

    /// Request opening a tab. Shell will focus it if already open, otherwise push it.
    pub fn open_tab(&mut self, tab: Tab) {
        self.pending_tabs.push(tab);
    }

    pub fn has_project(&self) -> bool {
        self.store.has_project()
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

impl Default for AppState {
    fn default() -> Self { Self::new() }
}
