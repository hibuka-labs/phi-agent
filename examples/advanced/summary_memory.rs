//! Summary Memory — demonstrate LLM-based conversation summarization.
//!
//! When the conversation grows too long, compress earlier messages into a
//! summary paragraph. The summary is injected as a system message so the
//! agent retains context without exceeding token limits.
//!
//! phi-agent uses `CompressionMiddleware` from agent-works (in
//! `agent_works::compression`). This example shows how to build your own
//! custom compression strategy on top of the Middleware trait.
//!
//! Usage:
//!   cargo run --example summary_memory

use std::sync::{Arc, Mutex};

use agent_base::{AgentResult, ChatMessage, Middleware, PreLlmCtx};
use async_trait::async_trait;
use phi_agent::{PhiAgent, PhiAgentConfig, ReasoningEffort, SafetyConfig, base_agent_builder, build_system_prompt};

#[path = "../common/mod.rs"]
mod common;

/// ── Summarizing middleware ──
///
/// When the message count exceeds `threshold`, the first half of the
/// conversation is replaced with a summary message. In production, you'd
/// use an LLM to generate the summary.
struct SummaryMemory {
    threshold: usize,
}

#[async_trait]
impl Middleware for SummaryMemory {
    async fn on_pre_llm(&self, ctx: &mut PreLlmCtx) -> AgentResult<()> {
        if ctx.messages.len() <= self.threshold {
            return Ok(());
        }

        let mid = ctx.messages.len() / 2;
        let older_count = mid;

        // In a real implementation, send older messages to an LLM for
        // summarization. Here we create a placeholder summary.
        let summary = format!(
            "[Previous conversation summary: {} messages about the ongoing task. \
             Key context preserved.]",
            older_count
        );

        let mut compressed: Vec<ChatMessage> = Vec::new();

        // Keep system message if present
        if let Some(sys) = ctx.messages.iter().find(|m| matches!(m, ChatMessage::System { .. })) {
            compressed.push(sys.clone());
        }

        // Add summary as a system message
        compressed.push(ChatMessage::system(summary));

        // Keep recent messages (skip original system messages)
        for msg in &ctx.messages[mid..] {
            if !matches!(msg, ChatMessage::System { .. }) {
                compressed.push(msg.clone());
            }
        }

        println!(
            "[summary-memory] compressed {} messages → {} messages (threshold={})",
            ctx.messages.len(),
            compressed.len(),
            self.threshold
        );

        ctx.messages = compressed;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let llm_client = common::client();

    // ── Build agent with summary memory (summarize after 12 messages) ──
    let builder =
        base_agent_builder(llm_client).system_prompt(build_system_prompt()).middleware(SummaryMemory { threshold: 12 });

    let agent = PhiAgent::build(
        builder,
        PhiAgentConfig {
            model: common::resolve_llm_env().model,
            enable_thinking: false,
            thinking_budget: None,
            thinking_effort: ReasoningEffort::Medium,
            safety: SafetyConfig::default(),
            max_turns: Some(20),
        },
    )?;

    let session = agent.create_session().await;
    let renderer = Arc::new(Mutex::new(phi_agent::create_stdout_renderer(&phi_agent::OutputFormat::Terminal {
        show_thinking: false,
        show_tool_args: false,
        color: true,
    })));
    let renderer_clone = renderer.clone();

    println!("=== Summary Memory demo ===\n");
    agent
        .run_turn(session, "Tell me a short story in 3 sentences.", move |event| {
            renderer_clone.lock().unwrap().render(event)
        })
        .await?;

    println!("\n=== Summary memory demonstrated ===");
    Ok(())
}
