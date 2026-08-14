//! Event log: serialize turn events to JSONL files.
//!
//! Provides event persistence integrated with SessionContext,
//! shared across CLI, Web, and other consumers.
//!
//! Note: `save_turn_log` performs synchronous file I/O. Consumers should
//! call it via `tokio::task::spawn_blocking` in async contexts to avoid
//! blocking the runtime.

use std::io::Write;

use agent_base::{AgentResult, RuntimeEvent, UserEvent};

use crate::session::SessionContext;

/// Save all events from a turn to a JSONL file.
///
/// Performs synchronous file I/O. Callers should invoke this via
/// `tokio::task::spawn_blocking`.
pub fn save_turn_log(
    session_ctx: &SessionContext,
    turn: u32,
    events: &[RuntimeEvent],
    user_input: &str,
) -> AgentResult<()> {
    let turn_path = session_ctx.turn_path(turn as usize);
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&turn_path)?;

    let meta = serde_json::json!({
        "turn": turn,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "user_input": user_input,
    });
    writeln!(file, "{}", serde_json::to_string(&meta)?)?;

    for event in events {
        let line = event_to_jsonl(event);
        writeln!(file, "{}", line)?;
    }

    writeln!(file, "{}", serde_json::to_string(&serde_json::json!({"type": "turn_end", "turn": turn}))?)?;

    file.flush()?;

    Ok(())
}

/// Convert a RuntimeEvent to a JSON Value (shared by event_log and render).
pub fn event_to_value(event: &RuntimeEvent) -> serde_json::Value {
    let mut value = match event {
        RuntimeEvent::ThoughtDelta { text, .. } => {
            serde_json::json!({"type": "thought_delta", "text": text})
        },
        RuntimeEvent::TextDelta { text, .. } => {
            serde_json::json!({"type": "text_delta", "text": text})
        },
        RuntimeEvent::ToolCallStarted { tool_name, args_json, .. } => {
            let args: serde_json::Value = serde_json::from_str(args_json).unwrap_or(serde_json::Value::Null);
            serde_json::json!({"type": "tool_call_started", "tool": tool_name, "args": args})
        },
        RuntimeEvent::ToolCallFinished { tool_name, summary, denied, .. } => {
            serde_json::json!({"type": "tool_call_finished", "tool": tool_name, "summary": summary, "denied": denied})
        },
        RuntimeEvent::AwaitingApproval { request, .. } => {
            serde_json::json!({"type": "approval_request", "title": request.title, "message": request.message})
        },
        RuntimeEvent::PlanUpdated { explanation, plan, .. } => {
            serde_json::json!({"type": "plan_updated", "explanation": explanation, "plan": plan})
        },
        RuntimeEvent::UserEvent { event: UserEvent::Structured { event_type, data }, .. } => {
            serde_json::json!({"type": "user_event", "event_type": event_type, "data": data})
        },
        RuntimeEvent::UserEvent { .. } => serde_json::json!({"type": "other"}),
        RuntimeEvent::RunCancelled { .. } => serde_json::json!({"type": "run_cancelled"}),
        RuntimeEvent::RunFinished { .. } => serde_json::json!({"type": "run_finished"}),
        RuntimeEvent::Checkpoint { .. } => serde_json::json!({"type": "checkpoint"}),
    };
    if let Some(agent_id) = event.agent_id() {
        value["agent_id"] = serde_json::json!(agent_id);
    }
    value
}

/// Convert a RuntimeEvent to a JSONL line.
pub fn event_to_jsonl(event: &RuntimeEvent) -> String {
    let value = event_to_value(event);
    serde_json::to_string(&value).unwrap_or_else(|_| r#"{"type":"serialize_error"}"#.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_base::{SessionId, UserEvent};
    use tempfile::TempDir;

    fn session_id() -> SessionId {
        SessionId { id: 1, external_id: None }
    }

    // ── event_to_value tests ──

    #[test]
    fn test_event_to_value_text_delta() {
        let v = event_to_value(&RuntimeEvent::TextDelta {
            session_id: session_id(),
            text: "hello".into(),
            agent_id: None,
            trace_id: None,
        });
        assert_eq!(v["type"], "text_delta");
        assert_eq!(v["text"], "hello");
    }

    #[test]
    fn test_event_to_value_thought_delta() {
        let v = event_to_value(&RuntimeEvent::ThoughtDelta {
            session_id: session_id(),
            text: "thinking".into(),
            agent_id: None,
            trace_id: None,
        });
        assert_eq!(v["type"], "thought_delta");
    }

    #[test]
    fn test_event_to_value_tool_call_started() {
        let v = event_to_value(&RuntimeEvent::ToolCallStarted {
            session_id: session_id(),
            tool_name: "shell".into(),
            args_json: r#"{"cmd":"ls"}"#.into(),
            agent_id: None,
            trace_id: None,
        });
        assert_eq!(v["type"], "tool_call_started");
        assert_eq!(v["tool"], "shell");
        assert_eq!(v["args"]["cmd"], "ls");
    }

    #[test]
    fn test_event_to_value_tool_call_finished() {
        let v = event_to_value(&RuntimeEvent::ToolCallFinished {
            session_id: session_id(),
            tool_name: "shell".into(),
            summary: "done".into(),
            agent_id: None,
            trace_id: None,
            denied: false,
        });
        assert_eq!(v["type"], "tool_call_finished");
        assert_eq!(v["summary"], "done");
        assert_eq!(v["denied"], false);
    }

    #[test]
    fn test_event_to_value_run_finished() {
        let v = event_to_value(&RuntimeEvent::RunFinished { session_id: session_id(), agent_id: None, trace_id: None });
        assert_eq!(v["type"], "run_finished");
    }

    #[test]
    fn test_event_to_value_run_cancelled() {
        let v =
            event_to_value(&RuntimeEvent::RunCancelled { session_id: session_id(), agent_id: None, trace_id: None });
        assert_eq!(v["type"], "run_cancelled");
    }

    #[test]
    fn test_event_to_value_user_event_other() {
        let v = event_to_value(&RuntimeEvent::UserEvent {
            session_id: session_id(),
            event: UserEvent::Progress { text: "loading".into() },
            agent_id: None,
            trace_id: None,
        });
        assert_eq!(v["type"], "other");
    }

    #[test]
    fn test_event_to_value_user_event_structured() {
        let v = event_to_value(&RuntimeEvent::UserEvent {
            session_id: session_id(),
            event: UserEvent::Structured { event_type: "custom".into(), data: serde_json::json!({"key": "value"}) },
            agent_id: None,
            trace_id: None,
        });
        assert_eq!(v["type"], "user_event");
        assert_eq!(v["event_type"], "custom");
    }

    // ── event_to_jsonl tests ──

    #[test]
    fn test_event_to_jsonl_returns_valid_json() {
        let line = event_to_jsonl(&RuntimeEvent::TextDelta {
            session_id: session_id(),
            text: "hello".into(),
            agent_id: None,
            trace_id: None,
        });
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["type"], "text_delta");
    }

    #[test]
    fn test_event_to_jsonl_all_variants_valid() {
        let events: Vec<RuntimeEvent> = vec![
            RuntimeEvent::TextDelta { session_id: session_id(), text: "t".into(), agent_id: None, trace_id: None },
            RuntimeEvent::ThoughtDelta { session_id: session_id(), text: "t".into(), agent_id: None, trace_id: None },
            RuntimeEvent::ToolCallStarted {
                session_id: session_id(),
                tool_name: "t".into(),
                args_json: "{}".into(),
                agent_id: None,
                trace_id: None,
            },
            RuntimeEvent::ToolCallFinished {
                session_id: session_id(),
                tool_name: "t".into(),
                summary: "s".into(),
                agent_id: None,
                trace_id: None,
                denied: false,
            },
            RuntimeEvent::RunFinished { session_id: session_id(), agent_id: None, trace_id: None },
            RuntimeEvent::RunCancelled { session_id: session_id(), agent_id: None, trace_id: None },
            RuntimeEvent::UserEvent {
                session_id: session_id(),
                event: UserEvent::Progress { text: "t".into() },
                agent_id: None,
                trace_id: None,
            },
        ];
        for e in &events {
            let line = event_to_jsonl(e);
            assert!(serde_json::from_str::<serde_json::Value>(&line).is_ok(), "Failed to parse JSONL for event");
        }
    }

    // ── save_turn_log tests ──

    fn make_session_ctx(tmp: &TempDir) -> SessionContext {
        crate::session::resolve_session(Some("test-session"), tmp.path()).unwrap()
    }

    #[test]
    fn test_save_turn_log_creates_file() {
        let tmp = TempDir::new().unwrap();
        let ctx = make_session_ctx(&tmp);
        let events = vec![RuntimeEvent::TextDelta {
            session_id: session_id(),
            text: "hello".into(),
            agent_id: None,
            trace_id: None,
        }];
        save_turn_log(&ctx, 1, &events, "test input").unwrap();
        let path = ctx.turn_path(1);
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(!content.is_empty());
    }

    #[test]
    fn test_save_turn_log_contains_metadata() {
        let tmp = TempDir::new().unwrap();
        let ctx = make_session_ctx(&tmp);
        let events = vec![RuntimeEvent::TextDelta {
            session_id: session_id(),
            text: "hello".into(),
            agent_id: None,
            trace_id: None,
        }];
        save_turn_log(&ctx, 1, &events, "my input").unwrap();
        let content = std::fs::read_to_string(ctx.turn_path(1)).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(first["turn"], 1);
        assert_eq!(first["user_input"], "my input");
        assert!(first["timestamp"].as_str().is_some());
    }

    #[test]
    fn test_save_turn_log_contains_events() {
        let tmp = TempDir::new().unwrap();
        let ctx = make_session_ctx(&tmp);
        let events = vec![
            RuntimeEvent::TextDelta { session_id: session_id(), text: "hello".into(), agent_id: None, trace_id: None },
            RuntimeEvent::RunFinished { session_id: session_id(), agent_id: None, trace_id: None },
        ];
        save_turn_log(&ctx, 1, &events, "input").unwrap();
        let content = std::fs::read_to_string(ctx.turn_path(1)).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        // Line 0: metadata, Line 1: text_delta, Line 2: run_finished, Line 3: turn_end
        assert!(lines.len() >= 4);
        let line1: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(line1["type"], "text_delta");
        let line2: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
        assert_eq!(line2["type"], "run_finished");
    }

    #[test]
    fn test_save_turn_log_contains_turn_end_marker() {
        let tmp = TempDir::new().unwrap();
        let ctx = make_session_ctx(&tmp);
        let events = vec![RuntimeEvent::TextDelta {
            session_id: session_id(),
            text: "hello".into(),
            agent_id: None,
            trace_id: None,
        }];
        save_turn_log(&ctx, 1, &events, "input").unwrap();
        let content = std::fs::read_to_string(ctx.turn_path(1)).unwrap();
        let last_line = content.lines().last().unwrap();
        let v: serde_json::Value = serde_json::from_str(last_line).unwrap();
        assert_eq!(v["type"], "turn_end");
        assert_eq!(v["turn"], 1);
    }

    #[test]
    fn test_save_turn_log_appends() {
        let tmp = TempDir::new().unwrap();
        let ctx = make_session_ctx(&tmp);
        let events1 = vec![RuntimeEvent::TextDelta {
            session_id: session_id(),
            text: "turn1".into(),
            agent_id: None,
            trace_id: None,
        }];
        let events2 = vec![RuntimeEvent::TextDelta {
            session_id: session_id(),
            text: "turn2".into(),
            agent_id: None,
            trace_id: None,
        }];
        save_turn_log(&ctx, 1, &events1, "input1").unwrap();
        save_turn_log(&ctx, 1, &events2, "input2").unwrap();
        let content = std::fs::read_to_string(ctx.turn_path(1)).unwrap();
        assert!(content.contains("turn1"));
        assert!(content.contains("turn2"));
        // Should have two turn_end markers
        let turn_end_count = content.lines().filter(|l| l.contains("turn_end")).count();
        assert_eq!(turn_end_count, 2);
    }

    #[test]
    fn test_save_turn_log_empty_events() {
        let tmp = TempDir::new().unwrap();
        let ctx = make_session_ctx(&tmp);
        save_turn_log(&ctx, 1, &[], "no events").unwrap();
        let content = std::fs::read_to_string(ctx.turn_path(1)).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        // metadata header + turn_end marker
        assert_eq!(lines.len(), 2);
        let last: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(last["type"], "turn_end");
        assert_eq!(last["turn"], 1);
    }

    // ── agent_id serialization tests ──

    #[test]
    fn test_event_to_value_attaches_agent_id() {
        let v = event_to_value(&RuntimeEvent::TextDelta {
            session_id: session_id(),
            text: "result".into(),
            agent_id: Some("root/searcher".into()),
            trace_id: None,
        });
        assert_eq!(v["type"], "text_delta");
        assert_eq!(v["text"], "result");
        assert_eq!(v["agent_id"], "root/searcher");
    }

    #[test]
    fn test_event_to_value_omits_agent_id_when_none() {
        let v = event_to_value(&RuntimeEvent::TextDelta {
            session_id: session_id(),
            text: "result".into(),
            agent_id: None,
            trace_id: None,
        });
        assert_eq!(v["type"], "text_delta");
        assert!(v.get("agent_id").is_none());
    }
}
