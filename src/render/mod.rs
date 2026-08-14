//! Event renderers — transform [`RuntimeEvent`] streams into display output.
//!
//! Three renderers are provided:
//! - [`TerminalRenderer`] — colored terminal output with duration/tool-count
//! - [`JsonStreamRenderer`] — newline-delimited JSON for programmatic consumers
//! - [`NullRenderer`] — silent no-op for automated/CI scenarios

pub mod json_stream;
pub mod null;
pub mod terminal;

pub use json_stream::JsonStreamRenderer;
pub use null::NullRenderer;
pub use terminal::TerminalRenderer;

use std::io::{self, Write};

use agent_base::{AgentResult, RuntimeEvent};

/// Event renderer: converts RuntimeEvents into a specific output format.
///
/// Each renderer is a pure consumer — it only reads events and produces
/// output, without modifying Agent state.
pub trait EventRenderer: Send {
    /// Process one runtime event.
    fn render(&mut self, event: RuntimeEvent) -> AgentResult<()>;

    /// End of current turn — renderer may flush / output summary.
    fn finish_turn(&mut self) -> AgentResult<()>;

    /// End of entire session.
    fn finish_session(&mut self) -> AgentResult<()> {
        Ok(())
    }
}

/// Output format
#[derive(Clone, Debug)]
pub enum OutputFormat {
    /// Rich terminal output (with colors and emoji)
    Terminal {
        /// Whether to display the model's thinking/reasoning content.
        show_thinking: bool,
        /// Whether to display tool call arguments.
        show_tool_args: bool,
        /// Whether to use ANSI color codes.
        color: bool,
    },
    /// One JSON object per line
    Json,
    /// No output
    Quiet,
}

impl Default for OutputFormat {
    /// Sensible terminal defaults: tool args shown, thinking hidden, colors on.
    fn default() -> Self {
        OutputFormat::Terminal { show_thinking: false, show_tool_args: true, color: true }
    }
}

/// Create the corresponding renderer for a given output format.
///
/// `writer` defaults to stdout (CLI scenario). Web consumers can pass a
/// custom writer (e.g. a WebSocket sink).
pub fn create_renderer(format: &OutputFormat, writer: Option<Box<dyn Write + Send>>) -> Box<dyn EventRenderer> {
    match format {
        OutputFormat::Terminal { show_thinking, show_tool_args, color } => {
            let w = writer.unwrap_or_else(|| Box::new(io::stdout()));
            Box::new(TerminalRenderer::new(*show_thinking, *show_tool_args, *color, w))
        },
        OutputFormat::Json => {
            let w = writer.unwrap_or_else(|| Box::new(io::stdout()));
            Box::new(JsonStreamRenderer::new(w))
        },
        OutputFormat::Quiet => Box::new(NullRenderer),
    }
}

/// Create a renderer using stdout (backward-compatible).
pub fn create_stdout_renderer(format: &OutputFormat) -> Box<dyn EventRenderer> {
    create_renderer(format, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_base::SessionId;
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

    fn session_id() -> SessionId {
        SessionId { id: 1, external_id: None }
    }

    #[test]
    fn test_create_terminal_renderer() {
        let (writer, buf) = {
            let inner = Arc::new(Mutex::new(Vec::new()));
            (SharedWriter { inner: inner.clone() }, inner)
        };
        let format = OutputFormat::Terminal { show_thinking: true, show_tool_args: true, color: false };
        let mut r = create_renderer(&format, Some(Box::new(writer)));
        r.render(RuntimeEvent::TextDelta {
            session_id: session_id(),
            text: "hello".into(),
            agent_id: None,
            trace_id: None,
        })
        .unwrap();
        drop(r);
        let out = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        assert!(out.contains("hello"));
        assert!(!out.contains('\x1b')); // color disabled
    }

    #[test]
    fn test_create_json_renderer() {
        let (writer, buf) = {
            let inner = Arc::new(Mutex::new(Vec::new()));
            (SharedWriter { inner: inner.clone() }, inner)
        };
        let mut r = create_renderer(&OutputFormat::Json, Some(Box::new(writer)));
        r.render(RuntimeEvent::TextDelta {
            session_id: session_id(),
            text: "world".into(),
            agent_id: None,
            trace_id: None,
        })
        .unwrap();
        drop(r);
        let out = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
        assert_eq!(v["type"], "text_delta");
    }

    #[test]
    fn test_create_quiet_renderer() {
        let (writer, buf) = {
            let inner = Arc::new(Mutex::new(Vec::new()));
            (SharedWriter { inner: inner.clone() }, inner)
        };
        let mut r = create_renderer(&OutputFormat::Quiet, Some(Box::new(writer)));
        r.render(RuntimeEvent::TextDelta {
            session_id: session_id(),
            text: "quiet".into(),
            agent_id: None,
            trace_id: None,
        })
        .unwrap();
        r.finish_turn().unwrap();
        drop(r);
        let out = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn test_null_renderer_consumes_events_silently() {
        let mut r = NullRenderer;
        let sid = session_id();

        // Representative framework events should all succeed and produce no output.
        assert!(
            r.render(RuntimeEvent::TextDelta {
                session_id: sid.clone(),
                text: "hello".into(),
                agent_id: None,
                trace_id: None
            })
            .is_ok()
        );
        assert!(
            r.render(RuntimeEvent::ThoughtDelta {
                session_id: sid.clone(),
                text: "thinking".into(),
                agent_id: None,
                trace_id: None
            })
            .is_ok()
        );
        assert!(
            r.render(RuntimeEvent::ToolCallStarted {
                session_id: sid.clone(),
                tool_name: "test_tool".into(),
                args_json: "{}".into(),
                agent_id: None,
                trace_id: None
            })
            .is_ok()
        );
        assert!(
            r.render(RuntimeEvent::ToolCallFinished {
                session_id: sid.clone(),
                tool_name: "test_tool".into(),
                summary: "done".into(),
                agent_id: None,
                trace_id: None,
                denied: false,
            })
            .is_ok()
        );
        assert!(
            r.render(RuntimeEvent::UserEvent {
                session_id: sid.clone(),
                event: agent_base::UserEvent::Progress { text: "progress".into() },
                agent_id: None,
                trace_id: None
            })
            .is_ok()
        );
        assert!(
            r.render(RuntimeEvent::PlanUpdated {
                session_id: sid.clone(),
                objective: "obj".into(),
                explanation: None,
                plan: vec![],
                agent_id: None,
                trace_id: None
            })
            .is_ok()
        );
        assert!(
            r.render(RuntimeEvent::RunFinished { session_id: sid.clone(), agent_id: None, trace_id: None }).is_ok()
        );
        assert!(r.finish_turn().is_ok());
        assert!(r.finish_session().is_ok());
    }
}
