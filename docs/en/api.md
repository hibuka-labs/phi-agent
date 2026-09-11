# API Reference

phi-agent's API documentation is auto-generated from doc comments and hosted on [docs.rs](https://docs.rs/phi-agent).

## Key Modules

| Module | Description |
|--------|-------------|
| `agent` | `PhiAgent`, `PhiAgentConfig`, `base_agent_builder()` factory |
| `config` | LLM configuration resolution (CLI > env > .env > default) |
| `prompt` | System prompt builders (`build_system_prompt`, `build_system_prompt_cn`) |
| `render` | `EventRenderer` trait + `TerminalRenderer`, `JsonStreamRenderer`, `NullRenderer` |
| `session` | Session management (ID, directory, file locking, cleanup) |
| `cli` | `AutoApprovalHandler`, `QueuedApprovalHandler` |
| `event_log` | Turn event → JSONL persistence |

## Builder Factories

| Function | Description |
|----------|-------------|
| `base_agent_builder()` | Default builder with compression middleware |
| `base_agent_builder_no_compression()` | Builder without compression (for custom context rotation) |
| `base_agent_builder_with_excludes()` | Builder that skips specific kernel tools |
| `base_agent_builder_with_options()` | Builder accepting custom `CompressionConfig` |

## Re-exports from agent-base

phi-agent re-exports key types from [`agent-base`](https://docs.rs/agent-base):

| Category | Types |
|----------|-------|
| **Core** | `AgentResult`, `AgentError`, `AgentRuntime`, `Tool`, `ToolContext`, `Content`, `ToolMetadata`, `TypedTool` |
| **Approval** | `ApprovalHandler`, `ApprovalDecision`, `ApprovalRequest`, `AllowAllApprovalHandler`, `DenyAllApprovalHandler` |
| **Tool Policy** | `ToolPolicy`, `ToolDecision`, `DenyAllToolPolicy`, `ToolRegistry` |
| **Tool Visibility** | `ToolExposure`, `ActivationContext` |
| **LLM** | `ReasoningConfig`, `ReasoningEffort`, `llm_trait` (module: `LlmProvider`, `Protocol`, `config::LlmConfig`) |
| **Config** | `AgentConfig`, `SafetyConfig`, `SessionConfig`, `Language` |
| **Events** | `RuntimeEvent`, `UserEvent`, `FinishReason` |
| **Session** | `SessionId`, `TurnContext`, `ChatMessage` |
| **Plan** | `PlanItem`, `PlanStepStatus`, `UpdatePlanTool` |
| **Checkpoint** | `CheckpointData`, `CheckpointStep` |
| **Recovery** | `ConsecutiveFailureRecovery`, `RetryOnError` |
| **Context Window** | `ContextCompaction`, `ContextWindowManager`, `estimate_messages_tokens`, `first_system_prompt` |
| **Engine** | `MaxTurnsNudgeConfig`, `MaxTurnsNudgeMiddleware` |
| **Guard** | `GuardCtx`, `GuardDecision`, `Middleware`, `NoopGuard` |
| **Middleware** | `TurnFactMiddleware`, `TurnToolLimitMiddleware`, `PreLlmCtx`, `PostLlmCtx` |
| **Pipeline** | `DefaultPipeline`, `ToolExecutionPipeline` |

## Re-exports from agent-works

phi-agent re-exports key types from [`agent-works`](https://docs.rs/agent-works):

| Category | Types |
|----------|-------|
| **Builder** | `AgentBuilder` |
| **Focus** | `Focus`, `FocusContext`, `FocusInput`, `FocusOutput`, `FocusError` |
| **Prompt Fragments** | `PromptFragment`, `FragmentContext`, `compose_fragments`, `DynamicToolsFragment`, `EnvironmentFragment` |
| **Guard** | `DefaultGuard`, `DefaultGuardConfig`, `ReasoningOnlyAction` |
| **Token Budget** | `TokenBudgetAction`, `TokenBudgetConfig`, `TokenBudgetCore`, `TokenBudgetState`, `DEFAULT_SEED_MESSAGE`, `build_context_window_info`, `token_budget_base_overhead` |
| **Compression** | `CompressionMiddleware`, `clear_compression_cache`, `run_compact_session` (behind `compression`) |
| **MCP** | `McpServer`, `McpServerConfig`, `McpServeConfig` (behind `mcp`) |
| **Multi-Agent** | `MultiAgentConfig`, `ChildPermissionMode`, `ChildReport`, `ChildResultEvent`, `ControlConfig` |
| **Fan-in** | `ChildResultRoute`, `ChildResultRouter` (behind `multi-agent`) |
| **Registry** | `AgentSnapshot`, `RegistrySnapshot` (behind `multi-agent`) |
| **Skills** | `Skill`, `PromptSkill` (behind `skill`) |

## Re-exports from llm-unified

| Function | Description |
|----------|-------------|
| `create_provider()` | LLM provider factory — resolves protocol/client from an `llm_trait` config |

## Feature-gated Re-exports

| Feature | Types |
|---------|-------|
| `shell` | `LocalShellTool` (from `phi-kernel-tools`) |
| `skill` | `Skill`, `PromptSkill` (from `agent-works`) |
| `mcp` | `McpServer`, MCP types (from `agent-works`) |
| `multi-agent` | `ChildResultRoute`, `ChildResultRouter`, `AgentSnapshot`, `RegistrySnapshot` |
| `compression` | `CompressionMiddleware`, `clear_compression_cache`, `run_compact_session` |
| `telemetry` | Metrics types (from `phi-telemetry`) |
| `browser` | 21 CDP browser automation tools (from `phi-tools`) |

## CLI Types

| Type | Description |
|------|-------------|
| `AutoApprovalHandler` | Auto / DenyAll approval mode |
| `QueuedApprovalHandler` | Enqueue approvals to a channel for UI consumption |
| `ApprovalItem` | Single queued approval request with oneshot response |
| `ApprovalMode` | CLI approval mode enum |

---

→ See the [Focus guide](guide/concepts/focus.md) for Focus usage examples.
→ See [Advanced Usage](guide/advanced/advanced.md) for ToolPolicy, compression, and PromptFragment examples.
→ See [Multi-Agent](guide/advanced/multi-agent.md) for fan-in routing and child permissions.

---

→ [Full API docs on docs.rs](https://docs.rs/phi-agent)
