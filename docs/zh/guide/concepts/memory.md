---
description: "phi-agent 的文件式记忆：Agent 以 markdown 在轮次和会话之间持久化信息，无向量库、无隐藏状态，透明可调试。"
---

# 记忆

phi-agent 支持基于文件系统的记忆功能 — Agent 可以在对话轮次和会话之间持久化信息，通过读写 markdown 文件实现。

## 工作原理

记忆采用 **prompt-injection** 模式（与 Claude Code Memory 相同）：

1. 记忆指令注入 system prompt
2. Agent 读写 `.phi/memory/` 目录下的 `.md` 文件
3. 没有专用的 `remember`/`recall`/`forget` 工具 — Agent 使用标准的 `read_file` 和 `write_file` 内核工具

这样保持了工具面的精简和可预测性。Agent 自行决定何时记住、何时回想，就像它决定何时读取其他文件一样。

## 记忆文件

记忆文件存放在项目目录下：

```
.phi/memory/
  user-preferences.md
  project-context.md
  decisions.md
```

每个文件是带可选 YAML frontmatter 的 markdown 文件：

```markdown
---
name: my-memory
description: 这段记忆的内容
---

实际的记忆内容...
```

## 使用方式

记忆功能默认开启（需要 `file` feature）。system prompt 会指示 Agent：

- **行动前**：检查 `.phi/memory/` 中是否有相关上下文
- **重要决策后**：写入记忆文件以持久化上下文
- **用户要求时**：响应"记住 X"或"你了解 Y 的哪些信息？"

## 模板

phi-tools 提供了常用场景的预制记忆模板：

| 模板 | 用途 |
|------|------|
| `user-preferences` | 用户的偏好、风格、约定 |
| `project-context` | 项目架构、技术栈、关键决策 |
| `session-notes` | 当前工作会话的笔记 |

使用 `phi memory init` 初始化 `.phi/memory/` 目录并生成模板。

## 记忆不是什么

- **不是向量数据库** — 没有 embedding，没有语义搜索。Agent 直接读写 `.phi/memory/` 下的 markdown 文件，无需向量数据库。
- **不是无差别记录** — Agent 会智能判断哪些信息值得记住，不会什么都写进去。你也可以明确要求它记住或回想特定内容。
- **不是隐藏状态** — 所有记忆文件是项目中的明文 markdown。你可以随时阅读、编辑或删除。

需要 RAG 或语义记忆时，自行接入向量数据库（Qdrant、pgvector、LanceDB）并注册为工具。
