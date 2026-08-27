# API 参考

phi-agent 的 API 文档从源码注释自动生成，托管在 [docs.rs](https://docs.rs/phi-agent)。

## 核心模块

| 模块 | 描述 |
|------|------|
| `agent` | `PhiAgent`、`PhiAgentConfig`、`base_agent_builder()` 工厂 |
| `bridge` | 外部集成桥接协议（Tauri、Web） |
| `config` | LLM 配置解析 (CLI > env > .env > 默认值) |
| `prompt` | 系统提示词构建 (`build_system_prompt`、`build_system_prompt_cn`) |
| `render` | `EventRenderer` trait + `TerminalRenderer`、`JsonStreamRenderer`、`NullRenderer` |
| `session` | 会话管理 (ID、目录、文件锁、清理) |
| `cli` | `AutoApprovalHandler` (自动 / 全部拒绝) |
| `event_log` | 事件 → JSONL 持久化 |

## agent-base 重导出

phi-agent 重导出了 [`agent-base`](https://docs.rs/agent-base) 的关键类型：

| 分类 | 类型 |
|------|------|
| **核心** | `AgentResult`、`AgentError`、`Tool`、`ToolContext`、`Content`、`ToolMetadata` |
| **工具策略** | `ToolPolicy`、`ToolDecision`（Proceed / Block / Modify）、`DenyAllToolPolicy` |
| **工具可见性** | `ToolExposure`（Direct / Deferred / Hidden）、`ActivationContext` |
| **LLM** | `ReasoningConfig`、`ReasoningEffort`、`LlmProvider`（via `llm-trait`） |
| **配置** | `AgentConfig`、`SafetyConfig`、`SessionConfig`、`Language` |
| **事件** | `RuntimeEvent`、`UserEvent`、`FinishReason` |
| **会话** | `SessionId`、`TurnContext` |
| **计划** | `PlanItem`、`PlanStepStatus`、`UpdatePlanTool` |
| **流水线** | `DefaultPipeline`、`ToolExecutionPipeline` |

## agent-works 重导出

phi-agent 重导出了 [`agent-works`](https://docs.rs/agent-works) 的关键类型：

| 分类 | 类型 |
|------|------|
| **Focus** | `Focus`、`FocusContext`、`FocusInput`、`FocusOutput`、`FocusError` |
| **Prompt Fragments** | `PromptFragment`、`FragmentContext`、`compose_fragments`、`DynamicToolsFragment`、`EnvironmentFragment` |
| **上下文压缩** | `CompressionMiddleware`、`clear_compression_cache`、`run_compact_session`（`compression` feature） |
| **MCP** | `McpServer`、MCP 连接类型（`mcp` feature） |
| **多 Agent** | `MultiAgentConfig`、`ChildPermissionMode`（`multi-agent` feature） |

→ Focus 使用示例请参见 [Focus 指南](guide/concepts/focus.md)。
→ ToolPolicy、压缩、PromptFragment 示例请参见 [高级用法](guide/advanced/advanced.md)。

---

→ [完整 API 文档 (docs.rs)](https://docs.rs/phi-agent)
