//! Standalone MCP Server for Bonnie Engine
//!
//! This lightweight binary connects to a running Bonnie Engine instance
//! via HTTP and exposes its functionality through MCP.

use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::router::tool::ToolRouter,
    model::*,
    tool, tool_handler, tool_router,
    transport::io::stdio,
};

const ENGINE_URL: &str = "http://127.0.0.1:7779";

/// MCP Server that proxies to Bonnie Engine's HTTP API
#[derive(Clone)]
pub struct BonnieMcp {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl BonnieMcp {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    /// Take a screenshot of the current engine view
    #[tool(description = "Capture the current engine window as a PNG image. Returns base64-encoded PNG data.")]
    async fn take_screenshot(&self) -> Result<CallToolResult, McpError> {
        // Fetch screenshot from engine's HTTP server
        match reqwest::get(format!("{}/screenshot", ENGINE_URL)).await {
            Ok(response) => {
                if response.status().is_success() {
                    match response.bytes().await {
                        Ok(png_data) => {
                            use base64::Engine as _;
                            let base64_data = base64::engine::general_purpose::STANDARD.encode(&png_data);
                            Ok(CallToolResult::success(vec![
                                Content::image(base64_data, "image/png".to_string())
                            ]))
                        }
                        Err(e) => Ok(CallToolResult::success(vec![
                            Content::text(format!("Failed to read screenshot data: {}", e))
                        ])),
                    }
                } else {
                    Ok(CallToolResult::success(vec![
                        Content::text(format!("Engine returned error: {}", response.status()))
                    ]))
                }
            }
            Err(_) => {
                Ok(CallToolResult::success(vec![
                    Content::text("Bonnie Engine is not running. Start it with 'cargo run' or run the bonnie-engine binary.".to_string())
                ]))
            }
        }
    }

    /// Get the current editor state
    #[tool(description = "Get information about the current editor state including active tab and status.")]
    async fn get_editor_state(&self) -> Result<CallToolResult, McpError> {
        match reqwest::get(format!("{}/state", ENGINE_URL)).await {
            Ok(response) => {
                if response.status().is_success() {
                    match response.text().await {
                        Ok(text) => Ok(CallToolResult::success(vec![
                            Content::text(text)
                        ])),
                        Err(e) => Ok(CallToolResult::success(vec![
                            Content::text(format!("Failed to read state: {}", e))
                        ])),
                    }
                } else {
                    Ok(CallToolResult::success(vec![
                        Content::text(format!("Engine returned error: {}", response.status()))
                    ]))
                }
            }
            Err(_) => {
                Ok(CallToolResult::success(vec![
                    Content::text("Bonnie Engine is not running. Start it with 'cargo run' or run the bonnie-engine binary.".to_string())
                ]))
            }
        }
    }
}

#[tool_handler]
impl ServerHandler for BonnieMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some("Bonnie Engine MCP Server - Use take_screenshot to see the current engine view".to_string()),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = BonnieMcp::new();

    let transport = stdio();
    let service = server.serve(transport).await?;

    service.waiting().await?;

    Ok(())
}
