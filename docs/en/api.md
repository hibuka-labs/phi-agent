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
| `cli` | `AutoApprovalHandler` (Auto / DenyAll) |
| `event_log` | Turn event → JSONL persistence |

## Re-exports from agent-base

phi-agent re-exports key types from [`agent-base`](https://docs.rs/agent-base):

`AgentResult`, `Tool`, `ToolContext`, `Content`, `OpenAiClient`, `ReasoningEffort`, `SafetyConfig`, `OutputFormat`, and more.

## Re-exports from agent-works

phi-agent re-exports key types from [`agent-works`](https://docs.rs/agent-works):

`Focus`, `FocusContext`, `FocusInput`, `FocusOutput`, `FocusError`.

→ See the [Focus guide](guide/concepts/focus.md) for usage examples.

---

→ [Full API docs on docs.rs](https://docs.rs/phi-agent)
