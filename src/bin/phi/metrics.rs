//! Metrics display — `phi metrics` subcommand output formatting.
//!
//! Extracted from main.rs to keep the entry point focused on config assembly.

use anyhow::Result;
use phi_agent::SessionMetrics;
use phi_telemetry::{SessionOutcome, TurnOutcome, list_all_metrics, load_metrics};

use crate::args::{CliArgs, MetricsCmd, MetricsSort, OutputFormatArg};
use crate::run::truncate_str;

// ── Metrics commands ──

pub fn handle_metrics(cmd: &MetricsCmd, args: &CliArgs) -> Result<()> {
    let log_dir = args.log_dir.replace("~", &std::env::var("HOME").unwrap_or_default());
    let log_dir_path = std::path::PathBuf::from(&log_dir);

    match cmd {
        MetricsCmd::List { sort } => {
            let mut summaries = list_all_metrics(&log_dir_path)?;

            // Default order is whatever list_all_metrics produced (date
            // ascending); the other keys sort descending — biggest first —
            // with created_at as a stable tiebreaker.
            match sort {
                MetricsSort::Date => {},
                MetricsSort::Turns => {
                    summaries.sort_by(|a, b| b.total_turns.cmp(&a.total_turns).then(a.created_at.cmp(&b.created_at)));
                },
                MetricsSort::Chars => {
                    summaries.sort_by(|a, b| b.total_chars.cmp(&a.total_chars).then(a.created_at.cmp(&b.created_at)));
                },
                MetricsSort::Outcome => {
                    let rank = |o: &SessionOutcome| match o {
                        SessionOutcome::Failed => 0,
                        SessionOutcome::MaxTurns => 1,
                        SessionOutcome::Cancelled => 2,
                        SessionOutcome::Completed => 3,
                    };
                    summaries
                        .sort_by(|a, b| rank(&a.outcome).cmp(&rank(&b.outcome)).then(a.created_at.cmp(&b.created_at)));
                },
            }

            if summaries.is_empty() {
                println!("No sessions found.");
                return Ok(());
            }

            // JSON consumers get the raw summaries; the table below is for
            // terminals only.
            if args.format == OutputFormatArg::Json {
                println!("{}", serde_json::to_string_pretty(&summaries)?);
                return Ok(());
            }

            println!("  {:<30} {:<22} {:>6} {:>10}  Outcome", "Session", "Node", "Turns", "Chars");
            println!("  {}", "-".repeat(80));

            for s in &summaries {
                let label = if let Some(ref product) = s.product {
                    format!("{} ({})", s.session_id, product)
                } else {
                    s.session_id.clone()
                };

                let outcome_icon = match s.outcome {
                    SessionOutcome::Completed => "✅ completed",
                    SessionOutcome::Failed => "❌ failed",
                    SessionOutcome::Cancelled => "⏹️ cancelled",
                    SessionOutcome::MaxTurns => "⚠️ max_turns",
                };

                println!(
                    "  {:<30} {:<22} {:>6} {:>10}  {}",
                    truncate_str(&label, 29),
                    if s.node_id.is_empty() { "-" } else { &s.node_id },
                    s.total_turns,
                    format_number(s.total_chars),
                    outcome_icon,
                );
            }
            println!("\n  {} session(s)", summaries.len());
        },

        MetricsCmd::Show { session_id } => {
            let session_dir = log_dir_path.join("sessions").join(session_id);
            if !session_dir.exists() {
                eprintln!("Session '{}' not found.", session_id);
                return Ok(());
            }

            let metrics = load_metrics(&session_dir)?;
            print_session_detail(&metrics, session_id);
        },

        MetricsCmd::Last => {
            let summaries = list_all_metrics(&log_dir_path)?;
            match summaries.first() {
                Some(summary) => {
                    let session_dir = log_dir_path.join("sessions").join(&summary.session_id);
                    let metrics = load_metrics(&session_dir)?;
                    print_session_detail(&metrics, &summary.session_id);
                },
                None => {
                    println!("No sessions found.");
                },
            }
        },
    }

    Ok(())
}

pub fn print_session_detail(metrics: &SessionMetrics, session_id: &str) {
    println!();
    println!("  Session:    {}", session_id);
    // Node: always show (default_node_id ensures it's never empty)
    println!("  Node:       {}", metrics.node_id);
    println!("  Model:      {}", metrics.model);

    // Product info from custom
    if let Some(product) = metrics.custom.get("product").and_then(|v| v.as_str()) {
        let role = metrics.custom.get("role").and_then(|v| v.as_str()).map(|r| format!(" ({})", r)).unwrap_or_default();
        println!("  Product:    {}{}", product, role);
    }

    println!("  Turns:      {}", metrics.total_turns);
    println!(
        "  Duration:   {}s (avg {}s/turn, P50 {}s, P95 {}s, P99 {}s)",
        metrics.total_duration_ms / 1000,
        metrics.avg_turn_ms / 1000,
        metrics.p50_turn_ms / 1000,
        metrics.p95_turn_ms / 1000,
        metrics.p99_turn_ms / 1000,
    );
    println!("  ─────────────────────────────────────────");
    println!("  Chars:      {}", format_number(metrics.total_chars));
    println!("  ─────────────────────────────────────────");
    println!(
        "  LLM:        {}s ({}%)",
        metrics.total_llm_ms / 1000,
        (metrics.total_llm_ms * 100).checked_div(metrics.total_duration_ms).unwrap_or(0)
    );
    println!(
        "  Tool:       {}s ({}%)",
        metrics.total_tool_ms / 1000,
        (metrics.total_tool_ms * 100).checked_div(metrics.total_duration_ms).unwrap_or(0)
    );
    if !metrics.tool_breakdown.is_empty() {
        let tools: Vec<String> =
            metrics.tool_breakdown.iter().map(|(name, count)| format!("{}({})", name, count)).collect();
        println!("  Tools:      {}", tools.join(", "));
    }
    println!("  ─────────────────────────────────────────");
    let outcome_icon = match metrics.outcome {
        SessionOutcome::Completed => "✅ completed",
        SessionOutcome::Failed => "❌ failed",
        SessionOutcome::Cancelled => "⏹️ cancelled",
        SessionOutcome::MaxTurns => "⚠️ max_turns",
    };
    println!("  Outcome:    {}", outcome_icon);
    println!("  Errors:     {}", metrics.error_count);
    if metrics.total_plan_updates > 0 || metrics.total_approvals > 0 {
        println!("  Plans:      {} update(s), {} approval(s)", metrics.total_plan_updates, metrics.total_approvals);
    }

    if !metrics.turns.is_empty() {
        println!();
        println!("  Turn breakdown:");
        for turn in &metrics.turns {
            let tools_str = if turn.tools_used.is_empty() {
                "text-only".to_string()
            } else {
                format!("tools[{}]", turn.tools_used.join(", "))
            };
            let outcome_icon = match turn.outcome {
                TurnOutcome::Completed => "✅",
                TurnOutcome::ToolCalls => "🔧",
                TurnOutcome::Error => "❌",
                TurnOutcome::Cancelled => "⏹️",
                TurnOutcome::MaxTurns => "⚠️",
            };
            println!(
                "  #{:<3} {:>4}s  TTFT {:>4}ms  {:<30} {}",
                turn.turn_number,
                turn.duration_ms / 1000,
                turn.time_to_first_token_ms,
                truncate_str(&tools_str, 29),
                outcome_icon,
            );
        }
    }
    println!();
}

pub fn format_number(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
