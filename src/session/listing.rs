use std::fs::File;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::context::read_session_title;
use super::cwd::read_session_cwd;

// ── session listing (for resume picker / session browser) ──

/// A session entry returned by [`list_sessions`].
#[derive(Debug)]
pub struct SessionInfo {
    /// Session ID (directory name, e.g. `"20260914_50cf809d"`).
    pub session_id: String,
    /// Display title (LLM-generated, first User message, or empty).
    pub title: String,
    /// Last activity time (`messages.jsonl` mtime).
    pub last_active_at: SystemTime,
    /// Path to the session directory.
    pub session_dir: PathBuf,
    /// Home workspace — the process cwd when the session was created.
    ///
    /// `None` for sessions created before the field existed. Listing them is
    /// still correct; only a workspace-scoped view has no place to put them.
    pub cwd: Option<PathBuf>,
}

/// List resumable sessions under `base_dir/sessions/`.
///
/// Scans session directories, filters out locked / empty / current sessions,
/// and returns entries sorted by last activity descending (newest first).
/// Any framework consumer can use this to present a session history list.
///
/// A session counts as resumable if it has **any** conversation evidence:
/// `messages.jsonl` (the normal case), turn logs, or a history archive.
/// Gating on `messages.jsonl` alone hid real sessions from the picker — a
/// context rotation used to delete that file after archiving, so a session
/// that rotated and then died was listed nowhere and its history was
/// unreachable.
pub fn list_sessions(base_dir: &Path, current_session_id: Option<&str>) -> Vec<SessionInfo> {
    let sessions_dir = base_dir.join("sessions");
    if !sessions_dir.exists() {
        return Vec::new();
    }

    let mut entries = Vec::new();

    for dir_entry in match std::fs::read_dir(&sessions_dir) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    }
    .flatten()
    {
        let path = dir_entry.path();
        if !path.is_dir() {
            continue;
        }

        let session_id = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();

        // Skip the current session.
        if Some(session_id.as_str()) == current_session_id {
            continue;
        }

        // Skip sessions that are locked by another running process.
        let lock_path = path.join("session.lock");
        if let Ok(file) = File::open(&lock_path) {
            if file.try_lock_shared().is_err() {
                continue;
            }
            drop(file);
        }

        // Any of the three evidence sources makes the session resumable.
        let messages_path = path.join("messages.jsonl");
        let has_messages = messages_path.exists();
        if !has_messages && !has_turn_logs(&path) && !has_history_archive(base_dir, &session_id) {
            continue;
        }

        let title = extract_session_title(&path, base_dir);
        if title.is_empty() {
            continue;
        }

        // Newest evidence wins: messages.jsonl is usually the freshest, but
        // for an archive-only session the turn logs / archived windows are
        // all there is to sort by.
        let mtime = std::fs::metadata(&messages_path)
            .and_then(|m| m.modified())
            .or_else(|_| newest_evidence_mtime(&path, base_dir, &session_id))
            .unwrap_or(SystemTime::UNIX_EPOCH);

        let cwd = read_session_cwd(&path);
        entries.push(SessionInfo { session_id, title, last_active_at: mtime, session_dir: path, cwd });
    }

    entries.sort_by_key(|e| std::cmp::Reverse(e.last_active_at));
    entries
}

/// Whether the session directory holds any `turn_*.jsonl` turn log.
fn has_turn_logs(session_dir: &Path) -> bool {
    std::fs::read_dir(session_dir)
        .map(|rd| {
            rd.flatten().any(|e| {
                let name = e.file_name();
                let name = name.to_string_lossy();
                name.starts_with("turn_") && name.ends_with(".jsonl")
            })
        })
        .unwrap_or(false)
}

/// Whether a context-rotation history archive exists for this session.
///
/// Directory shape only — the record schema is owned by
/// `phi_kernel_tools::context_rotation::history`.
fn has_history_archive(base_dir: &Path, session_id: &str) -> bool {
    std::fs::read_dir(base_dir.join("history").join(session_id).join("windows"))
        .map(|rd| rd.flatten().next().is_some())
        .unwrap_or(false)
}

/// Last-modified time of the newest non-`messages.jsonl` evidence.
fn newest_evidence_mtime(session_dir: &Path, base_dir: &Path, session_id: &str) -> std::io::Result<SystemTime> {
    let mut newest: Option<SystemTime> = None;
    let mut consider = |p: PathBuf| {
        if let Ok(m) = std::fs::metadata(&p)
            && let Ok(t) = m.modified()
            && newest.is_none_or(|n| t > n)
        {
            newest = Some(t);
        }
    };

    if let Ok(rd) = std::fs::read_dir(session_dir) {
        for e in rd.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("turn_") && name.ends_with(".jsonl") {
                consider(e.path());
            }
        }
    }
    if let Ok(rd) = std::fs::read_dir(base_dir.join("history").join(session_id).join("windows")) {
        for e in rd.flatten() {
            consider(e.path());
        }
    }

    newest.ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no session evidence"))
}

/// Extract a display title for a session.
///
/// Priority: LLM-generated title → first User message → turn_001 user_input →
/// first user message in the history archive → empty.
pub(super) fn extract_session_title(session_dir: &Path, base_dir: &Path) -> String {
    // 1. LLM-generated title from session_meta.json.
    if let Some(title) = read_session_title(session_dir) {
        return title;
    }

    // 2. First User message from messages.jsonl.
    let messages_path = session_dir.join("messages.jsonl");
    if let Ok(file) = File::open(&messages_path) {
        let reader = io::BufReader::new(file);
        for line in reader.lines().map_while(Result::ok) {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line)
                && let Some(content) = msg.get("User").and_then(|u| u.get("content")).and_then(|c| c.as_str())
            {
                return truncate_display(content, 60);
            }
        }
    }

    // 3. turn_001.jsonl user_input field.
    let turn_path = session_dir.join("turn_001.jsonl");
    if let Ok(file) = File::open(&turn_path) {
        let reader = io::BufReader::new(file);
        for line in reader.lines().map_while(Result::ok) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line)
                && let Some(input) = val.get("user_input").and_then(|u| u.as_str())
            {
                return truncate_display(input, 60);
            }
        }
    }

    // 4. First user message in the history archive. A context rotation used
    //    to delete messages.jsonl after archiving, so a session that rotated
    //    and then died has neither messages.jsonl nor a turn_001 user_input —
    //    the archive is the only place its opening message survives. Schema
    //    owner is `phi_kernel_tools::context_rotation::history`; this is a
    //    display title, so a schema-light peek is enough (same pattern as the
    //    turn_001 fallback above).
    if let Some(session_id) = session_dir.file_name().and_then(|n| n.to_str()) {
        let windows_dir = base_dir.join("history").join(session_id).join("windows");
        if let Ok(rd) = std::fs::read_dir(&windows_dir) {
            let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
            files.sort();
            for file in files {
                let Ok(content) = std::fs::read_to_string(&file) else { continue };
                for line in content.lines() {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(line)
                        && val.get("role").and_then(|r| r.as_str()) == Some("user")
                        && let Some(text) = val.get("content").and_then(|c| c.as_str())
                        && !text.trim().is_empty()
                    {
                        return truncate_display(text, 60);
                    }
                }
            }
        }
    }

    String::new()
}

/// Truncate display text to `max` chars, appending "..." if truncated.
fn truncate_display(s: &str, max: usize) -> String {
    let clean: String = s.split_whitespace().collect::<Vec<&str>>().join(" ");
    if clean.chars().count() <= max {
        clean
    } else {
        let truncated: String = clean.chars().take(max).collect();
        format!("{}...", truncated)
    }
}
