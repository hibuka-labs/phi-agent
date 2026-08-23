# Skills（技能）

Skills 是以 markdown 文件定义的可复用 Agent 行为。phi-agent 遵循 [agentskills.io](https://agentskills.io) 开放标准 — 与 Claude Code、Codex 等主流 Agent 框架使用相同的格式。

## 为什么是 `.claude` 目录

Skills 规范由 Claude Code 率先提出，`.claude/skills/` 已成为社区约定的通用路径。phi-agent 沿用这一约定，而非自创 `.phi/skills/`——这样你在 Claude Code 中积累的技能，phi-agent 开箱即用，零迁移成本。

## 工作原理

Skills 使用 **prompt-injection** 模式：

1. 从 `.claude/skills/` 和 `~/.claude/skills/` 扫描技能定义
2. 技能列表（名称 + 描述）注入 system prompt
3. Agent 需要某个技能时，使用 `read_file` 加载完整的 `SKILL.md`
4. 没有专用的技能工具 — 只有文件 I/O + prompt 指令

这与 Claude Code / Codex 的模式一致：给 Agent 文件访问能力，技能就成为可发现的内容，而非硬编码的工具。

## 目录结构

```
.claude/skills/
  deploy/
    SKILL.md              # 必需：YAML frontmatter + markdown 正文
    scripts/              # 可选：可执行辅助脚本
    references/           # 可选：按需加载的参考文档
    templates/            # 可选：模板文件
  code-review/
    SKILL.md
    references/
      checklist.md
```

## SKILL.md 格式

```markdown
---
name: deploy
description: 将当前项目部署到生产环境
version: 1.0.0
author: team
tags: [deploy, ops]
allowed-tools: shell, read_file, write_file
user-invocable: true
disable-model-invocation: false
arguments: [branch, env]
---

# Deploy 技能

将项目部署到目标环境。

## 步骤

1. 读取当前分支：`read_file .git/HEAD`
2. 部署前运行测试
3. 执行部署脚本：`scripts/deploy.sh $branch $env`

## 脚本

- `scripts/deploy.sh` — 主部署脚本
- `scripts/rollback.sh` — 失败时回滚
```

## Frontmatter 字段

| 字段 | 必需 | 说明 |
|------|------|------|
| `name` | ✅ | kebab-case，≤64 字符，须与目录名一致 |
| `description` | ✅ | 一句话描述，用于 LLM 发现 |
| `version` | 否 | 语义版本号 |
| `author` | 否 | 作者名称 |
| `tags` | 否 | 逗号分隔的关键词 |
| `allowed-tools` | 否 | 技能激活时的工具白名单 |
| `disallowed-tools` | 否 | 技能激活时禁用的工具 |
| `model` | 否 | 模型覆盖（默认 `inherit`） |
| `user-invocable` | 否 | 用户能否通过 `/skill-name` 触发（默认 true） |
| `disable-model-invocation` | 否 | 禁止 LLM 自动触发（用于有副作用的技能） |
| `arguments` | 否 | 参数占位符，如 `[branch, env]` |

## 渐进式披露

Skills 使用三层模型以节省 token：

| 层级 | 内容 | 时机 |
|------|------|------|
| **发现层** | name + description（每技能约 50 tokens） | 始终在 system prompt 中 |
| **激活层** | SKILL.md 正文（建议 ≤5000 tokens） | Agent 读取文件时 |
| **执行层** | scripts/ + references/ | 技能执行时按需加载 |

## 用户级 vs 项目级

- `~/.claude/skills/` — 用户级，跨所有项目共享
- `.claude/skills/` — 项目级，随代码版本控制

名称冲突时以项目级优先。

## 创建技能

```bash
# 脚手架新建技能
phi skill init my-skill

# 生成结果：
# .claude/skills/my-skill/
#   SKILL.md
```

然后编辑 `SKILL.md` 填入技能指令和辅助脚本。
