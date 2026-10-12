//! CLI metrics output, using synthetic sessions and no LLM connection.

#![cfg(all(feature = "mcp", feature = "telemetry", feature = "logging"))]

use std::path::Path;
use std::process::{Command, Output};

use phi_telemetry::{SessionMetrics, save_metrics};
use serde_json::{Value, json};

fn session_fixture() -> (tempfile::TempDir, SessionMetrics) {
    let dir = tempfile::tempdir().unwrap();
    let mut metrics = SessionMetrics::new("metrics-session".into(), "test-node".into(), "test-model".into());
    metrics.total_turns = 2;
    metrics.total_chars = 1500;
    metrics.custom = json!({"product": "test-product"});
    let session_dir = dir.path().join("sessions").join(&metrics.session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    save_metrics(&metrics, &session_dir).unwrap();
    (dir, metrics)
}

fn run_metrics(dir: &Path, args: &[&str]) -> Output {
    let output =
        Command::new(env!("CARGO_BIN_EXE_phi")).current_dir(dir).arg("--log-dir").arg(dir).args(args).output().unwrap();
    assert!(output.status.success(), "CLI failed: {}", String::from_utf8_lossy(&output.stderr));
    output
}

#[test]
fn metrics_show_json_returns_full_object_with_either_flag_placement() {
    let (dir, metrics) = session_fixture();
    for args in [
        ["--format", "json", "metrics", "show", "metrics-session"],
        ["metrics", "show", "metrics-session", "--format", "json"],
    ] {
        let output = run_metrics(dir.path(), &args);
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value, serde_json::to_value(&metrics).unwrap());
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn metrics_last_json_returns_full_object_with_either_flag_placement() {
    let (dir, metrics) = session_fixture();
    for args in [["--format", "json", "metrics", "last"], ["metrics", "last", "--format", "json"]] {
        let output = run_metrics(dir.path(), &args);
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value, serde_json::to_value(&metrics).unwrap());
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn metrics_list_json_remains_an_array_with_either_flag_placement() {
    let (dir, metrics) = session_fixture();
    for args in [["--format", "json", "metrics", "list"], ["metrics", "list", "--format", "json"]] {
        let output = run_metrics(dir.path(), &args);
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        let summaries = value.as_array().unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0]["session_id"], metrics.session_id);
        assert_eq!(summaries[0]["total_turns"], metrics.total_turns);
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn metrics_show_and_last_keep_terminal_output_by_default() {
    let (dir, _) = session_fixture();
    let show = run_metrics(dir.path(), &["metrics", "show", "metrics-session"]);
    let last = run_metrics(dir.path(), &["metrics", "last"]);
    assert_eq!(show.stdout, last.stdout);
    let text = String::from_utf8(show.stdout).unwrap();
    assert!(text.contains("Session:    metrics-session"));
    assert!(text.contains("Model:      test-model"));
    assert!(text.contains("Turns:      2"));
    assert!(show.stderr.is_empty());
    assert!(last.stderr.is_empty());
}

#[test]
fn metrics_show_json_keeps_missing_session_message() {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        ["--format", "json", "metrics", "show", "missing-session"],
        ["metrics", "show", "missing-session", "--format", "json"],
    ] {
        let output = run_metrics(dir.path(), &args);
        assert!(output.stdout.is_empty());
        assert_eq!(String::from_utf8(output.stderr).unwrap(), "Session 'missing-session' not found.\n");
    }
}

#[test]
fn metrics_list_and_last_keep_empty_session_message() {
    let dir = tempfile::tempdir().unwrap();
    for command in ["list", "last"] {
        for args in [
            vec!["metrics", command],
            vec!["--format", "json", "metrics", command],
            vec!["metrics", command, "--format", "json"],
        ] {
            let output = run_metrics(dir.path(), &args);
            assert_eq!(String::from_utf8(output.stdout).unwrap(), "No sessions found.\n");
            assert!(output.stderr.is_empty());
        }
    }
}
