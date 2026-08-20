//! Landing — the exact code shown on the homepage.
//!
//! Proves that the docs.phiagent.dev front-page snippet compiles and runs.
//!
//! Run with:
//! ```bash
//! cargo run --example landing
//! ```
//! (reads LLM config from .env)

#[path = "../common/mod.rs"]
mod common;

use std::sync::{Arc, Mutex};

use agent_base::{AgentResult, Content, Tool, ToolContext};
use async_trait::async_trait;
use phi_agent::{
    OutputFormat, PhiAgent, PhiAgentConfig, base_agent_builder, build_system_prompt, create_stdout_renderer,
};
use serde_json::{Value, json};

// ── Your tool: controls the air conditioner ──

struct SmartAc;

#[async_trait]
impl Tool for SmartAc {
    fn name(&self) -> &'static str {
        "control_ac"
    }

    fn description(&self) -> &'static str {
        "Adjust the air conditioner temperature"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "temperature": {
                    "type": "integer",
                    "description": "Target temperature in Celsius, e.g. 26"
                }
            },
            "required": ["temperature"]
        })
    }

    async fn call(&self, args: &Value, _ctx: &ToolContext) -> AgentResult<Vec<Content>> {
        let temp = args["temperature"].as_i64().unwrap_or(26);
        Ok(vec![Content::text(format!("Air conditioner set to {temp}°C"))])
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let llm = common::client();

    // Your system prompt: domain role + framework orchestration
    let system_prompt = format!(
        "You control a smart home. When the user says they are cold, raise the temperature. When they are hot, lower it. Use the control_ac tool.\n\n\
         {}",
        // framework: agent loop, tool routing, sessions
        build_system_prompt()
    );

    // ── Homepage snippet ───────────────────────────────────────────────
    let agent = PhiAgent::build(
        base_agent_builder(llm)
            .system_prompt(system_prompt)
            // your tool
            .register_tool(SmartAc),
        PhiAgentConfig::default(),
    )?;

    let session = agent.create_session().await;
    let renderer = Arc::new(Mutex::new(create_stdout_renderer(&OutputFormat::default())));
    let renderer_clone = renderer.clone();
    agent
        .run_turn(session, "I feel a bit cold", move |e| renderer_clone.lock().unwrap().render(e))
        .await?;
    // ────────────────────────────────────────────────────────────────────

    Ok(())
}
