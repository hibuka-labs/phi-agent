//! General-purpose AgentBuilder factory — provides default configuration
//! shared across consumers.
//!
//! Returns a pre-configured [`agent_works::AgentBuilder`]; callers then register
//! tools and approval handlers on top.

#[cfg(feature = "file")]
use std::path::PathBuf;
use std::sync::Arc;

use agent_base::{ConsecutiveFailureRecovery, Language, ReasoningConfig, ReasoningEffort};

use crate::agent::compression::SummarizingMiddleware;

/// Returns an [`agent_works::AgentBuilder`] with sensible defaults:
/// - English
/// - Medium reasoning effort
/// - Thinking enabled
/// - Consecutive failure recovery (default 3 retries)
/// - Session limits (50 sessions / 100 turns per session / 50k per-message cap)
/// - Per-run react-loop cap (200 iterations for one user input)
/// - LLM-based context compression for long tool-heavy conversations
/// - File tools (read_file / write_file / edit_file / list_files) — enabled by default
/// - Skills injected into system prompt (not as tools — LLM uses read_file;
///   enabled by default via `file` feature)
/// - MCP protocol support — enabled by default
/// - Telemetry + logging — enabled by default
///
/// Callers are responsible for: registering additional tools, setting the approval
/// handler, setting the system prompt, then calling `.build()`.
///
/// Feature groups (opt-in):
/// - `shell`: shell execution (`--features shell`)
/// - `multi-agent`: multi-agent support (`--features multi-agent`)
/// - `browser`: browser automation via CDP (`--features browser`)
/// - `protocol` meta: MCP
/// - `observability` meta: telemetry + logging
/// - `app` meta: browser (NOT included in `full`)
/// - `full`: file + shell + mcp + telemetry + logging (excludes browser and multi-agent)
#[allow(unused_mut)]
pub fn base_agent_builder(llm_client: Arc<dyn agent_base::StreamClient>) -> agent_works::AgentBuilder {
    // Tool-output cap (default 4000 chars). Tune via PHI_MAX_TOOL_OUTPUT_CHARS for large
    // outputs (HTML, base64 images, long lists). Truncated results carry an explicit
    // "...(truncated)" suffix plus structured TruncationInfo from agent-base.
    let max_tool_output_chars = match std::env::var("PHI_MAX_TOOL_OUTPUT_CHARS") {
        Ok(value) => match value.trim().parse::<usize>() {
            Ok(n) => n,
            Err(_) => {
                tracing::warn!(
                    value = %value,
                    "PHI_MAX_TOOL_OUTPUT_CHARS is not a valid integer; falling back to default 4000"
                );
                4000
            },
        },
        Err(_) => 4000,
    };

    #[cfg(feature = "file")]
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let mut builder = agent_works::AgentBuilder::new(llm_client.clone())
        .language(Language::En)
        .reasoning(ReasoningConfig { effort: Some(ReasoningEffort::Medium), ..Default::default() })
        .enable_thought(true)
        .enable_thinking(true)
        .max_sessions(50)
        .max_turns_per_session(100)
        .execution_max_turns(200)
        .max_message_tokens(50_000)
        .max_tool_output_chars(max_tool_output_chars)
        .error_recovery(Arc::new(ConsecutiveFailureRecovery::new(3)))
        // Summarise the earlier part of long conversations so per-call LLM context
        // stays bounded (see compression.rs). Override via the returned builder, or
        // build your own AgentBuilder to opt out.
        .middleware(SummarizingMiddleware::new(llm_client));

    // ── File tools (opt-in via `file` feature) ──
    #[cfg(feature = "file")]
    {
        use phi_kernel_tools::file::{EditFileTool, ListFilesTool, ReadFileTool, WriteFileTool};
        builder = builder
            .register_tool_arc(Arc::new(ReadFileTool::new(cwd.clone())))
            .register_tool_arc(Arc::new(WriteFileTool::new(cwd.clone())))
            .register_tool_arc(Arc::new(EditFileTool::new(cwd.clone())))
            .register_tool_arc(Arc::new(ListFilesTool::new(cwd.clone())));
    }

    // ── Multi-agent (opt-in) ──
    #[cfg(feature = "multi-agent")]
    {
        use agent_works::multi_agent::MultiAgentConfig;
        builder =
            builder.with_multi_agent(MultiAgentConfig::default()).with_multi_agent_tool_factory(Arc::new(|runtime| {
                phi_kernel_tools::multi_agent::create_all_tools(runtime)
            }));
    }

    // ── Skills: prompt-injection mode (uses read_file, no skill-specific tools) ──
    #[cfg(feature = "file")]
    {
        use agent_works::skill::Skill;
        use agent_works::skill::prompt_skill::PromptSkill;
        let skill_dirs: Vec<PathBuf> = vec![
            // User-level skills (low priority)
            dirs_next().join(".phi").join("skills"),
            // Project-level skills (high priority, loaded last to override)
            PathBuf::from(".phi/skills"),
        ];

        for dir in &skill_dirs {
            if dir.is_dir() {
                match PromptSkill::scan_dir(dir) {
                    Ok(skills) => {
                        for skill in skills {
                            tracing::debug!(
                                name = skill.name(),
                                dir = %dir.display(),
                                "auto-loaded skill (prompt-injection mode)"
                            );
                            builder = builder.register_skill(skill);
                        }
                    },
                    Err(e) => {
                        tracing::warn!(dir = %dir.display(), error = %e, "failed to scan skills directory");
                    },
                }
            }
        }
    }

    builder
}

/// Resolve the user's home directory for `~/.phi/skills/`.
#[cfg(feature = "file")]
fn dirs_next() -> std::path::PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures_core::Stream;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    struct StubClient;
    struct EmptyStream;

    impl Stream for EmptyStream {
        type Item = agent_base::AgentResult<agent_base::StreamChunk>;
        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Ready(None)
        }
    }

    #[async_trait]
    impl agent_base::LlmClient for StubClient {
        async fn chat(
            &self,
            _messages: &[agent_base::ChatMessage],
            _tools: &[serde_json::Value],
            _reasoning: Option<&agent_base::ReasoningConfig>,
            _response_format: Option<&agent_base::ResponseFormat>,
        ) -> agent_base::AgentResult<serde_json::Value> {
            Ok(serde_json::json!({"choices":[{"message":{"content":"stub"}}]}))
        }
        async fn chat_stream(
            &self,
            _messages: &[agent_base::ChatMessage],
            _tools: &[serde_json::Value],
            _reasoning: Option<&agent_base::ReasoningConfig>,
            _response_format: Option<&agent_base::ResponseFormat>,
        ) -> agent_base::AgentResult<Pin<Box<dyn Stream<Item = agent_base::AgentResult<agent_base::StreamChunk>> + Send>>>
        {
            Ok(Box::pin(EmptyStream))
        }
        fn capabilities(&self) -> agent_base::LlmCapabilities {
            agent_base::LlmCapabilities {
                supports_streaming: true,
                supports_tools: true,
                supports_vision: false,
                supports_thinking: true,
                max_context_tokens: Some(128_000),
                max_output_tokens: Some(16_384),
            }
        }
    }

    #[test]
    fn test_max_tool_output_chars_default() {
        unsafe { std::env::remove_var("PHI_MAX_TOOL_OUTPUT_CHARS") };
        let builder = base_agent_builder(agent_base::llm::adapt(Arc::new(StubClient)));
        let _ = builder;
    }

    #[test]
    fn test_max_tool_output_chars_custom() {
        unsafe { std::env::set_var("PHI_MAX_TOOL_OUTPUT_CHARS", "8000") };
        let builder = base_agent_builder(agent_base::llm::adapt(Arc::new(StubClient)));
        let _ = builder;
        unsafe { std::env::remove_var("PHI_MAX_TOOL_OUTPUT_CHARS") };
    }

    #[test]
    fn test_max_tool_output_chars_invalid_fallback() {
        unsafe { std::env::set_var("PHI_MAX_TOOL_OUTPUT_CHARS", "not-a-number") };
        let builder = base_agent_builder(agent_base::llm::adapt(Arc::new(StubClient)));
        let _ = builder;
        unsafe { std::env::remove_var("PHI_MAX_TOOL_OUTPUT_CHARS") };
    }

    /// Verify that the default `base_agent_builder()` registers the 6 multi-agent tools.
    #[cfg(feature = "multi-agent")]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_base_agent_builder_registers_multi_agent_tools() {
        let builder = base_agent_builder(agent_base::llm::adapt(Arc::new(StubClient))).system_prompt("test");

        let runtime = builder.build().unwrap();

        let tools = tokio::task::block_in_place(|| {
            let tools = runtime.tools_mut();
            let guard = tools.blocking_read();
            guard.metadatas().into_iter().map(|m| m.name).collect::<Vec<String>>()
        });

        assert!(tools.contains(&"spawn_agent".to_string()), "expected spawn_agent tool");
        assert!(tools.contains(&"send_message".to_string()), "expected send_message tool");
        assert!(tools.contains(&"followup_task".to_string()), "expected followup_task tool");
        assert!(tools.contains(&"wait_agent".to_string()), "expected wait_agent tool");
        assert!(tools.contains(&"list_agents".to_string()), "expected list_agents tool");
        assert!(tools.contains(&"close_agent".to_string()), "expected close_agent tool");
    }

    /// Verify that `.without_multi_agent()` on the returned builder removes the tools.
    #[cfg(feature = "multi-agent")]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_base_agent_builder_without_multi_agent() {
        let builder = base_agent_builder(agent_base::llm::adapt(Arc::new(StubClient)))
            .system_prompt("test")
            .without_multi_agent();

        let runtime = builder.build().unwrap();

        let tools = tokio::task::block_in_place(|| {
            let tools = runtime.tools_mut();
            let guard = tools.blocking_read();
            guard.metadatas().into_iter().map(|m| m.name).collect::<Vec<String>>()
        });

        assert!(!tools.contains(&"spawn_agent".to_string()), "spawn_agent should not be registered");
        assert!(!tools.contains(&"list_agents".to_string()), "list_agents should not be registered");
    }
}
