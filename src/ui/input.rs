//! Input state for UI interaction

use super::Rect;
use macroquad::prelude::*;

/// Mouse button state
#[derive(Debug, Clone, Copy, Default)]
pub struct MouseState {
    pub x: f32,
    pub y: f32,
    pub left_down: bool,
    pub right_down: bool,
    pub left_pressed: bool,  // Just pressed this frame
    pub left_released: bool, // Just released this frame
    pub right_pressed: bool, // Right button just pressed this frame
    pub scroll: f32,         // Scroll wheel delta
    pub double_clicked: bool, // Double-click detected this frame
}

/// Pending tooltip to be drawn at end of frame
#[derive(Clone)]
pub struct PendingTooltip {
    pub text: String,
    pub x: f32,
    pub y: f32,
}

impl MouseState {
    /// Check if mouse is inside a rect
    pub fn inside(&self, rect: &Rect) -> bool {
        rect.contains(self.x, self.y)
    }

    /// Check if mouse is clicking inside a rect
    pub fn clicking(&self, rect: &Rect) -> bool {
        self.left_down && rect.contains(self.x, self.y)
    }

    /// Check if mouse just clicked inside a rect
    pub fn clicked(&self, rect: &Rect) -> bool {
        self.left_pressed && rect.contains(self.x, self.y)
    }
}

/// UI context passed through the frame
pub struct UiContext {
    pub mouse: MouseState,
    /// ID of the widget currently being dragged (if any)
    pub dragging: Option<u64>,
    /// ID of the widget that is "hot" (mouse hovering)
    pub hot: Option<u64>,
    /// Counter for generating unique IDs
    id_counter: u64,
    /// Tooltip to show this frame (set by widgets, drawn at end)
    pub tooltip: Option<PendingTooltip>,
    /// Whether a modal dialog is active (blocks input to background)
    modal_active: bool,
}

impl UiContext {
    pub fn new() -> Self {
        Self {
            mouse: MouseState::default(),
            dragging: None,
            hot: None,
            id_counter: 0,
            tooltip: None,
            modal_active: false,
        }
    }

    /// Check if a modal is currently blocking input
    pub fn is_modal_active(&self) -> bool {
        self.modal_active
    }

    /// Begin a modal section - blocks input to everything drawn after this
    /// Saves the real mouse state and replaces it with a "dead" state
    pub fn begin_modal(&mut self) {
        if !self.modal_active {
            self.modal_active = true;
            // Block all mouse interactions
            self.mouse.left_down = false;
            self.mouse.right_down = false;
            self.mouse.left_pressed = false;
            self.mouse.left_released = false;
            self.mouse.right_pressed = false;
            self.mouse.scroll = 0.0;
        }
    }

    /// End modal section - restores mouse input for modal widgets
    /// Call this before drawing modal content so it can receive input
    pub fn end_modal(&mut self, real_mouse: MouseState) {
        self.modal_active = false;
        self.mouse = real_mouse;
    }

    /// Generate a unique ID for a widget
    pub fn next_id(&mut self) -> u64 {
        self.id_counter += 1;
        self.id_counter
    }

    /// Reset at start of frame (call before UI code)
    pub fn begin_frame(&mut self, mouse: MouseState) {
        self.mouse = mouse;
        self.hot = None;
        self.id_counter = 0;
        self.tooltip = None;
        self.modal_active = false;

        // Clear dragging if mouse released
        if !self.mouse.left_down {
            self.dragging = None;
        }
    }

    /// Set tooltip to show (call from widget when hovered)
    /// Ignored when a modal is active (background widgets shouldn't show tooltips)
    pub fn set_tooltip(&mut self, text: &str, x: f32, y: f32) {
        if self.modal_active {
            return;
        }
        self.tooltip = Some(PendingTooltip {
            text: text.to_string(),
            x,
            y,
        });
    }

    /// Draw the tooltip if one is pending (call at end of frame)
    pub fn draw_tooltip(&self) {
        if let Some(tip) = &self.tooltip {
            let padding = 6.0;
            let font_size = 13.0;
            let dims = measure_text(&tip.text, None, font_size as u16, 1.0);

            let box_w = dims.width + padding * 2.0;
            let box_h = dims.height + padding * 2.0;

            // Position below and to the right of cursor, but keep on screen
            let screen_w = screen_width();
            let screen_h = screen_height();
            let mut x = tip.x + 12.0;
            let mut y = tip.y + 20.0;

            if x + box_w > screen_w {
                x = screen_w - box_w - 4.0;
            }
            if y + box_h > screen_h {
                y = tip.y - box_h - 4.0;
            }

            // Draw background
            draw_rectangle(x, y, box_w, box_h, Color::from_rgba(30, 30, 35, 240));
            draw_rectangle_lines(x, y, box_w, box_h, 1.0, Color::from_rgba(80, 80, 90, 255));

            // Draw text
            draw_text(
                &tip.text,
                x + padding,
                y + padding + dims.height - 2.0,
                font_size,
                Color::from_rgba(220, 220, 220, 255),
            );
        }
    }

    /// Check if this widget is being dragged
    pub fn is_dragging(&self, id: u64) -> bool {
        self.dragging == Some(id)
    }

    /// Start dragging a widget
    pub fn start_drag(&mut self, id: u64) {
        self.dragging = Some(id);
    }

    /// Set hot widget (hovering)
    pub fn set_hot(&mut self, id: u64) {
        // Only set hot if not dragging something else
        if self.dragging.is_none() || self.dragging == Some(id) {
            self.hot = Some(id);
        }
    }

    /// Check if widget is hot
    pub fn is_hot(&self, id: u64) -> bool {
        self.hot == Some(id)
    }
}

impl Default for UiContext {
    fn default() -> Self {
        Self::new()
    }
}
