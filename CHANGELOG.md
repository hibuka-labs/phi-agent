# Changelog

All notable changes to phi-agent will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `phi serve` now reports the registered tool count when the bridge server is ready

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

[Unreleased]: https://github.com/hibuka-labs/phi-agent/compare/v0.2.8...HEAD
[0.2.8]: https://github.com/hibuka-labs/phi-agent/compare/v0.2.7...v0.2.8
[0.2.7]: https://github.com/hibuka-labs/phi-agent/compare/v0.2.6...v0.2.7
[0.2.6]: https://github.com/hibuka-labs/phi-agent/compare/v0.2.5...v0.2.6
[0.2.5]: https://github.com/hibuka-labs/phi-agent/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/hibuka-labs/phi-agent/compare/v0.2.0...v0.2.4
[0.2.0]: https://github.com/hibuka-labs/phi-agent/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/hibuka-labs/phi-agent/releases/tag/v0.1.0
