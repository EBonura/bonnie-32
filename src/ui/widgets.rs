//! Basic UI widgets

use macroquad::prelude::*;
use super::{Rect, UiContext, draw_icon_centered};

// =============================================================================
// Scrollable List Widget
// =============================================================================

/// Colors for the scrollable list
pub struct ListColors {
    pub row_even: Color,
    pub row_odd: Color,
    pub row_selected: Color,
    pub row_hovered: Color,
    pub text_normal: Color,
    pub text_selected: Color,
}

impl Default for ListColors {
    fn default() -> Self {
        Self {
            row_even: Color::new(0.13, 0.13, 0.15, 1.0),
            row_odd: Color::new(0.11, 0.11, 0.13, 1.0),
            row_selected: ACCENT_COLOR,
            row_hovered: Color::new(0.20, 0.20, 0.24, 1.0),
            text_normal: Color::new(0.78, 0.78, 0.78, 1.0),
            text_selected: WHITE,
        }
    }
}

/// Result from drawing a scrollable list
pub struct ListResult {
    /// Index of clicked item (if any)
    pub clicked: Option<usize>,
    /// Index of double-clicked item (if any)
    pub double_clicked: Option<usize>,
}

/// Draw a scrollable list with alternating row colors
///
/// - `ctx`: UI context for input handling
/// - `rect`: Bounding rectangle for the list
/// - `items`: Slice of item labels to display
/// - `selected`: Currently selected index (if any)
/// - `scroll_offset`: Mutable scroll offset (will be updated on scroll)
/// - `row_height`: Height of each row
/// - `colors`: Optional custom colors (uses default if None)
///
/// Returns clicked/double-clicked indices
pub fn draw_scrollable_list(
    ctx: &mut UiContext,
    rect: Rect,
    items: &[String],
    selected: Option<usize>,
    scroll_offset: &mut f32,
    row_height: f32,
    colors: Option<&ListColors>,
) -> ListResult {
    let default_colors = ListColors::default();
    let colors = colors.unwrap_or(&default_colors);

    let mut result = ListResult {
        clicked: None,
        double_clicked: None,
    };

    // Handle scrolling
    if ctx.mouse.inside(&rect) {
        let scroll_delta = mouse_wheel().1 * 30.0;
        let max_scroll = (items.len() as f32 * row_height - rect.h).max(0.0);
        *scroll_offset = (*scroll_offset - scroll_delta).clamp(0.0, max_scroll);
    }

    // Calculate visible range
    let start_idx = (*scroll_offset / row_height).floor() as usize;
    let visible_count = (rect.h / row_height).ceil() as usize + 1;
    let end_idx = (start_idx + visible_count).min(items.len());

    // Draw visible items
    for i in start_idx..end_idx {
        let y = rect.y + (i as f32 * row_height) - *scroll_offset;

        // Skip if outside visible area
        if y + row_height < rect.y || y > rect.bottom() {
            continue;
        }

        let item_rect = Rect::new(rect.x, y, rect.w, row_height);
        let is_selected = selected == Some(i);
        let is_hovered = ctx.mouse.inside(&item_rect) && ctx.mouse.inside(&rect);

        // Row background
        let bg_color = if is_selected {
            colors.row_selected
        } else if is_hovered {
            colors.row_hovered
        } else if i % 2 == 0 {
            colors.row_even
        } else {
            colors.row_odd
        };
        draw_rectangle(item_rect.x, item_rect.y, item_rect.w, item_rect.h, bg_color);

        // Text
        let text_color = if is_selected { colors.text_selected } else { colors.text_normal };
        let text_y = y + (row_height + 12.0) / 2.0; // Approximate vertical centering for 12px font
        draw_text(&items[i], rect.x + 8.0, text_y, 14.0, text_color);

        // Click handling
        if is_hovered && ctx.mouse.left_pressed {
            result.clicked = Some(i);
        }
    }

    // Draw scrollbar if needed
    let total_height = items.len() as f32 * row_height;
    if total_height > rect.h {
        let scrollbar_w = 6.0;
        let scrollbar_x = rect.right() - scrollbar_w - 2.0;
        let scrollbar_h = (rect.h / total_height * rect.h).max(20.0);
        let max_scroll = total_height - rect.h;
        let scrollbar_y = rect.y + (*scroll_offset / max_scroll) * (rect.h - scrollbar_h);

        // Scrollbar track
        draw_rectangle(scrollbar_x, rect.y, scrollbar_w, rect.h, Color::new(0.08, 0.08, 0.1, 1.0));
        // Scrollbar thumb
        draw_rectangle(scrollbar_x, scrollbar_y, scrollbar_w, scrollbar_h, Color::new(0.3, 0.3, 0.35, 1.0));
    }

    result
}

// Platform-specific URL opening
#[cfg(not(target_arch = "wasm32"))]
fn open_url(url: &str) {
    let _ = webbrowser::open(url);
}

#[cfg(target_arch = "wasm32")]
extern "C" {
    fn bonnie_open_url(ptr: *const u8, len: usize);
}

#[cfg(target_arch = "wasm32")]
fn open_url(url: &str) {
    unsafe { bonnie_open_url(url.as_ptr(), url.len()) }
}

// =============================================================================
// Clickable Link Widget
// =============================================================================

/// Result of drawing a clickable link
pub struct LinkResult {
    /// The bounding rect of the link (for layout)
    pub rect: Rect,
    /// Whether the link was clicked
    pub clicked: bool,
}

/// Draw a clickable text link that opens a URL when clicked
/// Returns the link rect for layout purposes and whether it was clicked
pub fn draw_link(
    x: f32,
    y: f32,
    text: &str,
    url: &str,
    font_size: f32,
    color: Color,
    hover_color: Color,
) -> LinkResult {
    let dims = measure_text(text, None, font_size as u16, 1.0);
    let link_rect = Rect::new(x, y - dims.height, dims.width, dims.height + 4.0);

    let (mx, my) = mouse_position();
    let hovered = link_rect.contains(mx, my);
    let clicked = hovered && is_mouse_button_pressed(MouseButton::Left);

    // Draw text with appropriate color
    let draw_color = if hovered { hover_color } else { color };
    draw_text(text, x, y, font_size, draw_color);

    // Draw underline when hovered
    if hovered {
        draw_line(x, y + 2.0, x + dims.width, y + 2.0, 1.0, draw_color);
    }

    // Open URL if clicked
    if clicked {
        open_url(url);
    }

    LinkResult {
        rect: link_rect,
        clicked,
    }
}

/// Draw a row of links separated by a separator string
/// Returns the total width used
pub fn draw_link_row(
    x: f32,
    y: f32,
    links: &[(&str, &str)], // (text, url) pairs
    separator: &str,
    font_size: f32,
    color: Color,
    hover_color: Color,
    separator_color: Color,
) -> f32 {
    let mut cursor_x = x;
    let sep_dims = measure_text(separator, None, font_size as u16, 1.0);

    for (i, (text, url)) in links.iter().enumerate() {
        // Draw separator before all but first link
        if i > 0 {
            draw_text(separator, cursor_x, y, font_size, separator_color);
            cursor_x += sep_dims.width;
        }

        // Draw link
        let result = draw_link(cursor_x, y, text, url, font_size, color, hover_color);
        cursor_x += result.rect.w;
    }

    cursor_x - x // Return total width
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

    /// Add a letter button with active state (for object type picker)
    pub fn letter_button_active(&mut self, ctx: &mut UiContext, letter: char, tooltip: &str, is_active: bool) -> bool {
        let size = (self.rect.h - 4.0).round();
        let btn_rect = Rect::new(self.cursor_x.round(), (self.rect.y + 2.0).round(), size, size);
        self.cursor_x += size + self.spacing;
        letter_button_active(ctx, btn_rect, letter, tooltip, is_active)
    }

    /// Add an arrow picker widget: "< label >" with clickable arrows
    /// Returns true if either arrow was clicked. The callback receives -1 (left) or +1 (right).
    pub fn arrow_picker<F>(&mut self, ctx: &mut UiContext, icon_font: Option<&Font>, label: &str, on_change: &mut F) -> bool
    where
        F: FnMut(i32),
    {
        let size = (self.rect.h - 4.0).round();
        let arrow_size = size;
        let y = (self.rect.y + 2.0).round();

        // Measure label text
        let font_size = 14.0;
        let text_dims = measure_text(label, None, font_size as u16, 1.0);
        let label_width = text_dims.width.max(60.0); // Minimum width for short labels

        // Left arrow button "<"
        let left_rect = Rect::new(self.cursor_x.round(), y, arrow_size, size);
        self.cursor_x += arrow_size;

        // Label area (centered text)
        let label_rect = Rect::new(self.cursor_x.round(), y, label_width + 8.0, size);
        self.cursor_x += label_width + 8.0;

        // Right arrow button ">"
        let right_rect = Rect::new(self.cursor_x.round(), y, arrow_size, size);
        self.cursor_x += arrow_size + self.spacing;

        // Draw left arrow
        let left_clicked = draw_arrow_button(ctx, left_rect, icon_font, true);

        // Draw label with subtle background
        draw_rectangle(
            label_rect.x, label_rect.y, label_rect.w, label_rect.h,
            Color::from_rgba(50, 50, 55, 255),
        );
        // Center label text
        let text_x = label_rect.x + (label_rect.w - text_dims.width) * 0.5;
        let text_y = label_rect.y + (label_rect.h + text_dims.height) * 0.5 - 2.0;
        draw_text(label, text_x.round(), text_y.round(), font_size, WHITE);

        // Draw right arrow
        let right_clicked = draw_arrow_button(ctx, right_rect, icon_font, false);

        if left_clicked {
            on_change(-1);
            true
        } else if right_clicked {
            on_change(1);
            true
        } else {
            false
        }
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

/// Draw an arrow button (< or >) for picker navigation
fn draw_arrow_button(ctx: &mut UiContext, rect: Rect, icon_font: Option<&Font>, is_left: bool) -> bool {
    let id = ctx.next_id();
    let hovered = ctx.mouse.inside(&rect);
    let pressed = ctx.mouse.clicking(&rect);
    let clicked = ctx.mouse.clicked(&rect);

    if hovered {
        ctx.set_hot(id);
    }

    let corner_radius = 4.0;

    // Draw background on hover/press
    if pressed {
        draw_rounded_rect(rect.x, rect.y, rect.w, rect.h, corner_radius, Color::from_rgba(60, 60, 70, 255));
    } else if hovered {
        draw_rounded_rect(rect.x, rect.y, rect.w, rect.h, corner_radius, Color::from_rgba(50, 50, 60, 255));
    }

    // Arrow color
    let arrow_color = if hovered {
        Color::from_rgba(220, 220, 220, 255)
    } else {
        Color::from_rgba(160, 160, 160, 255)
    };

    // Draw arrow using chevron icons
    let icon = if is_left {
        crate::ui::icons::icon::CHEVRON_LEFT
    } else {
        crate::ui::icons::icon::CHEVRON_RIGHT
    };
    let icon_size = (rect.h * 0.5).min(14.0);
    draw_icon_centered(icon_font, icon, &rect, icon_size, arrow_color);

    clicked
}

/// Draw a letter button with active state (for object type picker)
pub fn letter_button_active(ctx: &mut UiContext, rect: Rect, letter: char, tooltip: &str, is_active: bool) -> bool {
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

    // Draw background
    if is_active {
        draw_rounded_rect(rect.x, rect.y, rect.w, rect.h, corner_radius, ACCENT_COLOR);
    } else if pressed {
        draw_rounded_rect(rect.x, rect.y, rect.w, rect.h, corner_radius, Color::from_rgba(60, 60, 70, 255));
    } else if hovered {
        draw_rounded_rect(rect.x, rect.y, rect.w, rect.h, corner_radius, Color::from_rgba(50, 50, 60, 255));
    }

    // Letter color
    let letter_color = if is_active {
        WHITE
    } else if hovered {
        Color::from_rgba(220, 220, 220, 255)
    } else {
        Color::from_rgba(180, 180, 180, 255)
    };

    // Draw letter centered
    let text = letter.to_string();
    let font_size = (rect.h * 0.6).min(14.0) as u16;
    let text_dims = measure_text(&text, None, font_size, 1.0);
    let text_x = rect.x + (rect.w - text_dims.width) / 2.0;
    let text_y = rect.y + (rect.h + text_dims.height) / 2.0 - 2.0;
    draw_text(&text, text_x, text_y, font_size as f32, letter_color);

    clicked
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
    let knob_rect = Rect::new(center_x - radius, center_y - radius, radius * 2.0, radius * 2.0);
    let hovered = ctx.mouse.inside(&knob_rect);

    // Colors
    let bg_color = Color::new(0.12, 0.12, 0.15, 1.0);
    let ring_color = Color::new(0.25, 0.25, 0.3, 1.0);
    let indicator_color = ACCENT_COLOR;
    let text_color = Color::new(0.8, 0.8, 0.8, 1.0);
    let label_color = Color::new(0.6, 0.6, 0.6, 1.0);

    // Draw knob body (outer ring) - thicker perimeter
    draw_circle(center_x, center_y, radius, ring_color);
    draw_circle(center_x, center_y, radius - 5.0, bg_color);

    // Knob rotation: map 0-127 to angle range
    // Start at 225° (bottom-left), end at -45° (bottom-right) = 270° sweep
    let start_angle = 225.0_f32.to_radians();
    let end_angle = -45.0_f32.to_radians();
    let angle_range = start_angle - end_angle; // 270 degrees

    let normalized = value as f32 / 127.0;
    let angle = start_angle - normalized * angle_range;

    // Draw arc showing value (using line segments)
    let arc_radius = radius - 2.5; // Center of the 5px ring
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
                draw_line(x1, y1, x2, y2, 5.0, indicator_color);
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
            draw_line(x1, y1, x2, y2, 5.0, indicator_color);
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
        let mouse_angle = dx.atan2(dy); // atan2(x,y) gives angle from vertical (12 o'clock)

        // The knob sweeps from 225° to -45° (or equivalently, from -135° to 45° from 12 o'clock)
        // Convert to a 0-1 range where:
        // - Leftmost position (225° = -135° from vertical) = 0
        // - Rightmost position (-45° = 45° from vertical) = 1
        // atan2(x,y) returns: 0 at top, positive going clockwise, -π to π range

        // mouse_angle: -π to π, where 0 is up, positive is right/clockwise
        // We want: -135° (-3π/4) = 0.0, +45° (π/4) = 1.0
        // Linear mapping: norm = (mouse_angle - (-3π/4)) / (π/4 - (-3π/4))
        //                      = (mouse_angle + 3π/4) / π

        let min_angle = -135.0_f32.to_radians(); // -3π/4
        let max_angle = 45.0_f32.to_radians();   // π/4

        // Handle the dead zone at the bottom (between 135° and 180°, and -180° and -135°)
        let mut norm = (mouse_angle - min_angle) / (max_angle - min_angle);

        // Clamp to valid range - if in the dead zone at bottom, snap to nearest end
        if mouse_angle > max_angle && mouse_angle <= std::f32::consts::PI {
            // Bottom-right dead zone - snap to max
            norm = 1.0;
        } else if mouse_angle < min_angle && mouse_angle >= -std::f32::consts::PI {
            // Bottom-left dead zone - snap to min
            norm = 0.0;
        }

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

// =============================================================================
// Drag Value Widget (for numeric input with drag-to-adjust)
// =============================================================================

/// Result from drawing a drag value widget
pub struct DragValueResult {
    /// New value if changed
    pub value: Option<f32>,
    /// Whether the widget is being dragged
    pub dragging: bool,
}

/// Draw a compact drag value without label (for inline use)
/// with optional text editing support. When editing_field matches field_id, shows text input.
pub fn draw_drag_value_compact_editable(
    ctx: &mut UiContext,
    rect: Rect,
    value: f32,
    step: f32,
    drag_id: u64,
    is_dragging: &mut bool,
    drag_start_value: &mut f32,
    drag_start_x: &mut f32,
    editing_field: Option<&mut Option<usize>>,
    edit_state: Option<(&mut String, usize)>, // (buffer, field_id)
) -> DragValueResult {
    let hovered = ctx.mouse.inside(&rect);

    // Check if this field is being edited
    let is_editing = match (&editing_field, &edit_state) {
        (Some(ef), Some((_, field_id))) => **ef == Some(*field_id),
        _ => false,
    };

    // Colors
    let bg_color = if is_editing {
        Color::from_rgba(50, 60, 70, 255)
    } else if *is_dragging {
        Color::from_rgba(60, 60, 70, 255)
    } else if hovered {
        Color::from_rgba(50, 50, 55, 255)
    } else {
        Color::from_rgba(40, 40, 45, 255)
    };
    let border_color = if is_editing {
        ACCENT_COLOR
    } else {
        Color::from_rgba(60, 60, 65, 255)
    };
    let value_color = if is_editing || *is_dragging { ACCENT_COLOR } else { WHITE };

    // Draw background
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, bg_color);
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, border_color);

    let mut new_value = None;

    if is_editing {
        // Text input mode
        if let Some((buffer, _field_id)) = edit_state {
            if let Some(ef) = editing_field {
                // Draw text buffer
                let text_y = rect.y + rect.h * 0.5 + 4.0;
                let display_text = if buffer.is_empty() { "0" } else { buffer.as_str() };
                let text_dims = measure_text(display_text, None, 11, 1.0);
                let text_x = rect.x + (rect.w - text_dims.width) * 0.5;
                draw_text(display_text, text_x, text_y, 11.0, value_color);

                // Draw cursor (blinking)
                let time = macroquad::time::get_time();
                if (time * 2.0) as i32 % 2 == 0 {
                    let cursor_x = text_x + text_dims.width + 1.0;
                    draw_line(cursor_x, rect.y + 3.0, cursor_x, rect.bottom() - 3.0, 1.0, ACCENT_COLOR);
                }

                // Handle keyboard input
                while let Some(c) = get_char_pressed() {
                    if c.is_ascii_digit() || c == '.' || c == '-' {
                        buffer.push(c);
                    }
                }

                // Handle backspace
                if is_key_pressed(KeyCode::Backspace) {
                    buffer.pop();
                }

                // Handle Enter - confirm edit
                if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) {
                    if let Ok(v) = buffer.parse::<f32>() {
                        new_value = Some(v);
                    }
                    *ef = None;
                    buffer.clear();
                }

                // Handle Escape - cancel edit
                if is_key_pressed(KeyCode::Escape) {
                    *ef = None;
                    buffer.clear();
                }

                // Click outside to confirm
                if ctx.mouse.left_pressed && !hovered {
                    if let Ok(v) = buffer.parse::<f32>() {
                        new_value = Some(v);
                    }
                    *ef = None;
                    buffer.clear();
                }
            }
        }
    } else {
        // Normal display mode
        let value_str = format!("{:.2}", value);
        let value_dims = measure_text(&value_str, None, 11, 1.0);
        let value_x = rect.x + (rect.w - value_dims.width) * 0.5;
        let text_y = rect.y + rect.h * 0.5 + 4.0;
        draw_text(&value_str, value_x, text_y, 11.0, value_color);

        // Handle double-click to start editing
        if let (Some(ef), Some((buffer, field_id))) = (editing_field, edit_state) {
            if hovered && ctx.mouse.double_clicked {
                *ef = Some(field_id);
                *buffer = format!("{:.2}", value);
            }
        }

        // Handle drag interaction
        // Start dragging on mouse press
        if hovered && ctx.mouse.left_pressed && !*is_dragging {
            *is_dragging = true;
            *drag_start_value = value;
            *drag_start_x = ctx.mouse.x;
            ctx.dragging = Some(drag_id);
        }

        // Continue dragging
        if *is_dragging && ctx.mouse.left_down {
            let delta_x = ctx.mouse.x - *drag_start_x;
            let new_val = *drag_start_value + delta_x * step;
            new_value = Some(new_val);
        }

        // End dragging
        if *is_dragging && !ctx.mouse.left_down {
            *is_dragging = false;
            ctx.dragging = None;
        }
    }

    DragValueResult {
        value: new_value,
        dragging: *is_dragging,
    }
}

// =============================================================================
// PS1 Color Picker Widget (15-bit color: 5 bits per channel, 0-31 range)
// =============================================================================

use crate::rasterizer::Color as RasterColor;

/// Result from the PS1 color picker
pub struct ColorPickerResult {
    /// New color if changed
    pub color: Option<RasterColor>,
    /// Whether a slider is currently being dragged
    pub active: bool,
}

/// PS1 preset colors (5-bit values)
const PS1_PRESETS: [(u8, u8, u8); 8] = [
    (31, 31, 31), // White
    (0, 0, 0),    // Black
    (31, 0, 0),   // Red
    (0, 31, 0),   // Green
    (0, 0, 31),   // Blue
    (31, 31, 0),  // Yellow
    (0, 31, 31),  // Cyan
    (31, 0, 31),  // Magenta
];

/// Draw a PS1-authentic color picker with RGB sliders (0-31 range) and preset swatches
///
/// Layout:
/// ```text
/// [Swatch]  R: [====|------] 16
///           G: [==|--------] 8
///           B: [======|----] 24
/// [■][■][■][■][■][■][■][■]  <- presets
/// ```
pub fn draw_ps1_color_picker(
    ctx: &mut UiContext,
    x: f32,
    y: f32,
    width: f32,
    current_color: RasterColor,
    label: &str,
    active_slider: &mut Option<usize>, // 0=R, 1=G, 2=B
) -> ColorPickerResult {
    let mut result = ColorPickerResult {
        color: None,
        active: false,
    };

    let swatch_size = 32.0;
    let slider_height = 10.0;
    let slider_spacing = 1.0;
    let label_width = 16.0;
    let value_width = 20.0;
    let slider_x = x + swatch_size + 8.0 + label_width;
    let slider_width = width - swatch_size - 8.0 - label_width - value_width - 4.0;
    let sliders_total_height = 3.0 * slider_height + 2.0 * slider_spacing; // 3 sliders with 2 gaps

    let text_color = Color::new(0.8, 0.8, 0.8, 1.0);
    let label_color = Color::new(0.6, 0.6, 0.6, 1.0);
    let track_bg = Color::new(0.15, 0.15, 0.18, 1.0);

    // Draw label above
    if !label.is_empty() {
        draw_text(label, x, y - 4.0, 11.0, label_color);
    }

    // Draw color swatch (current color)
    let swatch_y = y;
    draw_rectangle(x, swatch_y, swatch_size, swatch_size, Color::from_rgba(60, 60, 65, 255));
    draw_rectangle(
        x + 1.0,
        swatch_y + 1.0,
        swatch_size - 2.0,
        swatch_size - 2.0,
        Color::from_rgba(current_color.r, current_color.g, current_color.b, 255),
    );

    // Get current 5-bit values
    let mut r5 = current_color.r5();
    let mut g5 = current_color.g5();
    let mut b5 = current_color.b5();

    // Draw RGB sliders (vertically centered with swatch)
    let sliders_start_y = y + (swatch_size - sliders_total_height) / 2.0;
    let channels = [
        ("R", r5, Color::new(0.8, 0.2, 0.2, 1.0)),
        ("G", g5, Color::new(0.2, 0.8, 0.2, 1.0)),
        ("B", b5, Color::new(0.2, 0.4, 0.9, 1.0)),
    ];

    for (i, (name, value, tint)) in channels.iter().enumerate() {
        let slider_y = sliders_start_y + (i as f32) * (slider_height + slider_spacing);

        // Label
        draw_text(name, x + swatch_size + 8.0, slider_y + slider_height - 3.0, 11.0, text_color);

        // Slider track background
        let track_rect = Rect::new(slider_x, slider_y, slider_width, slider_height);
        draw_rectangle(track_rect.x, track_rect.y, track_rect.w, track_rect.h, track_bg);

        // Filled portion with channel tint
        let fill_ratio = *value as f32 / 31.0;
        let fill_width = fill_ratio * slider_width;
        draw_rectangle(track_rect.x, track_rect.y, fill_width, track_rect.h, *tint);

        // Thumb indicator
        let thumb_x = track_rect.x + fill_width - 1.0;
        draw_rectangle(thumb_x, track_rect.y, 3.0, track_rect.h, WHITE);

        // Value text
        let value_str = format!("{:2}", value);
        draw_text(
            &value_str,
            slider_x + slider_width + 4.0,
            slider_y + slider_height - 3.0,
            11.0,
            text_color,
        );

        // Handle slider interaction
        let hovered = ctx.mouse.inside(&track_rect);

        // Start dragging
        if hovered && ctx.mouse.left_pressed {
            *active_slider = Some(i);
        }

        // Continue dragging (even outside the rect)
        if *active_slider == Some(i) && ctx.mouse.left_down {
            result.active = true;
            let rel_x = (ctx.mouse.x - track_rect.x).clamp(0.0, slider_width);
            let new_val = ((rel_x / slider_width) * 31.0).round() as u8;
            match i {
                0 => r5 = new_val,
                1 => g5 = new_val,
                2 => b5 = new_val,
                _ => {}
            }
            result.color = Some(RasterColor::from_ps1(r5, g5, b5));
        }

        // End dragging
        if *active_slider == Some(i) && !ctx.mouse.left_down {
            *active_slider = None;
        }
    }

    // Draw preset row below sliders
    let preset_y = y + swatch_size + 6.0;
    let preset_size = 14.0;
    let preset_spacing = 2.0;

    for (i, (pr, pg, pb)) in PS1_PRESETS.iter().enumerate() {
        let preset_x = x + (i as f32) * (preset_size + preset_spacing);
        let preset_rect = Rect::new(preset_x, preset_y, preset_size, preset_size);

        // Border
        draw_rectangle(preset_x, preset_y, preset_size, preset_size, Color::from_rgba(60, 60, 65, 255));

        // Color fill
        let preset_color = RasterColor::from_ps1(*pr, *pg, *pb);
        draw_rectangle(
            preset_x + 1.0,
            preset_y + 1.0,
            preset_size - 2.0,
            preset_size - 2.0,
            Color::from_rgba(preset_color.r, preset_color.g, preset_color.b, 255),
        );

        // Click to apply preset
        if ctx.mouse.inside(&preset_rect) && ctx.mouse.left_pressed {
            result.color = Some(preset_color);
        }
    }

    result
}

/// Calculate the height needed for a PS1 color picker
pub fn ps1_color_picker_height() -> f32 {
    // Swatch height (32) + spacing (6) + preset row (14) = 52
    52.0
}
