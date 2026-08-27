# Advanced Usage

Middleware, sessions, event logging, and more.

## Middleware

Middleware hooks into the agent loop before and after LLM calls:

```rust
use agent_base::{TurnFactMiddleware, TurnToolLimitMiddleware};

let builder = base_agent_builder(llm_client)
    .system_prompt(system_prompt)
    .middleware(TurnFactMiddleware::new())
    .middleware(TurnToolLimitMiddleware::from_config(&safety));
```

Built-in middleware:
- `TurnFactMiddleware` — injects facts/context at the start of each turn
- `TurnToolLimitMiddleware` — enforces `max_tool_calls_per_turn`

### Custom Middleware

Implement the `Middleware` trait to hook into the agent loop at three points:

```rust
use phi_agent::{AgentResult, Middleware, PreLlmCtx, PostLlmCtx, UserMessageCtx};
use async_trait::async_trait;

struct LoggingMiddleware;

#[async_trait]
impl Middleware for LoggingMiddleware {
    // 1. Called when user sends a message (before anything else)
    async fn on_user_message(&self, ctx: &mut UserMessageCtx) -> AgentResult<()> {
        tracing::info!(session = ?ctx.session_id, input = %ctx.user_input, "user message");
        Ok(())
    }

    // 2. Called just before the LLM call (can modify messages or tools)
    async fn on_pre_llm(&self, ctx: &mut PreLlmCtx) -> AgentResult<()> {
        tracing::info!(session = ?ctx.session_id, msg_count = ctx.messages.len(), "pre-llm");
        Ok(())
    }

    // 3. Called after the LLM responds (can suppress output, inject follow-up)
    async fn on_post_llm(&self, ctx: &mut PostLlmCtx) -> AgentResult<()> {
        tracing::info!(
            session = ?ctx.session_id,
            is_tool_call = ctx.is_tool_call,
            tool_count = ctx.tool_calls.len(),
            "post-llm"
        );
        Ok(())
    }
}

builder = builder.middleware(LoggingMiddleware);
```

Key `PostLlmCtx` fields:

| Field | Type | Description |
|-------|------|-------------|
| `full_text` | `String` | LLM's text response (empty if pure tool call) |
| `is_tool_call` | `bool` | Whether the LLM requested tool calls |
| `tool_calls` | `Vec<(id, name, args)>` | Parsed tool call list |
| `available_tools` | `Vec<String>` | Tools currently registered |
| `total_tool_calls` | `usize` | Total tool calls executed this turn so far |
| `skip_push` | `bool` | Set to `true` to suppress the LLM response from the session |
| `follow_up_message` | `Option<String>` | Inject a follow-up User message into the loop |

### Custom ToolPolicy

Implement the `ToolPolicy` trait to control tool execution behavior — approval, pre-execution checks, and post-execution auditing:

```rust
use phi_agent::{AgentResult, ApprovalRequest, Content, RiskLevel, ToolContext, ToolPolicy};
use agent_base::ToolDecision;
use async_trait::async_trait;
use serde_json::Value;

struct RiskAwarePolicy;

#[async_trait]
impl ToolPolicy for RiskAwarePolicy {
    // 1. Decide whether a tool call needs user approval (async)
    async fn evaluate_approval(&self, tool_name: &str, args: &Value) -> Option<ApprovalRequest> {
        let command = args.get("command").and_then(Value::as_str).unwrap_or("");
        if command.contains("rm ") || command.contains("sudo") {
            return Some(ApprovalRequest {
                title: "Destructive command".into(),
                message: format!("AI wants to run: {command}"),
                action_key: Some(format!("cmd:{command}")),
                risk_level: RiskLevel::Destructive,
                raw: Some(args.clone()),
            });
        }
        None // safe commands run without approval
    }

    // 2. Sync check just before tool execution — return ToolDecision
    fn before_call(
        &self, tool_name: &str, args: &Value, _ctx: &ToolContext,
    ) -> AgentResult<ToolDecision> {
        tracing::info!("about to execute tool: {tool_name}");
        // Proceed with original args, Block to abort, or Modify to rewrite args
        Ok(ToolDecision::Proceed)
    }

    // 3. Sync hook after successful execution — for auditing or metrics
    fn after_call(
        &self, tool_name: &str, _args: &Value, result: &[Content], _ctx: &ToolContext,
    ) -> AgentResult<()> {
        tracing::info!(tool = tool_name, content_count = result.len(), "tool executed");
        Ok(())
    }
}

builder = builder.tool_policy(Arc::new(RiskAwarePolicy));
```

The execution pipeline runs: `evaluate_approval` → (wait for user if needed) → `before_call` → `tool.call()` → `after_call`.

`before_call` returns a `ToolDecision`:

| Variant | Effect |
|---------|--------|
| `ToolDecision::Proceed` | Execute with the original arguments |
| `ToolDecision::Block(msg)` | Abort the call; `msg` is sent to the LLM |
| `ToolDecision::Modify(new_args)` | Execute with replacement arguments |

`Modify` is useful for auto-injecting flags (e.g. `--no-color`), path normalization, or sanitizing inputs before execution.

> 💡 Runnable demo: [`examples/tools/custom_policy.rs`](https://github.com/hibuka-labs/phi-agent/blob/master/examples/tools/custom_policy.rs) covers both custom Middleware and ToolPolicy. Run with `cargo run --example custom_policy` — no API key required.

## Approval Handlers

Control which tool calls require human confirmation:

```rust
// Auto-approve everything (CI / automation)
use phi_agent::{AutoApprovalHandler, ApprovalMode};
builder = builder.approval_handler(Arc::new(
    AutoApprovalHandler::new(ApprovalMode::Auto)
));

// Deny all (read-only / preview mode)
builder = builder.approval_handler(Arc::new(
    AutoApprovalHandler::new(ApprovalMode::DenyAll)
));
```

For interactive CLI approval, see `CliApprovalHandler` in the phi binary.

## Session Management

Sessions persist conversation history and tool call results:

```rust
use phi_agent::session::{resolve_session, cleanup_expired_sessions};

// Create or reuse a session
let ctx = resolve_session(Some("my-session"), &base_dir)?;
println!("Session: {} (new: {})", ctx.session_id, ctx.is_new_session);

// Clean up old sessions (> 7 days)
let cleaned = cleanup_expired_sessions(&base_dir, 7)?;
println!("Cleaned {} expired sessions", cleaned);
```

Session directory layout:
```
~/.phi-agent/sessions/<id>/
├── session_id           # Session ID marker
├── session.lock         # Exclusive file lock
├── session_meta.json    # Created at, last active at
└── turn_001.jsonl       # Per-turn event log (JSONL)
```

## Event Logging

Every turn is persisted as JSONL for replay and analysis:

```rust
use phi_agent::{save_turn_log, event_to_jsonl};

// Save turn events
save_turn_log(&session_ctx, 1, &events, "user query")?;

// Convert a single event to JSONL
let line = event_to_jsonl(&event);
```

Event types in the log:
- `thought_delta` — LLM thinking content
- `text_delta` — Assistant text output
- `tool_call_started` / `tool_call_finished` — Tool invocations
- `approval_request` — When a tool needs approval
- `plan_updated` — Task plan changes
- `turn_finished` — Turn summary with duration and stats

## System Prompts

phi-agent provides two system prompt variants:

```rust
use phi_agent::{build_system_prompt, build_system_prompt_cn};

// Default (international)
let prompt = build_system_prompt();

// China-aware variant (prefers domestic services, handles GFW)
let prompt_cn = build_system_prompt_cn();
```

You can also pass a fully custom prompt via `builder.system_prompt(...)`.

## Reasoning / Thinking

Control the LLM's chain-of-thought behavior:

```rust
use agent_base::{ReasoningConfig, ReasoningEffort};

// Builder-level default
builder = builder.reasoning(ReasoningConfig {
    effort: Some(ReasoningEffort::High),
    ..Default::default()
});

// Per-turn override
agent.set_reasoning_effort(ReasoningEffort::XHigh).await;
```

Effort levels and when to use them:
- `Low` — simple tasks, fast responses
- `Medium` — default, balanced
- `High` — complex multi-step tasks
- `XHigh` — hardest problems, longest think time

## Programmatic Renderers

Use renderers outside the CLI:

```rust
use phi_agent::{
    TerminalRenderer, JsonStreamRenderer, NullRenderer, EventRenderer,
};
use std::io;

// Terminal
let mut renderer = TerminalRenderer::new(true, true, true, Box::new(io::stdout()));

// JSON stream (for IDE integration)
let mut renderer = JsonStreamRenderer::stdout();

// Silent (for web backends)
let mut renderer = NullRenderer;
```

## Error Recovery

phi-agent configures consecutive failure recovery by default:

```rust
use agent_base::ConsecutiveFailureRecovery;

// 3 consecutive failures → stop and explain
builder = builder.error_recovery(Arc::new(
    ConsecutiveFailureRecovery::new(3)
));
```

## Parallel Tool Execution

When the LLM returns multiple tool calls in a single turn, they execute **concurrently** via `join_all`. This reduces latency from `sum(tool_times)` to `max(tool_times)`.

- Approval is handled sequentially first (batch-approve all calls)
- Then all approved tools run in parallel
- A single tool failure does **not** abort the others — it's collected in `failures`

No configuration needed — parallel execution is the default.

## Context Compression

For long conversations that approach the context window limit, enable compression:

```rust
use phi_agent::CompressionMiddleware;

builder = builder.middleware(CompressionMiddleware::new());
```

When the context window fills up, the framework automatically compresses the conversation history and continues the loop — no restart needed.

CLI users can also compress manually with `/compact`.

## Prompt Fragments (Composable Prompts)

Instead of a single hardcoded system prompt, you can compose it from independent fragments:

```rust
use agent_base::PromptFragment;
use agent_base::FragmentContext;

struct MyPersonalityFragment;

impl PromptFragment for MyPersonalityFragment {
    fn name(&self) -> &str { "personality" }
    fn priority(&self) -> i32 { 10 } // lower = earlier in prompt
    fn render(&self, _ctx: &FragmentContext) -> Option<String> {
        Some("You are a helpful assistant specialized in Rust.".into())
    }
}
```

Fragments are sorted by `priority` and concatenated. The built-in `DynamicToolsFragment` automatically injects registered tool descriptions into the prompt.

## Further Reading

No-API-key examples — run immediately:

```bash
cargo run --example custom_policy    # ToolPolicy + Middleware + event hooks
cargo run --example session_persist  # Session lifecycle and file locking
cargo run --example event_log        # Per-turn JSONL event persistence
```

→ [Full examples table](https://github.com/hibuka-labs/phi-agent#examples) — all 17 examples with API key requirements.
