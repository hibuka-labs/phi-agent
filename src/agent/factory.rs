use std::sync::Arc;

use agent_base::{
    AgentBuilder, AgentResult, AgentRuntime, ReasoningEffort, RunOutcome, RuntimeEvent, SafetyConfig, SessionId,
};
use anyhow::Result;

use crate::agent::builder::base_agent_builder;

/// phi-agent configuration (tool-agnostic).
///
/// This config covers model and safety settings only. Tools are registered
/// externally on [`AgentBuilder`] — phi-agent itself never bundles tools.
#[derive(Clone)]
pub struct PhiAgentConfig {
    /// Model name passed to the LLM provider (e.g. `"opus"`, `"gpt-4o"`).
    pub model: String,
    /// Enable extended thinking / chain-of-thought.
    pub enable_thinking: bool,
    /// Token budget for thinking (provider-dependent). `None` means use the
    /// provider default.
    pub thinking_budget: Option<u64>,
    /// Reasoning intensity: Low / Medium / High / XHigh.
    pub thinking_effort: ReasoningEffort,
    /// Per-turn safety limits (max tool calls, max consecutive failures, etc.).
    pub safety: SafetyConfig,
    /// React-loop iteration cap for a single run (one user input).
    /// `None` means use the builder default (200 in [`base_agent_builder`]).
    pub max_turns: Option<u32>,
}

impl Default for PhiAgentConfig {
    fn default() -> Self {
        Self {
            model: String::new(),
            enable_thinking: false,
            thinking_budget: None,
            thinking_effort: ReasoningEffort::default(),
            safety: SafetyConfig::default(),
            max_turns: None,
        }
    }
}

/// A built Agent instance.
///
/// Wraps [`AgentRuntime`] with common operations behind a simpler API.
///
/// ## Example
///
/// ```ignore
/// let agent = PhiAgent::build(builder, config)?;
/// let session = agent.create_session().await;
/// agent.run_turn(session, "Hello!", |event| renderer.render(event)).await?;
/// ```
#[derive(Clone)]
pub struct PhiAgent {
    runtime: AgentRuntime,
    /// The configuration this agent was built with.
    pub config: PhiAgentConfig,
}

impl PhiAgent {
    /// Create a pre-configured AgentBuilder.
    ///
    /// Equivalent to `base_agent_builder(llm_client).system_prompt(system_prompt)`,
    /// after which you register tools, middleware, and approval handlers,
    /// then call `Self::build`.
    pub fn builder(llm_client: Arc<dyn agent_base::LlmClient>, system_prompt: String) -> AgentBuilder {
        base_agent_builder(llm_client).system_prompt(system_prompt)
    }

    /// Build from an AgentBuilder.
    pub fn build(builder: AgentBuilder, config: PhiAgentConfig) -> Result<Self> {
        let runtime = builder.build()?;
        Ok(Self { runtime, config })
    }

    /// Create an agent session.
    pub async fn create_session(&self) -> SessionId {
        self.runtime.create_session().await
    }

    /// Execute one turn.
    pub async fn run_turn<F>(&self, session_id: SessionId, query: &str, on_event: F) -> AgentResult<RunOutcome>
    where
        F: FnMut(RuntimeEvent) -> AgentResult<()> + Send,
    {
        self.runtime.run_turn(session_id, query, on_event).await
    }

    /// Cancel the currently executing turn.
    pub fn cancel(&self) {
        self.runtime.cancel();
    }

    /// Check whether the agent has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.runtime.is_cancelled()
    }

    /// Set the reasoning effort.
    pub async fn set_reasoning_effort(&self, effort: ReasoningEffort) {
        self.runtime.set_reasoning_effort(effort).await;
    }

    /// Access the underlying runtime (for advanced use like hook registration).
    pub fn runtime(&self) -> &AgentRuntime {
        &self.runtime
    }

    /// List all registered tools with their metadata, sorted by name.
    pub async fn list_tools(&self) -> Vec<agent_base::ToolMetadata> {
        let tools = self.runtime.tools_mut();
        let registry = tools.read().await;
        registry.metadatas()
    }
}
