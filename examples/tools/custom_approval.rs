//! Custom Approval — demonstrate an interactive tool-approval handler.
//!
//! The example uses an offline mock model so the approval flow can be tried
//! without an API key. The model requests one `echo` tool call, the policy
//! turns that request into an approval prompt, and the handler waits for a
//! `y`/`n` answer before the tool runs.
//!
//! Run with:
//! ```bash
//! cargo run --example custom-approval
//! ```

use std::collections::VecDeque;
use std::io::Write;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::task::{Context, Poll};

use agent_base::{
    AgentResult, ApprovalDecision, ApprovalHandler, ApprovalRequest, ChatMessage, LlmCapabilities, LlmClient,
    ReasoningConfig, ResponseFormat, RiskLevel, StreamChunk, Tool, ToolContext, ToolControlFlow, ToolOutput,
    ToolPolicy,
};
use async_trait::async_trait;
use futures_core::Stream;
use phi_agent::{PhiAgent, PhiAgentConfig, ReasoningEffort, SafetyConfig, base_agent_builder};
use serde_json::{Value, json};

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
        let call = self.call_count.fetch_add(1, Ordering::SeqCst);
        if call == 0 {
            Ok(json!({
                "choices": [{
                    "message": {
                        "content": "",
                        "tool_calls": [{
                            "id": "call_1",
                            "type": "function",
                            "function": {
                                "name": "echo",
                                "arguments": "{\"message\":\"hello from the approval example\"}"
                            }
                        }]
                    }
                }]
            }))
        } else {
            Ok(json!({
                "choices": [{
                    "message": {
                        "content": "The approved tool call completed.",
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
        let call = self.call_count.fetch_add(1, Ordering::SeqCst);
        let mut items = VecDeque::new();
        if call == 0 {
            items.push_back(Ok(StreamChunk::ToolCall(json!({
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_1",
                        "function": {
                            "name": "echo",
                            "arguments": "{\"message\":\"hello from the approval example\"}"
                        }
                    }]
                }
            }))));
        } else {
            items.push_back(Ok(StreamChunk::Text("The approved tool call completed.".into())));
        }
        items.push_back(Ok(StreamChunk::Stop));
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

struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &'static str {
        "echo"
    }

    fn definition(&self) -> Value {
        json!({
            "name": "echo",
            "description": "Echo a message after the user approves the call",
            "parameters": {
                "type": "object",
                "properties": {
                    "message": { "type": "string" }
                },
                "required": ["message"]
            }
        })
    }

    async fn call(&self, args: &Value, _ctx: &ToolContext) -> AgentResult<ToolOutput> {
        let message = args.get("message").and_then(Value::as_str).unwrap_or("");
        println!("[tool] echo: {message}");
        Ok(ToolOutput {
            summary: format!("Echoed: {message}"),
            control_flow: ToolControlFlow::Continue,
            raw: None,
            truncation: None,
        })
    }
}

struct RequireApproval;

#[async_trait]
impl ToolPolicy for RequireApproval {
    async fn evaluate_approval(&self, tool_name: &str, args: &Value) -> Option<ApprovalRequest> {
        Some(ApprovalRequest {
            title: format!("Approve {tool_name}"),
            message: format!("The agent wants to call {tool_name} with {args}"),
            action_key: Some(format!("tool:{tool_name}")),
            risk_level: RiskLevel::Sensitive,
            raw: Some(args.clone()),
        })
    }
}

struct InteractiveApprovalHandler;

#[async_trait]
impl ApprovalHandler for InteractiveApprovalHandler {
    async fn approve(
        &self,
        request: ApprovalRequest,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> AgentResult<ApprovalDecision> {
        println!("\nApproval required: {}", request.title);
        println!("{}", request.message);

        let prompt = tokio::task::spawn_blocking(|| {
            print!("Approve this tool call? [y/N] ");
            let _ = std::io::stdout().flush();
            let mut answer = String::new();
            std::io::stdin().read_line(&mut answer).is_ok() && answer.trim().eq_ignore_ascii_case("y")
        });

        tokio::select! {
            _ = cancel_token.cancelled() => Ok(ApprovalDecision::Deny),
            result = prompt => Ok(if result.unwrap_or(false) {
                ApprovalDecision::AllowOnce
            } else {
                ApprovalDecision::Deny
            }),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let builder = base_agent_builder(Arc::new(MockLlmClient::new()))
        .system_prompt("Use the registered tools when they match the user's request.")
        .register_tool(EchoTool)
        .tool_policy(Arc::new(RequireApproval))
        .approval_handler(Arc::new(InteractiveApprovalHandler));

    let agent = PhiAgent::build(
        builder,
        PhiAgentConfig {
            model: "mock".into(),
            enable_thinking: false,
            thinking_budget: None,
            thinking_effort: ReasoningEffort::Low,
            safety: SafetyConfig::default(),
            max_turns: None,
        },
    )?;

    println!("The offline model will request one echo call.");
    let session = agent.create_session().await;
    agent
        .run_turn(session, "Use the echo tool once with a greeting.", |event| {
            if let phi_agent::RuntimeEvent::TextDelta { text, .. } = event {
                println!("[assistant] {text}");
            }
            Ok(())
        })
        .await?;

    Ok(())
}
