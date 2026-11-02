---
description: "The phi CLI for phi-agent: interactive REPL, one-shot runs and project scaffolding — one binary to run Rust AI agents from the terminal."
---

# CLI Usage

The `phi` CLI supports three modes: REPL, one-shot, and project scaffolding.

## Install

```bash
cargo install phi-agent
```

## Create a project

```bash
phi init my-agent          # REPL with ClockTool example
phi init --lib my-agent    # Single-shot, for library integration
```

## REPL mode

```bash
phi
# phi> Type your question, press Enter
# phi> /exit to quit
```

## One-shot

```bash
phi "What time is it?"
phi "List all .rs files" --model gpt-4o
phi "Show architecture" --format json | jq '.type'
```

## View metrics

```bash
phi metrics list              # All sessions
phi metrics show <session-id> # Session details
phi metrics last              # Most recent session
```

## CLI options

| Option | Description | Default |
|--------|-------------|---------|
| `QUERY` | Positional — one-shot question | — |
| `-m, --model` | Override model name | — |
| `--base-url` | Override API base URL | — |
| `-s, --session-id` | Session ID for resume | — |
| `--format` | Output: `terminal` / `json` / `quiet` | `terminal` |
| `--thinking-effort` | Reasoning: `low` / `medium` / `high` / `xhigh` | `medium` |
| `--thinking-budget` | Max thinking tokens | — |
| `--no-thinking` | Disable thinking | — |
| `--no-tool-args` | Hide tool arguments | — |
| `--no-color` | Disable colors | — |
| `-y, --auto-approve` | Auto-approve all operations | — |
| `--shell-timeout-ms` | Shell timeout (ms) | `30000` |
| `--log-dir` | Log directory | `~/.phi-agent` |
| `--log-level` | Log level | `info` |
| `--no-log` | Disable file logging | — |
| `--max-tool-calls` | Max tool calls per turn | — |
| `--max-failures` | Max consecutive failures | — |

## Session persistence

Sessions are saved to `~/.phi-agent/sessions/<id>/`. Resume with `--session-id`:

```bash
phi --session-id 20250728_a1b2c3d4
```

Sessions inactive for 7 days are auto-cleaned.