# React Loop Guard

> LLMs aren't perfect. They go silent, give half-answers, or spin in circles. The Guard makes the agent recover on its own — instead of looping forever or calling it done too early.

## What is a React Loop

ReAct (**Re**asoning + **Act**ing) is the core working pattern of an agent: the LLM reasons → calls a tool → observes the result → reasons again, looping until the task is done. This "think-act" cycle is the React Loop.

The React Loop Guard protects this cycle — when the LLM produces abnormal output mid-loop, the Guard detects and handles it automatically, preventing infinite loops or premature termination.

## The Problem

Agent loops depend on the LLM producing valid responses turn after turn. In practice, LLMs sometimes:

- **Spin** — output internal reasoning but never call a tool or give an answer, like thinking forever without acting
- **Go silent** — return completely empty, saying nothing at all
- **Quit early** — call a bunch of tools, then wrap up with a one-liner that doesn't actually answer the question

Without a Guard, these either burn tokens in infinite loops or silently fail — you think it's done, but it isn't.

We hit these problems again and again across real-world vertical agent deployments. The Guard is the result — not a single hard-coded rule, but a layered, configurable, trait-based defense system refined through production use.

## Four Lines of Defense

The Guard automatically detects anomalies after every LLM response. No manual checks needed:

| Anomaly | What happened | What the Guard does |
|---------|--------------|-------------------|
| **reasoning-only** | LLM keeps thinking, never acts | Injects a nudge to decide; stops after 3 strikes |
| **empty response** | LLM says nothing at all | Injects a nudge to retry; stops after 3 strikes |
| **text-only** | LLM gives a plain text answer | Ends immediately, trusts the model |
| **text-only after tools** | Short answer after calling tools | **LLM judge steps in** — is the task actually done? |

The first three are basic protection. The fourth is the key — the **judge mechanism**.

## Judge: Using LLM to Supervise LLM

When the agent returns plain text after calling tools, the Guard doesn't just trust it. It calls another LLM as a referee:

```
User question + Agent response → Judge → {"done": true/false, "reason": "..."}
```

- Judge says done → loop ends
- Judge says not done → reason is injected, agent keeps working

This is fully transparent to the user, with zero extra configuration. The judge itself is carefully optimized — short response detection, long response skip, large input fallback — balancing safety with token efficiency.

## Configuration

```rust
use agent_works::guard::{DefaultGuard, DefaultGuardConfig};

// Works out of the box (recommended)
let guard = DefaultGuard::with_llm_client(
    DefaultGuardConfig::default(),
    llm_client,
);
builder = builder.guard(guard);
```

| Parameter | Default | Tuning tip |
|-----------|---------|-----------|
| `use_llm_judge` | `true` | Turning off saves tokens but risks premature completion |
| `judge_skip_threshold` | `256` | Skip judge for long responses. Higher = stricter, lower = saves more |
| `judge_fail_open` | `false` | `false` = safer, extra round on failure; `true` = trust model if judge fails |
| `judge_timeout_secs` | `10` | Judge timeout ceiling |
| `detect_short_response` | `true` | Long question + short answer = likely incomplete, auto-nudge |
| `reasoning_only_max_strikes` | `3` | How many spins before giving up |
| `empty_response_max_strikes` | `3` | How many silences before giving up |

## Fault Tolerance: Fail-Closed vs Fail-Open

The judge defaults to **fail-closed**: when the judge times out or errors, the model is not trusted and the agent is forced to continue.

This is a lesson from production — for long-running tasks, the cost of finishing early far outweighs the cost of one extra round. Better to confirm than to miss something.

For simpler tasks or unstable judges, switch to fail-open:

```rust
let config = DefaultGuardConfig {
    judge_fail_open: true,
    ..DefaultGuardConfig::default()
};
```

## Disabling the Judge

Don't pass an LLM client to disable the judge entirely. Good for latency-sensitive scenarios where you'd rather risk premature completion than add overhead:

```rust
let guard = DefaultGuard::new(DefaultGuardConfig::default());
```

## Extending

The Guard is built on a trait, supporting full custom implementations or decorator-style enhancement. See the [agent-base design docs](https://github.com/hibuka-labs/agent-base) for details.
