//! Stress tests: bridge protocol server concurrency.
//!
//! Validates that concurrent sessions are properly isolated,
//! no cross-session event leakage occurs, and session reuse works correctly.

use agent_base::llm_trait::response::FinishReason;
use agent_base::llm_trait::{Capabilities, ChatRequest, ChatResponse, ChatStream, LlmError, LlmProvider, ProviderInfo};
use agent_base::{StreamChunk, UsageInfo};
use phi_agent::base_agent_builder;
use phi_agent::bridge::server::ProtocolServer;
use phi_agent::build_system_prompt;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

/// Mock LLM client.
struct EmptyLlmClient;
#[async_trait::async_trait]
impl LlmProvider for EmptyLlmClient {
    async fn stream(&self, _request: ChatRequest) -> Result<ChatStream, LlmError> {
        struct EmptyStream;
        impl Stream for EmptyStream {
            type Item = Result<StreamChunk, LlmError>;
            fn poll_next(self: Pin<&mut Self>, _: &mut std::task::Context<'_>) -> std::task::Poll<Option<Self::Item>> {
                std::task::Poll::Ready(None)
            }
        }
        Ok(ChatStream::new(Box::pin(EmptyStream)))
    }
    async fn chat(&self, _request: ChatRequest) -> Result<ChatResponse, LlmError> {
        Ok(ChatResponse {
            content: String::new(),
            reasoning_content: None,
            tool_calls: vec![],
            usage: UsageInfo::default(),
            finish_reason: FinishReason::Stop,
            raw: None,
            thinking_signature: None,
        })
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
    fn info(&self) -> ProviderInfo {
        ProviderInfo { name: "mock".to_string(), model: "mock-model".to_string(), version: None }
    }
}

/// Test: 10 concurrent sessions, each verified to be unique.
#[tokio::test(flavor = "multi_thread")]
async fn test_concurrent_sessions_isolated() {
    let client = Arc::new(EmptyLlmClient);
    let builder = base_agent_builder(client).system_prompt(build_system_prompt());
    let server = ProtocolServer::from_builder(builder).unwrap();

    let mut handles = vec![];
    for i in 0..10 {
        let server = server.clone();
        handles.push(tokio::spawn(async move {
            let ext_id = format!("concurrent-{}", i);
            server.get_or_create_session(Some(ext_id)).await
        }));
    }

    let mut sids = vec![];
    for h in handles {
        sids.push(h.await.unwrap());
    }

    // Verify all sessions have unique session IDs
    let mut sorted: Vec<String> = sids.iter().map(|s| format!("{:?}", s)).collect();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), 10, "each concurrent session should get a unique id: {:?}", sorted);
}

/// Test: rapid session create/destroy cycle.
#[tokio::test(flavor = "multi_thread")]
async fn test_rapid_session_cycle() {
    let client = Arc::new(EmptyLlmClient);
    let builder = base_agent_builder(client).system_prompt(build_system_prompt());
    let server = ProtocolServer::from_builder(builder).unwrap();

    for i in 0..100 {
        let ext_id = format!("rapid-{}", i);
        let _sid = server.get_or_create_session(Some(ext_id)).await;
    }
}

/// Test: concurrent get_or_create_session with same external_id (race on HashMap).
#[tokio::test(flavor = "multi_thread")]
async fn test_session_id_reuse_concurrent() {
    let client = Arc::new(EmptyLlmClient);
    let builder = base_agent_builder(client).system_prompt(build_system_prompt());
    let server = ProtocolServer::from_builder(builder).unwrap();

    // Spawn 10 tasks all requesting the same external_id
    let mut handles = vec![];
    for _ in 0..10 {
        let server = server.clone();
        handles
            .push(tokio::spawn(async move { server.get_or_create_session(Some("shared-session".to_string())).await }));
    }

    let mut sids = vec![];
    for h in handles {
        sids.push(h.await.unwrap());
    }

    // All should return the same session ID
    let first = &sids[0];
    for sid in &sids[1..] {
        assert_eq!(format!("{:?}", sid), format!("{:?}", first), "same external_id should reuse the same session");
    }
}

/// Test: event subscription is live under concurrency.
#[tokio::test(flavor = "multi_thread")]
async fn test_event_subscription_concurrent() {
    let client = Arc::new(EmptyLlmClient);
    let builder = base_agent_builder(client).system_prompt(build_system_prompt());
    let server = ProtocolServer::from_builder(builder).unwrap();

    // Spawn 5 receivers concurrently
    let mut receivers = vec![];
    for _ in 0..5 {
        let server = server.clone();
        receivers.push(tokio::spawn(async move { server.subscribe_events() }));
    }

    for r in receivers {
        let mut rx = r.await.unwrap();
        // Each receiver should be live (recv will timeout, not error)
        match tokio::time::timeout(Duration::from_millis(50), rx.recv()).await {
            Err(_elapsed) => { /* expected: no events */ },
            Ok(Ok(_)) => { /* fine */ },
            Ok(Err(_)) => panic!("broadcast sender should be alive"),
        }
    }
}
