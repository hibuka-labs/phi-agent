---
description: "phi-agent 浏览器自动化：基于 Chrome DevTools Protocol 的 21 个 CDP 工具，由 browser feature flag 控制。"
---

# 浏览器自动化

> ⚠️ **注意：浏览器工具是扩展工具（非内核工具），默认关闭。** 需要显式启用 `browser` feature 才能使用。

## 启用方式

```bash
# cargo add
cargo add phi-agent --features browser

# 或配置 Cargo.toml
[dependencies]
phi-agent = { version = "0.9", features = ["browser"] }

# 命令行编译
cargo run --features browser
```

---

phi-agent 提供可选的浏览器自动化能力，基于 Chrome DevTools Protocol (CDP)。21 个工具覆盖导航、交互、内容提取和标签页管理 — 全部由 `browser` Cargo feature 控制。

## 快速开始

```bash
# 编译并启用浏览器功能
cargo run --features browser -- --enable-browser "上网查今天天气"

# Headed 模式（可见浏览器窗口，便于调试）
cargo run --features browser -- --enable-browser --headed "浏览 example.com"

# 连接已有的 Chrome 实例
# 首先启动 Chrome 并开启远程调试：
#   /Applications/Google\ Chrome.app/Contents/MacOS/Google\ Chrome --remote-debugging-port=9222
cargo run --features browser -- --connect-ws ws://localhost:9222 "在当前页面查找..."
```

## 工作原理

1. `--enable-browser` 启动无头 Chrome 实例（或通过 `--connect-ws` 连接已有实例）
2. `browser_navigate` 打开网页并返回 ARIA 无障碍快照，可交互元素带有数字索引
3. Agent 通过索引点击元素（如 `browser_click index=5`），无需编写脆弱的 CSS 选择器
4. `browser_screenshot` 截取页面截图；`browser_get_markdown` 提取可读内容

## 工具分类

| 类别 | 工具 |
|------|------|
| **导航** | `browser_navigate`, `browser_go_back`, `browser_go_forward`, `browser_wait` |
| **交互** | `browser_click`, `browser_hover`, `browser_input_fill`, `browser_select`, `browser_press_key`, `browser_scroll` |
| **查看** | `browser_snapshot`, `browser_screenshot`, `browser_get_markdown`, `browser_read_links`, `browser_evaluate` |
| **标签页管理** | `browser_new_tab`, `browser_tab_list`, `browser_switch_tab`, `browser_close_tab` |
| **控制** | `browser_close`, `browser_extract_content` |

## 环境要求

- 需安装 Chrome 或 Chromium
- 编译需带 `cargo run --features browser`（`browser` feature 控制 `headless_chrome` 等重量级依赖）

## Feature Gate

浏览器工具按需引入。不启用 `browser` feature 时，不会编译任何浏览器相关代码：

```toml
[dependencies]
phi-agent = { version = "0.9", features = ["browser"] }
```
