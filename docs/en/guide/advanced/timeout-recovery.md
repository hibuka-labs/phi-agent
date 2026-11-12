---
description: "What happens when a tool call times out, crashes, or is retried in phi-agent: one-shot side effects, observation replay, and how a restored session avoids duplicate actions."
---

# Timeouts & Recovery

When a tool call is interrupted — by a timeout, a crash, or a cancelled run — the
remote action may already have happened. This page explains how phi-agent's
execution kernel handles that ambiguity: what a timed-out call returns, what a
restored session knows, and why a resumed batch is never re-executed.

## The problem

A tool call is not a pure function. `send_email`, `refund_payment`, or
`create_record` mutate the outside world, and "the call didn't finish" does not
mean "the action didn't happen":

- a call can **time out while the remote side has already committed**;
- a process can **crash between "started" and "finished"**;
- a parent agent can **stop waiting while a child agent is still working**.

A naive retry in any of these cases produces a duplicate: two emails, two
refunds, two records. Everything below follows one invariant:

> **Side effects are one-shot. Recovery never re-executes an action.**

## The approach

### A timeout is a plain result, not an error state

Every tool call runs under an execution budget. On overrun, the call returns an
ordinary result whose content is the text `[Tool Timeout]` — the model reads it
like any other tool output and decides what to do next. Structurally there is no
separate "timed out" or "unknown" status; the timeout is part of the result
text.

Three properties of the budget:

- **Active time, not wall clock.** Time the tool spent paused waiting for a
  human (e.g. a confirmation popup) extends the deadline by exactly the paused
  amount. A user reading a dialog never causes a bogus timeout.
- **Precedence**: a per-call `timeout_ms` argument (for tools that accept one)
  → the tool's own override → global config → the framework default
  (10 minutes). A budget of `0` means unbounded — the tool arms its own fuse or
  the call is fire-and-forget.
- **Local cancellation is best-effort.** On overrun the in-flight future is
  dropped, but work already handed to the outside world cannot be recalled. A
  `[Tool Timeout]` result means *the outcome is unknown*, not *nothing happened*.

### Errors go back to the model — the kernel never retries on its own

A failed call's error is fed back to the LLM as a tool result, and the reasoning
loop continues — the model chooses whether to try again, switch strategy, or
stop. After the same tool fails repeatedly, the model gets one grace round with
the full error history before the run is stopped. There is no automatic
re-execution anywhere in the call path, and no implicit deduplication layer:
retries are visible, deliberate acts by the model (or the user).

### Checkpoints mark batch boundaries

While a turn runs, the kernel writes checkpoints at action boundaries:

```
CheckpointStep::BeforeActions { actions }        // id, tool name, args per call
CheckpointStep::AfterActions { actions, observation }  // + recorded results
```

A checkpoint captures *what was about to run* (or *what ran and what it
returned*), at the granularity of the whole batch of tool calls the model
requested in that turn.

### Resume replays; it never re-runs

Restoring a session rebuilds the conversation history from checkpoints and
recorded observations:

```
completed batch  ──► replay the recorded observation verbatim
                     (recorded evidence, never rewritten)

dangling batch   ──► synthesize a fate for every call:
   (crash window)    "已中断/未执行" (interrupted / not executed)
                     the batch is NOT re-executed
```

A dangling batch is the crash window: the process died after `BeforeActions`
was written but before `AfterActions` landed. The kernel cannot know which
calls in that batch had already reached the outside world, so it records the
interruption rather than guessing a success — and it never re-executes the
batch, because a re-run could duplicate the very side effects that may already
have happened. What the model sees after restore is a complete, provider-valid
history (assistant tool-call message + one result per call) ending in those
synthetic fates, so the next turn starts from honest ground.

The exact fate text depends on which recovery entry point restores the
session: the checkpoint-replay path records `已中断/未执行`, while the
window-snapshot (`messages.jsonl`) restore path patches dangling calls with
`interrupted`. Both mean the same thing — the call was interrupted and its
external effect is unknown — and neither is a confirmation that nothing
happened.

### Multi-agent: a wait timeout is not a failure

When a parent agent waits on a child with a deadline and the deadline elapses,
the outcome is `Timeout` — a first-class, non-error outcome that means *the
wait ended*, not *the child failed*. The child keeps running; its result lands
in the mailbox later and a subsequent wait picks it up. Tasks run serially
inside a child, and a per-task execution timeout is optional (off by default).

The mirror image of the invariant applies here: sending the same task twice
runs it twice. There is no idempotency key on tasks and no deduplication in the
mailbox — if a parent treats `Timeout` as a failure and re-sends, the child
cannot tell a retry from a new request. (The same caution drives the MCP
client's retry policy: `tools/call` is not idempotent, so it is retried at most
once.)

## What a restored session knows

| Question | After restore |
|---|---|
| Which calls was the interrupted batch about to run? | Yes — call id, tool name and arguments, from the checkpoint |
| Did a call from a dangling batch take effect? | **Unknown** — recorded as an interrupted fate (`已中断/未执行` or `interrupted`, by recovery entry point); never assumed either way |
| What did a completed batch return? | The exact recorded observation |
| A provider idempotency key for safe retry? | Not provided — that contract belongs to the tool / provider |

## Trade-offs

- **Timeout as text, not a structured state.** Simple, provider-agnostic, and
  visible in every UI that renders tool output — but programs cannot
  distinguish a timeout from a normal result without parsing the text.
- **Batch-granular recovery.** If three calls were in flight and only the first
  committed, the restored history still folds all three into the same
  interrupted fate. Per-action truth lives in the event log (observability),
  not in the recovery state.
- **No built-in idempotency layer.** The kernel stays neutral: it cannot know
  which tools are safe to repeat. Deduplication keys, if any, belong to the
  provider contract.

## What this means for safe retries

Treat `[Tool Timeout]` and the interrupted fates (`已中断/未执行`,
`interrupted`) as **unknown outcomes**:

1. **Verify before re-issuing.** Read back the external state (did the email
   send? does the record exist?) instead of assuming failure.
2. **Prefer tools with idempotency keys.** If the provider accepts a key,
   re-sending with the same key is safe; the kernel doesn't manage this for you.
3. **Don't re-send tasks on a wait timeout.** Wait again — the child may have
   finished in the meantime.

## Related

- [Sessions & Snapshots](session.md) — session persistence and the JSONL event
  log that records every tool call start/finish.
- [Multi-Agent](multi-agent.md) — spawning child agents, tasks and mailboxes.
- [Custom Tools](../tools/custom-tool.md) — implementing tools, including
  per-call timeout overrides.
