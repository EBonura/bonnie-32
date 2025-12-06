//! Basic UI widgets

use macroquad::prelude::*;
use super::{Rect, UiContext, draw_icon_centered};

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
        let font_size = 14.0;
        let text_dims = measure_text(text, None, font_size as u16, 1.0);
        // Center vertically in toolbar - round to integer pixels for crisp rendering
        let text_y = (self.rect.y + (self.rect.h + text_dims.height) * 0.5).round();
        draw_text(text, self.cursor_x.round(), text_y, font_size, WHITE);
        self.cursor_x += text_dims.width + self.spacing;
    }

    /// Add an icon button (square button with icon)
    pub fn icon_button(&mut self, ctx: &mut UiContext, icon: char, icon_font: Option<&Font>, tooltip: &str) -> bool {
        let size = (self.rect.h - 4.0).round();
        // Round positions to integer pixels for crisp rendering
        let btn_rect = Rect::new(self.cursor_x.round(), (self.rect.y + 2.0).round(), size, size);
        self.cursor_x += size + self.spacing;
        icon_button(ctx, btn_rect, icon, icon_font, tooltip)
    }

    /// Add an icon button with active state
    pub fn icon_button_active(&mut self, ctx: &mut UiContext, icon: char, icon_font: Option<&Font>, tooltip: &str, is_active: bool) -> bool {
        let size = (self.rect.h - 4.0).round();
        // Round positions to integer pixels for crisp rendering
        let btn_rect = Rect::new(self.cursor_x.round(), (self.rect.y + 2.0).round(), size, size);
        self.cursor_x += size + self.spacing;
        icon_button_active(ctx, btn_rect, icon, icon_font, tooltip, is_active)
    }
}

/// Accent color (cyan like MuseScore)
pub const ACCENT_COLOR: Color = Color::new(0.0, 0.75, 0.9, 1.0);

/// Draw an icon button, returns true if clicked (flat style, no background when inactive)
pub fn icon_button(ctx: &mut UiContext, rect: Rect, icon: char, icon_font: Option<&Font>, tooltip: &str) -> bool {
    draw_flat_icon_button(ctx, rect, icon, icon_font, tooltip, false)
}

/// Draw an icon button with active state highlighting (rounded cyan background when active)
pub fn icon_button_active(ctx: &mut UiContext, rect: Rect, icon: char, icon_font: Option<&Font>, tooltip: &str, is_active: bool) -> bool {
    draw_flat_icon_button(ctx, rect, icon, icon_font, tooltip, is_active)
}

/// Draw a flat icon button with optional active state (MuseScore style)
fn draw_flat_icon_button(ctx: &mut UiContext, rect: Rect, icon: char, icon_font: Option<&Font>, tooltip: &str, is_active: bool) -> bool {
    let id = ctx.next_id();
    let hovered = ctx.mouse.inside(&rect);
    let pressed = ctx.mouse.clicking(&rect);
    let clicked = ctx.mouse.clicked(&rect);

    if hovered {
        ctx.set_hot(id);
        if !tooltip.is_empty() {
            ctx.set_tooltip(tooltip, ctx.mouse.x, ctx.mouse.y);
        }
    }

    let corner_radius = 4.0;

    // Draw background only when active or hovered
    if is_active {
        // Cyan rounded rectangle for active state
        draw_rounded_rect(rect.x, rect.y, rect.w, rect.h, corner_radius, ACCENT_COLOR);
    } else if pressed {
        // Slight highlight when pressed
        draw_rounded_rect(rect.x, rect.y, rect.w, rect.h, corner_radius, Color::from_rgba(60, 60, 70, 255));
    } else if hovered {
        // Subtle hover effect
        draw_rounded_rect(rect.x, rect.y, rect.w, rect.h, corner_radius, Color::from_rgba(50, 50, 60, 255));
    }
    // No background when inactive and not hovered (flat)

    // Icon color: white when active, slightly dimmer when inactive
    let icon_color = if is_active {
        WHITE
    } else if hovered {
        Color::from_rgba(220, 220, 220, 255)
    } else {
        Color::from_rgba(180, 180, 180, 255)
    };

    // Draw icon centered
    let icon_size = (rect.h * 0.55).min(16.0);
    draw_icon_centered(icon_font, icon, &rect, icon_size, icon_color);

    clicked
}

/// Draw a rounded rectangle (simple approximation using overlapping rects)
fn draw_rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32, color: Color) {
    // Main body
    draw_rectangle(x + r, y, w - r * 2.0, h, color);
    draw_rectangle(x, y + r, w, h - r * 2.0, color);
    // Corners (circles)
    draw_circle(x + r, y + r, r, color);
    draw_circle(x + w - r, y + r, r, color);
    draw_circle(x + r, y + h - r, r, color);
    draw_circle(x + w - r, y + h - r, r, color);
}
