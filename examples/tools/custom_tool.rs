//! Custom Tool — demonstrate how to implement and register a custom Tool.
//!
//! This example shows the complete flow: define a Tool struct, implement the
//! `Tool` trait, register it with the builder, and let the agent use it.
//!
//! Run with:
//! ```bash
//! LLM_API_KEY=your-key cargo run --example custom-tool
//! ```

#[path = "../common/mod.rs"]
mod common;

use std::sync::{Arc, Mutex};

use agent_base::{AgentResult, Content, Tool, ToolContext};
use async_trait::async_trait;
use common::client;
use phi_agent::{
    OutputFormat, PhiAgent, PhiAgentConfig, ReasoningEffort, SafetyConfig, base_agent_builder, build_system_prompt,
    create_stdout_renderer,
};
use serde_json::{Value, json};

// ── Custom Tool ──

/// A simple calculator tool that the agent can invoke.
struct CalculatorTool;

#[async_trait]
impl Tool for CalculatorTool {
    fn name(&self) -> &'static str {
        "calculator"
    }

    fn description(&self) -> &'static str {
        "Evaluate a simple arithmetic expression"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "expression": {
                    "type": "string",
                    "description": "Math expression, e.g. '2 + 3 * 4'"
                }
            },
            "required": ["expression"]
        })
    }

    async fn call(&self, args: &Value, _ctx: &ToolContext) -> AgentResult<Vec<Content>> {
        let expr = args["expression"].as_str().unwrap_or("0");

        // Simple evaluation — in production, use a proper expression parser
        let result = evaluate(expr);

        Ok(vec![Content::text(format!("{} = {}", expr, result))])
    }
}

/// Very basic calculator (supports +, -, *, / with integers)
fn evaluate(expr: &str) -> f64 {
    // Split by whitespace and evaluate left-to-right for + and -
    let tokens: Vec<&str> = expr.split_whitespace().collect();
    if tokens.is_empty() {
        return 0.0;
    }

    let mut result: f64 = tokens[0].parse().unwrap_or(0.0);
    let mut i = 1;
    while i < tokens.len() {
        let op = tokens[i];
        let rhs: f64 = tokens.get(i + 1).and_then(|t| t.parse().ok()).unwrap_or(0.0);
        match op {
            "+" => result += rhs,
            "-" => result -= rhs,
            "*" => result *= rhs,
            "/" if rhs != 0.0 => result /= rhs,
            "/" => {},
            _ => {},
        }
        i += 2;
    }
    result
}

// ── Main ──

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let model = common::resolve_llm_env().model;
    let llm_client = client();

    // Register the custom calculator tool
    let builder = base_agent_builder(llm_client).system_prompt(build_system_prompt()).register_tool(CalculatorTool);

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
    let renderer = Arc::new(Mutex::new(create_stdout_renderer(&OutputFormat::Terminal {
        show_thinking: true,
        show_tool_args: true,
        color: true,
    })));
    let renderer_clone = renderer.clone();

    println!("Agent ready. Ask a math question!\n");

    agent
        .run_turn(session, "What is (15 + 27) * 3?", move |event| renderer_clone.lock().unwrap().render(event))
        .await?;

    Ok(())
}
