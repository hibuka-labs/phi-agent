---
description: "Browser automation for phi-agent via the Chrome DevTools Protocol: 21 CDP tools behind the browser feature flag for web testing and scraping."
---

# Browser Automation

> ⚠️ **Note: Browser tools are extension tools (not kernel tools), off by default.** Enable the `browser` feature explicitly to use them.

## Enabling

```bash
# cargo add
cargo add phi-agent --features browser

# Or in Cargo.toml
[dependencies]
phi-agent = { version = "0.9", features = ["browser"] }

# Command line
cargo run --features browser
```

---

phi-agent includes optional browser automation via Chrome DevTools Protocol (CDP). 21 tools cover navigation, interaction, content extraction, and tab management — all gated behind the `browser` Cargo feature.

## Quick start

```bash
# Build and run with browser enabled
cargo run --features browser -- --enable-browser "search for today's weather"

# Headed mode (visible browser window, useful for debugging)
cargo run --features browser -- --enable-browser --headed "browse example.com"

# Connect to an existing Chrome instance
# First, start Chrome with remote debugging:
#   /Applications/Google\ Chrome.app/Contents/MacOS/Google\ Chrome --remote-debugging-port=9222
cargo run --features browser -- --connect-ws ws://localhost:9222 "find something..."
```

## How it works

1. `--enable-browser` launches a headless Chrome instance (or connects to an existing one via `--connect-ws`)
2. `browser_navigate` opens a URL and returns an ARIA accessibility snapshot — interactive elements get numbered indices
3. The agent clicks elements by index (e.g., `browser_click index=5`) — no fragile CSS selectors needed
4. `browser_screenshot` captures visual snapshots; `browser_get_markdown` extracts readable content

## Tool categories

| Category | Tools |
|----------|-------|
| **Navigation** | `browser_navigate`, `browser_go_back`, `browser_go_forward`, `browser_wait` |
| **Interaction** | `browser_click`, `browser_hover`, `browser_input_fill`, `browser_select`, `browser_press_key`, `browser_scroll` |
| **Viewing** | `browser_snapshot`, `browser_screenshot`, `browser_get_markdown`, `browser_read_links`, `browser_evaluate` |
| **Tab Management** | `browser_new_tab`, `browser_tab_list`, `browser_switch_tab`, `browser_close_tab` |
| **Control** | `browser_close`, `browser_extract_content` |

## Requirements

- Chrome or Chromium installed
- Build with `cargo run --features browser` (the `browser` feature gates the `headless_chrome` dependency)

## Feature gate

Browser tools are opt-in. Without the `browser` feature, no browser-related code is compiled:

```toml
[dependencies]
phi-agent = { version = "0.9", features = ["browser"] }
```
