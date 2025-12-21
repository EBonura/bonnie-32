//! Application state and tool management
//!
//! Fixed set of tools, each with its own persistent state.
//! Switch between tools via the tab bar - all tools stay alive in background.

use crate::editor::{EditorState, EditorLayout, ExampleBrowser};
use crate::game::GameToolState;
use crate::landing::LandingState;
use crate::modeler::{ModelerState, ModelerLayout, ModelBrowser, MeshBrowser};
use crate::project::ProjectData;
use crate::tracker::TrackerState;
use crate::world::Level;
use macroquad::prelude::Font;
use std::path::PathBuf;

/// The available tools (fixed set, one tab each)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    Home = 0,
    WorldEditor = 1,
    Game = 2,
    Modeler = 3,
    Tracker = 4,
}

impl Tool {
    pub const ALL: [Tool; 5] = [
        Tool::Home,
        Tool::WorldEditor,
        Tool::Game,
        Tool::Modeler,
        Tool::Tracker,
    ];

    /// Get the display label for this tool
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            Tool::Home => "Home",
            Tool::WorldEditor => "World",
            Tool::Game => "Game",
            Tool::Modeler => "Assets",
            Tool::Tracker => "Music",
        }
    }

    /// Get all tool labels (for tab bar)
    #[allow(dead_code)]
    pub fn labels() -> [&'static str; 5] {
        [
            Tool::Home.label(),
            Tool::WorldEditor.label(),
            Tool::Game.label(),
            Tool::Modeler.label(),
            Tool::Tracker.label(),
        ]
    }

    pub fn from_index(i: usize) -> Option<Tool> {
        Tool::ALL.get(i).copied()
    }
}

/// State for the World Editor tool
pub struct WorldEditorState {
    pub editor_state: EditorState,
    pub editor_layout: EditorLayout,
    pub example_browser: ExampleBrowser,
}

/// State for the Modeler tool
pub struct ModelerToolState {
    pub modeler_state: ModelerState,
    pub modeler_layout: ModelerLayout,
    pub model_browser: ModelBrowser,
    pub mesh_browser: MeshBrowser,
}

/// Main application state containing all tool states
pub struct AppState {
    /// Currently active tool
    pub active_tool: Tool,

    /// Shared project data (single source of truth for all editors)
    /// This enables live editing: changes in any editor are immediately
    /// visible in all other views including the game preview.
    pub project: ProjectData,

    /// Landing page state
    pub landing: LandingState,

    /// World Editor state
    pub world_editor: WorldEditorState,

    /// Game preview state
    pub game: GameToolState,

    /// Modeler state
    pub modeler: ModelerToolState,

    /// Music Editor state
    pub tracker: TrackerState,

    /// Icon font (Lucide)
    pub icon_font: Option<Font>,
}

impl AppState {
    /// Create new app state with the given initial level for the world editor
    pub fn new(level: Level, file_path: Option<PathBuf>, icon_font: Option<Font>) -> Self {
        let editor_state = if let Some(path) = file_path {
            EditorState::with_file(level, path)
        } else {
            EditorState::new(level)
        };

        Self {
            active_tool: Tool::Home,
            project: ProjectData::new(),
            landing: LandingState::new(),
            world_editor: WorldEditorState {
                editor_state,
                editor_layout: EditorLayout::new(),
                example_browser: ExampleBrowser::default(),
            },
            game: GameToolState::new(),
            modeler: ModelerToolState {
                modeler_state: ModelerState::new(),
                modeler_layout: ModelerLayout::new(),
                model_browser: ModelBrowser::default(),
                mesh_browser: MeshBrowser::default(),
            },
            tracker: TrackerState::new(),
            icon_font,
        }
    }

    /// Switch to a different tool
    pub fn set_active_tool(&mut self, tool: Tool) {
        self.active_tool = tool;
    }

    /// Get the active tool index (for tab bar)
    pub fn active_tool_index(&self) -> usize {
        self.active_tool as usize
    }
}
