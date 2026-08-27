use anyhow::Result;
use std::fs;
use std::path::Path;

const ENV_EXAMPLE: &str = r#"# phi-agent LLM configuration
LLM_API_KEY=sk-your-key-here
LLM_BASE_URL=https://api.openai.com/v1
LLM_MODEL=gpt-4o
"#;

const MAIN_RS: &str = r#"use phi_agent::{
    base_agent_builder, build_system_prompt,
    PhiAgent, PhiAgentConfig,
    SafetyConfig, ReasoningEffort,
    OutputFormat, create_stdout_renderer,
    AgentResult, Content, Tool, ToolContext,
};
use async_trait::async_trait;
use rustyline::DefaultEditor;
use serde_json::{Value, json};
use std::sync::Arc;

// ── ClockTool: 一个最简单的自定义工具 ──

struct ClockTool;

#[async_trait]
impl Tool for ClockTool {
    fn name(&self) -> &'static str { "get_time" }

    fn description(&self) -> &'static str {
        "获取当前日期和时间"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn call(&self, _args: &Value, _ctx: &ToolContext) -> AgentResult<Vec<Content>> {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        Ok(vec![Content::text(format!("当前时间：{}", now))])
    }
}

// ── REPL ──

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let api_key = std::env::var("LLM_API_KEY")
        .map_err(|_| anyhow::anyhow!(
            "LLM_API_KEY not found.\n\n\
             Create a .env file:\n  cp .env.example .env\n  # edit with your API key"
        ))?;
    let model = std::env::var("LLM_MODEL").unwrap_or_else(|_| "gpt-4o".into());
    let llm = llm_unified::create_provider(&phi_agent::agent_base::llm_trait::LlmConfig {
        protocol: None,
        api_key,
        model: model.clone(),
        base_url: std::env::var("LLM_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".into()),
        options: std::collections::HashMap::new(),
    })?;

    let agent = PhiAgent::build(
        base_agent_builder(llm)
            .system_prompt(build_system_prompt())
            .register_tool(ClockTool),
        PhiAgentConfig {
            model,
            enable_thinking: true,
            thinking_budget: None,
            thinking_effort: ReasoningEffort::Medium,
            safety: SafetyConfig::default(),
            max_turns: None,
        },
    )?;

    let mut rl = DefaultEditor::new()?;
    let mut renderer = create_stdout_renderer(&OutputFormat::Terminal {
        show_thinking: true,
        show_tool_args: true,
        color: true,
    });

    println!("phi-agent REPL — type /exit to quit\n");
    println!("Try: 现在几点了？\n");
    loop {
        let line = rl.readline("phi> ")?;
        let input = line.trim().to_string();
        if input.is_empty() { continue; }
        if input == "/exit" { break; }
        if input == "tools" {
            let tools = agent.list_tools().await;
            if tools.is_empty() {
                println!("\n  (no tools registered)\n");
            } else {
                println!("\n  Registered tools ({}):\n", tools.len());
                for m in &tools {
                    println!("  \x1b[1m{}\x1b[0m  {}  v{}", m.name, m.origin, m.version);
                    println!("    {}", m.description);
                    if !m.requirements.is_empty() {
                        println!("    requirements: {}", m.requirements.join(", "));
                    }
                }
                println!();
            }
            continue;
        }
        rl.add_history_entry(&input)?;

        let session = agent.create_session().await;
        agent.run_turn(session, &input, |event| renderer.render(event)).await?;
        println!();
    }

    Ok(())
}
"#;

const LIB_RS: &str = r#"//! phi-agent library integration example.
//!
//! Three steps: define a tool → register → run.
//! Run:  cargo run

use phi_agent::{
    base_agent_builder, build_system_prompt,
    PhiAgent, PhiAgentConfig,
    SafetyConfig, ReasoningEffort,
    OutputFormat, create_stdout_renderer,
    AgentResult, Content, Tool, ToolContext,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::Arc;

// ── ClockTool ──

struct ClockTool;

#[async_trait]
impl Tool for ClockTool {
    fn name(&self) -> &'static str { "get_time" }

    fn description(&self) -> &'static str {
        "获取当前日期和时间"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn call(&self, _args: &Value, _ctx: &ToolContext) -> AgentResult<Vec<Content>> {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        Ok(vec![Content::text(format!("当前时间：{}", now))])
    }
}

// ── Main ──

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let api_key = std::env::var("LLM_API_KEY")
        .map_err(|_| anyhow::anyhow!(
            "LLM_API_KEY not found.\n\n\
             Create a .env file:\n  cp .env.example .env\n  # edit with your API key"
        ))?;
    let model = std::env::var("LLM_MODEL").unwrap_or_else(|_| "gpt-4o".into());
    let llm = llm_unified::create_provider(&phi_agent::agent_base::llm_trait::LlmConfig {
        protocol: None,
        api_key,
        model: model.clone(),
        base_url: std::env::var("LLM_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".into()),
        options: std::collections::HashMap::new(),
    })?;

    let agent = PhiAgent::build(
        base_agent_builder(llm)
            .system_prompt(build_system_prompt())
            .register_tool(ClockTool),
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
    let mut renderer = create_stdout_renderer(&OutputFormat::Terminal {
        show_thinking: true, show_tool_args: true, color: true,
    });

    agent.run_turn(session, "现在几点了？", |event| renderer.render(event)).await?;
    Ok(())
}
"#;

pub fn run(name: &str, is_lib: bool) -> Result<()> {
    let dir = Path::new(name);
    let project_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or(name);

    if dir.exists() {
        anyhow::bail!("directory '{}' already exists", name);
    }

    fs::create_dir_all(dir.join("src"))?;

    // Cargo.toml — lib mode skips rustyline
    let rustyline_dep = if is_lib { "" } else { "rustyline = \"15\"\n" };
    fs::write(
        dir.join("Cargo.toml"),
        format!(
            r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[dependencies]
phi-agent = "{}"
tokio = {{ version = "1", features = ["full"] }}
anyhow = "1"
dotenvy = "0.15"
{}async-trait = "0.1"
serde_json = "1"
chrono = "0.4"
"#,
            project_name,
            env!("CARGO_PKG_VERSION"),
            rustyline_dep,
        ),
    )?;

    // .env.example
    fs::write(dir.join(".env.example"), ENV_EXAMPLE)?;

    // src/main.rs
    let code = if is_lib { LIB_RS } else { MAIN_RS };
    fs::write(dir.join("src").join("main.rs"), code)?;

    let mode = if is_lib { "library integration" } else { "REPL" };
    println!("✅ Created project: {} ({})", name, mode);
    println!();
    println!("Next steps:");
    println!("  cd {}", name);
    println!("  cp .env.example .env   # edit with your API key");
    println!("  cargo run");

    Ok(())
}
