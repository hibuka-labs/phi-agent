---
description: "Sessions and snapshots in phi-agent: persistent conversation state, workspace tracking and instant save/restore for long-running agent work."
---

# Sessions & Snapshots

phi-agent manages conversation state through sessions and supports creating/restoring snapshots for long-running work.

## Sessions

Each conversation turn is tracked within a session. Sessions provide:

- **Isolation** — each session has its own conversation history
- **Persistence** — event logs written to `~/.phi-agent/sessions/<id>/` as JSONL
- **Locking** — file-based locks prevent concurrent modification
- **Cleanup** — expired sessions are automatically removed

### Session lifecycle

```
create_session() → run_turn() → run_turn() → ... → session expires → cleanup
```

Sessions are created automatically on first use and cleaned up after expiry (default: 50 sessions max, configurable in `base_agent_builder`).

### Event log

Every turn is persisted as JSONL in the session directory:

```
~/.phi-agent/sessions/<session-id>/
  turn-001.jsonl
  turn-002.jsonl
  turn-003.jsonl
```

Each line is a JSON object representing one event (text delta, tool call start, tool call result, etc.).

## Snapshots

Snapshots capture the full conversation state at a point in time. They are useful for:

- Saving progress before a risky operation
- Creating checkpoints in long-running tasks
- Sharing conversation state for debugging

### REPL commands

| Command | Description |
|---------|-------------|
| `/snapshot <name>` | Create a snapshot of the current session |
| `/snapshots` | List all snapshots (sorted by date, newest first) |
| `/session` | Show current session ID and metadata |
| `/events` | Show recent events from the current turn |
| `/tools` | List registered tools |

### Programmatic API

```rust
use phi_agent::SessionContext;

let ctx = SessionContext::new(&session_id);

// Create a snapshot
create_snapshot(&ctx, "before-refactor").await?;

// List snapshots
let snapshots = list_snapshots(&ctx).await?;
for snap in &snapshots {
    println!("{} - {}", snap.name, snap.created_at);
}

// Restore a snapshot
restore_snapshot(&ctx, "before-refactor").await?;

// Delete a snapshot
delete_snapshot(&ctx, "before-refactor").await?;
```

### Snapshot storage

Snapshots are stored alongside session data:

```
~/.phi-agent/sessions/<session-id>/
  snapshots/
    before-refactor.json
    after-migration.json
```

## Validation

Session IDs and snapshot names are validated:

- Session IDs: alphanumeric + hyphens + underscores, max 64 characters
- Snapshot names: alphanumeric + hyphens + underscores, max 128 characters

## See also

- [Timeouts & Recovery](timeout-recovery.md) — what a restored session knows
  about interrupted tool calls, and why recovery never re-executes an action.
