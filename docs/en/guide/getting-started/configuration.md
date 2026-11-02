---
description: "Configure phi-agent: LLM providers, API keys, models and agent behavior via CLI flags, environment variables or .env files, with clear precedence."
---

# Configuration

All configuration options and how to set them.

## LLM Configuration

Environment variables (`.env` file or system env):

| Variable | Description | Default |
|----------|-------------|---------|
| `LLM_API_KEY` | API key (required) | — |
| `LLM_MODEL` | Model name | `gpt-4o` |
| `LLM_BASE_URL` | API endpoint | `https://api.openai.com/v1` |

Also accepts `OPENAI_API_KEY`, `OPENAI_MODEL`, `OPENAI_BASE_URL` as fallbacks.

### Priority

```
CLI argument > environment variable > .env file > default
```

## Agent Configuration

`PhiAgentConfig` fields:

| Field | Type | Description | Default |
|-------|------|-------------|---------|
| `model` | `String` | Model name passed to provider | — |
| `enable_thinking` | `bool` | Enable chain-of-thought | `true` |
| `thinking_budget` | `Option<u64>` | Token budget for thinking | `None` (provider default) |
| `thinking_effort` | `ReasoningEffort` | Low / Medium / High / XHigh | `Medium` |
| `safety` | `SafetyConfig` | Tool call limits | See below |
| `max_turns` | `Option<u32>` | Per-run react-loop cap | `None` (builder default: 200) |

## Safety Config

```rust
SafetyConfig {
    max_tool_calls_per_turn: 128,  // Max tool invocations per turn
    max_consecutive_failures: 3,   // Stop after N consecutive failures
}
```

## CLI Flags

| Flag | Description |
|------|-------------|
| `--format <fmt>` | Output format: `terminal`, `json`, `quiet` |
| `--model <name>` | Override model name |
| `--base-url <url>` | Override API base URL |
| `--no-thinking` | Disable thinking/chain-of-thought |
| `--thinking-budget <n>` | Token budget for thinking |
| `--thinking-effort <level>` | `low` / `medium` / `high` / `xhigh` |
| `--no-tool-args` | Hide tool argument details |
| `--no-color` | Disable terminal colors |
| `--max-tool-calls <n>` | Max tool calls per turn |
| `--max-failures <n>` | Max consecutive failures |
| `-y` / `--auto-approve` | Auto-approve all tool calls |
| `--session-id <id>` | Specify session ID |
| `--shell-timeout-ms <ms>` | Timeout for shell commands |
| `--log-dir <dir>` | Log directory (default `~/.phi-agent`) |
| `--log-level <level>` | Log level (default `info`) |
| `--no-log` | Disable file logging |

## Output Formats

| Format | CLI Flag | Use Case |
|--------|----------|----------|
| Terminal | `--format terminal` (default) | Human interaction — colors, emoji, streaming |
| JSON | `--format json` | Scripting / IDE integration — one JSON per line |
| Quiet | `--format quiet` | Web backend — no stdout, tracing only |

## Session Directory

Sessions stored at `~/.phi-agent/sessions/<session_id>/`:

```
session_id            # Session ID marker
session.lock          # File lock (prevent concurrent access)
session_meta.json     # Creation time, last active time
session_metrics.json  # Metrics (tokens, latency, cost)
session.log           # Human-readable log (if enabled)
turn_001.jsonl        # Per-turn event log
turn_002.jsonl
...
```

Sessions inactive for 7+ days are auto-cleaned at startup.
