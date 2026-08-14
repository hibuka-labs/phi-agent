#![allow(missing_docs)]

use std::io::{self, Write};

use agent_base::{AgentResult, RuntimeEvent, UserEvent};
use serde_json::{Value, json};

use crate::render::EventRenderer;

/// JSON stream renderer: outputs one JSON line per event (JSONL format).
/// Suitable for IDE integrations and programmatic consumers.
pub struct JsonStreamRenderer {
    writer: Box<dyn Write + Send>,
    turn_start: Option<std::time::Instant>,
    tool_call_count: u32,
    last_assistant_text: String,
}

impl JsonStreamRenderer {
    /// Create a new JSON stream renderer writing to the given writer.
    pub fn new(writer: Box<dyn Write + Send>) -> Self {
        Self { writer, turn_start: None, tool_call_count: 0, last_assistant_text: String::new() }
    }

    /// Create a renderer that writes to stdout.
    pub fn stdout() -> Self {
        Self::new(Box::new(io::stdout()))
    }

    fn emit(&mut self, value: &Value) -> AgentResult<()> {
        let line = serde_json::to_string(value)
            .map_err(|e| agent_base::AgentError::internal(format!("JSON serialize error: {e}")))?;
        writeln!(self.writer, "{}", line).map_err(|e| agent_base::AgentError::internal(format!("write error: {e}")))?;
        Ok(())
    }

    /// Emit an event line, attaching `agent_id` when the event carries one.
    fn emit_event(&mut self, event: &RuntimeEvent, mut value: Value) -> AgentResult<()> {
        if let Some(agent_id) = event.agent_id() {
            value["agent_id"] = json!(agent_id);
        }
        self.emit(&value)
    }
}

impl EventRenderer for JsonStreamRenderer {
    fn render(&mut self, event: RuntimeEvent) -> AgentResult<()> {
        if self.turn_start.is_none() {
            self.turn_start = Some(std::time::Instant::now());
        }

        match &event {
            RuntimeEvent::ThoughtDelta { text, .. } => {
                self.emit_event(&event, json!({ "type": "thought_delta", "text": text }))?;
            },
            RuntimeEvent::TextDelta { text, .. } => {
                if event.agent_id().is_none() {
                    self.last_assistant_text.push_str(text);
                }
                self.emit_event(&event, json!({ "type": "text_delta", "text": text }))?;
            },
            RuntimeEvent::ToolCallStarted { tool_name, args_json, .. } => {
                self.tool_call_count += 1;
                let args: Value = serde_json::from_str(args_json).unwrap_or(Value::Null);
                self.emit_event(
                    &event,
                    json!({
                        "type": "tool_call_started",
                        "tool": tool_name,
                        "args": args,
                    }),
                )?;
            },
            RuntimeEvent::ToolCallFinished { tool_name, summary, .. } => {
                self.emit_event(
                    &event,
                    json!({
                        "type": "tool_call_finished",
                        "tool": tool_name,
                        "summary": summary,
                    }),
                )?;
            },
            RuntimeEvent::AwaitingApproval { request, .. } => {
                self.emit_event(
                    &event,
                    json!({
                        "type": "approval_request",
                        "title": request.title,
                        "risk": format!("{:?}", request.risk_level),
                        "message": request.message,
                    }),
                )?;
            },
            RuntimeEvent::PlanUpdated { explanation, plan, .. } => {
                self.emit_event(
                    &event,
                    json!({
                        "type": "plan_updated",
                        "explanation": explanation,
                        "plan": plan,
                    }),
                )?;
            },
            RuntimeEvent::UserEvent { event: UserEvent::Structured { event_type, data }, .. } => {
                self.emit_event(
                    &event,
                    json!({
                        "type": "user_event",
                        "event_type": event_type,
                        "data": data,
                    }),
                )?;
            },
            RuntimeEvent::UserEvent { .. } => {},
            RuntimeEvent::Checkpoint { .. } => {},
            RuntimeEvent::RunFinished { .. } => {},
            RuntimeEvent::RunCancelled { .. } => {
                self.emit_event(&event, json!({ "type": "run_cancelled" }))?;
            },
        }

        Ok(())
    }

    fn finish_turn(&mut self) -> AgentResult<()> {
        let duration_ms = self.turn_start.map(|s| s.elapsed().as_millis() as u64).unwrap_or(0);

        self.emit(&json!({
            "type": "turn_finished",
            "duration_ms": duration_ms,
            "tool_call_count": self.tool_call_count,
            "assistant_text": self.last_assistant_text.trim(),
        }))?;

        self.turn_start = None;
        self.tool_call_count = 0;
        self.last_assistant_text.clear();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_base::{ApprovalRequest, PlanItem, PlanStepStatus, RiskLevel, SessionId, UserEvent};
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    struct SharedWriter {
        inner: Arc<Mutex<Vec<u8>>>,
    }

    impl Write for SharedWriter {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            self.inner.lock().unwrap().extend_from_slice(data);
            Ok(data.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl SharedWriter {
        fn new() -> (Self, Arc<Mutex<Vec<u8>>>) {
            let inner = Arc::new(Mutex::new(Vec::new()));
            (Self { inner: inner.clone() }, inner)
        }
    }

    fn session_id() -> SessionId {
        SessionId { id: 1, external_id: None }
    }

    fn render_one(event: RuntimeEvent) -> Vec<String> {
        let (writer, buf) = SharedWriter::new();
        let mut r = JsonStreamRenderer::new(Box::new(writer));
        r.render(event).unwrap();
        drop(r);
        let text = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        if text.is_empty() { vec![] } else { text.lines().map(|l| l.to_string()).collect() }
    }

    fn render_and_finish(events: &[RuntimeEvent]) -> Vec<String> {
        let (writer, buf) = SharedWriter::new();
        let mut r = JsonStreamRenderer::new(Box::new(writer));
        for e in events {
            r.render(e.clone()).unwrap();
        }
        r.finish_turn().unwrap();
        drop(r);
        let text = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        text.lines().map(|l| l.to_string()).collect()
    }

    #[test]
    fn test_text_delta_produces_valid_json() {
        let lines = render_one(RuntimeEvent::TextDelta {
            session_id: session_id(),
            text: "hello".into(),
            agent_id: None,
            trace_id: None,
        });
        assert_eq!(lines.len(), 1);
        let v: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(v["type"], "text_delta");
        assert_eq!(v["text"], "hello");
    }

    #[test]
    fn test_thought_delta_produces_valid_json() {
        let lines = render_one(RuntimeEvent::ThoughtDelta {
            session_id: session_id(),
            text: "thinking...".into(),
            agent_id: None,
            trace_id: None,
        });
        let v: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(v["type"], "thought_delta");
    }

    #[test]
    fn test_tool_call_started_parses_args() {
        let lines = render_one(RuntimeEvent::ToolCallStarted {
            session_id: session_id(),
            tool_name: "shell".into(),
            args_json: r#"{"cmd":"ls"}"#.into(),
            agent_id: None,
            trace_id: None,
        });
        let v: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(v["type"], "tool_call_started");
        assert_eq!(v["tool"], "shell");
        assert_eq!(v["args"]["cmd"], "ls");
    }

    #[test]
    fn test_tool_call_finished_produces_valid_json() {
        let lines = render_one(RuntimeEvent::ToolCallFinished {
            session_id: session_id(),
            tool_name: "shell".into(),
            summary: "done".into(),
            agent_id: None,
            trace_id: None,
            denied: false,
        });
        let v: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(v["type"], "tool_call_finished");
        assert_eq!(v["tool"], "shell");
        assert_eq!(v["summary"], "done");
    }

    #[test]
    fn test_awaiting_approval_produces_valid_json() {
        let lines = render_one(RuntimeEvent::AwaitingApproval {
            session_id: session_id(),
            request: ApprovalRequest {
                title: "Delete".into(),
                message: "Dangerous".into(),
                action_key: None,
                risk_level: RiskLevel::Destructive,
                raw: None,
            },
            agent_id: None,
            trace_id: None,
        });
        let v: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(v["type"], "approval_request");
        assert_eq!(v["title"], "Delete");
    }

    #[test]
    fn test_plan_updated_produces_valid_json() {
        let lines = render_one(RuntimeEvent::PlanUpdated {
            session_id: session_id(),
            objective: "test".into(),
            explanation: Some("step 1 done".into()),
            plan: vec![PlanItem { step: "Step 1".into(), status: PlanStepStatus::Completed }],
            agent_id: None,
            trace_id: None,
        });
        let v: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(v["type"], "plan_updated");
    }

    #[test]
    fn test_user_event_structured() {
        let lines = render_one(RuntimeEvent::UserEvent {
            session_id: session_id(),
            event: UserEvent::Structured { event_type: "custom".into(), data: serde_json::json!({"key": "value"}) },
            agent_id: None,
            trace_id: None,
        });
        let v: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(v["type"], "user_event");
        assert_eq!(v["event_type"], "custom");
        assert_eq!(v["data"]["key"], "value");
    }

    #[test]
    fn test_user_event_progress_ignored() {
        let lines = render_one(RuntimeEvent::UserEvent {
            session_id: session_id(),
            event: UserEvent::Progress { text: "loading...".into() },
            agent_id: None,
            trace_id: None,
        });
        assert!(lines.is_empty());
    }

    #[test]
    fn test_run_finished_no_output() {
        let lines = render_one(RuntimeEvent::RunFinished { session_id: session_id(), agent_id: None, trace_id: None });
        assert!(lines.is_empty());
    }

    #[test]
    fn test_run_cancelled_produces_valid_json() {
        let lines = render_one(RuntimeEvent::RunCancelled { session_id: session_id(), agent_id: None, trace_id: None });
        let v: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(v["type"], "run_cancelled");
    }

    #[test]
    fn test_finish_turn_emits_summary() {
        let lines = render_and_finish(&[RuntimeEvent::TextDelta {
            session_id: session_id(),
            text: "hello".into(),
            agent_id: None,
            trace_id: None,
        }]);
        let last: serde_json::Value = serde_json::from_str(lines.last().unwrap()).unwrap();
        assert_eq!(last["type"], "turn_finished");
        assert!(last["duration_ms"].as_u64().is_some());
        assert_eq!(last["tool_call_count"], 0);
    }

    #[test]
    fn test_tool_call_count_incremented() {
        let lines = render_and_finish(&[
            RuntimeEvent::ToolCallStarted {
                session_id: session_id(),
                tool_name: "a".into(),
                args_json: "{}".into(),
                agent_id: None,
                trace_id: None,
            },
            RuntimeEvent::ToolCallStarted {
                session_id: session_id(),
                tool_name: "b".into(),
                args_json: "{}".into(),
                agent_id: None,
                trace_id: None,
            },
        ]);
        let last: serde_json::Value = serde_json::from_str(lines.last().unwrap()).unwrap();
        assert_eq!(last["tool_call_count"], 2);
    }

    #[test]
    fn test_assistant_text_accumulated() {
        let lines = render_and_finish(&[
            RuntimeEvent::TextDelta { session_id: session_id(), text: "Hello ".into(), agent_id: None, trace_id: None },
            RuntimeEvent::TextDelta { session_id: session_id(), text: "World".into(), agent_id: None, trace_id: None },
        ]);
        let last: serde_json::Value = serde_json::from_str(lines.last().unwrap()).unwrap();
        assert_eq!(last["assistant_text"], "Hello World");
    }

    // ── Sub-agent (agent_id) tests ──

    #[test]
    fn test_subagent_text_delta_includes_agent_id() {
        let lines = render_one(RuntimeEvent::TextDelta {
            session_id: session_id(),
            text: "found 3 items".into(),
            agent_id: Some("root/searcher".into()),
            trace_id: None,
        });
        assert_eq!(lines.len(), 1);
        let v: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(v["type"], "text_delta");
        assert_eq!(v["text"], "found 3 items");
        assert_eq!(v["agent_id"], "root/searcher");
    }

    #[test]
    fn test_subagent_tool_call_includes_agent_id() {
        let lines = render_one(RuntimeEvent::ToolCallStarted {
            session_id: session_id(),
            tool_name: "shell".into(),
            args_json: r#"{"cmd":"ls"}"#.into(),
            agent_id: Some("root/worker".into()),
            trace_id: None,
        });
        let v: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(v["type"], "tool_call_started");
        assert_eq!(v["tool"], "shell");
        assert_eq!(v["agent_id"], "root/worker");
    }

    #[test]
    fn test_root_event_omits_agent_id() {
        let lines = render_one(RuntimeEvent::TextDelta {
            session_id: session_id(),
            text: "hello".into(),
            agent_id: None,
            trace_id: None,
        });
        let v: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert!(v.get("agent_id").is_none());
    }

    #[test]
    fn test_subagent_text_does_not_pollute_assistant_text() {
        let lines = render_and_finish(&[
            RuntimeEvent::TextDelta { session_id: session_id(), text: "root ".into(), agent_id: None, trace_id: None },
            RuntimeEvent::TextDelta {
                session_id: session_id(),
                text: "child".into(),
                agent_id: Some("root/child".into()),
                trace_id: None,
            },
        ]);
        let last: serde_json::Value = serde_json::from_str(lines.last().unwrap()).unwrap();
        assert_eq!(last["assistant_text"], "root");
    }
}
