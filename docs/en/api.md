# API Reference

phi-agent's API documentation is auto-generated from doc comments and hosted on [docs.rs](https://docs.rs/phi-agent).

## Key Modules

| Module | Description |
|--------|-------------|
| `agent` | `PhiAgent`, `PhiAgentConfig`, `base_agent_builder()` factory |
| `bridge` | Bridge protocol for external integrations (Tauri, Web) |
| `config` | LLM configuration resolution (CLI > env > .env > default) |
| `prompt` | System prompt builders (`build_system_prompt`, `build_system_prompt_cn`) |
| `render` | `EventRenderer` trait + `TerminalRenderer`, `JsonStreamRenderer`, `NullRenderer` |
| `session` | Session management (ID, directory, file locking, cleanup) |
| `cli` | `AutoApprovalHandler` (Auto / DenyAll) |
| `event_log` | Turn event → JSONL persistence |

## Re-exports from agent-base

phi-agent re-exports key types from [`agent-base`](https://docs.rs/agent-base):

| Category | Types |
|----------|-------|
| **Core** | `AgentResult`, `AgentError`, `Tool`, `ToolContext`, `Content`, `ToolMetadata` |
| **Tool Policy** | `ToolPolicy`, `ToolDecision` (Proceed / Block / Modify), `DenyAllToolPolicy` |
| **Tool Visibility** | `ToolExposure` (Direct / Deferred / Hidden), `ActivationContext` |
| **LLM** | `ReasoningConfig`, `ReasoningEffort`, `LlmProvider` (via `llm-trait`) |
| **Config** | `AgentConfig`, `SafetyConfig`, `SessionConfig`, `Language` |
| **Events** | `RuntimeEvent`, `UserEvent`, `FinishReason` |
| **Session** | `SessionId`, `TurnContext` |
| **Plan** | `PlanItem`, `PlanStepStatus`, `UpdatePlanTool` |
| **Pipeline** | `DefaultPipeline`, `ToolExecutionPipeline` |

## Re-exports from agent-works

phi-agent re-exports key types from [`agent-works`](https://docs.rs/agent-works):

| Category | Types |
|----------|-------|
| **Focus** | `Focus`, `FocusContext`, `FocusInput`, `FocusOutput`, `FocusError` |
| **Prompt Fragments** | `PromptFragment`, `FragmentContext`, `compose_fragments`, `DynamicToolsFragment`, `EnvironmentFragment` |
| **Compression** | `CompressionMiddleware`, `clear_compression_cache`, `run_compact_session` (behind `compression` feature) |
| **MCP** | `McpServer`, MCP connection types (behind `mcp` feature) |
| **Multi-Agent** | `MultiAgentConfig`, `ChildPermissionMode` (behind `multi-agent` feature) |

→ See the [Focus guide](guide/concepts/focus.md) for Focus usage examples.
→ See [Advanced Usage](guide/advanced/advanced.md) for ToolPolicy, compression, and PromptFragment examples.

---

→ [Full API docs on docs.rs](https://docs.rs/phi-agent)
