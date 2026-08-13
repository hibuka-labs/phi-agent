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

Four things to implement:

| Method | Purpose |
|--------|---------|
| `name()` | Unique identifier the LLM uses to invoke this tool |
| `description()` | Human-readable description of what the tool does |
| `schema()` | JSON Schema describing parameters (sent to the LLM) |
| `call()` | The actual logic — receives parsed args, returns content |

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
