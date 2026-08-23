//! HTML EventRenderer — demonstrates the renderer extension point.
//!
//! Streams agent events as a simple HTML page. This example is meant to show
//! how a consumer can plug a custom `EventRenderer` into phi-agent without
//! changing the framework.
//!
//! Run with:
//! ```bash
//! LLM_API_KEY=your-key cargo run --example html-renderer > output.html
//! ```

#[path = "../common/mod.rs"]
mod common;

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use phi_agent::{
    AgentError, AgentResult, EventRenderer, PhiAgent, PhiAgentConfig, PlanStepStatus, ReasoningEffort, RuntimeEvent,
    SafetyConfig, base_agent_builder, build_system_prompt,
};

fn escape_html(input: &str) -> String {
    input.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

/// Renderer that writes a stream of events as HTML fragments.
struct HtmlRenderer {
    writer: Box<dyn Write + Send>,
}

impl HtmlRenderer {
    fn new(writer: Box<dyn Write + Send>) -> Self {
        Self { writer }
    }

    fn write_html(&mut self, html: &str) -> AgentResult<()> {
        write!(self.writer, "{}", html).map_err(|err| AgentError::internal(format!("write error: {err}")))?;
        self.writer.flush().map_err(|err| AgentError::internal(format!("flush error: {err}")))
    }
}

impl EventRenderer for HtmlRenderer {
    fn render(&mut self, event: RuntimeEvent) -> AgentResult<()> {
        let html = match &event {
            RuntimeEvent::ThoughtDelta { text, .. } => {
                format!("<p class=\"thought\">{}</p>\n", escape_html(text))
            },
            RuntimeEvent::TextDelta { text, .. } => {
                format!("<p>{}</p>\n", escape_html(text))
            },
            RuntimeEvent::ToolCallStarted { tool_name, args_json, .. } => format!(
                "<details><summary>Tool: {}</summary><pre>{}</pre></details>\n",
                escape_html(tool_name),
                escape_html(args_json),
            ),
            RuntimeEvent::ToolCallFinished { summary, .. } => {
                format!("<p class=\"tool-result\">{}</p>\n", escape_html(summary))
            },
            RuntimeEvent::AwaitingApproval { request, .. } => format!(
                "<p class=\"approval\"><strong>Approval needed:</strong> {} — {}</p>\n",
                escape_html(&request.title),
                escape_html(&request.message),
            ),
            RuntimeEvent::PlanUpdated { explanation, plan, .. } => {
                let mut html = String::from("<section class=\"plan\"><h2>Plan Update</h2>\n");
                if let Some(explanation) = explanation {
                    html.push_str(&format!("<p>{}</p>\n", escape_html(explanation)));
                }
                html.push_str("<ol>\n");
                for item in plan {
                    let status = match item.status {
                        PlanStepStatus::Completed => "completed",
                        PlanStepStatus::InProgress => "in-progress",
                        PlanStepStatus::Pending => "pending",
                    };
                    html.push_str(&format!("  <li data-status=\"{}\">{}</li>\n", status, escape_html(&item.step),));
                }
                html.push_str("</ol></section>\n");
                html
            },
            RuntimeEvent::RunCancelled { .. } => String::from("<p class=\"cancelled\">Run cancelled</p>\n"),
            _ => String::new(),
        };

        self.write_html(&html)
    }

    fn finish_turn(&mut self) -> AgentResult<()> {
        self.write_html("<hr/>\n")
    }

    fn finish_session(&mut self) -> AgentResult<()> {
        self.write_html("</body>\n</html>\n")
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let model = common::resolve_llm_env().model;
    let llm_client = common::client();

    let builder = base_agent_builder(llm_client).system_prompt(build_system_prompt());
    let agent = PhiAgent::build(
        builder,
        PhiAgentConfig {
            model,
            enable_thinking: true,
            thinking_budget: None,
            thinking_effort: ReasoningEffort::Medium,
            safety: SafetyConfig::default(),
            max_turns: None,
        },
    )?;

    let session = agent.create_session().await;
    let renderer = Arc::new(Mutex::new(HtmlRenderer::new(Box::new(io::stdout()))));
    let renderer_clone = renderer.clone();

    println!("<!doctype html>\n<html>\n<head><meta charset=\"utf-8\"><title>Agent Session</title></head>\n<body>\n");

    agent
        .run_turn(session, "Hello! Introduce yourself in one sentence.", move |event| {
            renderer_clone.lock().unwrap().render(event)
        })
        .await?;

    renderer.lock().unwrap().finish_session()?;

    Ok(())
}
