//! MCP / Bridge protocol integration tests.
//!
//! Validates the full ProtocolServer lifecycle — the bridge layer that the
//! MCP server (`phi serve`) uses. Tests session management, tool registration,
//! event forwarding, and tool listing in a single integrated process.
//!
//! These complement `bridge_concurrent.rs` (concurrency) by testing
//! correctness of the sequential protocol flow.

use agent_base::{
    AgentResult, ChatMessage, Content, LlmCapabilities, LlmClient, ReasoningConfig, ResponseFormat, StreamChunk,
};
use futures_core::Stream;
use phi_agent::base_agent_builder;
use phi_agent::bridge::server::ProtocolServer;
use phi_agent::build_system_prompt;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

// ── Mock LLM client (same pattern as bridge_concurrent.rs) ──────────────

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

fn build_server() -> ProtocolServer {
    let client = agent_base::llm::adapt(Arc::new(EmptyLlmClient));
    let builder = base_agent_builder(client).system_prompt(build_system_prompt());
    ProtocolServer::from_builder(builder).unwrap()
}

// ── Session lifecycle ───────────────────────────────────────────────────

/// Creating a session without external_id yields a unique internal ID each time.
#[tokio::test(flavor = "multi_thread")]
async fn test_create_session_anonymous() {
    let server = build_server();
    let (sid1, ext1) = server.create_session(None).await;
    let (sid2, ext2) = server.create_session(None).await;
    assert!(ext1.is_none());
    assert!(ext2.is_none());
    assert_ne!(sid1.id, sid2.id, "anonymous sessions should get unique ids");
}

/// Creating a session with external_id records the external_id.
#[tokio::test(flavor = "multi_thread")]
async fn test_create_session_with_external_id() {
    let server = build_server();
    let (sid, ext) = server.create_session(Some("my-session".into())).await;
    assert_eq!(ext.as_deref(), Some("my-session"));
    assert!(sid.id > 0);
}

/// get_or_create_session reuses the session for the same external_id.
#[tokio::test(flavor = "multi_thread")]
async fn test_get_or_create_session_reuses() {
    let server = build_server();
    let sid1 = server.get_or_create_session(Some("reuse-me".into())).await;
    let sid2 = server.get_or_create_session(Some("reuse-me".into())).await;
    assert_eq!(sid1.id, sid2.id, "same external_id should reuse the session");
    assert_eq!(sid1.external_id, sid2.external_id);
}

/// get_or_create_session without external_id always creates new sessions.
#[tokio::test(flavor = "multi_thread")]
async fn test_get_or_create_session_anon_always_new() {
    let server = build_server();
    let sid1 = server.get_or_create_session(None).await;
    let sid2 = server.get_or_create_session(None).await;
    assert_ne!(sid1.id, sid2.id, "anonymous sessions should always be new");
}

// ── Tool registration & listing ─────────────────────────────────────────

/// Register a proxy tool and verify it appears in the tool list.
#[tokio::test(flavor = "multi_thread")]
async fn test_register_and_list_proxy_tool() {
    let server = build_server();
    let tools_before = server.list_tools().await;

    server
        .register_tool("my_tool".into(), "A test tool".into(), serde_json::json!({"type": "object", "properties": {}}))
        .await;

    let tools_after = server.list_tools().await;
    assert_eq!(tools_after.len(), tools_before.len() + 1);
    let registered = tools_after.iter().find(|t| t.name == "my_tool").unwrap();
    assert_eq!(registered.description, "A test tool");
}

/// Multiple proxy tools can be registered and listed sorted by name.
#[tokio::test(flavor = "multi_thread")]
async fn test_register_multiple_proxy_tools() {
    let server = build_server();
    server.register_tool("zebra".into(), "z".into(), serde_json::json!({})).await;
    server.register_tool("alpha".into(), "a".into(), serde_json::json!({})).await;
    server.register_tool("mike".into(), "m".into(), serde_json::json!({})).await;

    let tools = server.list_tools().await;
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    // ToolMetadata list is sorted by name
    let sorted: Vec<&str> = {
        let mut v = names.clone();
        v.sort();
        v
    };
    assert_eq!(names, sorted, "tools should be listed in sorted order");
}

/// Listing tools on a fresh server returns an empty or factory-default list.
#[tokio::test(flavor = "multi_thread")]
async fn test_list_tools_on_fresh_server() {
    let server = build_server();
    let tools = server.list_tools().await;
    // A fresh server with no custom tools should have zero or only built-in tools
    // (phi-agent itself bundles no tools)
    for t in &tools {
        // All tools should have non-empty names
        assert!(!t.name.is_empty());
    }
}

// ── Event subscription ──────────────────────────────────────────────────

/// subscribe_events returns a live receiver.
#[tokio::test(flavor = "multi_thread")]
async fn test_event_subscription_live() {
    let server = build_server();
    let mut rx = server.subscribe_events();
    // Should time out (no events) but not error (sender is alive)
    match tokio::time::timeout(Duration::from_millis(50), rx.recv()).await {
        Err(_elapsed) => { /* expected: no events on a fresh server */ },
        Ok(Ok(_event)) => { /* also fine */ },
        Ok(Err(_)) => panic!("broadcast sender should be alive"),
    }
}

/// Multiple subscribers can receive independently.
#[tokio::test(flavor = "multi_thread")]
async fn test_multiple_event_subscribers() {
    let server = build_server();
    let mut rx1 = server.subscribe_events();
    let mut rx2 = server.subscribe_events();

    match tokio::time::timeout(Duration::from_millis(50), rx1.recv()).await {
        Err(_) | Ok(Ok(_)) => {},
        Ok(Err(_)) => panic!("rx1 sender should be alive"),
    }
    match tokio::time::timeout(Duration::from_millis(50), rx2.recv()).await {
        Err(_) | Ok(Ok(_)) => {},
        Ok(Err(_)) => panic!("rx2 sender should be alive"),
    }
}

// ── Cancel ──────────────────────────────────────────────────────────────

/// Cancel on an idle server should not panic.
#[tokio::test(flavor = "multi_thread")]
async fn test_cancel_idempotent() {
    let server = build_server();
    server.cancel();
    server.cancel(); // second cancel should be fine
}

// ── ProtocolServer clone ────────────────────────────────────────────────

/// ProtocolServer is Clone and shares state.
#[tokio::test(flavor = "multi_thread")]
async fn test_server_clone_shares_state() {
    let server = build_server();
    let server2 = server.clone();
    let sid = server.get_or_create_session(Some("clone-test".into())).await;
    let sid2 = server2.get_or_create_session(Some("clone-test".into())).await;
    assert_eq!(sid.id, sid2.id, "cloned server should share session map");
}

// ── Tool call prepare / response slot ───────────────────────────────────

/// Full proxy-tool round-trip: register a tool, prepare the slot, send
/// a result through it.  This is the contract between the bridge server
/// and SDK consumers: every tool call is preceded by `prepare_tool_call`,
/// and the SDK sends the result back via the returned sender.
#[tokio::test(flavor = "multi_thread")]
async fn test_proxy_tool_slot_round_trip() {
    let server = build_server();

    // 1. Register a proxy tool (simulates SDK registering a tool)
    server
        .register_tool(
            "bridge_tool".into(),
            "A tool bridged from SDK".into(),
            serde_json::json!({"type": "object", "properties": {"input": {"type": "string"}}}),
        )
        .await;

    // 2. Verify the tool appears in the listing with correct metadata
    let tools = server.list_tools().await;
    let registered = tools.iter().find(|t| t.name == "bridge_tool").expect("tool should be listed");
    assert_eq!(registered.description, "A tool bridged from SDK");
    assert_eq!(registered.origin, "custom");

    // 3. Prepare the slot for an incoming tool call
    let tx = server.prepare_tool_call().await;

    // 4. SDK sends the tool result back through the slot
    tx.send(Ok(vec![Content::text("bridge_tool executed: result=42".to_string())])).unwrap();

    // 5. After sending, the slot is consumed — a new prepare gives a fresh slot
    let tx2 = server.prepare_tool_call().await;
    assert!(tx2.send(Ok(vec![Content::text("second".to_string())])).is_ok());
}

/// prepare_tool_call creates a sender; sending a result closes the slot.
#[tokio::test(flavor = "multi_thread")]
async fn test_prepare_tool_call_slot() {
    let server = build_server();
    let tx = server.prepare_tool_call().await;
    let result = tx.send(Ok(vec![Content::text("done".to_string())]));
    assert!(result.is_ok(), "sending to the prepared slot should succeed");
}

/// prepare_tool_call creates a new slot each time.
#[tokio::test(flavor = "multi_thread")]
async fn test_prepare_tool_call_reuse() {
    let server = build_server();

    let tx1 = server.prepare_tool_call().await;
    tx1.send(Ok(vec![Content::text("first".to_string())])).unwrap();

    let tx2 = server.prepare_tool_call().await;
    tx2.send(Ok(vec![Content::text("second".to_string())])).unwrap();
}
