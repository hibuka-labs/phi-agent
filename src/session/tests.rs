use std::fs::File;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use agent_base::{AgentError, ChatMessage};
use tempfile::TempDir;

use super::listing::extract_session_title;
use super::messages::messages_jsonl_path;
use super::*;

#[test]
fn test_validate_session_id_valid() {
    assert!(validate_session_id("my-session-123").is_ok());
    assert!(validate_session_id("test_456").is_ok());
    assert!(validate_session_id("a").is_ok());
}

#[test]
fn test_validate_session_id_invalid() {
    assert!(validate_session_id("").is_err());
    assert!(validate_session_id("my session").is_err());
    assert!(validate_session_id("../etc").is_err());
    assert!(validate_session_id("path/traversal").is_err());
}

#[test]
fn test_generate_session_id() {
    let id = generate_session_id();
    assert!(id.contains('_'));
    let parts: Vec<&str> = id.split('_').collect();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].len(), 8);
    assert_eq!(parts[1].len(), 8);
}

#[test]
fn test_session_context_methods() {
    let tmp = TempDir::new().unwrap();
    let ctx = resolve_session(Some("test-ctx"), tmp.path()).unwrap();

    assert_eq!(ctx.session_id, "test-ctx");
    assert!(ctx.session_id_path().exists());
    assert!(ctx.metadata_path().exists());
    assert_eq!(ctx.log_path(), ctx.session_dir.join("session.log"));
    assert_eq!(ctx.turn_path(1), ctx.session_dir.join("turn_001.jsonl"));
}

#[test]
fn test_last_turn_number_scans_existing_turns() {
    let tmp = TempDir::new().unwrap();
    let ctx = resolve_session(Some("turn-scan"), tmp.path()).unwrap();
    assert_eq!(ctx.last_turn_number(), 0, "empty session should start at 0");

    std::fs::write(ctx.turn_path(1), "").unwrap();
    std::fs::write(ctx.turn_path(3), "").unwrap();
    assert_eq!(ctx.last_turn_number(), 3);

    // Non-turn files (and malformed turn names) are ignored.
    std::fs::write(ctx.session_dir.join("session.log"), "").unwrap();
    std::fs::write(ctx.session_dir.join("turn_abc.jsonl"), "").unwrap();
    std::fs::write(ctx.session_dir.join("turn_004.jsonl"), "").unwrap();
    assert_eq!(ctx.last_turn_number(), 4);
}

#[test]
fn test_cleanup_expired_sessions() {
    let tmp = TempDir::new().unwrap();
    let (dir, _) = get_or_create_session_dir("old-session", tmp.path()).unwrap();

    let meta_path = dir.join("session_meta.json");
    let mut meta: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
    let old = (chrono::Utc::now() - chrono::Duration::days(8)).to_rfc3339();
    meta["last_active_at"] = serde_json::json!(old);
    std::fs::write(&meta_path, serde_json::to_string_pretty(&meta).unwrap()).unwrap();

    get_or_create_session_dir("new-session", tmp.path()).unwrap();
    let cleaned = cleanup_expired_sessions(tmp.path(), 7).unwrap();
    assert_eq!(cleaned, 1);
    assert!(!dir.exists());
}

// ── Phase 1: AgentError contract tests ──

#[test]
fn test_validate_session_id_returns_config_error() {
    // Empty ID
    let err = validate_session_id("").unwrap_err();
    assert!(matches!(err, AgentError::ConfigError(_)), "expected ConfigError for empty session ID, got {:?}", err);

    // Invalid characters
    let err = validate_session_id("../etc").unwrap_err();
    assert!(matches!(err, AgentError::ConfigError(_)), "expected ConfigError for invalid session ID, got {:?}", err);
}

#[test]
fn test_resolve_session_returns_config_error_for_invalid_id() {
    let tmp = TempDir::new().unwrap();
    let result = resolve_session(Some("bad id with spaces"), tmp.path());
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(matches!(err, AgentError::ConfigError(_)), "expected ConfigError for invalid session ID, got {:?}", err);
}

#[test]
fn test_acquire_session_lock_returns_resource_unavailable_when_locked() {
    let tmp = TempDir::new().unwrap();
    let (_dir, _) = get_or_create_session_dir("lock-test", tmp.path()).unwrap();
    let session_dir = tmp.path().join("sessions").join("lock-test");

    // First lock succeeds
    let _lock1 = acquire_session_lock(&session_dir).unwrap();

    // Second lock should fail with ResourceUnavailable
    let err = acquire_session_lock(&session_dir).unwrap_err();
    assert!(
        matches!(err, AgentError::ResourceUnavailable(_)),
        "expected ResourceUnavailable for locked session, got {:?}",
        err
    );
}

// ── Snapshot name validation tests ──

#[test]
fn test_validate_snapshot_name_valid() {
    assert!(validate_snapshot_name("my-snapshot").is_ok());
    assert!(validate_snapshot_name("test_123").is_ok());
    assert!(validate_snapshot_name("a").is_ok());
    assert!(validate_snapshot_name("snapshot-2024-01-01").is_ok());
}

#[test]
fn test_validate_snapshot_name_invalid() {
    assert!(validate_snapshot_name("").is_err());
    assert!(validate_snapshot_name("../etc").is_err());
    assert!(validate_snapshot_name("path/traversal").is_err());
    assert!(validate_snapshot_name("back\\slash").is_err());
    assert!(validate_snapshot_name("dot..dot").is_err());
    assert!(validate_snapshot_name("has space").is_err());
    assert!(validate_snapshot_name(&"x".repeat(65)).is_err()); // too long
}

#[test]
fn test_validate_snapshot_name_returns_config_error() {
    let err = validate_snapshot_name("").unwrap_err();
    assert!(matches!(err, AgentError::ConfigError(_)), "expected ConfigError, got {:?}", err);

    let err = validate_snapshot_name("../etc").unwrap_err();
    assert!(matches!(err, AgentError::ConfigError(_)), "expected ConfigError for path traversal, got {:?}", err);
}

// ── Snapshot create / list tests ──

#[test]
fn test_create_snapshot_and_list() {
    let tmp = TempDir::new().unwrap();
    let ctx = resolve_session(Some("snap-session"), tmp.path()).unwrap();

    // Have at least one turn log so we can verify it is copied
    let turn_path = ctx.turn_path(1);
    std::fs::write(&turn_path, "{}").unwrap();

    // Create a snapshot
    let snap_path = create_snapshot(&ctx, "test-snap", tmp.path()).unwrap();
    assert!(snap_path.exists());
    assert!(snap_path.join("snapshot_info.json").exists());
    assert!(snap_path.join("turn_001.jsonl").exists());

    // List — should contain exactly one
    let snaps = list_snapshots(tmp.path()).unwrap();
    assert_eq!(snaps.len(), 1);
    assert_eq!(snaps[0].name, "test-snap");
    assert_eq!(snaps[0].session_id, "snap-session");
    assert_eq!(snaps[0].turn_count, 1);
}

#[test]
fn test_create_snapshot_overwrites_existing() {
    let tmp = TempDir::new().unwrap();
    let ctx = resolve_session(Some("snap-session"), tmp.path()).unwrap();

    create_snapshot(&ctx, "dup-snap", tmp.path()).unwrap();
    // Second create with same name should succeed (overwrite)
    let result = create_snapshot(&ctx, "dup-snap", tmp.path());
    assert!(result.is_ok());

    let snaps = list_snapshots(tmp.path()).unwrap();
    assert_eq!(snaps.len(), 1);
}

#[test]
fn test_list_snapshots_empty_dir() {
    let tmp = TempDir::new().unwrap();
    let snaps = list_snapshots(tmp.path()).unwrap();
    assert!(snaps.is_empty());
}

#[test]
fn test_list_snapshots_sorted_by_date_desc() {
    let tmp = TempDir::new().unwrap();
    let ctx = resolve_session(Some("snap-session"), tmp.path()).unwrap();

    create_snapshot(&ctx, "first", tmp.path()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(10));
    create_snapshot(&ctx, "second", tmp.path()).unwrap();

    let snaps = list_snapshots(tmp.path()).unwrap();
    assert_eq!(snaps.len(), 2);
    // Most recent first
    assert_eq!(snaps[0].name, "second");
    assert_eq!(snaps[1].name, "first");
}

// ── Snapshot restore tests ──

#[test]
fn test_restore_snapshot_success() {
    let tmp = TempDir::new().unwrap();
    let ctx = resolve_session(Some("snap-session"), tmp.path()).unwrap();

    // Write a turn log
    std::fs::write(ctx.turn_path(1), r#"{"type":"text_delta","text":"hello"}"#).unwrap();
    create_snapshot(&ctx, "restore-me", tmp.path()).unwrap();

    // Restore into a new session
    let restored = restore_snapshot("restore-me", tmp.path()).unwrap();
    assert_ne!(restored.session_id, "snap-session");
    assert!(!restored.is_new_session);
    assert!(restored.turn_path(1).exists());

    // Verify turn log content was copied
    let content = std::fs::read_to_string(restored.turn_path(1)).unwrap();
    assert!(content.contains("hello"));
}

#[test]
fn test_restore_snapshot_not_found() {
    let tmp = TempDir::new().unwrap();
    let err = restore_snapshot("no-such-snapshot", tmp.path()).unwrap_err();
    assert!(matches!(err, AgentError::ConfigError(_)), "expected ConfigError for missing snapshot, got {:?}", err);
}

// ── Snapshot delete tests ──

#[test]
fn test_delete_snapshot_success() {
    let tmp = TempDir::new().unwrap();
    let ctx = resolve_session(Some("snap-session"), tmp.path()).unwrap();

    create_snapshot(&ctx, "del-me", tmp.path()).unwrap();
    assert_eq!(list_snapshots(tmp.path()).unwrap().len(), 1);

    delete_snapshot("del-me", tmp.path()).unwrap();
    assert_eq!(list_snapshots(tmp.path()).unwrap().len(), 0);
}

#[test]
fn test_delete_snapshot_not_found() {
    let tmp = TempDir::new().unwrap();
    let err = delete_snapshot("no-such-snapshot", tmp.path()).unwrap_err();
    assert!(matches!(err, AgentError::ConfigError(_)), "expected ConfigError for missing snapshot, got {:?}", err);
}

// ── SessionContext base_dir test ──

#[test]
fn test_session_context_stores_base_dir() {
    let tmp = TempDir::new().unwrap();
    let ctx = resolve_session(Some("base-test"), tmp.path()).unwrap();
    assert_eq!(ctx.base_dir, tmp.path());
    assert!(ctx.session_dir.starts_with(&ctx.base_dir));
}

#[test]
fn test_session_context_messages_jsonl_path() {
    let tmp = TempDir::new().unwrap();
    let ctx = resolve_session(Some("mj-path"), tmp.path()).unwrap();
    assert_eq!(ctx.messages_jsonl_path(), ctx.session_dir.join("messages.jsonl"));
}

// ── Phase 1: messages.jsonl persistence tests ──

/// Build a minimal ChatMessage list for testing (no System messages).
fn sample_messages() -> Vec<ChatMessage> {
    vec![
        ChatMessage::User { content: "hello".to_string(), images: vec![], ephemeral: false, synthetic: false },
        ChatMessage::Assistant {
            content: Some("hi there".to_string()),
            reasoning_content: Some("thinking...".to_string()),
            thinking_signature: Some("sig123".to_string()),
            tool_calls: None,
        },
    ]
}

#[test]
fn test_persist_and_load_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();

    let msgs = sample_messages();
    persist_window_messages(dir, &msgs).unwrap();

    let loaded = load_session_messages(dir).unwrap();
    assert_eq!(loaded.len(), 2);

    // User message round-trips.
    assert!(matches!(&loaded[0], ChatMessage::User { content, .. } if content == "hello"));

    // Assistant: reasoning_content and thinking_signature are stripped.
    match &loaded[1] {
        ChatMessage::Assistant { content, reasoning_content, thinking_signature, tool_calls } => {
            assert_eq!(content.as_deref(), Some("hi there"));
            assert!(reasoning_content.is_none(), "reasoning_content should be stripped");
            assert!(thinking_signature.is_none(), "thinking_signature should be stripped");
            assert!(tool_calls.is_none());
        },
        other => panic!("expected Assistant, got {:?}", other),
    }
}

#[test]
fn test_persist_skips_prompt_and_ephemeral_only() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();

    let msgs = vec![
        ChatMessage::System { content: "system prompt".to_string(), ephemeral: false },
        ChatMessage::User { content: "ephemeral ask".to_string(), images: vec![], ephemeral: true, synthetic: false },
        ChatMessage::System { content: "context window info".to_string(), ephemeral: false },
        ChatMessage::User { content: "real question".to_string(), images: vec![], ephemeral: false, synthetic: false },
        ChatMessage::System { content: "reminder".to_string(), ephemeral: true },
    ];
    persist_window_messages(dir, &msgs).unwrap();

    let loaded = load_session_messages(dir).unwrap();
    assert_eq!(loaded.len(), 2, "prompt + ephemeral rows filtered, window context kept");
    assert!(
        matches!(&loaded[0], ChatMessage::System { content, .. } if content == "context window info"),
        "mid-list System row is window context and must survive: {:?}",
        loaded[0]
    );
    assert!(matches!(&loaded[1], ChatMessage::User { content, .. } if content == "real question"));
}

/// Regression fixture shaped from the 3-row window archives of session
/// 20261103_4206f951: a window that was resumed mid-flight came back
/// without its `<context_window>`, hint slot or guidance, because both
/// the persist and the load side stripped every System row. The whole
/// rotation skeleton except the prompt must survive the roundtrip —
/// `resume_session` re-bakes the prompt around it.
#[test]
fn test_window_context_survives_resume_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();

    let msgs = vec![
        ChatMessage::system("You are phimint, a coding agent."),
        ChatMessage::system("<context_window>budget 200k, used 40k</context_window>"),
        ChatMessage::system("<thread_hint>Sow the nursing detail page.</thread_hint>"),
        ChatMessage::user("sow the nursing detail page"),
        ChatMessage::system("Use notes/history tools silently to maintain continuity."),
        ChatMessage::user_synthetic("Your previous context window was archived."),
    ];
    persist_window_messages(dir, &msgs).unwrap();

    let loaded = load_session_messages(dir).unwrap();
    assert_eq!(loaded.len(), 5, "everything but the leading prompt survives");

    let texts: Vec<&str> = loaded
        .iter()
        .map(|m| match m {
            ChatMessage::System { content, .. } | ChatMessage::User { content, .. } => content.as_str(),
            other => panic!("unexpected message {:?}", other),
        })
        .collect();
    assert_eq!(texts[0], "<context_window>budget 200k, used 40k</context_window>");
    assert_eq!(texts[1], "<thread_hint>Sow the nursing detail page.</thread_hint>");
    assert_eq!(texts[2], "sow the nursing detail page");
    assert_eq!(texts[3], "Use notes/history tools silently to maintain continuity.");
    assert_eq!(texts[4], "Your previous context window was archived.");

    // Provenance survives persistence: the trail stays real, the seed
    // stays synthetic, so a later rotation still knows which is which.
    assert!(matches!(&loaded[2], ChatMessage::User { synthetic: false, ephemeral: false, .. }));
    assert!(matches!(&loaded[4], ChatMessage::User { synthetic: true, ephemeral: false, .. }));
}

#[test]
fn test_load_skips_malformed_lines() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    let path = dir.join("messages.jsonl");

    // Mix valid and invalid lines.
    let mut file = File::create(&path).unwrap();
    writeln!(file, r#"{{"User":{{"content":"good","images":[]}}}}"#).unwrap();
    writeln!(file, "NOT VALID JSON").unwrap();
    writeln!(file).unwrap(); // empty line
    writeln!(file, r#"{{"User":{{"content":"also good","images":[]}}}}"#).unwrap();

    let loaded = load_session_messages(dir).unwrap();
    assert_eq!(loaded.len(), 2, "malformed + empty lines should be skipped");
}

#[test]
fn test_load_patches_dangling_tool_calls() {
    use agent_base::llm_trait::ToolCallMessage;

    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();

    // Assistant with two tool_calls, only one answered.
    let msgs = vec![
        ChatMessage::User { content: "run tools".to_string(), images: vec![], ephemeral: false, synthetic: false },
        ChatMessage::Assistant {
            content: None,
            reasoning_content: None,
            thinking_signature: None,
            tool_calls: Some(vec![
                ToolCallMessage {
                    id: "tc_answered".to_string(),
                    name: "read_file".to_string(),
                    arguments: "{}".to_string(),
                },
                ToolCallMessage {
                    id: "tc_dangling".to_string(),
                    name: "write_file".to_string(),
                    arguments: "{}".to_string(),
                },
            ]),
        },
        ChatMessage::Tool {
            tool_call_id: "tc_answered".to_string(),
            name: Some("read_file".to_string()),
            content: "file content".to_string(),
            images: Vec::new(),
        },
        // tc_dangling has no response — simulates a cancelled turn.
    ];
    persist_window_messages(dir, &msgs).unwrap();

    let loaded = load_session_messages(dir).unwrap();
    // Should have: User, Assistant, Tool(answered), Tool(interrupted patch).
    assert_eq!(loaded.len(), 4, "expected 4 messages including patched Tool");

    match &loaded[3] {
        ChatMessage::Tool { tool_call_id, content, .. } => {
            assert_eq!(tool_call_id, "tc_dangling");
            assert_eq!(content, "interrupted");
        },
        other => panic!("expected patched Tool message, got {:?}", other),
    }
}

#[test]
fn test_persist_atomic_crash_preserves_old_file() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();

    // Write an initial valid file.
    let original =
        vec![ChatMessage::User { content: "original".to_string(), images: vec![], ephemeral: false, synthetic: false }];
    persist_window_messages(dir, &original).unwrap();

    // Simulate a partial write by writing a truncated tmp and NOT renaming.
    let path = messages_jsonl_path(dir);
    let tmp_path = path.with_extension("jsonl.tmp");
    std::fs::write(&tmp_path, "TRUNCATED").unwrap();

    // The real file should still be intact.
    let loaded = load_session_messages(dir).unwrap();
    assert_eq!(loaded.len(), 1);
    assert!(matches!(&loaded[0], ChatMessage::User { content, .. } if content == "original"));
}

#[test]
fn test_clear_messages_jsonl() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();

    persist_window_messages(dir, &sample_messages()).unwrap();
    assert!(messages_jsonl_path(dir).exists());

    clear_messages_jsonl(dir).unwrap();
    assert!(!messages_jsonl_path(dir).exists());
}

#[test]
fn test_clear_messages_jsonl_noop_when_missing() {
    let tmp = TempDir::new().unwrap();
    // Should not error when the file doesn't exist.
    clear_messages_jsonl(tmp.path()).unwrap();
}

#[test]
fn test_load_returns_empty_when_no_file() {
    let tmp = TempDir::new().unwrap();
    let loaded = load_session_messages(tmp.path()).unwrap();
    assert!(loaded.is_empty());
}

#[test]
fn test_persist_empty_messages() {
    let tmp = TempDir::new().unwrap();
    persist_window_messages(tmp.path(), &[]).unwrap();

    let loaded = load_session_messages(tmp.path()).unwrap();
    assert!(loaded.is_empty());
}

#[test]
fn test_load_strips_reasoning_on_all_assistants() {
    use agent_base::llm_trait::ToolCallMessage;

    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();

    let msgs = vec![
        ChatMessage::Assistant {
            content: Some("first".to_string()),
            reasoning_content: Some("r1".to_string()),
            thinking_signature: Some("s1".to_string()),
            tool_calls: Some(vec![ToolCallMessage {
                id: "tc1".to_string(),
                name: "tool".to_string(),
                arguments: "{}".to_string(),
            }]),
        },
        ChatMessage::Tool {
            tool_call_id: "tc1".to_string(),
            name: Some("tool".to_string()),
            content: "ok".to_string(),
            images: Vec::new(),
        },
        ChatMessage::Assistant {
            content: Some("second".to_string()),
            reasoning_content: Some("r2".to_string()),
            thinking_signature: Some("s2".to_string()),
            tool_calls: None,
        },
    ];
    persist_window_messages(dir, &msgs).unwrap();

    let loaded = load_session_messages(dir).unwrap();
    for msg in &loaded {
        if let ChatMessage::Assistant { reasoning_content, thinking_signature, .. } = msg {
            assert!(reasoning_content.is_none(), "reasoning_content should be stripped");
            assert!(thinking_signature.is_none(), "thinking_signature should be stripped");
        }
    }
}

// ── extract_session_title tests ──

#[test]
fn extract_title_from_messages_jsonl() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("s1");
    std::fs::create_dir_all(&dir).unwrap();
    let msg = serde_json::json!({"User":{"content":"帮我写个函数","images":[]}});
    std::fs::write(dir.join("messages.jsonl"), format!("{}\n", msg)).unwrap();
    assert_eq!(extract_session_title(&dir, tmp.path()), "帮我写个函数");
}

#[test]
fn extract_title_truncates_long_content() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("s1");
    std::fs::create_dir_all(&dir).unwrap();
    let long_msg = "a".repeat(100);
    let msg = serde_json::json!({"User":{"content":long_msg,"images":[]}});
    std::fs::write(dir.join("messages.jsonl"), format!("{}\n", msg)).unwrap();
    let title = extract_session_title(&dir, tmp.path());
    assert!(title.ends_with("..."), "expected truncation, got: {}", title);
}

#[test]
fn extract_title_fallback_to_turn_log() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("fallback");
    std::fs::create_dir_all(&dir).unwrap();
    let sys = serde_json::json!({"System":{"content":"sys"}});
    std::fs::write(dir.join("messages.jsonl"), format!("{}\n", sys)).unwrap();
    let turn = serde_json::json!({"turn":1,"timestamp":"2026-09-10T10:00:00Z","user_input":"from turn log"});
    std::fs::write(dir.join("turn_001.jsonl"), format!("{}\n", turn)).unwrap();
    assert_eq!(extract_session_title(&dir, tmp.path()), "from turn log");
}

#[test]
fn extract_title_empty_when_nothing() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(extract_session_title(tmp.path(), tmp.path()), "");
}

// ── list_sessions tests ──

#[test]
fn list_sessions_finds_resumable() {
    let tmp = TempDir::new().unwrap();
    for id in &["s1", "s2"] {
        let dir = tmp.path().join("sessions").join(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("session_meta.json"), "{}").unwrap();
        let msg = serde_json::json!({"User":{"content":"hello","images":[]}});
        std::fs::write(dir.join("messages.jsonl"), format!("{}\n", msg)).unwrap();
    }
    let entries = list_sessions(tmp.path(), None);
    assert_eq!(entries.len(), 2);
}

#[test]
fn list_sessions_skips_current() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("sessions").join("cur");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("session_meta.json"), "{}").unwrap();
    let msg = serde_json::json!({"User":{"content":"hello","images":[]}});
    std::fs::write(dir.join("messages.jsonl"), format!("{}\n", msg)).unwrap();
    assert_eq!(list_sessions(tmp.path(), Some("cur")).len(), 0);
}

/// A context rotation used to delete `messages.jsonl` after archiving, so
/// a session that rotated and then died had history on disk but was
/// listed nowhere. Evidence beyond `messages.jsonl` has to count.
#[test]
fn list_sessions_includes_history_archive_only_sessions() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("sessions").join("arch");
    std::fs::create_dir_all(&dir).unwrap();
    // No messages.jsonl, no turn logs — only the archive.
    let windows = tmp.path().join("history").join("arch").join("windows");
    std::fs::create_dir_all(&windows).unwrap();
    let rec = serde_json::json!({"window":1,"index":0,"role":"user","content":"archived opener","ts":"t"});
    std::fs::write(windows.join("001.jsonl"), format!("{}\n", rec)).unwrap();

    let entries = list_sessions(tmp.path(), None);
    assert_eq!(entries.len(), 1, "archive-only session must be resumable");
    assert_eq!(entries[0].session_id, "arch");
    assert_eq!(entries[0].title, "archived opener");
}

#[test]
fn list_sessions_includes_turn_log_only_sessions() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("sessions").join("turns");
    std::fs::create_dir_all(&dir).unwrap();
    let turn = serde_json::json!({"turn":1,"timestamp":"t","user_input":"started but never finished"});
    std::fs::write(dir.join("turn_001.jsonl"), format!("{}\n", turn)).unwrap();

    let entries = list_sessions(tmp.path(), None);
    assert_eq!(entries.len(), 1, "turn-log-only session must be resumable");
    assert_eq!(entries[0].title, "started but never finished");
}

#[test]
fn list_sessions_still_skips_sessions_with_no_evidence() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("sessions").join("empty")).unwrap();
    assert_eq!(list_sessions(tmp.path(), None).len(), 0);
}

// ── workspace (home directory) recording ──

#[test]
fn new_session_records_its_birthplace_workspace() {
    let tmp = TempDir::new().unwrap();
    let (dir, is_new) = get_or_create_session_dir("born-here", tmp.path()).unwrap();
    assert!(is_new);

    let expected = std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap();
    assert_eq!(read_session_cwd(&dir), Some(expected));
}

/// The birthplace is written once. Resuming a session from another
/// directory must not migrate it into that directory's list.
#[test]
fn reuse_never_overwrites_the_workspace() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("sessions").join("already-home");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("session_meta.json"), r#"{"cwd":"/original/place"}"#).unwrap();

    let (dir, is_new) = get_or_create_session_dir("already-home", tmp.path()).unwrap();
    assert!(!is_new);
    assert_eq!(read_session_cwd(&dir), Some(PathBuf::from("/original/place")));
}

/// Sessions that predate the field keep no workspace rather than being
/// backfilled from wherever they happen to be reopened.
#[test]
fn legacy_session_stays_untagged() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("sessions").join("legacy");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("session_meta.json"), r#"{"session_id":"legacy"}"#).unwrap();

    let (dir, _) = get_or_create_session_dir("legacy", tmp.path()).unwrap();
    assert_eq!(read_session_cwd(&dir), None);
}

/// A birthplace recorded at creation is labelled as such, so a later
/// recovery pass can never be confused with the real thing.
#[test]
fn a_birthplace_recorded_at_creation_says_so() {
    let tmp = TempDir::new().unwrap();
    let (dir, _) = get_or_create_session_dir("born-here", tmp.path()).unwrap();
    assert_eq!(read_session_cwd_source(&dir), Some(CwdSource::Recorded));
}

#[test]
fn a_recovered_birthplace_is_distinguishable() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("sessions").join("recovered");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("session_meta.json"), r#"{"session_id":"recovered"}"#).unwrap();

    write_session_cwd(&dir, Some(Path::new("/inferred/home")), CwdSource::Recovered);
    assert_eq!(read_session_cwd(&dir), Some(PathBuf::from("/inferred/home")));
    assert_eq!(read_session_cwd_source(&dir), Some(CwdSource::Recovered));
}

/// Declining to guess is recorded too — otherwise every launch would
/// re-read every session that has no defensible home.
#[test]
fn an_unrecoverable_session_is_marked_not_retried() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("sessions").join("hopeless");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("session_meta.json"), r#"{"session_id":"hopeless"}"#).unwrap();

    write_session_cwd(&dir, None, CwdSource::Unrecoverable);
    assert_eq!(read_session_cwd(&dir), None);
    assert_eq!(read_session_cwd_source(&dir), Some(CwdSource::Unrecoverable));
}

#[test]
fn list_sessions_exposes_the_workspace() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("sessions").join("tagged");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("session_meta.json"), r#"{"session_id":"tagged","cwd":"/work/repo"}"#).unwrap();
    let msg = serde_json::json!({"User":{"content":"hello","images":[]}});
    std::fs::write(dir.join("messages.jsonl"), format!("{}\n", msg)).unwrap();

    let entries = list_sessions(tmp.path(), None);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].cwd, Some(PathBuf::from("/work/repo")));
}

#[test]
fn extract_title_falls_back_to_history_archive() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("sessions").join("arch");
    std::fs::create_dir_all(&dir).unwrap();
    let windows = tmp.path().join("history").join("arch").join("windows");
    std::fs::create_dir_all(&windows).unwrap();
    let rec = serde_json::json!({"window":1,"index":0,"role":"user","content":"from the archive","ts":"t"});
    std::fs::write(windows.join("001.jsonl"), format!("{}\n", rec)).unwrap();
    assert_eq!(extract_session_title(&dir, tmp.path()), "from the archive");
}

/// A recovered transcript can carry tool results whose originating
/// `tool_calls` did not survive. `validate_message_sequence` rejects an
/// unanswered Tool outright, so without dropping them the sessions that
/// most need recovering fail to resume at all.
#[test]
fn load_drops_orphan_tool_results() {
    let tmp = TempDir::new().unwrap();
    let orphan = ChatMessage::Tool {
        tool_call_id: "never-issued".into(),
        name: Some("shell".into()),
        content: "output with no caller".into(),
        images: Vec::new(),
    };
    let paired = (
        ChatMessage::Assistant {
            content: None,
            reasoning_content: None,
            thinking_signature: None,
            tool_calls: Some(vec![agent_base::llm_trait::ToolCallMessage {
                id: "tc_ok".into(),
                name: "shell".into(),
                arguments: "{}".into(),
            }]),
        },
        ChatMessage::Tool {
            tool_call_id: "tc_ok".into(),
            name: Some("shell".into()),
            content: "real output".into(),
            images: Vec::new(),
        },
    );
    let msgs = vec![ChatMessage::user("do the thing"), orphan, paired.0, paired.1];
    persist_window_messages(tmp.path(), &msgs).unwrap();

    let loaded = load_session_messages(tmp.path()).unwrap();
    assert!(
        !loaded.iter().any(|m| matches!(m, ChatMessage::Tool { tool_call_id, .. } if tool_call_id == "never-issued")),
        "orphan tool result must be dropped, got: {loaded:?}"
    );
    assert!(
        loaded.iter().any(|m| matches!(m, ChatMessage::Tool { tool_call_id, .. } if tool_call_id == "tc_ok")),
        "paired tool result must be kept, got: {loaded:?}"
    );
}
