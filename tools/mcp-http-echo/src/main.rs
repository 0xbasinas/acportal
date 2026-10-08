//! Minimal MCP server over Streamable HTTP with one `echo` tool, for checking that an
//! agent reaches a phone-supplied HTTP MCP server through acpd. Listens on loopback only.
//! Usage: mcp-http-echo [PORT]   (prints the URL; serves /mcp)
//!
//! The echo reply says whether the request carried an `X-Acpd-Marker` header, without
//! repeating its value, so a test can check that phone-supplied headers reach the server.
use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{common::Extension, router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
struct EchoParams {
    /// The message to echo back
    message: String,
}

#[derive(Clone)]
struct EchoServer {
    #[allow(dead_code)]
    tool_router: ToolRouter<EchoServer>,
}

#[tool_router]
impl EchoServer {
    #[tool(description = "Echoes back the input message")]
    async fn echo(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(params): Parameters<EchoParams>,
    ) -> Result<CallToolResult, McpError> {
        let marker = if parts.headers.contains_key("x-acpd-marker") {
            "present"
        } else {
            "absent"
        };
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "HTTP echo: {} (marker header {marker})",
            params.message
        ))]))
    }
}

#[tool_handler]
impl ServerHandler for EchoServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("mcp-http-echo", "0.1.0"))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let port: u16 = std::env::args()
        .nth(1)
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(0);
    let service: StreamableHttpService<EchoServer, LocalSessionManager> =
        StreamableHttpService::new(
            || {
                Ok(EchoServer {
                    tool_router: EchoServer::tool_router(),
                })
            },
            Default::default(),
            StreamableHttpServerConfig::default(),
        );
    let router = axum::Router::new().nest_service("/mcp", service);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    println!("http://{}/mcp", listener.local_addr()?);
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
