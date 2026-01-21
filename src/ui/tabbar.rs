//! Tab bar widget - Fixed tabs for switching between tools
//!
//! Each tool (World Editor, Sound Designer, Tracker, etc.) has one fixed tab.
//! Tabs cannot be added or removed - they're always present.

use macroquad::prelude::*;
use super::{Rect, UiContext};
use crate::storage::StorageMode;

/// Actions returned by the tab bar
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabBarAction {
    /// No action
    None,
    /// Switch to a different tab
    SwitchTab(usize),
    /// User clicked Sign In
    SignIn,
    /// User clicked Sign Out
    SignOut,
}

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
    /// Icon size
    pub const ICON_SIZE: f32 = 14.0;
    /// Spacing between icon and label
    pub const ICON_LABEL_GAP: f32 = 6.0;
}

/// A tab entry with icon and label
pub struct TabEntry {
    pub icon: char,
    pub label: &'static str,
}

impl TabEntry {
    pub const fn new(icon: char, label: &'static str) -> Self {
        Self { icon, label }
    }
}

/// Draw a fixed tab bar with icons and labels
/// Returns the index of the clicked tab, or None if no click
pub fn draw_fixed_tabs(
    ctx: &mut UiContext,
    rect: Rect,
    tabs: &[TabEntry],
    active_index: usize,
    icon_font: Option<&Font>,
) -> Option<usize> {
    let mut dummy = false;
    draw_fixed_tabs_with_version(ctx, rect, tabs, active_index, icon_font, None, &mut dummy)
}

/// Draw a fixed tab bar with icons, labels, and optional version string
/// Returns the index of the clicked tab, or None if no click
/// The version_highlighted parameter toggles color when version is clicked (easter egg!)
pub fn draw_fixed_tabs_with_version(
    ctx: &mut UiContext,
    rect: Rect,
    tabs: &[TabEntry],
    active_index: usize,
    icon_font: Option<&Font>,
    version: Option<&str>,
    version_highlighted: &mut bool,
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

    // Draw version at far right if provided
    if let Some(ver) = version {
        let version_text = format!("v{}", ver);
        let font_size = 18.0;
        let text_dims = measure_text(&version_text, None, font_size as u16, 1.0);
        let padding_right = 16.0;
        let text_x = rect.x + rect.w - text_dims.width - padding_right;
        let text_y = rect.y + (rect.h + text_dims.height) * 0.5 - 1.0;

        // Create clickable rect for the version
        let version_rect = Rect::new(text_x - 4.0, rect.y, text_dims.width + 8.0, rect.h);

        // Toggle highlight on click (easter egg!)
        if ctx.mouse.clicked(&version_rect) {
            *version_highlighted = !*version_highlighted;
        }

        if *version_highlighted {
            // Knight Rider scanner effect!
            let time = get_time() as f32;
            let char_count = version_text.chars().count() as f32;

            // Scanner position oscillates back and forth (0 to char_count-1)
            let speed = 3.0; // cycles per second
            let phase = (time * speed) % 2.0; // 0-2 range for ping-pong
            let scanner_pos = if phase < 1.0 {
                phase * (char_count - 1.0) // forward
            } else {
                (2.0 - phase) * (char_count - 1.0) // backward
            };

            // Draw each character with glow based on distance from scanner
            let mut char_x = text_x;
            for (i, ch) in version_text.chars().enumerate() {
                let char_str = ch.to_string();
                let char_dims = measure_text(&char_str, None, font_size as u16, 1.0);

                // Distance from scanner (0 = at scanner, higher = further)
                let distance = (i as f32 - scanner_pos).abs();

                // Glow intensity: bright at scanner, fades with distance
                let glow = (1.0 - distance / 2.0).max(0.0).powf(0.5);

                // Interpolate between gray and cyan based on glow
                let gray = Color::new(0.4, 0.4, 0.45, 1.0);
                let r = gray.r + (style::ACCENT.r - gray.r) * glow;
                let g = gray.g + (style::ACCENT.g - gray.g) * glow;
                let b = gray.b + (style::ACCENT.b - gray.b) * glow;
                let char_color = Color::new(r, g, b, 1.0);

                draw_text_ex(
                    &char_str,
                    char_x.round(),
                    text_y.round(),
                    TextParams {
                        font: None,
                        font_size: font_size as u16,
                        color: char_color,
                        ..Default::default()
                    },
                );
                char_x += char_dims.width;
            }
        } else {
            // Static gray text
            draw_text_ex(
                &version_text,
                text_x.round(),
                text_y.round(),
                TextParams {
                    font: None,
                    font_size: font_size as u16,
                    color: Color::new(0.5, 0.5, 0.55, 1.0),
                    ..Default::default()
                },
            );
        }
    }

    if tabs.is_empty() {
        return None;
    }

    let mut clicked_tab = None;
    // Round starting x to integer for crisp rendering
    let mut x = rect.x.round();
    let y = rect.y.round();
    let h = rect.h.round();

    for (i, tab) in tabs.iter().enumerate() {
        // Measure text to size tab - round width to integer to prevent accumulation of fractional pixels
        let text_dims = measure_text(tab.label, None, layout::FONT_SIZE as u16, 1.0);
        // Tab width: padding + icon + gap + text + padding
        let content_width = layout::ICON_SIZE + layout::ICON_LABEL_GAP + text_dims.width;
        let tab_width = (content_width + layout::TAB_PADDING_H * 2.0).round();

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

        // Colors for icon and text
        let content_color = if is_active {
            style::TAB_ACTIVE_TEXT
        } else {
            style::TAB_INACTIVE_TEXT
        };

        // Calculate vertical center of tab
        let center_y = tab_rect.y + tab_rect.h * 0.5;

        // Content starts at padding from left edge
        let content_start_x = tab_rect.x + layout::TAB_PADDING_H;

        // Draw icon centered vertically (icon draws from baseline, so offset by half icon size)
        let icon_x = content_start_x;
        let icon_y = (center_y + layout::ICON_SIZE * 0.5).round();
        draw_text_ex(
            &tab.icon.to_string(),
            icon_x.round(),
            icon_y,
            TextParams {
                font: icon_font,
                font_size: layout::ICON_SIZE as u16,
                color: content_color,
                ..Default::default()
            },
        );

        // Draw label after icon, also centered vertically
        let text_x = (content_start_x + layout::ICON_SIZE + layout::ICON_LABEL_GAP).round();
        let text_y = (center_y + text_dims.height * 0.5 - 1.0).round();
        draw_text_ex(
            tab.label,
            text_x,
            text_y,
            TextParams {
                font: None,
                font_size: layout::FONT_SIZE as u16,
                font_scale: 1.0,
                font_scale_aspect: 1.0,
                color: content_color,
                ..Default::default()
            },
        );

        // Handle click
        if ctx.mouse.clicked(&tab_rect) {
            clicked_tab = Some(i);
        }

        x += tab_width;
    }

    clicked_tab
}

/// Draw a fixed tab bar with icons, labels, version, and auth/storage controls
/// Returns a TabBarAction indicating what was clicked
pub fn draw_fixed_tabs_with_auth(
    ctx: &mut UiContext,
    rect: Rect,
    tabs: &[TabEntry],
    active_index: usize,
    icon_font: Option<&Font>,
    version: Option<&str>,
    version_highlighted: &mut bool,
    storage_mode: StorageMode,
    can_write: bool,
    is_authenticated: bool,
) -> TabBarAction {
    let mut action = TabBarAction::None;

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

    // Calculate positions from right edge
    let padding_right = 16.0;
    let font_size = 14.0;
    let mut right_x = rect.x + rect.w - padding_right;

    // === VERSION (far right) ===
    if let Some(ver) = version {
        let version_text = format!("v{}", ver);
        let ver_font_size = 18.0;
        let text_dims = measure_text(&version_text, None, ver_font_size as u16, 1.0);
        let text_x = right_x - text_dims.width;
        let text_y = rect.y + (rect.h + text_dims.height) * 0.5 - 1.0;

        // Create clickable rect for the version
        let version_rect = Rect::new(text_x - 4.0, rect.y, text_dims.width + 8.0, rect.h);

        // Toggle highlight on click (easter egg!)
        if ctx.mouse.clicked(&version_rect) {
            *version_highlighted = !*version_highlighted;
        }

        if *version_highlighted {
            // Knight Rider scanner effect!
            let time = get_time() as f32;
            let char_count = version_text.chars().count() as f32;
            let speed = 3.0;
            let phase = (time * speed) % 2.0;
            let scanner_pos = if phase < 1.0 {
                phase * (char_count - 1.0)
            } else {
                (2.0 - phase) * (char_count - 1.0)
            };

            let mut char_x = text_x;
            for (i, ch) in version_text.chars().enumerate() {
                let char_str = ch.to_string();
                let char_dims = measure_text(&char_str, None, ver_font_size as u16, 1.0);
                let distance = (i as f32 - scanner_pos).abs();
                let glow = (1.0 - distance / 2.0).max(0.0).powf(0.5);
                let gray = Color::new(0.4, 0.4, 0.45, 1.0);
                let r = gray.r + (style::ACCENT.r - gray.r) * glow;
                let g = gray.g + (style::ACCENT.g - gray.g) * glow;
                let b = gray.b + (style::ACCENT.b - gray.b) * glow;
                let char_color = Color::new(r, g, b, 1.0);

                draw_text_ex(
                    &char_str,
                    char_x.round(),
                    text_y.round(),
                    TextParams {
                        font: None,
                        font_size: ver_font_size as u16,
                        color: char_color,
                        ..Default::default()
                    },
                );
                char_x += char_dims.width;
            }
        } else {
            draw_text_ex(
                &version_text,
                text_x.round(),
                text_y.round(),
                TextParams {
                    font: None,
                    font_size: ver_font_size as u16,
                    color: Color::new(0.5, 0.5, 0.55, 1.0),
                    ..Default::default()
                },
            );
        }

        right_x = text_x - 20.0; // Gap before next element
    }

    // === SIGN IN/OUT BUTTON ===
    let button_text = if is_authenticated { "Sign Out" } else { "Sign In" };
    let button_dims = measure_text(button_text, None, font_size as u16, 1.0);
    let button_padding = 12.0;
    let button_width = button_dims.width + button_padding * 2.0;
    let button_height = 24.0;
    let button_x = right_x - button_width;
    let button_y = rect.y + (rect.h - button_height) * 0.5;
    let button_rect = Rect::new(button_x, button_y, button_width, button_height);

    let is_hovered = ctx.mouse.inside(&button_rect);
    let button_bg = if is_hovered {
        Color::new(0.22, 0.22, 0.26, 1.0)
    } else {
        Color::new(0.16, 0.16, 0.20, 1.0)
    };
    let button_border = if is_authenticated {
        Color::new(0.5, 0.5, 0.55, 1.0) // Gray for sign out
    } else {
        style::ACCENT // Cyan for sign in
    };

    draw_rectangle(button_x, button_y, button_width, button_height, button_bg);
    draw_rectangle_lines(button_x, button_y, button_width, button_height, 1.0, button_border);

    let text_x = button_x + button_padding;
    let text_y = button_y + (button_height + button_dims.height) * 0.5 - 2.0;
    draw_text_ex(
        button_text,
        text_x.round(),
        text_y.round(),
        TextParams {
            font: None,
            font_size: font_size as u16,
            color: Color::new(0.9, 0.9, 0.92, 1.0),
            ..Default::default()
        },
    );

    if ctx.mouse.clicked(&button_rect) {
        action = if is_authenticated {
            TabBarAction::SignOut
        } else {
            TabBarAction::SignIn
        };
    }

    right_x = button_x - 16.0; // Gap before storage label

    // === STORAGE LABEL ===
    let mode_text = match (storage_mode, can_write) {
        (StorageMode::Cloud, _) => "Storage: Cloud",
        (StorageMode::Local, true) => "Storage: Local",
        (StorageMode::Local, false) => "Storage: Read-only",
    };
    let mode_color = match (storage_mode, can_write) {
        (StorageMode::Cloud, _) => Color::new(0.3, 0.8, 0.3, 1.0), // Green for cloud
        (StorageMode::Local, true) => style::ACCENT,
        (StorageMode::Local, false) => Color::new(0.6, 0.6, 0.65, 1.0),
    };

    let mode_dims = measure_text(mode_text, None, font_size as u16, 1.0);
    let mode_x = right_x - mode_dims.width;
    let mode_y = rect.y + (rect.h + mode_dims.height) * 0.5 - 1.0;

    draw_text_ex(
        mode_text,
        mode_x.round(),
        mode_y.round(),
        TextParams {
            font: None,
            font_size: font_size as u16,
            color: mode_color,
            ..Default::default()
        },
    );

    // === TABS (left side) ===
    if !tabs.is_empty() {
        let mut x = rect.x.round();
        let y = rect.y.round();
        let h = rect.h.round();

        for (i, tab) in tabs.iter().enumerate() {
            let text_dims = measure_text(tab.label, None, layout::FONT_SIZE as u16, 1.0);
            let content_width = layout::ICON_SIZE + layout::ICON_LABEL_GAP + text_dims.width;
            let tab_width = (content_width + layout::TAB_PADDING_H * 2.0).round();

            let tab_rect = Rect::new(x, y, tab_width, h);
            let is_active = i == active_index;
            let is_tab_hovered = ctx.mouse.inside(&tab_rect);

            let bg_color = if is_active {
                style::TAB_ACTIVE_BG
            } else if is_tab_hovered {
                style::TAB_HOVER_BG
            } else {
                style::TAB_INACTIVE_BG
            };

            draw_rectangle(tab_rect.x, tab_rect.y, tab_rect.w, tab_rect.h, bg_color);

            // Separator
            draw_rectangle(
                tab_rect.x + tab_rect.w - 1.0,
                tab_rect.y + 6.0,
                1.0,
                tab_rect.h - 12.0,
                style::TAB_BORDER,
            );

            // Active indicator
            if is_active {
                draw_rectangle(
                    tab_rect.x,
                    tab_rect.y + tab_rect.h - layout::INDICATOR_HEIGHT,
                    tab_rect.w,
                    layout::INDICATOR_HEIGHT,
                    style::ACCENT,
                );
            }

            let content_color = if is_active {
                style::TAB_ACTIVE_TEXT
            } else {
                style::TAB_INACTIVE_TEXT
            };

            let center_y = tab_rect.y + tab_rect.h * 0.5;
            let content_start_x = tab_rect.x + layout::TAB_PADDING_H;

            // Icon
            let icon_x = content_start_x;
            let icon_y = (center_y + layout::ICON_SIZE * 0.5).round();
            draw_text_ex(
                &tab.icon.to_string(),
                icon_x.round(),
                icon_y,
                TextParams {
                    font: icon_font,
                    font_size: layout::ICON_SIZE as u16,
                    color: content_color,
                    ..Default::default()
                },
            );

            // Label
            let label_x = (content_start_x + layout::ICON_SIZE + layout::ICON_LABEL_GAP).round();
            let label_y = (center_y + text_dims.height * 0.5 - 1.0).round();
            draw_text_ex(
                tab.label,
                label_x,
                label_y,
                TextParams {
                    font: None,
                    font_size: layout::FONT_SIZE as u16,
                    font_scale: 1.0,
                    font_scale_aspect: 1.0,
                    color: content_color,
                    ..Default::default()
                },
            );

            // Handle click
            if ctx.mouse.clicked(&tab_rect) && action == TabBarAction::None {
                action = TabBarAction::SwitchTab(i);
            }

            x += tab_width;
        }
    }

    action
}
