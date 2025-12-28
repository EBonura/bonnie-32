//! Tracker/Music Editor Action Definitions
//!
//! Defines all actions available in the music tracker, with their shortcuts,
//! icons, and enable conditions.

use macroquad::prelude::*;
use crate::ui::{Action, ActionRegistry, ActionContext, Shortcut, icon};

/// Custom flags for tracker-specific conditions
pub mod flags {
    /// Is currently playing
    pub const PLAYING: u32 = 1 << 0;
    /// Is in recording mode
    pub const RECORDING: u32 = 1 << 1;
    /// Has a pattern loaded
    pub const HAS_PATTERN: u32 = 1 << 2;
    /// Has a song loaded
    pub const HAS_SONG: u32 = 1 << 3;
    /// In note column
    pub const NOTE_COLUMN: u32 = 1 << 4;
    /// In instrument column
    pub const INSTRUMENT_COLUMN: u32 = 1 << 5;
    /// In effect column
    pub const EFFECT_COLUMN: u32 = 1 << 6;
    /// Editing a knob value
    pub const EDITING_KNOB: u32 = 1 << 7;
}

/// Create the complete action registry for the tracker
pub fn create_tracker_actions() -> ActionRegistry {
    let mut registry = ActionRegistry::new();

    // ========================================================================
    // Playback Actions
    // ========================================================================
    registry.register(
        Action::new("playback.toggle")
            .label("Play/Pause")
            .shortcut(Shortcut::key(KeyCode::Space))
            .icon(icon::PLAY)
            .status_tip("Start or pause playback")
            .category("Playback"),
    );

    registry.register(
        Action::new("playback.stop")
            .label("Stop")
            .shortcut(Shortcut::key(KeyCode::Escape))
            .icon(icon::SQUARE)
            .status_tip("Stop playback and return to start")
            .category("Playback"),
    );

    registry.register(
        Action::new("playback.rewind")
            .label("Rewind")
            .icon(icon::SKIP_BACK)
            .status_tip("Return to beginning of pattern")
            .category("Playback"),
    );

    // ========================================================================
    // Navigation Actions
    // ========================================================================
    registry.register(
        Action::new("nav.up")
            .label("Move Up")
            .shortcut(Shortcut::key(KeyCode::Up))
            .status_tip("Move cursor up one row")
            .category("Navigation"),
    );

    registry.register(
        Action::new("nav.down")
            .label("Move Down")
            .shortcut(Shortcut::key(KeyCode::Down))
            .status_tip("Move cursor down one row")
            .category("Navigation"),
    );

    registry.register(
        Action::new("nav.left")
            .label("Move Left")
            .shortcut(Shortcut::key(KeyCode::Left))
            .status_tip("Move cursor left one column")
            .category("Navigation"),
    );

    registry.register(
        Action::new("nav.right")
            .label("Move Right")
            .shortcut(Shortcut::key(KeyCode::Right))
            .status_tip("Move cursor right one column")
            .category("Navigation"),
    );

    registry.register(
        Action::new("nav.next_channel")
            .label("Next Channel")
            .shortcut(Shortcut::key(KeyCode::Tab))
            .status_tip("Move to next channel")
            .category("Navigation"),
    );

    registry.register(
        Action::new("nav.prev_channel")
            .label("Previous Channel")
            .shortcut(Shortcut::shift(KeyCode::Tab))
            .status_tip("Move to previous channel")
            .category("Navigation"),
    );

    registry.register(
        Action::new("nav.page_up")
            .label("Page Up")
            .shortcut(Shortcut::key(KeyCode::PageUp))
            .status_tip("Move up 16 rows")
            .category("Navigation"),
    );

    registry.register(
        Action::new("nav.page_down")
            .label("Page Down")
            .shortcut(Shortcut::key(KeyCode::PageDown))
            .status_tip("Move down 16 rows")
            .category("Navigation"),
    );

    registry.register(
        Action::new("nav.home")
            .label("Go to Start")
            .shortcut(Shortcut::key(KeyCode::Home))
            .status_tip("Go to beginning of pattern")
            .category("Navigation"),
    );

    registry.register(
        Action::new("nav.end")
            .label("Go to End")
            .shortcut(Shortcut::key(KeyCode::End))
            .status_tip("Go to end of pattern")
            .category("Navigation"),
    );

    // ========================================================================
    // Octave Actions
    // ========================================================================
    registry.register(
        Action::new("octave.up")
            .label("Octave Up")
            .shortcut(Shortcut::key(KeyCode::KpAdd))
            .status_tip("Increase octave")
            .category("Octave"),
    );

    registry.register(
        Action::new("octave.down")
            .label("Octave Down")
            .shortcut(Shortcut::key(KeyCode::KpSubtract))
            .status_tip("Decrease octave")
            .category("Octave"),
    );

    // ========================================================================
    // Edit Step Actions
    // ========================================================================
    registry.register(
        Action::new("edit_step.decrease")
            .label("Decrease Edit Step")
            .shortcut(Shortcut::key(KeyCode::F9))
            .status_tip("Decrease edit step size")
            .category("Edit Step"),
    );

    registry.register(
        Action::new("edit_step.increase")
            .label("Increase Edit Step")
            .shortcut(Shortcut::key(KeyCode::F10))
            .status_tip("Increase edit step size")
            .category("Edit Step"),
    );

    // ========================================================================
    // Note Entry Actions
    // ========================================================================
    registry.register(
        Action::new("note.delete")
            .label("Delete Note")
            .shortcut(Shortcut::key(KeyCode::Delete))
            .status_tip("Delete note at cursor")
            .category("Note Entry")
            .enabled_when(|ctx| ctx.has_flag(flags::NOTE_COLUMN)),
    );

    registry.register(
        Action::new("note.off")
            .label("Note Off")
            .shortcut(Shortcut::key(KeyCode::Apostrophe))
            .status_tip("Enter note-off command")
            .category("Note Entry")
            .enabled_when(|ctx| ctx.has_flag(flags::NOTE_COLUMN)),
    );

    // ========================================================================
    // Pattern Actions
    // ========================================================================
    registry.register(
        Action::new("pattern.new")
            .label("New Pattern")
            .status_tip("Create a new pattern")
            .category("Pattern"),
    );

    registry.register(
        Action::new("pattern.duplicate")
            .label("Duplicate Pattern")
            .status_tip("Duplicate current pattern")
            .category("Pattern")
            .enabled_when(|ctx| ctx.has_flag(flags::HAS_PATTERN)),
    );

    registry.register(
        Action::new("pattern.clear")
            .label("Clear Pattern")
            .status_tip("Clear all notes in current pattern")
            .category("Pattern")
            .enabled_when(|ctx| ctx.has_flag(flags::HAS_PATTERN)),
    );

    // ========================================================================
    // Instrument/Sound Actions
    // ========================================================================
    registry.register(
        Action::new("instrument.prev")
            .label("Previous Instrument")
            .status_tip("Select previous instrument")
            .category("Instrument"),
    );

    registry.register(
        Action::new("instrument.next")
            .label("Next Instrument")
            .status_tip("Select next instrument")
            .category("Instrument"),
    );

    registry
}

/// Build an ActionContext from the current tracker state
pub fn build_context(
    is_playing: bool,
    has_pattern: bool,
    column_type: &str, // "note", "instrument", "effect"
    editing_knob: bool,
) -> ActionContext {
    let mut flags = 0u32;

    if is_playing {
        flags |= flags::PLAYING;
    }
    if has_pattern {
        flags |= flags::HAS_PATTERN;
    }
    if editing_knob {
        flags |= flags::EDITING_KNOB;
    }

    match column_type {
        "note" => flags |= flags::NOTE_COLUMN,
        "instrument" => flags |= flags::INSTRUMENT_COLUMN,
        "effect" => flags |= flags::EFFECT_COLUMN,
        _ => {}
    }

    ActionContext {
        can_undo: false, // Tracker doesn't have undo yet
        can_redo: false,
        has_selection: false, // No block selection yet
        has_clipboard: false,
        mode: "tracker",
        text_editing: editing_knob, // Block shortcuts when editing knob values
        has_face_selection: false,
        has_vertex_selection: false,
        is_dirty: false,
        flags,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracker_actions_registered() {
        let registry = create_tracker_actions();

        assert!(registry.get("playback.toggle").is_some());
        assert!(registry.get("nav.up").is_some());
        assert!(registry.get("note.delete").is_some());
        assert!(registry.get("pattern.new").is_some());
    }

    #[test]
    fn test_note_column_conditions() {
        let registry = create_tracker_actions();

        // Note delete requires being in note column
        let ctx = build_context(false, true, "effect", false);
        assert!(!registry.is_enabled("note.delete", &ctx));

        let ctx2 = build_context(false, true, "note", false);
        assert!(registry.is_enabled("note.delete", &ctx2));
    }

    #[test]
    fn test_knob_editing_blocks_shortcuts() {
        let registry = create_tracker_actions();

        // When editing a knob, shortcuts should be blocked
        let ctx = build_context(false, true, "note", true);
        // text_editing = true should disable actions
        assert!(!registry.is_enabled("note.delete", &ctx));
    }
}
