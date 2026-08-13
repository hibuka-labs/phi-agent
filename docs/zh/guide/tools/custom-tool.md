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

需要实现四个方法：

| 方法 | 作用 |
|------|------|
| `name()` | LLM 调用此工具时使用的唯一标识 |
| `description()` | 人类可读的工具用途描述 |
| `schema()` | JSON Schema 描述参数（发送给 LLM） |
| `call()` | 实际逻辑 — 接收解析后的参数，返回内容 |

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
