---
description: "Focus makes structured single-purpose LLM calls outside the agent loop — classification, judgment and typed extraction returning plain Rust results."
---

# Focus — Structured Single-Purpose LLM Calls

Focus is a lightweight primitive for making standalone, single-purpose LLM calls outside the main agent conversation loop. It's designed for **classification, judgment, and structured extraction** — anytime you need the LLM to make one focused decision and return a typed result.

## Why Focus?

In a normal agent conversation, the LLM manages tools, context, and multi-turn reasoning all at once. But sometimes you just need an answer to one specific question:

- "Is this terminal output normal, or does it indicate an error?"
- "Classify this user request as: question, command, or chitchat."
- "Extract the key entities from this text as structured JSON."

Throwing these judgments into the main agent loop adds noise. Focus decomposes them into isolated calls — one system prompt, one input, one typed output. If the decomposition is good, even a weak model can do one thing well.

## Quick Example

```rust
use std::sync::Arc;
use std::time::Duration;
use phi_agent::{Focus, OpenAiClient};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct Sentiment {
    sentiment: String,  // "positive", "negative", or "neutral"
    confidence: f64,    // 0.0 to 1.0
}

async fn classify_sentiment(client: Arc<OpenAiClient>, text: String) -> Result<Sentiment, FocusError> {
    let focus = Focus::new(
        client,
        "You are a sentiment classifier. Analyze the text and return JSON: \
         {\"sentiment\": \"positive|negative|neutral\", \"confidence\": 0.0-1.0}",
    );

    let output = focus.ask::<Sentiment>(&text, Duration::from_secs(10)).await?;
    Ok(output.result)
}
```

That's it. No agent loop, no tool registration — just a focused call with a typed return value.

## Core Concepts

### 1. Focus

`Focus` binds an LLM client to a **system prompt** at creation time. The system prompt describes the role and expected output format. Once created, the system prompt never changes — a `Focus` instance does one job.

```rust
pub struct Focus {
    // Holds Arc<dyn LlmClient> + system_prompt (private)
}

impl Focus {
    pub fn new(client: Arc<dyn LlmClient>, system_prompt: impl Into<String>) -> Self;
    pub async fn ask<T: DeserializeOwned>(&self, input: &impl FocusInput, timeout: Duration)
        -> Result<FocusOutput<T>, FocusError>;
}
```

- **`new()`** is cheap — multiple Focus instances can share the same LLM client.
- **`ask::<T>()`** sends system prompt + user input, forces JSON output mode, and deserializes into your type `T`.
- **Timeout** is explicit — you control how long to wait.

### 2. FocusInput

Anything that can be formatted into the user prompt:

| Input | When to Use |
|-------|-------------|
| `&str` / `String` | Single piece of text to classify or judge |
| `FocusContext` | Multiple related fields (e.g., terminal output + elapsed time + command) |

### 3. FocusContext (structured input)

When you need to send multiple labeled fields:

```rust
use phi_agent::FocusContext;

let ctx = FocusContext::new()
    .add("command", "apt install nginx")
    .add("elapsed", "30s")
    .add("screen", "Reading package lists...\nBuilding dependency tree...");

let output = focus.ask::<TaskStatus>(&ctx, Duration::from_secs(5)).await?;
```

Fields are formatted as `【key】\nvalue` before being sent to the LLM, with each label acting as context for the model.

### 4. FocusOutput\<T\>

The return value contains both the structured result and the raw response:

```rust
pub struct FocusOutput<T> {
    pub result: T,           // Deserialized from JSON
    pub raw_response: String, // Raw LLM output (for debugging)
}
```

Keep `raw_response` for logging — when parsing fails, it tells you exactly what the LLM returned.

### 5. FocusError

Three failure modes, all explicit:

```rust
pub enum FocusError {
    Timeout(Duration),              // LLM didn't respond in time
    Llm(String),                    // Network error, API error, etc.
    Parse { error: String, raw: String }, // LLM didn't return valid JSON matching T
}
```

## When to Use Focus vs. the Agent

| Scenario | Use |
|----------|-----|
| Multi-turn conversation with tools | Agent (`PhiAgent::run_turn`) |
| One-off classification or judgment | Focus |
| Structured extraction from text | Focus |
| Pre/post processing outside agent loop | Focus |
| Simple "is this done?" / "what state is this?" checks | Focus |

A common pattern: use Focus as a **sidecar** inside a tool implementation. Your tool does the mechanical work (run a command, fetch data), then uses Focus to interpret the result.

## Full Example

See [`examples/focus-demo.rs`](https://github.com/hibuka-labs/phi-agent/blob/master/examples/focus-demo.rs) for a complete runnable example.

## API Reference

Focus types are re-exported from phi-agent:

```rust
pub use agent_works::focus::{
    Context as FocusContext,
    Focus,
    FocusError,
    FocusInput,
    FocusOutput,
};
```

For detailed API docs, see [docs.rs/phi-agent](https://docs.rs/phi-agent).
