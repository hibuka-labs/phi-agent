//! NDJSON protocol message types shared between phi-agent and language SDKs.
//!
//! This module has **zero** dependency on `agent-base` — it is a pure serde
//! contract. Message fields are self-documenting by their names.
#![allow(missing_docs)]
//! contract.  SDK authors can use this file as the authoritative reference for
//! the wire format without pulling in the entire Rust crate.
//!
//! # Protocol overview
//!
//! - **Transport**: stdio, one JSON object per line (NDJSON).
//! - **Schema rule**: new fields may be added at any time (receivers MUST ignore
//!   unknown fields).  Removing or re-typing a field is a MAJOR version change.
//!
//! # Message flow
//!
//! ```text
//! SDK → phi serve         SDK ← phi serve
//! ─────────────────       ─────────────────
//! register_tool           hello (on connect)
//! create_session          session_created
//! run                     event
//! tool_result             tool_call
//! list_tools              tools_listed
//! cancel                  done
//!                         error
//! ```

use agent_base::ToolMetadata as AgentToolMetadata;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: u32 = 1;

// ── Tool metadata (bridge-facing, mirrors agent_base::ToolMetadata) ────

/// Stable wire-format representation of a registered tool's metadata.
/// Mirrors `agent_base::ToolMetadata` without depending on agent-base so
/// SDK authors can read this file as a pure serde contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetadata {
    pub name: String,
    pub description: String,
    pub origin: String,
    pub version: String,
    pub requirements: Vec<String>,
}

impl From<AgentToolMetadata> for ToolMetadata {
    fn from(m: AgentToolMetadata) -> Self {
        Self {
            name: m.name,
            description: m.description,
            origin: m.origin,
            version: m.version,
            requirements: m.requirements,
        }
    }
}

// ── Incoming (SDK → phi serve) ────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IncomingMessage {
    RegisterTool {
        name: String,
        description: String,
        parameters: Value,
    },
    CreateSession {
        #[serde(default)]
        session_id: Option<String>,
    },
    Run {
        #[serde(default)]
        session_id: String,
        query: String,
        #[serde(default)]
        config: Option<RunConfig>,
    },
    ToolResult {
        call_id: String,
        summary: String,
        #[serde(default)]
        raw: Option<Value>,
        #[serde(default)]
        control_flow: Option<String>,
    },
    Cancel {
        #[serde(default)]
        session_id: String,
    },
    ListTools {},
}

#[derive(Debug, Deserialize, Default)]
pub struct RunConfig {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    pub enable_thinking: Option<bool>,
    pub thinking_budget: Option<u64>,
    pub thinking_effort: Option<String>,
    pub max_tool_calls_per_turn: Option<usize>,
    pub max_consecutive_failures: Option<usize>,
    pub max_turns: Option<u32>,
}

// ── Outgoing (phi serve → SDK) ────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutgoingMessage {
    Hello {
        protocol_version: u32,
        server_name: String,
        server_version: String,
    },
    SessionCreated {
        session_id: Option<String>,
        internal_id: u64,
    },
    Event {
        seq: u64,
        #[serde(flatten)]
        event: Value,
    },
    ToolCall {
        seq: u64,
        call_id: String,
        name: String,
        args: Value,
    },
    ToolRegistered {
        name: String,
        ok: bool,
    },
    Done {
        seq: u64,
        outcome: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        turns: Option<u32>,
    },
    Error {
        code: String,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<Value>,
    },
    ToolsListed {
        tools: Vec<ToolMetadata>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_base::ToolMetadata as AgentToolMetadata;

    // ── IncomingMessage deserialization ──

    #[test]
    fn test_deserialize_all_incoming_variants() {
        // register_tool
        let json = r#"{"type":"register_tool","name":"shell","description":"run shell","parameters":{}}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, IncomingMessage::RegisterTool { .. }));

        // create_session with session_id
        let json = r#"{"type":"create_session","session_id":"ext-1"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, IncomingMessage::CreateSession { .. }));

        // create_session without session_id
        let json = r#"{"type":"create_session"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, IncomingMessage::CreateSession { session_id: None }));

        // run
        let json = r#"{"type":"run","session_id":"abc","query":"hello"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, IncomingMessage::Run { .. }));

        // tool_result
        let json = r#"{"type":"tool_result","call_id":"c1","summary":"done"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, IncomingMessage::ToolResult { .. }));

        // cancel
        let json = r#"{"type":"cancel","session_id":"abc"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, IncomingMessage::Cancel { .. }));

        // list_tools
        let json = r#"{"type":"list_tools"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, IncomingMessage::ListTools {}));
    }

    #[test]
    fn test_run_config_all_fields_populated() {
        let json = r#"{
            "type":"run",
            "session_id":"s1",
            "query":"test",
            "config":{
                "model":"gpt-4",
                "api_key":"sk-xxx",
                "base_url":"https://example.com/v1",
                "enable_thinking":true,
                "thinking_budget":32000,
                "thinking_effort":"high",
                "max_tool_calls_per_turn":10,
                "max_consecutive_failures":3,
                "max_turns":5
            }
        }"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        if let IncomingMessage::Run { config: Some(cfg), .. } = &msg {
            assert_eq!(cfg.model.as_deref(), Some("gpt-4"));
            assert_eq!(cfg.api_key.as_deref(), Some("sk-xxx"));
            assert_eq!(cfg.base_url.as_deref(), Some("https://example.com/v1"));
            assert_eq!(cfg.enable_thinking, Some(true));
            assert_eq!(cfg.thinking_budget, Some(32000));
            assert_eq!(cfg.thinking_effort.as_deref(), Some("high"));
            assert_eq!(cfg.max_tool_calls_per_turn, Some(10));
            assert_eq!(cfg.max_consecutive_failures, Some(3));
            assert_eq!(cfg.max_turns, Some(5));
        } else {
            panic!("expected Run with config");
        }
    }

    #[test]
    fn test_run_config_default_all_none() {
        let cfg: RunConfig = serde_json::from_str("{}").unwrap();
        assert!(cfg.model.is_none());
        assert!(cfg.api_key.is_none());
        assert!(cfg.base_url.is_none());
        assert!(cfg.enable_thinking.is_none());
        assert!(cfg.thinking_budget.is_none());
        assert!(cfg.thinking_effort.is_none());
        assert!(cfg.max_tool_calls_per_turn.is_none());
        assert!(cfg.max_consecutive_failures.is_none());
        assert!(cfg.max_turns.is_none());
    }

    #[test]
    fn test_run_without_config_uses_none() {
        let json = r#"{"type":"run","session_id":"s1","query":"test"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        if let IncomingMessage::Run { config, .. } = &msg {
            assert!(config.is_none());
        } else {
            panic!("expected Run");
        }
    }

    #[test]
    fn test_unknown_fields_ignored() {
        let json = r#"{"type":"list_tools","extra_field":"should-be-ignored","nested":{"a":1}}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, IncomingMessage::ListTools {}));
    }

    #[test]
    fn test_missing_type_field_errors() {
        let json = r#"{"session_id":"abc","query":"hello"}"#;
        let result = serde_json::from_str::<IncomingMessage>(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_tool_result_with_all_fields() {
        let json =
            r#"{"type":"tool_result","call_id":"c1","summary":"done","raw":{"output":"hello"},"control_flow":"break"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        if let IncomingMessage::ToolResult { call_id, summary, raw, control_flow } = &msg {
            assert_eq!(call_id, "c1");
            assert_eq!(summary, "done");
            assert_eq!(raw.as_ref().and_then(|v| v.get("output")).and_then(|v| v.as_str()), Some("hello"));
            assert_eq!(control_flow.as_deref(), Some("break"));
        } else {
            panic!("expected ToolResult");
        }
    }

    // ── OutgoingMessage serialization ──

    #[test]
    fn test_serialize_all_outgoing_variants() {
        // Hello
        let msg =
            OutgoingMessage::Hello { protocol_version: 1, server_name: "phi".into(), server_version: "0.2.6".into() };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["type"], "hello");
        assert_eq!(json["protocol_version"], 1);

        // SessionCreated
        let msg = OutgoingMessage::SessionCreated { session_id: Some("ext-1".into()), internal_id: 42 };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["type"], "session_created");
        assert_eq!(json["internal_id"], 42);

        // Event (flattened — inner "type" overrides the enum tag)
        let msg = OutgoingMessage::Event { seq: 1, event: serde_json::json!({"type":"text_delta","text":"hi"}) };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["seq"], 1);
        assert_eq!(json["text"], "hi"); // flattened field present

        // ToolCall
        let msg = OutgoingMessage::ToolCall {
            seq: 2,
            call_id: "c1".into(),
            name: "shell".into(),
            args: serde_json::json!({"cmd":"ls"}),
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["type"], "tool_call");
        assert_eq!(json["call_id"], "c1");

        // ToolRegistered
        let msg = OutgoingMessage::ToolRegistered { name: "shell".into(), ok: true };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["type"], "tool_registered");
        assert!(json["ok"].as_bool().unwrap());

        // Done
        let msg = OutgoingMessage::Done { seq: 3, outcome: "completed".into(), error: None, turns: Some(1) };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["type"], "done");
        assert_eq!(json["outcome"], "completed");
        assert!(json.get("error").is_none());

        // Done with error (error should appear)
        let msg = OutgoingMessage::Done {
            seq: 4,
            outcome: "failed".into(),
            error: Some("something went wrong".into()),
            turns: None,
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["error"], "something went wrong");
        assert!(json.get("turns").is_none());

        // Error
        let msg = OutgoingMessage::Error { code: "E001".into(), message: "bad request".into(), detail: None };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["type"], "error");
        assert!(json.get("detail").is_none());

        // ToolsListed
        let msg = OutgoingMessage::ToolsListed { tools: vec![] };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["type"], "tools_listed");
    }

    // ── ToolMetadata ──

    #[test]
    fn test_tool_metadata_from_agent_tool_metadata() {
        let am = AgentToolMetadata {
            name: "shell".into(),
            description: "Run shell commands".into(),
            origin: "phi-tools".into(),
            version: "1.0.0".into(),
            requirements: vec!["bash".into()],
        };
        let tm = ToolMetadata::from(am);
        assert_eq!(tm.name, "shell");
        assert_eq!(tm.description, "Run shell commands");
        assert_eq!(tm.origin, "phi-tools");
        assert_eq!(tm.version, "1.0.0");
        assert_eq!(tm.requirements, vec!["bash"]);
    }

    #[test]
    fn test_tool_metadata_round_trip() {
        let tm = ToolMetadata {
            name: "shell".into(),
            description: "desc".into(),
            origin: "phi".into(),
            version: "1.0".into(),
            requirements: vec!["bash".into(), "zsh".into()],
        };
        let json = serde_json::to_string(&tm).unwrap();
        let back: ToolMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, tm.name);
        assert_eq!(back.description, tm.description);
        assert_eq!(back.origin, tm.origin);
        assert_eq!(back.version, tm.version);
        assert_eq!(back.requirements, tm.requirements);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;

    proptest::proptest! {
        #[test]
        fn incoming_message_deser_never_panics(json in ".*") {
            let _ = serde_json::from_str::<IncomingMessage>(&json);
        }

        #[test]
        fn incoming_message_unknown_type_returns_err(type_name in "[a-z_]{1,20}") {
            let known = ["register_tool", "create_session", "run", "tool_result", "cancel", "list_tools"];
            let json = format!(r#"{{"type":"{}","name":"x","description":"d","parameters":{{}},"session_id":"s","query":"q","call_id":"c","summary":"s"}}"#, type_name);
            let result = serde_json::from_str::<IncomingMessage>(&json);
            if known.contains(&type_name.as_str()) {
                proptest::prop_assert!(result.is_ok(), "known type '{}' should deserialize", type_name);
            } else {
                proptest::prop_assert!(result.is_err(), "unknown type '{}' should fail", type_name);
            }
        }
    }
}
