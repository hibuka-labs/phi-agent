use std::sync::Arc;

use agent_base::{AgentResult, AgentRuntime, ReasoningEffort, RunOutcome, RuntimeEvent, SafetyConfig, SessionId};

use agent_works::AgentBuilder;

use crate::agent::builder::base_agent_builder;

/// phi-agent configuration (tool-agnostic).
///
/// This config covers model and safety settings only. Tools are registered
/// externally on [`agent_works::AgentBuilder`] — phi-agent itself never bundles tools
/// beyond kernel tools (multi-agent, skills) which are opt-in via feature flags.
#[derive(Clone, Default)]
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
    /// MCP hub for runtime server management. Only available with the `mcp` feature.
    #[cfg(feature = "mcp")]
    mcp_hub: Arc<tokio::sync::Mutex<Option<Arc<agent_works::mcp::EnhancedMcpHub>>>>,
}

impl PhiAgent {
    /// Create a pre-configured AgentBuilder.
    ///
    /// Equivalent to `base_agent_builder(llm_client).system_prompt(system_prompt)`,
    /// after which you register tools, middleware, and approval handlers,
    /// then call [`Self::build`].
    ///
    /// # Example
    ///
    /// ```ignore
    /// use phi_agent::PhiAgent;
    /// use phi_agent::build_system_prompt;
    /// use std::sync::Arc;
    ///
    /// let llm_client = Arc::new(phi_agent::OpenAiClient::new(
    ///     "sk-...".into(),
    ///     "gpt-4o".into(),
    ///     Some("https://api.openai.com/v1".into()),
    /// ));
    ///
    /// let builder = PhiAgent::builder(llm_client, build_system_prompt());
    /// ```
    pub fn builder(llm_client: Arc<dyn agent_base::StreamClient>, system_prompt: String) -> AgentBuilder {
        base_agent_builder(llm_client).system_prompt(system_prompt)
    }

    /// Build from an AgentBuilder.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use phi_agent::{PhiAgent, PhiAgentConfig, base_agent_builder, build_system_prompt};
    /// use std::sync::Arc;
    ///
    /// let llm_client = Arc::new(phi_agent::OpenAiClient::new(
    ///     "sk-...".into(),
    ///     "gpt-4o".into(),
    ///     Some("https://api.openai.com/v1".into()),
    /// ));
    /// let builder = base_agent_builder(llm_client).system_prompt(build_system_prompt());
    ///
    /// let config = PhiAgentConfig {
    ///     model: "gpt-4o".into(),
    ///     enable_thinking: true,
    ///     ..Default::default()
    /// };
    /// let agent = PhiAgent::build(builder, config)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn build(builder: AgentBuilder, config: PhiAgentConfig) -> AgentResult<Self> {
        let runtime = builder.build()?;
        Ok(Self {
            runtime,
            config,
            #[cfg(feature = "mcp")]
            mcp_hub: Arc::new(tokio::sync::Mutex::new(None)),
        })
    }

    /// Create an agent session.
    ///
    /// # Example
    ///
    /// ```ignore
    /// # use phi_agent::{PhiAgent, PhiAgentConfig, base_agent_builder, build_system_prompt};
    /// # use std::sync::Arc;
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let llm_client = Arc::new(phi_agent::OpenAiClient::new("sk-...".into(), "gpt-4o".into(), None));
    /// # let builder = base_agent_builder(llm_client).system_prompt(build_system_prompt());
    /// # let agent = PhiAgent::build(builder, PhiAgentConfig::default())?;
    /// let session = agent.create_session().await;
    /// println!("Session id: {}", session.id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_session(&self) -> SessionId {
        self.runtime.create_session().await
    }

    /// Execute one turn.
    ///
    /// # Example
    ///
    /// ```ignore
    /// # use phi_agent::{PhiAgent, PhiAgentConfig, base_agent_builder, build_system_prompt};
    /// # use phi_agent::create_stdout_renderer;
    /// # use std::sync::Arc;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let llm_client = Arc::new(phi_agent::OpenAiClient::new("sk-...".into(), "gpt-4o".into(), None));
    /// # let builder = base_agent_builder(llm_client).system_prompt(build_system_prompt());
    /// # let agent = PhiAgent::build(builder, PhiAgentConfig::default())?;
    /// let session = agent.create_session().await;
    /// let renderer = create_stdout_renderer();
    ///
    /// let outcome = agent
    ///     .run_turn(session, "What is 2+2?", |event| renderer.render(event))
    ///     .await?;
    /// println!("Turn completed: {:?}", outcome);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn run_turn<F>(&self, session_id: SessionId, query: &str, on_event: F) -> AgentResult<RunOutcome>
    where
        F: FnMut(RuntimeEvent) -> AgentResult<()> + Send,
    {
        self.runtime.run_turn(session_id, query, on_event).await
    }

    /// Cancel the currently executing turn.
    ///
    /// # Example
    ///
    /// ```ignore
    /// # use phi_agent::{PhiAgent, PhiAgentConfig, base_agent_builder, build_system_prompt};
    /// # use std::sync::Arc;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let llm_client = Arc::new(phi_agent::OpenAiClient::new("sk-...".into(), "gpt-4o".into(), None));
    /// # let builder = base_agent_builder(llm_client).system_prompt(build_system_prompt());
    /// # let agent = PhiAgent::build(builder, PhiAgentConfig::default())?;
    /// let agent_clone = agent.clone();
    /// let session = agent.create_session().await;
    ///
    /// // Run the turn in a separate task
    /// let handle = tokio::spawn(async move {
    ///     agent_clone.run_turn(session, "count to 100", |_| Ok(())).await
    /// });
    ///
    /// // Cancel after a short delay
    /// tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    /// agent.cancel();
    /// # Ok(())
    /// # }
    /// ```
    pub fn cancel(&self) {
        self.runtime.cancel();
    }

    /// Check whether the agent has been cancelled.
    ///
    /// # Example
    ///
    /// ```ignore
    /// # use phi_agent::{PhiAgent, PhiAgentConfig, base_agent_builder, build_system_prompt};
    /// # use std::sync::Arc;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let llm_client = Arc::new(phi_agent::OpenAiClient::new("sk-...".into(), "gpt-4o".into(), None));
    /// # let builder = base_agent_builder(llm_client).system_prompt(build_system_prompt());
    /// # let agent = PhiAgent::build(builder, PhiAgentConfig::default())?;
    /// assert!(!agent.is_cancelled());
    /// agent.cancel();
    /// assert!(agent.is_cancelled());
    /// # Ok(())
    /// # }
    /// ```
    pub fn is_cancelled(&self) -> bool {
        self.runtime.is_cancelled()
    }

    /// Set the reasoning effort.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use phi_agent::ReasoningEffort;
    /// # use phi_agent::{PhiAgent, PhiAgentConfig, base_agent_builder, build_system_prompt};
    /// # use std::sync::Arc;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let llm_client = Arc::new(phi_agent::OpenAiClient::new("sk-...".into(), "gpt-4o".into(), None));
    /// # let builder = base_agent_builder(llm_client).system_prompt(build_system_prompt());
    /// # let agent = PhiAgent::build(builder, PhiAgentConfig::default())?;
    /// // Switch to high reasoning effort mid-conversation
    /// agent.set_reasoning_effort(ReasoningEffort::High).await;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn set_reasoning_effort(&self, effort: ReasoningEffort) {
        self.runtime.set_reasoning_effort(effort).await;
    }

    /// Access the underlying runtime (for advanced use like hook registration).
    ///
    /// # Example
    ///
    /// ```ignore
    /// # use phi_agent::{PhiAgent, PhiAgentConfig, base_agent_builder, build_system_prompt};
    /// # use std::sync::Arc;
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let llm_client = Arc::new(phi_agent::OpenAiClient::new("sk-...".into(), "gpt-4o".into(), None));
    /// # let builder = base_agent_builder(llm_client).system_prompt(build_system_prompt());
    /// # let agent = PhiAgent::build(builder, PhiAgentConfig::default())?;
    /// let runtime = agent.runtime();
    /// let event_rx = runtime.subscribe_runtime_events();
    /// println!("Subscribed to events, receiver lag: {}", event_rx.len());
    /// # Ok(())
    /// # }
    /// ```
    pub fn runtime(&self) -> &AgentRuntime {
        &self.runtime
    }

    /// List all registered tools with their metadata, sorted by name.
    ///
    /// # Example
    ///
    /// ```ignore
    /// # use phi_agent::{PhiAgent, PhiAgentConfig, base_agent_builder, build_system_prompt};
    /// # use std::sync::Arc;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let llm_client = Arc::new(phi_agent::OpenAiClient::new("sk-...".into(), "gpt-4o".into(), None));
    /// # let builder = base_agent_builder(llm_client).system_prompt(build_system_prompt());
    /// # let agent = PhiAgent::build(builder, PhiAgentConfig::default())?;
    /// let tools = agent.list_tools().await;
    /// for tool in &tools {
    ///     println!("{} - {} ({})", tool.name, tool.description, tool.origin);
    /// }
    /// println!("{} tools registered", tools.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_tools(&self) -> Vec<agent_base::ToolMetadata> {
        let tools = self.runtime.tools_mut();
        let registry = tools.read().await;
        registry.metadatas()
    }
}

// ── MCP Runtime Management (Phase 1.2) ──

#[cfg(feature = "mcp")]
impl PhiAgent {
    /// Get or lazily initialize the MCP hub.
    async fn get_or_init_hub(&self) -> Arc<agent_works::mcp::EnhancedMcpHub> {
        let mut guard = self.mcp_hub.lock().await;
        if let Some(ref hub) = *guard {
            return hub.clone();
        }
        let hub = Arc::new(agent_works::mcp::EnhancedMcpHub::new());
        *guard = Some(hub.clone());
        hub
    }

    /// Dynamically attach an MCP server at runtime.
    ///
    /// Adds the server config, connects, discovers tools, and registers them
    /// into the agent's `ToolRegistry`. Tools are registered with the
    /// `mcp.<server_name>.<tool_name>` naming convention.
    ///
    /// Returns an error if the server cannot be connected or tools cannot be
    /// discovered. On failure, the server config is rolled back (removed from
    /// the hub) so a partial entry is never left behind.
    ///
    /// # Example
    ///
    /// ```ignore
    /// # use phi_agent::{PhiAgent, PhiAgentConfig, base_agent_builder, build_system_prompt};
    /// # use phi_agent::McpServerConfig;
    /// # use std::sync::Arc;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let llm_client = Arc::new(phi_agent::OpenAiClient::new("sk-...".into(), "gpt-4o".into(), None));
    /// # let builder = base_agent_builder(llm_client).system_prompt(build_system_prompt());
    /// # let agent = PhiAgent::build(builder, PhiAgentConfig::default())?;
    /// // Attach a filesystem MCP server at runtime
    /// agent
    ///     .attach_mcp(McpServerConfig {
    ///         name: "filesystem".into(),
    ///         transport: phi_agent::McpTransport::Stdio {
    ///             command: "npx".into(),
    ///             args: vec!["-y".into(), "@modelcontextprotocol/server-filesystem".into(), "/tmp".into()],
    ///             env: Default::default(),
    ///         },
    ///     })
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance note
    ///
    /// Currently calls `hub.register_all()` which re-registers all servers'
    /// tools (O(total-servers)). For the common case this is fine because
    /// re-registration is a no-op HashMap insert. A future optimization would
    /// register only the newly attached server's tools.
    pub async fn attach_mcp(&self, config: agent_works::mcp::McpServerConfig) -> AgentResult<()> {
        let name = config.name.clone();
        let hub = self.get_or_init_hub().await;

        // Add server config and attempt connection
        hub.add_server(config);
        if let Err(e) = hub.connect_one(&name).await {
            hub.remove_server(&name).await;
            return Err(e);
        }

        // Discover tools; rollback on failure
        let discovered = match hub.discover_all().await {
            Ok(d) => d,
            Err(e) => {
                hub.remove_server(&name).await;
                return Err(e);
            },
        };

        // Register only the newly attached server's tools
        let tools = self.runtime.tools_mut();
        let mut registry = tools.write().await;
        hub.register_server(&mut registry, &name).await;

        let count: usize = discovered.iter().filter(|(n, _)| n == &name).map(|(_, t)| t.len()).sum();
        if count == 0 {
            tracing::warn!(
                server_name = %name,
                "attached MCP server but discovered zero tools — server may be misconfigured"
            );
        }
        tracing::info!(server_name = %name, tool_count = count, "attached MCP server at runtime");
        Ok(())
    }

    /// Dynamically detach an MCP server at runtime.
    ///
    /// Unregisters all tools belonging to this server from the agent's
    /// `ToolRegistry`, disconnects the server, and removes its config
    /// from the hub.
    ///
    /// This is a no-op if the server is not attached.
    ///
    /// # Example
    ///
    /// ```ignore
    /// # use phi_agent::{PhiAgent, PhiAgentConfig, base_agent_builder, build_system_prompt};
    /// # use phi_agent::McpServerConfig;
    /// # use std::sync::Arc;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let llm_client = Arc::new(phi_agent::OpenAiClient::new("sk-...".into(), "gpt-4o".into(), None));
    /// # let builder = base_agent_builder(llm_client).system_prompt(build_system_prompt());
    /// # let agent = PhiAgent::build(builder, PhiAgentConfig::default())?;
    /// # agent.attach_mcp(McpServerConfig {
    /// #     name: "filesystem".into(),
    /// #     transport: phi_agent::McpTransport::Stdio {
    /// #         command: "npx".into(), args: vec!["-y".into(), "@modelcontextprotocol/server-filesystem".into(), "/tmp".into()],
    /// #         env: Default::default(),
    /// #     },
    /// # }).await?;
    /// // Detach the server — tools are unregistered, connection is closed
    /// agent.detach_mcp("filesystem").await;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Concurrency note
    ///
    /// There is a TOCTOU window between collecting tool names (read lock) and
    /// removing them (write lock). If another thread re-attaches a server with
    /// the same name during this window, its tools may be prematurely removed.
    /// In practice this race is harmless: the new attach will re-register tools
    /// on the next turn, and tool calls in flight will fail with a clear error
    /// since `hub.remove_server` disconnects clients.
    pub async fn detach_mcp(&self, name: &str) {
        let hub = {
            let guard = self.mcp_hub.lock().await;
            match *guard {
                Some(ref hub) => hub.clone(),
                None => return,
            }
        };

        // Collect tool names matching the mcp.<server>.<tool> prefix.
        // NOTE: the "mcp.<server>.<tool>" naming convention is defined by
        // agent_works::mcp::McpToolAdapter. If that convention changes, this
        // prefix must be updated.
        let mcp_prefix = format!("mcp.{}.", name);
        let tool_names: Vec<String> = {
            let tools = self.runtime.tools_mut();
            let registry = tools.read().await;
            registry.metadatas().iter().filter(|m| m.name.starts_with(&mcp_prefix)).map(|m| m.name.clone()).collect()
        };

        // Unregister tools from the runtime
        if !tool_names.is_empty() {
            let tools = self.runtime.tools_mut();
            let mut registry = tools.write().await;
            for tool_name in &tool_names {
                registry.remove(tool_name);
            }
        }

        // Remove the server from the hub (disconnects clients)
        hub.remove_server(name).await;

        tracing::info!(
            server_name = %name,
            tool_count = tool_names.len(),
            "detached MCP server at runtime"
        );
    }

    // ── MCP Server (Phase 4.1) ──

    /// Convert this agent into an MCP server that external orchestrators can call.
    ///
    /// # Example
    ///
    /// ```ignore
    /// # use phi_agent::{PhiAgent, PhiAgentConfig, base_agent_builder, build_system_prompt};
    /// # use phi_agent::{McpServeConfig, McpServerTransport};
    /// # use std::sync::Arc;
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let llm_client = Arc::new(phi_agent::OpenAiClient::new("sk-...".into(), "gpt-4o".into(), None));
    /// # let builder = base_agent_builder(llm_client).system_prompt(build_system_prompt());
    /// # let agent = PhiAgent::build(builder, PhiAgentConfig::default())?;
    /// // Expose the agent as an MCP server via stdio
    /// let mcp_server = agent.into_mcp_server(McpServeConfig {
    ///     transport: McpServerTransport::Stdio,
    ///     name: "phi-agent".into(),
    ///     version: "1.0.0".into(),
    /// });
    /// // External orchestrators (LangGraph, CrewAI, etc.) can now call
    /// // the agent's tools through the MCP protocol
    /// # Ok(())
    /// # }
    /// ```
    pub fn into_mcp_server(&self, config: agent_works::mcp::McpServeConfig) -> agent_works::mcp::McpServer {
        agent_works::mcp::McpServer::new(self.runtime.clone(), config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt::build_system_prompt;
    use async_trait::async_trait;
    use futures_core::Stream;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    struct StubClient;

    /// Yields one `Text` chunk, one `Stop` chunk, then ends — a minimal valid
    /// LLM response that lets the react loop complete a turn.
    struct StopStream {
        state: u8,
    }

    impl Stream for StopStream {
        type Item = agent_base::AgentResult<agent_base::StreamChunk>;

        fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            match self.state {
                0 => {
                    self.state = 1;
                    Poll::Ready(Some(Ok(agent_base::StreamChunk::Text("hello".to_string()))))
                },
                1 => {
                    self.state = 2;
                    Poll::Ready(Some(Ok(agent_base::StreamChunk::Stop { finish_reason: Some("stop".to_string()) })))
                },
                _ => Poll::Ready(None),
            }
        }
    }

    #[async_trait]
    impl agent_base::StreamClient for StubClient {
        async fn stream(
            &self,
            _messages: &[agent_base::ChatMessage],
            _tools: &[serde_json::Value],
            _reasoning: Option<&agent_base::ReasoningConfig>,
            _response_format: Option<&agent_base::ResponseFormat>,
        ) -> agent_base::AgentResult<Pin<Box<dyn Stream<Item = agent_base::AgentResult<agent_base::StreamChunk>> + Send>>>
        {
            Ok(Box::pin(StopStream { state: 0 }))
        }

        fn capabilities(&self) -> agent_base::LlmCapabilities {
            agent_base::LlmCapabilities::default()
        }
    }

    fn client() -> Arc<dyn agent_base::StreamClient> {
        Arc::new(StubClient)
    }

    fn build_agent() -> PhiAgent {
        let builder = PhiAgent::builder(client(), build_system_prompt());
        PhiAgent::build(builder, PhiAgentConfig::default()).unwrap()
    }

    #[test]
    fn test_phi_agent_config_default() {
        let cfg = PhiAgentConfig::default();
        assert!(cfg.model.is_empty());
        assert!(!cfg.enable_thinking);
        assert!(cfg.thinking_budget.is_none());
        assert!(cfg.max_turns.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_builder_and_build() {
        let builder = PhiAgent::builder(client(), "custom prompt".to_string());
        let agent = PhiAgent::build(builder, PhiAgentConfig::default()).unwrap();
        let _ = agent.runtime();
        assert!(agent.config.model.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_delegate_methods() {
        let agent = build_agent();

        let session = agent.create_session().await;
        agent.set_reasoning_effort(agent_base::ReasoningEffort::High).await;

        let tools = agent.list_tools().await;
        let _ = tools;

        // A turn with the stub client's Text+Stop stream should complete.
        let outcome = agent.run_turn(session, "hi", |_| Ok(())).await;
        assert!(outcome.is_ok());

        assert!(!agent.is_cancelled());
        agent.cancel();
        assert!(agent.is_cancelled());
    }

    #[cfg(feature = "mcp")]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_into_mcp_server() {
        let agent = build_agent();
        let server = agent.into_mcp_server(agent_works::mcp::McpServeConfig::default());
        let _ = server;
    }

    #[cfg(feature = "mcp")]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_attach_mcp_connection_failure_rolls_back() {
        let agent = build_agent();
        // A Stdio transport with a nonexistent command fails at `McpClient::new`
        // (process spawn), which surfaces as an attach error and rolls back.
        let config = agent_works::mcp::McpServerConfig {
            name: "bogus".to_string(),
            transport: agent_works::mcp::McpTransport::Stdio {
                command: "definitely-not-a-real-command-xyz".to_string(),
                args: vec![],
            },
            auto_reconnect: false,
        };
        // Spawn failure → attach fails and rolls the server back.
        assert!(agent.attach_mcp(config).await.is_err());
    }

    #[cfg(feature = "mcp")]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_detach_mcp_noop_when_not_attached() {
        let agent = build_agent();
        // No hub initialized → detach is a no-op.
        agent.detach_mcp("never-attached").await;
    }
}
