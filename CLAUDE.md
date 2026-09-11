# CLAUDE.md

## Project: phi-agent

A general-purpose AI Agent framework in Rust, built on `agent-base` and `agent-works`.

### Architecture Principle
**phi-agent itself does NOT bundle application tools.** It provides infrastructure only (builder factory, renderers, config, session management). Kernel tools (file I/O, shell, multi-agent) are available via `phi-kernel-tools` behind feature flags. File tools and MCP are on by default; shell and multi-agent are opt-in. Application tools are implemented in `phi-tools` and injected by consumers (CLI, web, etc.).

### Current Branch
`main` — the public open-source branch. Browser automation tools (21 tools via CDP) are included behind the `browser` feature gate. Enable with `cargo build --features browser` or `phi --features browser`.

### Dependency Chain

All crates are **independent git repositories** with **pure version dependencies** on crates.io:

```
agent-base (runtime kernel)    ← github.com/hibuka-labs/agent-base
    ↑
agent-works (MCP, Skills)      ← github.com/hibuka-labs/agent-works
    ↑
phi-agent (framework + CLI)    ← github.com/hibuka-labs/phi-agent  (this repo)
```

Additional optional deps (pulled from crates.io):
- `phi-tools` — optional, contains LocalShellTool
- `phi-telemetry` — optional, metrics collection
- `log-core` — optional, structured logging

### Key Crates (independent repos)
| Crate | GitHub | crates.io |
|-------|--------|-----------|
| agent-base | hibuka-labs/agent-base | agent-base |
| agent-works | hibuka-labs/agent-works | agent-works |
| phi-agent | hibuka-labs/phi-agent | phi-agent |
| phi-tools | hibuka-labs/phi-tools | phi-tools |
| phi-telemetry | hibuka-labs/phi-telemetry | phi-telemetry |
| log-core | hibuka-labs/log-core | log-core |

All repos have 3 remotes: `github`, `gitee`, `origin`.

### Development Workflow

**This repo uses pure version dependencies.** All `Cargo.toml` files contain only `version` (no `path`). This means:

```toml
# phi-agent/Cargo.toml — committed as-is
agent-base = "0.1.6"
agent-works = "0.1.4"
```

**To modify a dependency locally:**

1. Clone the dependency repo to a sibling directory:
   ```bash
   git clone git@github.com:hibuka-labs/agent-base.git ../agent-base
   ```

2. Temporarily add `path` to Cargo.toml (**DO NOT COMMIT this change**):
   ```toml
   agent-base = { version = "0.1.6", path = "../agent-base" }
   ```

   ⚠️ **Never use path-only** (`agent-base = { path = "..." }` without version). The release script's
   sed strips `path` but leaves `version` — if there's no version, `cargo publish` fails.
   Always keep `version` alongside `path`.

3. Remove `path` before committing.

**CRITICAL: Always use local paths during development.** Every crate that depends on sibling crates MUST have `path = "../<crate>"` on its dependency line AND `[patch.crates-io]` pointing to the local path. Without this, Cargo resolves to the stale published version on crates.io, which may lack new methods (e.g., `metadatas()`) — causing confusing "method not found" errors even though the local source has the method.

Pattern to use in EVERY crate's Cargo.toml during local dev:
```toml
[dependencies]
agent-base = { version = "0.1.12", path = "../agent-base" }

# LOCAL DEV ONLY — DO NOT COMMIT
[patch.crates-io]
agent-base = { path = "../agent-base" }
```

**After publishing a new version of a dependency:**

```bash
cargo update -p agent-base    # update to latest crates.io version
# Update Cargo.toml version number if needed
```

### Why no monorepo?

Each crate is an independent repo to maintain clear boundaries:
- AI assistance sessions work within a single crate — prevents cross-crate contamination
- Each crate has its own release cycle
- Contributors can fork and modify individual crates without the entire workspace

### Project Structure
```
phi-agent/
├── .github/workflows/ci.yml  # CI: fmt + clippy + build + test + doc
├── examples/                 # Runnable examples
│   ├── hello-agent.rs
│   ├── custom-tool.rs
│   ├── focus-demo.rs
│   └── multi-tool.rs
├── guide/                    # User tutorials (EN + CN)
│   ├── getting-started.md / _CN.md
│   ├── custom-tool.md / _CN.md
│   ├── focus.md / _CN.md
│   ├── configuration.md / _CN.md
│   └── advanced.md / _CN.md
├── book/                     # mdBook documentation site
│   ├── book.toml
│   └── src/
├── assets/                   # Brand assets (logo, etc.)
│   └── logo.svg
├── tests/
│   └── integration_test.rs   # 7 tests with mock LLM client
├── docs/                     # Public guide (en/ zh/) + git-ignored internal design notes
├── CHANGELOG.md
├── CONTRIBUTING.md
├── SECURITY.md               # Contact: phiagent@hibuka.com
├── CODE_OF_CONDUCT.md
├── rustfmt.toml
├── README.md / README_CN.md
└── src/
    ├── lib.rs                # Public API re-exports
    ├── agent/
    │   ├── builder.rs        # base_agent_builder() factory
    │   └── factory.rs        # PhiAgent struct + PhiAgentConfig
    ├── render/               # EventRenderer trait + Terminal/JsonStream/Null
    ├── cli/                  # AutoApprovalHandler (Auto/DenyAll)
    ├── config/               # LLM config resolution (CLI > env > .env > default)
    ├── prompt.rs             # build_system_prompt() + build_system_prompt_cn()
    ├── event_log.rs          # Turn event → JSONL persistence
    ├── session.rs            # Session ID, directory, file locking, cleanup
    └── bin/forge/            # CLI binary (phi): REPL + one-shot
```

### Brand Building Progress
- ✅ Phase 1 — Quality: CI/CD, rustfmt, CHANGELOG, CONTRIBUTING, SECURITY, CODE_OF_CONDUCT, tests
- ✅ Phase 2 — DevEx: API doc comments, enhanced README (badges, architecture, FAQ, Why section), 3 examples, 4 tutorials (EN+CN), .env.example
- 🔄 Phase 3 — Branding: Logo ✅, mdBook doc site ✅, Issue templates ✅, community (GitHub Discussions pending)

### Conventions
- Rust edition 2024
- Async runtime: tokio (full features)
- Error handling: anyhow + agent_base::AgentResult
- CLI: clap derive mode, uses OpenAiClient (OpenAI-compatible APIs only)
- Session data: ~/.phi-agent/sessions/<id>/
- Contact email: phiagent@hibuka.com

### Key Design Decisions
- **No built-in application tools** — framework knows nothing about specific tools (web search, DB, etc.)
- **Kernel tools off by default (except file)** — shell, multi-agent are behind feature flags (`shell`, `multi-agent`), all opt-in; file tools are on by default with skills and MCP
- **No built-in memory** — no vector DB, no hidden state. Predictable and debuggable.
- **OpenAI-compatible CLI** — CLI uses OpenAiClient; Anthropic requires AnthropicClient (code change)
- **China-first prompt** — build_system_prompt_cn() for GFW-aware environments
- **Browser tools behind feature gate** — 21 CDP tools available via `--features browser`

### Pre-Commit Checklist (MANDATORY)

**Every commit MUST pass this before `git commit`:**

```bash
cargo fmt --check && cargo test && cargo clippy --all-targets -- -D warnings 2>&1 | grep -v 'agent-base'
```

- `cargo fmt --check` — if it reports diffs, fix them with `cargo fmt` (no `--check`), then re-check.
- `cargo test` — all tests must pass (currently 108 tests).
- `cargo clippy --all-targets -- -D warnings` — filter out agent-base warnings (from patched crate); only phi-agent warnings matter.
- **Never skip these steps.** CI failures on fmt/clippy/test are unacceptable because they're always reproducible locally.

If `cargo fmt` reports diffs, fix them: `cargo fmt` (no `--check`).  
If clippy warns, fix the warnings — CI treats all warnings as errors.

### Community

**PR Review SLA: 48 hours.** Respond to every PR within 48h — even if just "looking at this, will review soon." Small, focused PRs get merged faster. If a PR is too large, ask the contributor to break it up rather than doing a marathon review.

**First-time contributors:** Be welcoming. Merge small fixes (typos, doc improvements) quickly — don't nitpick style on a first PR. Format issues can be fixed by the maintainer or a follow-up PR. The goal is to make their first contribution feel good, not perfect.

**Good First Issues:**
- Scope: 1–2 files, clear acceptance criteria
- Each issue must answer: (1) what to change, (2) which file(s), (3) how to verify
- Keep `.github/GOOD_FIRST_ISSUES.md` as a template source — when it runs low, add 3–5 more
- Label: `good first issue` + `help wanted`

**Labels:**
| Label | Purpose |
|-------|---------|
| `good first issue` | Small, newcomer-friendly tasks |
| `help wanted` | Needs community contribution |
| `bug` | Confirmed bug |
| `enhancement` | Feature request or improvement |
| `documentation` | Docs-only change |

**After merging a PR:** Add the contributor to the all-contributors table. (Setup: install [all-contributors bot](https://github.com/all-contributors/all-contributors) on the repo, then comment `@all-contributors add @username for code` on the merged PR.)

### Pre-Push

### Documentation Deployment

```
docs.phi-agent.dev (域名: phiagent.dev, 注册商: 22net)
  ├── 境内 → 阿里云 OSS 香港 (phiagent-docs.oss-cn-hongkong.aliyuncs.com)
  └── 境外 → GitHub Pages (hibuka-labs.github.io)
```

- **Building**: `mkdocs build` (MkDocs Material + i18n plugin) → `site/`
- **Deploy**: `.github/workflows/deploy-docs.yml` — builds then syncs to OSS (oss2 SDK) + GitHub Pages
- **Manual trigger**: Actions → Deploy Docs → Run workflow (needed after first-time setup or when docs files aren't changed)
- **Auto trigger**: Push to main with changes in `docs/**`, `mkdocs.yml`, `requirements.txt`, or `deploy-docs.yml`
- **OSS Bucket**: `phiagent-docs` (香港), static website hosting enabled, default page `index.html`
- **Secrets** (GitHub → Settings → Secrets): `OSS_ACCESS_KEY_ID`, `OSS_ACCESS_KEY_SECRET`, `OSS_ENDPOINT`, `OSS_BUCKET`

### Gotchas

- **log-core** uses `main` as default branch (not `master`) on GitHub — push to `main` not `master`
- **Phi-tools** browser tools are on main behind `browser` feature flag — no separate branch needed
- CI clones all 5 repos as siblings with `--depth 1`, so pushed code must be on the default branch
