//! Rectangle type for UI layout

/// A rectangle defined by position and size
#[derive(Debug, Clone, Copy, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// Create from screen dimensions
    pub fn screen(width: f32, height: f32) -> Self {
        Self::new(0.0, 0.0, width, height)
    }

    /// Right edge
    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    /// Bottom edge
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    /// Center X
    pub fn center_x(&self) -> f32 {
        self.x + self.w * 0.5
    }

    /// Center Y
    pub fn center_y(&self) -> f32 {
        self.y + self.h * 0.5
    }

    /// Check if point is inside
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    /// Shrink by padding on all sides
    pub fn pad(&self, padding: f32) -> Self {
        Self::new(
            self.x + padding,
            self.y + padding,
            (self.w - padding * 2.0).max(0.0),
            (self.h - padding * 2.0).max(0.0),
        )
    }

    /// Shrink by different padding on each side
    pub fn pad_sides(&self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self::new(
            self.x + left,
            self.y + top,
            (self.w - left - right).max(0.0),
            (self.h - top - bottom).max(0.0),
        )
    }

    /// Split horizontally at ratio (0.0 - 1.0), returns (left, right)
    pub fn split_h(&self, ratio: f32) -> (Self, Self) {
        let split_x = self.w * ratio.clamp(0.0, 1.0);
        (
            Self::new(self.x, self.y, split_x, self.h),
            Self::new(self.x + split_x, self.y, self.w - split_x, self.h),
        )
    }

    /// Split vertically at ratio (0.0 - 1.0), returns (top, bottom)
    pub fn split_v(&self, ratio: f32) -> (Self, Self) {
        let split_y = self.h * ratio.clamp(0.0, 1.0);
        (
            Self::new(self.x, self.y, self.w, split_y),
            Self::new(self.x, self.y + split_y, self.w, self.h - split_y),
        )
    }

    /// Split horizontally at fixed pixel position from left
    pub fn split_h_px(&self, pixels: f32) -> (Self, Self) {
        let split_x = pixels.clamp(0.0, self.w);
        (
            Self::new(self.x, self.y, split_x, self.h),
            Self::new(self.x + split_x, self.y, self.w - split_x, self.h),
        )
    }

    /// Split vertically at fixed pixel position from top
    pub fn split_v_px(&self, pixels: f32) -> (Self, Self) {
        let split_y = pixels.clamp(0.0, self.h);
        (
            Self::new(self.x, self.y, self.w, split_y),
            Self::new(self.x, self.y + split_y, self.w, self.h - split_y),
        )
    }

    /// Get a horizontal slice (for toolbars, status bars)
    pub fn slice_top(&self, height: f32) -> Self {
        Self::new(self.x, self.y, self.w, height.min(self.h))
    }

    /// Get remaining area after slicing top
    pub fn remaining_after_top(&self, height: f32) -> Self {
        let h = height.min(self.h);
        Self::new(self.x, self.y + h, self.w, self.h - h)
    }

    /// Get a horizontal slice from bottom
    pub fn slice_bottom(&self, height: f32) -> Self {
        let h = height.min(self.h);
        Self::new(self.x, self.bottom() - h, self.w, h)
    }

    /// Get remaining area after slicing bottom
    pub fn remaining_after_bottom(&self, height: f32) -> Self {
        let h = height.min(self.h);
        Self::new(self.x, self.y, self.w, self.h - h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_h() {
        let r = Rect::new(0.0, 0.0, 100.0, 50.0);
        let (left, right) = r.split_h(0.3);
        assert!((left.w - 30.0).abs() < 0.001);
        assert!((right.w - 70.0).abs() < 0.001);
        assert!((right.x - 30.0).abs() < 0.001);
    }

    #[test]
    fn test_contains() {
        let r = Rect::new(10.0, 20.0, 100.0, 50.0);
        assert!(r.contains(50.0, 40.0));
        assert!(!r.contains(5.0, 40.0));
        assert!(!r.contains(50.0, 100.0));
    }
}
