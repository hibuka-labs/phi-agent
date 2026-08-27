//! ``phi serve`` — MCP Server (stdio / HTTP JSON-RPC 2.0) for external orchestrators.
//!
//! Exposes the agent as a single `run` tool via the Model Context Protocol.
//! External systems (LangGraph, CrewAI, scripts) call `tools/list` to discover
//! the tool, then `tools/call` to delegate tasks.
//!
//! ## Transports
//!
//! - **stdio** (``phi serve``): line-delimited JSON on stdin/stdout, subprocess mode
//! - **HTTP** (``phi serve --http 8080``): POST /mcp with SSE streaming
//!
//! Requires ``--features mcp``.

#![cfg(feature = "mcp")]

use std::sync::Arc;

use agent_works::mcp::{McpServeConfig, McpServerTransport};
use phi_agent::config::resolve_llm_config;
use phi_agent::{
    ApprovalMode, AutoApprovalHandler, PhiAgent, SafetyConfig, TurnFactMiddleware, TurnToolLimitMiddleware,
    base_agent_builder, build_system_prompt,
};

/// Run the MCP server.
///
/// - `http: None` → stdio transport
/// - `http: Some(port)` → HTTP transport on the given port
pub async fn run(http: Option<u16>) -> anyhow::Result<()> {
    // 1. Resolve LLM config
    let llm_config = resolve_llm_config(None, None)?;
    let llm_client = llm_unified::create_provider(&agent_base::llm_trait::LlmConfig {
        protocol: None,
        api_key: llm_config.api_key.clone(),
        model: llm_config.model.clone(),
        base_url: llm_config.base_url.clone(),
        options: std::collections::HashMap::new(),
    })?;

    // 2. Build agent
    let builder = base_agent_builder(llm_client)
        .system_prompt(build_system_prompt())
        .approval_handler(Arc::new(AutoApprovalHandler::new(ApprovalMode::Auto)))
        .middleware(TurnFactMiddleware::new())
        .middleware(TurnToolLimitMiddleware::from_config(&SafetyConfig::default()));

    let agent =
        PhiAgent::build(builder, phi_agent::PhiAgentConfig { model: llm_config.model.clone(), ..Default::default() })?;

    // 3. Configure transport
    let transport = match http {
        Some(port) => McpServerTransport::Http { host: "127.0.0.1".to_string(), port },
        None => McpServerTransport::Stdio,
    };

    let config =
        McpServeConfig { name: "phi-agent".to_string(), version: env!("CARGO_PKG_VERSION").to_string(), transport };

    // 4. Serve
    match &config.transport {
        McpServerTransport::Stdio => {
            eprintln!("phi-agent MCP server ready (stdio). Waiting for JSON-RPC requests on stdin...");
        },
        McpServerTransport::Http { host, port } => {
            eprintln!("phi-agent MCP server ready (HTTP). Listening on http://{host}:{port}/mcp");
        },
    }

    let server = agent.into_mcp_server(config);
    server.serve().await?;

    Ok(())
}
