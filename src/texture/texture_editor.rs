//! Unified texture editor component
//!
//! Provides a reusable UI component for editing indexed textures with:
//! - Canvas with zoom/pan
//! - Drawing tools (pencil, brush, fill, shapes)
//! - Palette editing with RGB555 sliders
//! - Undo/redo support

use macroquad::prelude::*;
use crate::rasterizer::{ClutDepth, Color15};
use crate::ui::{Rect, UiContext, icon};
use super::user_texture::UserTexture;

// UI constants
const TEXT_COLOR: Color = Color::new(0.85, 0.85, 0.85, 1.0);
const TEXT_DIM: Color = Color::new(0.55, 0.55, 0.55, 1.0);
const ACCENT_COLOR: Color = Color::new(0.28, 0.51, 0.71, 1.0);
const PANEL_BG: Color = Color::new(0.18, 0.18, 0.20, 1.0);

/// Drawing tool types
/// Note: No eraser - paint with index 0 (transparent) to erase
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DrawTool {
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

impl DrawTool {
    /// Icon for this tool
    pub fn icon(&self) -> char {
        match self {
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

/// State for the texture editor
#[derive(Debug)]
pub struct TextureEditorState {
    /// Currently selected drawing tool
    pub tool: DrawTool,
    /// Brush size (1-16 pixels)
    pub brush_size: u8,
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
}

impl Default for TextureEditorState {
    fn default() -> Self {
        Self {
            tool: DrawTool::Brush,
            brush_size: 1,
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
        }
    }
}

impl TextureEditorState {
    /// Create a new editor state
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset the editor state for a new texture
    pub fn reset(&mut self) {
        self.tool = DrawTool::Brush;
        self.brush_size = 1;
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

/// Draw a brush stroke (square brush)
fn tex_draw_brush(texture: &mut UserTexture, cx: i32, cy: i32, size: u8, index: u8) {
    let half = (size as i32 - 1) / 2;
    for dy in 0..size as i32 {
        for dx in 0..size as i32 {
            tex_draw_pixel(texture, cx - half + dx, cy - half + dy, index);
        }
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

/// Draw the texture canvas
pub fn draw_texture_canvas(
    ctx: &mut UiContext,
    canvas_rect: Rect,
    texture: &mut UserTexture,
    state: &mut TextureEditorState,
) {
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
    let check_size = (state.zoom * 2.0).max(4.0);
    let clip_x = tex_x.max(canvas_rect.x);
    let clip_y = tex_y.max(canvas_rect.y);
    let end_x = (tex_x + tex_screen_w).min(canvas_rect.x + canvas_rect.w);
    let end_y = (tex_y + tex_screen_h).min(canvas_rect.y + canvas_rect.h);

    // Calculate the first row/col indices that are visible
    let first_row = ((clip_y - tex_y) / check_size).floor() as i32;
    let first_col = ((clip_x - tex_x) / check_size).floor() as i32;

    // Start drawing from the actual grid position (may be before clip region)
    let mut row = first_row;
    let mut cy = tex_y + first_row as f32 * check_size;
    while cy < end_y {
        let mut col = first_col;
        let mut cx = tex_x + first_col as f32 * check_size;
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

    // Draw texture pixels
    for py in 0..texture.height {
        for px in 0..texture.width {
            let screen_x = tex_x + px as f32 * state.zoom;
            let screen_y = tex_y + py as f32 * state.zoom;

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
                    Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0),
                );
            }
        }
    }

    // Draw pixel grid at high zoom (when enabled)
    if state.show_grid && state.zoom >= 4.0 {
        let grid_color = Color::new(1.0, 1.0, 1.0, 0.1);
        // Vertical lines
        for px in 0..=texture.width {
            let x = tex_x + px as f32 * state.zoom;
            if x >= canvas_rect.x && x <= canvas_rect.x + canvas_rect.w {
                draw_line(x, tex_y.max(canvas_rect.y), x, (tex_y + tex_screen_h).min(canvas_rect.y + canvas_rect.h), 1.0, grid_color);
            }
        }
        // Horizontal lines
        for py in 0..=texture.height {
            let y = tex_y + py as f32 * state.zoom;
            if y >= canvas_rect.y && y <= canvas_rect.y + canvas_rect.h {
                draw_line(tex_x.max(canvas_rect.x), y, (tex_x + tex_screen_w).min(canvas_rect.x + canvas_rect.w), y, 1.0, grid_color);
            }
        }
    }

    // Draw texture border
    draw_rectangle_lines(tex_x, tex_y, tex_screen_w, tex_screen_h, 1.0, Color::new(0.5, 0.5, 0.5, 1.0));

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

    // Zoom with scroll wheel (gentle 8% per tick)
    if inside && ctx.mouse.scroll != 0.0 {
        let old_zoom = state.zoom;
        let zoom_factor = 1.08f32;
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

    // Drawing
    if inside && !state.panning {
        if let Some((px, py)) = screen_to_texture(ctx.mouse.x, ctx.mouse.y, &canvas_rect, texture, state) {
            // Show cursor preview
            if px >= 0 && py >= 0 && (px as usize) < texture.width && (py as usize) < texture.height {
                let cursor_x = tex_x + px as f32 * state.zoom;
                let cursor_y = tex_y + py as f32 * state.zoom;

                if state.tool.uses_brush_size() {
                    let size = state.brush_size as f32 * state.zoom;
                    let half = ((state.brush_size as f32 - 1.0) / 2.0) * state.zoom;
                    draw_rectangle_lines(
                        cursor_x - half,
                        cursor_y - half,
                        size,
                        size,
                        1.0,
                        Color::new(1.0, 1.0, 1.0, 0.5),
                    );
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

                if state.tool.is_shape_tool() {
                    state.shape_start = Some((px, py));
                } else {
                    // Save undo state at start of stroke
                    state.save_undo(texture, &format!("{:?}", state.tool));

                    match state.tool {
                        DrawTool::Brush => {
                            tex_draw_brush(texture, px, py, state.brush_size, state.selected_index);
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
                                tex_draw_brush(texture, ix, iy, state.brush_size, state.selected_index);
                            }
                        }
                        state.last_draw_pos = Some((px, py));
                    }
                }
            }

            // Shape preview
            if state.drawing && state.tool.is_shape_tool() {
                if let Some((sx, sy)) = state.shape_start {
                    // Draw preview (using current color as overlay)
                    let color = texture.get_palette_color(state.selected_index);
                    let [r, g, b, _] = color.to_rgba();
                    let preview_color = Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 0.5);

                    // Simple preview rectangle
                    let x0 = tex_x + sx.min(px) as f32 * state.zoom;
                    let y0 = tex_y + sy.min(py) as f32 * state.zoom;
                    let w = ((sx - px).abs() + 1) as f32 * state.zoom;
                    let h = ((sy - py).abs() + 1) as f32 * state.zoom;
                    draw_rectangle_lines(x0, y0, w, h, 2.0, preview_color);
                }
            }

            // Complete shape on release
            if !ctx.mouse.left_down && state.drawing {
                if state.tool.is_shape_tool() {
                    if let Some((sx, sy)) = state.shape_start {
                        state.save_undo(texture, &format!("{:?}", state.tool));

                        match state.tool {
                            DrawTool::Line => {
                                // Line uses brush_size as thickness (for now just 1px)
                                tex_draw_line(texture, sx, sy, px, py, state.selected_index);
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
    if draw_toggle_button_small(ctx, col1_x + btn_size + gap, y, btn_size, icon::GRID, "Toggle grid", state.show_grid, icon_font) {
        state.show_grid = !state.show_grid;
    }
    y += btn_size + gap;

    // Separator before tools
    y += 2.0;
    draw_line(col1_x, y, col2_x + btn_size, y, 1.0, Color::new(0.3, 0.3, 0.32, 1.0));
    y += 4.0;

    // === Drawing tools in 2-column grid ===
    // Note: No Eraser tool - just paint with index 0 (transparent) to erase
    let mut clicked_tool: Option<DrawTool> = None;
    let all_tools = [
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

    // === Tool options section (size, fill toggle) ===
    // Show for tools that use brush size OR shape tools
    let show_options = state.tool.uses_brush_size() || matches!(state.tool, DrawTool::Rectangle | DrawTool::Ellipse);
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

    // Palette grid
    let grid_size = match texture.depth {
        ClutDepth::Bpp4 => 4,  // 4x4 = 16 colors
        ClutDepth::Bpp8 => 16, // 16x16 = 256 colors
    };

    // Make palette cells bigger - use available width, max 24px per cell
    let cell_size = ((rect.w - padding * 2.0) / grid_size as f32).min(24.0);
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
                        draw_rectangle(
                            cell_x + cx as f32 * check,
                            cell_y + cy as f32 * check,
                            check,
                            check,
                            c,
                        );
                    }
                }
            } else {
                let [r, g, b, _] = color15.to_rgba();
                draw_rectangle(
                    cell_x,
                    cell_y,
                    cell_size,
                    cell_size,
                    Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0),
                );
            }

            // Selection highlight
            let is_selected = state.selected_index == idx as u8;
            let is_editing = state.editing_index == idx;
            let hovered = ctx.mouse.inside(&cell_rect);

            if is_selected {
                draw_rectangle_lines(cell_x, cell_y, cell_size, cell_size, 2.0, WHITE);
            } else if is_editing {
                draw_rectangle_lines(cell_x, cell_y, cell_size, cell_size, 1.0, Color::new(1.0, 0.8, 0.2, 1.0));
            } else if hovered {
                draw_rectangle_lines(cell_x, cell_y, cell_size, cell_size, 1.0, Color::new(1.0, 1.0, 1.0, 0.3));
            }

            // Click to select for drawing
            if ctx.mouse.clicked(&cell_rect) {
                state.selected_index = idx as u8;
                state.editing_index = idx;
            }

            // Right-click to edit color
            if hovered && ctx.mouse.right_pressed {
                state.editing_index = idx;
            }
        }
    }

    y += grid_size as f32 * cell_size + 8.0;

    // Color editor for editing_index - only show if there's enough space
    let remaining_height = rect.bottom() - y;
    let slider_section_height = 14.0 + 3.0 * (10.0 + 4.0); // label + 3 sliders

    if state.editing_index < texture.palette.len() && remaining_height >= slider_section_height {
        let color = texture.palette[state.editing_index];

        // Index label
        draw_text(
            &format!("Color {}", state.editing_index),
            rect.x + padding,
            y + 10.0,
            10.0,
            TEXT_DIM,
        );
        y += 14.0;

        // RGB sliders - constrained to available space
        let slider_w = (rect.w - padding * 2.0 - 40.0).max(40.0);
        let slider_h = 10.0;

        let channels = [
            ("R", color.r5(), Color::new(0.7, 0.3, 0.3, 1.0), 0),
            ("G", color.g5(), Color::new(0.3, 0.7, 0.3, 1.0), 1),
            ("B", color.b5(), Color::new(0.3, 0.3, 0.7, 1.0), 2),
        ];

        for (label, value, tint, slider_idx) in channels {
            // Don't draw if we'd overflow the panel
            if y + slider_h > rect.bottom() - padding {
                break;
            }

            let slider_x = rect.x + padding + 14.0;
            let track_rect = Rect::new(slider_x, y, slider_w, slider_h);

            draw_text(label, rect.x + padding, y + 8.0, 10.0, tint);
            draw_rectangle(track_rect.x, track_rect.y, track_rect.w, track_rect.h, Color::new(0.12, 0.12, 0.14, 1.0));

            let fill_ratio = value as f32 / 31.0;
            draw_rectangle(track_rect.x, track_rect.y, track_rect.w * fill_ratio, slider_h, tint);

            let handle_x = track_rect.x + track_rect.w * fill_ratio - 2.0;
            draw_rectangle(handle_x.max(track_rect.x), track_rect.y, 4.0, slider_h, WHITE);

            draw_text(&format!("{}", value), track_rect.x + track_rect.w + 4.0, y + 8.0, 10.0, TEXT_DIM);

            // Slider interaction
            if ctx.mouse.inside(&track_rect) && ctx.mouse.left_down && state.color_slider.is_none() {
                state.color_slider = Some(slider_idx);
            }

            if state.color_slider == Some(slider_idx) {
                if ctx.mouse.left_down {
                    let rel_x = (ctx.mouse.x - track_rect.x).clamp(0.0, track_rect.w);
                    let new_val = ((rel_x / track_rect.w) * 31.0).round() as u8;

                    let c = texture.palette[state.editing_index];
                    let semi = c.is_semi_transparent();
                    let (r, g, b) = match slider_idx {
                        0 => (new_val, c.g5(), c.b5()),
                        1 => (c.r5(), new_val, c.b5()),
                        _ => (c.r5(), c.g5(), new_val),
                    };
                    texture.palette[state.editing_index] = Color15::new_semi(r, g, b, semi);
                    state.dirty = true;
                } else {
                    state.color_slider = None;
                }
            }

            y += slider_h + 4.0;
        }
    }
}
