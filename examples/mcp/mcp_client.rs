//! MCP Client — demonstrate connecting to an MCP server at build time.
//!
//! This example shows how to configure and connect to an MCP server
//! when building the agent. Tools from the server are discovered and
//! registered automatically with the `mcp.<server>.<tool>` naming convention.
//!
//! Usage:
//!   cargo run --features mcp --example mcp_client
//!
//! Prerequisites:
//!   - A running MCP server (e.g. a local stdio server or HTTP endpoint)
//!   - Update the MCP_SERVER_COMMAND / MCP_SERVER_URL below to match your server

use std::sync::{Arc, Mutex};

use agent_works::mcp::{McpServerConfig, McpTransport};
use phi_agent::{PhiAgent, PhiAgentConfig, ReasoningEffort, SafetyConfig, base_agent_builder, build_system_prompt};

#[path = "../common/mod.rs"]
mod common;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // ── 1. Create LLM client ──
    let llm_client = common::client();

    // ── 2. Configure MCP server ──
    //
    // stdio transport (local process):
    let _mcp_config = McpServerConfig {
        name: "my-server".into(),
        transport: McpTransport::Stdio {
            command: "echo".into(), // replace with your MCP server binary
            args: vec![],
        },
        auto_reconnect: false,
    };

    // Alternative: HTTP transport
    // let mcp_config = McpServerConfig {
    //     name: "my-server".into(),
    //     transport: McpTransport::Http {
    //         url: "http://localhost:3000/mcp".into(),
    //     },
    //     auto_reconnect: false,
    // };

    // ── 3. Build agent ──
    let builder = base_agent_builder(llm_client.clone()).system_prompt(build_system_prompt());

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

    // ── 4. Attach MCP server at runtime ──
    //    Tools are auto-discovered and registered as `mcp.my-server.<tool>`.
    match agent
        .attach_mcp(phi_agent::McpServerConfig {
            name: "my-server".into(),
            transport: phi_agent::McpTransport::Stdio {
                command: "echo".into(), // replace with your MCP server binary
                args: vec![],
            },
            auto_reconnect: false,
        })
        .await
    {
        Ok(()) => println!("MCP server attached successfully"),
        Err(e) => eprintln!("Could not attach MCP server (expected without a real server): {e}"),
    }

    let tools = agent.list_tools().await;
    println!("Agent has {} tools:", tools.len());
    for tool in &tools {
        println!("  - {}", tool.name);
    }

    // ── 5. Run ──
    let session = agent.create_session().await;
    let renderer = Arc::new(Mutex::new(phi_agent::create_stdout_renderer(&phi_agent::OutputFormat::Terminal {
        show_thinking: true,
        show_tool_args: true,
        color: true,
    })));
    let renderer_clone = renderer.clone();

    println!("\n=== Agent ready with MCP tools ===\n");
    agent
        .run_turn(session, "List your available tools", move |event| renderer_clone.lock().unwrap().render(event))
        .await?;

    Ok(())
}
