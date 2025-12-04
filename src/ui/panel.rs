//! Resizable panel system
//!
//! Panels can be split horizontally or vertically with draggable dividers.

use macroquad::prelude::*;
use super::{Rect, UiContext};

/// Direction of a split
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SplitDir {
    Horizontal, // Left | Right
    Vertical,   // Top / Bottom
}

/// A split panel that divides space between two children
pub struct SplitPanel {
    pub id: u64,
    pub dir: SplitDir,
    pub ratio: f32,        // 0.0 - 1.0
    pub min_size: f32,     // Minimum size in pixels for each side
    pub divider_size: f32, // Width/height of the draggable divider
}

impl SplitPanel {
    pub fn new(id: u64, dir: SplitDir) -> Self {
        Self {
            id,
            dir,
            ratio: 0.5,
            min_size: 50.0,
            divider_size: 4.0,
        }
    }

    pub fn horizontal(id: u64) -> Self {
        Self::new(id, SplitDir::Horizontal)
    }

    pub fn vertical(id: u64) -> Self {
        Self::new(id, SplitDir::Vertical)
    }

    pub fn with_ratio(mut self, ratio: f32) -> Self {
        self.ratio = ratio.clamp(0.0, 1.0);
        self
    }

    pub fn with_min_size(mut self, min_size: f32) -> Self {
        self.min_size = min_size;
        self
    }

    /// Update and render the split panel
    /// Returns (first_rect, second_rect) for the two child areas
    pub fn update(&mut self, ctx: &mut UiContext, bounds: Rect) -> (Rect, Rect) {
        let divider_rect = self.divider_rect(bounds);

        // Handle divider dragging
        if ctx.mouse.inside(&divider_rect) {
            ctx.set_hot(self.id);
        }

        if ctx.is_hot(self.id) && ctx.mouse.left_pressed {
            ctx.start_drag(self.id);
        }

        if ctx.is_dragging(self.id) {
            // Update ratio based on mouse position
            match self.dir {
                SplitDir::Horizontal => {
                    let new_ratio = (ctx.mouse.x - bounds.x) / bounds.w;
                    self.ratio = self.clamp_ratio(new_ratio, bounds.w);
                }
                SplitDir::Vertical => {
                    let new_ratio = (ctx.mouse.y - bounds.y) / bounds.h;
                    self.ratio = self.clamp_ratio(new_ratio, bounds.h);
                }
            }
        }

        // Draw divider
        let is_hot = ctx.is_hot(self.id) || ctx.is_dragging(self.id);
        let color = if is_hot {
            Color::from_rgba(100, 150, 255, 255)
        } else {
            Color::from_rgba(60, 60, 60, 255)
        };
        draw_rectangle(divider_rect.x, divider_rect.y, divider_rect.w, divider_rect.h, color);

        // Return child rects
        self.child_rects(bounds)
    }

    /// Clamp ratio to respect minimum sizes
    fn clamp_ratio(&self, ratio: f32, total_size: f32) -> f32 {
        let min_ratio = self.min_size / total_size;
        let max_ratio = 1.0 - min_ratio;
        ratio.clamp(min_ratio, max_ratio)
    }

    /// Get the divider rectangle
    fn divider_rect(&self, bounds: Rect) -> Rect {
        match self.dir {
            SplitDir::Horizontal => {
                let x = bounds.x + bounds.w * self.ratio - self.divider_size * 0.5;
                Rect::new(x, bounds.y, self.divider_size, bounds.h)
            }
            SplitDir::Vertical => {
                let y = bounds.y + bounds.h * self.ratio - self.divider_size * 0.5;
                Rect::new(bounds.x, y, bounds.w, self.divider_size)
            }
        }
    }

    /// Get the two child rectangles (excluding divider)
    fn child_rects(&self, bounds: Rect) -> (Rect, Rect) {
        let half_div = self.divider_size * 0.5;

        match self.dir {
            SplitDir::Horizontal => {
                let split = bounds.w * self.ratio;
                (
                    Rect::new(bounds.x, bounds.y, split - half_div, bounds.h),
                    Rect::new(
                        bounds.x + split + half_div,
                        bounds.y,
                        bounds.w - split - half_div,
                        bounds.h,
                    ),
                )
            }
            SplitDir::Vertical => {
                let split = bounds.h * self.ratio;
                (
                    Rect::new(bounds.x, bounds.y, bounds.w, split - half_div),
                    Rect::new(
                        bounds.x,
                        bounds.y + split + half_div,
                        bounds.w,
                        bounds.h - split - half_div,
                    ),
                )
            }
        }
    }
}

/// Draw a panel background with optional title
pub fn draw_panel(rect: Rect, title: Option<&str>, bg_color: Color) {
    // Background
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, bg_color);

    // Border
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, Color::from_rgba(80, 80, 80, 255));

    // Title bar if provided
    if let Some(title) = title {
        let title_height = 20.0;
        draw_rectangle(
            rect.x,
            rect.y,
            rect.w,
            title_height,
            Color::from_rgba(50, 50, 60, 255),
        );
        draw_text(title, rect.x + 5.0, rect.y + 14.0, 16.0, WHITE);
    }
}

/// Get the content area of a panel (after title bar)
pub fn panel_content_rect(rect: Rect, has_title: bool) -> Rect {
    if has_title {
        rect.remaining_after_top(20.0).pad(2.0)
    } else {
        rect.pad(2.0)
    }
}
