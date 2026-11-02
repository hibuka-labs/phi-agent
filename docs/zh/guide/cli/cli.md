---
description: "phi 命令行工具：REPL 交互、单次执行、项目脚手架三种模式，一个二进制在终端运行 Rust 智能体。"
---

# CLI 使用

`phi` 命令行工具支持三种模式：REPL 交互、单次执行、项目脚手架。

## 安装

```bash
cargo install phi-agent
```

## 创建项目

```bash
phi init my-agent          # REPL 交互，带 ClockTool 示例
phi init --lib my-agent    # 单次调用，适合库集成
```

## REPL 模式

```bash
phi
# phi> 输入问题，回车发送
# phi> /exit 退出
```

## 单次执行

```bash
phi "现在几点了？"
phi "列出所有 .rs 文件" --model gpt-4o
phi "查看架构" --format json | jq '.type'
```

## 查看观测数据

```bash
phi metrics list              # 所有会话列表
phi metrics show <session-id> # 指定会话详情
phi metrics last              # 最近一次会话
```

## CLI 参数

| 参数 | 说明 | 默认值 |
|------|------|--------|
| `QUERY` | 位置参数 — 单次执行的问题 | — |
| `-m, --model` | 覆盖模型名称 | — |
| `--base-url` | 覆盖 API 基础 URL | — |
| `-s, --session-id` | 会话 ID（用于恢复） | — |
| `--format` | 输出格式：`terminal` / `json` / `quiet` | `terminal` |
| `--thinking-effort` | 思考强度：`low` / `medium` / `high` / `xhigh` | `medium` |
| `--thinking-budget` | 思考 token 上限 | — |
| `--no-thinking` | 禁用思考 | — |
| `--no-tool-args` | 隐藏工具参数 | — |
| `--no-color` | 禁用终端颜色 | — |
| `-y, --auto-approve` | 自动批准所有操作 | — |
| `--shell-timeout-ms` | Shell 命令超时（毫秒） | `30000` |
| `--log-dir` | 日志目录 | `~/.phi-agent` |
| `--log-level` | 日志级别 | `info` |
| `--no-log` | 禁用文件日志 | — |
| `--max-tool-calls` | 每轮工具调用上限 | — |
| `--max-failures` | 连续失败上限 | — |

## 会话持久化

会话自动保存到 `~/.phi-agent/sessions/<id>/`。使用 `--session-id` 恢复：

```bash
phi --session-id 20250728_a1b2c3d4
```

超过 7 天不活跃的会话会自动清理。