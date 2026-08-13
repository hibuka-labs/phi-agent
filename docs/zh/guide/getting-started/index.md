# 快速开始

5 分钟跑起你的第一个 phi-agent。

## 前置条件

- [Rust](https://rustup.rs)（stable，edition 2024）
- 一个 LLM API Key（兼容 OpenAI 接口）

## 安装

```bash
cargo install phi-agent
```

> 如需 Shell 命令执行，加上 `--features shell`。

## 方式一：REPL 交互（推荐）

**1. 创建项目**

```bash
phi init my-agent
```

**2. 配置 API Key**

```bash
cd my-agent
cp .env.example .env
```

编辑 `.env`，改成你的 Key：

```
LLM_API_KEY=sk-your-key-here
LLM_BASE_URL=https://api.openai.com/v1
LLM_MODEL=gpt-4o
```

**3. 运行**

```bash
cargo run
```

```
phi> 现在几点了？
🔧 get_time
当前时间：2025-07-30 19:30:00

phi> /exit
```

### 源码解读

打开 `src/main.rs`，你会看到三部分：

**1. 定义工具** — 实现 `Tool` trait，告诉 Agent 这个工具叫什么、能干什么：

```rust
struct ClockTool;

#[async_trait]
impl Tool for ClockTool {
    fn name(&self) -> &'static str { "get_time" }

    fn description(&self) -> &'static str {
        "获取当前日期和时间"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn call(&self, _args: &Value, _ctx: &ToolContext) -> AgentResult<Vec<Content>> {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        Ok(vec![Content::text(format!("当前时间：{}", now))])
    }
}
```

**2. 注册工具** — 把工具挂到 Agent 上：

```rust
let agent = PhiAgent::build(
    base_agent_builder(llm)
        .system_prompt(build_system_prompt())
        .register_tool(ClockTool),      // ← 这里注册
    PhiAgentConfig { ... },
)?;
```

**3. REPL** — 交互对话，Agent 自动决定何时调用工具。

照着 `ClockTool` 写你自己的工具就行。[自定义工具](../tools/custom-tool.md) 里有更多示例。

## 方式二：库集成

生成单次调用版本（不含 REPL），适合嵌入已有项目。

**1. 创建项目**

```bash
phi init --lib my-agent
```

**2. 配置 API Key**

```bash
cd my-agent
cp .env.example .env
```

编辑 `.env`：

```
LLM_API_KEY=sk-your-key-here
LLM_BASE_URL=https://api.openai.com/v1
LLM_MODEL=gpt-4o
```

**3. 运行**

```bash
cargo run
```

### 源码区别

打开 `src/main.rs`，同样的 ClockTool，运行时变成单次调用：

```rust
// ClockTool 定义（同方式一）

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let api_key = std::env::var("LLM_API_KEY")?;
    let model = std::env::var("LLM_MODEL").unwrap_or_else(|_| "gpt-4o".into());
    let llm = Arc::new(OpenAiClient::new(api_key, model.clone(), std::env::var("LLM_BASE_URL").ok()));

    let agent = PhiAgent::build(
        base_agent_builder(llm)
            .system_prompt(build_system_prompt())
            .register_tool(ClockTool),
        PhiAgentConfig {
            model,
            enable_thinking: true,
            thinking_budget: None,
            thinking_effort: ReasoningEffort::Medium,
            safety: SafetyConfig::default(),
        },
    )?;

    let session = agent.create_session().await;
    let mut renderer = create_stdout_renderer(&OutputFormat::Terminal {
        show_thinking: true, show_tool_args: true, color: true,
    });
    agent.run_turn(session, "现在几点了？", |event| renderer.render(event)).await?;
    Ok(())
}
```

和方式一的区别：没有 `rustyline`，没有 REPL 循环，直接调用一次 `run_turn`。
[自定义工具](../tools/custom-tool.md) 里有更多示例。

## 示例

仓库包含 17 个可运行示例，其中 3 个无需 API Key：

```bash
# 无需 API Key — 使用 Mock LLM
cargo run --example custom_policy    # ToolPolicy + Middleware + 事件钩子
cargo run --example session_persist  # 会话生命周期与文件锁
cargo run --example event_log        # 每轮事件 JSONL 持久化
```

→ [完整示例列表](https://github.com/hibuka-labs/phi-agent#examples)（含全部 17 个示例及 API Key 要求）。
更多示例参见 [自定义工具](../tools/custom-tool.md)。