//! Shared test utilities for phi-agent integration tests.
//!
//! Provides a mock LLM client, stream stubs, and helper functions
//! used across multiple test files.

#![allow(dead_code)]

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use agent_base::llm_trait::response::FinishReason;
use agent_base::llm_trait::{Capabilities, ChatRequest, ChatResponse, ChatStream, LlmError, LlmProvider, ProviderInfo};
use agent_base::{StreamChunk, UsageInfo};
use async_trait::async_trait;
use futures_core::Stream;
use phi_agent::bridge::server::ProtocolServer;
use phi_agent::{
    ApprovalMode, AutoApprovalHandler, SafetyConfig, TurnFactMiddleware, TurnToolLimitMiddleware, base_agent_builder,
    build_system_prompt,
};
use serde_json::Value;

// ── EmptyStream — stubs chat_stream() ─────────────────────────────────

pub struct EmptyStream;

impl Stream for EmptyStream {
    type Item = Result<StreamChunk, LlmError>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(None)
    }
}

// ── MockLlmClient — configurable LLM stub ─────────────────────────────

/// A mock LLM client that can be programmed to return tool calls or text.
pub struct MockLlmClient {
    pub tool_call_response: tokio::sync::Mutex<Option<(String, String)>>,
    pub text_response: String,
}

impl MockLlmClient {
    pub fn new() -> Self {
        Self { tool_call_response: tokio::sync::Mutex::new(None), text_response: "mock response".to_string() }
    }

    pub async fn set_tool_call(&self, name: &str, args: &Value) {
        *self.tool_call_response.lock().await = Some((name.to_string(), args.to_string()));
    }
}

#[async_trait]
impl LlmProvider for MockLlmClient {
    async fn stream(&self, _request: ChatRequest) -> Result<ChatStream, LlmError> {
        Ok(ChatStream::new(Box::pin(EmptyStream)))
    }
    async fn chat(&self, _request: ChatRequest) -> Result<ChatResponse, LlmError> {
        let tc = self.tool_call_response.lock().await.take();
        if let Some((name, args)) = tc {
            Ok(ChatResponse {
                content: String::new(),
                reasoning_content: None,
                tool_calls: vec![agent_base::llm_trait::response::ToolCall {
                    id: "call-test-1".to_string(),
                    name,
                    arguments: args,
                }],
                usage: UsageInfo::default(),
                finish_reason: FinishReason::ToolCalls,
                raw: None,
                thinking_signature: None,
            })
        } else {
            Ok(ChatResponse {
                content: self.text_response.clone(),
                reasoning_content: None,
                tool_calls: vec![],
                usage: UsageInfo::default(),
                finish_reason: FinishReason::Stop,
                raw: None,
                thinking_signature: None,
            })
        }
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
    fn info(&self) -> ProviderInfo {
        ProviderInfo { name: "mock".to_string(), model: "mock-model".to_string(), version: None }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

pub fn build_server(mock: Arc<dyn LlmProvider>) -> ProtocolServer {
    let builder = base_agent_builder(mock)
        .system_prompt(build_system_prompt())
        .approval_handler(Arc::new(AutoApprovalHandler::new(ApprovalMode::Auto)))
        .middleware(TurnFactMiddleware::new())
        .middleware(TurnToolLimitMiddleware::from_config(&SafetyConfig::default()));
    ProtocolServer::from_builder(builder).expect("build server")
}

pub fn event_type(event: &phi_agent::RuntimeEvent) -> &'static str {
    match event {
        phi_agent::RuntimeEvent::TextDelta { .. } => "text_delta",
        phi_agent::RuntimeEvent::ThoughtDelta { .. } => "thought_delta",
        phi_agent::RuntimeEvent::ToolCallStarted { .. } => "tool_call_started",
        phi_agent::RuntimeEvent::ToolCallFinished { .. } => "tool_call_finished",
        phi_agent::RuntimeEvent::RunFinished { .. } => "run_finished",
        phi_agent::RuntimeEvent::RunCancelled { .. } => "run_cancelled",
        _ => "other",
    }
}

pub async fn collect_events(event_rx: &mut tokio::sync::broadcast::Receiver<phi_agent::RuntimeEvent>) -> Vec<String> {
    let mut events = Vec::new();
    loop {
        tokio::select! {
            event_result = event_rx.recv() => {
                match event_result {
                    Ok(event) => {
                        let typ = event_type(&event);
                        events.push(typ.to_string());
                        if typ == "run_finished" || typ == "run_cancelled" {
                            return events;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return events,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                return events;
            }
        }
    }
}
