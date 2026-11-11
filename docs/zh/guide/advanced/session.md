---
description: "phi-agent 会话与快照：持久化对话状态、跟踪工作区，随时保存和恢复长时间运行的 Agent 工作。"
---

# 会话与快照

phi-agent 通过会话管理对话状态，并支持创建/恢复快照以处理长时间运行的工作。

## 会话

每个对话轮次在会话中进行跟踪。会话提供：

- **隔离性** — 每个会话有独立的对话历史
- **持久化** — 事件日志以 JSONL 格式写入 `~/.phi-agent/sessions/<id>/`
- **文件锁** — 基于文件的锁防止并发修改
- **自动清理** — 过期会话自动移除

### 会话生命周期

```
create_session() → run_turn() → run_turn() → ... → 会话过期 → 清理
```

会话在首次使用时自动创建，过期后清理（默认最多 50 个会话，可在 `base_agent_builder` 中配置）。

### 事件日志

每个轮次以 JSONL 格式持久化在会话目录中：

```
~/.phi-agent/sessions/<session-id>/
  turn-001.jsonl
  turn-002.jsonl
  turn-003.jsonl
```

每行是一个 JSON 对象，代表一个事件（文本增量、工具调用开始、工具调用结果等）。

## 快照

快照捕获某个时间点的完整对话状态。适用场景：

- 在风险操作前保存进度
- 为长时间运行的任务创建检查点
- 分享对话状态用于调试

### REPL 命令

| 命令 | 说明 |
|------|------|
| `/snapshot <name>` | 创建当前会话的快照 |
| `/snapshots` | 列出所有快照（按日期倒序） |
| `/session` | 显示当前会话 ID 和元数据 |
| `/events` | 显示当前轮次的最近事件 |
| `/tools` | 列出已注册的工具 |

### 编程接口

```rust
use phi_agent::SessionContext;

let ctx = SessionContext::new(&session_id);

// 创建快照
create_snapshot(&ctx, "before-refactor").await?;

// 列出快照
let snapshots = list_snapshots(&ctx).await?;
for snap in &snapshots {
    println!("{} - {}", snap.name, snap.created_at);
}

// 恢复快照
restore_snapshot(&ctx, "before-refactor").await?;

// 删除快照
delete_snapshot(&ctx, "before-refactor").await?;
```

### 快照存储

快照与会话数据一起存放：

```
~/.phi-agent/sessions/<session-id>/
  snapshots/
    before-refactor.json
    after-migration.json
```

## 校验

会话 ID 和快照名称经过校验：

- 会话 ID：字母数字 + 连字符 + 下划线，最长 64 字符
- 快照名称：字母数字 + 连字符 + 下划线，最长 128 字符

## 另见

- [超时与恢复语义](timeout-recovery.md) —— 恢复后的会话对被中断的工具调用知道什么，
  以及为什么恢复绝不重新执行动作。
