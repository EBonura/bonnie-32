//! Basic UI widgets

use macroquad::prelude::*;
use super::{Rect, UiContext};

/// Colors for widget states
pub struct WidgetColors {
    pub normal: Color,
    pub hover: Color,
    pub active: Color,
    pub text: Color,
}

impl Default for WidgetColors {
    fn default() -> Self {
        Self {
            normal: Color::from_rgba(60, 60, 70, 255),
            hover: Color::from_rgba(80, 80, 100, 255),
            active: Color::from_rgba(100, 120, 150, 255),
            text: WHITE,
        }
    }
}

/// Draw a button, returns true if clicked
pub fn button(ctx: &mut UiContext, rect: Rect, label: &str) -> bool {
    button_styled(ctx, rect, label, &WidgetColors::default())
}

/// Draw a button with custom colors
pub fn button_styled(ctx: &mut UiContext, rect: Rect, label: &str, colors: &WidgetColors) -> bool {
    let id = ctx.next_id();
    let hovered = ctx.mouse.inside(&rect);
    let pressed = ctx.mouse.clicking(&rect);
    let clicked = ctx.mouse.clicked(&rect);

    if hovered {
        ctx.set_hot(id);
    }

    // Determine color
    let bg_color = if pressed {
        colors.active
    } else if hovered {
        colors.hover
    } else {
        colors.normal
    };

    // Draw button
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, bg_color);
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, Color::from_rgba(100, 100, 100, 255));

    // Center text
    let text_size = 16.0;
    let text_width = measure_text(label, None, text_size as u16, 1.0).width;
    let text_x = rect.x + (rect.w - text_width) * 0.5;
    let text_y = rect.y + rect.h * 0.5 + text_size * 0.3;
    draw_text(label, text_x, text_y, text_size, colors.text);

    clicked
}

/// Draw a label
pub fn label(rect: Rect, text: &str) {
    label_colored(rect, text, WHITE);
}

/// Draw a label with custom color
pub fn label_colored(rect: Rect, text: &str, color: Color) {
    draw_text(text, rect.x, rect.y + 14.0, 16.0, color);
}

/// Draw a checkbox, returns new state if clicked
pub fn checkbox(ctx: &mut UiContext, rect: Rect, label: &str, checked: bool) -> bool {
    let id = ctx.next_id();
    let box_size = 16.0;
    let box_rect = Rect::new(rect.x, rect.y + (rect.h - box_size) * 0.5, box_size, box_size);

    let hovered = ctx.mouse.inside(&rect);
    let clicked = ctx.mouse.clicked(&rect);

    if hovered {
        ctx.set_hot(id);
    }

    // Draw checkbox
    let bg_color = if hovered {
        Color::from_rgba(80, 80, 100, 255)
    } else {
        Color::from_rgba(50, 50, 60, 255)
    };
    draw_rectangle(box_rect.x, box_rect.y, box_rect.w, box_rect.h, bg_color);
    draw_rectangle_lines(box_rect.x, box_rect.y, box_rect.w, box_rect.h, 1.0, Color::from_rgba(100, 100, 100, 255));

    // Draw check mark
    if checked {
        let pad = 3.0;
        draw_rectangle(
            box_rect.x + pad,
            box_rect.y + pad,
            box_rect.w - pad * 2.0,
            box_rect.h - pad * 2.0,
            Color::from_rgba(100, 200, 100, 255),
        );
    }

    // Draw label
    draw_text(label, rect.x + box_size + 6.0, rect.y + 14.0, 16.0, WHITE);

    // Return toggled state if clicked
    if clicked { !checked } else { checked }
}

/// Draw a horizontal slider, returns new value
pub fn slider(ctx: &mut UiContext, rect: Rect, value: f32, min: f32, max: f32) -> f32 {
    let id = ctx.next_id();

    // Track
    let track_height = 4.0;
    let track_y = rect.y + (rect.h - track_height) * 0.5;
    draw_rectangle(rect.x, track_y, rect.w, track_height, Color::from_rgba(40, 40, 50, 255));

    // Handle position
    let ratio = (value - min) / (max - min);
    let handle_width = 12.0;
    let handle_x = rect.x + ratio * (rect.w - handle_width);
    let handle_rect = Rect::new(handle_x, rect.y, handle_width, rect.h);

    // Handle interaction
    let hovered = ctx.mouse.inside(&handle_rect) || ctx.is_dragging(id);

    if ctx.mouse.inside(&handle_rect) {
        ctx.set_hot(id);
    }

    if ctx.is_hot(id) && ctx.mouse.left_pressed {
        ctx.start_drag(id);
    }

    let mut new_value = value;
    if ctx.is_dragging(id) {
        let new_ratio = ((ctx.mouse.x - rect.x - handle_width * 0.5) / (rect.w - handle_width)).clamp(0.0, 1.0);
        new_value = min + new_ratio * (max - min);
    }

    // Draw handle
    let handle_color = if ctx.is_dragging(id) {
        Color::from_rgba(120, 150, 200, 255)
    } else if hovered {
        Color::from_rgba(100, 120, 160, 255)
    } else {
        Color::from_rgba(80, 80, 100, 255)
    };
    draw_rectangle(handle_rect.x, handle_rect.y, handle_rect.w, handle_rect.h, handle_color);

    new_value
}

/// Simple toolbar layout helper
pub struct Toolbar {
    rect: Rect,
    cursor_x: f32,
    spacing: f32,
}

impl Toolbar {
    pub fn new(rect: Rect) -> Self {
        Self {
            rect,
            cursor_x: rect.x + 4.0,
            spacing: 4.0,
        }
    }

    /// Add a button to the toolbar
    pub fn button(&mut self, ctx: &mut UiContext, label: &str, width: f32) -> bool {
        let btn_rect = Rect::new(self.cursor_x, self.rect.y + 2.0, width, self.rect.h - 4.0);
        self.cursor_x += width + self.spacing;
        button(ctx, btn_rect, label)
    }

    /// Add a separator
    pub fn separator(&mut self) {
        self.cursor_x += self.spacing * 2.0;
        draw_line(
            self.cursor_x,
            self.rect.y + 4.0,
            self.cursor_x,
            self.rect.bottom() - 4.0,
            1.0,
            Color::from_rgba(80, 80, 80, 255),
        );
        self.cursor_x += self.spacing * 2.0;
    }

    /// Add a label
    pub fn label(&mut self, text: &str) {
        draw_text(text, self.cursor_x, self.rect.y + 16.0, 14.0, WHITE);
        self.cursor_x += measure_text(text, None, 14, 1.0).width + self.spacing;
    }

    /// Get current cursor X position
    pub fn cursor_x(&self) -> f32 {
        self.cursor_x
    }
}
