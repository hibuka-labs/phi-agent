# API 参考

phi-agent 的 API 文档从源码注释自动生成，托管在 [docs.rs](https://docs.rs/phi-agent)。

## 核心模块

| 模块 | 描述 |
|------|------|
| `agent` | `PhiAgent`、`PhiAgentConfig`、`base_agent_builder()` 工厂 |
| `config` | LLM 配置解析 (CLI > env > .env > 默认值) |
| `prompt` | 系统提示词构建 (`build_system_prompt`、`build_system_prompt_cn`) |
| `render` | `EventRenderer` trait + `TerminalRenderer`、`JsonStreamRenderer`、`NullRenderer` |
| `session` | 会话管理 (ID、目录、文件锁、清理) |
| `cli` | `AutoApprovalHandler`、`QueuedApprovalHandler` |
| `event_log` | 事件 → JSONL 持久化 |

## 构建工厂

| 函数 | 描述 |
|------|------|
| `base_agent_builder()` | 默认构建器，含压缩中间件 |
| `base_agent_builder_no_compression()` | 无压缩构建器（用于自定义上下文轮转） |
| `base_agent_builder_with_excludes()` | 跳过指定内核工具的构建器 |
| `base_agent_builder_with_options()` | 接受自定义 `CompressionConfig` 的构建器 |

## agent-base 重导出

phi-agent 重导出了 [`agent-base`](https://docs.rs/agent-base) 的关键类型：

| 分类 | 类型 |
|------|------|
| **核心** | `AgentResult`、`AgentError`、`AgentRuntime`、`Tool`、`ToolContext`、`Content`、`ToolMetadata`、`TypedTool` |
| **审批** | `ApprovalHandler`、`ApprovalDecision`、`ApprovalRequest`、`AllowAllApprovalHandler`、`DenyAllApprovalHandler` |
| **工具策略** | `ToolPolicy`、`ToolDecision`、`DenyAllToolPolicy`、`ToolRegistry` |
| **工具可见性** | `ToolExposure`、`ActivationContext` |
| **LLM** | `ReasoningConfig`、`ReasoningEffort`、`llm_trait`（模块：`LlmProvider`、`Protocol`、`config::LlmConfig`） |
| **配置** | `AgentConfig`、`SafetyConfig`、`SessionConfig`、`Language` |
| **事件** | `RuntimeEvent`、`UserEvent`、`FinishReason` |
| **会话** | `SessionId`、`TurnContext`、`ChatMessage` |
| **计划** | `PlanItem`、`PlanStepStatus`、`UpdatePlanTool` |
| **检查点** | `CheckpointData`、`CheckpointStep` |
| **恢复** | `ConsecutiveFailureRecovery`、`RetryOnError` |
| **上下文窗口** | `ContextCompaction`、`ContextWindowManager`、`estimate_messages_tokens`、`first_system_prompt` |
| **引擎** | `MaxTurnsNudgeConfig`、`MaxTurnsNudgeMiddleware` |
| **守卫** | `GuardCtx`、`GuardDecision`、`Middleware`、`NoopGuard` |
| **中间件** | `TurnFactMiddleware`、`TurnToolLimitMiddleware`、`PreLlmCtx`、`PostLlmCtx` |
| **流水线** | `DefaultPipeline`、`ToolExecutionPipeline` |

## agent-works 重导出

phi-agent 重导出了 [`agent-works`](https://docs.rs/agent-works) 的关键类型：

| 分类 | 类型 |
|------|------|
| **构建器** | `AgentBuilder` |
| **Focus** | `Focus`、`FocusContext`、`FocusInput`、`FocusOutput`、`FocusError` |
| **Prompt Fragments** | `PromptFragment`、`FragmentContext`、`compose_fragments`、`DynamicToolsFragment`、`EnvironmentFragment` |
| **守卫** | `DefaultGuard`、`DefaultGuardConfig`、`ReasoningOnlyAction` |
| **Token 预算** | `TokenBudgetAction`、`TokenBudgetConfig`、`TokenBudgetCore`、`TokenBudgetState`、`DEFAULT_SEED_MESSAGE`、`build_context_window_info`、`token_budget_base_overhead` |
| **上下文压缩** | `CompressionMiddleware`、`clear_compression_cache`、`run_compact_session`（`compression` feature） |
| **MCP** | `McpServer`、`McpServerConfig`、`McpServeConfig`（`mcp` feature） |
| **多 Agent** | `MultiAgentConfig`、`ChildPermissionMode`、`ChildReport`、`ChildResultEvent`、`ControlConfig` |
| **Fan-in** | `ChildResultRoute`、`ChildResultRouter`（`multi-agent` feature） |
| **注册表** | `AgentSnapshot`、`RegistrySnapshot`（`multi-agent` feature） |
| **Skills** | `Skill`、`PromptSkill`（`skill` feature） |

## llm-unified 重导出

| 函数 | 描述 |
|------|------|
| `create_provider()` | LLM 提供者工厂 — 根据 `llm_trait` 配置解析协议/客户端 |

## Feature 门控重导出

| Feature | 类型 |
|---------|------|
| `shell` | `LocalShellTool`（来自 `phi-kernel-tools`） |
| `skill` | `Skill`、`PromptSkill`（来自 `agent-works`） |
| `mcp` | `McpServer`、MCP 类型（来自 `agent-works`） |
| `multi-agent` | `ChildResultRoute`、`ChildResultRouter`、`AgentSnapshot`、`RegistrySnapshot` |
| `compression` | `CompressionMiddleware`、`clear_compression_cache`、`run_compact_session` |
| `telemetry` | 指标类型（来自 `phi-telemetry`） |
| `browser` | 21 个 CDP 浏览器自动化工具（来自 `phi-tools`） |

## CLI 类型

| 类型 | 描述 |
|------|------|
| `AutoApprovalHandler` | 自动 / 全部拒绝审批模式 |
| `QueuedApprovalHandler` | 将审批请求入队到 channel，供 UI 消费 |
| `ApprovalItem` | 单个排队的审批请求（含 oneshot 响应） |
| `ApprovalMode` | CLI 审批模式枚举 |

---

→ Focus 使用示例请参见 [Focus 指南](guide/concepts/focus.md)。
→ ToolPolicy、压缩、PromptFragment 示例请参见 [高级用法](guide/advanced/advanced.md)。
→ Fan-in 路由和子权限请参见 [多 Agent](guide/advanced/multi-agent.md)。

---

→ [完整 API 文档 (docs.rs)](https://docs.rs/phi-agent)
