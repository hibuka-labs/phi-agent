//! Multi-Tool — register multiple tools and let the agent choose.
//!
//! Demonstrates registering several tools and shows how the agent decides
//! which tool to use based on the user's request.
//!
//! Run with:
//! ```bash
//! LLM_API_KEY=your-key cargo run --example multi-tool
//! ```

#[path = "../common/mod.rs"]
mod common;

use agent_base::{AgentResult, Content, Tool, ToolContext};
use async_trait::async_trait;
use common::client;
use phi_agent::{
    OutputFormat, PhiAgent, PhiAgentConfig, ReasoningEffort, SafetyConfig, base_agent_builder, build_system_prompt,
    create_stdout_renderer,
};
use serde_json::{Value, json};

// ── Tool: System Info ──

struct SystemInfoTool;

#[async_trait]
impl Tool for SystemInfoTool {
    fn name(&self) -> &'static str {
        "system_info"
    }

    fn description(&self) -> &'static str {
        "Get current system information (OS, time, working directory)"
    }

    fn schema(&self) -> Value {
        json!({ "type": "object", "properties": {}, "required": [] })
    }

    async fn call(&self, _args: &Value, _ctx: &ToolContext) -> AgentResult<Vec<Content>> {
        let info = format!(
            "OS: {} | Time: {} | CWD: {}",
            std::env::consts::OS,
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_else(|_| "unknown".into()),
        );
        Ok(vec![Content::text(info)])
    }
}

// ── Tool: Env Var ──

struct EnvVarTool;

#[async_trait]
impl Tool for EnvVarTool {
    fn name(&self) -> &'static str {
        "env_var"
    }

    fn description(&self) -> &'static str {
        "Read an environment variable value"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Environment variable name" }
            },
            "required": ["name"]
        })
    }

    async fn call(&self, args: &Value, _ctx: &ToolContext) -> AgentResult<Vec<Content>> {
        let name = args["name"].as_str().unwrap_or("");
        match std::env::var(name) {
            Ok(val) => Ok(vec![Content::text(format!("{}={}", name, val))]),
            Err(_) => Ok(vec![Content::text(format!("{} is not set", name))]),
        }
    }
}

// ── Main ──

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let model = common::resolve_llm_env().model;
    let llm_client = client();

    // Register multiple tools — the agent picks the right one for each request
    let builder = base_agent_builder(llm_client)
        .system_prompt(build_system_prompt())
        .register_tool(SystemInfoTool)
        .register_tool(EnvVarTool);

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

    let session = agent.create_session().await;
    let mut renderer =
        create_stdout_renderer(&OutputFormat::Terminal { show_thinking: true, show_tool_args: true, color: true });

    println!("Agent with system_info + env_var tools. Try asking about the system!\n");

    agent
        .run_turn(session, "What's my current system info? Also check if HOME is set.", |event| renderer.render(event))
        .await?;

    Ok(())
}
