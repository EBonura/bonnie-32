//! Application state and tool management
//!
//! Fixed set of tools, each with its own persistent state.
//! Switch between tools via the tab bar - all tools stay alive in background.

use crate::editor::{EditorState, EditorLayout};
use crate::world::Level;
use macroquad::prelude::Font;
use std::path::PathBuf;

/// The available tools (fixed set, one tab each)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    WorldEditor = 0,
    SoundDesigner = 1,
    Tracker = 2,
    Game = 3,
}

impl Tool {
    pub const ALL: [Tool; 4] = [
        Tool::WorldEditor,
        Tool::SoundDesigner,
        Tool::Tracker,
        Tool::Game,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Tool::WorldEditor => "World Editor",
            Tool::SoundDesigner => "Sound Designer",
            Tool::Tracker => "Tracker",
            Tool::Game => "Game",
        }
    }

    pub fn labels() -> [&'static str; 4] {
        [
            Tool::WorldEditor.label(),
            Tool::SoundDesigner.label(),
            Tool::Tracker.label(),
            Tool::Game.label(),
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
}

/// State for Sound Designer (placeholder)
pub struct SoundDesignerState {
    // TODO: Add sound designer state
}

/// State for Tracker (placeholder)
pub struct TrackerState {
    // TODO: Add tracker state
}

/// State for Game preview (placeholder)
pub struct GameState {
    // TODO: Add game state (camera, etc.)
}

/// Main application state containing all tool states
pub struct AppState {
    /// Currently active tool
    pub active_tool: Tool,

    /// World Editor state
    pub world_editor: WorldEditorState,

    /// Sound Designer state
    pub sound_designer: SoundDesignerState,

    /// Tracker state
    pub tracker: TrackerState,

    /// Game state
    pub game: GameState,

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
            active_tool: Tool::WorldEditor,
            world_editor: WorldEditorState {
                editor_state,
                editor_layout: EditorLayout::new(),
            },
            sound_designer: SoundDesignerState {},
            tracker: TrackerState {},
            game: GameState {},
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
