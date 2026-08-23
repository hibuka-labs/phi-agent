# Good First Issues — Copy to GitHub

These are ready-to-publish. Copy each one as a new GitHub Issue, add the `good first issue` label.

> **Last updated:** 2026-08-19 — Previous batch (#1–#5) completed via PRs #7–#11.

---

### 1. Add `format_number` unit tests

**Labels:** `good first issue`, `test`

**Files:** `src/bin/phi/metrics.rs`

**What to do:**

The `format_number()` helper formats large numbers with K/M suffixes but has no dedicated tests. Add a `#[cfg(test)] mod tests` block covering:

```rust
assert_eq!(format_number(0), "0");
assert_eq!(format_number(42), "42");
assert_eq!(format_number(999), "999");
assert_eq!(format_number(1000), "1.0K");
assert_eq!(format_number(1500), "1.5K");
assert_eq!(format_number(999_999), "1000.0K");
assert_eq!(format_number(1_000_000), "1.0M");
assert_eq!(format_number(2_500_000), "2.5M");
```

**How to verify:** `cargo test -p phi-agent metrics` passes.

---

### 2. Generate shell completions for bash/zsh/fish

**Labels:** `good first issue`, `enhancement`

**Files:** `Cargo.toml`, `src/bin/phi/args.rs`, `src/bin/phi/main.rs`

**What to do:**

phi uses clap derive mode, so generating completions is straightforward with `clap_complete`:

1. Add `clap_complete = "4"` to `[dependencies]` in `Cargo.toml`.
2. Add a new subcommand to `SubCommand`:
   ```rust
   /// Generate shell completion scripts.
   Completions {
       /// Shell to generate for (bash, zsh, fish, elvish, powershell).
       #[arg(value_enum)]
       shell: clap_complete::Shell,
   },
   ```
3. In `main.rs`, handle the new subcommand by writing the completion script to stdout:
   ```rust
   SubCommand::Completions { shell } => {
       let mut cmd = <CliArgs as clap::CommandFactory>::command();
       clap_complete::generate(shell, &mut cmd, "phi", &mut std::io::stdout());
       Ok(())
   }
   ```

**How to verify:** `cargo build && ./target/debug/phi completions bash` outputs a valid bash completion script. Optionally test with `source <(./target/debug/phi completions bash)`.

---

### 3. Add `--format json` to `phi metrics list`

**Labels:** `good first issue`, `enhancement`

**Files:** `src/bin/phi/metrics.rs`, `src/bin/phi/args.rs`

**What to do:**

The `phi` CLI already supports `--format json` for agent output, but `phi metrics list` ignores it and always prints a terminal table. Make it respect the `--format` flag:

1. In `handle_metrics()`, accept the `OutputFormatArg` from `CliArgs`.
2. When `format == OutputFormatArg::Json`, serialize the metrics summaries as a JSON array to stdout:
   ```rust
   if args.format == OutputFormatArg::Json {
       println!("{}", serde_json::to_string_pretty(&summaries)?);
       return Ok(());
   }
   ```
3. The `SessionSummary` struct from `phi-telemetry` may need `#[derive(Serialize)]` — check and add if missing.

**How to verify:** `phi metrics list --format json` outputs valid JSON. `phi metrics list` still shows the table.

---

### 4. Add `--sort` flag to `phi metrics list`

**Labels:** `good first issue`, `enhancement`

**Files:** `src/bin/phi/metrics.rs`, `src/bin/phi/args.rs`

**What to do:**

`phi metrics list` always shows sessions in default order. Add a `--sort` flag to let users sort by different columns:

1. Add a new arg to `MetricsCmd::List`:
   ```rust
   MetricsCmd::List {
       /// Sort by: date (default), turns, chars, outcome.
       #[arg(long, value_enum, default_value = "date")]
       sort: MetricsSort,
   },
   ```
2. Define `MetricsSort` enum:
   ```rust
   #[derive(Clone, Debug, clap::ValueEnum)]
   pub enum MetricsSort {
       Date,
       Turns,
       Chars,
       Outcome,
   }
   ```
3. After fetching summaries, sort them before printing.

**How to verify:** `phi metrics list --sort turns` shows sessions ordered by turn count.

---

### 5. Add `phi metrics export` subcommand

**Labels:** `good first issue`, `enhancement`

**Files:** `src/bin/phi/metrics.rs`, `src/bin/phi/args.rs`

**What to do:**

Users currently can't export all metrics in a single file for analysis. Add:

```rust
MetricsCmd::Export {
    /// Output file path (JSON).
    #[arg(long, short)]
    output: Option<PathBuf>,
},
```

1. Collect all session summaries + detailed metrics.
2. Serialize as a JSON array.
3. Write to `output` or stdout if not specified.

**How to verify:** `phi metrics export -o metrics.json` creates a valid JSON file with all sessions.
