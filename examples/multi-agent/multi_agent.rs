//! Multi-Agent example — demonstrates sub-agent spawning and lifecycle.
//!
//! This example enables multi-agent mode, which gives the LLM 6 tools
//! (spawn_agent, send_message, followup_task, wait_agent, list_agents,
//! close_agent) for dynamic task decomposition.
//!
//! The LLM can spawn sub-agents that run concurrently, communicate with
//! them, collect results, and close them when done.
//!
//! Run with:
//! ```bash
//! LLM_API_KEY=your-key cargo run --example multi-agent
//! ```

#[path = "../common/mod.rs"]
mod common;

use std::sync::{Arc, Mutex};

use agent_works::multi_agent::MultiAgentConfig;
use common::client;
use phi_agent::{OutputFormat, build_system_prompt, create_stdout_renderer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let llm_client = client();

    // Build with agent-works AgentBuilder to enable multi-agent mode.
    // MultiAgentConfig::enabled() sets max_sub_agents=8, max_agent_depth=1.
    let runtime = agent_works::AgentBuilder::new(llm_client)
        .system_prompt(build_system_prompt())
        .with_multi_agent(MultiAgentConfig::enabled())
        .build()?;

    println!("🤖 Multi-Agent mode enabled. The LLM can spawn sub-agents to decompose tasks.\n");

    let session = runtime.create_session().await;
    let renderer = Arc::new(Mutex::new(create_stdout_renderer(&OutputFormat::Terminal {
        show_thinking: true,
        show_tool_args: true,
        color: true,
    })));
    let renderer_clone = renderer.clone();

    let _outcome = runtime
        .run_turn(
            session,
            "Research two topics in parallel: (1) Rust async programming best practices, \
             (2) tokio runtime internals. For each topic, summarize the key points in 2-3 bullet points.",
            move |event| renderer_clone.lock().unwrap().render(event),
        )
        .await?;

    Ok(())
}
