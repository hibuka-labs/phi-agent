//! Custom ApprovalHandler — demonstrate implementing a custom approval strategy.
//!
//! This example shows how to create a custom [`ApprovalHandler`] that makes
//! approval decisions based on the tool's risk level. Uses a Mock LLM client
//! so it runs offline — no API key needed.
//!
//! # What you'll learn
//!
//! 1. Implement the [`ApprovalHandler`] trait with custom logic
//! 2. Inspect [`ApprovalRequest`] fields (title, risk_level, message)
//! 3. Register the handler on the builder via `.approval_handler()`
//! 4. How [`ApprovalDecision`] values affect tool execution
//!
//! # ApprovalHandler vs ToolPolicy
//!
//! - **ApprovalHandler** — global decision-maker: approve or deny every tool call
//!   that requires approval. Best when you want a single policy for all tools.
//! - **ToolPolicy** — per-tool hooks: selectively skip approval for specific
//!   tools while requiring it for others. See `custom_policy.rs` for that pattern.
//!
//! Run with:
//! ```bash
//! cargo run --example custom-approval
//! ```

use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use agent_base::llm_trait::response::FinishReason;
use agent_base::llm_trait::{Capabilities, ChatRequest, ChatResponse, ChatStream, LlmError, LlmProvider, ProviderInfo};
use agent_base::{AgentResult, ApprovalDecision, ApprovalHandler, ApprovalRequest, RiskLevel, StreamChunk};
use async_trait::async_trait;
use futures_core::Stream;

// ── Mock LLM client (same pattern as custom_policy.rs) ────────────────────

struct QueueStream {
    items: VecDeque<Result<StreamChunk, LlmError>>,
}

impl Stream for QueueStream {
    type Item = Result<StreamChunk, LlmError>;
    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.items.pop_front())
    }
}

struct MockLlmClient;

impl MockLlmClient {
    fn new() -> Self {
        Self
    }
}

#[async_trait]
impl LlmProvider for MockLlmClient {
    async fn stream(&self, _request: ChatRequest) -> Result<ChatStream, LlmError> {
        let mut items = VecDeque::new();
        items.push_back(Ok(StreamChunk::Text("Hello from mock!".into())));
        items.push_back(Ok(StreamChunk::Stop { finish_reason: Some("stop".into()) }));
        Ok(ChatStream::new(Box::pin(QueueStream { items })))
    }

    async fn chat(&self, _request: ChatRequest) -> Result<ChatResponse, LlmError> {
        Ok(ChatResponse {
            content: "mock response".to_string(),
            reasoning_content: None,
            tool_calls: vec![],
            usage: agent_base::UsageInfo::default(),
            finish_reason: FinishReason::Stop,
            raw: None,
            thinking_signature: None,
        })
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { supports_streaming: true, supports_tools: true, ..Default::default() }
    }

    fn info(&self) -> ProviderInfo {
        ProviderInfo { name: "mock".into(), model: "mock".into(), version: None }
    }
}

// ── Custom ApprovalHandler — risk-based decisions ─────────────────────────

/// An approval handler that makes decisions based on the tool's risk level.
///
/// - `Safe` tools are always approved (AllowAlways).
/// - `Sensitive` tools are approved once (AllowOnce — re-prompt on next call).
/// - `Destructive` tools are always denied.
///
/// This pattern is useful for CI/CD pipelines, sandboxed environments, or any
/// scenario where you want a predictable, non-interactive approval strategy.
pub struct RiskBasedApprovalHandler;

impl RiskBasedApprovalHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RiskBasedApprovalHandler {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl ApprovalHandler for RiskBasedApprovalHandler {
    async fn approve(
        &self,
        request: ApprovalRequest,
        _cancel_token: tokio_util::sync::CancellationToken,
    ) -> AgentResult<ApprovalDecision> {
        let decision = match request.risk_level {
            RiskLevel::Safe => {
                eprintln!("  [APPROVE] '{}' is safe — AllowAlways", request.title);
                ApprovalDecision::AllowAlways
            },
            RiskLevel::Sensitive => {
                eprintln!("  [APPROVE] '{}' is sensitive — AllowOnce (will re-prompt next time)", request.title);
                ApprovalDecision::AllowOnce
            },
            RiskLevel::Destructive => {
                eprintln!("  [DENY]   '{}' is destructive — Deny\n    reason: {}", request.title, request.message);
                ApprovalDecision::Deny
            },
        };
        Ok(decision)
    }
}

// ── Main ──────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Custom ApprovalHandler Demo ===\n");
    println!("This example demonstrates a risk-based approval handler.\n");
    println!("Strategy:");
    println!("  Safe        → AllowAlways (cached — won't prompt again)");
    println!("  Sensitive   → AllowOnce   (approve this once, re-prompt later)");
    println!("  Destructive → Deny        (always reject)\n");

    let llm_client = Arc::new(MockLlmClient::new());
    let approval_handler = Arc::new(RiskBasedApprovalHandler::new());

    // Build the agent with the custom handler
    let runtime = phi_agent::base_agent_builder(llm_client)
        .system_prompt("You are a helpful assistant.".to_string())
        .approval_handler(approval_handler)
        .build()?;

    let session = runtime.create_session().await;

    // Run a simple turn — the mock LLM returns text, so no tool calls here.
    // In a real scenario with tools, you would see the approval handler
    // print its decisions to stderr for each tool call.
    println!("--- Running turn: 'hello' ---\n");
    runtime
        .run_turn(session, "hello", |event| {
            if let agent_base::RuntimeEvent::TextDelta { text, .. } = &event {
                eprintln!("  [LLM] {text}");
            }
            Ok(())
        })
        .await?;

    println!("\n--- Done ---");
    println!("\nTo use this handler in your own code:");
    println!("  let handler = Arc::new(RiskBasedApprovalHandler::new());");
    println!("  builder.approval_handler(handler);");

    Ok(())
}
