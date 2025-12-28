//! Box Select Drag Tracker
//!
//! Handles rectangular selection in screen space.

/// Tracks a box selection drag operation
#[derive(Debug, Clone)]
pub struct BoxSelectTracker {
    /// Starting mouse position (corner of box)
    pub start_mouse: (f32, f32),
    /// Current mouse position (opposite corner)
    pub current_mouse: (f32, f32),
}

impl BoxSelectTracker {
    pub fn new(start_mouse: (f32, f32)) -> Self {
        Self {
            start_mouse,
            current_mouse: start_mouse,
        }
    }

    /// Get the selection rectangle bounds (min_x, min_y, max_x, max_y)
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        let min_x = self.start_mouse.0.min(self.current_mouse.0);
        let min_y = self.start_mouse.1.min(self.current_mouse.1);
        let max_x = self.start_mouse.0.max(self.current_mouse.0);
        let max_y = self.start_mouse.1.max(self.current_mouse.1);
        (min_x, min_y, max_x, max_y)
    }

    /// Check if a screen point is inside the selection box
    pub fn contains(&self, x: f32, y: f32) -> bool {
        let (min_x, min_y, max_x, max_y) = self.bounds();
        x >= min_x && x <= max_x && y >= min_y && y <= max_y
    }

    /// Get width of the selection box
    pub fn width(&self) -> f32 {
        (self.current_mouse.0 - self.start_mouse.0).abs()
    }

    /// Get height of the selection box
    pub fn height(&self) -> f32 {
        (self.current_mouse.1 - self.start_mouse.1).abs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bounds() {
        let tracker = BoxSelectTracker {
            start_mouse: (100.0, 100.0),
            current_mouse: (50.0, 150.0), // Dragged left and down
        };

        let (min_x, min_y, max_x, max_y) = tracker.bounds();
        assert!((min_x - 50.0).abs() < 0.001);
        assert!((min_y - 100.0).abs() < 0.001);
        assert!((max_x - 100.0).abs() < 0.001);
        assert!((max_y - 150.0).abs() < 0.001);
    }

    #[test]
    fn test_contains() {
        let tracker = BoxSelectTracker {
            start_mouse: (0.0, 0.0),
            current_mouse: (100.0, 100.0),
        };

        assert!(tracker.contains(50.0, 50.0));
        assert!(tracker.contains(0.0, 0.0));
        assert!(tracker.contains(100.0, 100.0));
        assert!(!tracker.contains(-1.0, 50.0));
        assert!(!tracker.contains(101.0, 50.0));
    }
}
