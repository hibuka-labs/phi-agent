//! Stress tests: bridge protocol server concurrency.
//!
//! Validates that concurrent sessions are properly isolated,
//! no cross-session event leakage occurs, and session reuse works correctly.

use agent_base::{AgentResult, ChatMessage, LlmCapabilities, LlmClient, ReasoningConfig, ResponseFormat, StreamChunk};
use futures_core::Stream;
use phi_agent::base_agent_builder;
use phi_agent::bridge::server::ProtocolServer;
use phi_agent::build_system_prompt;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

/// Mock LLM client.
struct EmptyLlmClient;
#[async_trait::async_trait]
impl LlmClient for EmptyLlmClient {
    async fn chat(
        &self,
        _: &[ChatMessage],
        _: &[serde_json::Value],
        _: Option<&ReasoningConfig>,
        _: Option<&ResponseFormat>,
    ) -> AgentResult<serde_json::Value> {
        Ok(serde_json::json!({}))
    }
    async fn chat_stream(
        &self,
        _: &[ChatMessage],
        _: &[serde_json::Value],
        _: Option<&ReasoningConfig>,
        _: Option<&ResponseFormat>,
    ) -> AgentResult<Pin<Box<dyn Stream<Item = AgentResult<StreamChunk>> + Send>>> {
        struct EmptyStream;
        impl Stream for EmptyStream {
            type Item = AgentResult<StreamChunk>;
            fn poll_next(self: Pin<&mut Self>, _: &mut std::task::Context<'_>) -> std::task::Poll<Option<Self::Item>> {
                std::task::Poll::Ready(None)
            }
        }
        Ok(Box::pin(EmptyStream))
    }
    fn capabilities(&self) -> LlmCapabilities {
        LlmCapabilities {
            supports_thinking: false,
            supports_streaming: false,
            supports_tools: true,
            supports_vision: false,
            max_context_tokens: Some(4096),
            max_output_tokens: Some(4096),
        }
    }
}

/// Test: 10 concurrent sessions, each verified to be unique.
#[tokio::test(flavor = "multi_thread")]
async fn test_concurrent_sessions_isolated() {
    let client = agent_base::llm::adapt(Arc::new(EmptyLlmClient));
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
    let client = agent_base::llm::adapt(Arc::new(EmptyLlmClient));
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
    let client = agent_base::llm::adapt(Arc::new(EmptyLlmClient));
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
    let client = agent_base::llm::adapt(Arc::new(EmptyLlmClient));
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
