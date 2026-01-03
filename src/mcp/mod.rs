//! MCP (Model Context Protocol) integration for Bonnie Engine
//!
//! Provides an HTTP API that external MCP servers can connect to,
//! enabling AI assistants like Claude to view and control the engine.

/// A pending click event from MCP
#[derive(Clone, Copy)]
pub struct PendingClick {
    pub x: f32,
    pub y: f32,
    pub button: MouseButton,
}

/// Mouse button type
#[derive(Clone, Copy)]
pub enum MouseButton {
    Left,
    Right,
}

/// Shared state for the HTTP server
pub struct HttpState {
    /// Latest screenshot as PNG bytes
    pub screenshot: Option<Vec<u8>>,
    /// Current editor tab
    pub current_tab: String,
    /// Status message
    pub status: String,
    /// Screen dimensions for coordinate mapping
    pub screen_width: f32,
    pub screen_height: f32,
    /// Pending click events from MCP
    pub pending_clicks: Vec<PendingClick>,
}

impl Default for HttpState {
    fn default() -> Self {
        Self {
            screenshot: None,
            current_tab: "Home".to_string(),
            status: "Engine starting...".to_string(),
            screen_width: 960.0,
            screen_height: 720.0,
            pending_clicks: Vec::new(),
        }
    }
}
