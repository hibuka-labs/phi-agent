# Good First Issues — Copy to GitHub

These are ready-to-publish. Copy each one as a new GitHub Issue, add the `good first issue` label.

> **Last updated:** 2026-10-07 — Batch 3.
> Batch 2 (#16–#20) completed via PRs #22/#27/#28 and closed 2026-08-22.
> Batch 1 (#2–#6, #12–#14) completed via PRs #7–#11, #22 and closed 2026-08-06..08-12.

Every entry below answers three questions: **what to change**, **which file(s)**, **how to verify**.
Keep scope to 1–2 files. Labels: `good first issue` + `help wanted` + category (`test` / `enhancement`).

---

### 1. Cover the `resolve_llm_config` priority chain

**Labels:** `good first issue`, `help wanted`, `test`

**Files:** `src/config/llm.rs`

**What to do:**

`src/config/llm.rs` documents a multi-source resolution order for LLM config:

> Priority: CLI arg > environment variable (.env) > default

`resolve_llm_config()` implements that chain, but nothing asserts it. The only current coverage is a `Clone`/`Debug` smoke test and a no-panic proptest — a regression that silently reorders the fallbacks would pass CI today.

Add unit tests that pin the resolution order.

**Acceptance criteria:**

1. **CLI wins over env** — with `LLM_MODEL=env-model` set, `resolve_llm_config(Some("cli-model"), None)` returns `model == "cli-model"`.
2. **`LLM_*` wins over `OPENAI_*`** — with both `LLM_MODEL` and `OPENAI_MODEL` set, the `LLM_MODEL` value is used. Same for `LLM_BASE_URL` / `OPENAI_BASE_URL`.
3. **`LLM_API_KEY` wins over `OPENAI_API_KEY`** — both set, `LLM_*` is used.
4. **Env wins over default** — no CLI arg, `LLM_MODEL` unset, `OPENAI_MODEL` set → `OPENAI_MODEL`'s value.
5. **Defaults** — with no args and no env vars except an API key: `model == "copilot"`, `base_url == "https://api.openai.com/v1"`.
6. **Missing API key errors** — with neither `LLM_API_KEY` nor `OPENAI_API_KEY` set, the call returns an `Err` (not a panic) and the message mentions `LLM_API_KEY`.

**How to verify:** `cargo test -p phi-agent config` passes; existing `test_llm_config_debug_clone` + the proptest still pass.

**Note:** there is already an `EnvGuard` helper in the `proptests` module in the same file that saves/restores env vars around a test — reuse it (or move it up into `mod tests`) rather than writing a new one. Env-var tests run in one process, so every test must set *and* restore the vars it touches.

---

### 2. Make `phi metrics show` / `last` respect `--format json`

**Labels:** `good first issue`, `help wanted`, `enhancement`

**Files:** `src/bin/phi/metrics.rs`

**What to do:**

`phi metrics list --format json` works (added in #17), but `show` and `last` ignore the flag and always print the terminal table.

In `handle_metrics()`, the `OutputFormatArg::Json` check lives only in the `MetricsCmd::List` arm. The `Show` and `Last` arms call `print_session_detail()` unconditionally. Make both honour the flag.

1. In the `MetricsCmd::Show` arm, when `args.format == OutputFormatArg::Json`, print `serde_json::to_string_pretty(&metrics)?` and return, instead of calling `print_session_detail()`.
2. Same for the `MetricsCmd::Last` arm.
3. Leave the terminal path exactly as it is when the flag is absent — `phi metrics show` / `last` should look unchanged by default.

**Acceptance criteria:**

- `phi metrics show <id> --format json` emits a single JSON object (the `SessionMetrics`), not an array — note `list` returns an array of summaries, `show`/`last` return one full metrics object. Worth a line in the doc comment.
- `phi metrics last --format json` likewise.
- Without the flag, output is unchanged.
- The `Session '…' not found.` path still works.

**How to verify:**

```bash
phi metrics list                                  # note a session id
phi metrics show <session-id> --format json | jq .total_turns
phi metrics last --format json | jq .outcome
phi metrics show <session-id>                     # still the human-readable table
```

**Note:** `SessionMetrics` already derives `Serialize`/`Deserialize` in `phi-telemetry`, so no type changes are needed. If you find yourself adding a `#[derive(Serialize)]`, check the `phi-telemetry` version you're building against first.

---

### 3. Add unit tests for `truncate_str` (including multi-byte input)

**Labels:** `good first issue`, `help wanted`, `test`

**Files:** `src/bin/phi/run.rs`

**What to do:**

`truncate_str()` is the only string-ellipsis helper the CLI uses (metrics tables, debug logging) and it has **zero** tests.

```rust
pub fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() > max_chars {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{}...", truncated)
    } else {
        s.to_string()
    }
}
```

Two behaviours are easy to get wrong and worth pinning down:

1. It counts **`chars()`**, not bytes — multi-byte input must not panic or cut a code point in half. This matters: phi's prompts are China-first and session labels / tool names routinely contain CJK.
2. A truncated result is `max_chars + 3` long (the `"..."` is appended *after* the cut), not `max_chars`.

**Acceptance criteria:**

Add a `#[cfg(test)] mod tests` covering at least:

1. Short string returned unchanged (no `"..."` appended).
2. Exact-fit string (`s.chars().count() == max_chars`) returned unchanged.
3. Long ASCII string truncated to `max_chars` + `"..."`.
4. Multi-byte / CJK input: `truncate_str("你好世界朋友们", 3)` does not panic and the result is valid UTF-8.
5. Empty string and `max_chars == 0`.
6. The `+3` contract asserted explicitly: `truncate_str(&"a".repeat(50), 29).chars().count() == 32`.

**How to verify:** `cargo test -p phi-agent --bin phi` passes.

**Note:** `truncate_str(&label, 29)` is passed to a `{:<30}` column in `src/bin/phi/metrics.rs` (twice). Because of the `+3`, a truncated cell is 32 chars wide and pushes that column out of alignment. **You don't need to fix this** — just assert the current `+3` behaviour so any future change is a deliberate one. Filing the column-width mismatch as a separate issue is welcome.

---

### 4. Dedupe `format_number`

**Labels:** `good first issue`, `help wanted`, `enhancement`

**Files:** `src/lib.rs`, `src/bin/phi/metrics.rs`

**What to do:**

`format_number()` is implemented twice, byte-for-byte identical:

- `src/lib.rs` (~line 142) — `pub fn format_number(n: u64) -> String`
- `src/bin/phi/metrics.rs` (~line 230) — a private copy in the binary

The binary already depends on the library (`use phi_agent::SessionMetrics;` in the same file), so the copy in `metrics.rs` can be replaced with an import of the public one.

1. Delete the local `format_number` in `src/bin/phi/metrics.rs` and use `phi_agent::format_number` instead (add it to the existing `use` statement).
2. Keep behaviour identical — this is a dedupe, not a behaviour change.
3. Handle the test module: `mod tests` in `metrics.rs` only tests `format_number`. `src/lib.rs` already has a `proptests` module covering it (`format_number_never_panics`, `format_number_correct_suffix`, `format_number_boundary_correct`). Fold the concrete boundary assertions from the `metrics.rs` tests into `src/lib.rs`'s coverage if they aren't already there, then remove the now-empty module.

**Acceptance criteria:**

- `format_number` is defined in exactly one place.
- Boundary cases are still asserted somewhere: `999 → "999"`, `1_000 → "1.0K"`, `999_999 → "1000.0K"`, `1_000_000 → "1.0M"`.
- No behaviour change.

**How to verify:**

```bash
cargo test -p phi-agent
cargo run --bin phi -- metrics list    # column formatting unchanged
cargo clippy --all-targets -- -D warnings
```

**Note:** don't change the rounding rule (`999_999 → "1000.0K"` is intentional and already locked in by a test) — just make sure there is one implementation of it.

---

### 5. Cover the `ReasoningEffortArg` → `ReasoningEffort` mapping

**Labels:** `good first issue`, `help wanted`, `test`

**Files:** `src/bin/phi/args.rs`

**What to do:**

`src/bin/phi/args.rs` is 205 lines with no tests at all. The smallest useful place to start is the `From<ReasoningEffortArg> for ReasoningEffort` impl — a four-arm mapping where one variant is spelled differently on each side:

```rust
ReasoningEffortArg::Xhigh => ReasoningEffort::XHigh,  // note the casing
```

That `Xhigh → XHigh` rename is exactly the kind of thing a test should lock in.

**Acceptance criteria:**

1. One test per variant asserting the mapped value: `Low→Low`, `Medium→Medium`, `High→High`, `Xhigh→XHigh`.
2. Round-trip through the CLI parser so a user-facing flag string maps to the right enum: `--thinking-effort low` / `medium` / `high` / `xhigh` each produce the expected `ReasoningEffort` (use `CliArgs::try_parse_from([...])`).

**How to verify:** `cargo test -p phi-agent --bin phi` passes.

**Note:** this is a good first contribution if you're new to Rust testing — the function under test is pure and the assertions are one-liners. If you'd like something bigger afterwards, `SubCommand` parsing (`init`, `serve`, `metrics`) is still untested in the same file.
