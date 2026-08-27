# 高级用法

Middleware、会话管理、事件日志等进阶功能。

## Middleware（中间件）

Middleware 在 LLM 调用前后介入 Agent 循环：

```rust
use agent_base::{TurnFactMiddleware, TurnToolLimitMiddleware};

let builder = base_agent_builder(llm_client)
    .system_prompt(system_prompt)
    .middleware(TurnFactMiddleware::new())
    .middleware(TurnToolLimitMiddleware::from_config(&safety));
```

内置中间件：
- `TurnFactMiddleware` — 在每轮开始时注入事实/上下文
- `TurnToolLimitMiddleware` — 强制执行 `max_tool_calls_per_turn` 限制

### 自定义 Middleware

实现 `Middleware` trait 可以在 Agent 循环的三个节点介入：

```rust
use phi_agent::{AgentResult, Middleware, PreLlmCtx, PostLlmCtx, UserMessageCtx};
use async_trait::async_trait;

struct LoggingMiddleware;

#[async_trait]
impl Middleware for LoggingMiddleware {
    // 1. 用户发送消息时调用（最先触发）
    async fn on_user_message(&self, ctx: &mut UserMessageCtx) -> AgentResult<()> {
        tracing::info!(session = ?ctx.session_id, input = %ctx.user_input, "收到用户消息");
        Ok(())
    }

    // 2. LLM 调用前调用（可修改消息列表或工具列表）
    async fn on_pre_llm(&self, ctx: &mut PreLlmCtx) -> AgentResult<()> {
        tracing::info!(session = ?ctx.session_id, msg_count = ctx.messages.len(), "准备调用 LLM");
        Ok(())
    }

    // 3. LLM 响应后调用（可拦截输出、注入追问）
    async fn on_post_llm(&self, ctx: &mut PostLlmCtx) -> AgentResult<()> {
        tracing::info!(
            session = ?ctx.session_id,
            is_tool_call = ctx.is_tool_call,
            tool_count = ctx.tool_calls.len(),
            "LLM 响应完成"
        );
        Ok(())
    }
}

builder = builder.middleware(LoggingMiddleware);
```

`PostLlmCtx` 关键字段：

| 字段 | 类型 | 说明 |
|-------|------|------|
| `full_text` | `String` | LLM 的文本响应（纯工具调用时为空） |
| `is_tool_call` | `bool` | LLM 是否请求了工具调用 |
| `tool_calls` | `Vec<(id, name, args)>` | 解析后的工具调用列表 |
| `available_tools` | `Vec<String>` | 当前注册的工具名称列表 |
| `total_tool_calls` | `usize` | 本轮已执行的工具调用总数 |
| `skip_push` | `bool` | 设为 `true` 可阻止当前响应写入会话历史 |
| `follow_up_message` | `Option<String>` | 注入一条追问消息到 Agent 循环中 |

### 自定义 ToolPolicy

实现 `ToolPolicy` trait 可以控制工具的执行行为——审批、执行前检查、执行后审计：

```rust
use phi_agent::{AgentResult, ApprovalRequest, Content, RiskLevel, ToolContext, ToolPolicy};
use agent_base::ToolDecision;
use async_trait::async_trait;
use serde_json::Value;

struct RiskAwarePolicy;

#[async_trait]
impl ToolPolicy for RiskAwarePolicy {
    // 1. 判断工具调用是否需要用户审批（异步）
    async fn evaluate_approval(&self, tool_name: &str, args: &Value) -> Option<ApprovalRequest> {
        let command = args.get("command").and_then(Value::as_str).unwrap_or("");
        if command.contains("rm ") || command.contains("sudo") {
            return Some(ApprovalRequest {
                title: "危险命令".into(),
                message: format!("AI 准备执行：{command}"),
                action_key: Some(format!("cmd:{command}")),
                risk_level: RiskLevel::Destructive,
                raw: Some(args.clone()),
            });
        }
        None // 安全命令免审批
    }

    // 2. 工具执行前的同步检查——返回 ToolDecision
    fn before_call(
        &self, tool_name: &str, args: &Value, _ctx: &ToolContext,
    ) -> AgentResult<ToolDecision> {
        tracing::info!("即将执行工具：{tool_name}");
        // Proceed 用原始参数执行，Block 中断，Modify 替换参数
        Ok(ToolDecision::Proceed)
    }

    // 3. 工具执行成功后的同步回调——用于审计、埋点
    fn after_call(
        &self, tool_name: &str, _args: &Value, result: &[Content], _ctx: &ToolContext,
    ) -> AgentResult<()> {
        tracing::info!(tool = tool_name, content_count = result.len(), "工具执行完成");
        Ok(())
    }
}

builder = builder.tool_policy(Arc::new(RiskAwarePolicy));
```

执行流程：`evaluate_approval` → （如需审批则等待用户）→ `before_call` → `tool.call()` → `after_call`。

`before_call` 返回 `ToolDecision`：

| 变体 | 效果 |
|------|------|
| `ToolDecision::Proceed` | 使用原始参数执行 |
| `ToolDecision::Block(msg)` | 中断调用；`msg` 会发送给 LLM |
| `ToolDecision::Modify(new_args)` | 使用替换参数执行 |

`Modify` 适用于自动注入标志（如 `--no-color`）、路径规范化、或在执行前清理输入。

> 💡 完整可运行示例：[`examples/tools/custom_policy.rs`](https://github.com/hibuka-labs/phi-agent/blob/master/examples/tools/custom_policy.rs)，同时演示了自定义 Middleware 和 ToolPolicy。执行 `cargo run --example custom_policy` 即可运行，无需 API key。

## 审批处理器

控制哪些工具调用需要人工确认：

```rust
// 全部自动批准（CI / 自动化场景）
use phi_agent::{AutoApprovalHandler, ApprovalMode};
builder = builder.approval_handler(Arc::new(
    AutoApprovalHandler::new(ApprovalMode::Auto)
));

// 全部拒绝（只读 / 预览模式）
builder = builder.approval_handler(Arc::new(
    AutoApprovalHandler::new(ApprovalMode::DenyAll)
));
```

交互式 CLI 审批参见 phi 二进制中的 `CliApprovalHandler`。

## 会话管理

会话用于持久化对话历史和工具调用结果：

```rust
use phi_agent::session::{resolve_session, cleanup_expired_sessions};

// 创建或复用会话
let ctx = resolve_session(Some("my-session"), &base_dir)?;
println!("Session: {} (new: {})", ctx.session_id, ctx.is_new_session);

// 清理过期会话（> 7 天）
let cleaned = cleanup_expired_sessions(&base_dir, 7)?;
println!("Cleaned {} expired sessions", cleaned);
```

会话目录结构：
```
~/.phi-agent/sessions/<id>/
├── session_id           # 会话 ID 标记
├── session.lock         # 独占文件锁
├── session_meta.json    # 创建时间、最后活跃时间
└── turn_001.jsonl       # 每轮事件日志（JSONL）
```

## 事件日志

每轮对话都以 JSONL 格式保存，方便回放和分析：

```rust
use phi_agent::{save_turn_log, event_to_jsonl};

// 保存本轮事件
save_turn_log(&session_ctx, 1, &events, "用户查询内容")?;

// 将单个事件转为 JSONL 行
let line = event_to_jsonl(&event);
```

日志中的事件类型：
- `thought_delta` — LLM 思维过程内容
- `text_delta` — 助手文本输出
- `tool_call_started` / `tool_call_finished` — 工具调用
- `approval_request` — 需要审批的工具调用
- `plan_updated` — 任务计划更新
- `turn_finished` — 轮次汇总（包含耗时和统计信息）

## 系统提示词

phi-agent 提供两种系统提示词变体：

```rust
use phi_agent::{build_system_prompt, build_system_prompt_cn};

// 默认（国际版）
let prompt = build_system_prompt();

// 中国网络环境适配版（优先国内服务，处理 GFW）
let prompt_cn = build_system_prompt_cn();
```

你也可以通过 `builder.system_prompt(...)` 传入完全自定义的提示词。

## 推理 / 思考

控制 LLM 的思维链行为：

```rust
use agent_base::{ReasoningConfig, ReasoningEffort};

// Builder 级别的默认值
builder = builder.reasoning(ReasoningConfig {
    effort: Some(ReasoningEffort::High),
    ..Default::default()
});

// 单轮覆盖
agent.set_reasoning_effort(ReasoningEffort::XHigh).await;
```

推理强度级别及适用场景：
- `Low` — 简单任务，快速响应
- `Medium` — 默认，平衡
- `High` — 复杂的多步骤任务
- `XHigh` — 最困难的问题，最长思考时间

## 编程式使用 Renderer

在 CLI 之外使用渲染器：

```rust
use phi_agent::{
    TerminalRenderer, JsonStreamRenderer, NullRenderer, EventRenderer,
};
use std::io;

// 终端渲染
let mut renderer = TerminalRenderer::new(true, true, true, Box::new(io::stdout()));

// JSON 流渲染（适用于 IDE 集成）
let mut renderer = JsonStreamRenderer::stdout();

// 静默渲染（适用于 Web 后端）
let mut renderer = NullRenderer;
```

## 错误恢复

phi-agent 默认配置了连续失败恢复机制：

```rust
use agent_base::ConsecutiveFailureRecovery;

// 连续 3 次失败 → 停止并说明原因
builder = builder.error_recovery(Arc::new(
    ConsecutiveFailureRecovery::new(3)
));
```

## 并行工具执行

当 LLM 在单轮返回多个工具调用时，它们会通过 `join_all` **并发执行**。延迟从 `sum(各工具耗时)` 降低为 `max(各工具耗时)`。

- 审批阶段顺序执行（批量审批所有调用）
- 审批通过后所有工具并行执行
- 单个工具失败**不会**中断其他工具——失败记录在 `failures` 中

无需配置——并行执行是默认行为。

## 上下文压缩

对于接近上下文窗口限制的长对话，可启用压缩：

```rust
use phi_agent::CompressionMiddleware;

builder = builder.middleware(CompressionMiddleware::new());
```

当上下文窗口满时，框架会自动压缩对话历史并继续循环——无需重启。

CLI 用户也可以用 `/compact` 手动压缩。

## Prompt Fragments（可组合提示词）

可以用独立的 Fragment 组合系统提示词，而非单一硬编码：

```rust
use agent_base::PromptFragment;
use agent_base::FragmentContext;

struct MyPersonalityFragment;

impl PromptFragment for MyPersonalityFragment {
    fn name(&self) -> &str { "personality" }
    fn priority(&self) -> i32 { 10 } // 越小越靠前
    fn render(&self, _ctx: &FragmentContext) -> Option<String> {
        Some("你是一个精通 Rust 的助手。".into())
    }
}
```

Fragment 按 `priority` 排序后拼接。内置的 `DynamicToolsFragment` 会自动将已注册工具的描述注入提示词。

## 延伸阅读

无需 API Key 的示例 — 可直接运行：

```bash
cargo run --example custom_policy    # ToolPolicy + Middleware + 事件钩子
cargo run --example session_persist  # 会话生命周期与文件锁
cargo run --example event_log        # 每轮事件 JSONL 持久化
```

→ [完整示例列表](https://github.com/hibuka-labs/phi-agent#examples) — 全部 17 个示例及 API Key 要求。
