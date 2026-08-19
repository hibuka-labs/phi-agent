//! Session runner — one-shot and REPL execution modes.
//!
//! Extracted from main.rs to keep the entry point focused on config assembly.

use anyhow::Result;
use phi_agent::render::{OutputFormat, create_stdout_renderer};
use phi_agent::{PhiAgent, RunOutcome, SessionContext, save_turn_log};
use phi_telemetry::{self, SessionOutcome, save_metrics};

// ── One-shot mode ──

pub async fn run_one_shot(
    agent: &PhiAgent,
    agent_session_id: &phi_agent::SessionId,
    session_ctx: &SessionContext,
    query: &str,
    format: &OutputFormat,
) -> (Result<()>, RunOutcome) {
    let turn_start = std::time::Instant::now();
    tracing::debug!(input = %truncate_str(query, 80), "one-shot started");

    let mut renderer = create_stdout_renderer(format);
    let mut turn_events: Vec<phi_agent::RuntimeEvent> = Vec::new();

    // Ctrl+C cancellation
    let cancel_agent = agent.clone();
    let cancel_handle = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            cancel_agent.cancel();
        }
    });

    let result = agent
        .run_turn(agent_session_id.clone(), query, |event| {
            turn_events.push(event.clone());
            renderer.render(event)
        })
        .await;

    let run_outcome = match &result {
        Ok(outcome) => outcome.clone(),
        Err(_) => RunOutcome::Failed { error: "agent error".to_string() },
    };

    cancel_handle.abort();
    let _ = renderer.finish_turn();

    let _ = save_turn_log(session_ctx, 1, &turn_events, query);

    if matches!(format, OutputFormat::Json) {
        let session_info = serde_json::json!({
            "type": "session_info",
            "session_id": session_ctx.session_id,
            "is_new_session": session_ctx.is_new_session,
        });
        if let Ok(json) = serde_json::to_string(&session_info) {
            println!("{}", json);
        }
    }

    match &result {
        Ok(_) => {
            tracing::info!(duration_ms = turn_start.elapsed().as_millis() as u64, "one-shot completed");
            (Ok(()), run_outcome)
        },
        Err(err) => {
            tracing::error!(error = %err, "one-shot failed");
            if matches!(format, OutputFormat::Terminal { .. }) {
                eprintln!("\n❌ Error: {}", err);
            }
            (Err(anyhow::anyhow!("{}", err)), run_outcome)
        },
    }
}

// ── REPL mode ──

pub async fn run_repl(
    agent: &PhiAgent,
    agent_session_id: &phi_agent::SessionId,
    session_ctx: &SessionContext,
    format: &OutputFormat,
) -> Result<()> {
    if matches!(format, OutputFormat::Terminal { .. }) {
        print_welcome_banner(agent, session_ctx);
    }

    let node_id = std::env::var("PHI_NODE_ID").unwrap_or_else(|_| default_node_id());
    let metrics_enabled = std::env::var("PHI_METRICS_ENABLED")
        .map(|v| {
            let v = v.to_lowercase();
            !matches!(v.as_str(), "false" | "0" | "no" | "off" | "")
        })
        .unwrap_or(true);

    let mut telemetry = if metrics_enabled {
        Some(phi_telemetry::init_telemetry(
            agent.runtime(),
            session_ctx.session_id.clone(),
            node_id,
            agent.config.model.clone(),
        ))
    } else {
        None
    };

    let mut agent_session_id = agent_session_id.clone();
    // Resume turn numbering from any turns already logged for this session, so
    // reusing a session across runs continues the sequence instead of appending
    // a later run's `turn_001.jsonl` into an earlier one.
    let mut turn_number: u32 = session_ctx.last_turn_number();

    let mut rl = rustyline::Editor::<(), rustyline::history::FileHistory>::new()?;
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let history_path = std::path::PathBuf::from(home).join(".phi-agent").join("history");
    let _ = rl.load_history(&history_path);
    let prompt = format!("\n{}Phi > {}", "\x1b[1m", "\x1b[0m");

    loop {
        let input = match rl.readline(&prompt) {
            Ok(line) => {
                let trimmed = line.trim().to_string();
                if !trimmed.is_empty() {
                    let _ = rl.add_history_entry(&trimmed);
                }
                trimmed
            },
            Err(rustyline::error::ReadlineError::Eof) => break,
            Err(rustyline::error::ReadlineError::Interrupted) => continue,
            Err(_) => break,
        };

        if input.is_empty() {
            continue;
        }
        // Normalize: strip leading "/" so both "compact" and "/compact" work.
        let input = input.strip_prefix('/').unwrap_or(&input).to_string();
        if matches!(input.as_str(), "exit" | "quit") {
            tracing::info!("user exit");
            break;
        }
        if input == "reset" {
            agent_session_id = agent.create_session().await;
            turn_number = session_ctx.last_turn_number();
            tracing::info!(new_session_id = %agent_session_id.id, "session reset");
            if matches!(format, OutputFormat::Terminal { .. }) {
                println!("\n✅ New session created");
            }
            continue;
        }
        if input == "compact" {
            match phi_agent::agent::builder::run_compact_session(agent.runtime(), &agent_session_id).await {
                Ok(true) => {
                    if matches!(format, OutputFormat::Terminal { .. }) {
                        println!("\n✅ Session compressed (history summarised, recent messages kept)");
                    }
                },
                Ok(false) => {
                    if matches!(format, OutputFormat::Terminal { .. }) {
                        println!("\n✅ Session is below threshold — no compression needed");
                    }
                },
                Err(e) => {
                    if matches!(format, OutputFormat::Terminal { .. }) {
                        println!("\n❌ Compression failed: {e}");
                    }
                },
            }
            continue;
        }
        if input == "tools" {
            let tools = agent.list_tools().await;
            if matches!(format, OutputFormat::Terminal { .. }) {
                println!();
                if tools.is_empty() {
                    println!("  (no tools registered)");
                } else {
                    println!("  Registered tools ({}):\n", tools.len());
                    for m in &tools {
                        println!("  \x1b[1m{}\x1b[0m  {}  v{}", m.name, m.origin, m.version);
                        println!("    {}", m.description);
                        if !m.requirements.is_empty() {
                            println!("    requirements: {}", m.requirements.join(", "));
                        }
                    }
                }
                println!();
            }
            continue;
        }
        if input == "session" {
            if matches!(format, OutputFormat::Terminal { .. }) {
                println!();
                println!("  \x1b[1mSession Context\x1b[0m");
                println!("  ─────────────────");
                println!("  Session ID:  {}", session_ctx.session_id);
                println!("  Directory:   {}", session_ctx.session_dir.display());
                println!("  Status:      {}", if session_ctx.is_new_session { "new" } else { "reused" });
                println!("  Turn:        {}", turn_number);
                println!("  Log:         {}", session_ctx.log_path().display());
                println!();
            }
            continue;
        }
        if input == "events" {
            if matches!(format, OutputFormat::Terminal { .. }) {
                println!();
                println!("  \x1b[1mEvent Stream\x1b[0m");
                println!("  ────────────────");
                println!("  Turns completed: {}", turn_number);
                if turn_number > 0 {
                    for t in 1..=turn_number {
                        let turn_path = session_ctx.turn_path(t as usize);
                        let size = std::fs::metadata(&turn_path).map(|m| m.len()).unwrap_or(0);
                        println!("    turn {:03}: {} ({} bytes)", t, turn_path.display(), size);
                    }
                } else {
                    println!("  (no turns yet)");
                }
                println!();
            }
            continue;
        }
        if input == "snapshots" {
            match phi_agent::session::list_snapshots(&session_ctx.base_dir) {
                Ok(snapshots) => {
                    if matches!(format, OutputFormat::Terminal { .. }) {
                        println!();
                        if snapshots.is_empty() {
                            println!("  (no snapshots saved)");
                        } else {
                            println!("  \x1b[1mSnapshots\x1b[0m ({}):\n", snapshots.len());
                            for s in &snapshots {
                                println!("  \x1b[1m{}\x1b[0m  ({})", s.name, s.session_id);
                                println!("    {} turns, saved {}", s.turn_count, s.created_at);
                            }
                        }
                        println!();
                    }
                },
                Err(e) => eprintln!("  Failed to list snapshots: {e}"),
            }
            continue;
        }
        if let Some(name) = input.strip_prefix("snapshot ") {
            let name = name.trim();
            if name.is_empty() {
                println!("  Usage: snapshot <name>");
            } else {
                match phi_agent::session::create_snapshot(session_ctx, name, &session_ctx.base_dir) {
                    Ok(_) => {
                        if matches!(format, OutputFormat::Terminal { .. }) {
                            println!("\n✅ Snapshot '{}' saved\n", name);
                        }
                    },
                    Err(e) => eprintln!("\n  Failed to save snapshot: {e}\n"),
                }
            }
            continue;
        }

        let _ = rl.save_history(&history_path);

        turn_number += 1;
        let turn_start = std::time::Instant::now();
        tracing::debug!(turn = turn_number, input = %truncate_str(&input, 80), "turn started");

        let mut renderer = create_stdout_renderer(format);
        let mut turn_events: Vec<phi_agent::RuntimeEvent> = Vec::new();

        let cancel_agent = agent.clone();
        let cancel_handle = tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                cancel_agent.cancel();
            }
        });

        match agent
            .run_turn(agent_session_id.clone(), &input, |event| {
                turn_events.push(event.clone());
                renderer.render(event)
            })
            .await
        {
            Ok(_) => {
                cancel_handle.abort();
                renderer.finish_turn()?;

                let is_cancelled = agent.is_cancelled();
                save_turn_log(session_ctx, turn_number, &turn_events, &input)?;

                // Save metrics incrementally
                if let Some(ref handle) = telemetry {
                    let session = handle.session.read().await;
                    let _ = save_metrics(&session, &session_ctx.session_dir);
                }

                if is_cancelled {
                    tracing::info!(turn = turn_number, "turn cancelled by user");
                } else {
                    tracing::info!(
                        turn = turn_number,
                        duration_ms = turn_start.elapsed().as_millis() as u64,
                        "turn completed"
                    );
                }
            },
            Err(err) => {
                cancel_handle.abort();
                renderer.finish_turn()?;

                save_turn_log(session_ctx, turn_number, &turn_events, &input)?;

                // Save metrics on error too
                if let Some(ref handle) = telemetry {
                    let session = handle.session.read().await;
                    let _ = save_metrics(&session, &session_ctx.session_dir);
                }

                tracing::error!(error = %err, turn = turn_number, "agent turn failed");
                if matches!(format, OutputFormat::Terminal { .. }) {
                    eprintln!("\n❌ Error: {}", err);
                }
            },
        }
    }

    // Finalize metrics on session end
    if let Some(handle) = &mut telemetry {
        handle.shutdown().await;
        let session = handle.session.read().await;
        let mut session = session.clone();
        session.finalize(SessionOutcome::Completed);
        let _ = save_metrics(&session, &session_ctx.session_dir);
    }

    Ok(())
}

// ── Helpers ──

pub fn print_welcome_banner(agent: &PhiAgent, session_ctx: &SessionContext) {
    println!();
    println!("╔═══════════════════════════════════════════════════╗");
    println!("║  \x1b[1mphi\x1b[0m — General-purpose AI Agent CLI                 ║");
    println!("║                                                   ║");
    println!("║  Model: {:<42}║", if agent.config.model.is_empty() { "default" } else { &agent.config.model });
    println!("║  Session: {:<40}║", session_ctx.session_id);
    if session_ctx.is_new_session {
        println!("║  Status: New session                                ║");
    } else {
        println!("║  Status: Reusing session                            ║");
    }
    println!("║                                                   ║");
    println!("║  Commands: exit/quit | reset | tools | session | events║");
    println!("║            compact | snapshot <name> | snapshots        ║");
    println!("╚═══════════════════════════════════════════════════╝");
    println!();
}

/// Default node_id: phi-{current_dir_name}, or phi-unknown.
pub fn default_node_id() -> String {
    let dir = std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "unknown".to_string());
    format!("phi-{}", dir)
}

pub fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() > max_chars {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{}...", truncated)
    } else {
        s.to_string()
    }
}

/// Initialize logging: write to file only, no console output.
pub async fn init_logging(session_ctx: &SessionContext, log_level: &str) -> Result<()> {
    use log_core::{LogCoreLayer, LogLevel};
    use tracing_subscriber::prelude::*;

    let session_log_path = session_ctx.log_path();

    let level = match log_level {
        "debug" => LogLevel::Debug,
        "info" => LogLevel::Info,
        "warn" => LogLevel::Warn,
        "error" => LogLevel::Error,
        _ => LogLevel::Info,
    };

    let layer = LogCoreLayer::file(session_log_path.to_str().unwrap_or("phi.log"), level).await?;

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level)),
        )
        .with(layer)
        .init();

    tracing::info!(path = %session_log_path.display(), "logging initialized");

    Ok(())
}
