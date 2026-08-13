# API 参考

phi-agent 的 API 文档从源码注释自动生成，托管在 [docs.rs](https://docs.rs/phi-agent)。

## 核心模块

| 模块 | 描述 |
|--------|-------------|
| `agent` | `PhiAgent`、`PhiAgentConfig`、`base_agent_builder()` 工厂 |
| `config` | LLM 配置解析 (CLI > env > .env > 默认值) |
| `prompt` | 系统提示词构建 (`build_system_prompt`、`build_system_prompt_cn`) |
| `render` | `EventRenderer` trait + `TerminalRenderer`、`JsonStreamRenderer`、`NullRenderer` |
| `session` | 会话管理 (ID、目录、文件锁、清理) |
| `cli` | `AutoApprovalHandler` (自动 / 全部拒绝) |
| `event_log` | 事件 → JSONL 持久化 |

## agent-base 重导出

phi-agent 重导出了 [`agent-base`](https://docs.rs/agent-base) 的关键类型：

`AgentResult`、`Tool`、`ToolContext`、`Content`、`OpenAiClient`、`ReasoningEffort`、`SafetyConfig`、`OutputFormat` 等。

## agent-works 重导出

phi-agent 重导出了 [`agent-works`](https://docs.rs/agent-works) 的关键类型：

`Focus`、`FocusContext`、`FocusInput`、`FocusOutput`、`FocusError`。

→ 使用示例请参见 [Focus 指南](./guide/focus.html)。

---

→ [完整 API 文档 (docs.rs)](https://docs.rs/phi-agent)
