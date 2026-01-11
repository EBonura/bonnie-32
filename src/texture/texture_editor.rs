//! Unified texture editor component
//!
//! Provides a reusable UI component for editing indexed textures with:
//! - Canvas with zoom/pan
//! - Drawing tools (pencil, brush, fill, shapes)
//! - UV editing with vertex manipulation
//! - Palette editing with RGB555 sliders
//! - Undo/redo support

use macroquad::prelude::*;
use crate::rasterizer::{BlendMode, ClutDepth, Color15, Vec2 as RastVec2};
use crate::ui::{Rect, UiContext, icon};
use super::user_texture::UserTexture;

/// Editor mode - Paint or UV editing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextureEditorMode {
    /// Texture painting mode (default)
    #[default]
    Paint,
    /// UV coordinate editing mode
    Uv,
}

/// Modal transform for UV editing (G/T/R keys)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UvModalTransform {
    #[default]
    None,
    /// G key - move UV selection
    Grab,
    /// T key pressed - waiting for user to click and drag to scale
    ScalePending,
    /// T key - actively scaling UV selection (after click)
    Scale,
    /// R key - rotate UV selection
    Rotate,
}

/// Pending UV operation requested by button click
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UvOperation {
    /// Flip UVs horizontally around center
    FlipHorizontal,
    /// Flip UVs vertically around center
    FlipVertical,
    /// Rotate UVs 90 degrees clockwise
    RotateCW,
    /// Reset UVs to default positions
    ResetUVs,
}

// UI constants
const TEXT_COLOR: Color = Color::new(0.85, 0.85, 0.85, 1.0);
const TEXT_DIM: Color = Color::new(0.55, 0.55, 0.55, 1.0);
const ACCENT_COLOR: Color = Color::new(0.28, 0.51, 0.71, 1.0);
const PANEL_BG: Color = Color::new(0.18, 0.18, 0.20, 1.0);

/// Drawing tool types
/// Note: No eraser - paint with index 0 (transparent) to erase
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DrawTool {
    /// Selection tool for copy/paste/move
    Select,
    /// Brush with configurable size (size 1 = pencil)
    #[default]
    Brush,
    /// Flood fill
    Fill,
    /// Line drawing (thickness = brush_size)
    Line,
    /// Rectangle (outline or filled based on fill_shapes toggle)
    Rectangle,
    /// Ellipse (outline or filled based on fill_shapes toggle)
    Ellipse,
}

/// Brush shape for the brush tool
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrushShape {
    #[default]
    Square,
    Circle,
}

/// Selection rectangle with optional floating pixel data
#[derive(Debug, Clone)]
pub struct Selection {
    /// Top-left X coordinate in texture pixels
    pub x: i32,
    /// Top-left Y coordinate in texture pixels
    pub y: i32,
    /// Width in pixels
    pub width: usize,
    /// Height in pixels
    pub height: usize,
    /// Floating pixel data (lifted from canvas when moving)
    /// None = selection outline only, Some = floating pixels
    pub floating: Option<Vec<u8>>,
}

impl Selection {
    /// Create a new selection from two corner points
    pub fn from_corners(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        let (min_x, max_x) = if x0 < x1 { (x0, x1) } else { (x1, x0) };
        let (min_y, max_y) = if y0 < y1 { (y0, y1) } else { (y1, y0) };
        Self {
            x: min_x,
            y: min_y,
            width: (max_x - min_x + 1) as usize,
            height: (max_y - min_y + 1) as usize,
            floating: None,
        }
    }

    /// Check if a point is inside the selection
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x
            && px < self.x + self.width as i32
            && py >= self.y
            && py < self.y + self.height as i32
    }

    /// Get pixel index within the selection (for floating data)
    pub fn pixel_index(&self, px: i32, py: i32) -> Option<usize> {
        if self.contains(px, py) {
            let local_x = (px - self.x) as usize;
            let local_y = (py - self.y) as usize;
            Some(local_y * self.width + local_x)
        } else {
            None
        }
    }

    /// Check if a screen position is near a selection edge/corner
    /// Returns the edge if within threshold, None otherwise
    /// Uses screen coordinates (after zoom/pan transform)
    pub fn hit_test_edge(
        &self,
        screen_x: f32,
        screen_y: f32,
        tex_x: f32,
        tex_y: f32,
        zoom: f32,
        threshold: f32,
    ) -> Option<ResizeEdge> {
        let sel_x = tex_x + self.x as f32 * zoom;
        let sel_y = tex_y + self.y as f32 * zoom;
        let sel_w = self.width as f32 * zoom;
        let sel_h = self.height as f32 * zoom;

        let left = sel_x;
        let right = sel_x + sel_w;
        let top = sel_y;
        let bottom = sel_y + sel_h;

        let near_left = (screen_x - left).abs() < threshold;
        let near_right = (screen_x - right).abs() < threshold;
        let near_top = (screen_y - top).abs() < threshold;
        let near_bottom = (screen_y - bottom).abs() < threshold;

        let in_x_range = screen_x >= left - threshold && screen_x <= right + threshold;
        let in_y_range = screen_y >= top - threshold && screen_y <= bottom + threshold;

        // Corners take priority (check these first)
        if near_left && near_top {
            return Some(ResizeEdge::TopLeft);
        }
        if near_right && near_top {
            return Some(ResizeEdge::TopRight);
        }
        if near_left && near_bottom {
            return Some(ResizeEdge::BottomLeft);
        }
        if near_right && near_bottom {
            return Some(ResizeEdge::BottomRight);
        }

        // Then edges
        if near_top && in_x_range {
            return Some(ResizeEdge::Top);
        }
        if near_bottom && in_x_range {
            return Some(ResizeEdge::Bottom);
        }
        if near_left && in_y_range {
            return Some(ResizeEdge::Left);
        }
        if near_right && in_y_range {
            return Some(ResizeEdge::Right);
        }

        None
    }
}

/// Clipboard data for copy/paste
#[derive(Debug, Clone)]
pub struct ClipboardData {
    /// Width of clipboard content
    pub width: usize,
    /// Height of clipboard content
    pub height: usize,
    /// Pixel indices
    pub indices: Vec<u8>,
}

impl DrawTool {
    /// Icon for this tool
    pub fn icon(&self) -> char {
        match self {
            DrawTool::Select => icon::POINTER,         // selection tool
            DrawTool::Brush => icon::PENCIL,           // pencil icon (size 1 = pixel, size 2+ = brush)
            DrawTool::Fill => icon::PAINT_BUCKET,
            DrawTool::Line => icon::PENCIL_LINE,       // pencil-line icon
            DrawTool::Rectangle => icon::RECTANGLE_HORIZONTAL,
            DrawTool::Ellipse => icon::CIRCLE,
        }
    }

    /// Tooltip text
    pub fn tooltip(&self) -> &'static str {
        match self {
            DrawTool::Select => "Select (S)",
            DrawTool::Brush => "Brush (B)",
            DrawTool::Fill => "Fill (F)",
            DrawTool::Line => "Line (L)",
            DrawTool::Rectangle => "Rectangle (R)",
            DrawTool::Ellipse => "Ellipse (O)",
        }
    }

    /// Whether this tool uses brush size
    pub fn uses_brush_size(&self) -> bool {
        matches!(self, DrawTool::Brush | DrawTool::Line | DrawTool::Rectangle | DrawTool::Ellipse)
    }

    /// Whether this tool is a shape tool (requires start/end points)
    pub fn is_shape_tool(&self) -> bool {
        matches!(self, DrawTool::Line | DrawTool::Rectangle | DrawTool::Ellipse)
    }
}

/// Undo entry for texture editing
#[derive(Debug, Clone)]
pub struct TextureUndoEntry {
    /// Description of the change
    pub description: String,
    /// Snapshot of indices before the change
    pub indices: Vec<u8>,
    /// Snapshot of palette before the change
    pub palette: Vec<Color15>,
}

/// UV vertex data for overlay rendering
#[derive(Debug, Clone, Copy)]
pub struct UvVertex {
    /// UV coordinate (0.0-1.0 range)
    pub uv: RastVec2,
    /// Global vertex index (for selection tracking)
    pub vertex_index: usize,
}

/// Face data for UV overlay
#[derive(Debug, Clone)]
pub struct UvFace {
    /// Indices into the UvVertex array
    pub vertex_indices: Vec<usize>,
}

/// UV overlay data passed to the texture canvas for rendering
#[derive(Debug, Clone)]
pub struct UvOverlayData {
    /// All UV vertices
    pub vertices: Vec<UvVertex>,
    /// Faces referencing vertices
    pub faces: Vec<UvFace>,
    /// Which faces are selected (indices into faces array)
    pub selected_faces: Vec<usize>,
}

/// State for the texture editor
#[derive(Debug)]
pub struct TextureEditorState {
    /// Currently selected drawing tool
    pub tool: DrawTool,
    /// Brush size (1-16 pixels)
    pub brush_size: u8,
    /// Brush shape (square or circle)
    pub brush_shape: BrushShape,
    /// Currently selected palette index for drawing
    pub selected_index: u8,
    /// Currently selected palette entry for editing
    pub editing_index: usize,
    /// Zoom level (1.0 = 1:1, 2.0 = 2x, etc.)
    pub zoom: f32,
    /// Pan offset in canvas space
    pub pan_x: f32,
    pub pan_y: f32,
    /// Is currently drawing (mouse down)
    pub drawing: bool,
    /// Shape tool start position (pixel coords)
    pub shape_start: Option<(i32, i32)>,
    /// Last drawn position (for line interpolation)
    pub last_draw_pos: Option<(i32, i32)>,
    /// Undo stack
    pub undo_stack: Vec<TextureUndoEntry>,
    /// Redo stack
    pub redo_stack: Vec<TextureUndoEntry>,
    /// Maximum undo entries
    pub max_undo: usize,
    /// Active color slider (0=R, 1=G, 2=B)
    pub color_slider: Option<usize>,
    /// Is dragging brush size slider
    pub brush_slider_active: bool,
    /// Is panning the canvas
    pub panning: bool,
    /// Pan start position
    pub pan_start: (f32, f32),
    /// Pan start offset
    pub pan_start_offset: (f32, f32),
    /// Whether texture has unsaved changes
    pub dirty: bool,
    /// Undo requested via button
    pub undo_requested: bool,
    /// Redo requested via button
    pub redo_requested: bool,
    /// Fill shapes (Rectangle/Ellipse) instead of outline
    pub fill_shapes: bool,
    /// Show pixel grid overlay
    pub show_grid: bool,
    /// Show tiling preview (8 copies around center for seamless texture editing)
    pub show_tiling: bool,
    /// Current selection (None = no selection)
    pub selection: Option<Selection>,
    /// Clipboard for copy/paste
    pub clipboard: Option<ClipboardData>,
    /// Selection drag start position (for creating/moving selection)
    pub selection_drag_start: Option<(i32, i32)>,
    /// Whether we're creating a new selection (vs moving existing)
    pub creating_selection: bool,
    /// Animation frame counter for marching ants
    pub selection_anim_frame: u32,
    /// Which edge/corner is being resized (None = not resizing)
    pub resizing_edge: Option<ResizeEdge>,
    /// Status message to show (forwarded to main editor)
    pub status_message: Option<String>,
    /// Original selection position before move (for cancel)
    pub move_original_pos: Option<(i32, i32)>,
    /// Signal to caller that undo should be saved (description of the action)
    /// Caller should check this after draw_texture_canvas and call global save_texture_undo
    pub undo_save_pending: Option<String>,
    /// Blend mode dropdown is open
    pub blend_dropdown_open: bool,
    /// Palette generator: 3 key colors for ramp generation
    pub palette_gen_colors: [(u8, u8, u8); 3],
    /// Palette generator: brightness range (0.3 = subtle, 1.0 = full range)
    pub palette_gen_brightness: f32,
    /// Palette generator: hue shift per step in degrees (0 = monochrome, 10-20 = natural)
    pub palette_gen_hue_shift: f32,
    /// Which palette generator color is being edited (0-2), None if not editing
    pub palette_gen_editing: Option<usize>,

    // === UV Editing State ===
    /// Current editor mode (Paint or UV)
    pub mode: TextureEditorMode,
    /// Selected UV vertex indices (indices into the vertices array)
    pub uv_selection: Vec<usize>,
    /// Is currently dragging UV vertices
    pub uv_drag_active: bool,
    /// Drag start position in screen coords
    pub uv_drag_start: (f32, f32),
    /// Original UV positions when drag started: (object_idx, vertex_idx, original_uv)
    pub uv_drag_start_uvs: Vec<(usize, usize, RastVec2)>,
    /// Start of box selection in screen coords
    pub uv_box_select_start: Option<(f32, f32)>,
    /// Current modal transform mode (G/S/R)
    pub uv_modal_transform: UvModalTransform,
    /// Mouse position when modal transform started
    pub uv_modal_start_mouse: (f32, f32),
    /// Original UV positions when modal transform started: (vertex_idx, original_uv)
    pub uv_modal_start_uvs: Vec<(usize, RastVec2)>,
    /// Center of UV selection for rotation/scale operations
    pub uv_modal_center: RastVec2,
    /// Pending UV operation (flip/rotate/reset) requested by button
    pub uv_operation_pending: Option<UvOperation>,
}

/// Edge or corner being resized
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeEdge {
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Default for TextureEditorState {
    fn default() -> Self {
        Self {
            tool: DrawTool::Brush,
            brush_size: 3,
            brush_shape: BrushShape::Square,
            selected_index: 1, // Default to first non-transparent color
            editing_index: 1,
            zoom: 4.0, // Start at 4x zoom
            pan_x: 0.0,
            pan_y: 0.0,
            drawing: false,
            shape_start: None,
            last_draw_pos: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_undo: 50,
            color_slider: None,
            brush_slider_active: false,
            panning: false,
            pan_start: (0.0, 0.0),
            pan_start_offset: (0.0, 0.0),
            dirty: false,
            undo_requested: false,
            redo_requested: false,
            fill_shapes: false,
            show_grid: true, // Grid on by default
            show_tiling: false, // Tiling preview off by default
            selection: None,
            clipboard: None,
            selection_drag_start: None,
            creating_selection: false,
            selection_anim_frame: 0,
            resizing_edge: None,
            status_message: None,
            move_original_pos: None,
            undo_save_pending: None,
            blend_dropdown_open: false,
            // Palette generator defaults: warm skin, cool blue, earthy green
            palette_gen_colors: [(24, 16, 12), (8, 12, 20), (12, 18, 8)],
            palette_gen_brightness: 0.7,
            palette_gen_hue_shift: 10.0,
            palette_gen_editing: None,
            // UV editing state
            mode: TextureEditorMode::Paint,
            uv_selection: Vec::new(),
            uv_drag_active: false,
            uv_drag_start: (0.0, 0.0),
            uv_drag_start_uvs: Vec::new(),
            uv_box_select_start: None,
            uv_modal_transform: UvModalTransform::None,
            uv_modal_start_mouse: (0.0, 0.0),
            uv_modal_start_uvs: Vec::new(),
            uv_modal_center: RastVec2::new(0.0, 0.0),
            uv_operation_pending: None,
        }
    }
}

impl TextureEditorState {
    /// Create a new editor state
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a status message to be forwarded to the main editor
    pub fn set_status(&mut self, message: &str) {
        self.status_message = Some(message.to_string());
    }

    /// Take the status message (clears it)
    pub fn take_status(&mut self) -> Option<String> {
        self.status_message.take()
    }

    /// Reset the editor state for a new texture
    pub fn reset(&mut self) {
        self.tool = DrawTool::Brush;
        self.brush_size = 3;
        self.brush_shape = BrushShape::Square;
        self.selected_index = 1;
        self.editing_index = 1;
        self.zoom = 4.0;
        self.pan_x = 0.0;
        self.pan_y = 0.0;
        self.drawing = false;
        self.shape_start = None;
        self.last_draw_pos = None;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.color_slider = None;
        self.brush_slider_active = false;
        self.panning = false;
        self.dirty = false;
        self.selection = None;
        self.selection_drag_start = None;
        self.creating_selection = false;
        self.palette_gen_editing = None;
        // UV state reset
        self.mode = TextureEditorMode::Paint;
        self.uv_selection.clear();
        self.uv_drag_active = false;
        self.uv_box_select_start = None;
        self.uv_modal_transform = UvModalTransform::None;
        self.uv_modal_start_uvs.clear();
        // Note: clipboard and palette_gen_colors are NOT reset - allow reuse across textures
    }

    /// Reset zoom and pan to fit texture in view
    pub fn reset_view(&mut self, tex_width: usize, tex_height: usize, view_width: f32, view_height: f32) {
        // Calculate zoom to fit texture with some padding
        let padding = 20.0;
        let available_w = view_width - padding * 2.0;
        let available_h = view_height - padding * 2.0;

        let zoom_x = available_w / tex_width as f32;
        let zoom_y = available_h / tex_height as f32;
        self.zoom = zoom_x.min(zoom_y).max(1.0).min(16.0);

        // Center the texture
        self.pan_x = 0.0;
        self.pan_y = 0.0;
    }

    /// Save current state for undo
    pub fn save_undo(&mut self, texture: &UserTexture, description: &str) {
        // Clear redo stack on new edit
        self.redo_stack.clear();

        // Save current state
        self.undo_stack.push(TextureUndoEntry {
            description: description.to_string(),
            indices: texture.indices.clone(),
            palette: texture.palette.clone(),
        });

        // Limit undo stack size
        while self.undo_stack.len() > self.max_undo {
            self.undo_stack.remove(0);
        }

        self.dirty = true;
    }

    /// Undo last change
    pub fn undo(&mut self, texture: &mut UserTexture) -> bool {
        if let Some(entry) = self.undo_stack.pop() {
            // Save current state to redo
            self.redo_stack.push(TextureUndoEntry {
                description: entry.description.clone(),
                indices: texture.indices.clone(),
                palette: texture.palette.clone(),
            });

            // Restore previous state
            texture.indices = entry.indices;
            texture.palette = entry.palette;
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Redo last undone change
    pub fn redo(&mut self, texture: &mut UserTexture) -> bool {
        if let Some(entry) = self.redo_stack.pop() {
            // Save current state to undo
            self.undo_stack.push(TextureUndoEntry {
                description: entry.description.clone(),
                indices: texture.indices.clone(),
                palette: texture.palette.clone(),
            });

            // Apply redo state
            texture.indices = entry.indices;
            texture.palette = entry.palette;
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }
}

/// Draw a pixel on the texture
fn tex_draw_pixel(texture: &mut UserTexture, x: i32, y: i32, index: u8) {
    if x >= 0 && y >= 0 && (x as usize) < texture.width && (y as usize) < texture.height {
        texture.set_index(x as usize, y as usize, index);
    }
}

/// Draw a line using Bresenham's algorithm
fn tex_draw_line(texture: &mut UserTexture, x0: i32, y0: i32, x1: i32, y1: i32, index: u8) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    let mut x = x0;
    let mut y = y0;

    loop {
        tex_draw_pixel(texture, x, y, index);
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// Draw a thick line using square brush at each point
fn tex_draw_line_thick(texture: &mut UserTexture, x0: i32, y0: i32, x1: i32, y1: i32, thickness: u8, index: u8) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    let mut x = x0;
    let mut y = y0;

    loop {
        // Draw square brush at each point
        tex_draw_brush_square(texture, x, y, thickness, index);
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// Draw a brush stroke (square brush)
fn tex_draw_brush_square(texture: &mut UserTexture, cx: i32, cy: i32, size: u8, index: u8) {
    let half = (size as i32 - 1) / 2;
    for dy in 0..size as i32 {
        for dx in 0..size as i32 {
            tex_draw_pixel(texture, cx - half + dx, cy - half + dy, index);
        }
    }
}

/// Draw a brush stroke (circle brush)
fn tex_draw_brush_circle(texture: &mut UserTexture, cx: i32, cy: i32, size: u8, index: u8) {
    let r = (size as i32 - 1) / 2;
    // For size 1, just draw a single pixel
    if r == 0 {
        tex_draw_pixel(texture, cx, cy, index);
        return;
    }
    // For larger sizes, draw filled circle
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                tex_draw_pixel(texture, cx + dx, cy + dy, index);
            }
        }
    }
}

/// Draw a brush stroke with the current shape
fn tex_draw_brush(texture: &mut UserTexture, cx: i32, cy: i32, size: u8, index: u8, shape: BrushShape) {
    match shape {
        BrushShape::Square => tex_draw_brush_square(texture, cx, cy, size, index),
        BrushShape::Circle => tex_draw_brush_circle(texture, cx, cy, size, index),
    }
}

/// Flood fill using scanline algorithm
fn flood_fill(texture: &mut UserTexture, start_x: i32, start_y: i32, fill_index: u8) {
    if start_x < 0 || start_y < 0 {
        return;
    }
    let x = start_x as usize;
    let y = start_y as usize;
    if x >= texture.width || y >= texture.height {
        return;
    }

    let target_index = texture.get_index(x, y);
    if target_index == fill_index {
        return; // Already filled
    }

    let mut stack = vec![(x, y)];
    while let Some((cx, cy)) = stack.pop() {
        if cx >= texture.width || cy >= texture.height {
            continue;
        }
        if texture.get_index(cx, cy) != target_index {
            continue;
        }

        texture.set_index(cx, cy, fill_index);

        if cx > 0 {
            stack.push((cx - 1, cy));
        }
        if cx + 1 < texture.width {
            stack.push((cx + 1, cy));
        }
        if cy > 0 {
            stack.push((cx, cy - 1));
        }
        if cy + 1 < texture.height {
            stack.push((cx, cy + 1));
        }
    }
}

/// Draw a rectangle outline on texture
fn tex_draw_rect_outline(texture: &mut UserTexture, x0: i32, y0: i32, x1: i32, y1: i32, index: u8) {
    let (min_x, max_x) = if x0 < x1 { (x0, x1) } else { (x1, x0) };
    let (min_y, max_y) = if y0 < y1 { (y0, y1) } else { (y1, y0) };

    // Top and bottom edges
    for x in min_x..=max_x {
        tex_draw_pixel(texture, x, min_y, index);
        tex_draw_pixel(texture, x, max_y, index);
    }
    // Left and right edges
    for y in min_y..=max_y {
        tex_draw_pixel(texture, min_x, y, index);
        tex_draw_pixel(texture, max_x, y, index);
    }
}

/// Draw a filled rectangle on texture
fn tex_draw_rect_filled(texture: &mut UserTexture, x0: i32, y0: i32, x1: i32, y1: i32, index: u8) {
    let (min_x, max_x) = if x0 < x1 { (x0, x1) } else { (x1, x0) };
    let (min_y, max_y) = if y0 < y1 { (y0, y1) } else { (y1, y0) };

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            tex_draw_pixel(texture, x, y, index);
        }
    }
}

/// Draw an ellipse outline on texture
fn tex_draw_ellipse_outline(texture: &mut UserTexture, x0: i32, y0: i32, x1: i32, y1: i32, index: u8) {
    let cx = (x0 + x1) / 2;
    let cy = (y0 + y1) / 2;
    let rx = ((x1 - x0).abs() / 2).max(1);
    let ry = ((y1 - y0).abs() / 2).max(1);

    // Simple approximation using angle stepping
    let steps = (rx + ry).max(8) * 4;
    let mut last_x = cx + rx;
    let mut last_y = cy;

    for i in 1..=steps {
        let angle = 2.0 * std::f32::consts::PI * (i as f32 / steps as f32);
        let px = cx + (rx as f32 * angle.cos()) as i32;
        let py = cy + (ry as f32 * angle.sin()) as i32;
        tex_draw_line(texture, last_x, last_y, px, py, index);
        last_x = px;
        last_y = py;
    }
}

/// Draw a filled ellipse on texture
fn tex_draw_ellipse_filled(texture: &mut UserTexture, x0: i32, y0: i32, x1: i32, y1: i32, index: u8) {
    let cx = (x0 + x1) / 2;
    let cy = (y0 + y1) / 2;
    let rx = ((x1 - x0).abs() / 2).max(1);
    let ry = ((y1 - y0).abs() / 2).max(1);

    // Scan each row
    for y in (cy - ry)..=(cy + ry) {
        let dy = (y - cy) as f32 / ry as f32;
        if dy.abs() <= 1.0 {
            let dx = (1.0 - dy * dy).sqrt();
            let x_span = (rx as f32 * dx) as i32;
            for x in (cx - x_span)..=(cx + x_span) {
                tex_draw_pixel(texture, x, y, index);
            }
        }
    }
}

// ============================================================================
// Shape Preview Functions (for visual feedback while drawing)
// ============================================================================

/// Draw a single preview pixel at screen coordinates
fn draw_preview_pixel(tex_x: f32, tex_y: f32, px: i32, py: i32, pixel_size: f32, color: Color) {
    draw_rectangle(
        tex_x + px as f32 * pixel_size,
        tex_y + py as f32 * pixel_size,
        pixel_size,
        pixel_size,
        color,
    );
}

/// Draw thick line preview using square brush at each point
fn draw_line_preview(tex_x: f32, tex_y: f32, x0: i32, y0: i32, x1: i32, y1: i32, thickness: u8, pixel_size: f32, color: Color) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    let mut x = x0;
    let mut y = y0;
    let half = (thickness as i32 - 1) / 2;

    loop {
        // Draw square brush preview at each point
        for by in 0..thickness as i32 {
            for bx in 0..thickness as i32 {
                draw_preview_pixel(tex_x, tex_y, x - half + bx, y - half + by, pixel_size, color);
            }
        }
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// Draw rectangle outline preview
fn draw_rect_outline_preview(tex_x: f32, tex_y: f32, x0: i32, y0: i32, x1: i32, y1: i32, pixel_size: f32, color: Color) {
    let min_x = x0.min(x1);
    let max_x = x0.max(x1);
    let min_y = y0.min(y1);
    let max_y = y0.max(y1);

    // Top and bottom edges
    for x in min_x..=max_x {
        draw_preview_pixel(tex_x, tex_y, x, min_y, pixel_size, color);
        draw_preview_pixel(tex_x, tex_y, x, max_y, pixel_size, color);
    }
    // Left and right edges (excluding corners already drawn)
    for y in (min_y + 1)..max_y {
        draw_preview_pixel(tex_x, tex_y, min_x, y, pixel_size, color);
        draw_preview_pixel(tex_x, tex_y, max_x, y, pixel_size, color);
    }
}

/// Draw filled rectangle preview
fn draw_rect_filled_preview(tex_x: f32, tex_y: f32, x0: i32, y0: i32, x1: i32, y1: i32, pixel_size: f32, color: Color) {
    let min_x = x0.min(x1);
    let max_x = x0.max(x1);
    let min_y = y0.min(y1);
    let max_y = y0.max(y1);

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            draw_preview_pixel(tex_x, tex_y, x, y, pixel_size, color);
        }
    }
}

/// Draw ellipse outline preview using midpoint algorithm
fn draw_ellipse_outline_preview(tex_x: f32, tex_y: f32, x0: i32, y0: i32, x1: i32, y1: i32, pixel_size: f32, color: Color) {
    let cx = (x0 + x1) / 2;
    let cy = (y0 + y1) / 2;
    let rx = ((x1 - x0).abs() / 2).max(1);
    let ry = ((y1 - y0).abs() / 2).max(1);

    // Sample points around the ellipse
    let steps = ((rx + ry) * 4).max(32) as usize;
    for i in 0..steps {
        let t = (i as f32 / steps as f32) * std::f32::consts::TAU;
        let x = cx + (rx as f32 * t.cos()).round() as i32;
        let y = cy + (ry as f32 * t.sin()).round() as i32;
        draw_preview_pixel(tex_x, tex_y, x, y, pixel_size, color);
    }
}

/// Draw filled ellipse preview
fn draw_ellipse_filled_preview(tex_x: f32, tex_y: f32, x0: i32, y0: i32, x1: i32, y1: i32, pixel_size: f32, color: Color) {
    let cx = (x0 + x1) / 2;
    let cy = (y0 + y1) / 2;
    let rx = ((x1 - x0).abs() / 2).max(1);
    let ry = ((y1 - y0).abs() / 2).max(1);

    // Scan each row
    for y in (cy - ry)..=(cy + ry) {
        let dy = (y - cy) as f32 / ry as f32;
        if dy.abs() <= 1.0 {
            let dx = (1.0 - dy * dy).sqrt();
            let x_span = (rx as f32 * dx) as i32;
            for x in (cx - x_span)..=(cx + x_span) {
                draw_preview_pixel(tex_x, tex_y, x, y, pixel_size, color);
            }
        }
    }
}

/// Draw marching ants selection border
fn draw_selection_marching_ants(selection: &Selection, tex_x: f32, tex_y: f32, zoom: f32, frame: u32) {
    let x = tex_x + selection.x as f32 * zoom;
    let y = tex_y + selection.y as f32 * zoom;
    let w = selection.width as f32 * zoom;
    let h = selection.height as f32 * zoom;

    // Marching ants effect: alternate black and white dashes that move over time
    let dash_len = 4.0;
    let offset = (frame / 12) as f32 * 2.0; // Slow animation speed (divide by 12 for ~5 FPS)

    // Draw dashed lines for all four edges
    draw_marching_line(x, y, x + w, y, dash_len, offset);           // Top
    draw_marching_line(x + w, y, x + w, y + h, dash_len, offset);   // Right
    draw_marching_line(x + w, y + h, x, y + h, dash_len, offset);   // Bottom
    draw_marching_line(x, y + h, x, y, dash_len, offset);           // Left
}

/// Draw a single marching ants line segment
fn draw_marching_line(x0: f32, y0: f32, x1: f32, y1: f32, dash_len: f32, offset: f32) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let length = (dx * dx + dy * dy).sqrt();
    if length < 0.1 {
        return;
    }

    let nx = dx / length;
    let ny = dy / length;

    // Draw alternating black and white segments
    let mut pos = 0.0;
    let mut is_white = ((offset / dash_len) as i32 % 2) == 0;

    // Adjust start position based on offset for animation
    let adjusted_offset = offset % (dash_len * 2.0);
    pos -= adjusted_offset;

    while pos < length {
        let seg_start = pos.max(0.0);
        let seg_end = (pos + dash_len).min(length);

        if seg_end > seg_start {
            let sx = x0 + nx * seg_start;
            let sy = y0 + ny * seg_start;
            let ex = x0 + nx * seg_end;
            let ey = y0 + ny * seg_end;

            let color = if is_white { WHITE } else { BLACK };
            draw_line(sx, sy, ex, ey, 1.0, color);
        }

        pos += dash_len;
        is_white = !is_white;
    }
}

/// Draw resize handles on selection edges/corners
fn draw_selection_handles(
    selection: &Selection,
    tex_x: f32,
    tex_y: f32,
    zoom: f32,
    hovered_edge: Option<ResizeEdge>,
) {
    let x = tex_x + selection.x as f32 * zoom;
    let y = tex_y + selection.y as f32 * zoom;
    let w = selection.width as f32 * zoom;
    let h = selection.height as f32 * zoom;

    let handle_size = 6.0;
    let half = handle_size / 2.0;

    // Define handle positions: (x, y, edge)
    let handles = [
        (x - half, y - half, ResizeEdge::TopLeft),
        (x + w / 2.0 - half, y - half, ResizeEdge::Top),
        (x + w - half, y - half, ResizeEdge::TopRight),
        (x + w - half, y + h / 2.0 - half, ResizeEdge::Right),
        (x + w - half, y + h - half, ResizeEdge::BottomRight),
        (x + w / 2.0 - half, y + h - half, ResizeEdge::Bottom),
        (x - half, y + h - half, ResizeEdge::BottomLeft),
        (x - half, y + h / 2.0 - half, ResizeEdge::Left),
    ];

    for (hx, hy, edge) in handles {
        let is_hovered = hovered_edge == Some(edge);
        let fill_color = if is_hovered {
            Color::new(1.0, 0.8, 0.2, 1.0) // Gold when hovered
        } else {
            WHITE
        };
        let border_color = BLACK;

        // Draw filled square with border
        draw_rectangle(hx, hy, handle_size, handle_size, fill_color);
        draw_rectangle_lines(hx, hy, handle_size, handle_size, 1.0, border_color);
    }
}

/// Copy selection pixels to clipboard (returns ClipboardData to avoid borrow issues)
fn make_clipboard_from_selection(texture: &UserTexture, selection: &Selection) -> ClipboardData {
    let mut indices = Vec::with_capacity(selection.width * selection.height);

    // If we have floating data, use that; otherwise read from texture
    if let Some(ref floating) = selection.floating {
        indices = floating.clone();
    } else {
        for y in 0..selection.height {
            for x in 0..selection.width {
                let tx = selection.x + x as i32;
                let ty = selection.y + y as i32;
                if tx >= 0 && ty >= 0 && (tx as usize) < texture.width && (ty as usize) < texture.height {
                    indices.push(texture.get_index(tx as usize, ty as usize));
                } else {
                    indices.push(0); // Transparent for out-of-bounds
                }
            }
        }
    }

    ClipboardData {
        width: selection.width,
        height: selection.height,
        indices,
    }
}

/// Clear the selection area (fill with index 0 = transparent)
fn clear_selection_area(texture: &mut UserTexture, selection: &Selection) {
    for y in 0..selection.height {
        for x in 0..selection.width {
            let tx = selection.x + x as i32;
            let ty = selection.y + y as i32;
            if tx >= 0 && ty >= 0 && (tx as usize) < texture.width && (ty as usize) < texture.height {
                texture.set_index(tx as usize, ty as usize, 0);
            }
        }
    }
}

/// Lift selection pixels into floating data (removes from texture)
fn lift_selection_to_floating(texture: &mut UserTexture, state: &mut TextureEditorState) {
    // Check if already floating
    if let Some(ref selection) = state.selection {
        if selection.floating.is_some() {
            return; // Already floating
        }
    } else {
        return; // No selection
    }

    // Signal to caller to save undo before lifting
    state.undo_save_pending = Some("Move selection".to_string());

    // Now do the lift
    if let Some(ref mut selection) = state.selection {
        let mut floating = Vec::with_capacity(selection.width * selection.height);

        for y in 0..selection.height {
            for x in 0..selection.width {
                let tx = selection.x + x as i32;
                let ty = selection.y + y as i32;
                if tx >= 0 && ty >= 0 && (tx as usize) < texture.width && (ty as usize) < texture.height {
                    let idx = texture.get_index(tx as usize, ty as usize);
                    floating.push(idx);
                    // Clear the pixel from the texture
                    texture.set_index(tx as usize, ty as usize, 0);
                } else {
                    floating.push(0);
                }
            }
        }

        selection.floating = Some(floating);
    }
}

/// Commit floating selection back to texture
fn commit_floating_selection(texture: &mut UserTexture, state: &mut TextureEditorState) {
    if let Some(ref selection) = state.selection {
        if let Some(ref floating) = selection.floating {
            // Draw floating pixels onto texture
            for y in 0..selection.height {
                for x in 0..selection.width {
                    let tx = selection.x + x as i32;
                    let ty = selection.y + y as i32;
                    let idx = floating[y * selection.width + x];

                    // Only draw non-transparent pixels
                    if idx != 0 && tx >= 0 && ty >= 0 && (tx as usize) < texture.width && (ty as usize) < texture.height {
                        texture.set_index(tx as usize, ty as usize, idx);
                    }
                }
            }
        }
    }
    // Clear the selection
    state.selection = None;
}

/// Draw the mode tabs (Paint / UV) at the top of the editor
/// Returns the remaining rect below the tabs for content
pub fn draw_mode_tabs(
    ctx: &mut UiContext,
    rect: Rect,
    state: &mut TextureEditorState,
) -> Rect {
    const TAB_HEIGHT: f32 = 26.0;
    const TAB_BG: Color = Color::new(0.14, 0.14, 0.16, 1.0);
    const TAB_ACTIVE: Color = Color::new(0.22, 0.22, 0.25, 1.0);
    const TAB_HOVER: Color = Color::new(0.18, 0.18, 0.20, 1.0);

    // Draw tab bar background
    let tab_bar_rect = Rect::new(rect.x, rect.y, rect.w, TAB_HEIGHT);
    draw_rectangle(tab_bar_rect.x, tab_bar_rect.y, tab_bar_rect.w, tab_bar_rect.h, TAB_BG);

    // Two tabs: Paint and UV
    let tab_width = rect.w / 2.0;
    let tabs = [
        (TextureEditorMode::Paint, "Paint"),
        (TextureEditorMode::Uv, "UV"),
    ];

    for (i, (mode, label)) in tabs.iter().enumerate() {
        let tab_x = rect.x + i as f32 * tab_width;
        let tab_rect = Rect::new(tab_x, rect.y, tab_width, TAB_HEIGHT);
        let is_active = state.mode == *mode;
        let hovered = ctx.mouse.inside(&tab_rect);

        // Tab background
        let bg = if is_active {
            TAB_ACTIVE
        } else if hovered {
            TAB_HOVER
        } else {
            TAB_BG
        };
        draw_rectangle(tab_rect.x, tab_rect.y, tab_rect.w, tab_rect.h, bg);

        // Active indicator (bottom line)
        if is_active {
            draw_rectangle(tab_rect.x, tab_rect.y + TAB_HEIGHT - 2.0, tab_rect.w, 2.0, ACCENT_COLOR);
        }

        // Tab label
        let text_color = if is_active { TEXT_COLOR } else { TEXT_DIM };
        let text_size = 14.0;
        let text_dims = measure_text(label, None, text_size as u16, 1.0);
        let text_x = tab_rect.x + (tab_rect.w - text_dims.width) / 2.0;
        let text_y = tab_rect.y + (TAB_HEIGHT + text_dims.height) / 2.0 - 2.0;
        draw_text(label, text_x, text_y, text_size, text_color);

        // Handle click
        if hovered && ctx.mouse.left_pressed {
            state.mode = *mode;
            // Clear UV selection when switching modes to avoid stale state
            if *mode == TextureEditorMode::Paint {
                state.uv_selection.clear();
                state.uv_modal_transform = UvModalTransform::None;
            }
        }
    }

    // Separator line
    draw_line(rect.x, rect.y + TAB_HEIGHT, rect.x + rect.w, rect.y + TAB_HEIGHT, 1.0, Color::new(0.25, 0.25, 0.28, 1.0));

    // Return content rect below tabs
    Rect::new(rect.x, rect.y + TAB_HEIGHT, rect.w, rect.h - TAB_HEIGHT)
}

/// Convert screen position to texture pixel coordinates
pub fn screen_to_texture(
    screen_x: f32,
    screen_y: f32,
    canvas_rect: &Rect,
    texture: &UserTexture,
    state: &TextureEditorState,
) -> Option<(i32, i32)> {
    // Canvas center
    let canvas_cx = canvas_rect.x + canvas_rect.w / 2.0;
    let canvas_cy = canvas_rect.y + canvas_rect.h / 2.0;

    // Texture size in screen space
    let tex_screen_w = texture.width as f32 * state.zoom;
    let tex_screen_h = texture.height as f32 * state.zoom;

    // Texture top-left in screen space
    let tex_x = canvas_cx - tex_screen_w / 2.0 + state.pan_x;
    let tex_y = canvas_cy - tex_screen_h / 2.0 + state.pan_y;

    // Convert to texture coordinates
    let px = ((screen_x - tex_x) / state.zoom).floor() as i32;
    let py = ((screen_y - tex_y) / state.zoom).floor() as i32;

    Some((px, py))
}

// =============================================================================
// Palette Generation (Color Ramps from Key Colors)
// =============================================================================

/// Convert RGB (0-31 per channel) to HSL (h: 0-360, s: 0-1, l: 0-1)
fn rgb5_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 31.0;
    let g = g as f32 / 31.0;
    let b = b as f32 / 31.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;

    if (max - min).abs() < 0.0001 {
        return (0.0, 0.0, l); // Achromatic
    }

    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };

    let h = if (max - r).abs() < 0.0001 {
        let mut h = (g - b) / d;
        if g < b { h += 6.0; }
        h * 60.0
    } else if (max - g).abs() < 0.0001 {
        ((b - r) / d + 2.0) * 60.0
    } else {
        ((r - g) / d + 4.0) * 60.0
    };

    (h, s, l)
}

/// Convert HSL to RGB (0-31 per channel)
fn hsl_to_rgb5(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    if s.abs() < 0.0001 {
        let v = (l * 31.0).round() as u8;
        return (v, v, v);
    }

    let h = h % 360.0;
    let h = if h < 0.0 { h + 360.0 } else { h };

    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;

    fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
        if t < 0.0 { t += 1.0; }
        if t > 1.0 { t -= 1.0; }
        if t < 1.0 / 6.0 { return p + (q - p) * 6.0 * t; }
        if t < 0.5 { return q; }
        if t < 2.0 / 3.0 { return p + (q - p) * (2.0 / 3.0 - t) * 6.0; }
        p
    }

    let r = hue_to_rgb(p, q, h / 360.0 + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h / 360.0);
    let b = hue_to_rgb(p, q, h / 360.0 - 1.0 / 3.0);

    (
        (r * 31.0).round().clamp(0.0, 31.0) as u8,
        (g * 31.0).round().clamp(0.0, 31.0) as u8,
        (b * 31.0).round().clamp(0.0, 31.0) as u8,
    )
}

/// Generate a 5-color ramp from a key color
///
/// - `key_color`: RGB values (0-31 per channel)
/// - `brightness_range`: 0.0-1.0, how much lightness varies across the ramp
/// - `hue_shift`: degrees to shift hue per step (0 = monochrome, 10-20 = natural)
///
/// Returns 5 colors: dark → key (mid) → light
fn generate_ramp(
    key_color: (u8, u8, u8),
    brightness_range: f32,
    hue_shift: f32,
) -> [Color15; 5] {
    let (h, s, l) = rgb5_to_hsl(key_color.0, key_color.1, key_color.2);

    // Key color sits at index 2 (middle)
    // Dark shades: decrease lightness, optionally shift hue toward warm
    // Light shades: increase lightness, decrease saturation, optionally shift hue toward cool

    let l_range = brightness_range * 0.4; // How much lightness varies from mid

    let mut colors = [Color15::default(); 5];
    for i in 0..5 {
        let step = i as f32 - 2.0; // -2, -1, 0, 1, 2

        // Lightness: darker for negative steps, lighter for positive
        let new_l = (l + step * l_range / 2.0).clamp(0.05, 0.95);

        // Saturation: decrease for light colors to avoid neon look
        let sat_factor = if step > 0.0 { 1.0 - step * 0.15 } else { 1.0 };
        let new_s = (s * sat_factor).clamp(0.0, 1.0);

        // Hue shift: warm for shadows, cool for highlights
        let new_h = h + step * hue_shift;

        let (r, g, b) = hsl_to_rgb5(new_h, new_s, new_l);
        colors[i] = Color15::new(r, g, b);
    }

    colors
}

/// Generate a complete 16-color palette from 3 key colors
///
/// Layout:
/// - Index 0: Transparent
/// - Indices 1-5: Ramp from key color 1
/// - Indices 6-10: Ramp from key color 2
/// - Indices 11-15: Ramp from key color 3
pub fn generate_palette_from_keys(
    key_colors: [(u8, u8, u8); 3],
    brightness_range: f32,
    hue_shift: f32,
) -> [Color15; 16] {
    let mut palette = [Color15::TRANSPARENT; 16];

    // Index 0 stays transparent

    // Generate 3 ramps of 5 colors each
    for (ramp_idx, key_color) in key_colors.iter().enumerate() {
        let ramp = generate_ramp(*key_color, brightness_range, hue_shift);
        let start_idx = 1 + ramp_idx * 5;
        for (i, color) in ramp.iter().enumerate() {
            palette[start_idx + i] = *color;
        }
    }

    palette
}

/// Draw the texture canvas with optional UV overlay
///
/// When `uv_data` is Some and state.mode is Uv, draws UV wireframe overlay on top of texture.
/// The texture is always drawn as background (useful for seeing UV placement).
pub fn draw_texture_canvas(
    ctx: &mut UiContext,
    canvas_rect: Rect,
    texture: &mut UserTexture,
    state: &mut TextureEditorState,
    uv_data: Option<&UvOverlayData>,
) {
    // Update selection animation frame
    state.selection_anim_frame = state.selection_anim_frame.wrapping_add(1);

    // Draw canvas background
    draw_rectangle(canvas_rect.x, canvas_rect.y, canvas_rect.w, canvas_rect.h, PANEL_BG);

    // Enable scissor clipping to canvas bounds
    let dpi = screen_dpi_scale();
    gl_use_default_material();
    unsafe {
        get_internal_gl().quad_gl.scissor(Some((
            (canvas_rect.x * dpi) as i32,
            (canvas_rect.y * dpi) as i32,
            (canvas_rect.w * dpi) as i32,
            (canvas_rect.h * dpi) as i32,
        )));
    }

    // Calculate texture position
    let canvas_cx = canvas_rect.x + canvas_rect.w / 2.0;
    let canvas_cy = canvas_rect.y + canvas_rect.h / 2.0;
    let tex_screen_w = texture.width as f32 * state.zoom;
    let tex_screen_h = texture.height as f32 * state.zoom;
    let tex_x = canvas_cx - tex_screen_w / 2.0 + state.pan_x;
    let tex_y = canvas_cy - tex_screen_h / 2.0 + state.pan_y;

    // Draw checkerboard background for transparency
    // The checkerboard moves smoothly with the texture by anchoring to tex_x/tex_y
    // When tiling preview is on, extend to 3x3 tile area
    let check_size = (state.zoom * 2.0).max(4.0);
    let (checker_x, checker_y, checker_w, checker_h) = if state.show_tiling {
        (tex_x - tex_screen_w, tex_y - tex_screen_h, tex_screen_w * 3.0, tex_screen_h * 3.0)
    } else {
        (tex_x, tex_y, tex_screen_w, tex_screen_h)
    };
    let clip_x = checker_x.max(canvas_rect.x);
    let clip_y = checker_y.max(canvas_rect.y);
    let end_x = (checker_x + checker_w).min(canvas_rect.x + canvas_rect.w);
    let end_y = (checker_y + checker_h).min(canvas_rect.y + canvas_rect.h);

    // Calculate the first row/col indices that are visible
    let first_row = ((clip_y - checker_y) / check_size).floor() as i32;
    let first_col = ((clip_x - checker_x) / check_size).floor() as i32;

    // Start drawing from the actual grid position (may be before clip region)
    let mut row = first_row;
    let mut cy = checker_y + first_row as f32 * check_size;
    while cy < end_y {
        let mut col = first_col;
        let mut cx = checker_x + first_col as f32 * check_size;
        while cx < end_x {
            let c = if (row + col) % 2 == 0 {
                Color::new(0.25, 0.25, 0.28, 1.0)
            } else {
                Color::new(0.18, 0.18, 0.20, 1.0)
            };
            // Clip the rectangle to the visible area
            let draw_x = cx.max(clip_x);
            let draw_y = cy.max(clip_y);
            let draw_w = (cx + check_size).min(end_x) - draw_x;
            let draw_h = (cy + check_size).min(end_y) - draw_y;
            if draw_w > 0.0 && draw_h > 0.0 {
                draw_rectangle(draw_x, draw_y, draw_w, draw_h, c);
            }
            cx += check_size;
            col += 1;
        }
        cy += check_size;
        row += 1;
    }

    // Draw texture pixels (with optional tiling preview)
    // When tiling is enabled, we draw 9 copies: center + 8 surrounding
    let tile_offsets: &[(i32, i32, f32)] = if state.show_tiling {
        // Outer tiles are slightly dimmed to emphasize the center
        &[
            (-1, -1, 0.6), (0, -1, 0.6), (1, -1, 0.6),
            (-1,  0, 0.6),               (1,  0, 0.6),
            (-1,  1, 0.6), (0,  1, 0.6), (1,  1, 0.6),
            ( 0,  0, 1.0), // Center tile drawn last (on top) at full brightness
        ]
    } else {
        &[(0, 0, 1.0)] // Just center tile
    };

    for &(tile_ox, tile_oy, brightness) in tile_offsets {
        let tile_offset_x = tile_ox as f32 * tex_screen_w;
        let tile_offset_y = tile_oy as f32 * tex_screen_h;

        for py in 0..texture.height {
            for px in 0..texture.width {
                let screen_x = tex_x + tile_offset_x + px as f32 * state.zoom;
                let screen_y = tex_y + tile_offset_y + py as f32 * state.zoom;

                // Clip to canvas
                if screen_x + state.zoom < canvas_rect.x
                    || screen_x > canvas_rect.x + canvas_rect.w
                    || screen_y + state.zoom < canvas_rect.y
                    || screen_y > canvas_rect.y + canvas_rect.h
                {
                    continue;
                }

                let color = texture.get_color(px, py);
                if !color.is_transparent() {
                    let [r, g, b, _] = color.to_rgba();
                    draw_rectangle(
                        screen_x,
                        screen_y,
                        state.zoom,
                        state.zoom,
                        Color::new(
                            r as f32 / 255.0 * brightness,
                            g as f32 / 255.0 * brightness,
                            b as f32 / 255.0 * brightness,
                            1.0,
                        ),
                    );
                }
            }
        }
    }

    // Draw pixel grid at high zoom (when enabled)
    // When tiling is on, extend grid to cover 3x3 tile area
    if state.show_grid && state.zoom >= 4.0 {
        let grid_color = Color::new(1.0, 1.0, 1.0, 0.1);
        let (grid_start_x, grid_start_y, grid_tiles) = if state.show_tiling {
            (tex_x - tex_screen_w, tex_y - tex_screen_h, 3)
        } else {
            (tex_x, tex_y, 1)
        };
        let grid_end_x = grid_start_x + tex_screen_w * grid_tiles as f32;
        let grid_end_y = grid_start_y + tex_screen_h * grid_tiles as f32;

        // Vertical lines
        for tile in 0..grid_tiles {
            let tile_offset = tile as f32 * tex_screen_w;
            for px in 0..=texture.width {
                let x = grid_start_x + tile_offset + px as f32 * state.zoom;
                if x >= canvas_rect.x && x <= canvas_rect.x + canvas_rect.w {
                    draw_line(
                        x,
                        grid_start_y.max(canvas_rect.y),
                        x,
                        grid_end_y.min(canvas_rect.y + canvas_rect.h),
                        1.0,
                        grid_color,
                    );
                }
            }
        }
        // Horizontal lines
        for tile in 0..grid_tiles {
            let tile_offset = tile as f32 * tex_screen_h;
            for py in 0..=texture.height {
                let y = grid_start_y + tile_offset + py as f32 * state.zoom;
                if y >= canvas_rect.y && y <= canvas_rect.y + canvas_rect.h {
                    draw_line(
                        grid_start_x.max(canvas_rect.x),
                        y,
                        grid_end_x.min(canvas_rect.x + canvas_rect.w),
                        y,
                        1.0,
                        grid_color,
                    );
                }
            }
        }
    }

    // Draw texture border (always shows center tile boundary)
    draw_rectangle_lines(tex_x, tex_y, tex_screen_w, tex_screen_h, 1.0, Color::new(0.5, 0.5, 0.5, 1.0));

    // Draw floating selection pixels (if any)
    if let Some(ref selection) = state.selection {
        if let Some(ref floating) = selection.floating {
            for sy in 0..selection.height {
                for sx in 0..selection.width {
                    let idx = floating[sy * selection.width + sx];
                    if idx == 0 {
                        continue; // Transparent
                    }

                    let tx = selection.x + sx as i32;
                    let ty = selection.y + sy as i32;

                    let screen_x = tex_x + tx as f32 * state.zoom;
                    let screen_y = tex_y + ty as f32 * state.zoom;

                    // Clip to canvas
                    if screen_x + state.zoom < canvas_rect.x
                        || screen_x > canvas_rect.x + canvas_rect.w
                        || screen_y + state.zoom < canvas_rect.y
                        || screen_y > canvas_rect.y + canvas_rect.h
                    {
                        continue;
                    }

                    let color = texture.get_palette_color(idx);
                    let [r, g, b, _] = color.to_rgba();
                    draw_rectangle(
                        screen_x,
                        screen_y,
                        state.zoom,
                        state.zoom,
                        Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0),
                    );
                }
            }
        }

        // Draw marching ants
        draw_selection_marching_ants(selection, tex_x, tex_y, state.zoom, state.selection_anim_frame);

        // Check for edge hover (only when not floating - can't resize floating selection)
        let hovered_edge = if selection.floating.is_none() && !state.creating_selection {
            selection.hit_test_edge(ctx.mouse.x, ctx.mouse.y, tex_x, tex_y, state.zoom, 8.0)
        } else {
            None
        };

        // Draw resize handles (only for non-floating selections)
        if selection.floating.is_none() {
            draw_selection_handles(selection, tex_x, tex_y, state.zoom, hovered_edge);
        }
    }

    // Draw UV overlay when in UV mode
    if state.mode == TextureEditorMode::Uv {
        if let Some(uv) = uv_data {
            draw_uv_overlay(
                &canvas_rect,
                texture.width as f32,
                texture.height as f32,
                tex_x,
                tex_y,
                state.zoom,
                uv,
                &state.uv_selection,
            );
        }
    }

    // Disable scissor clipping
    unsafe {
        get_internal_gl().quad_gl.scissor(None);
    }

    // Handle input
    let inside = ctx.mouse.inside(&canvas_rect);

    // Panning with right mouse button
    if inside && ctx.mouse.right_pressed {
        state.panning = true;
        state.pan_start = (ctx.mouse.x, ctx.mouse.y);
        state.pan_start_offset = (state.pan_x, state.pan_y);
    }
    if state.panning {
        if ctx.mouse.right_down {
            state.pan_x = state.pan_start_offset.0 + (ctx.mouse.x - state.pan_start.0);
            state.pan_y = state.pan_start_offset.1 + (ctx.mouse.y - state.pan_start.1);
        } else {
            state.panning = false;
        }
    }

    // Zoom with scroll wheel (gentle 4% per tick)
    if inside && ctx.mouse.scroll != 0.0 {
        let old_zoom = state.zoom;
        let zoom_factor = 1.04f32;
        if ctx.mouse.scroll > 0.0 {
            state.zoom = (state.zoom * zoom_factor).min(32.0);
        } else {
            state.zoom = (state.zoom / zoom_factor).max(0.5);
        }

        // Zoom toward mouse position
        if old_zoom != state.zoom {
            let mouse_rel_x = ctx.mouse.x - canvas_cx;
            let mouse_rel_y = ctx.mouse.y - canvas_cy;
            let scale = state.zoom / old_zoom;
            state.pan_x = (state.pan_x - mouse_rel_x) * scale + mouse_rel_x;
            state.pan_y = (state.pan_y - mouse_rel_y) * scale + mouse_rel_y;
        }
    }

    // Keyboard shortcuts for selection (work regardless of mouse position)
    let cmd_held = is_key_down(KeyCode::LeftSuper) || is_key_down(KeyCode::RightSuper);

    // Escape to deselect or cancel move
    if is_key_pressed(KeyCode::Escape) && state.selection.is_some() {
        let has_floating = state.selection.as_ref().map_or(false, |s| s.floating.is_some());

        // If we have a floating selection being moved, cancel and restore original position
        if has_floating && state.move_original_pos.is_some() {
            if let (Some(ref mut selection), Some((orig_x, orig_y))) = (&mut state.selection, state.move_original_pos) {
                selection.x = orig_x;
                selection.y = orig_y;
            }
            state.move_original_pos = None;
            state.set_status("Move cancelled");
        } else if has_floating {
            // Commit floating selection
            commit_floating_selection(texture, state);
            state.set_status("Selection committed");
        } else {
            // Just clear selection
            state.selection = None;
        }
        state.creating_selection = false;
        state.selection_drag_start = None;
    }

    // Copy (Cmd+C)
    if cmd_held && is_key_pressed(KeyCode::C) {
        if let Some(ref selection) = state.selection {
            let clipboard = make_clipboard_from_selection(texture, selection);
            let w = clipboard.width;
            let h = clipboard.height;
            state.clipboard = Some(clipboard);
            state.set_status(&format!("Copied {}×{} pixels", w, h));
        }
    }

    // Cut (Cmd+X)
    if cmd_held && is_key_pressed(KeyCode::X) {
        if let Some(selection) = state.selection.take() {
            let clipboard = make_clipboard_from_selection(texture, &selection);
            let w = clipboard.width;
            let h = clipboard.height;
            state.clipboard = Some(clipboard);
            // Clear the selected area (signal undo to caller)
            state.undo_save_pending = Some("Cut".to_string());
            clear_selection_area(texture, &selection);
            state.set_status(&format!("Cut {}×{} pixels", w, h));
        }
    }

    // Paste (Cmd+V)
    if cmd_held && is_key_pressed(KeyCode::V) {
        if let Some(ref clipboard) = state.clipboard.clone() {
            // Commit any existing floating selection
            let has_floating = state.selection.as_ref().map_or(false, |s| s.floating.is_some());
            if has_floating {
                commit_floating_selection(texture, state);
            }

            // Create floating selection at center of texture
            let center_x = (texture.width as i32 - clipboard.width as i32) / 2;
            let center_y = (texture.height as i32 - clipboard.height as i32) / 2;

            state.selection = Some(Selection {
                x: center_x,
                y: center_y,
                width: clipboard.width,
                height: clipboard.height,
                floating: Some(clipboard.indices.clone()),
            });
            state.tool = DrawTool::Select;
            state.set_status(&format!("Pasted {}×{} pixels", clipboard.width, clipboard.height));
        }
    }

    // Drawing and selection
    if inside && !state.panning {
        // UV mode: handle UV-specific input
        if state.mode == TextureEditorMode::Uv {
            handle_uv_input(ctx, &canvas_rect, texture, state, uv_data);
        } else if let Some((px, py)) = screen_to_texture(ctx.mouse.x, ctx.mouse.y, &canvas_rect, texture, state) {
            // Handle Select tool
            if state.tool == DrawTool::Select {
                // Check for edge hover (for resize cursor feedback)
                let hovered_edge = if let Some(ref selection) = state.selection {
                    if selection.floating.is_none() && !state.creating_selection && state.resizing_edge.is_none() {
                        selection.hit_test_edge(ctx.mouse.x, ctx.mouse.y, tex_x, tex_y, state.zoom, 8.0)
                    } else {
                        None
                    }
                } else {
                    None
                };

                // Show cursor preview (crosshair or edge indicator)
                if px >= 0 && py >= 0 && (px as usize) < texture.width && (py as usize) < texture.height {
                    let cursor_x = tex_x + px as f32 * state.zoom;
                    let cursor_y = tex_y + py as f32 * state.zoom;

                    // Only show crosshair if not hovering an edge
                    if hovered_edge.is_none() && state.resizing_edge.is_none() {
                        draw_rectangle_lines(
                            cursor_x,
                            cursor_y,
                            state.zoom,
                            state.zoom,
                            1.0,
                            Color::new(1.0, 1.0, 1.0, 0.5),
                        );
                    }
                }

                // Mouse pressed - start selection, move, or resize
                if ctx.mouse.left_pressed {
                    // First check if we're clicking on an edge/handle
                    if let Some(edge) = hovered_edge {
                        // Shift+click on edge = resize, otherwise move
                        let shift_held = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
                        if shift_held {
                            // Shift+click edge = resize
                            state.resizing_edge = Some(edge);
                            state.selection_drag_start = Some((px, py));
                            state.creating_selection = false;
                        } else {
                            // Click edge = move (lift and drag, like clicking inside)
                            state.selection_drag_start = Some((px, py));
                            state.creating_selection = false;

                            // Store original position for cancel
                            if let Some(ref selection) = state.selection {
                                state.move_original_pos = Some((selection.x, selection.y));
                            }

                            // Lift pixels into floating selection if not already floating
                            if state.selection.as_ref().map_or(false, |s| s.floating.is_none()) {
                                lift_selection_to_floating(texture, state);
                            }
                        }
                    } else if let Some(ref selection) = state.selection {
                        if selection.contains(px, py) {
                            // Click inside selection - start moving
                            state.selection_drag_start = Some((px, py));
                            state.creating_selection = false;

                            // Store original position for cancel
                            state.move_original_pos = Some((selection.x, selection.y));

                            // Lift pixels into floating selection if not already floating
                            if selection.floating.is_none() {
                                lift_selection_to_floating(texture, state);
                            }
                        } else {
                            // Click outside - commit floating and start new selection
                            if selection.floating.is_some() {
                                commit_floating_selection(texture, state);
                            }
                            state.selection = None;
                            state.move_original_pos = None;
                            state.selection_drag_start = Some((px, py));
                            state.creating_selection = true;
                        }
                    } else {
                        // No selection - start creating one
                        state.selection_drag_start = Some((px, py));
                        state.creating_selection = true;
                    }
                }

                // Mouse dragging
                if ctx.mouse.left_down {
                    if let Some((start_x, start_y)) = state.selection_drag_start {
                        if let Some(edge) = state.resizing_edge {
                            // Resize selection by moving the appropriate edge
                            if let Some(ref mut selection) = state.selection {
                                let dx = px - start_x;
                                let dy = py - start_y;

                                match edge {
                                    ResizeEdge::Left => {
                                        let new_x = selection.x + dx;
                                        let new_w = (selection.width as i32 - dx).max(1) as usize;
                                        if new_w >= 1 {
                                            selection.x = new_x;
                                            selection.width = new_w;
                                        }
                                    }
                                    ResizeEdge::Right => {
                                        selection.width = (selection.width as i32 + dx).max(1) as usize;
                                    }
                                    ResizeEdge::Top => {
                                        let new_y = selection.y + dy;
                                        let new_h = (selection.height as i32 - dy).max(1) as usize;
                                        if new_h >= 1 {
                                            selection.y = new_y;
                                            selection.height = new_h;
                                        }
                                    }
                                    ResizeEdge::Bottom => {
                                        selection.height = (selection.height as i32 + dy).max(1) as usize;
                                    }
                                    ResizeEdge::TopLeft => {
                                        let new_x = selection.x + dx;
                                        let new_y = selection.y + dy;
                                        let new_w = (selection.width as i32 - dx).max(1) as usize;
                                        let new_h = (selection.height as i32 - dy).max(1) as usize;
                                        if new_w >= 1 && new_h >= 1 {
                                            selection.x = new_x;
                                            selection.y = new_y;
                                            selection.width = new_w;
                                            selection.height = new_h;
                                        }
                                    }
                                    ResizeEdge::TopRight => {
                                        let new_y = selection.y + dy;
                                        let new_w = (selection.width as i32 + dx).max(1) as usize;
                                        let new_h = (selection.height as i32 - dy).max(1) as usize;
                                        if new_w >= 1 && new_h >= 1 {
                                            selection.y = new_y;
                                            selection.width = new_w;
                                            selection.height = new_h;
                                        }
                                    }
                                    ResizeEdge::BottomLeft => {
                                        let new_x = selection.x + dx;
                                        let new_w = (selection.width as i32 - dx).max(1) as usize;
                                        let new_h = (selection.height as i32 + dy).max(1) as usize;
                                        if new_w >= 1 && new_h >= 1 {
                                            selection.x = new_x;
                                            selection.width = new_w;
                                            selection.height = new_h;
                                        }
                                    }
                                    ResizeEdge::BottomRight => {
                                        selection.width = (selection.width as i32 + dx).max(1) as usize;
                                        selection.height = (selection.height as i32 + dy).max(1) as usize;
                                    }
                                }
                                state.selection_drag_start = Some((px, py));
                            }
                        } else if state.creating_selection {
                            // Update selection rectangle preview
                            state.selection = Some(Selection::from_corners(start_x, start_y, px, py));
                        } else if let Some(ref mut selection) = state.selection {
                            // Move floating selection
                            let dx = px - start_x;
                            let dy = py - start_y;
                            selection.x += dx;
                            selection.y += dy;
                            state.selection_drag_start = Some((px, py));
                        }
                    }
                }

                // Mouse released
                if !ctx.mouse.left_down && state.selection_drag_start.is_some() {
                    if state.creating_selection {
                        // Finalize selection creation
                        if let Some(ref selection) = state.selection {
                            // If selection is too small (0 or 1 pixel), clear it
                            if selection.width < 2 && selection.height < 2 {
                                state.selection = None;
                            }
                        }
                    }
                    state.selection_drag_start = None;
                    state.creating_selection = false;
                    state.resizing_edge = None;
                    state.move_original_pos = None; // Move committed, clear cancel position
                }
            } else {
                // Non-Select tool behavior
                // Show cursor preview
                if px >= 0 && py >= 0 && (px as usize) < texture.width && (py as usize) < texture.height {
                    let cursor_x = tex_x + px as f32 * state.zoom;
                    let cursor_y = tex_y + py as f32 * state.zoom;

                    if state.tool.uses_brush_size() {
                        let size = state.brush_size as f32 * state.zoom;
                        let half = ((state.brush_size as f32 - 1.0) / 2.0) * state.zoom;
                        let cursor_color = Color::new(1.0, 1.0, 1.0, 0.5);

                        // Show shape-appropriate cursor
                        if state.tool == DrawTool::Brush && state.brush_shape == BrushShape::Circle && state.brush_size > 1 {
                            // Circle cursor
                            let radius = size / 2.0;
                            let cx = cursor_x + state.zoom / 2.0;
                            let cy = cursor_y + state.zoom / 2.0;
                            draw_circle_lines(cx, cy, radius, 1.0, cursor_color);
                        } else {
                            // Square cursor (or single pixel)
                            draw_rectangle_lines(
                                cursor_x - half,
                                cursor_y - half,
                                size,
                                size,
                                1.0,
                                cursor_color,
                            );
                        }
                    } else {
                        draw_rectangle_lines(
                            cursor_x,
                            cursor_y,
                            state.zoom,
                            state.zoom,
                            1.0,
                            Color::new(1.0, 1.0, 1.0, 0.5),
                        );
                    }
                }

                // Handle drawing
                if ctx.mouse.left_pressed && !state.drawing {
                    state.drawing = true;
                    state.last_draw_pos = Some((px, py));

                    // Signal to caller to save undo at start of stroke/shape
                    state.undo_save_pending = Some(format!("{:?}", state.tool));

                    if state.tool.is_shape_tool() {
                        state.shape_start = Some((px, py));
                    } else {

                        match state.tool {
                            DrawTool::Brush => {
                                tex_draw_brush(texture, px, py, state.brush_size, state.selected_index, state.brush_shape);
                            }
                            DrawTool::Fill => {
                                flood_fill(texture, px, py, state.selected_index);
                            }
                            _ => {}
                        }
                    }
                }

                if ctx.mouse.left_down && state.drawing && !state.tool.is_shape_tool() {
                    // Continue stroke
                    if let Some((last_x, last_y)) = state.last_draw_pos {
                        if (px, py) != (last_x, last_y) {
                            if state.tool == DrawTool::Brush {
                                // Interpolate brush along line
                                let dx = (px - last_x).abs();
                                let dy = (py - last_y).abs();
                                let steps = dx.max(dy);
                                for i in 0..=steps {
                                    let t = if steps == 0 { 0.0 } else { i as f32 / steps as f32 };
                                    let ix = last_x + ((px - last_x) as f32 * t) as i32;
                                    let iy = last_y + ((py - last_y) as f32 * t) as i32;
                                    tex_draw_brush(texture, ix, iy, state.brush_size, state.selected_index, state.brush_shape);
                                }
                            }
                            state.last_draw_pos = Some((px, py));
                        }
                    }
                }

                // Shape preview - draw actual shape pixels
                if state.drawing && state.tool.is_shape_tool() {
                    if let Some((sx, sy)) = state.shape_start {
                        // Draw preview (using current color as overlay)
                        let color = texture.get_palette_color(state.selected_index);
                        let [r, g, b, _] = color.to_rgba();
                        let preview_color = Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 0.7);
                        let pixel_size = state.zoom;

                        match state.tool {
                            DrawTool::Line => {
                                // Draw line preview using Bresenham with brush thickness
                                draw_line_preview(tex_x, tex_y, sx, sy, px, py, state.brush_size, pixel_size, preview_color);
                            }
                            DrawTool::Rectangle => {
                                if state.fill_shapes {
                                    draw_rect_filled_preview(tex_x, tex_y, sx, sy, px, py, pixel_size, preview_color);
                                } else {
                                    draw_rect_outline_preview(tex_x, tex_y, sx, sy, px, py, pixel_size, preview_color);
                                }
                            }
                            DrawTool::Ellipse => {
                                if state.fill_shapes {
                                    draw_ellipse_filled_preview(tex_x, tex_y, sx, sy, px, py, pixel_size, preview_color);
                                } else {
                                    draw_ellipse_outline_preview(tex_x, tex_y, sx, sy, px, py, pixel_size, preview_color);
                                }
                            }
                            _ => {}
                        }
                    }
                }

                // Complete shape on release
                if !ctx.mouse.left_down && state.drawing {
                    if state.tool.is_shape_tool() {
                        if let Some((sx, sy)) = state.shape_start {
                            match state.tool {
                                DrawTool::Line => {
                                    tex_draw_line_thick(texture, sx, sy, px, py, state.brush_size, state.selected_index);
                                }
                                DrawTool::Rectangle => {
                                    if state.fill_shapes {
                                        tex_draw_rect_filled(texture, sx, sy, px, py, state.selected_index);
                                    } else {
                                        tex_draw_rect_outline(texture, sx, sy, px, py, state.selected_index);
                                    }
                                }
                                DrawTool::Ellipse => {
                                    if state.fill_shapes {
                                        tex_draw_ellipse_filled(texture, sx, sy, px, py, state.selected_index);
                                    } else {
                                        tex_draw_ellipse_outline(texture, sx, sy, px, py, state.selected_index);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    state.drawing = false;
                    state.shape_start = None;
                    state.last_draw_pos = None;
                }
            }
        }
    }

    // Reset drawing state if mouse released outside
    if !ctx.mouse.left_down && state.drawing {
        state.drawing = false;
        state.shape_start = None;
        state.last_draw_pos = None;
    }
}

/// Draw the tool panel in 2-column layout (below canvas)
pub fn draw_tool_panel(
    ctx: &mut UiContext,
    rect: Rect,
    state: &mut TextureEditorState,
    icon_font: Option<&Font>,
) {
    // Background
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::new(0.14, 0.14, 0.16, 1.0));

    let btn_size = 28.0;  // Bigger buttons for easier clicking
    let gap = 2.0;
    let padding = 4.0;

    // Calculate 2-column positions
    let col1_x = rect.x + padding;
    let col2_x = rect.x + padding + btn_size + gap;
    let mut y = rect.y + padding;

    // === Row 1: Undo/Redo (always at top) ===
    if draw_action_button_small(ctx, col1_x, y, btn_size, icon::UNDO, "Undo", icon_font) {
        state.undo_requested = true;
    }
    if draw_action_button_small(ctx, col2_x, y, btn_size, icon::REDO, "Redo", icon_font) {
        state.redo_requested = true;
    }
    y += btn_size + gap;

    // === Row 2: Zoom-/Zoom+ ===
    if draw_action_button_small(ctx, col1_x, y, btn_size, icon::ZOOM_OUT, "Zoom-", icon_font) {
        state.zoom = (state.zoom / 1.5).max(1.0);
    }
    if draw_action_button_small(ctx, col2_x, y, btn_size, icon::ZOOM_IN, "Zoom+", icon_font) {
        state.zoom = (state.zoom * 1.5).min(32.0);
    }
    y += btn_size + gap;

    // === Row 3: Fit/Grid ===
    if draw_action_button_small(ctx, col1_x, y, btn_size, icon::FOCUS, "Fit to view", icon_font) {
        state.pan_x = 0.0;
        state.pan_y = 0.0;
        state.zoom = 4.0;
    }
    if draw_toggle_button_small(ctx, col2_x, y, btn_size, icon::GRID, "Toggle grid", state.show_grid, icon_font) {
        state.show_grid = !state.show_grid;
    }
    y += btn_size + gap;

    // === Row 4: Tiling preview ===
    if draw_toggle_button_small(ctx, col1_x, y, btn_size, icon::SQUARE_SQUARE, "Tiling preview", state.show_tiling, icon_font) {
        state.show_tiling = !state.show_tiling;
    }
    y += btn_size + gap;

    // Separator before tools
    y += 2.0;
    draw_line(col1_x, y, col2_x + btn_size, y, 1.0, Color::new(0.3, 0.3, 0.32, 1.0));
    y += 4.0;

    // Mode-specific tools
    match state.mode {
        TextureEditorMode::Paint => {
            // === Drawing tools in 2-column grid ===
            // Note: No Eraser tool - just paint with index 0 (transparent) to erase
            let mut clicked_tool: Option<DrawTool> = None;
            let all_tools = [
                DrawTool::Select,
                DrawTool::Brush,
                DrawTool::Fill,
                DrawTool::Line,
                DrawTool::Rectangle,
                DrawTool::Ellipse,
            ];

            for (i, tool) in all_tools.iter().enumerate() {
                let x = if i % 2 == 0 { col1_x } else { col2_x };
                if draw_tool_button_small(ctx, x, y, btn_size, *tool, state.tool == *tool, icon_font) {
                    clicked_tool = Some(*tool);
                }
                if i % 2 == 1 {
                    y += btn_size + gap;
                }
            }
            // If odd number of tools, advance y
            if all_tools.len() % 2 == 1 {
                y += btn_size + gap;
            }

            // Apply tool selection
            if let Some(tool) = clicked_tool {
                state.tool = tool;
            }
        }
        TextureEditorMode::Uv => {
            // === UV tools ===
            // Flip H / Flip V
            if draw_action_button_small(ctx, col1_x, y, btn_size, icon::FLIP_HORIZONTAL, "Flip H", icon_font) {
                state.uv_operation_pending = Some(UvOperation::FlipHorizontal);
                state.set_status("Flip Horizontal");
            }
            if draw_action_button_small(ctx, col2_x, y, btn_size, icon::FLIP_VERTICAL, "Flip V", icon_font) {
                state.uv_operation_pending = Some(UvOperation::FlipVertical);
                state.set_status("Flip Vertical");
            }
            y += btn_size + gap;

            // Rotate CW / Reset UVs
            if draw_action_button_small(ctx, col1_x, y, btn_size, icon::ROTATE_CW, "Rotate 90", icon_font) {
                state.uv_operation_pending = Some(UvOperation::RotateCW);
                state.set_status("Rotate 90° CW");
            }
            if draw_action_button_small(ctx, col2_x, y, btn_size, icon::REFRESH_CW, "Reset UV", icon_font) {
                state.uv_operation_pending = Some(UvOperation::ResetUVs);
                state.set_status("Reset UVs");
            }
            y += btn_size + gap;

            // Separator
            y += 2.0;
            draw_line(col1_x, y, col2_x + btn_size, y, 1.0, Color::new(0.3, 0.3, 0.32, 1.0));
            y += 4.0;

            // Hint text for keyboard shortcuts
            let hint_color = Color::new(0.5, 0.5, 0.52, 1.0);
            draw_text("G: Move", col1_x, y + 10.0, 11.0, hint_color);
            y += 14.0;
            draw_text("S: Scale", col1_x, y + 10.0, 11.0, hint_color);
            y += 14.0;
            draw_text("R: Rotate", col1_x, y + 10.0, 11.0, hint_color);
            y += 14.0;
        }
    }

    let _ = y; // silence unused warning

    // === Tool options section (size, fill toggle) - Paint mode only ===
    // Show for tools that use brush size OR shape tools
    let show_options = state.mode == TextureEditorMode::Paint
        && (state.tool.uses_brush_size() || matches!(state.tool, DrawTool::Rectangle | DrawTool::Ellipse));
    if show_options {
        y += 2.0;
        draw_line(col1_x, y, col2_x + btn_size, y, 1.0, Color::new(0.3, 0.3, 0.32, 1.0));
        y += 4.0;

        // Size row: - [size] +
        let small_btn = btn_size * 0.8;

        // Minus button
        let minus_rect = Rect::new(col1_x, y, small_btn, small_btn);
        let minus_hovered = ctx.mouse.inside(&minus_rect);
        draw_rectangle(minus_rect.x, minus_rect.y, minus_rect.w, minus_rect.h,
            if minus_hovered { Color::new(0.35, 0.35, 0.38, 1.0) } else { Color::new(0.22, 0.22, 0.25, 1.0) });
        draw_text("-", minus_rect.x + small_btn / 2.0 - 2.0, minus_rect.y + small_btn / 2.0 + 4.0, 12.0, TEXT_COLOR);
        if ctx.mouse.clicked(&minus_rect) {
            state.brush_size = (state.brush_size - 1).max(1);
        }

        // Size label centered
        let size_text = format!("{}", state.brush_size);
        let text_dims = measure_text(&size_text, None, 11, 1.0);
        let center_x = col1_x + small_btn + (col2_x - col1_x - small_btn) / 2.0;
        draw_text(&size_text, center_x - text_dims.width / 2.0, y + small_btn / 2.0 + 4.0, 11.0, WHITE);

        // Plus button
        let plus_rect = Rect::new(col2_x + btn_size - small_btn, y, small_btn, small_btn);
        let plus_hovered = ctx.mouse.inside(&plus_rect);
        draw_rectangle(plus_rect.x, plus_rect.y, plus_rect.w, plus_rect.h,
            if plus_hovered { Color::new(0.35, 0.35, 0.38, 1.0) } else { Color::new(0.22, 0.22, 0.25, 1.0) });
        draw_text("+", plus_rect.x + small_btn / 2.0 - 3.0, plus_rect.y + small_btn / 2.0 + 4.0, 12.0, TEXT_COLOR);
        if ctx.mouse.clicked(&plus_rect) {
            state.brush_size = (state.brush_size + 1).min(16);
        }
        y += small_btn + gap;

        // Brush shape toggle (only for Brush tool)
        if state.tool == DrawTool::Brush {
            // Square button
            let sq_rect = Rect::new(col1_x, y, btn_size, btn_size);
            let sq_hovered = ctx.mouse.inside(&sq_rect);
            let sq_selected = state.brush_shape == BrushShape::Square;
            let sq_bg = if sq_selected {
                ACCENT_COLOR
            } else if sq_hovered {
                Color::new(0.35, 0.35, 0.38, 1.0)
            } else {
                Color::new(0.22, 0.22, 0.25, 1.0)
            };
            draw_rectangle(sq_rect.x, sq_rect.y, sq_rect.w, sq_rect.h, sq_bg);
            if let Some(font) = icon_font {
                draw_icon_in_rect(font, icon::SQUARE, &sq_rect, if sq_selected { WHITE } else { TEXT_COLOR });
            }
            if sq_hovered {
                ctx.set_tooltip("Square brush", ctx.mouse.x, ctx.mouse.y);
            }
            if ctx.mouse.clicked(&sq_rect) {
                state.brush_shape = BrushShape::Square;
            }

            // Circle button
            let circ_rect = Rect::new(col2_x, y, btn_size, btn_size);
            let circ_hovered = ctx.mouse.inside(&circ_rect);
            let circ_selected = state.brush_shape == BrushShape::Circle;
            let circ_bg = if circ_selected {
                ACCENT_COLOR
            } else if circ_hovered {
                Color::new(0.35, 0.35, 0.38, 1.0)
            } else {
                Color::new(0.22, 0.22, 0.25, 1.0)
            };
            draw_rectangle(circ_rect.x, circ_rect.y, circ_rect.w, circ_rect.h, circ_bg);
            if let Some(font) = icon_font {
                draw_icon_in_rect(font, icon::CIRCLE, &circ_rect, if circ_selected { WHITE } else { TEXT_COLOR });
            }
            if circ_hovered {
                ctx.set_tooltip("Circle brush", ctx.mouse.x, ctx.mouse.y);
            }
            if ctx.mouse.clicked(&circ_rect) {
                state.brush_shape = BrushShape::Circle;
            }

            y += btn_size + gap;
        }

        // Fill toggle for Rectangle/Ellipse (in the options section, after size)
        if matches!(state.tool, DrawTool::Rectangle | DrawTool::Ellipse) {
            let fill_rect = Rect::new(col1_x, y, btn_size, btn_size);
            let fill_hovered = ctx.mouse.inside(&fill_rect);

            let bg = if state.fill_shapes {
                ACCENT_COLOR
            } else if fill_hovered {
                Color::new(0.35, 0.35, 0.38, 1.0)
            } else {
                Color::new(0.22, 0.22, 0.25, 1.0)
            };
            draw_rectangle(fill_rect.x, fill_rect.y, fill_rect.w, fill_rect.h, bg);

            if let Some(font) = icon_font {
                draw_icon_in_rect(font, icon::DROPLET, &fill_rect, if state.fill_shapes { WHITE } else { TEXT_COLOR });
            }

            if fill_hovered {
                ctx.set_tooltip(if state.fill_shapes { "Filled" } else { "Outline" }, ctx.mouse.x, ctx.mouse.y);
            }

            if ctx.mouse.clicked(&fill_rect) {
                state.fill_shapes = !state.fill_shapes;
            }
        }
    }
}

/// Helper: Draw a small tool button and return true if clicked
fn draw_tool_button_small(
    ctx: &mut UiContext,
    x: f32,
    y: f32,
    size: f32,
    tool: DrawTool,
    is_selected: bool,
    icon_font: Option<&Font>,
) -> bool {
    let btn_rect = Rect::new(x, y, size, size);
    let hovered = ctx.mouse.inside(&btn_rect);

    let bg = if is_selected {
        ACCENT_COLOR
    } else if hovered {
        Color::new(0.35, 0.35, 0.38, 1.0)
    } else {
        Color::new(0.22, 0.22, 0.25, 1.0)
    };
    draw_rectangle(btn_rect.x, btn_rect.y, btn_rect.w, btn_rect.h, bg);

    if let Some(font) = icon_font {
        let icon_char = tool.icon();
        draw_icon_in_rect(font, icon_char, &btn_rect, if is_selected { WHITE } else { TEXT_COLOR });
    }

    if hovered {
        ctx.set_tooltip(tool.tooltip(), ctx.mouse.x, ctx.mouse.y);
    }

    ctx.mouse.clicked(&btn_rect)
}

/// Helper: Draw a small action button and return true if clicked
fn draw_action_button_small(
    ctx: &mut UiContext,
    x: f32,
    y: f32,
    size: f32,
    icon_char: char,
    tooltip: &str,
    icon_font: Option<&Font>,
) -> bool {
    let btn_rect = Rect::new(x, y, size, size);
    let hovered = ctx.mouse.inside(&btn_rect);

    let bg = if hovered {
        Color::new(0.35, 0.35, 0.38, 1.0)
    } else {
        Color::new(0.22, 0.22, 0.25, 1.0)
    };
    draw_rectangle(btn_rect.x, btn_rect.y, btn_rect.w, btn_rect.h, bg);

    if let Some(font) = icon_font {
        draw_icon_in_rect(font, icon_char, &btn_rect, TEXT_COLOR);
    }

    if hovered {
        ctx.set_tooltip(tooltip, ctx.mouse.x, ctx.mouse.y);
    }

    ctx.mouse.clicked(&btn_rect)
}

/// Helper: Draw a small toggle button (highlighted when active) and return true if clicked
fn draw_toggle_button_small(
    ctx: &mut UiContext,
    x: f32,
    y: f32,
    size: f32,
    icon_char: char,
    tooltip: &str,
    is_active: bool,
    icon_font: Option<&Font>,
) -> bool {
    let btn_rect = Rect::new(x, y, size, size);
    let hovered = ctx.mouse.inside(&btn_rect);

    let bg = if is_active {
        ACCENT_COLOR
    } else if hovered {
        Color::new(0.35, 0.35, 0.38, 1.0)
    } else {
        Color::new(0.22, 0.22, 0.25, 1.0)
    };
    draw_rectangle(btn_rect.x, btn_rect.y, btn_rect.w, btn_rect.h, bg);

    if let Some(font) = icon_font {
        draw_icon_in_rect(font, icon_char, &btn_rect, if is_active { WHITE } else { TEXT_COLOR });
    }

    if hovered {
        ctx.set_tooltip(tooltip, ctx.mouse.x, ctx.mouse.y);
    }

    ctx.mouse.clicked(&btn_rect)
}

/// Helper: Draw an icon centered in a rect
fn draw_icon_in_rect(font: &Font, icon_char: char, rect: &Rect, color: Color) {
    let icon_str = icon_char.to_string();
    let icon_size = 14;  // Bigger icons
    // Icon fonts have square glyphs, use font size for centering
    draw_text_ex(
        &icon_str,
        (rect.x + (rect.w - icon_size as f32) / 2.0).round(),
        (rect.y + (rect.h + icon_size as f32) / 2.0).round(),
        TextParams {
            font: Some(font),
            font_size: icon_size,
            color,
            ..Default::default()
        },
    );
}

/// Helper: Draw a tool button and return true if clicked
fn draw_tool_button(
    ctx: &mut UiContext,
    x: f32,
    y: f32,
    size: f32,
    tool: DrawTool,
    is_selected: bool,
    icon_font: Option<&Font>,
) -> bool {
    let btn_rect = Rect::new(x, y, size, size);
    let hovered = ctx.mouse.inside(&btn_rect);

    let bg = if is_selected {
        ACCENT_COLOR
    } else if hovered {
        Color::new(0.35, 0.35, 0.38, 1.0)
    } else {
        Color::new(0.22, 0.22, 0.25, 1.0)
    };
    draw_rectangle(btn_rect.x, btn_rect.y, btn_rect.w, btn_rect.h, bg);

    if let Some(font) = icon_font {
        let icon = tool.icon();
        let icon_size = 16;  // Match main toolbar icon size
        let text_dims = measure_text(&icon.to_string(), Some(font), icon_size, 1.0);
        draw_text_ex(
            &icon.to_string(),
            btn_rect.x + (btn_rect.w - text_dims.width) / 2.0,
            btn_rect.y + (btn_rect.h + text_dims.height) / 2.0 - 2.0,
            TextParams {
                font: Some(font),
                font_size: icon_size,
                color: if is_selected { WHITE } else { TEXT_COLOR },
                ..Default::default()
            },
        );
    }

    if hovered {
        ctx.set_tooltip(tool.tooltip(), ctx.mouse.x, ctx.mouse.y);
    }

    ctx.mouse.clicked(&btn_rect)
}

/// Helper: Draw an action button and return true if clicked
fn draw_action_button(
    ctx: &mut UiContext,
    x: f32,
    y: f32,
    size: f32,
    icon_char: char,
    tooltip: &str,
    icon_font: Option<&Font>,
) -> bool {
    let btn_rect = Rect::new(x, y, size, size);
    let hovered = ctx.mouse.inside(&btn_rect);

    let bg = if hovered {
        Color::new(0.35, 0.35, 0.38, 1.0)
    } else {
        Color::new(0.22, 0.22, 0.25, 1.0)
    };
    draw_rectangle(btn_rect.x, btn_rect.y, btn_rect.w, btn_rect.h, bg);

    if let Some(font) = icon_font {
        let text_dims = measure_text(&icon_char.to_string(), Some(font), 14, 1.0);
        draw_text_ex(
            &icon_char.to_string(),
            btn_rect.x + (btn_rect.w - text_dims.width) / 2.0,
            btn_rect.y + (btn_rect.h + text_dims.height) / 2.0 - 2.0,
            TextParams {
                font: Some(font),
                font_size: 14,
                color: TEXT_COLOR,
                ..Default::default()
            },
        );
    }

    if hovered {
        ctx.set_tooltip(tooltip, ctx.mouse.x, ctx.mouse.y);
    }

    ctx.mouse.clicked(&btn_rect)
}

/// Draw the palette panel with color selection and RGB555 editing
pub fn draw_palette_panel(
    ctx: &mut UiContext,
    rect: Rect,
    texture: &mut UserTexture,
    state: &mut TextureEditorState,
    _icon_font: Option<&Font>,
) {
    let padding = 4.0;
    let mut y = rect.y + padding;

    // CLUT depth toggle buttons
    let btn_w = (rect.w - padding * 3.0) / 2.0;
    let btn_h = 18.0;

    let btn_4bit = Rect::new(rect.x + padding, y, btn_w, btn_h);
    let btn_8bit = Rect::new(rect.x + padding * 2.0 + btn_w, y, btn_w, btn_h);

    let is_4bit = texture.depth == ClutDepth::Bpp4;

    // 4-bit button
    let bg_4bit = if is_4bit { ACCENT_COLOR } else { Color::new(0.22, 0.22, 0.24, 1.0) };
    let hover_4bit = ctx.mouse.inside(&btn_4bit) && !is_4bit;
    draw_rectangle(btn_4bit.x, btn_4bit.y, btn_4bit.w, btn_4bit.h,
        if hover_4bit { Color::new(0.28, 0.28, 0.30, 1.0) } else { bg_4bit });
    let text_4bit = "4-bit";
    let tw = text_4bit.len() as f32 * 4.5;
    draw_text(text_4bit, btn_4bit.x + (btn_4bit.w - tw) / 2.0, btn_4bit.y + 13.0, 12.0,
        if is_4bit { WHITE } else { TEXT_COLOR });

    // 8-bit button
    let bg_8bit = if !is_4bit { ACCENT_COLOR } else { Color::new(0.22, 0.22, 0.24, 1.0) };
    let hover_8bit = ctx.mouse.inside(&btn_8bit) && is_4bit;
    draw_rectangle(btn_8bit.x, btn_8bit.y, btn_8bit.w, btn_8bit.h,
        if hover_8bit { Color::new(0.28, 0.28, 0.30, 1.0) } else { bg_8bit });
    let text_8bit = "8-bit";
    draw_text(text_8bit, btn_8bit.x + (btn_8bit.w - tw) / 2.0, btn_8bit.y + 13.0, 12.0,
        if !is_4bit { WHITE } else { TEXT_COLOR });

    // Handle depth toggle clicks
    if ctx.mouse.clicked(&btn_4bit) && !is_4bit {
        let affected = texture.convert_to_4bit();
        if affected > 0 {
            // Could show a status message about data loss
        }
        state.dirty = true;
        // Clamp selected index to valid range
        if state.selected_index > 15 {
            state.selected_index = state.selected_index % 16;
        }
        if state.editing_index > 15 {
            state.editing_index = state.editing_index % 16;
        }
    }
    if ctx.mouse.clicked(&btn_8bit) && is_4bit {
        texture.convert_to_8bit();
        state.dirty = true;
    }

    y += btn_h + 4.0;

    // Palette generator section (only for 4-bit mode)
    if is_4bit {
        let gen_h = 20.0;
        let swatch_size = 16.0;
        let swatch_gap = 2.0;

        // Three key color swatches
        let swatches_w = 3.0 * swatch_size + 2.0 * swatch_gap;
        let swatch_x = rect.x + padding;

        for i in 0..3 {
            let sx = swatch_x + i as f32 * (swatch_size + swatch_gap);
            let swatch_rect = Rect::new(sx, y, swatch_size, swatch_size);

            let (r, g, b) = state.palette_gen_colors[i];
            let rgb = Color::new(r as f32 / 31.0, g as f32 / 31.0, b as f32 / 31.0, 1.0);
            draw_rectangle(sx, y, swatch_size, swatch_size, rgb);

            // Selection highlight if editing this key color
            if state.palette_gen_editing == Some(i) {
                draw_rectangle_lines(sx - 1.0, y - 1.0, swatch_size + 2.0, swatch_size + 2.0, 2.0, WHITE);
            } else if ctx.mouse.inside(&swatch_rect) {
                draw_rectangle_lines(sx, y, swatch_size, swatch_size, 1.0, Color::new(1.0, 1.0, 1.0, 0.5));
            }

            // Click to select this key color for editing (mutually exclusive with palette selection)
            if ctx.mouse.clicked(&swatch_rect) {
                if state.palette_gen_editing == Some(i) {
                    // Already editing, deselect
                    state.palette_gen_editing = None;
                } else {
                    // Select this key color - RGB sliders will now edit it
                    state.palette_gen_editing = Some(i);
                }
            }
        }

        // "Gen" button
        let btn_gen_w = rect.w - swatches_w - padding * 3.0 - 4.0;
        let btn_gen_x = swatch_x + swatches_w + 4.0;
        let btn_gen = Rect::new(btn_gen_x, y, btn_gen_w.max(30.0), swatch_size);
        let gen_hover = ctx.mouse.inside(&btn_gen);
        let gen_bg = if gen_hover { Color::new(0.35, 0.50, 0.35, 1.0) } else { Color::new(0.25, 0.40, 0.25, 1.0) };
        draw_rectangle(btn_gen.x, btn_gen.y, btn_gen.w, btn_gen.h, gen_bg);
        let gen_text = "Gen";
        let tw = gen_text.len() as f32 * 5.0;
        draw_text(gen_text, btn_gen.x + (btn_gen.w - tw) / 2.0, btn_gen.y + 12.0, 12.0, WHITE);

        if ctx.mouse.clicked(&btn_gen) {
            // Generate palette from key colors
            let new_palette = generate_palette_from_keys(
                state.palette_gen_colors,
                state.palette_gen_brightness,
                state.palette_gen_hue_shift,
            );
            // Apply to texture
            for (i, color) in new_palette.iter().enumerate() {
                if i < texture.palette.len() {
                    texture.palette[i] = *color;
                }
            }
            state.dirty = true;
            state.palette_gen_editing = None;
        }

        y += gen_h + 4.0;
    } else {
        y += 2.0;
    }

    // Palette grid - custom layout for 4-bit (ramp-based), regular grid for 8-bit
    let grid_height: f32;

    if is_4bit {
        // 4-bit layout: Transparent square on left, 3 rows of 5 colors on right
        // Layout: [T] [1 2 3 4 5]
        //             [6 7 8 9 10]
        //             [11 12 13 14 15]
        let cell_size = ((rect.w - padding * 2.0 - 4.0) / 6.0).min(20.0);
        let trans_size = cell_size * 3.0; // Transparent cell spans 3 rows
        let grid_x = rect.x + padding;

        // Draw transparent cell (index 0) - large square on the left
        {
            let cell_x = grid_x;
            let cell_y = y;
            let cell_rect = Rect::new(cell_x, cell_y, trans_size, trans_size);

            // Checkerboard for transparent
            let check = trans_size / 4.0;
            for cy in 0..4 {
                for cx in 0..4 {
                    let c = if (cx + cy) % 2 == 0 {
                        Color::new(0.25, 0.25, 0.27, 1.0)
                    } else {
                        Color::new(0.15, 0.15, 0.17, 1.0)
                    };
                    draw_rectangle(cell_x + cx as f32 * check, cell_y + cy as f32 * check, check, check, c);
                }
            }

            // Selection/editing highlight (editing only shown when no key color selected)
            let is_selected = state.selected_index == 0;
            let is_editing = state.editing_index == 0 && state.palette_gen_editing.is_none();
            let hovered = ctx.mouse.inside(&cell_rect);

            if is_selected {
                draw_rectangle_lines(cell_x, cell_y, trans_size, trans_size, 2.0, WHITE);
            } else if is_editing {
                draw_rectangle_lines(cell_x, cell_y, trans_size, trans_size, 1.0, Color::new(1.0, 0.8, 0.2, 1.0));
            } else if hovered {
                draw_rectangle_lines(cell_x, cell_y, trans_size, trans_size, 1.0, Color::new(1.0, 1.0, 1.0, 0.3));
            }

            if ctx.mouse.clicked(&cell_rect) {
                state.selected_index = 0;
                state.editing_index = 0;
                state.palette_gen_editing = None; // Deselect key color
            }
            if hovered && ctx.mouse.right_pressed {
                state.editing_index = 0;
                state.palette_gen_editing = None;
            }
        }

        // Draw 3 ramps (5 colors each) to the right of transparent
        let ramp_x = grid_x + trans_size + 4.0;
        for row in 0..3 {
            for col in 0..5 {
                let idx = 1 + row * 5 + col;
                if idx >= texture.palette.len() {
                    break;
                }

                let cell_x = ramp_x + col as f32 * cell_size;
                let cell_y = y + row as f32 * cell_size;
                let cell_rect = Rect::new(cell_x, cell_y, cell_size, cell_size);
                let color15 = texture.palette[idx];

                // Draw color
                let [r, g, b, _] = color15.to_rgba();
                draw_rectangle(cell_x, cell_y, cell_size, cell_size,
                    Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0));

                // STP indicator
                if color15.is_semi_transparent() {
                    let tri_size = (cell_size * 0.4).min(6.0);
                    draw_triangle(
                        Vec2::new(cell_x + cell_size - tri_size, cell_y),
                        Vec2::new(cell_x + cell_size, cell_y),
                        Vec2::new(cell_x + cell_size, cell_y + tri_size),
                        Color::new(0.3, 0.8, 0.9, 0.9),
                    );
                }

                // Selection highlight (editing only shown when no key color selected)
                let is_selected = state.selected_index == idx as u8;
                let is_editing = state.editing_index == idx && state.palette_gen_editing.is_none();
                let hovered = ctx.mouse.inside(&cell_rect);

                if is_selected {
                    draw_rectangle_lines(cell_x, cell_y, cell_size, cell_size, 2.0, WHITE);
                } else if is_editing {
                    draw_rectangle_lines(cell_x, cell_y, cell_size, cell_size, 1.0, Color::new(1.0, 0.8, 0.2, 1.0));
                } else if hovered {
                    draw_rectangle_lines(cell_x, cell_y, cell_size, cell_size, 1.0, Color::new(1.0, 1.0, 1.0, 0.3));
                }

                if ctx.mouse.clicked(&cell_rect) {
                    state.selected_index = idx as u8;
                    state.editing_index = idx;
                    state.palette_gen_editing = None; // Deselect key color
                }
                if hovered && ctx.mouse.right_pressed {
                    state.editing_index = idx;
                    state.palette_gen_editing = None;
                }
            }
        }

        grid_height = trans_size;
    } else {
        // 8-bit: regular 16x16 grid
        let grid_size = 16;
        let cell_size = ((rect.w - padding * 2.0) / grid_size as f32).min(12.0);
        let grid_w = cell_size * grid_size as f32;
        let grid_x = rect.x + (rect.w - grid_w) / 2.0;

        for gy in 0..grid_size {
            for gx in 0..grid_size {
                let idx = gy * grid_size + gx;
                if idx >= texture.palette.len() {
                    break;
                }

                let cell_x = grid_x + gx as f32 * cell_size;
                let cell_y = y + gy as f32 * cell_size;
                let cell_rect = Rect::new(cell_x, cell_y, cell_size, cell_size);
                let color15 = texture.palette[idx];

                // Draw color or checkerboard for transparent
                if color15.is_transparent() {
                    let check = cell_size / 2.0;
                    for cy in 0..2 {
                        for cx in 0..2 {
                            let c = if (cx + cy) % 2 == 0 {
                                Color::new(0.25, 0.25, 0.27, 1.0)
                            } else {
                                Color::new(0.15, 0.15, 0.17, 1.0)
                            };
                            draw_rectangle(cell_x + cx as f32 * check, cell_y + cy as f32 * check, check, check, c);
                        }
                    }
                } else {
                    let [r, g, b, _] = color15.to_rgba();
                    draw_rectangle(cell_x, cell_y, cell_size, cell_size,
                        Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0));
                }

                // Selection highlight (editing only shown when no key color selected)
                let is_selected = state.selected_index == idx as u8;
                let is_editing = state.editing_index == idx && state.palette_gen_editing.is_none();
                let hovered = ctx.mouse.inside(&cell_rect);

                if is_selected {
                    draw_rectangle_lines(cell_x, cell_y, cell_size, cell_size, 2.0, WHITE);
                } else if is_editing {
                    draw_rectangle_lines(cell_x, cell_y, cell_size, cell_size, 1.0, Color::new(1.0, 0.8, 0.2, 1.0));
                } else if hovered {
                    draw_rectangle_lines(cell_x, cell_y, cell_size, cell_size, 1.0, Color::new(1.0, 1.0, 1.0, 0.3));
                }

                if ctx.mouse.clicked(&cell_rect) {
                    state.selected_index = idx as u8;
                    state.editing_index = idx;
                    state.palette_gen_editing = None; // Deselect key color
                }
                if hovered && ctx.mouse.right_pressed {
                    state.editing_index = idx;
                    state.palette_gen_editing = None;
                }
            }
        }

        grid_height = grid_size as f32 * cell_size;
    }

    y += grid_height + 8.0;

    // Color editor - shows RGB sliders for either:
    // 1. Selected key color (palette_gen_editing) - for generator
    // 2. Selected palette index (editing_index) - for direct editing
    let remaining_height = rect.bottom() - y;
    let slider_section_height = 16.0 + 3.0 * (10.0 + 4.0); // label + 3 sliders

    // Determine what we're editing: key color or palette color
    let editing_key_color = state.palette_gen_editing;
    let show_sliders = if editing_key_color.is_some() {
        remaining_height >= slider_section_height
    } else {
        state.editing_index < texture.palette.len() && remaining_height >= slider_section_height
    };

    if show_sliders {
        // Get current RGB values based on what's selected
        let (current_r, current_g, current_b, label_text) = if let Some(key_idx) = editing_key_color {
            let (r, g, b) = state.palette_gen_colors[key_idx];
            (r, g, b, format!("Key {}", key_idx + 1))
        } else {
            let color = texture.palette[state.editing_index];
            (color.r5(), color.g5(), color.b5(), format!("Color {}", state.editing_index))
        };

        // Label
        draw_text(
            &label_text,
            rect.x + padding,
            y + 11.0,
            12.0,
            TEXT_DIM,
        );
        y += 16.0;

        // RGB sliders - constrained to available space
        let slider_w = (rect.w - padding * 2.0 - 40.0).max(40.0);
        let slider_h = 10.0;

        let channels = [
            ("R", current_r, Color::new(0.7, 0.3, 0.3, 1.0), 0),
            ("G", current_g, Color::new(0.3, 0.7, 0.3, 1.0), 1),
            ("B", current_b, Color::new(0.3, 0.3, 0.7, 1.0), 2),
        ];

        for (label, value, tint, slider_idx) in channels {
            // Don't draw if we'd overflow the panel
            if y + slider_h > rect.bottom() - padding {
                break;
            }

            let slider_x = rect.x + padding + 14.0;
            let track_rect = Rect::new(slider_x, y, slider_w, slider_h);

            draw_text(label, rect.x + padding, y + 9.0, 12.0, tint);
            draw_rectangle(track_rect.x, track_rect.y, track_rect.w, track_rect.h, Color::new(0.12, 0.12, 0.14, 1.0));

            let fill_ratio = value as f32 / 31.0;
            draw_rectangle(track_rect.x, track_rect.y, track_rect.w * fill_ratio, slider_h, tint);

            let handle_x = track_rect.x + track_rect.w * fill_ratio - 2.0;
            draw_rectangle(handle_x.max(track_rect.x), track_rect.y, 4.0, slider_h, WHITE);

            draw_text(&format!("{}", value), track_rect.x + track_rect.w + 4.0, y + 9.0, 11.0, TEXT_DIM);

            // Slider interaction
            if ctx.mouse.inside(&track_rect) && ctx.mouse.left_down && state.color_slider.is_none() {
                state.color_slider = Some(slider_idx);
            }

            if state.color_slider == Some(slider_idx) {
                if ctx.mouse.left_down {
                    let rel_x = (ctx.mouse.x - track_rect.x).clamp(0.0, track_rect.w);
                    let new_val = ((rel_x / track_rect.w) * 31.0).round() as u8;

                    if let Some(key_idx) = editing_key_color {
                        // Editing a key color for the generator
                        let (r, g, b) = state.palette_gen_colors[key_idx];
                        state.palette_gen_colors[key_idx] = match slider_idx {
                            0 => (new_val, g, b),
                            1 => (r, new_val, b),
                            _ => (r, g, new_val),
                        };
                    } else {
                        // Editing a palette color
                        let c = texture.palette[state.editing_index];
                        let semi = c.is_semi_transparent();
                        let (r, g, b) = match slider_idx {
                            0 => (new_val, c.g5(), c.b5()),
                            1 => (c.r5(), new_val, c.b5()),
                            _ => (c.r5(), c.g5(), new_val),
                        };
                        texture.palette[state.editing_index] = Color15::new_semi(r, g, b, semi);
                        state.dirty = true;
                    }
                } else {
                    state.color_slider = None;
                }
            }

            y += slider_h + 4.0;
        }

        // Effect checkbox + blend mode dropdown - only show for palette colors (not key colors)
        let show_effect = editing_key_color.is_none() && y + 16.0 <= rect.bottom() - padding;
        if show_effect {
            let color = texture.palette[state.editing_index];
            let checkbox_size = 12.0;
            let checkbox_rect = Rect::new(rect.x + padding, y, checkbox_size, checkbox_size);
            let is_stp = color.is_semi_transparent();

            // Checkbox background
            draw_rectangle(checkbox_rect.x, checkbox_rect.y, checkbox_rect.w, checkbox_rect.h,
                Color::new(0.12, 0.12, 0.14, 1.0));
            draw_rectangle_lines(checkbox_rect.x, checkbox_rect.y, checkbox_rect.w, checkbox_rect.h,
                1.0, Color::new(0.4, 0.4, 0.42, 1.0));

            // Checkmark if checked
            if is_stp {
                draw_rectangle(checkbox_rect.x + 2.0, checkbox_rect.y + 2.0,
                    checkbox_rect.w - 4.0, checkbox_rect.h - 4.0, ACCENT_COLOR);
            }

            // Label "Effect:"
            draw_text("Effect:", checkbox_rect.right() + 4.0, y + 10.0, 12.0, TEXT_COLOR);

            // Blend mode dropdown (right next to checkbox)
            let dropdown_x = checkbox_rect.right() + 48.0;
            let dropdown_w = rect.right() - dropdown_x - padding;
            let dropdown_h = 14.0;
            let dropdown_rect = Rect::new(dropdown_x, y - 1.0, dropdown_w.max(50.0), dropdown_h);

            let blend_names = ["Opaque", "Average", "Add", "Subtract", "Add 25%"];
            let current_idx = match texture.blend_mode {
                BlendMode::Opaque => 0,
                BlendMode::Average => 1,
                BlendMode::Add => 2,
                BlendMode::Subtract => 3,
                BlendMode::AddQuarter => 4,
                BlendMode::Erase => 0,
            };
            let current_name = blend_names[current_idx];

            let hover_dropdown = ctx.mouse.inside(&dropdown_rect);
            let dropdown_bg = if state.blend_dropdown_open || hover_dropdown {
                Color::new(0.28, 0.28, 0.30, 1.0)
            } else {
                Color::new(0.22, 0.22, 0.24, 1.0)
            };
            draw_rectangle(dropdown_rect.x, dropdown_rect.y, dropdown_rect.w, dropdown_rect.h, dropdown_bg);
            draw_text(current_name, dropdown_rect.x + 4.0, dropdown_rect.y + 11.0, 11.0, TEXT_COLOR);

            // Dropdown arrow
            draw_text("\u{25BC}", dropdown_rect.right() - 10.0, dropdown_rect.y + 10.0, 9.0, TEXT_DIM);

            // Click handlers
            let checkbox_click_area = Rect::new(checkbox_rect.x, y, 54.0, 14.0);
            if ctx.mouse.clicked(&checkbox_click_area) {
                texture.palette[state.editing_index].set_semi_transparent(!is_stp);
                state.dirty = true;
            }

            if ctx.mouse.clicked(&dropdown_rect) {
                state.blend_dropdown_open = !state.blend_dropdown_open;
            }

            // Draw dropdown options if open
            if state.blend_dropdown_open {
                let option_h = 18.0;
                let menu_y = dropdown_rect.bottom();
                let menu_h = blend_names.len() as f32 * option_h;

                draw_rectangle(dropdown_rect.x, menu_y, dropdown_rect.w, menu_h, Color::new(0.16, 0.16, 0.18, 1.0));
                draw_rectangle_lines(dropdown_rect.x, menu_y, dropdown_rect.w, menu_h, 1.0, Color::new(0.3, 0.3, 0.32, 1.0));

                for (i, name) in blend_names.iter().enumerate() {
                    let opt_rect = Rect::new(dropdown_rect.x, menu_y + i as f32 * option_h, dropdown_rect.w, option_h);
                    if ctx.mouse.inside(&opt_rect) {
                        draw_rectangle(opt_rect.x, opt_rect.y, opt_rect.w, opt_rect.h, ACCENT_COLOR);
                    }
                    let text_color = if i == current_idx { WHITE } else { TEXT_COLOR };
                    draw_text(name, opt_rect.x + 4.0, opt_rect.y + 13.0, 11.0, text_color);

                    if ctx.mouse.clicked(&opt_rect) {
                        texture.blend_mode = match i {
                            0 => BlendMode::Opaque,
                            1 => BlendMode::Average,
                            2 => BlendMode::Add,
                            3 => BlendMode::Subtract,
                            4 => BlendMode::AddQuarter,
                            _ => BlendMode::Opaque,
                        };
                        state.dirty = true;
                        state.blend_dropdown_open = false;
                    }
                }

                // Close dropdown if clicking outside
                if ctx.mouse.left_pressed && !ctx.mouse.inside(&dropdown_rect) {
                    let menu_rect = Rect::new(dropdown_rect.x, menu_y, dropdown_rect.w, menu_h);
                    if !ctx.mouse.inside(&menu_rect) {
                        state.blend_dropdown_open = false;
                    }
                }
            }
        }
    }
}

/// Draw UV overlay on the texture canvas
///
/// Renders UV wireframe edges and vertex handles for selected faces.
fn draw_uv_overlay(
    _canvas_rect: &Rect,
    tex_width: f32,
    tex_height: f32,
    tex_x: f32,
    tex_y: f32,
    zoom: f32,
    uv_data: &UvOverlayData,
    uv_selection: &[usize],
) {
    const EDGE_COLOR: Color = Color::new(1.0, 0.78, 0.39, 1.0);      // Orange
    const VERTEX_COLOR: Color = Color::new(1.0, 1.0, 1.0, 1.0);      // White
    const SELECTED_COLOR: Color = Color::new(0.39, 0.78, 1.0, 1.0);  // Light blue

    // Convert UV coords (0-1) to screen coords
    let uv_to_screen = |u: f32, v: f32| -> (f32, f32) {
        // U goes left-to-right, V goes top-to-bottom (inverted from typical UV)
        let px = u * tex_width;
        let py = (1.0 - v) * tex_height;
        (tex_x + (px + 0.5) * zoom, tex_y + (py + 0.5) * zoom)
    };

    // Draw selected faces
    for &face_idx in &uv_data.selected_faces {
        if let Some(face) = uv_data.faces.get(face_idx) {
            // Collect screen positions for all vertices
            let screen_uvs: Vec<_> = face.vertex_indices.iter()
                .filter_map(|&vi| uv_data.vertices.get(vi))
                .map(|v| uv_to_screen(v.uv.x, v.uv.y))
                .collect();

            // Draw edges (all edges of n-gon)
            let n = screen_uvs.len();
            for i in 0..n {
                let j = (i + 1) % n;
                draw_line(
                    screen_uvs[i].0, screen_uvs[i].1,
                    screen_uvs[j].0, screen_uvs[j].1,
                    2.0, EDGE_COLOR,
                );
            }

            // Draw vertices
            for (i, &vi) in face.vertex_indices.iter().enumerate() {
                if let Some((sx, sy)) = screen_uvs.get(i) {
                    if let Some(uv_vert) = uv_data.vertices.get(vi) {
                        let is_selected = uv_selection.contains(&uv_vert.vertex_index);
                        let color = if is_selected { SELECTED_COLOR } else { VERTEX_COLOR };
                        let size = if is_selected { 8.0 } else { 6.0 };
                        draw_rectangle(sx - size * 0.5, sy - size * 0.5, size, size, color);
                    }
                }
            }
        }
    }
}

/// Handle UV mode input (vertex selection, transforms)
fn handle_uv_input(
    ctx: &mut UiContext,
    canvas_rect: &Rect,
    texture: &UserTexture,
    state: &mut TextureEditorState,
    uv_data: Option<&UvOverlayData>,
) {
    let uv_data = match uv_data {
        Some(d) => d,
        None => return, // No UV data, nothing to interact with
    };

    let tex_width = texture.width as f32;
    let tex_height = texture.height as f32;

    // Extract values from state to avoid borrow issues with closures
    let zoom = state.zoom;
    let pan_x = state.pan_x;
    let pan_y = state.pan_y;

    // Calculate texture position (same as in draw_texture_canvas)
    let canvas_cx = canvas_rect.x + canvas_rect.w / 2.0;
    let canvas_cy = canvas_rect.y + canvas_rect.h / 2.0;
    let tex_screen_w = tex_width * zoom;
    let tex_screen_h = tex_height * zoom;
    let tex_x = canvas_cx - tex_screen_w / 2.0 + pan_x;
    let tex_y = canvas_cy - tex_screen_h / 2.0 + pan_y;

    // Helper: Convert UV to screen coords
    let uv_to_screen = |u: f32, v: f32| -> (f32, f32) {
        let px = u * tex_width;
        let py = (1.0 - v) * tex_height;
        (tex_x + (px + 0.5) * zoom, tex_y + (py + 0.5) * zoom)
    };

    // Find nearest vertex to a screen position
    let find_nearest_vertex = |sx: f32, sy: f32, threshold: f32| -> Option<usize> {
        let mut nearest: Option<(usize, f32)> = None;
        for uv_vert in &uv_data.vertices {
            let (vx, vy) = uv_to_screen(uv_vert.uv.x, uv_vert.uv.y);
            let dist = ((sx - vx).powi(2) + (sy - vy).powi(2)).sqrt();
            if dist < threshold {
                if nearest.is_none() || dist < nearest.unwrap().1 {
                    nearest = Some((uv_vert.vertex_index, dist));
                }
            }
        }
        nearest.map(|(idx, _)| idx)
    };

    let shift_held = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
    let ctrl_held = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl)
        || is_key_down(KeyCode::LeftSuper) || is_key_down(KeyCode::RightSuper);

    // Ctrl+A: Select all UV vertices
    if ctrl_held && is_key_pressed(KeyCode::A) {
        state.uv_selection.clear();
        for uv_vert in &uv_data.vertices {
            state.uv_selection.push(uv_vert.vertex_index);
        }
        if !state.uv_selection.is_empty() {
            state.set_status(&format!("Selected {} vertices", state.uv_selection.len()));
        }
    }

    // Handle keyboard shortcuts for modal transforms
    if is_key_pressed(KeyCode::G) && !state.uv_selection.is_empty() {
        state.uv_modal_transform = UvModalTransform::Grab;
        state.uv_modal_start_mouse = (ctx.mouse.x, ctx.mouse.y);
        // Store original UVs for all selected vertices
        state.uv_modal_start_uvs.clear();
        for &vi in &state.uv_selection {
            if let Some(uv_vert) = uv_data.vertices.iter().find(|v| v.vertex_index == vi) {
                state.uv_modal_start_uvs.push((vi, uv_vert.uv));
            }
        }
        state.set_status("Grab: Move mouse, click to confirm, Esc to cancel");
    }

    if is_key_pressed(KeyCode::T) && !state.uv_selection.is_empty() {
        // Enter scale pending mode - wait for user to click and drag
        state.uv_modal_transform = UvModalTransform::ScalePending;
        state.uv_modal_start_uvs.clear();
        // Pre-calculate selection center and store original UVs
        let mut center = RastVec2::new(0.0, 0.0);
        let mut count = 0;
        for &vi in &state.uv_selection {
            if let Some(uv_vert) = uv_data.vertices.iter().find(|v| v.vertex_index == vi) {
                center.x += uv_vert.uv.x;
                center.y += uv_vert.uv.y;
                count += 1;
                state.uv_modal_start_uvs.push((vi, uv_vert.uv));
            }
        }
        if count > 0 {
            center.x /= count as f32;
            center.y /= count as f32;
        }
        state.uv_modal_center = center;
        state.set_status("Scale: Click and drag to scale, Esc to cancel");
    }

    // Handle ScalePending → Scale transition when mouse is pressed
    if state.uv_modal_transform == UvModalTransform::ScalePending && ctx.mouse.left_pressed {
        state.uv_modal_transform = UvModalTransform::Scale;
        state.uv_modal_start_mouse = (ctx.mouse.x, ctx.mouse.y);
        state.set_status("Scale: Release to confirm, Esc to cancel");
        return; // Don't process this click as anything else
    }

    // Handle Scale → None transition when mouse is released
    if state.uv_modal_transform == UvModalTransform::Scale && !ctx.mouse.left_down {
        state.uv_modal_transform = UvModalTransform::None;
        state.uv_modal_start_uvs.clear();
        state.set_status("Scale applied");
        return;
    }

    if is_key_pressed(KeyCode::R) && !state.uv_selection.is_empty() {
        state.uv_modal_transform = UvModalTransform::Rotate;
        state.uv_modal_start_mouse = (ctx.mouse.x, ctx.mouse.y);
        state.uv_modal_start_uvs.clear();
        // Calculate selection center
        let mut center = RastVec2::new(0.0, 0.0);
        let mut count = 0;
        for &vi in &state.uv_selection {
            if let Some(uv_vert) = uv_data.vertices.iter().find(|v| v.vertex_index == vi) {
                center.x += uv_vert.uv.x;
                center.y += uv_vert.uv.y;
                count += 1;
                state.uv_modal_start_uvs.push((vi, uv_vert.uv));
            }
        }
        if count > 0 {
            center.x /= count as f32;
            center.y /= count as f32;
        }
        state.uv_modal_center = center;
        state.set_status("Rotate: Move mouse, click to confirm, Esc to cancel");
    }

    // Cancel modal transform with Escape
    if is_key_pressed(KeyCode::Escape) {
        if state.uv_modal_transform != UvModalTransform::None {
            state.uv_modal_transform = UvModalTransform::None;
            state.uv_modal_start_uvs.clear();
            state.set_status("Transform cancelled");
        } else {
            // Clear selection
            state.uv_selection.clear();
        }
    }

    // Confirm modal transform with click (except Scale modes which use drag)
    if ctx.mouse.left_pressed {
        match state.uv_modal_transform {
            UvModalTransform::Grab | UvModalTransform::Rotate => {
                // The transform is applied by the caller (modeler) based on state.uv_modal_start_uvs
                // Here we just clear the modal state
                state.uv_modal_transform = UvModalTransform::None;
                state.uv_modal_start_uvs.clear();
                state.set_status("Transform applied");
                return; // Don't process click as selection
            }
            // ScalePending and Scale are handled separately above
            _ => {}
        }
    }

    // Handle direct drag continuation
    if state.uv_drag_active {
        if ctx.mouse.left_down {
            // Continue dragging - the actual vertex movement is handled by apply_uv_direct_drag in modeler
        } else {
            // Mouse released - end drag
            state.uv_drag_active = false;
            state.uv_drag_start_uvs.clear();
            state.set_status("Drag complete");
        }
        return; // Don't process other input while dragging
    }

    // Vertex selection and drag initiation with click
    if ctx.mouse.left_pressed {
        let click_threshold = 12.0; // Pixels

        if let Some(vi) = find_nearest_vertex(ctx.mouse.x, ctx.mouse.y, click_threshold) {
            let is_already_selected = state.uv_selection.contains(&vi);

            if shift_held {
                // Toggle selection
                if let Some(pos) = state.uv_selection.iter().position(|&x| x == vi) {
                    state.uv_selection.remove(pos);
                } else {
                    state.uv_selection.push(vi);
                }
            } else if is_already_selected {
                // Clicked on already-selected vertex - start dragging all selected
                state.uv_drag_active = true;
                state.uv_drag_start = (ctx.mouse.x, ctx.mouse.y);
                state.uv_drag_start_uvs.clear();
                // Store original UVs for all selected vertices (object_idx=0 since we don't track that here)
                for &sel_vi in &state.uv_selection {
                    if let Some(uv_vert) = uv_data.vertices.iter().find(|v| v.vertex_index == sel_vi) {
                        state.uv_drag_start_uvs.push((0, sel_vi, uv_vert.uv));
                    }
                }
                state.set_status("Dragging vertices (pixel snap enabled)");
            } else {
                // Click on unselected vertex - select and start drag
                state.uv_selection.clear();
                state.uv_selection.push(vi);
                // Start dragging this vertex
                state.uv_drag_active = true;
                state.uv_drag_start = (ctx.mouse.x, ctx.mouse.y);
                state.uv_drag_start_uvs.clear();
                if let Some(uv_vert) = uv_data.vertices.iter().find(|v| v.vertex_index == vi) {
                    state.uv_drag_start_uvs.push((0, vi, uv_vert.uv));
                }
                state.set_status("Dragging vertices (pixel snap enabled)");
            }
        } else if !shift_held {
            // Click on empty space - start box selection or clear selection
            state.uv_box_select_start = Some((ctx.mouse.x, ctx.mouse.y));
        }
    }

    // Box selection drag
    if let Some((start_x, start_y)) = state.uv_box_select_start {
        if ctx.mouse.left_down {
            // Draw box selection rectangle
            let min_x = start_x.min(ctx.mouse.x);
            let min_y = start_y.min(ctx.mouse.y);
            let max_x = start_x.max(ctx.mouse.x);
            let max_y = start_y.max(ctx.mouse.y);
            draw_rectangle_lines(min_x, min_y, max_x - min_x, max_y - min_y, 1.0, Color::new(1.0, 1.0, 1.0, 0.8));
        } else {
            // Mouse released - finalize box selection
            let min_x = start_x.min(ctx.mouse.x);
            let min_y = start_y.min(ctx.mouse.y);
            let max_x = start_x.max(ctx.mouse.x);
            let max_y = start_y.max(ctx.mouse.y);

            // Find all vertices within the box
            if !shift_held {
                state.uv_selection.clear();
            }
            for uv_vert in &uv_data.vertices {
                let (vx, vy) = uv_to_screen(uv_vert.uv.x, uv_vert.uv.y);
                if vx >= min_x && vx <= max_x && vy >= min_y && vy <= max_y {
                    if !state.uv_selection.contains(&uv_vert.vertex_index) {
                        state.uv_selection.push(uv_vert.vertex_index);
                    }
                }
            }

            state.uv_box_select_start = None;
        }
    }
}
