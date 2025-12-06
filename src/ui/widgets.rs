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

// =============================================================================
// Knob / Potentiometer Widget
// =============================================================================

/// Result from drawing a knob - contains the new value if changed
pub struct KnobResult {
    /// New value if the knob was adjusted
    pub value: Option<u8>,
    /// Whether the value box was clicked for text entry
    pub editing: bool,
}

/// Draw a rotary knob/potentiometer with value display
///
/// - `ctx`: UI context for input handling
/// - `center_x`, `center_y`: Center position of the knob
/// - `radius`: Radius of the knob
/// - `value`: Current value (0-127)
/// - `label`: Label to display above the knob
/// - `is_bipolar`: If true, center is at 64 (for pan)
/// - `is_editing`: If true, the value box is in text edit mode
///
/// Returns KnobResult with new value (if changed) and whether editing was triggered
pub fn draw_knob(
    ctx: &mut UiContext,
    center_x: f32,
    center_y: f32,
    radius: f32,
    value: u8,
    label: &str,
    is_bipolar: bool,
    is_editing: bool,
) -> KnobResult {
    use std::f32::consts::PI;

    let knob_rect = Rect::new(center_x - radius, center_y - radius, radius * 2.0, radius * 2.0);
    let hovered = ctx.mouse.inside(&knob_rect);

    // Colors
    let bg_color = Color::new(0.12, 0.12, 0.15, 1.0);
    let ring_color = Color::new(0.25, 0.25, 0.3, 1.0);
    let indicator_color = ACCENT_COLOR;
    let text_color = Color::new(0.8, 0.8, 0.8, 1.0);
    let label_color = Color::new(0.6, 0.6, 0.6, 1.0);

    // Draw knob body (outer ring)
    draw_circle(center_x, center_y, radius, ring_color);
    draw_circle(center_x, center_y, radius - 3.0, bg_color);

    // Knob rotation: map 0-127 to angle range
    // Start at 225° (bottom-left), end at -45° (bottom-right) = 270° sweep
    let start_angle = 225.0_f32.to_radians();
    let end_angle = -45.0_f32.to_radians();
    let angle_range = start_angle - end_angle; // 270 degrees

    let normalized = value as f32 / 127.0;
    let angle = start_angle - normalized * angle_range;

    // Draw arc showing value (using line segments)
    let arc_radius = radius - 1.5;
    let segments = 32;

    if is_bipolar {
        // For bipolar, draw from center (64) to current value
        let center_angle = start_angle - 0.5 * angle_range; // Middle = 64
        let (from_angle, to_angle) = if value < 64 {
            (angle, center_angle)
        } else {
            (center_angle, angle)
        };

        for i in 0..segments {
            let t1 = i as f32 / segments as f32;
            let t2 = (i + 1) as f32 / segments as f32;
            let a1 = from_angle + (to_angle - from_angle) * t1;
            let a2 = from_angle + (to_angle - from_angle) * t2;

            // Only draw segments in the arc range
            if a1 >= end_angle && a1 <= start_angle && a2 >= end_angle && a2 <= start_angle {
                let x1 = center_x + arc_radius * a1.cos();
                let y1 = center_y - arc_radius * a1.sin();
                let x2 = center_x + arc_radius * a2.cos();
                let y2 = center_y - arc_radius * a2.sin();
                draw_line(x1, y1, x2, y2, 3.0, indicator_color);
            }
        }
    } else {
        // Draw arc from start to current value
        for i in 0..segments {
            let t1 = i as f32 / segments as f32;
            let t2 = (i + 1) as f32 / segments as f32;
            let a1 = start_angle - t1 * normalized * angle_range;
            let a2 = start_angle - t2 * normalized * angle_range;

            let x1 = center_x + arc_radius * a1.cos();
            let y1 = center_y - arc_radius * a1.sin();
            let x2 = center_x + arc_radius * a2.cos();
            let y2 = center_y - arc_radius * a2.sin();
            draw_line(x1, y1, x2, y2, 3.0, indicator_color);
        }
    }

    // Draw indicator line (pointer)
    let inner_radius = radius * 0.35;
    let outer_radius = radius * 0.75;
    let pointer_x1 = center_x + inner_radius * angle.cos();
    let pointer_y1 = center_y - inner_radius * angle.sin();
    let pointer_x2 = center_x + outer_radius * angle.cos();
    let pointer_y2 = center_y - outer_radius * angle.sin();
    draw_line(pointer_x1, pointer_y1, pointer_x2, pointer_y2, 2.0, indicator_color);

    // Draw center dot
    draw_circle(center_x, center_y, 3.0, indicator_color);

    // Label above knob
    let label_dims = measure_text(label, None, 11, 1.0);
    draw_text(
        label,
        center_x - label_dims.width / 2.0,
        center_y - radius - 8.0,
        11.0,
        label_color,
    );

    // Value box below knob
    let box_width = 36.0;
    let box_height = 16.0;
    let box_x = center_x - box_width / 2.0;
    let box_y = center_y + radius + 6.0;
    let value_box = Rect::new(box_x, box_y, box_width, box_height);
    let box_hovered = ctx.mouse.inside(&value_box);

    // Value box background
    let box_bg = if is_editing {
        Color::new(0.2, 0.25, 0.3, 1.0)
    } else if box_hovered {
        Color::new(0.18, 0.18, 0.22, 1.0)
    } else {
        Color::new(0.14, 0.14, 0.17, 1.0)
    };
    draw_rectangle(box_x, box_y, box_width, box_height, box_bg);

    // Border when editing
    if is_editing {
        draw_rectangle_lines(box_x, box_y, box_width, box_height, 1.0, ACCENT_COLOR);
    }

    // Value text
    let value_str = format!("{:3}", value);
    let value_dims = measure_text(&value_str, None, 11, 1.0);
    draw_text(
        &value_str,
        center_x - value_dims.width / 2.0,
        box_y + box_height - 4.0,
        11.0,
        text_color,
    );

    // Handle knob interaction (drag to change value)
    let mut new_value = None;
    let mut start_editing = false;

    if hovered && is_mouse_button_down(MouseButton::Left) {
        // Calculate angle from mouse position to center
        let dx = ctx.mouse.x - center_x;
        let dy = center_y - ctx.mouse.y; // Flip Y for standard math coords
        let mouse_angle = dy.atan2(dx);

        // Map angle back to value (with wrapping protection)
        let mut norm = (start_angle - mouse_angle) / angle_range;
        norm = norm.clamp(0.0, 1.0);
        new_value = Some((norm * 127.0).round() as u8);
    }

    // Click on value box to start editing
    if box_hovered && is_mouse_button_pressed(MouseButton::Left) && !is_editing {
        start_editing = true;
    }

    KnobResult {
        value: new_value,
        editing: start_editing,
    }
}
