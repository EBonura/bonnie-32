//! Tab bar widget - Fixed tabs for switching between tools
//!
//! Each tool (World Editor, Sound Designer, Tracker, etc.) has one fixed tab.
//! Tabs cannot be added or removed - they're always present.

use macroquad::prelude::*;
use super::{Rect, UiContext};

/// Visual style for tab bar
pub mod style {
    use macroquad::prelude::Color;

    /// Tab bar background
    pub const BAR_BG: Color = Color::new(0.12, 0.12, 0.14, 1.0);
    /// Active tab background
    pub const TAB_ACTIVE_BG: Color = Color::new(0.18, 0.18, 0.22, 1.0);
    /// Inactive tab background
    pub const TAB_INACTIVE_BG: Color = Color::new(0.14, 0.14, 0.16, 1.0);
    /// Hovered tab background
    pub const TAB_HOVER_BG: Color = Color::new(0.16, 0.16, 0.20, 1.0);
    /// Active tab text
    pub const TAB_ACTIVE_TEXT: Color = Color::new(1.0, 1.0, 1.0, 1.0);
    /// Inactive tab text
    pub const TAB_INACTIVE_TEXT: Color = Color::new(0.6, 0.6, 0.65, 1.0);
    /// Tab border/separator
    pub const TAB_BORDER: Color = Color::new(0.08, 0.08, 0.10, 1.0);
    /// Accent color for active tab indicator (cyan like MuseScore)
    pub const ACCENT: Color = Color::new(0.0, 0.75, 0.9, 1.0);
}

/// Layout constants
pub mod layout {
    /// Tab bar height
    pub const BAR_HEIGHT: f32 = 32.0;
    /// Tab horizontal padding
    pub const TAB_PADDING_H: f32 = 16.0;
    /// Active tab indicator height
    pub const INDICATOR_HEIGHT: f32 = 2.0;
    /// Font size for tab labels
    pub const FONT_SIZE: f32 = 14.0;
}

/// Draw a fixed tab bar with the given labels
/// Returns the index of the clicked tab, or None if no click
pub fn draw_fixed_tabs(
    ctx: &mut UiContext,
    rect: Rect,
    labels: &[&str],
    active_index: usize,
) -> Option<usize> {
    // Draw bar background
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, style::BAR_BG);

    // Bottom border
    draw_rectangle(
        rect.x,
        rect.y + rect.h - 1.0,
        rect.w,
        1.0,
        style::TAB_BORDER,
    );

    if labels.is_empty() {
        return None;
    }

    let mut clicked_tab = None;
    // Round starting x to integer for crisp rendering
    let mut x = rect.x.round();
    let y = rect.y.round();
    let h = rect.h.round();

    for (i, label) in labels.iter().enumerate() {
        // Measure text to size tab - round width to integer to prevent accumulation of fractional pixels
        let text_dims = measure_text(label, None, layout::FONT_SIZE as u16, 1.0);
        let tab_width = (text_dims.width + layout::TAB_PADDING_H * 2.0).round();

        let tab_rect = Rect::new(x, y, tab_width, h);
        let is_active = i == active_index;
        let is_hovered = ctx.mouse.inside(&tab_rect);

        // Determine background color
        let bg_color = if is_active {
            style::TAB_ACTIVE_BG
        } else if is_hovered {
            style::TAB_HOVER_BG
        } else {
            style::TAB_INACTIVE_BG
        };

        // Draw tab background
        draw_rectangle(tab_rect.x, tab_rect.y, tab_rect.w, tab_rect.h, bg_color);

        // Draw separator on right edge
        draw_rectangle(
            tab_rect.x + tab_rect.w - 1.0,
            tab_rect.y + 6.0,
            1.0,
            tab_rect.h - 12.0,
            style::TAB_BORDER,
        );

        // Draw active indicator at bottom
        if is_active {
            draw_rectangle(
                tab_rect.x,
                tab_rect.y + tab_rect.h - layout::INDICATOR_HEIGHT,
                tab_rect.w,
                layout::INDICATOR_HEIGHT,
                style::ACCENT,
            );
        }

        // Draw label centered - all values already rounded, just need to round the centering calc
        let text_color = if is_active {
            style::TAB_ACTIVE_TEXT
        } else {
            style::TAB_INACTIVE_TEXT
        };

        let text_x = (tab_rect.x + (tab_rect.w - text_dims.width) * 0.5).round();
        let text_y = (tab_rect.y + (tab_rect.h + text_dims.height) * 0.5 - 2.0).round();
        draw_text(label, text_x, text_y, layout::FONT_SIZE, text_color);

        // Handle click
        if ctx.mouse.clicked(&tab_rect) {
            clicked_tab = Some(i);
        }

        x += tab_width;
    }

    clicked_tab
}
