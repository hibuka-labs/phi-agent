//! Middleware Hooks — demonstrate the Middleware lifecycle hooks.
//!
//! Middleware lets you intercept and transform agent behavior at three points:
//! 1. `on_user_message` — before the user message reaches the LLM
//! 2. `on_pre_llm`     — before the LLM call, can modify messages/tools
//! 3. `on_post_llm`    — after the LLM responds, before tool execution
//!
//! For event-level monitoring, use the callback on `agent.run_turn()`.
//!
//! Usage:
//!   cargo run --example middleware_hooks

use std::sync::atomic::AtomicU32;
use std::sync::{Arc, Mutex};

use agent_base::{AgentResult, Middleware, PostLlmCtx, PreLlmCtx, UserMessageCtx};
use async_trait::async_trait;
use phi_agent::{PhiAgent, PhiAgentConfig, ReasoningEffort, SafetyConfig, base_agent_builder, build_system_prompt};

#[path = "../common/mod.rs"]
mod common;

/// A middleware that logs every lifecycle hook and tracks turn count.
struct LoggingMiddleware {
    turn_count: AtomicU32,
}

impl LoggingMiddleware {
    fn new() -> Self {
        Self { turn_count: AtomicU32::new(0) }
    }
}

#[async_trait]
impl Middleware for LoggingMiddleware {
    async fn on_user_message(&self, ctx: &mut UserMessageCtx) -> AgentResult<()> {
        let turn = self.turn_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        println!(
            "[middleware] on_user_message — turn={}, session={}, input_len={}",
            turn,
            ctx.session_id.id,
            ctx.user_input.len()
        );
        Ok(())
    }

    async fn on_pre_llm(&self, ctx: &mut PreLlmCtx) -> AgentResult<()> {
        println!(
            "[middleware] on_pre_llm — session={}, messages={}, tools={}",
            ctx.session_id.id,
            ctx.messages.len(),
            ctx.tools.len()
        );
        Ok(())
    }

    async fn on_post_llm(&self, ctx: &mut PostLlmCtx) -> AgentResult<()> {
        println!(
            "[middleware] on_post_llm — is_tool_call={}, text_len={}, turn={}",
            ctx.is_tool_call,
            ctx.full_text.len(),
            ctx.turn_count
        );
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // ── 1. Build agent with middleware ──
    let llm_client = common::client();

    let builder =
        base_agent_builder(llm_client).system_prompt(build_system_prompt()).middleware(LoggingMiddleware::new());

    let agent = PhiAgent::build(
        builder,
        PhiAgentConfig {
            model: common::resolve_llm_env().model,
            enable_thinking: false,
            thinking_budget: None,
            thinking_effort: ReasoningEffort::Medium,
            safety: SafetyConfig::default(),
            max_turns: Some(10),
        },
    )?;

    // ── 2. Run with event callback (event-level monitoring) ──
    let session = agent.create_session().await;
    let renderer = Arc::new(Mutex::new(phi_agent::create_stdout_renderer(&phi_agent::OutputFormat::Terminal {
        show_thinking: false,
        show_tool_args: false,
        color: true,
    })));
    let renderer_clone = renderer.clone();

    println!("=== Middleware demo ===\n");
    agent
        .run_turn(session, "Say hello in exactly 5 words.", move |event| {
            // Event-level hook — fires for every RuntimeEvent
            use agent_base::RuntimeEvent;
            if let RuntimeEvent::ToolCallStarted { tool_name, .. } = &event {
                println!("[callback] tool call started: {}", tool_name);
            }
            renderer_clone.lock().unwrap().render(event)
        })
        .await?;

    println!("\n=== Middleware demonstrated ===");
    Ok(())
}
