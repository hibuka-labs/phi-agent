//! Event Log — demonstrate per-turn JSONL event persistence.
//!
//! Shows how to save agent turn events to a JSONL file for audit trails,
//! debugging, and offline analysis. Each turn produces a timestamped file
//! with metadata headers and structured event lines.
//!
//! Usage:
//!   cargo run --example event_log

use phi_agent::{event_log::save_turn_log, session};

#[path = "../common/mod.rs"]
mod common;

fn main() -> anyhow::Result<()> {
    // ── 1. Prepare a session ──
    let base_dir = std::env::temp_dir().join("phi-agent-event-log-demo");
    let ctx = session::resolve_session(Some("event-demo"), &base_dir)?;

    // ── 2. Simulate turn events (in a real app, collect from agent.run_turn) ──
    let session_id = phi_agent::SessionId { id: 1, external_id: None };

    let events = vec![
        phi_agent::RuntimeEvent::ThoughtDelta {
            session_id: session_id.clone(),
            text: "Let me think about this...".into(),
            agent_id: None,
            trace_id: None,
        },
        phi_agent::RuntimeEvent::ToolCallStarted {
            session_id: session_id.clone(),
            tool_name: "shell".into(),
            args_json: r#"{"cmd":"ls"}"#.into(),
            agent_id: None,
            trace_id: None,
        },
        phi_agent::RuntimeEvent::ToolCallFinished {
            session_id: session_id.clone(),
            tool_name: "shell".into(),
            summary: "Listed 3 files".into(),
            agent_id: None,
            trace_id: None,
            denied: false,
        },
        phi_agent::RuntimeEvent::TextDelta {
            session_id: session_id.clone(),
            text: "Your directory contains 3 files.".into(),
            agent_id: None,
            trace_id: None,
        },
        phi_agent::RuntimeEvent::RunFinished { session_id: session_id.clone(), agent_id: None, trace_id: None },
    ];

    // ── 3. Save turn log ──
    save_turn_log(&ctx, 1, &events, "What files are in this directory?")?;
    println!("Turn log saved to: {}", ctx.turn_path(1).display());

    // ── 4. Read back the log ──
    let content = std::fs::read_to_string(ctx.turn_path(1))?;
    let lines: Vec<&str> = content.lines().collect();

    println!("\n=== Turn 1 JSONL ({}) lines ===", lines.len());
    for (i, line) in lines.iter().enumerate() {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
            let event_type = val["type"].as_str().unwrap_or("?");
            println!("  [{}] {}", i, event_type);
        }
    }

    // ── 5. Verify structure ──
    let metadata: serde_json::Value = serde_json::from_str(lines[0])?;
    assert_eq!(metadata["turn"], 1);
    assert_eq!(metadata["user_input"], "What files are in this directory?");
    assert!(metadata["timestamp"].as_str().is_some());

    let last: serde_json::Value = serde_json::from_str(lines.last().unwrap())?;
    assert_eq!(last["type"], "turn_end");

    // ── 6. Multi-turn append ──
    let events2 = vec![phi_agent::RuntimeEvent::TextDelta {
        session_id: session_id.clone(),
        text: "Follow-up response.".into(),
        agent_id: None,
        trace_id: None,
    }];
    save_turn_log(&ctx, 1, &events2, "Tell me more")?;
    // The file now has 2 turn_end markers — one per call

    // ── 7. Clean up ──
    drop(ctx);
    let _ = std::fs::remove_dir_all(&base_dir);

    println!("\n=== Event log demonstrated ===");
    Ok(())
}
