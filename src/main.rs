//! rmcp-trash: MCP server for cross-platform trash/recycle bin operations
//!
//! Move files to trash safely. Cross-platform via trash crate.

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters, ServerHandler},
    model::*,
    ErrorData as McpError,
    ServiceExt,
};
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

// === Parameter Types ===

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TrashFileParams {
    #[schemars(description = "Path to the file or directory to move to trash")]
    pub path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TrashFilesParams {
    #[schemars(description = "List of paths to move to trash")]
    pub paths: Vec<String>,
}

// === Server ===

#[derive(Debug)]
pub struct TrashServer {
    pub tool_router: ToolRouter<Self>,
}

impl Default for TrashServer {
    fn default() -> Self {
        Self::new()
    }
}

impl TrashServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[rmcp::tool_router]
impl TrashServer {
    #[rmcp::tool(description = "Move a file or directory to the system trash/recycle bin")]
    pub async fn trash_file(
        &self,
        Parameters(params): Parameters<TrashFileParams>,
    ) -> Result<CallToolResult, McpError> {
        let path = PathBuf::from(&params.path);

        if !path.exists() {
            return Ok(CallToolResult::success(vec![Content::text(
                format!("Path does not exist: {}", params.path)
            )]));
        }

        match trash::delete(&path) {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(
                format!("Moved to trash: {}", params.path)
            )])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(
                format!("Failed to trash: {}", e)
            )])),
        }
    }

    #[rmcp::tool(description = "Move multiple files or directories to the system trash/recycle bin")]
    pub async fn trash_files(
        &self,
        Parameters(params): Parameters<TrashFilesParams>,
    ) -> Result<CallToolResult, McpError> {
        let paths: Vec<PathBuf> = params.paths.iter().map(PathBuf::from).collect();

        // Check which paths exist
        let mut missing: Vec<&str> = Vec::new();
        let mut to_trash: Vec<&PathBuf> = Vec::new();

        for (i, path) in paths.iter().enumerate() {
            if path.exists() {
                to_trash.push(path);
            } else {
                missing.push(&params.paths[i]);
            }
        }

        if to_trash.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "No valid paths to trash"
            )]));
        }

        match trash::delete_all(&to_trash) {
            Ok(()) => {
                let mut msg = format!("Moved {} items to trash", to_trash.len());
                if !missing.is_empty() {
                    msg.push_str(&format!("\nSkipped (not found): {}", missing.join(", ")));
                }
                Ok(CallToolResult::success(vec![Content::text(msg)]))
            }
            Err(e) => Ok(CallToolResult::success(vec![Content::text(
                format!("Failed to trash: {}", e)
            )])),
        }
    }

    #[rmcp::tool(description = "List items currently in the system trash (Linux/Windows only)")]
    pub async fn list_trash(&self) -> Result<CallToolResult, McpError> {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            match trash::os_limited::list() {
                Ok(items) => {
                    if items.is_empty() {
                        Ok(CallToolResult::success(vec![Content::text("Trash is empty")]))
                    } else {
                        let list: Vec<String> = items
                            .iter()
                            .map(|item| item.name.to_string_lossy().into_owned())
                            .collect();
                        Ok(CallToolResult::success(vec![Content::text(
                            format!("Trash contents ({} items):\n{}", items.len(), list.join("\n"))
                        )]))
                    }
                }
                Err(e) => Ok(CallToolResult::success(vec![Content::text(
                    format!("Failed to list trash: {}", e)
                )])),
            }
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            Ok(CallToolResult::success(vec![Content::text(
                "list_trash is not supported on this platform (Linux/Windows only)"
            )]))
        }
    }
}

#[rmcp::tool_handler]
impl ServerHandler for TrashServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "Cross-platform trash/recycle bin operations. Safely delete files with recovery option.".into(),
            ),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    tracing::info!("Starting rmcp-trash server");

    let server = TrashServer::new();
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;

    tracing::info!("rmcp-trash server stopped");
    Ok(())
}
