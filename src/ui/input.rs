//! Input state for UI interaction

use super::Rect;

/// Mouse button state
#[derive(Debug, Clone, Copy, Default)]
pub struct MouseState {
    pub x: f32,
    pub y: f32,
    pub left_down: bool,
    pub right_down: bool,
    pub left_pressed: bool,  // Just pressed this frame
    pub left_released: bool, // Just released this frame
    pub scroll: f32,         // Scroll wheel delta
}

impl MouseState {
    /// Check if mouse is inside a rect
    pub fn inside(&self, rect: &Rect) -> bool {
        rect.contains(self.x, self.y)
    }

    /// Check if mouse is clicking inside a rect
    pub fn clicking(&self, rect: &Rect) -> bool {
        self.left_down && rect.contains(self.x, self.y)
    }

    /// Check if mouse just clicked inside a rect
    pub fn clicked(&self, rect: &Rect) -> bool {
        self.left_pressed && rect.contains(self.x, self.y)
    }
}

/// UI context passed through the frame
pub struct UiContext {
    pub mouse: MouseState,
    /// ID of the widget currently being dragged (if any)
    pub dragging: Option<u64>,
    /// ID of the widget that is "hot" (mouse hovering)
    pub hot: Option<u64>,
    /// Counter for generating unique IDs
    id_counter: u64,
}

impl UiContext {
    pub fn new() -> Self {
        Self {
            mouse: MouseState::default(),
            dragging: None,
            hot: None,
            id_counter: 0,
        }
    }

    /// Generate a unique ID for a widget
    pub fn next_id(&mut self) -> u64 {
        self.id_counter += 1;
        self.id_counter
    }

    /// Reset at start of frame (call before UI code)
    pub fn begin_frame(&mut self, mouse: MouseState) {
        self.mouse = mouse;
        self.hot = None;
        self.id_counter = 0;

        // Clear dragging if mouse released
        if !self.mouse.left_down {
            self.dragging = None;
        }
    }

    /// Check if this widget is being dragged
    pub fn is_dragging(&self, id: u64) -> bool {
        self.dragging == Some(id)
    }

    /// Start dragging a widget
    pub fn start_drag(&mut self, id: u64) {
        self.dragging = Some(id);
    }

    /// Set hot widget (hovering)
    pub fn set_hot(&mut self, id: u64) {
        // Only set hot if not dragging something else
        if self.dragging.is_none() || self.dragging == Some(id) {
            self.hot = Some(id);
        }
    }

    /// Check if widget is hot
    pub fn is_hot(&self, id: u64) -> bool {
        self.hot == Some(id)
    }
}

impl Default for UiContext {
    fn default() -> Self {
        Self::new()
    }
}
