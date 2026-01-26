//! Text input widget with proper cursor and keyboard handling

use macroquad::prelude::*;
use super::Rect;

/// State for a text input field
#[derive(Debug, Clone)]
pub struct TextInputState {
    /// The text content
    pub text: String,
    /// Cursor position (byte index)
    pub cursor: usize,
    /// Selection start (byte index), if selecting
    pub selection_start: Option<usize>,
    /// Blink timer for cursor
    pub blink_timer: f32,
    /// Whether the input has focus
    pub focused: bool,
}

impl TextInputState {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let cursor = text.len();
        Self {
            text,
            cursor,
            selection_start: None,
            blink_timer: 0.0,
            focused: true,
        }
    }

    /// Get selected text range (start, end) in sorted order
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        self.selection_start.map(|start| {
            if start < self.cursor {
                (start, self.cursor)
            } else {
                (self.cursor, start)
            }
        })
    }

    /// Delete selected text and return cursor to selection start
    pub fn delete_selection(&mut self) {
        if let Some((start, end)) = self.selection_range() {
            self.text.drain(start..end);
            self.cursor = start;
            self.selection_start = None;
        }
    }

    /// Check if there's a selection
    pub fn has_selection(&self) -> bool {
        self.selection_start.is_some() && self.selection_start != Some(self.cursor)
    }

    /// Move cursor left, optionally extending selection
    pub fn move_left(&mut self, extend_selection: bool) {
        if extend_selection {
            if self.selection_start.is_none() {
                self.selection_start = Some(self.cursor);
            }
        } else {
            // If we have a selection, move to the start of it
            if let Some((start, _)) = self.selection_range() {
                self.cursor = start;
                self.selection_start = None;
                return;
            }
        }

        if self.cursor > 0 {
            // Move back one character (handle UTF-8)
            let prev = self.text[..self.cursor]
                .char_indices()
                .rev()
                .next()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.cursor = prev;
        }

        if !extend_selection {
            self.selection_start = None;
        }
    }

    /// Move cursor right, optionally extending selection
    pub fn move_right(&mut self, extend_selection: bool) {
        if extend_selection {
            if self.selection_start.is_none() {
                self.selection_start = Some(self.cursor);
            }
        } else {
            // If we have a selection, move to the end of it
            if let Some((_, end)) = self.selection_range() {
                self.cursor = end;
                self.selection_start = None;
                return;
            }
        }

        if self.cursor < self.text.len() {
            // Move forward one character (handle UTF-8)
            let next = self.text[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(self.text.len());
            self.cursor = next;
        }

        if !extend_selection {
            self.selection_start = None;
        }
    }

    /// Move cursor to start
    pub fn move_home(&mut self, extend_selection: bool) {
        if extend_selection && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        }
        self.cursor = 0;
        if !extend_selection {
            self.selection_start = None;
        }
    }

    /// Move cursor to end
    pub fn move_end(&mut self, extend_selection: bool) {
        if extend_selection && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        }
        self.cursor = self.text.len();
        if !extend_selection {
            self.selection_start = None;
        }
    }

    /// Select all text
    pub fn select_all(&mut self) {
        self.selection_start = Some(0);
        self.cursor = self.text.len();
    }

    /// Insert text at cursor, replacing selection if any
    pub fn insert(&mut self, s: &str) {
        if self.has_selection() {
            self.delete_selection();
        }
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    /// Insert a character at cursor
    pub fn insert_char(&mut self, ch: char) {
        if self.has_selection() {
            self.delete_selection();
        }
        self.text.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    /// Delete character before cursor (backspace)
    pub fn backspace(&mut self) {
        if self.has_selection() {
            self.delete_selection();
            return;
        }
        if self.cursor > 0 {
            // Find previous character boundary
            let prev = self.text[..self.cursor]
                .char_indices()
                .rev()
                .next()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.text.drain(prev..self.cursor);
            self.cursor = prev;
        }
    }

    /// Delete character after cursor (delete key)
    pub fn delete(&mut self) {
        if self.has_selection() {
            self.delete_selection();
            return;
        }
        if self.cursor < self.text.len() {
            // Find next character boundary
            let next = self.text[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(self.text.len());
            self.text.drain(self.cursor..next);
        }
    }

    /// Handle keyboard input, returns true if text changed
    pub fn handle_input(&mut self) -> bool {
        let old_text = self.text.clone();
        self.blink_timer += get_frame_time();

        let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
        let ctrl = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl)
            || is_key_down(KeyCode::LeftSuper) || is_key_down(KeyCode::RightSuper);

        // Navigation
        if is_key_pressed(KeyCode::Left) {
            self.move_left(shift);
            self.blink_timer = 0.0;
        }
        if is_key_pressed(KeyCode::Right) {
            self.move_right(shift);
            self.blink_timer = 0.0;
        }
        if is_key_pressed(KeyCode::Home) {
            self.move_home(shift);
            self.blink_timer = 0.0;
        }
        if is_key_pressed(KeyCode::End) {
            self.move_end(shift);
            self.blink_timer = 0.0;
        }

        // Select all
        if ctrl && is_key_pressed(KeyCode::A) {
            self.select_all();
            self.blink_timer = 0.0;
        }

        // Deletion
        if is_key_pressed(KeyCode::Backspace) {
            self.backspace();
            self.blink_timer = 0.0;
        }
        if is_key_pressed(KeyCode::Delete) {
            self.delete();
            self.blink_timer = 0.0;
        }

        // Character input
        while let Some(ch) = get_char_pressed() {
            // Filter control characters
            if ch >= ' ' && ch != '\u{7f}' {
                self.insert_char(ch);
                self.blink_timer = 0.0;
            }
        }

        self.text != old_text
    }
}

/// Colors for text input
const INPUT_BG: Color = Color::new(0.12, 0.12, 0.14, 1.0);
const INPUT_BORDER: Color = Color::new(0.0, 0.75, 0.9, 1.0);
const INPUT_TEXT: Color = Color::new(0.8, 0.8, 0.85, 1.0);
const INPUT_SELECTION: Color = Color::new(0.0, 0.5, 0.7, 0.5);
const INPUT_CURSOR: Color = Color::new(0.9, 0.9, 0.95, 1.0);

/// Draw a text input field and handle input
/// Returns true if the text changed
pub fn draw_text_input(rect: Rect, state: &mut TextInputState, font_size: f32) -> bool {
    // Draw background
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, INPUT_BG);
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, INPUT_BORDER);

    let padding = 8.0;
    let text_x = rect.x + padding;
    let text_y = rect.y + (rect.h + font_size * 0.7) / 2.0;

    // Handle input
    let changed = state.handle_input();

    // Measure text up to cursor for cursor positioning
    let text_before_cursor = &state.text[..state.cursor];
    let cursor_offset = measure_text(text_before_cursor, None, font_size as u16, 1.0).width;

    // Draw selection highlight
    if let Some((start, end)) = state.selection_range() {
        let start_text = &state.text[..start];
        let selected_text = &state.text[start..end];
        let start_x = text_x + measure_text(start_text, None, font_size as u16, 1.0).width;
        let sel_width = measure_text(selected_text, None, font_size as u16, 1.0).width;
        draw_rectangle(start_x, rect.y + 4.0, sel_width, rect.h - 8.0, INPUT_SELECTION);
    }

    // Draw text
    draw_text(&state.text, text_x, text_y, font_size, INPUT_TEXT);

    // Draw cursor (blinking)
    if state.focused && (state.blink_timer % 1.0) < 0.5 {
        let cursor_x = text_x + cursor_offset;
        draw_line(cursor_x, rect.y + 6.0, cursor_x, rect.y + rect.h - 6.0, 1.5, INPUT_CURSOR);
    }

    changed
}
