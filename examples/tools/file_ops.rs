//! File Operations — demonstrate read_file, write_file, and list_files tools.
//!
//! This example shows the agent using file-system tools to:
//! - List directory contents with `list_files`
//! - Read file content with `read_file`
//! - Create and modify files with `write_file`
//!
//! File tools are enabled by default (feature gate `file`).
//! Together they give the LLM the ability to explore, read, and modify the
//! file system — the foundation for Skills (prompt-injection mode) and
//! Memory (persistent file-based).
//!
//! ## Usage
//!
//! ```bash
//! cargo run --example file_ops
//!
//! # Without file tools (opt out):
//! cargo run --no-default-features --example file_ops
//! ```

use std::sync::{Arc, Mutex};

use phi_agent::{PhiAgent, PhiAgentConfig, ReasoningEffort, SafetyConfig, base_agent_builder, build_system_prompt};

#[path = "../common/mod.rs"]
mod common;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // ── 1. Create LLM client ──
    let llm_client = common::client();

    // ── 2. Build agent with defaults (file tools included) ──
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

    // ── 3. List registered tools ──
    let tools = agent.list_tools().await;
    println!("Registered {} tools:", tools.len());
    for tool in &tools {
        println!("  - {}", tool.name);
    }

    // ── 4. Run a file-system task ──
    //
    // The agent can read/write/list files in the working directory.
    // Try tasks like:
    //   - "List all .rs files in src/"
    //   - "Read Cargo.toml and tell me the dependencies"
    //   - "Create a file named hello.txt with content 'Hello, phi!'"
    let session = agent.create_session().await;
    let renderer = Arc::new(Mutex::new(phi_agent::create_stdout_renderer(&phi_agent::OutputFormat::Terminal {
        show_thinking: true,
        show_tool_args: true,
        color: true,
    })));
    let renderer_clone = renderer.clone();

    println!("\n=== Agent ready with file tools ===\n");
    agent
        .run_turn(
            session,
            "List the files in the current directory and tell me what kind of project this is.",
            move |event| renderer_clone.lock().unwrap().render(event),
        )
        .await?;

    Ok(())
}
