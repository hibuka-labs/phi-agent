# Changelog

All notable changes to phi-agent will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.11.1] - 2026-08-17

### Added
- `base_agent_builder_with_excludes()` — builder variant that skips specific kernel tools (e.g. when an app re-registers its own `update_plan`).
- `update_plan` tool is default-registered in `base_agent_builder()`.
- Session turn-number resume for continued conversations.
- Re-export `agent-works` multi-agent types (`ChildPermissionMode`, `MultiAgentConfig`).

### Changed
- Skills directory moves from `.phi/skills` to `.claude/skills`, aligning with the Claude Code directory layout.
- Bump dependencies to `agent-works` 0.3.0 and `phi-kernel-tools` 0.3.0 (child permission modes, `denied_tools`, file-tool path sandbox removal).

## [0.11.0] - 2026-08-14

### Changed
- **Migrate to agent-base 0.2.0 Tool API** (breaking): tools now implement `description()` + `schema()` instead of `definition()`; tool results are `Vec<Content>` instead of `ToolOutput`; `ToolControlFlow` is replaced by `Content`.
- **Sub-agent events render via `agent_id`** (breaking): `SubAgentEvent` is removed — sub-agent streaming events reuse the parent event type, keyed by `agent_id`.
- Bump dependencies to their 0.2.0 releases: `agent-base`, `agent-works`, `phi-kernel-tools`, `phi-tools`, `phi-telemetry`.

### Added
- `denied` flag surfaced in `tool_call_finished` JSON output.
- Coverage backfill for factory/server/prompt, plus an `llvm-cov` CI gate.

### Removed
- `ToolControlFlow` / `ToolOutput` re-exports (superseded by `Content`).

## [0.10.1] - 2026-08-13

### Added
- **`edit_file` kernel tool wired into `base_agent_builder()`** — phi-kernel-tools already shipped `edit_file` (precision text replacement with uniqueness checks, overlap detection, atomic writes, and CRLF/LF preservation), but it wasn't registered. It's now available by default with the `file` feature, alongside `read_file`/`write_file`/`list_files`.

### Changed
- Sync EN/ZH file-tools docs: document `edit_file`, fix the truncation description, and add the `full` feature.
- Bump version to 0.10.1

## [0.9.1] - 2026-08-11

### Added
- **`phi serve --bridge`** — re-enables the legacy NDJSON bridge protocol for SDK consumption (Python, Node.js). The default `phi serve` still uses JSON-RPC 2.0 / MCP; use `--bridge` for backward compatibility with phi-agent-python >= 0.9.1. (317 lines in `bridge_serve.rs`)

### Changed
- Bump version to 0.9.1

## [0.9.0] - 2026-08-08

### Added
- **Phase 5: File system tools** — `read_file`, `write_file`, `list_files` (phi-kernel-tools, feature gate `file`, enabled by default). The LLM can now explore, read, and modify the file system directly.
- **Phase 5: Skills → prompt-injection mode** — Skills are no longer tools. LLM discovers skills via system prompt and reads `SKILL.md` with `read_file`. This aligns with Claude Code/Codex conventions.
- **Phase 5: Memory → prompt-injection mode** — Memory is now file-based (`.phi/memory/`). No dedicated `remember`/`recall`/`forget` tools. LLM uses `read_file`/`write_file` to manage memories, same as Claude Code Memory.
- **Phase 4 收尾: CLI `phi serve` as MCP Server** — stdio + HTTP transport for external orchestrator integration. Exposes agent as a single `run` tool via JSON-RPC 2.0.
- **Phase 6: REPL debug commands** — `/events`, `/session`, `/tools`, `/snapshot`, `/snapshots` for session introspection.
- **Phase 6: Session snapshots** — create, list, restore, and delete session snapshots.
- **Phase 6: Memory templates** — Pre-built `.md` templates in phi-tools for project/config/user memories.
- **Phase 6: `hybrid_langgraph` example** — LangGraph ↔ phi-agent integration via MCP.
- **Phase 7: Performance benchmarks** — Criterion benchmarks for phi-agent, agent-base, agent-works, and phi-kernel-tools. Full baseline data documented.
- **Phase 7: Stress tests** — Bridge protocol concurrency tests, session isolation verification, `stress_test.sh` for process-level concurrency.
- **MCP Server bridge API** — `PhiAgent::into_mcp_server()` for programmatic use.
- New examples: `mcp_server` (MCP Server), `file_ops` (file tools)
- `agent_works::build_memory_system_prompt()` — reusable memory prompt generator

### Changed
- **Feature gate restructuring:**
  - `file` — file tools (read_file/write_file/list_files) + skills prompt-injection + memory prompt-injection. **Enabled by default.**
  - `shell` — shell execution. **Enabled by default.**
  - `multi-agent` — sub-agent spawning. Opt-in.
  - `mcp` — MCP client + server. Opt-in.
  - `browser` — CDP browser automation (21 tools). Opt-in.
  - `full` — all features. One-click enable.
  - `skill` feature removed — skills now work through file tools (prompt-injection mode).
- Skills: `LazySkillPrompter` is now the default (compact listing with file paths). Removed old skill-specific tools (`ListSkillsTool`, `SkillDetailTool`, `ApplySkillTool`).
- System prompt now includes persistent memory instructions (`.phi/memory/` directory, `MEMORY.md` index, frontmatter convention).
- All phi-agent layer functions return `AgentResult` instead of `anyhow::Result` (Phase 1)
- **MCP hub uses `tokio::sync::Mutex`** instead of `std::sync::Mutex` for async safety.
- **`attach_mcp` registers only the new server's tools** (O(1) instead of O(n) re-registration).
- **Removed `io_err`/`serde_err` helpers** — `From<io::Error>` and `From<serde_json::Error>` impls added to agent-base's `AgentError`.
- Bump version to 0.9.0

### Removed
- phi-kernel-tools `skill` feature and associated 3 tools (`ListSkillsTool`, `SkillDetailTool`, `ApplySkillTool`)
- agent-works `with_skill_detail_tool_factory()` and `with_list_skills_tool_factory()` builder methods
- Old `bridge` module (replaced by MCP Server)
- `src/error.rs` — `io_err()` and `serde_err()` helper functions (replaced by From impls)

## [0.3.0] - 2026-08-06

### Added
- Browser automation tools (21 CDP tools) behind `browser` feature gate
- Bridge protocol: stdio NDJSON mode for SDK consumption (`phi serve`)
- Refactored CLI: `run.rs`, `metrics.rs`, `init.rs` submodules
- Shared example boilerplate via `examples/common/mod.rs`

### Changed
- Bump `phi-tools` to 0.1.4

### Fixed
- `ReasoningEffort` now derives `Default` (agent-base 0.1.12)

## [0.2.9] - 2026-08-06

### Added
- `phi serve` now reports the registered tool count when the bridge server is ready
- `Default` implementation for `PhiAgentConfig` — supports struct update syntax

### Changed
- Bump `agent-base` to 0.1.12

## [0.2.8] - 2026-08-06

### Added
- Good first issue suggestions and PR template for community contributions
- Contributors section in README with all-contributors placeholder

### Changed
- Refactored CLI — split `main.rs` into `run.rs` and `metrics.rs` submodules
- Rewritten CONTRIBUTING.md with single-repo quick start and 48h review SLA
- Added differentiation vs other frameworks (LangChain, CrewAI, AutoGen) to README
- Added community maintenance guidelines to CLAUDE.md

### Fixed
- Bridge `ProxyTool` now passes through real description and parameters instead of hardcoded values

## [0.2.7] - 2026-08-05

### Added
- 78 new tests covering all previously untested modules (event_log, render, bridge, compression, session, config)
- Codecov coverage integration with 80% target
- Shared test utilities (`tests/common/`) with mock LLM client and event helpers

### Changed
- Upgraded agent-base to 0.1.11

### Fixed
- CI release workflow permissions for cross-repo access
- aarch64 cross-compile in release workflow
- Unused imports in shared test utilities

## [0.2.6] - 2026-08-05

### Added
- `phi serve` — NDJSON stdio bridge for SDK consumption (Python, Node.js, etc.)
- `list_tools()` now returns rich metadata: `origin` (crate name) and `version` for each tool
- CI release workflow — cross-compile for linux/darwin × x86_64/arm64

### Changed
- Exposed `bridge` module in public API for SDK authors

## [0.2.5] - 2026-08-05

### Fixed
- Serve loop race condition: events dropped after tool call in `phi serve`

## [0.2.4] - 2026-08-04

### Added
- LLM-based context compression (`SummarizingMiddleware`) — automatically summarizes earlier conversation when context window grows too large
- `list_tools()` returns `ToolMetadata` with `origin` and `version` at runtime
- `tools` REPL command to list registered tools with metadata
- `phi init` template now includes `tools` command support
- Bridge protocol tests (BR-04 empty-slot error, BR-05 session reuse, BR-06 sequential tool calls)
- Focus API types re-exported from `agent-works`

### Changed
- Metrics list: show Chars instead of Tokens, auto-default Node ID
- Tools sorted alphabetically in `tools` command output
- Upgraded agent-base to 0.1.10
- Upgraded phi-telemetry to 0.1.4

### Fixed
- Empty `node_id` in REPL mode now defaults to `phi-{current_dir_name}`
- Premature-stop prompt rules: agent no longer stops after one tool call when more work is needed
- Configurable tool output cap via `PHI_MAX_TOOL_OUTPUT_CHARS` env var (default 4000)

### Added
- `phi init` subcommand to scaffold new phi-agent projects
- `phi metrics` subcommand (list/show/last) for session observability
- 8 user guides (EN + ZH): Quick Start, Custom Tools, CLI Usage, Configuration, Focus, Architecture, Observability, Advanced
- Full i18n documentation site at [docs.phi-agent.dev](https://docs.phi-agent.dev)
- Observability card on homepage — turn logging, metrics, tracing

### Changed
- Refreshed feature cards on homepage and README — simpler, more abstract, more compelling
- Added contact info to homepage and README
- Updated docs links to use docs.phi-agent.dev domain
- Copyright updated to "hibuka labs Contributors"

## [0.1.0] - 2025-07-23

### Added
- Initial public release
- `base_agent_builder()` factory with sensible defaults
- `PhiAgent` struct wrapping `AgentRuntime`
- Terminal / JSON stream / Null renderers
- CLI entry point (`phi`) with REPL and one-shot modes
- Session management with file locking and auto-cleanup
- LLM config resolution (CLI > env > .env > default)
- `LocalShellTool` (via phi-tools)

[0.9.1]: https://github.com/hibuka-labs/phi-agent/compare/v0.9.0...HEAD
[0.9.0]: https://github.com/hibuka-labs/phi-agent/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/hibuka-labs/phi-agent/compare/v0.2.9...v0.3.0
[0.2.9]: https://github.com/hibuka-labs/phi-agent/compare/v0.2.8...v0.2.9
[0.2.7]: https://github.com/hibuka-labs/phi-agent/compare/v0.2.6...v0.2.7
[0.2.6]: https://github.com/hibuka-labs/phi-agent/compare/v0.2.5...v0.2.6
[0.2.5]: https://github.com/hibuka-labs/phi-agent/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/hibuka-labs/phi-agent/compare/v0.2.0...v0.2.4
[0.2.0]: https://github.com/hibuka-labs/phi-agent/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/hibuka-labs/phi-agent/releases/tag/v0.1.0
