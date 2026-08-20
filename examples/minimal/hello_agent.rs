//! Hello Agent — the simplest phi-agent example.
//!
//! This demonstrates the minimum code needed to create an agent,
//! run a turn, and stream the response to the terminal.
//!
//! Run with:
//! ```bash
//! LLM_API_KEY=your-key cargo run --example hello-agent
//! ```

#[path = "../common/mod.rs"]
mod common;

use std::sync::{Arc, Mutex};

use common::client;
use phi_agent::{
    OutputFormat, PhiAgent, PhiAgentConfig, ReasoningEffort, SafetyConfig, base_agent_builder, build_system_prompt,
    create_stdout_renderer,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let model = common::resolve_llm_env().model;
    let llm_client = client();

    // Build agent
    let builder = base_agent_builder(llm_client).system_prompt(build_system_prompt());

    let agent = PhiAgent::build(
        builder,
        PhiAgentConfig {
            model,
            enable_thinking: true,
            thinking_budget: None,
            thinking_effort: ReasoningEffort::Medium,
            safety: SafetyConfig::default(),
            max_turns: None,
        },
    )?;

    // Run one turn
    let session = agent.create_session().await;
    let renderer = Arc::new(Mutex::new(create_stdout_renderer(&OutputFormat::Terminal {
        show_thinking: true,
        show_tool_args: true,
        color: true,
    })));
    let renderer_clone = renderer.clone();

    agent
        .run_turn(session, "Hello! Introduce yourself in one sentence.", move |event| {
            renderer_clone.lock().unwrap().render(event)
        })
        .await?;

    Ok(())
}
