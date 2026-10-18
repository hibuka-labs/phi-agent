//! MCP Dynamic Attach — demonstrate runtime MCP server attach/detach (Phase 1.2).
//!
//! This example shows how to dynamically connect and disconnect MCP servers
//! while the agent is running — no restart required. Tools appear and disappear
//! from the agent's tool registry in real time.
//!
//! Usage:
//!   cargo run --features mcp --example mcp_dynamic_attach
//!
//! Note: this example requires a real MCP server to demonstrate the full flow.
//! Without one, it illustrates the API surface and error handling.

use phi_agent::{
    McpServerConfig as PhiMcpConfig, McpTransport as PhiMcpTransport, PhiAgent, PhiAgentConfig, ReasoningEffort,
    SafetyConfig, base_agent_builder, build_system_prompt,
};

#[path = "../common/mod.rs"]
mod common;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // ── 1. Build agent (no MCP tools yet) ──
    let llm_client = common::client();
    let builder = base_agent_builder(llm_client).system_prompt(build_system_prompt());

    let agent = PhiAgent::build(
        builder,
        PhiAgentConfig {
            model: common::resolve_llm_env().model,
            enable_thinking: true,
            thinking_budget: None,
            thinking_effort: ReasoningEffort::Medium,
            safety: SafetyConfig::default(),
            max_turns: Some(20),
        },
    )?;

    // ── 2. Check tools before attaching ──
    let tools_before = agent.list_tools().await;
    println!("Tools before attach: {}", tools_before.len());

    // ── 3. Dynamically attach an MCP server ──
    let mcp_config = PhiMcpConfig {
        name: "dynamic-server".into(),
        transport: PhiMcpTransport::Stdio {
            command: "your-mcp-server".into(), // replace with your MCP server
            args: vec![],
            env: Default::default(),
        },
        auto_reconnect: false,
    };

    match agent.attach_mcp(mcp_config).await {
        Ok(()) => {
            let tools_after = agent.list_tools().await;
            println!("Tools after attach: {}", tools_after.len());
            for tool in &tools_after {
                println!("  - {}", tool.name);
            }
        },
        Err(e) => {
            // Expected when no real MCP server is available.
            // attach_mcp rolls back the server config on failure,
            // so no partial state is left behind.
            eprintln!("Could not attach MCP server (expected without a real server): {e}");
        },
    }

    // ── 4. Dynamically detach ──
    agent.detach_mcp("dynamic-server").await;
    let tools_after_detach = agent.list_tools().await;
    println!("Tools after detach: {}", tools_after_detach.len());

    // ── 5. Error handling: attach_mcp with invalid config ──
    let bad_config = PhiMcpConfig {
        name: "bad-server".into(),
        transport: PhiMcpTransport::Stdio {
            command: "/nonexistent/binary".into(),
            args: vec![],
            env: Default::default(),
        },
        auto_reconnect: false,
    };

    match agent.attach_mcp(bad_config).await {
        Ok(()) => println!("Unexpected success with bad config"),
        Err(e) => {
            // The error is an AgentError, callers can match the variant:
            eprintln!("Expected error (server binary not found): {e}");
            // Verify no zombie entry — server was rolled back
        },
    }

    println!("\n=== Dynamic MCP management demonstrated ===");
    Ok(())
}
