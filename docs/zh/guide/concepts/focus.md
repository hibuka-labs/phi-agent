---
description: "Focus 在主对话循环之外做结构化单任务 LLM 调用：分类、判断、结构化提取，直接返回 Rust 类型化结果。"
---

# Focus — 结构化单任务 LLM 调用

Focus 是一个轻量级原语，用于在主 agent 对话循环之外进行独立的、单一用途的 LLM 调用。它专为**分类、判断和结构化提取**而设计——任何时候你需要 LLM 做出一个专注的决定并返回类型化结果。

## 为什么需要 Focus？

在正常的 agent 对话中，LLM 同时管理工具、上下文和多轮推理。但有时候你只需要一个特定问题的答案：

- "这个终端输出正常，还是表示有错误？"
- "将这个用户请求分类为：提问、命令或闲聊。"
- "从这段文本中提取关键实体为结构化 JSON。"

把这些判断丢进主 agent 循环会增加噪音。Focus 将它们分解为独立的调用——一个系统提示、一个输入、一个类型化输出。如果业务分解得当，即使是弱模型也能做好一件事。

## 快速示例

```rust
use std::sync::Arc;
use std::time::Duration;
use phi_agent::{Focus, OpenAiClient};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct Sentiment {
    sentiment: String,  // "positive", "negative", "neutral"
    confidence: f64,    // 0.0 到 1.0
}

async fn classify_sentiment(client: Arc<OpenAiClient>, text: String) -> Result<Sentiment, FocusError> {
    let focus = Focus::new(
        client,
        "你是一个情感分类器。分析文本并返回 JSON: \
         {\"sentiment\": \"positive|negative|neutral\", \"confidence\": 0.0-1.0}",
    );

    let output = focus.ask::<Sentiment>(&text, Duration::from_secs(10)).await?;
    Ok(output.result)
}
```

就这样。没有 agent 循环，没有工具注册——只有一次专注的调用和一个类型化的返回值。

## 核心概念

### 1. Focus

`Focus` 在创建时将 LLM 客户端绑定到一个**系统提示**。系统提示描述角色和期望的输出格式。一旦创建，系统提示不再改变——一个 `Focus` 实例只做一件事。

```rust
pub struct Focus {
    // 持有 Arc<dyn LlmClient> + system_prompt (私有)
}

impl Focus {
    pub fn new(client: Arc<dyn LlmClient>, system_prompt: impl Into<String>) -> Self;
    pub async fn ask<T: DeserializeOwned>(&self, input: &impl FocusInput, timeout: Duration)
        -> Result<FocusOutput<T>, FocusError>;
}
```

- **`new()`** 很廉价——多个 Focus 实例可以共享同一个 LLM 客户端。
- **`ask::<T>()`** 发送系统提示 + 用户输入，强制 JSON 输出模式，并反序列化到你的类型 `T`。
- **超时**是显式的——你控制等待多久。

### 2. FocusInput

任何可以格式化到用户提示的内容：

| 输入 | 使用场景 |
|-------|----------|
| `&str` / `String` | 单段需要分类或判断的文本 |
| `FocusContext` | 多个相关字段（如终端输出 + 耗时 + 命令） |

### 3. FocusContext（结构化输入）

当需要发送多个带标签的字段时：

```rust
use phi_agent::FocusContext;

let ctx = FocusContext::new()
    .add("command", "apt install nginx")
    .add("elapsed", "30s")
    .add("screen", "Reading package lists...\nBuilding dependency tree...");

let output = focus.ask::<TaskStatus>(&ctx, Duration::from_secs(5)).await?;
```

字段在发送给 LLM 前格式化为 `【key】\nvalue`，每个标签作为模型的上下文。

### 4. FocusOutput\<T\>

返回值包含结构化结果和原始响应：

```rust
pub struct FocusOutput<T> {
    pub result: T,           // 从 JSON 反序列化
    pub raw_response: String, // 原始 LLM 输出（用于调试）
}
```

保留 `raw_response` 用于日志——解析失败时，它告诉你 LLM 到底返回了什么。

### 5. FocusError

三种失败模式，全部明确：

```rust
pub enum FocusError {
    Timeout(Duration),                     // LLM 未及时响应
    Llm(String),                           // 网络错误、API 错误等
    Parse { error: String, raw: String },  // LLM 未返回匹配 T 的有效 JSON
}
```

## Focus vs. Agent 的使用场景

| 场景 | 使用 |
|------|------|
| 带工具的多轮对话 | Agent (`PhiAgent::run_turn`) |
| 一次性分类或判断 | Focus |
| 从文本中结构化提取 | Focus |
| Agent 循环外的预处理/后处理 | Focus |
| 简单的"完成了吗？"/"什么状态？"检查 | Focus |

常见模式：在工具实现中使用 Focus 作为**辅助调用**。你的工具完成机械工作（运行命令、获取数据），然后用 Focus 解释结果。

## 完整示例

参见 [`examples/focus-demo.rs`](https://github.com/hibuka-labs/phi-agent/blob/master/examples/focus-demo.rs) 获取完整的可运行示例。

## API 参考

Focus 类型从 phi-agent 重新导出：

```rust
pub use agent_works::focus::{
    Context as FocusContext,
    Focus,
    FocusError,
    FocusInput,
    FocusOutput,
};
```

详细的 API 文档见 [docs.rs/phi-agent](https://docs.rs/phi-agent)。
