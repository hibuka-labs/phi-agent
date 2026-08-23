//! Window Memory — demonstrate sliding-window context management.
//!
//! Keeps only the most recent N messages in context to prevent token
//! overflow in long conversations. Earlier messages are dropped but
//! can be summarized or archived externally.
//!
//! The `on_pre_llm` hook is the right place for this — it fires right
//! before the LLM call, giving middleware the chance to trim the message
//! list.
//!
//! Usage:
//!   cargo run --example window_memory

use std::sync::{Arc, Mutex};

use agent_base::{AgentResult, ChatMessage, Middleware, PreLlmCtx};
use async_trait::async_trait;
use phi_agent::{PhiAgent, PhiAgentConfig, ReasoningEffort, SafetyConfig, base_agent_builder, build_system_prompt};

#[path = "../common/mod.rs"]
mod common;

/// ── Sliding-window middleware ──
///
/// Truncates the conversation history to the most recent `window_size`
/// messages (system message + last N user/assistant pairs).
struct SlidingWindow {
    window_size: usize,
}

impl SlidingWindow {
    fn new(window_size: usize) -> Self {
        assert!(window_size >= 2, "window_size must be at least 2 (system + 1 user)");
        Self { window_size }
    }
}

#[async_trait]
impl Middleware for SlidingWindow {
    async fn on_pre_llm(&self, ctx: &mut PreLlmCtx) -> AgentResult<()> {
        if ctx.messages.len() <= self.window_size {
            return Ok(());
        }

        // Keep system message + last (window_size - 1) messages
        let mut truncated: Vec<ChatMessage> = Vec::with_capacity(self.window_size);

        // Preserve system message if present
        if let Some(sys) = ctx.messages.iter().find(|m| matches!(m, ChatMessage::System { .. })) {
            truncated.push(sys.clone());
        }

        // Keep most recent non-system messages
        let keep_count = self.window_size - truncated.len();
        let recent_start = ctx.messages.len().saturating_sub(keep_count);
        for msg in &ctx.messages[recent_start..] {
            if !matches!(msg, ChatMessage::System { .. }) {
                truncated.push(msg.clone());
            }
        }

        println!(
            "[sliding-window] truncated {} messages → {} messages (window={})",
            ctx.messages.len(),
            truncated.len(),
            self.window_size
        );

        ctx.messages = truncated;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let llm_client = common::client();

    // ── Build agent with sliding window (keep last 8 messages) ──
    let builder = base_agent_builder(llm_client).system_prompt(build_system_prompt()).middleware(SlidingWindow::new(8));

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

    println!("=== Sliding Window Memory demo ===\n");
    agent
        .run_turn(session, "Count from 1 to 3, one number per line.", move |event| {
            renderer_clone.lock().unwrap().render(event)
        })
        .await?;

    println!("\n=== Window memory demonstrated ===");
    Ok(())
}
