//! Entry point for the Learny MCP server.
//!
//! Speaks MCP over stdio, so an MCP client (Claude Code, Claude Desktop, …)
//! launches it on demand as a child process and talks to it over the pipe.
//! Logs go to stderr — stdout carries the protocol and must stay clean.

mod db;
mod server;

use anyhow::Result;
use rmcp::transport::stdio;
use rmcp::ServiceExt;
use db::Db;
use server::LearnyServer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    tracing::info!("starting learny-mcp on stdio");

    // Fail here, with a clear message on stderr, rather than at the first
    // tool call.
    let db = Db::open()?;
    let service = LearnyServer::new(db).serve(stdio()).await?;
    // Resolves when the client disconnects or cancels.
    let reason = service.waiting().await?;
    tracing::info!(?reason, "learny-mcp stopped");

    Ok(())
}
