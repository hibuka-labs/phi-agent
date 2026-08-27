# Custom Tools

phi-agent doesn't bundle any tools — you bring your own by implementing the `Tool` trait.

## The Tool Trait

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn schema(&self) -> Value;
    async fn call(&self, args: &Value, ctx: &ToolContext) -> AgentResult<Vec<Content>>;
}
```

Four required methods:

| Method | Purpose |
|--------|---------|
| `name()` | Unique identifier the LLM uses to invoke this tool |
| `description()` | Human-readable description of what the tool does |
| `schema()` | JSON Schema describing parameters (sent to the LLM) |
| `call()` | The actual logic — receives parsed args, returns content |

Plus optional overrides with sensible defaults:

| Method | Default | Purpose |
|--------|---------|---------|
| `timeout_ms()` | `None` (use framework default) | Per-tool timeout in milliseconds |
| `metadata()` | `origin: "custom"`, `version: "unknown"` | Machine-readable origin and version info |
| `exposure()` | `Direct` | Visibility: `Direct` / `Deferred` / `Hidden` |
| `should_activate()` | `true` | Activation condition for `Deferred` tools |

## Example: Weather Tool

```rust
use agent_base::{AgentResult, Content, Tool, ToolContext};
use async_trait::async_trait;
use serde_json::{Value, json};

struct WeatherTool;

#[async_trait]
impl Tool for WeatherTool {
    fn name(&self) -> &'static str {
        "get_weather"
    }

    fn description(&self) -> &'static str {
        "Get current weather for a city"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "city": {
                    "type": "string",
                    "description": "City name, e.g. 'Beijing'"
                }
            },
            "required": ["city"]
        })
    }

    async fn call(&self, args: &Value, _ctx: &ToolContext) -> AgentResult<Vec<Content>> {
        let city = args["city"].as_str().unwrap_or("unknown");
        // In production, call a real weather API here
        Ok(vec![Content::text(format!("Weather in {}: 22°C, sunny", city))])
    }
}
```

## Registering

```rust
let builder = base_agent_builder(llm_client)
    .system_prompt(build_system_prompt())
    .register_tool(WeatherTool);   // ← register here

let agent = PhiAgent::build(builder, config)?;
```

## TypedTool — Schema from Types

Instead of writing JSON Schema by hand, implement `TypedTool` to auto-derive the schema from a typed `Args` struct via `schemars`:

```rust
use agent_base::{AgentResult, Content, TypedTool, ToolContext};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
struct WeatherArgs {
    /// City name, e.g. "Beijing"
    city: String,
}

struct WeatherTool;

#[async_trait]
impl TypedTool for WeatherTool {
    type Args = WeatherArgs;
    type Output = String;  // String → Content::text directly; other types → JSON

    fn name(&self) -> &'static str { "get_weather" }
    fn description(&self) -> &'static str { "Get current weather for a city" }

    async fn call_typed(&self, args: WeatherArgs, _ctx: &ToolContext) -> AgentResult<String> {
        Ok(format!("Weather in {}: 22°C, sunny", args.city))
    }
}
```

`TypedTool` implements `Tool` automatically — register it the same way:

```rust
builder.register_tool(WeatherTool);
```

## Tool Exposure

By default all tools are `Direct` — always visible to the model. You can control visibility:

```rust
use agent_base::ToolExposure;

impl Tool for MyTool {
    // ...

    fn exposure(&self) -> ToolExposure {
        ToolExposure::Deferred  // only visible when should_activate returns true
    }

    async fn should_activate(&self, ctx: &ActivationContext) -> bool {
        // Activate only when a specific other tool is present
        ctx.current_tools.iter().any(|t| t == "file_read")
    }
}
```

| Exposure | Behavior |
|----------|----------|
| `Direct` | Always sent to the model (default) |
| `Deferred` | Conditionally visible — gated by `should_activate()` |
| `Hidden` | Never visible to the model (internal/framework tools) |

`ActivationContext` provides `session_id`, `current_tools` (already-activated tool names), and `workspace` (working directory).

## Metadata and Timeout

Override `metadata()` to provide origin and version info for tool introspection:

```rust
fn metadata(&self) -> ToolMetadata {
    ToolMetadata {
        name: self.name().to_string(),
        description: self.description().to_string(),
        origin: "phi-tools".to_string(),
        version: "0.2.0".to_string(),
        requirements: vec!["network".to_string()],
    }
}
```

Override `timeout_ms()` for tools that need more or less time than the framework default:

```rust
fn timeout_ms(&self) -> Option<u64> {
    Some(30_000)  // 30 seconds
}
```

Return `None` to use the framework default from `ToolConfig.default_tool_timeout_ms`.

## Content

Tools return a `Vec<Content>`. `Content::text(...)` creates a simple text result:

```rust
Ok(vec![Content::text("Done")])
```

`Content` also supports images via `Content::image(data, mime_type)`, though only text is consumed by the first LLM adapter.

## Best Practices

1. **One tool per file** — keep tool implementations focused and testable
2. **Validate args** — never trust the LLM to provide correct types
3. **Handle errors gracefully** — return meaningful error messages the LLM can act on
4. **Keep `description()` and `schema()` accurate** — if the LLM's understanding doesn't match reality, tool calls will fail
5. **Timeout long operations** — use `tokio::time::timeout` for network calls

## Full Example

See [`examples/custom-tool.rs`](https://github.com/hibuka-labs/phi-agent/blob/master/examples/custom-tool.rs) for a complete runnable example with a calculator tool.
