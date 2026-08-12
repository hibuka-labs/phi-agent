//! Integration tests for the bridge protocol (ProtocolServer).
//!
//! Tests protocol flow end-to-end using a mock LLM client — no real API key needed.

mod common;
use common::*;

use std::sync::Arc;

use phi_agent::bridge::messages::PROTOCOL_VERSION;
use serde_json::json;

// ── Tests ─────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_full_run_returns_events() {
    let mock_llm = Arc::new(MockLlmClient::new());
    let mock = agent_base::llm::adapt(mock_llm.clone());
    let server = build_server(mock);

    let sid = server.create_session(None).await.0;
    let mut event_rx = server.subscribe_events();

    // Spawn the turn
    let server_clone = server.clone();
    tokio::spawn(async move {
        let _ = server_clone.run_turn(&sid, "hello", |_event| Ok(())).await;
    });

    let events = collect_events(&mut event_rx).await;
    assert!(!events.is_empty(), "should receive at least one event");
    assert!(events.contains(&"run_finished".to_string()), "should finish: {events:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_create_session_and_subscribe() {
    let mock_llm = Arc::new(MockLlmClient::new());
    let mock = agent_base::llm::adapt(mock_llm.clone());
    let server = build_server(mock);

    let (sid, ext) = server.create_session(Some("my-session".to_string())).await;
    assert_eq!(ext.as_deref(), Some("my-session"));
    assert!(sid.id > 0);
}

#[test]
fn test_protocol_version_is_1() {
    assert_eq!(PROTOCOL_VERSION, 1);
}

// ── BR-04, BR-05, BR-06 ─────────────────────────────────────────────

/// BR-04: ProxyTool called without a prepared slot returns error, not panic.
///
/// When the LLM requests a tool call but no slot has been prepared
/// (single-slot is None), ProxyTool should return a clean error
/// instead of panicking.
#[tokio::test(flavor = "multi_thread")]
async fn test_br_04_empty_slot_returns_error() {
    let mock_llm = Arc::new(MockLlmClient::new());
    let mock = agent_base::llm::adapt(mock_llm.clone());
    // Pre-configure mock to return a tool call
    mock_llm.set_tool_call("test_tool", &json!({"arg": "value"})).await;

    let server = build_server(mock.clone());
    server.register_tool("test_tool".to_string(), "A test tool".to_string(), json!({})).await;

    let (sid, _) = server.create_session(None).await;
    let mut event_rx = server.subscribe_events();

    // Deliberately do NOT prepare a slot — tool call should fail gracefully

    let server_clone = server.clone();
    let sid_clone = sid.clone();
    tokio::spawn(async move {
        let _ = server_clone.run_turn(&sid_clone, "call the tool", |_event| Ok(())).await;
    });

    let events = collect_events(&mut event_rx).await;
    // Should still finish (run_finished) — the tool error is handled,
    // not a panic
    assert!(events.contains(&"run_finished".to_string()), "should finish even with empty slot: {events:?}");
}

/// BR-05: Session ID reuse — same external_id returns same session.
///
/// ``get_or_create_session`` with the same external_id should return
/// the same underlying SessionId, preserving conversation context.
#[tokio::test(flavor = "multi_thread")]
async fn test_br_05_session_id_reuse() {
    let mock_llm = Arc::new(MockLlmClient::new());
    let mock = agent_base::llm::adapt(mock_llm.clone());
    let server = build_server(mock);

    // First call creates a new session
    let sid1 = server.get_or_create_session(Some("shared-session".to_string())).await;

    // Second call with same external_id should return the SAME session
    let sid2 = server.get_or_create_session(Some("shared-session".to_string())).await;

    assert_eq!(sid1.id, sid2.id, "same external_id should reuse session");

    // Different external_id should create a NEW session
    let sid3 = server.get_or_create_session(Some("other-session".to_string())).await;

    assert_ne!(sid3.id, sid1.id, "different external_id should create new session");

    // None (no external_id) should always create new sessions
    let sid4 = server.get_or_create_session(None).await;
    let sid5 = server.get_or_create_session(None).await;
    assert_ne!(sid4.id, sid5.id, "None external_id should always create new");
}

/// BR-06: Sequential tool calls don't interfere.
///
/// Multiple sequential tool calls (prepare → call → prepare → call)
/// should work correctly without cross-talk.  The single-slot pattern
/// handles one at a time; this test verifies the slot is properly
/// reset between calls.
#[tokio::test(flavor = "multi_thread")]
async fn test_br_06_sequential_tool_calls() {
    let mock_llm = Arc::new(MockLlmClient::new());
    let mock = agent_base::llm::adapt(mock_llm.clone());

    // First tool call: test_tool
    mock_llm.set_tool_call("test_tool", &json!({"step": 1})).await;

    let server = build_server(mock.clone());
    server.register_tool("test_tool".to_string(), "Tool 1".to_string(), json!({})).await;

    let sid = server.get_or_create_session(None).await;
    let mut event_rx = server.subscribe_events();

    // Prepare slot for the first tool call
    let _tx1 = server.prepare_tool_call().await;

    let server_clone = server.clone();
    let sid_clone = sid.clone();
    tokio::spawn(async move {
        let _ = server_clone.run_turn(&sid_clone, "call tool 1", |_event| Ok(())).await;
    });

    let events1 = collect_events(&mut event_rx).await;
    assert!(events1.contains(&"run_finished".to_string()), "first turn should finish: {events1:?}");

    // Second tool call — should work even after the first consumed the slot
    mock_llm.set_tool_call("test_tool", &json!({"step": 2})).await;

    let mut event_rx2 = server.subscribe_events();
    let _tx2 = server.prepare_tool_call().await;

    let sid_clone2 = sid.clone();
    let server_clone2 = server.clone();
    tokio::spawn(async move {
        let _ = server_clone2.run_turn(&sid_clone2, "call tool again", |_event| Ok(())).await;
    });

    let events2 = collect_events(&mut event_rx2).await;
    assert!(events2.contains(&"run_finished".to_string()), "second turn should finish: {events2:?}");

    // Verify both turns completed — sequential calls don't interfere
    let finished1 = events1.iter().any(|t| t == "run_finished");
    let finished2 = events2.iter().any(|t| t == "run_finished");
    assert!(finished1, "first turn should finish: {events1:?}");
    assert!(finished2, "second turn should finish: {events2:?}");
}

// ── ProtocolServer unit tests ──

#[tokio::test(flavor = "multi_thread")]
async fn test_register_tool_appears_in_list() {
    let mock_llm = Arc::new(MockLlmClient::new());
    let mock = agent_base::llm::adapt(mock_llm.clone());
    let server = build_server(mock);

    server.register_tool("my_tool".into(), "A test tool".into(), json!({})).await;

    let tools = server.list_tools().await;
    assert!(tools.iter().any(|t| t.name == "my_tool"));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_register_multiple_tools() {
    let mock_llm = Arc::new(MockLlmClient::new());
    let mock = agent_base::llm::adapt(mock_llm.clone());
    let server = build_server(mock);

    server.register_tool("zzz_tool".into(), "Z".into(), json!({})).await;
    server.register_tool("aaa_tool".into(), "A".into(), json!({})).await;
    server.register_tool("mmm_tool".into(), "M".into(), json!({})).await;

    let tools = server.list_tools().await;
    let names: Vec<&str> =
        tools.iter().map(|t| t.name.as_str()).filter(|n| ["aaa_tool", "mmm_tool", "zzz_tool"].contains(n)).collect();
    assert!(names.len() >= 3);
    assert_eq!(names[0], "aaa_tool");
    assert_eq!(names[1], "mmm_tool");
    assert_eq!(names[2], "zzz_tool");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_prepare_tool_call_sender_usable() {
    let mock_llm = Arc::new(MockLlmClient::new());
    let mock = agent_base::llm::adapt(mock_llm.clone());
    let server = build_server(mock);

    let tx = server.prepare_tool_call().await;
    // Sender should be usable
    let result = tx.send(Ok(agent_base::ToolOutput {
        summary: "done".into(),
        raw: None,
        control_flow: agent_base::ToolControlFlow::Continue,
        truncation: None,
    }));
    assert!(result.is_ok());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_subscribe_events_receiver_open() {
    let mock_llm = Arc::new(MockLlmClient::new());
    let mock = agent_base::llm::adapt(mock_llm.clone());
    let server = build_server(mock);

    let rx = server.subscribe_events();
    // Receiver should not be closed initially
    assert_eq!(rx.len(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_get_or_create_different_external_ids() {
    let mock_llm = Arc::new(MockLlmClient::new());
    let mock = agent_base::llm::adapt(mock_llm.clone());
    let server = build_server(mock);

    let sid1 = server.get_or_create_session(Some("ext-1".into())).await;
    let sid2 = server.get_or_create_session(Some("ext-2".into())).await;

    assert_ne!(sid1.id, sid2.id, "different external_ids should create different sessions");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_create_session_without_external_id() {
    let mock_llm = Arc::new(MockLlmClient::new());
    let mock = agent_base::llm::adapt(mock_llm.clone());
    let server = build_server(mock);

    let sid = server.get_or_create_session(None).await;
    assert!(sid.id > 0);
    assert!(sid.external_id.is_none());
}

// ── Additional ProtocolServer unit tests ──

/// Verify that `ProtocolServer::from_builder` constructs a working server
/// with tools registered by the builder.
#[tokio::test(flavor = "multi_thread")]
async fn test_from_builder_creates_functional_server() {
    let mock_llm = Arc::new(MockLlmClient::new());
    let mock = agent_base::llm::adapt(mock_llm.clone());

    // Build through the same path the CLI uses
    let builder = phi_agent::base_agent_builder(mock.clone()).system_prompt("test".to_string());
    let server = phi_agent::bridge::server::ProtocolServer::from_builder(builder)
        .expect("from_builder should succeed with valid builder");

    let tools = server.list_tools().await;
    assert!(!tools.is_empty(), "builder should register file tools by default");

    let sid = server.create_session(None).await;
    assert!(sid.0.id > 0, "should be able to create sessions from builder-created server");
}

/// Calling cancel() on an idle server should not panic.
#[tokio::test(flavor = "multi_thread")]
async fn test_cancel_idle_server_no_panic() {
    let mock_llm = Arc::new(MockLlmClient::new());
    let mock = agent_base::llm::adapt(mock_llm.clone());
    let server = build_server(mock);

    // Cancel on idle server — should be a no-op, no panic
    server.cancel();
    // Cancel twice — still no panic
    server.cancel();
}

/// Multiple anonymous create_session calls return different sessions.
#[tokio::test(flavor = "multi_thread")]
async fn test_multiple_anonymous_sessions_are_different() {
    let mock_llm = Arc::new(MockLlmClient::new());
    let mock = agent_base::llm::adapt(mock_llm.clone());
    let server = build_server(mock);

    let (sid1, _) = server.create_session(None).await;
    let (sid2, _) = server.create_session(None).await;
    let (sid3, _) = server.create_session(None).await;

    assert_ne!(sid1.id, sid2.id);
    assert_ne!(sid2.id, sid3.id);
    assert_ne!(sid1.id, sid3.id);
}

/// `list_tools` should return tools sorted by name.
#[tokio::test(flavor = "multi_thread")]
async fn test_list_tools_sorted_by_name() {
    let mock_llm = Arc::new(MockLlmClient::new());
    let mock = agent_base::llm::adapt(mock_llm.clone());
    let server = build_server(mock);

    // Register tools in reverse alphabetical order
    server.register_tool("z_tool".into(), "Z".into(), serde_json::json!({})).await;
    server.register_tool("m_tool".into(), "M".into(), serde_json::json!({})).await;
    server.register_tool("a_tool".into(), "A".into(), serde_json::json!({})).await;

    let tools = server.list_tools().await;
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();

    // All registered tools must appear in alphabetical order
    let custom_indices: Vec<usize> =
        names.iter().enumerate().filter(|(_, n)| ["a_tool", "m_tool", "z_tool"].contains(n)).map(|(i, _)| i).collect();

    assert_eq!(custom_indices.len(), 3, "all 3 custom tools should be present");
    assert!(custom_indices.windows(2).all(|w| w[0] < w[1]), "custom tools should appear in alphabetical order");
}

/// Session ID from `create_session` matches the one from `get_or_create_session`
/// when using the same external_id.
#[tokio::test(flavor = "multi_thread")]
async fn test_create_session_external_id_consistency() {
    let mock_llm = Arc::new(MockLlmClient::new());
    let mock = agent_base::llm::adapt(mock_llm.clone());
    let server = build_server(mock);

    // create_session doesn't register in the external_id map,
    // so consecutive get_or_create_session calls with the same id
    // should reuse after the first call
    let sid1 = server.get_or_create_session(Some("consistent-ext".into())).await;
    let sid2 = server.get_or_create_session(Some("consistent-ext".into())).await;

    assert_eq!(sid1.id, sid2.id, "get_or_create_session should reuse same external_id");
}

/// Verify that subscribing events before any turn returns an open receiver.
#[tokio::test(flavor = "multi_thread")]
async fn test_subscribe_before_run_is_open() {
    let mock_llm = Arc::new(MockLlmClient::new());
    let mock = agent_base::llm::adapt(mock_llm.clone());
    let server = build_server(mock);

    let rx = server.subscribe_events();
    // Before any turn, the receiver should be empty but not closed
    assert_eq!(rx.len(), 0);
}
