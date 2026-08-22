# 回路守卫（React Loop Guard）

> LLM 不是完美的。它会沉默、敷衍、自说自话。回路守卫让 agent 在这些情况下自己站起来，而不是原地打转或草草收场。

## 什么是 React Loop

ReAct（**Re**asoning + **Act**ing）是 agent 的核心工作模式：LLM 思考 → 调用工具 → 观察结果 → 再思考，循环往复直到任务完成。这个「思考-行动」的循环就是 React Loop。

回路守卫守护的就是这个循环——当 LLM 在循环中出现异常时，自动检测并处理，不让 agent 死循环或草率结束。

## 解决什么问题

Agent 循环依赖 LLM 持续输出有效响应。但现实中 LLM 会：

- **空转** — 只输出内部推理，不调工具也不给答案，像在想但永远不动手
- **沉默** — 完全空回复，什么都不说
- **提前收工** — 调了一堆工具，然后用一句话敷衍了事，任务没完成就停了

没有 Guard 时，这些情况要么导致无限循环（烧 token），要么导致任务静默失败（你以为做完了其实没有）。

我们在大量垂直领域 agent 的落地实践中反复遇到这些问题，最终形成了这套系统化的防御机制——不是靠单一规则硬拦，而是分层、可配置、可扩展的 trait 架构。

## 四种防御

回路守卫在每次 LLM 返回后自动检测异常，不需要你手动检查：

| 异常 | 发生了什么 | 回路守卫怎么做 |
|------|-----------|-------------|
| **reasoning-only** | LLM 一直在想，不动手 | 注入 nudge 催它做决定，3 次后强制停止 |
| **empty response** | LLM 什么都没说 | 注入 nudge 要求重试，3 次后强制停止 |
| **text-only** | LLM 给了纯文本回答 | 直接结束，信任模型 |
| **text-only after tools** | 调了工具后给个简短回答 | **LLM judge 介入验证** — 任务真的完成了吗？ |

前三个是基本防护。第四个是核心——**judge 机制**。

## Judge：用 LLM 监督 LLM

当 agent 调完工具返回纯文本时，回路守卫不直接信任它，而是调用另一个 LLM 做裁判：

```
用户问题 + agent 回复 → Judge → {"done": true/false, "reason": "..."}
```

- Judge 说完成了 → 结束
- Judge 说没完成 → 注入理由，让 agent 继续干活

这个过程对用户完全透明，不增加额外配置。Judge 本身也做了精心优化：短回复自动检测、长回复跳过验证、大输入兜底放行——在安全性和 token 效率之间取得平衡。

## 配置

```rust
use agent_works::guard::{DefaultGuard, DefaultGuardConfig};

// 开箱即用（推荐）
let guard = DefaultGuard::with_llm_client(
    DefaultGuardConfig::default(),
    llm_client,
);
builder = builder.guard(guard);
```

| 参数 | 默认 | 调优建议 |
|------|------|---------|
| `use_llm_judge` | `true` | 关闭后 text-only after tools 直接结束，省 token 但风险高 |
| `judge_skip_threshold` | `256` | 回复足够长就跳过 judge。调高 = 更严格，调低 = 更省 |
| `judge_fail_open` | `false` | `false` = 宁可多跑一轮；`true` = judge 挂了就信模型 |
| `judge_timeout_secs` | `10` | judge 超时上限 |
| `detect_short_response` | `true` | 长问题 + 短回答 = 可能没完成，自动 nudge |
| `reasoning_only_max_strikes` | `3` | 空转几次后放弃 |
| `empty_response_max_strikes` | `3` | 沉默几次后放弃 |

## 容错：fail-closed vs fail-open

Judge 默认 **fail-closed**：judge 超时或出错时，不信任模型，强制 agent 继续。

这是从实际生产中总结的经验——长时间运行的任务，提前收工的代价远大于多跑一轮。宁可多花几秒确认，也别漏了活。

简单任务或 judge 不稳定时可以改为 fail-open：

```rust
let config = DefaultGuardConfig {
    judge_fail_open: true,
    ..DefaultGuardConfig::default()
};
```

## 禁用 Judge

不传入 LLM client，judge 自动禁用。适合对延迟敏感、宁可冒风险也要快的场景：

```rust
let guard = DefaultGuard::new(DefaultGuardConfig::default());
```

## 扩展

回路守卫基于 trait 设计，支持完全自定义或装饰器增强。详见 [agent-base 设计文档](https://github.com/hibuka-labs/agent-base)。
