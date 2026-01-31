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

    // Draw background circle
    draw_circle(cx, cy, config.outer_radius, config.bg_color);
    draw_circle_lines(cx, cy, config.outer_radius, 2.0, config.border_color);

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

        // Draw icon if present
        let text_color = if item.enabled { config.text_color } else { config.disabled_color };
        if let Some(icon) = item.icon {
            let icon_str = icon.to_string();
            let icon_size = 16.0;
            draw_text(&icon_str, label_x - 4.0, label_y - 8.0, icon_size, text_color);
        }

        // Draw label
        let font_size = if is_highlighted { 14.0 } else { 12.0 };
        let label_offset_y = if item.icon.is_some() { 8.0 } else { 4.0 };

        // Center the text roughly
        let text_width = item.label.len() as f32 * font_size * 0.4;
        draw_text(
            &item.label,
            label_x - text_width * 0.5,
            label_y + label_offset_y,
            font_size,
            text_color,
        );

        // Draw arrow if has children
        if !item.children.is_empty() {
            let arrow_x = cx + mid_angle.cos() * (config.outer_radius - 12.0);
            let arrow_y = cy + mid_angle.sin() * (config.outer_radius - 12.0);
            draw_text("›", arrow_x, arrow_y + 4.0, 14.0, text_color);
        }
    }

    // Draw inner circle (cancel zone)
    draw_circle(cx, cy, config.inner_radius, Color::from_rgba(30, 32, 38, 255));
    draw_circle_lines(cx, cy, config.inner_radius, 1.0, config.border_color);

    // Draw cancel text in center
    let cancel_color = if state.highlighted.is_none() {
        Color::from_rgba(255, 150, 150, 255)
    } else {
        Color::from_rgba(150, 150, 150, 255)
    };
    draw_text("×", cx - 6.0, cy + 6.0, 20.0, cancel_color);

    // Handle click to select
    if is_mouse_button_pressed(MouseButton::Left) {
        if let Some(idx) = state.highlighted {
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
        } else {
            // Clicked in center = cancel
            state.close(false);
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
