//! MCP (Model Context Protocol) integration for Bonnie Engine
//!
//! Provides an HTTP API that external MCP servers can connect to,
//! enabling AI assistants like Claude to view and control the engine.

/// Shared state for the HTTP server
pub struct HttpState {
    /// Latest screenshot as PNG bytes
    pub screenshot: Option<Vec<u8>>,
    /// Current editor tab
    pub current_tab: String,
    /// Status message
    pub status: String,
}

impl Default for HttpState {
    fn default() -> Self {
        Self {
            screenshot: None,
            current_tab: "Home".to_string(),
            status: "Engine starting...".to_string(),
        }
    }
}
