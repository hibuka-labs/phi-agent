#![allow(missing_docs)]

use std::io::{self, Write};

use agent_base::{AgentResult, PlanStepStatus, RuntimeEvent};

use crate::render::EventRenderer;

/// Rich terminal renderer — colors, emoji, formatted output.
///
/// Streams AI responses in real-time, displays tool calls with icons, and
/// shows turn summaries including duration and tool call count.
pub struct TerminalRenderer {
    show_thinking: bool,
    show_tool_args: bool,
    color: bool,
    writer: Box<dyn Write + Send>,
    tool_call_count: u32,
    turn_start: Option<std::time::Instant>,
    last_assistant_text: String,
    last_was_thought: bool,
}

impl TerminalRenderer {
    /// Create a new terminal renderer.
    ///
    /// - `show_thinking` — display the LLM's chain-of-thought
    /// - `show_tool_args` — display tool call arguments inline
    /// - `color` — enable ANSI color codes
    /// - `writer` — output destination (usually stdout, can be a WebSocket, etc.)
    pub fn new(show_thinking: bool, show_tool_args: bool, color: bool, writer: Box<dyn Write + Send>) -> Self {
        Self {
            show_thinking,
            show_tool_args,
            color,
            writer,
            tool_call_count: 0,
            turn_start: None,
            last_assistant_text: String::new(),
            last_was_thought: false,
        }
    }

    pub fn stdout(show_thinking: bool, show_tool_args: bool, color: bool) -> Self {
        Self::new(show_thinking, show_tool_args, color, Box::new(io::stdout()))
    }

    fn green(&self, s: &str) -> String {
        if self.color { format!("\x1b[32m{}\x1b[0m", s) } else { s.to_string() }
    }

    fn dim(&self, s: &str) -> String {
        if self.color { format!("\x1b[2m{}\x1b[0m", s) } else { s.to_string() }
    }

    fn bold(&self, s: &str) -> String {
        if self.color { format!("\x1b[1m{}\x1b[0m", s) } else { s.to_string() }
    }

    fn yellow(&self, s: &str) -> String {
        if self.color { format!("\x1b[33m{}\x1b[0m", s) } else { s.to_string() }
    }

    fn subtle(&self, s: &str) -> String {
        if self.color { format!("\x1b[90m{}\x1b[0m", s) } else { s.to_string() }
    }

    fn write_line(&mut self, s: &str) -> AgentResult<()> {
        writeln!(self.writer, "{}", s).map_err(|e| agent_base::AgentError::internal(format!("write error: {e}")))?;
        self.writer.flush().map_err(|e| agent_base::AgentError::internal(format!("flush error: {e}")))?;
        Ok(())
    }

    /// Write without newline — for streaming text fragments
    fn write_text(&mut self, s: &str) -> AgentResult<()> {
        write!(self.writer, "{}", s).map_err(|e| agent_base::AgentError::internal(format!("write error: {e}")))?;
        self.writer.flush().map_err(|e| agent_base::AgentError::internal(format!("flush error: {e}")))?;
        Ok(())
    }
}

impl EventRenderer for TerminalRenderer {
    fn render(&mut self, event: RuntimeEvent) -> AgentResult<()> {
        if self.turn_start.is_none() {
            self.turn_start = Some(std::time::Instant::now());
        }

        // Sub-agent events (agent_id set) get a `[path]` prefix and must not
        // pollute the parent's assistant-text buffer.
        let agent_prefix = event.agent_id().map(|id| self.subtle(&format!("[{}]", id)));
        let is_subagent = agent_prefix.is_some();

        match &event {
            RuntimeEvent::ThoughtDelta { text, .. } => {
                if self.show_thinking {
                    match &agent_prefix {
                        Some(prefix) => self.write_text(&format!("{} {}", prefix, self.dim(text)))?,
                        None => self.write_text(&self.dim(text))?,
                    }
                }
                self.last_was_thought = true;
            },
            RuntimeEvent::TextDelta { text, .. } => {
                if self.last_was_thought {
                    let _ = writeln!(self.writer);
                    self.last_was_thought = false;
                }
                if !is_subagent {
                    self.last_assistant_text.push_str(text);
                }
                match &agent_prefix {
                    Some(prefix) => self.write_text(&format!("{} {}", prefix, text))?,
                    None => self.write_text(text)?,
                }
            },
            RuntimeEvent::ToolCallStarted { tool_name, args_json, .. } => {
                self.last_was_thought = false;
                self.tool_call_count += 1;
                let head = match &agent_prefix {
                    Some(prefix) => format!("{} {}", prefix, self.bold("\u{1F527}")),
                    None => self.bold("\u{1F527}"),
                };
                if self.show_tool_args {
                    self.write_line(&format!("\n{} {} {}", head, self.green(tool_name), self.dim(args_json)))?;
                } else {
                    self.write_line(&format!("\n{} {}", head, self.green(tool_name)))?;
                }
            },
            RuntimeEvent::ToolCallFinished { tool_name: _, summary, .. } => {
                let summary_short: String = if summary.chars().count() > 500 {
                    let truncated: String = summary.chars().take(500).collect();
                    format!("{}...", truncated)
                } else {
                    summary.clone()
                };
                match &agent_prefix {
                    Some(prefix) => {
                        self.write_line(&format!("{}   {} {}", prefix, self.dim("→"), self.dim(&summary_short)))?
                    },
                    None => self.write_line(&format!("   {} {}", self.dim("→"), self.dim(&summary_short)))?,
                }
                // Add a blank line after tool completion for readability
                let _ = writeln!(self.writer);
            },
            RuntimeEvent::AwaitingApproval { request, .. } => {
                self.write_line(&format!("\n⚠️  {} [{:?}] — {}", request.title, request.risk_level, request.message,))?;
            },
            RuntimeEvent::PlanUpdated { explanation, plan, .. } => {
                self.write_line(&format!("\n\u{1F4CB} {}", self.bold("Plan Update")))?;
                self.write_line(&format!("   {}", self.dim(explanation.as_deref().unwrap_or(""))))?;
                for item in plan {
                    let icon = match item.status {
                        PlanStepStatus::Completed => "✅",
                        PlanStepStatus::InProgress => "\u{1F504}",
                        PlanStepStatus::Pending => "⏳",
                    };
                    self.write_line(&format!("   {} {}", icon, item.step))?;
                }
                let _ = writeln!(self.writer);
            },
            RuntimeEvent::RunCancelled { .. } => match &agent_prefix {
                Some(prefix) => self.write_line(&format!("{} {} Cancelled", prefix, self.yellow("⚠")))?,
                None => self.write_line(&format!("\n{} Cancelled", self.yellow("⚠")))?,
            },
            RuntimeEvent::RunFinished { .. } => {},
            RuntimeEvent::UserEvent { .. } => {},
            RuntimeEvent::Checkpoint { .. } => {},
        }

        Ok(())
    }

    fn finish_turn(&mut self) -> AgentResult<()> {
        let duration_ms = self.turn_start.map(|s| s.elapsed().as_millis() as u64).unwrap_or(0);

        let duration_str = if duration_ms >= 1000 {
            format!("{:.1}s", duration_ms as f64 / 1000.0)
        } else {
            format!("{}ms", duration_ms)
        };

        writeln!(
            self.writer,
            "\n{}",
            self.subtle(&format!("· {} elapsed · {} tool call(s)", duration_str, self.tool_call_count)),
        )
        .map_err(|e| agent_base::AgentError::internal(format!("write error: {e}")))?;

        self.tool_call_count = 0;
        self.turn_start = None;
        self.last_assistant_text.clear();
        self.last_was_thought = false;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_base::{ApprovalRequest, PlanItem, PlanStepStatus, RiskLevel, SessionId, UserEvent};
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    /// A Write impl backed by shared memory, for testing renderers.
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

    fn render_one(show_thinking: bool, show_tool_args: bool, color: bool, event: RuntimeEvent) -> String {
        let (writer, buf) = SharedWriter::new();
        let mut r = TerminalRenderer::new(show_thinking, show_tool_args, color, Box::new(writer));
        r.render(event).unwrap();
        drop(r);
        String::from_utf8(buf.lock().unwrap().clone()).unwrap()
    }

    fn render_events(show_thinking: bool, show_tool_args: bool, color: bool, events: &[RuntimeEvent]) -> String {
        let (writer, buf) = SharedWriter::new();
        let mut r = TerminalRenderer::new(show_thinking, show_tool_args, color, Box::new(writer));
        for e in events {
            r.render(e.clone()).unwrap();
        }
        r.finish_turn().unwrap();
        drop(r);
        String::from_utf8(buf.lock().unwrap().clone()).unwrap()
    }

    // ── Color tests ──

    #[test]
    fn test_color_methods_enabled() {
        let (writer, _buf) = SharedWriter::new();
        let r = TerminalRenderer::new(true, true, true, Box::new(writer));
        assert!(r.green("hello").contains("\x1b[32m"));
        assert!(r.dim("hello").contains("\x1b[2m"));
        assert!(r.bold("hello").contains("\x1b[1m"));
        assert!(r.yellow("hello").contains("\x1b[33m"));
        assert!(r.subtle("hello").contains("\x1b[90m"));
        assert!(r.green("hello").ends_with("\x1b[0m"));
    }

    #[test]
    fn test_color_methods_disabled() {
        let (writer, _buf) = SharedWriter::new();
        let r = TerminalRenderer::new(true, true, false, Box::new(writer));
        assert!(!r.green("hello").contains('\x1b'));
        assert_eq!(r.green("hello"), "hello");
        assert_eq!(r.dim("x"), "x");
        assert_eq!(r.bold("x"), "x");
        assert_eq!(r.yellow("x"), "x");
        assert_eq!(r.subtle("x"), "x");
    }

    // ── Event rendering tests ──

    #[test]
    fn test_render_text_delta() {
        let out = render_one(
            true,
            true,
            true,
            RuntimeEvent::TextDelta {
                session_id: session_id(),
                text: "hello world".into(),
                agent_id: None,
                trace_id: None,
            },
        );
        assert!(out.contains("hello world"));
    }

    #[test]
    fn test_render_thought_delta_shown() {
        let out = render_one(
            true,
            true,
            true,
            RuntimeEvent::ThoughtDelta {
                session_id: session_id(),
                text: "thinking...".into(),
                agent_id: None,
                trace_id: None,
            },
        );
        assert!(out.contains("thinking..."));
    }

    #[test]
    fn test_render_thought_delta_hidden() {
        let out = render_one(
            false,
            true,
            true,
            RuntimeEvent::ThoughtDelta {
                session_id: session_id(),
                text: "secret thought".into(),
                agent_id: None,
                trace_id: None,
            },
        );
        assert!(!out.contains("secret thought"));
    }

    #[test]
    fn test_render_tool_call_started_with_args() {
        let out = render_one(
            true,
            true,
            true,
            RuntimeEvent::ToolCallStarted {
                session_id: session_id(),
                tool_name: "read_file".into(),
                args_json: r#"{"path":"/tmp/a.txt"}"#.into(),
                agent_id: None,
                trace_id: None,
            },
        );
        assert!(out.contains("read_file"));
        assert!(out.contains("a.txt"));
    }

    #[test]
    fn test_render_tool_call_started_without_args() {
        let out = render_one(
            true,
            false,
            true,
            RuntimeEvent::ToolCallStarted {
                session_id: session_id(),
                tool_name: "read_file".into(),
                args_json: r#"{"path":"/tmp/a.txt"}"#.into(),
                agent_id: None,
                trace_id: None,
            },
        );
        assert!(out.contains("read_file"));
        assert!(!out.contains("a.txt"));
    }

    #[test]
    fn test_render_tool_call_finished_short_summary() {
        let out = render_one(
            true,
            true,
            true,
            RuntimeEvent::ToolCallFinished {
                session_id: session_id(),
                tool_name: "read_file".into(),
                summary: "file contents here".into(),
                agent_id: None,
                trace_id: None,
                denied: false,
            },
        );
        assert!(out.contains("file contents here"));
    }

    #[test]
    fn test_render_tool_call_finished_truncated() {
        let long = "x".repeat(600);
        let out = render_one(
            true,
            true,
            true,
            RuntimeEvent::ToolCallFinished {
                session_id: session_id(),
                tool_name: "read_file".into(),
                summary: long.clone(),
                agent_id: None,
                trace_id: None,
                denied: false,
            },
        );
        assert!(!out.contains(&long));
        assert!(out.contains("..."));
        assert!(out.contains(&"x".repeat(400)));
    }

    #[test]
    fn test_render_awaiting_approval() {
        let out = render_one(
            true,
            true,
            true,
            RuntimeEvent::AwaitingApproval {
                session_id: session_id(),
                request: ApprovalRequest {
                    title: "Delete file".into(),
                    message: "This will delete /tmp/important.txt".into(),
                    action_key: None,
                    risk_level: RiskLevel::Destructive,
                    raw: None,
                },
                agent_id: None,
                trace_id: None,
            },
        );
        assert!(out.contains("Delete file"));
        assert!(out.contains("Destructive"));
    }

    #[test]
    fn test_render_plan_updated() {
        let out = render_one(
            true,
            true,
            true,
            RuntimeEvent::PlanUpdated {
                session_id: session_id(),
                objective: "test plan".into(),
                explanation: Some("starting work".into()),
                plan: vec![
                    PlanItem { step: "Step 1".into(), status: PlanStepStatus::Completed },
                    PlanItem { step: "Step 2".into(), status: PlanStepStatus::InProgress },
                    PlanItem { step: "Step 3".into(), status: PlanStepStatus::Pending },
                ],
                agent_id: None,
                trace_id: None,
            },
        );
        assert!(out.contains("Plan Update"));
        assert!(out.contains("starting work"));
        assert!(out.contains("✅"));
        assert!(out.contains("Step 1"));
        assert!(out.contains("Step 2"));
        assert!(out.contains("Step 3"));
    }

    #[test]
    fn test_render_run_cancelled() {
        let out = render_one(
            true,
            true,
            true,
            RuntimeEvent::RunCancelled { session_id: session_id(), agent_id: None, trace_id: None },
        );
        assert!(out.contains("Cancelled"));
    }

    #[test]
    fn test_render_run_finished_no_output() {
        let out = render_one(
            true,
            true,
            true,
            RuntimeEvent::RunFinished { session_id: session_id(), agent_id: None, trace_id: None },
        );
        assert!(out.is_empty());
    }

    #[test]
    fn test_render_user_event_progress_no_output() {
        let out = render_one(
            true,
            true,
            true,
            RuntimeEvent::UserEvent {
                session_id: session_id(),
                event: UserEvent::Progress { text: "loading...".into() },
                agent_id: None,
                trace_id: None,
            },
        );
        assert!(out.is_empty());
    }

    // ── finish_turn tests ──

    #[test]
    fn test_finish_turn_contains_duration_and_tool_count() {
        let out = render_events(
            true,
            true,
            true,
            &[RuntimeEvent::TextDelta { session_id: session_id(), text: "hi".into(), agent_id: None, trace_id: None }],
        );
        assert!(out.contains("elapsed"));
        assert!(out.contains("tool call"));
    }

    #[test]
    fn test_finish_turn_tool_count() {
        let out = render_events(
            true,
            true,
            true,
            &[
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
                RuntimeEvent::ToolCallStarted {
                    session_id: session_id(),
                    tool_name: "c".into(),
                    args_json: "{}".into(),
                    agent_id: None,
                    trace_id: None,
                },
            ],
        );
        assert!(out.contains("3 tool call"));
    }

    #[test]
    fn test_multiple_turns_reset() {
        let (writer, buf) = SharedWriter::new();
        {
            let mut r = TerminalRenderer::new(true, true, true, Box::new(writer));
            r.render(RuntimeEvent::ToolCallStarted {
                session_id: session_id(),
                tool_name: "t1".into(),
                args_json: "{}".into(),
                agent_id: None,
                trace_id: None,
            })
            .unwrap();
            r.finish_turn().unwrap();
            r.render(RuntimeEvent::TextDelta {
                session_id: session_id(),
                text: "hello".into(),
                agent_id: None,
                trace_id: None,
            })
            .unwrap();
            r.finish_turn().unwrap();
        }
        let out = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        assert!(out.contains("1 tool call"));
        assert!(out.contains("0 tool call"));
    }

    #[test]
    fn test_thought_to_text_transition_adds_newline() {
        let (writer, buf) = SharedWriter::new();
        {
            let mut r = TerminalRenderer::new(true, true, true, Box::new(writer));
            r.render(RuntimeEvent::ThoughtDelta {
                session_id: session_id(),
                text: "hmm".into(),
                agent_id: None,
                trace_id: None,
            })
            .unwrap();
            r.render(RuntimeEvent::TextDelta {
                session_id: session_id(),
                text: "hello".into(),
                agent_id: None,
                trace_id: None,
            })
            .unwrap();
        }
        let out = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        assert!(out.contains("hmm"));
        assert!(out.contains("hello"));
    }

    // ── Sub-agent (agent_id) tests ──

    #[test]
    fn test_render_subagent_text_delta() {
        let out = render_one(
            true,
            true,
            false,
            RuntimeEvent::TextDelta {
                session_id: session_id(),
                text: "found results".into(),
                agent_id: Some("root/searcher".into()),
                trace_id: None,
            },
        );
        assert!(out.contains("[root/searcher]"));
        assert!(out.contains("found results"));
    }

    #[test]
    fn test_render_subagent_tool_call() {
        let out = render_one(
            true,
            true,
            false,
            RuntimeEvent::ToolCallStarted {
                session_id: session_id(),
                tool_name: "search".into(),
                args_json: r#"{"q":"test"}"#.into(),
                agent_id: Some("root/worker".into()),
                trace_id: None,
            },
        );
        assert!(out.contains("[root/worker]"));
        assert!(out.contains("search"));
        assert!(out.contains("test"));
    }

    #[test]
    fn test_render_subagent_thought_hidden() {
        let out = render_one(
            false,
            true,
            false,
            RuntimeEvent::ThoughtDelta {
                session_id: session_id(),
                text: "secret plan".into(),
                agent_id: Some("root/thinker".into()),
                trace_id: None,
            },
        );
        assert!(!out.contains("secret plan"));
    }

    #[test]
    fn test_render_subagent_run_cancelled() {
        let out = render_one(
            true,
            true,
            false,
            RuntimeEvent::RunCancelled {
                session_id: session_id(),
                agent_id: Some("root/worker".into()),
                trace_id: None,
            },
        );
        assert!(out.contains("[root/worker]"));
        assert!(out.contains("Cancelled"));
    }
}
