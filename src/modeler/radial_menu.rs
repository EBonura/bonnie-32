//! Radial/Pie Menu Component
//!
//! A game-style radial menu that appears while holding a key (Tab).
//! The menu is context-sensitive - items change based on current selection.
//!
//! Usage:
//! - Hold Tab to open menu at cursor
//! - Move mouse toward option to highlight
//! - Release Tab to select highlighted option (or click)
//! - Move mouse to center and release to cancel

use macroquad::prelude::*;
use std::f32::consts::PI;

/// A single item in the radial menu
#[derive(Clone)]
pub struct RadialMenuItem {
    /// Unique identifier for this item
    pub id: String,
    /// Display label
    pub label: String,
    /// Optional icon character
    pub icon: Option<char>,
    /// Optional sub-items (for nested menus)
    pub children: Vec<RadialMenuItem>,
    /// Whether this item is enabled
    pub enabled: bool,
}

impl RadialMenuItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            children: Vec::new(),
            enabled: true,
        }
    }

    pub fn with_icon(mut self, icon: char) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn with_children(mut self, children: Vec<RadialMenuItem>) -> Self {
        self.children = children;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// State of the radial menu
#[derive(Clone, Default)]
pub struct RadialMenuState {
    /// Is the menu currently open?
    pub is_open: bool,
    /// Center position of the menu
    pub center: (f32, f32),
    /// Currently highlighted item index (None = center/cancel)
    pub highlighted: Option<usize>,
    /// Current items being displayed
    pub items: Vec<RadialMenuItem>,
    /// Stack of parent menus (for nested navigation)
    pub menu_stack: Vec<Vec<RadialMenuItem>>,
    /// Selected item ID (set when menu closes with selection)
    pub selected_id: Option<String>,
}

impl RadialMenuState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open the menu at the given position with the given items
    pub fn open(&mut self, x: f32, y: f32, items: Vec<RadialMenuItem>) {
        self.is_open = true;
        self.center = (x, y);
        self.items = items;
        self.highlighted = None;
        self.selected_id = None;
        self.menu_stack.clear();
    }

    /// Close the menu, optionally selecting the highlighted item
    pub fn close(&mut self, select: bool) -> Option<String> {
        self.is_open = false;
        if select {
            if let Some(idx) = self.highlighted {
                if let Some(item) = self.items.get(idx) {
                    if item.enabled {
                        self.selected_id = Some(item.id.clone());
                        return self.selected_id.clone();
                    }
                }
            }
        }
        self.selected_id = None;
        None
    }

    /// Take the selected ID (consumes it)
    pub fn take_selected(&mut self) -> Option<String> {
        self.selected_id.take()
    }

    /// Navigate into a submenu
    pub fn enter_submenu(&mut self, idx: usize) {
        // Check if item has children first
        let children = self.items.get(idx)
            .filter(|item| !item.children.is_empty())
            .map(|item| item.children.clone());

        if let Some(children) = children {
            let current = std::mem::take(&mut self.items);
            self.menu_stack.push(current);
            self.items = children;
            self.highlighted = None;
        }
    }

    /// Navigate back to parent menu
    pub fn back(&mut self) -> bool {
        if let Some(parent) = self.menu_stack.pop() {
            self.items = parent;
            self.highlighted = None;
            true
        } else {
            false
        }
    }
}

/// Configuration for radial menu appearance
pub struct RadialMenuConfig {
    /// Outer radius of the menu
    pub outer_radius: f32,
    /// Inner radius (dead zone / cancel area)
    pub inner_radius: f32,
    /// Background color
    pub bg_color: Color,
    /// Highlighted segment color
    pub highlight_color: Color,
    /// Text color
    pub text_color: Color,
    /// Disabled text color
    pub disabled_color: Color,
    /// Border color
    pub border_color: Color,
}

impl Default for RadialMenuConfig {
    fn default() -> Self {
        Self {
            outer_radius: 120.0,
            inner_radius: 35.0,
            bg_color: Color::from_rgba(40, 42, 48, 240),
            highlight_color: Color::from_rgba(70, 90, 120, 255),
            text_color: Color::from_rgba(220, 220, 220, 255),
            disabled_color: Color::from_rgba(100, 100, 100, 255),
            border_color: Color::from_rgba(80, 80, 90, 255),
        }
    }
}

/// Draw the radial menu and handle input
/// Returns the selected item ID if an item was selected this frame
pub fn draw_radial_menu(
    state: &mut RadialMenuState,
    config: &RadialMenuConfig,
    mouse_x: f32,
    mouse_y: f32,
) -> Option<String> {
    if !state.is_open || state.items.is_empty() {
        return None;
    }

    let (cx, cy) = state.center;
    let item_count = state.items.len();

    // Calculate which segment the mouse is in
    let dx = mouse_x - cx;
    let dy = mouse_y - cy;
    let dist = (dx * dx + dy * dy).sqrt();

    // Update highlighted based on mouse position
    if dist < config.inner_radius {
        // In center = cancel zone
        state.highlighted = None;
    } else if dist < config.outer_radius * 1.5 {
        // In a segment
        let angle = dy.atan2(dx);
        // Normalize angle to 0..2PI, with 0 at top
        let normalized = (angle + PI * 0.5 + PI * 2.0) % (PI * 2.0);
        let segment_size = (PI * 2.0) / item_count as f32;
        let segment_idx = (normalized / segment_size) as usize % item_count;
        state.highlighted = Some(segment_idx);
    }

    // Draw background polygon (16 sides for that 90s look, perfectly aligned)
    draw_poly_aligned(cx, cy, config.outer_radius, 16, config.bg_color, config.border_color, 2.0);

    // Draw segments
    let segment_angle = (PI * 2.0) / item_count as f32;

    for (i, item) in state.items.iter().enumerate() {
        let start_angle = -PI * 0.5 + (i as f32 * segment_angle);
        let end_angle = start_angle + segment_angle;
        let mid_angle = start_angle + segment_angle * 0.5;

        let is_highlighted = state.highlighted == Some(i);

        // Draw segment highlight
        if is_highlighted {
            draw_segment(
                cx, cy,
                config.inner_radius,
                config.outer_radius,
                start_angle,
                end_angle,
                config.highlight_color,
            );
        }

        // Draw segment divider lines
        let line_x = cx + start_angle.cos() * config.outer_radius;
        let line_y = cy + start_angle.sin() * config.outer_radius;
        let inner_x = cx + start_angle.cos() * config.inner_radius;
        let inner_y = cy + start_angle.sin() * config.inner_radius;
        draw_line(inner_x, inner_y, line_x, line_y, 1.0, config.border_color);

        // Calculate label position (in the middle of the segment)
        let label_dist = (config.inner_radius + config.outer_radius) * 0.55;
        let label_x = cx + mid_angle.cos() * label_dist;
        let label_y = cy + mid_angle.sin() * label_dist;

        // Draw label (no icons - font doesn't support unicode well)
        let text_color = if item.enabled { config.text_color } else { config.disabled_color };
        let font_size = if is_highlighted { 18.0 } else { 16.0 };

        // Center the text roughly
        let text_width = item.label.len() as f32 * font_size * 0.4;
        draw_text(
            &item.label,
            label_x - text_width * 0.5,
            label_y + font_size * 0.3,
            font_size,
            text_color,
        );
    }

    // Check if mouse is in center zone
    let in_center = dist < config.inner_radius;
    let in_submenu = !state.menu_stack.is_empty();

    // Determine which half of center (for submenu: left=back, right=exit)
    let mouse_on_left = dx < 0.0;

    if in_submenu {
        // Split center: Back (left) / Exit (right)
        let base_fill = Color::from_rgba(30, 32, 38, 255);
        let highlight_fill = Color::from_rgba(50, 55, 65, 255);

        // Draw left half (Back)
        let left_fill = if in_center && mouse_on_left { highlight_fill } else { base_fill };
        draw_half_circle(cx, cy, config.inner_radius, true, left_fill);

        // Draw right half (Exit)
        let right_fill = if in_center && !mouse_on_left { highlight_fill } else { base_fill };
        draw_half_circle(cx, cy, config.inner_radius, false, right_fill);

        // Draw divider line
        draw_line(cx, cy - config.inner_radius, cx, cy + config.inner_radius, 1.0, config.border_color);

        // Draw outline
        draw_poly_outline(cx, cy, config.inner_radius, 16, config.border_color, 1.0);

        // Labels
        let back_color = if in_center && mouse_on_left {
            Color::from_rgba(150, 200, 255, 255)
        } else {
            Color::from_rgba(120, 120, 130, 255)
        };
        let exit_color = if in_center && !mouse_on_left {
            Color::from_rgba(255, 150, 150, 255)
        } else {
            Color::from_rgba(120, 120, 130, 255)
        };
        draw_text("<", cx - 18.0, cy + 5.0, 16.0, back_color);
        draw_text("X", cx + 8.0, cy + 5.0, 16.0, exit_color);
    } else {
        // Simple center: just Exit
        let inner_fill = Color::from_rgba(30, 32, 38, 255);
        draw_poly_aligned(cx, cy, config.inner_radius, 16, inner_fill, config.border_color, 1.0);

        let cancel_color = if in_center {
            Color::from_rgba(255, 150, 150, 255)
        } else {
            Color::from_rgba(150, 150, 150, 255)
        };
        draw_text("X", cx - 6.0, cy + 6.0, 20.0, cancel_color);
    }

    // Handle click to select
    if is_mouse_button_pressed(MouseButton::Left) {
        // Click outside = cancel
        if dist > config.outer_radius * 1.2 {
            state.close(false);
        } else if let Some(idx) = state.highlighted {
            if let Some(item) = state.items.get(idx) {
                if item.enabled {
                    if !item.children.is_empty() {
                        // Enter submenu
                        state.enter_submenu(idx);
                    } else {
                        // Select item
                        return state.close(true);
                    }
                }
            }
        } else if in_center {
            // Clicked in center
            if in_submenu && mouse_on_left {
                // Back to parent menu
                state.back();
            } else {
                // Exit/cancel
                state.close(false);
            }
        }
    }

    None
}

/// Draw a filled pie segment
fn draw_segment(
    cx: f32, cy: f32,
    inner_r: f32, outer_r: f32,
    start_angle: f32, end_angle: f32,
    color: Color,
) {
    // Approximate with triangles
    let steps = 16;
    let angle_step = (end_angle - start_angle) / steps as f32;

    for i in 0..steps {
        let a1 = start_angle + i as f32 * angle_step;
        let a2 = start_angle + (i + 1) as f32 * angle_step;

        // Outer edge points
        let ox1 = cx + a1.cos() * outer_r;
        let oy1 = cy + a1.sin() * outer_r;
        let ox2 = cx + a2.cos() * outer_r;
        let oy2 = cy + a2.sin() * outer_r;

        // Inner edge points
        let ix1 = cx + a1.cos() * inner_r;
        let iy1 = cy + a1.sin() * inner_r;
        let ix2 = cx + a2.cos() * inner_r;
        let iy2 = cy + a2.sin() * inner_r;

        // Draw two triangles to form the segment slice
        draw_triangle(
            Vec2::new(ix1, iy1),
            Vec2::new(ox1, oy1),
            Vec2::new(ox2, oy2),
            color,
        );
        draw_triangle(
            Vec2::new(ix1, iy1),
            Vec2::new(ox2, oy2),
            Vec2::new(ix2, iy2),
            color,
        );
    }
}

/// Draw a polygon (fill + outline) using same vertices for perfect alignment
/// Keeps that 90s polygon aesthetic
fn draw_poly_aligned(cx: f32, cy: f32, radius: f32, sides: usize, fill: Color, outline: Color, thickness: f32) {
    let angle_step = (PI * 2.0) / sides as f32;

    // Pre-compute vertices once
    let verts: Vec<Vec2> = (0..sides)
        .map(|i| {
            let angle = -PI * 0.5 + i as f32 * angle_step; // Start at top
            Vec2::new(cx + angle.cos() * radius, cy + angle.sin() * radius)
        })
        .collect();

    // Draw filled triangles (fan from center)
    for i in 0..sides {
        let next = (i + 1) % sides;
        draw_triangle(
            Vec2::new(cx, cy),
            verts[i],
            verts[next],
            fill,
        );
    }

    // Draw outline using exact same vertices
    for i in 0..sides {
        let next = (i + 1) % sides;
        draw_line(verts[i].x, verts[i].y, verts[next].x, verts[next].y, thickness, outline);
    }
}

/// Draw just the outline of a polygon (no fill)
fn draw_poly_outline(cx: f32, cy: f32, radius: f32, sides: usize, color: Color, thickness: f32) {
    let angle_step = (PI * 2.0) / sides as f32;
    for i in 0..sides {
        let a1 = -PI * 0.5 + i as f32 * angle_step;
        let a2 = -PI * 0.5 + ((i + 1) % sides) as f32 * angle_step;
        let x1 = cx + a1.cos() * radius;
        let y1 = cy + a1.sin() * radius;
        let x2 = cx + a2.cos() * radius;
        let y2 = cy + a2.sin() * radius;
        draw_line(x1, y1, x2, y2, thickness, color);
    }
}

/// Draw a half circle (left or right half)
fn draw_half_circle(cx: f32, cy: f32, radius: f32, left_half: bool, color: Color) {
    let steps = 8;
    let start_angle = if left_half { PI * 0.5 } else { -PI * 0.5 };
    let angle_step = PI / steps as f32;

    for i in 0..steps {
        let a1 = start_angle + i as f32 * angle_step;
        let a2 = start_angle + (i + 1) as f32 * angle_step;
        draw_triangle(
            Vec2::new(cx, cy),
            Vec2::new(cx + a1.cos() * radius, cy + a1.sin() * radius),
            Vec2::new(cx + a2.cos() * radius, cy + a2.sin() * radius),
            color,
        );
    }
}

/// Build context-sensitive menu items based on current selection
pub fn build_context_items(
    has_vertex_selection: bool,
    has_face_selection: bool,
    has_edge_selection: bool,
    bone_names: &[String],
) -> Vec<RadialMenuItem> {
    let mut items = Vec::new();

    if has_vertex_selection && !bone_names.is_empty() {
        // Vertex mode with skeleton - show bone assignment
        let bone_items: Vec<RadialMenuItem> = bone_names
            .iter()
            .enumerate()
            .map(|(i, name)| RadialMenuItem::new(format!("bone_{}", i), name.clone()).with_icon('◆'))
            .collect();

        items.push(
            RadialMenuItem::new("assign_bone", "Assign to Bone")
                .with_icon('🦴')
                .with_children(bone_items)
        );
        items.push(RadialMenuItem::new("unbind", "Unbind").with_icon('✕'));
    }

    if has_vertex_selection {
        items.push(RadialMenuItem::new("merge", "Merge").with_icon('⊕'));
        items.push(RadialMenuItem::new("split", "Split").with_icon('✂'));
    }

    if has_face_selection {
        items.push(RadialMenuItem::new("extrude", "Extrude").with_icon('↑'));
        items.push(RadialMenuItem::new("inset", "Inset").with_icon('◇'));
        items.push(RadialMenuItem::new("flip", "Flip Normal").with_icon('↕'));
    }

    // Always show primitives option
    let primitives = vec![
        RadialMenuItem::new("prim_cube", "Cube").with_icon('□'),
        RadialMenuItem::new("prim_plane", "Plane").with_icon('▬'),
        RadialMenuItem::new("prim_cylinder", "Cylinder").with_icon('○'),
        RadialMenuItem::new("prim_prism", "Prism").with_icon('△'),
    ];
    items.push(
        RadialMenuItem::new("add_primitive", "Add Mesh")
            .with_icon('+')
            .with_children(primitives)
    );

    items
}
