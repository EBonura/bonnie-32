//! Centralized Action Registry
//!
//! Provides a unified system for managing keyboard shortcuts, toolbar buttons,
//! and menu items with:
//! - Dynamic enable/disable conditions
//! - User-rebindable shortcuts
//! - Icon support for toolbar buttons
//! - Status tips for tooltips
//!
//! # Example
//! ```ignore
//! let mut registry = ActionRegistry::new();
//!
//! registry.register(Action::new("edit.undo")
//!     .label("Undo")
//!     .shortcut(Shortcut::ctrl(KeyCode::Z))
//!     .icon(icon::UNDO)
//!     .status_tip("Undo last action")
//!     .enabled_when(|ctx| ctx.can_undo));
//!
//! // In your update loop:
//! if registry.triggered("edit.undo", &ctx) {
//!     state.undo();
//! }
//! ```

use macroquad::prelude::*;
use std::collections::HashMap;

/// A keyboard shortcut (key + modifiers)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Shortcut {
    pub key: KeyCode,
    pub ctrl: bool,   // Cmd on Mac
    pub shift: bool,
    pub alt: bool,
}

impl Shortcut {
    /// Create a shortcut with just a key (no modifiers)
    pub fn key(key: KeyCode) -> Self {
        Self {
            key,
            ctrl: false,
            shift: false,
            alt: false,
        }
    }

    /// Create a shortcut with Ctrl/Cmd + key
    pub fn ctrl(key: KeyCode) -> Self {
        Self {
            key,
            ctrl: true,
            shift: false,
            alt: false,
        }
    }

    /// Create a shortcut with Ctrl/Cmd + Shift + key
    pub fn ctrl_shift(key: KeyCode) -> Self {
        Self {
            key,
            ctrl: true,
            shift: true,
            alt: false,
        }
    }

    /// Create a shortcut with Shift + key
    pub fn shift(key: KeyCode) -> Self {
        Self {
            key,
            ctrl: false,
            shift: true,
            alt: false,
        }
    }

    /// Create a shortcut with Alt + key
    pub fn alt(key: KeyCode) -> Self {
        Self {
            key,
            ctrl: false,
            shift: false,
            alt: true,
        }
    }

    /// Check if this shortcut is currently pressed
    pub fn is_pressed(&self) -> bool {
        if !is_key_pressed(self.key) {
            return false;
        }

        let ctrl_down = is_key_down(KeyCode::LeftControl)
            || is_key_down(KeyCode::RightControl)
            || is_key_down(KeyCode::LeftSuper)
            || is_key_down(KeyCode::RightSuper);
        let shift_down = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
        let alt_down = is_key_down(KeyCode::LeftAlt) || is_key_down(KeyCode::RightAlt);

        self.ctrl == ctrl_down && self.shift == shift_down && self.alt == alt_down
    }

    /// Format shortcut for display (e.g., "Ctrl+Z", "⌘Z")
    pub fn display(&self) -> String {
        let mut parts = Vec::new();

        #[cfg(target_os = "macos")]
        {
            if self.ctrl {
                parts.push("⌘");
            }
            if self.shift {
                parts.push("⇧");
            }
            if self.alt {
                parts.push("⌥");
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            if self.ctrl {
                parts.push("Ctrl+");
            }
            if self.shift {
                parts.push("Shift+");
            }
            if self.alt {
                parts.push("Alt+");
            }
        }

        parts.push(key_name(self.key));
        parts.join("")
    }
}

/// Get a human-readable name for a key
fn key_name(key: KeyCode) -> &'static str {
    match key {
        KeyCode::A => "A",
        KeyCode::B => "B",
        KeyCode::C => "C",
        KeyCode::D => "D",
        KeyCode::E => "E",
        KeyCode::F => "F",
        KeyCode::G => "G",
        KeyCode::H => "H",
        KeyCode::I => "I",
        KeyCode::J => "J",
        KeyCode::K => "K",
        KeyCode::L => "L",
        KeyCode::M => "M",
        KeyCode::N => "N",
        KeyCode::O => "O",
        KeyCode::P => "P",
        KeyCode::Q => "Q",
        KeyCode::R => "R",
        KeyCode::S => "S",
        KeyCode::T => "T",
        KeyCode::U => "U",
        KeyCode::V => "V",
        KeyCode::W => "W",
        KeyCode::X => "X",
        KeyCode::Y => "Y",
        KeyCode::Z => "Z",
        KeyCode::Key0 => "0",
        KeyCode::Key1 => "1",
        KeyCode::Key2 => "2",
        KeyCode::Key3 => "3",
        KeyCode::Key4 => "4",
        KeyCode::Key5 => "5",
        KeyCode::Key6 => "6",
        KeyCode::Key7 => "7",
        KeyCode::Key8 => "8",
        KeyCode::Key9 => "9",
        KeyCode::Escape => "Esc",
        KeyCode::Enter => "Enter",
        KeyCode::Space => "Space",
        KeyCode::Tab => "Tab",
        KeyCode::Backspace => "Backspace",
        KeyCode::Delete => "Del",
        KeyCode::Up => "↑",
        KeyCode::Down => "↓",
        KeyCode::Left => "←",
        KeyCode::Right => "→",
        KeyCode::F1 => "F1",
        KeyCode::F2 => "F2",
        KeyCode::F3 => "F3",
        KeyCode::F4 => "F4",
        KeyCode::F5 => "F5",
        KeyCode::F6 => "F6",
        KeyCode::F7 => "F7",
        KeyCode::F8 => "F8",
        KeyCode::F9 => "F9",
        KeyCode::F10 => "F10",
        KeyCode::F11 => "F11",
        KeyCode::F12 => "F12",
        KeyCode::Home => "Home",
        KeyCode::End => "End",
        KeyCode::PageUp => "PgUp",
        KeyCode::PageDown => "PgDn",
        KeyCode::Minus => "-",
        KeyCode::Equal => "=",
        KeyCode::LeftBracket => "[",
        KeyCode::RightBracket => "]",
        KeyCode::Backslash => "\\",
        KeyCode::Semicolon => ";",
        KeyCode::Apostrophe => "'",
        KeyCode::Comma => ",",
        KeyCode::Period => ".",
        KeyCode::Slash => "/",
        KeyCode::GraveAccent => "`",
        _ => "?",
    }
}

/// Context for checking action enable/disable conditions
#[derive(Debug, Clone, Default)]
pub struct ActionContext {
    /// Can undo (has undo history)
    pub can_undo: bool,
    /// Can redo (has redo history)
    pub can_redo: bool,
    /// Has active selection
    pub has_selection: bool,
    /// Has clipboard content
    pub has_clipboard: bool,
    /// Current editor mode (for mode-specific actions)
    pub mode: &'static str,
    /// Is in text edit mode (should block shortcuts)
    pub text_editing: bool,
    /// Has faces selected (for extrude, etc.)
    pub has_face_selection: bool,
    /// Has vertices selected
    pub has_vertex_selection: bool,
    /// Is dirty (has unsaved changes)
    pub is_dirty: bool,
    /// Custom flags for app-specific conditions
    pub flags: u32,
}

impl ActionContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a custom flag
    pub fn with_flag(mut self, flag: u32) -> Self {
        self.flags |= flag;
        self
    }

    /// Check if a custom flag is set
    pub fn has_flag(&self, flag: u32) -> bool {
        (self.flags & flag) != 0
    }
}

/// Type alias for enable condition functions
type EnableFn = fn(&ActionContext) -> bool;

/// Type alias for checked state functions (for toggle actions)
type CheckedFn = fn(&ActionContext) -> bool;

/// Always enabled
fn always_enabled(_: &ActionContext) -> bool {
    true
}

/// A registered action
#[derive(Clone)]
pub struct Action {
    /// Unique identifier (e.g., "file.save", "edit.undo")
    pub id: &'static str,
    /// Human-readable label
    pub label: &'static str,
    /// Default keyboard shortcut (can be overridden by user)
    pub default_shortcut: Option<Shortcut>,
    /// Current keyboard shortcut (may differ from default if user customized)
    pub shortcut: Option<Shortcut>,
    /// Icon character (from icon font)
    pub icon: Option<char>,
    /// Status bar tip / tooltip
    pub status_tip: &'static str,
    /// Function to check if action is enabled
    enabled_fn: EnableFn,
    /// Function to check if action is "checked" (for toggles)
    checked_fn: Option<CheckedFn>,
    /// Category for grouping in menus/settings
    pub category: &'static str,
}

impl Action {
    /// Create a new action with the given ID
    pub fn new(id: &'static str) -> Self {
        Self {
            id,
            label: "",
            default_shortcut: None,
            shortcut: None,
            icon: None,
            status_tip: "",
            enabled_fn: always_enabled,
            checked_fn: None,
            category: "General",
        }
    }

    /// Set the display label
    pub fn label(mut self, label: &'static str) -> Self {
        self.label = label;
        self
    }

    /// Set the keyboard shortcut
    pub fn shortcut(mut self, shortcut: Shortcut) -> Self {
        self.default_shortcut = Some(shortcut.clone());
        self.shortcut = Some(shortcut);
        self
    }

    /// Set the icon character
    pub fn icon(mut self, icon: char) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Set the status tip / tooltip
    pub fn status_tip(mut self, tip: &'static str) -> Self {
        self.status_tip = tip;
        self
    }

    /// Set the category
    pub fn category(mut self, category: &'static str) -> Self {
        self.category = category;
        self
    }

    /// Set the enable condition
    pub fn enabled_when(mut self, f: EnableFn) -> Self {
        self.enabled_fn = f;
        self
    }

    /// Set the checked condition (for toggle actions)
    pub fn checked_when(mut self, f: CheckedFn) -> Self {
        self.checked_fn = Some(f);
        self
    }

    /// Check if this action is enabled in the given context
    pub fn is_enabled(&self, ctx: &ActionContext) -> bool {
        // Block all shortcuts when text editing
        if ctx.text_editing {
            return false;
        }
        (self.enabled_fn)(ctx)
    }

    /// Check if this action is checked (for toggle actions)
    pub fn is_checked(&self, ctx: &ActionContext) -> bool {
        self.checked_fn.map_or(false, |f| f(ctx))
    }

    /// Check if this action is a toggle (has a checked state)
    pub fn is_toggle(&self) -> bool {
        self.checked_fn.is_some()
    }

    /// Check if this action's shortcut is pressed and action is enabled
    pub fn is_triggered(&self, ctx: &ActionContext) -> bool {
        if !self.is_enabled(ctx) {
            return false;
        }
        self.shortcut.as_ref().map_or(false, |s| s.is_pressed())
    }

    /// Get tooltip with shortcut hint
    pub fn tooltip(&self) -> String {
        if let Some(ref shortcut) = self.shortcut {
            if self.status_tip.is_empty() {
                format!("{} ({})", self.label, shortcut.display())
            } else {
                format!("{} ({})", self.status_tip, shortcut.display())
            }
        } else if !self.status_tip.is_empty() {
            self.status_tip.to_string()
        } else {
            self.label.to_string()
        }
    }
}

/// Central registry for all actions
pub struct ActionRegistry {
    actions: HashMap<&'static str, Action>,
    /// Map from shortcut to action ID (for conflict detection)
    shortcut_map: HashMap<Shortcut, &'static str>,
}

impl ActionRegistry {
    pub fn new() -> Self {
        Self {
            actions: HashMap::new(),
            shortcut_map: HashMap::new(),
        }
    }

    /// Register an action
    pub fn register(&mut self, action: Action) {
        // Update shortcut map
        if let Some(ref shortcut) = action.shortcut {
            self.shortcut_map.insert(shortcut.clone(), action.id);
        }
        self.actions.insert(action.id, action);
    }

    /// Get an action by ID
    pub fn get(&self, id: &str) -> Option<&Action> {
        self.actions.get(id)
    }

    /// Get a mutable action by ID
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Action> {
        self.actions.get_mut(id)
    }

    /// Check if an action is triggered (shortcut pressed and enabled)
    pub fn triggered(&self, id: &str, ctx: &ActionContext) -> bool {
        self.actions.get(id).map_or(false, |a| a.is_triggered(ctx))
    }

    /// Check if an action is enabled
    pub fn is_enabled(&self, id: &str, ctx: &ActionContext) -> bool {
        self.actions.get(id).map_or(false, |a| a.is_enabled(ctx))
    }

    /// Check if an action is checked (for toggles)
    pub fn is_checked(&self, id: &str, ctx: &ActionContext) -> bool {
        self.actions.get(id).map_or(false, |a| a.is_checked(ctx))
    }

    /// Get tooltip for an action
    pub fn tooltip(&self, id: &str) -> String {
        self.actions.get(id).map_or_else(String::new, |a| a.tooltip())
    }

    /// Rebind a shortcut for an action
    pub fn rebind(&mut self, id: &str, new_shortcut: Option<Shortcut>) -> Result<(), &'static str> {
        // First, get the static ID from the action (so we can store it in the map)
        let static_id = match self.actions.get(id) {
            Some(action) => action.id,
            None => return Err("Action not found"),
        };

        // Check for conflicts
        if let Some(ref shortcut) = new_shortcut {
            if let Some(&existing_id) = self.shortcut_map.get(shortcut) {
                if existing_id != static_id {
                    return Err("Shortcut already in use");
                }
            }
        }

        // Remove old shortcut from map
        if let Some(action) = self.actions.get(id) {
            if let Some(ref old_shortcut) = action.shortcut {
                self.shortcut_map.remove(old_shortcut);
            }
        }

        // Update action and shortcut map
        if let Some(action) = self.actions.get_mut(id) {
            action.shortcut = new_shortcut.clone();
            if let Some(shortcut) = new_shortcut {
                self.shortcut_map.insert(shortcut, static_id);
            }
            Ok(())
        } else {
            Err("Action not found")
        }
    }

    /// Reset an action's shortcut to default
    pub fn reset_shortcut(&mut self, id: &str) {
        if let Some(action) = self.actions.get(id) {
            let default = action.default_shortcut.clone();
            let _ = self.rebind(id, default);
        }
    }

    /// Get all actions in a category
    pub fn actions_in_category(&self, category: &str) -> Vec<&Action> {
        self.actions
            .values()
            .filter(|a| a.category == category)
            .collect()
    }

    /// Get all categories
    pub fn categories(&self) -> Vec<&'static str> {
        let mut cats: Vec<_> = self.actions.values().map(|a| a.category).collect();
        cats.sort();
        cats.dedup();
        cats
    }

    /// Find actions that match a search query (searches label and id)
    pub fn search(&self, query: &str) -> Vec<&Action> {
        let query = query.to_lowercase();
        self.actions
            .values()
            .filter(|a| {
                a.label.to_lowercase().contains(&query)
                    || a.id.to_lowercase().contains(&query)
                    || a.status_tip.to_lowercase().contains(&query)
            })
            .collect()
    }

    /// Process all triggered actions this frame, returning their IDs
    pub fn process_triggers(&self, ctx: &ActionContext) -> Vec<&'static str> {
        self.actions
            .values()
            .filter(|a| a.is_triggered(ctx))
            .map(|a| a.id)
            .collect()
    }
}

impl Default for ActionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Standard Actions (can be used as templates)
// ============================================================================

/// Create common file actions
pub fn file_actions() -> Vec<Action> {
    vec![
        Action::new("file.new")
            .label("New")
            .shortcut(Shortcut::ctrl(KeyCode::N))
            .status_tip("Create a new file")
            .category("File"),
        Action::new("file.open")
            .label("Open")
            .shortcut(Shortcut::ctrl(KeyCode::O))
            .status_tip("Open an existing file")
            .category("File"),
        Action::new("file.save")
            .label("Save")
            .shortcut(Shortcut::ctrl(KeyCode::S))
            .status_tip("Save the current file")
            .category("File"),
        Action::new("file.save_as")
            .label("Save As...")
            .shortcut(Shortcut::ctrl_shift(KeyCode::S))
            .status_tip("Save to a new file")
            .category("File"),
    ]
}

/// Create common edit actions
pub fn edit_actions() -> Vec<Action> {
    vec![
        Action::new("edit.undo")
            .label("Undo")
            .shortcut(Shortcut::ctrl(KeyCode::Z))
            .status_tip("Undo last action")
            .category("Edit")
            .enabled_when(|ctx| ctx.can_undo),
        Action::new("edit.redo")
            .label("Redo")
            .shortcut(Shortcut::ctrl_shift(KeyCode::Z))
            .status_tip("Redo last undone action")
            .category("Edit")
            .enabled_when(|ctx| ctx.can_redo),
        Action::new("edit.cut")
            .label("Cut")
            .shortcut(Shortcut::ctrl(KeyCode::X))
            .status_tip("Cut selection to clipboard")
            .category("Edit")
            .enabled_when(|ctx| ctx.has_selection),
        Action::new("edit.copy")
            .label("Copy")
            .shortcut(Shortcut::ctrl(KeyCode::C))
            .status_tip("Copy selection to clipboard")
            .category("Edit")
            .enabled_when(|ctx| ctx.has_selection),
        Action::new("edit.paste")
            .label("Paste")
            .shortcut(Shortcut::ctrl(KeyCode::V))
            .status_tip("Paste from clipboard")
            .category("Edit")
            .enabled_when(|ctx| ctx.has_clipboard),
        Action::new("edit.delete")
            .label("Delete")
            .shortcut(Shortcut::key(KeyCode::Delete))
            .status_tip("Delete selection")
            .category("Edit")
            .enabled_when(|ctx| ctx.has_selection),
        Action::new("edit.select_all")
            .label("Select All")
            .shortcut(Shortcut::ctrl(KeyCode::A))
            .status_tip("Select all items")
            .category("Edit"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shortcut_display() {
        let s = Shortcut::ctrl(KeyCode::Z);
        // Platform-specific, but should contain Z
        assert!(s.display().contains("Z"));

        let s2 = Shortcut::ctrl_shift(KeyCode::S);
        assert!(s2.display().contains("S"));
    }

    #[test]
    fn test_action_builder() {
        let action = Action::new("test.action")
            .label("Test")
            .shortcut(Shortcut::ctrl(KeyCode::T))
            .status_tip("A test action")
            .category("Test");

        assert_eq!(action.id, "test.action");
        assert_eq!(action.label, "Test");
        assert!(action.shortcut.is_some());
        assert_eq!(action.category, "Test");
    }

    #[test]
    fn test_registry() {
        let mut registry = ActionRegistry::new();

        registry.register(
            Action::new("edit.undo")
                .label("Undo")
                .shortcut(Shortcut::ctrl(KeyCode::Z))
                .enabled_when(|ctx| ctx.can_undo),
        );

        let ctx = ActionContext {
            can_undo: false,
            ..Default::default()
        };
        assert!(!registry.is_enabled("edit.undo", &ctx));

        let ctx2 = ActionContext {
            can_undo: true,
            ..Default::default()
        };
        assert!(registry.is_enabled("edit.undo", &ctx2));
    }

    #[test]
    fn test_rebind() {
        let mut registry = ActionRegistry::new();

        registry.register(
            Action::new("edit.undo")
                .shortcut(Shortcut::ctrl(KeyCode::Z)),
        );

        // Rebind to Ctrl+U
        let result = registry.rebind("edit.undo", Some(Shortcut::ctrl(KeyCode::U)));
        assert!(result.is_ok());

        let action = registry.get("edit.undo").unwrap();
        assert_eq!(action.shortcut.as_ref().unwrap().key, KeyCode::U);
    }
}
