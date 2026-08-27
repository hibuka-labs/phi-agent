# 自定义工具

phi-agent 不内置任何工具 — 你通过实现 `Tool` trait 来创建自己的工具。

## Tool Trait

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn schema(&self) -> Value;
    async fn call(&self, args: &Value, ctx: &ToolContext) -> AgentResult<Vec<Content>>;
}
```

需要实现四个必需方法：

| 方法 | 作用 |
|------|------|
| `name()` | LLM 调用此工具时使用的唯一标识 |
| `description()` | 人类可读的工具用途描述 |
| `schema()` | JSON Schema 描述参数（发送给 LLM） |
| `call()` | 实际逻辑 — 接收解析后的参数，返回内容 |

以及可选的覆盖方法（有默认值）：

| 方法 | 默认值 | 作用 |
|------|--------|------|
| `timeout_ms()` | `None`（使用框架默认值） | 单个工具的超时时间（毫秒） |
| `metadata()` | `origin: "custom"`, `version: "unknown"` | 机器可读的来源和版本信息 |
| `exposure()` | `Direct` | 可见性：`Direct` / `Deferred` / `Hidden` |
| `should_activate()` | `true` | `Deferred` 工具的激活条件 |

## 示例：天气工具

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
        "获取指定城市的当前天气"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "city": {
                    "type": "string",
                    "description": "城市名称，例如 '北京'"
                }
            },
            "required": ["city"]
        })
    }

    async fn call(&self, args: &Value, _ctx: &ToolContext) -> AgentResult<Vec<Content>> {
        let city = args["city"].as_str().unwrap_or("unknown");
        // 生产环境中，这里调用真实的天气 API
        Ok(vec![Content::text(format!("{} 天气：22°C，晴", city))])
    }
}
```

## 注册工具

```rust
let builder = base_agent_builder(llm_client)
    .system_prompt(build_system_prompt())
    .register_tool(WeatherTool);   // ← 在这里注册

let agent = PhiAgent::build(builder, config)?;
```

## TypedTool — 从类型生成 Schema

无需手写 JSON Schema，实现 `TypedTool` 即可通过 `schemars` 从类型化的 `Args` 结构体自动派生：

```rust
use agent_base::{AgentResult, Content, TypedTool, ToolContext};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
struct WeatherArgs {
    /// 城市名称，例如 "北京"
    city: String,
}

struct WeatherTool;

#[async_trait]
impl TypedTool for WeatherTool {
    type Args = WeatherArgs;
    type Output = String;  // String → 直接转为 Content::text；其他类型 → JSON

    fn name(&self) -> &'static str { "get_weather" }
    fn description(&self) -> &'static str { "获取指定城市的当前天气" }

    async fn call_typed(&self, args: WeatherArgs, _ctx: &ToolContext) -> AgentResult<String> {
        Ok(format!("{} 天气：22°C，晴", args.city))
    }
}
```

`TypedTool` 自动实现 `Tool` — 和普通工具一样注册即可：

```rust
builder.register_tool(WeatherTool);
```

## 工具可见性

默认所有工具都是 `Direct` — 始终对模型可见。你可以控制可见性：

```rust
use agent_base::ToolExposure;

impl Tool for MyTool {
    // ...

    fn exposure(&self) -> ToolExposure {
        ToolExposure::Deferred  // 仅在 should_activate 返回 true 时可见
    }

    async fn should_activate(&self, ctx: &ActivationContext) -> bool {
        // 仅当特定工具存在时激活
        ctx.current_tools.iter().any(|t| t == "file_read")
    }
}
```

| 可见性 | 行为 |
|--------|------|
| `Direct` | 始终发送给模型（默认） |
| `Deferred` | 条件可见 — 由 `should_activate()` 控制 |
| `Hidden` | 对模型永远不可见（内部/框架工具） |

`ActivationContext` 提供 `session_id`、`current_tools`（已激活的工具名称列表）和 `workspace`（工作目录）。

## 元数据和超时

覆盖 `metadata()` 以提供工具来源和版本信息：

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

覆盖 `timeout_ms()` 为需要更长或更短时间的工具设置超时：

```rust
fn timeout_ms(&self) -> Option<u64> {
    Some(30_000)  // 30 秒
}
```

返回 `None` 使用框架默认值（`ToolConfig.default_tool_timeout_ms`）。

## Content

工具返回 `Vec<Content>`。`Content::text(...)` 创建一个简单的文本结果：

```rust
Ok(vec![Content::text("完成")])
```

`Content` 也支持图片（`Content::image(data, mime_type)`），不过目前首个 LLM 适配器只消费文本。

## 最佳实践

1. **一个文件一个工具** — 保持工具实现聚焦、可测试
2. **校验参数** — 不要信任 LLM 提供的类型一定正确
3. **优雅处理错误** — 返回有意义的错误信息，让 LLM 能据此调整
4. **保持 `description()` 和 `schema()` 准确** — 如果 LLM 的理解和实际行为不一致，工具调用会失败
5. **为长操作设置超时** — 对网络调用使用 `tokio::time::timeout`

## 完整示例

参见 [`examples/custom-tool.rs`](https://github.com/hibuka-labs/phi-agent/blob/master/examples/custom-tool.rs) 了解一个带计算器工具的完整可运行示例。
