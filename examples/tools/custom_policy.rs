//! Custom Policy — demonstrate ToolPolicy, Middleware, and RuntimeEvent hooks.
//!
//! This example shows the complete hook/event system without needing an API key:
//! 1. ToolPolicy  — evaluate_approval / before_call / after_call
//! 2. Middleware   — on_user_message / on_post_llm
//! 3. RuntimeEvent — collecting and printing the full event stream
//!
//! Uses a Mock LLM client so it runs offline.
//!
//! Run with:
//! ```bash
//! cargo run --example custom-policy
//! ```

use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::task::{Context, Poll};

use agent_base::{
    AgentResult, ApprovalRequest, ChatMessage, Content, LlmCapabilities, LlmClient, Middleware, PostLlmCtx,
    ReasoningConfig, ResponseFormat, RuntimeEvent, StreamChunk, Tool, ToolContext, ToolPolicy,
};
use async_trait::async_trait;
use futures_core::Stream;
use phi_agent::{ApprovalMode, AutoApprovalHandler, base_agent_builder};
use serde_json::{Value, json};

// ── Mock LLM that returns a tool call then text ──

/// A simple stream that yields items from a queue.
struct QueueStream {
    items: VecDeque<AgentResult<StreamChunk>>,
}

impl Stream for QueueStream {
    type Item = AgentResult<StreamChunk>;
    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.items.pop_front())
    }
}

struct MockLlmClient {
    call_count: AtomicU32,
}

impl MockLlmClient {
    fn new() -> Self {
        Self { call_count: AtomicU32::new(0) }
    }
}

#[async_trait]
impl LlmClient for MockLlmClient {
    async fn chat(
        &self,
        _messages: &[ChatMessage],
        _tools: &[Value],
        _reasoning: Option<&ReasoningConfig>,
        _response_format: Option<&ResponseFormat>,
    ) -> AgentResult<Value> {
        let count = self.call_count.fetch_add(1, Ordering::SeqCst);
        if count == 0 {
            Ok(json!({
                "choices": [{
                    "message": {
                        "content": "",
                        "tool_calls": [{
                            "id": "call_1",
                            "type": "function",
                            "function": {
                                "name": "echo",
                                "arguments": "{\"msg\": \"hello from hook demo\"}"
                            }
                        }]
                    }
                }]
            }))
        } else {
            Ok(json!({
                "choices": [{
                    "message": {
                        "content": "All hooks and events demonstrated!",
                        "tool_calls": null
                    }
                }]
            }))
        }
    }

    async fn chat_stream(
        &self,
        _messages: &[ChatMessage],
        _tools: &[Value],
        _reasoning: Option<&ReasoningConfig>,
        _response_format: Option<&ResponseFormat>,
    ) -> AgentResult<Pin<Box<dyn Stream<Item = AgentResult<StreamChunk>> + Send>>> {
        let count = self.call_count.fetch_add(1, Ordering::SeqCst);
        let mut items = VecDeque::new();
        if count == 0 {
            // First call: emit a tool call chunk
            items.push_back(Ok(StreamChunk::ToolCall(json!({
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_1",
                        "function": {
                            "name": "echo",
                            "arguments": "{\"msg\": \"hello from hook demo\"}"
                        }
                    }]
                }
            }))));
        } else {
            // Second call: emit text
            items.push_back(Ok(StreamChunk::Text("All hooks and events demonstrated!".to_string())));
        }
        items.push_back(Ok(StreamChunk::Stop { finish_reason: Some("stop".to_string()) }));
        Ok(Box::pin(QueueStream { items }))
    }

    fn capabilities(&self) -> LlmCapabilities {
        LlmCapabilities {
            supports_streaming: true,
            supports_tools: true,
            supports_vision: false,
            supports_thinking: false,
            max_context_tokens: Some(128_000),
            max_output_tokens: Some(16_384),
        }
    }
}

// ── A simple echo tool ──

struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &'static str {
        "echo"
    }

    fn description(&self) -> &'static str {
        "Echo a message back"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "msg": { "type": "string", "description": "The message to echo" }
            },
            "required": ["msg"]
        })
    }

    async fn call(&self, args: &Value, _ctx: &ToolContext) -> AgentResult<Vec<Content>> {
        let msg = args["msg"].as_str().unwrap_or("ok");
        println!("    → EchoTool.call() executing with msg=\"{msg}\"");
        Ok(vec![Content::text(format!("EchoTool executed: {msg}"))])
    }
}

// ── Custom ToolPolicy ──

struct DemoPolicy {
    before_count: AtomicU32,
    after_count: AtomicU32,
}

impl DemoPolicy {
    fn new() -> Self {
        Self { before_count: AtomicU32::new(0), after_count: AtomicU32::new(0) }
    }
}

#[async_trait]
impl ToolPolicy for DemoPolicy {
    /// 1. evaluate_approval — decide if tool needs user approval (async)
    async fn evaluate_approval(&self, tool_name: &str, args: &Value) -> Option<ApprovalRequest> {
        let msg = args.get("msg").and_then(Value::as_str).unwrap_or("");
        println!("  [ToolPolicy] evaluate_approval('{tool_name}', msg=\"{msg}\") → no approval needed");
        None
    }

    /// 2. before_call — check before execution, return Err to abort
    fn before_call(&self, tool_name: &str, _args: &Value, _ctx: &ToolContext) -> AgentResult<()> {
        self.before_count.fetch_add(1, Ordering::SeqCst);
        println!("  [ToolPolicy] before_call('{tool_name}') — about to execute");
        Ok(())
    }

    /// 3. after_call — callback after successful execution
    fn after_call(&self, tool_name: &str, _args: &Value, result: &[Content], _ctx: &ToolContext) -> AgentResult<()> {
        self.after_count.fetch_add(1, Ordering::SeqCst);
        println!("  [ToolPolicy] after_call('{tool_name}') — result: {}", agent_base::tool::content_text(result));
        Ok(())
    }
}

// ── Custom Middleware ──

struct DemoMiddleware;

#[async_trait]
impl Middleware for DemoMiddleware {
    async fn on_user_message(&self, ctx: &mut agent_base::UserMessageCtx) -> AgentResult<()> {
        println!("  [Middleware] on_user_message — input: \"{}\"", ctx.user_input);
        Ok(())
    }

    async fn on_post_llm(&self, ctx: &mut PostLlmCtx) -> AgentResult<()> {
        if ctx.is_tool_call {
            println!("  [Middleware] on_post_llm — is_tool_call=true, {} tool(s) requested", ctx.tool_calls.len(),);
        } else {
            println!("  [Middleware] on_post_llm — is_tool_call=false, text=\"{}\"", ctx.full_text,);
        }
        Ok(())
    }
}

// ── Main ──

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Custom Policy Demo ===\n");
    println!("This example demonstrates ToolPolicy, Middleware, and RuntimeEvent hooks.\n");

    let llm_client = agent_base::llm::adapt(Arc::new(MockLlmClient::new()));
    let policy = Arc::new(DemoPolicy::new());

    let runtime = base_agent_builder(llm_client)
        .system_prompt("You are a helpful assistant.".to_string())
        .register_tool(EchoTool)
        .tool_policy(policy.clone())
        .middleware(DemoMiddleware)
        .approval_handler(Arc::new(AutoApprovalHandler::new(ApprovalMode::Auto)))
        .build()?;

    let session_id = runtime.create_session().await;

    println!("--- Running turn: 'echo hello from hook demo' ---\n");

    let mut events = Vec::new();
    runtime
        .run_turn(session_id, "echo hello from hook demo", |event| {
            match &event {
                RuntimeEvent::ToolCallStarted { tool_name, args_json, .. } => {
                    println!("  [Event] ToolCallStarted   — tool=\"{tool_name}\", args={args_json}");
                },
                RuntimeEvent::ToolCallFinished { tool_name, summary, .. } => {
                    println!("  [Event] ToolCallFinished  — tool=\"{tool_name}\", summary=\"{summary}\"");
                },
                RuntimeEvent::TextDelta { text, .. } => {
                    println!("  [Event] TextDelta         — \"{text}\"");
                },
                RuntimeEvent::AwaitingApproval { request, .. } => {
                    println!(
                        "  [Event] AwaitingApproval  — risk={:?}, title=\"{}\"",
                        request.risk_level, request.title,
                    );
                },
                RuntimeEvent::RunFinished { .. } => {
                    println!("  [Event] RunFinished");
                },
                RuntimeEvent::UserEvent { event: agent_base::UserEvent::Progress { text }, .. } => {
                    println!("  [Event] UserEvent(Progress) — \"{text}\"");
                },
                _ => {},
            }
            events.push(event);
            Ok(())
        })
        .await?;

    println!("\n--- Summary ---");
    println!("Total events captured: {}", events.len());
    println!("ToolPolicy before_call count: {}", policy.before_count.load(Ordering::SeqCst),);
    println!("ToolPolicy after_call count:  {}", policy.after_count.load(Ordering::SeqCst),);
    println!("\nHook execution order:");
    println!("  on_user_message → on_post_llm → evaluate_approval → before_call →");
    println!("  ToolCallStarted → tool.call() → ToolCallFinished → after_call →");
    println!("  on_post_llm → TextDelta → RunFinished");
    println!("\n=== Done ===");

    Ok(())
}
