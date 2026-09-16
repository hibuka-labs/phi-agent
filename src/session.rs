use std::fs::File;
use std::io::{self, BufRead, Write as _};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::SystemTime;

use agent_base::{AgentError, AgentResult, ChatMessage};
use fs2::FileExt;
use regex::Regex;

static SESSION_ID_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9\-_]+$").unwrap());
static SNAPSHOT_NAME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9\-_]+$").unwrap());

/// Session context — holds the session ID, directory, base directory, and file lock.
///
/// Created via [`resolve_session`]. The lock is released when the struct is dropped.
#[derive(Debug)]
pub struct SessionContext {
    /// Session identifier (user-provided or auto-generated).
    pub session_id: String,
    /// Path to the session directory on disk.
    pub session_dir: PathBuf,
    /// Base directory for all session data.
    pub base_dir: PathBuf,
    /// Whether this session was just created (vs reused).
    pub is_new_session: bool,
    _lock: Option<File>,
}

impl SessionContext {
    /// Path to the session ID marker file.
    #[allow(dead_code)]
    pub fn session_id_path(&self) -> PathBuf {
        self.session_dir.join("session_id")
    }

    /// Path to the session metadata JSON file.
    pub fn metadata_path(&self) -> PathBuf {
        self.session_dir.join("session_meta.json")
    }

    /// Path to the session log file (human-readable).
    pub fn log_path(&self) -> PathBuf {
        self.session_dir.join("session.log")
    }

    /// Path to the per-turn JSONL event log. `turn` is 1-indexed.
    pub fn turn_path(&self, turn: usize) -> PathBuf {
        self.session_dir.join(format!("turn_{:03}.jsonl", turn))
    }

    /// Path to the `messages.jsonl` window snapshot (one `ChatMessage` per line).
    pub fn messages_jsonl_path(&self) -> PathBuf {
        self.session_dir.join("messages.jsonl")
    }

    /// Highest turn number already logged for this session (0 if none).
    ///
    /// Scans `session_dir` for `turn_NNN.jsonl` files and returns the max `N`.
    /// Consumers that persist per-turn logs should initialize their turn
    /// counter from this value (then increment) so that reusing a session id
    /// across process runs *continues* the turn sequence instead of resetting
    /// to 1 and appending a later run's `turn_001.jsonl` into an earlier one.
    pub fn last_turn_number(&self) -> u32 {
        let Ok(rd) = std::fs::read_dir(&self.session_dir) else {
            return 0;
        };
        let mut max = 0u32;
        for entry in rd.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(n) =
                name.strip_prefix("turn_").and_then(|s| s.strip_suffix(".jsonl")).and_then(|s| s.parse::<u32>().ok())
            {
                max = max.max(n);
            }
        }
        max
    }
}

/// Validate session ID format.
///
/// Allowed: alphanumerics, hyphens, underscores. 1–128 characters.
pub fn validate_session_id(session_id: &str) -> AgentResult<()> {
    if session_id.is_empty() || session_id.len() > 128 {
        return Err(AgentError::config_error(format!("Session ID must be 1-128 characters, got {}", session_id.len())));
    }

    if !SESSION_ID_RE.is_match(session_id) {
        return Err(AgentError::config_error(format!(
            "Invalid session_id format '{}'. Only alphanumeric, hyphens, and underscores allowed.",
            session_id
        )));
    }

    Ok(())
}

/// Validate snapshot name format.
///
/// Allowed: alphanumerics, hyphens, underscores. 1–64 characters.
/// No path separators or traversal sequences allowed.
pub fn validate_snapshot_name(name: &str) -> AgentResult<()> {
    if name.is_empty() || name.len() > 64 {
        return Err(AgentError::config_error(format!("Snapshot name must be 1-64 characters, got {}", name.len())));
    }

    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(AgentError::config_error(format!(
            "Invalid snapshot name '{}': path separators and '..' are not allowed",
            name
        )));
    }

    if !SNAPSHOT_NAME_RE.is_match(name) {
        return Err(AgentError::config_error(format!(
            "Invalid snapshot name '{}'. Only alphanumeric, hyphens, and underscores allowed.",
            name
        )));
    }

    Ok(())
}

/// Resolve session ID (priority: CLI arg → `PHI_SESSION_ID` env var → auto-generate).
pub fn resolve_session_id(cli_session_id: Option<&str>) -> AgentResult<String> {
    if let Some(id) = cli_session_id {
        validate_session_id(id)?;
        return Ok(id.to_string());
    }

    if let Ok(id) = std::env::var("PHI_SESSION_ID")
        && !id.is_empty()
    {
        validate_session_id(&id)?;
        return Ok(id);
    }

    Ok(generate_session_id())
}

/// Generate a new session_id (format: YYYYMMDD_first8ofUuid)
pub fn generate_session_id() -> String {
    let now = chrono::Local::now();
    let uuid = uuid::Uuid::new_v4().to_string();
    let uuid_short = &uuid[..8.min(uuid.len())];
    format!("{}_{}", now.format("%Y%m%d"), uuid_short)
}

/// Get or create a session directory under `base_dir/sessions/<session_id>`.
///
/// Returns the directory path and whether it was newly created.
pub fn get_or_create_session_dir(session_id: &str, base_dir: &Path) -> AgentResult<(PathBuf, bool)> {
    let session_dir = base_dir.join("sessions").join(session_id);
    let is_new = !session_dir.exists();

    if is_new {
        std::fs::create_dir_all(&session_dir)?;
        tracing::info!(session_id = %session_id, path = %session_dir.display(), "created new session directory");
    } else {
        tracing::info!(session_id = %session_id, path = %session_dir.display(), "reusing existing session directory");
    }

    // Write session_id file
    std::fs::write(session_dir.join("session_id"), session_id)?;

    // Update session_meta.json
    update_session_meta(&session_dir, session_id)?;

    Ok((session_dir, is_new))
}

/// Acquire an exclusive file lock on the session directory.
///
/// Prevents concurrent access from other processes. Returns an error if the
/// session is already in use.
pub fn acquire_session_lock(session_dir: &Path) -> AgentResult<File> {
    let lock_path = session_dir.join("session.lock");
    let file = File::create(&lock_path)?;

    file.try_lock_exclusive().map_err(|_| {
        AgentError::resource_unavailable(format!(
            "Session '{}' is currently in use by another process",
            session_dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
        ))
    })?;

    Ok(file)
}

/// Update session_meta.json
fn update_session_meta(session_dir: &Path, session_id: &str) -> AgentResult<()> {
    let meta_path = session_dir.join("session_meta.json");

    let mut meta = if meta_path.exists() {
        let content = std::fs::read_to_string(&meta_path)?;
        serde_json::from_str::<serde_json::Value>(&content)?
    } else {
        serde_json::json!({
            "session_id": session_id,
            "created_at": chrono::Utc::now().to_rfc3339(),
        })
    };

    meta["last_active_at"] = serde_json::json!(chrono::Utc::now().to_rfc3339());

    std::fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)?;
    Ok(())
}

/// Read the LLM-generated title from `session_meta.json`.
///
/// Returns `None` if the file is missing, malformed, or has no `title` field.
pub fn read_session_title(session_dir: &Path) -> Option<String> {
    let meta_path = session_dir.join("session_meta.json");
    let content = std::fs::read_to_string(&meta_path).ok()?;
    let meta: serde_json::Value = serde_json::from_str(&content).ok()?;
    meta["title"].as_str().filter(|s| !s.is_empty()).map(String::from)
}

/// Write a session title into `session_meta.json`.
///
/// `generated = true` means the title was produced by LLM at the 10th-user-message
/// threshold and should NOT be regenerated on subsequent exits.
/// `generated = false` means it's a tentative title (e.g., produced on early exit)
/// that will be regenerated if the session continues to 10 user messages.
pub fn write_session_title(session_dir: &Path, title: &str, generated: bool) -> io::Result<()> {
    let meta_path = session_dir.join("session_meta.json");
    let mut meta = if meta_path.exists() {
        let content = std::fs::read_to_string(&meta_path)?;
        serde_json::from_str::<serde_json::Value>(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    meta["title"] = serde_json::json!(title);
    meta["title_generated"] = serde_json::json!(generated);

    std::fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)?;
    Ok(())
}

/// Clean up expired sessions.
///
/// Sessions inactive for more than `max_age_days` are removed from disk.
/// Active (locked) are skipped.
pub fn cleanup_expired_sessions(base_dir: &Path, max_age_days: i64) -> AgentResult<u32> {
    let sessions_dir = base_dir.join("sessions");
    if !sessions_dir.exists() {
        return Ok(0);
    }

    let now = chrono::Utc::now();
    let mut cleaned = 0;

    for entry in std::fs::read_dir(&sessions_dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let lock_path = path.join("session.lock");
        if lock_path.exists()
            && let Ok(file) = File::open(&lock_path)
            && file.try_lock_shared().is_err()
        {
            continue; // locked, skip
        }

        let meta_path = path.join("session_meta.json");
        if !meta_path.exists() {
            std::fs::remove_dir_all(&path)?;
            cleaned += 1;
            continue;
        }

        let content = std::fs::read_to_string(&meta_path)?;
        let meta: serde_json::Value = serde_json::from_str(&content)?;

        if let Some(last_active) = meta["last_active_at"].as_str()
            && let Ok(last_active) = chrono::DateTime::parse_from_rfc3339(last_active)
        {
            let age = now - last_active.with_timezone(&chrono::Utc);
            if age.num_days() > max_age_days {
                tracing::info!(path = %path.display(), age_days = age.num_days(), "removing expired session");
                std::fs::remove_dir_all(&path)?;
                cleaned += 1;
            }
        }
    }

    if cleaned > 0 {
        tracing::info!(count = cleaned, "cleaned up expired sessions");
    }

    Ok(cleaned)
}

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
}

/// List resumable sessions under `base_dir/sessions/`.
///
/// Scans session directories, filters out locked / empty / current sessions,
/// and returns entries sorted by `messages.jsonl` mtime descending (newest first).
/// Any framework consumer can use this to present a session history list.
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

        let session_id = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

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

        // messages.jsonl must exist and contain at least one message.
        let messages_path = path.join("messages.jsonl");
        if !messages_path.exists() {
            continue;
        }

        let title = extract_session_title(&path);
        if title.is_empty() {
            continue;
        }

        let mtime = std::fs::metadata(&messages_path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);

        entries.push(SessionInfo {
            session_id,
            title,
            last_active_at: mtime,
            session_dir: path,
        });
    }

    entries.sort_by_key(|e| std::cmp::Reverse(e.last_active_at));
    entries
}

/// Extract a display title for a session.
///
/// Priority: LLM-generated title → first User message → turn_001 user_input → empty.
fn extract_session_title(session_dir: &Path) -> String {
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
                && let Some(content) = msg
                    .get("User")
                    .and_then(|u| u.get("content"))
                    .and_then(|c| c.as_str())
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

// ── messages.jsonl persistence (resume support) ──

/// Path to the `messages.jsonl` file inside a session directory.
fn messages_jsonl_path(session_dir: &Path) -> PathBuf {
    session_dir.join("messages.jsonl")
}

/// Strip stale reasoning metadata from an Assistant message so that a resumed
/// conversation never replays stale `reasoning_content` / `thinking_signature`.
fn strip_stale_fields(msg: &mut ChatMessage) {
    if let ChatMessage::Assistant {
        reasoning_content,
        thinking_signature,
        ..
    } = msg
    {
        *reasoning_content = None;
        *thinking_signature = None;
    }
}

/// Persist the current window's full `ChatMessage` list as a JSONL snapshot.
///
/// Writes atomically (tmp → rename) so a crash mid-write never corrupts the
/// existing file.  System and ephemeral messages are skipped — the resumed
/// session will use a fresh system prompt.
pub fn persist_window_messages(session_dir: &Path, messages: &[ChatMessage]) -> io::Result<()> {
    let path = messages_jsonl_path(session_dir);
    let tmp = path.with_extension("jsonl.tmp");

    let mut file = File::create(&tmp)?;
    for mut msg in messages.iter().cloned() {
        match &msg {
            ChatMessage::System { .. } => continue,
            ChatMessage::User { ephemeral: true, .. } => continue,
            _ => {}
        }
        strip_stale_fields(&mut msg);
        serde_json::to_writer(&mut file, &msg)?;
        file.write_all(b"\n")?;
    }
    file.flush()?;
    drop(file);

    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Load a previously-persisted `messages.jsonl` and sanitize for resume.
///
/// Sanitization:
/// - Skip empty lines and malformed JSON (logged + skipped, not fatal).
/// - Drop any `System` messages (the resumed session uses a fresh system prompt).
/// - Strip `reasoning_content` / `thinking_signature` from Assistant messages.
/// - Patch dangling tool calls at the tail: if the last Assistant message has
///   `tool_calls` that were never answered, synthesize a `Tool` response with
///   content `"interrupted"` so that `validate_message_sequence` passes.
pub fn load_session_messages(session_dir: &Path) -> io::Result<Vec<ChatMessage>> {
    let path = messages_jsonl_path(session_dir);
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = File::open(&path)?;
    let reader = io::BufReader::new(file);
    let mut messages = Vec::new();

    for line_result in reader.lines() {
        let line = line_result?;
        if line.trim().is_empty() {
            continue;
        }
        let mut msg: ChatMessage = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(error = %e, "skipping malformed messages.jsonl line");
                continue;
            }
        };
        // Drop System messages — resume prepends a fresh system prompt.
        if matches!(msg, ChatMessage::System { .. }) {
            continue;
        }
        strip_stale_fields(&mut msg);
        messages.push(msg);
    }

    // Patch dangling tool calls at the tail.  A cancelled turn may leave an
    // Assistant with `tool_calls` but no matching Tool response — synthesize
    // an "interrupted" Tool message so `validate_message_sequence` passes.
    let mut pending: Vec<String> = Vec::new();
    for msg in &messages {
        match msg {
            ChatMessage::Assistant {
                tool_calls: Some(tcs), ..
            } => {
                pending = tcs.iter().map(|t| t.id.clone()).collect();
            }
            ChatMessage::Tool { tool_call_id, .. } => {
                pending.retain(|id| id != tool_call_id);
            }
            _ => {}
        }
    }
    for id in pending {
        tracing::info!(tool_call_id = %id, "patching dangling tool_call with interrupted response");
        messages.push(ChatMessage::Tool {
            tool_call_id: id,
            name: None,
            content: "interrupted".to_string(),
        });
    }

    Ok(messages)
}

/// Delete the `messages.jsonl` file if it exists.
///
/// Called after a context-rotation window is archived — the archived window
/// already contains the messages, so keeping `messages.jsonl` would be
/// redundant and could cause double-injection on resume.
pub fn clear_messages_jsonl(session_dir: &Path) -> io::Result<()> {
    let path = messages_jsonl_path(session_dir);
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

// ── Session snapshots (Phase 6.3) ──

/// Create a named snapshot of a session.
///
/// Copies the session directory to `base_dir/snapshots/<name>/`.
/// Returns the snapshot path.
pub fn create_snapshot(session_ctx: &SessionContext, name: &str, base_dir: &Path) -> AgentResult<PathBuf> {
    validate_snapshot_name(name)?;
    let snapshot_dir = base_dir.join("snapshots").join(name);

    if snapshot_dir.exists() {
        std::fs::remove_dir_all(&snapshot_dir)?;
    }
    std::fs::create_dir_all(&snapshot_dir)?;

    // Copy session metadata
    let meta_path = session_ctx.metadata_path();
    if meta_path.exists() {
        std::fs::copy(&meta_path, snapshot_dir.join("session_meta.json"))?;
    }

    // Copy all turn logs
    let mut turn_count = 0u32;
    for entry in std::fs::read_dir(&session_ctx.session_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with("turn_") && name_str.ends_with(".jsonl") {
            std::fs::copy(entry.path(), snapshot_dir.join(&*name_str))?;
            turn_count += 1;
        }
    }

    // Write snapshot info
    let info = serde_json::json!({
        "session_id": session_ctx.session_id,
        "snapshot_name": name,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "turn_count": turn_count,
    });
    std::fs::write(snapshot_dir.join("snapshot_info.json"), serde_json::to_string_pretty(&info)?)?;

    tracing::info!(
        session_id = %session_ctx.session_id,
        name = name,
        turns = turn_count,
        "session snapshot created"
    );

    Ok(snapshot_dir)
}

/// List all saved snapshots under `base_dir/snapshots/`.
///
/// Returns a vector of (name, snapshot_info_json, turn_count).
pub fn list_snapshots(base_dir: &Path) -> AgentResult<Vec<SnapshotInfo>> {
    let snapshots_dir = base_dir.join("snapshots");
    if !snapshots_dir.exists() {
        return Ok(Vec::new());
    }

    let mut snapshots = Vec::new();
    for entry in std::fs::read_dir(&snapshots_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let info_path = path.join("snapshot_info.json");
        if !info_path.exists() {
            continue;
        }

        let content = std::fs::read_to_string(&info_path)?;
        let info: serde_json::Value = serde_json::from_str(&content)?;

        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let session_id = info["session_id"].as_str().unwrap_or("-").to_string();
        let created_at = info["created_at"].as_str().unwrap_or("-").to_string();
        let turn_count = info["turn_count"].as_u64().unwrap_or(0) as u32;

        snapshots.push(SnapshotInfo { name, session_id, created_at, turn_count, path });
    }

    snapshots.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(snapshots)
}

/// Info about a saved snapshot.
pub struct SnapshotInfo {
    /// Snapshot name.
    pub name: String,
    /// Original session ID this snapshot was taken from.
    pub session_id: String,
    /// ISO-8601 timestamp of when the snapshot was created.
    pub created_at: String,
    /// Number of turns captured in the snapshot.
    pub turn_count: u32,
    /// Path to the snapshot directory on disk.
    pub path: PathBuf,
}

/// Restore a session from a snapshot.
///
/// Creates a new session from the snapshot data. Returns the new session context.
pub fn restore_snapshot(name: &str, base_dir: &Path) -> AgentResult<SessionContext> {
    validate_snapshot_name(name)?;
    let snapshot_dir = base_dir.join("snapshots").join(name);
    if !snapshot_dir.exists() {
        return Err(AgentError::config_error(format!("Snapshot '{}' not found at {}", name, snapshot_dir.display())));
    }

    // Generate new session ID and directory
    let new_session_id = generate_session_id();
    let (new_session_dir, _) = get_or_create_session_dir(&new_session_id, base_dir)?;

    // Copy turn logs from snapshot
    for entry in std::fs::read_dir(&snapshot_dir)? {
        let entry = entry?;
        let fname = entry.file_name();
        let name_str = fname.to_string_lossy();
        if name_str.starts_with("turn_") && name_str.ends_with(".jsonl") {
            std::fs::copy(entry.path(), new_session_dir.join(&*name_str))?;
        }
    }

    let lock = acquire_session_lock(&new_session_dir)?;
    tracing::info!(from = name, to = %new_session_id, "session restored from snapshot");

    Ok(SessionContext {
        session_id: new_session_id,
        session_dir: new_session_dir,
        base_dir: base_dir.to_path_buf(),
        is_new_session: false,
        _lock: Some(lock),
    })
}

/// Delete a saved snapshot.
pub fn delete_snapshot(name: &str, base_dir: &Path) -> AgentResult<()> {
    validate_snapshot_name(name)?;
    let snapshot_dir = base_dir.join("snapshots").join(name);
    if !snapshot_dir.exists() {
        return Err(AgentError::config_error(format!("Snapshot '{}' not found", name)));
    }
    std::fs::remove_dir_all(&snapshot_dir)?;
    tracing::info!(name = name, "snapshot deleted");
    Ok(())
}

/// Resolve and create a session context — session ID, directory, and file lock.
///
/// This is the primary entry point for session setup. It combines ID resolution,
/// directory creation, and lock acquisition into a single call.
pub fn resolve_session(cli_session_id: Option<&str>, base_dir: &Path) -> AgentResult<SessionContext> {
    let session_id = resolve_session_id(cli_session_id)?;
    let (session_dir, is_new) = get_or_create_session_dir(&session_id, base_dir)?;
    let lock = acquire_session_lock(&session_dir)?;

    Ok(SessionContext {
        session_id,
        session_dir,
        base_dir: base_dir.to_path_buf(),
        is_new_session: is_new,
        _lock: Some(lock),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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
        assert!(
            matches!(err, AgentError::ConfigError(_)),
            "expected ConfigError for invalid session ID, got {:?}",
            err
        );
    }

    #[test]
    fn test_resolve_session_returns_config_error_for_invalid_id() {
        let tmp = TempDir::new().unwrap();
        let result = resolve_session(Some("bad id with spaces"), tmp.path());
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(
            matches!(err, AgentError::ConfigError(_)),
            "expected ConfigError for invalid session ID, got {:?}",
            err
        );
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
            ChatMessage::User {
                content: "hello".to_string(),
                images: vec![],
                ephemeral: false,
            },
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
            }
            other => panic!("expected Assistant, got {:?}", other),
        }
    }

    #[test]
    fn test_persist_skips_system_and_ephemeral() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        let msgs = vec![
            ChatMessage::System {
                content: "system prompt".to_string(),
                ephemeral: false,
            },
            ChatMessage::User {
                content: "ephemeral ask".to_string(),
                images: vec![],
                ephemeral: true,
            },
            ChatMessage::User {
                content: "real question".to_string(),
                images: vec![],
                ephemeral: false,
            },
        ];
        persist_window_messages(dir, &msgs).unwrap();

        let loaded = load_session_messages(dir).unwrap();
        assert_eq!(loaded.len(), 1, "System + ephemeral User should be filtered");
        assert!(matches!(&loaded[0], ChatMessage::User { content, .. } if content == "real question"));
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
        writeln!(file, "").unwrap(); // empty line
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
            ChatMessage::User {
                content: "run tools".to_string(),
                images: vec![],
                ephemeral: false,
            },
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
            }
            other => panic!("expected patched Tool message, got {:?}", other),
        }
    }

    #[test]
    fn test_persist_atomic_crash_preserves_old_file() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        // Write an initial valid file.
        let original = vec![ChatMessage::User {
            content: "original".to_string(),
            images: vec![],
            ephemeral: false,
        }];
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
        assert_eq!(extract_session_title(&dir), "帮我写个函数");
    }

    #[test]
    fn extract_title_truncates_long_content() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("s1");
        std::fs::create_dir_all(&dir).unwrap();
        let long_msg = "a".repeat(100);
        let msg = serde_json::json!({"User":{"content":long_msg,"images":[]}});
        std::fs::write(dir.join("messages.jsonl"), format!("{}\n", msg)).unwrap();
        let title = extract_session_title(&dir);
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
        assert_eq!(extract_session_title(&dir), "from turn log");
    }

    #[test]
    fn extract_title_empty_when_nothing() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(extract_session_title(tmp.path()), "");
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
}

#[cfg(test)]
mod proptests {
    use super::*;

    proptest::proptest! {
        #[test]
        fn validate_session_id_never_panics(s in ".*") {
            let _ = validate_session_id(&s);
        }

        #[test]
        fn validate_session_id_accepts_valid_chars_only(s in "[a-zA-Z0-9_-]{1,128}") {
            proptest::prop_assert!(validate_session_id(&s).is_ok(), "valid session_id '{}' should pass", s);
        }

        #[test]
        fn validate_snapshot_name_rejects_path_traversal(s in ".*") {
            let result = validate_snapshot_name(&s);
            if s.contains('/') || s.contains('\\') || s.contains("..") {
                proptest::prop_assert!(result.is_err(), "snapshot name '{}' with path traversal should fail", s);
            }
        }

        #[test]
        fn cleanup_timestamp_rfc3339_never_panics(ts in "[0-9T:+ -Zz.]{1,40}") {
            let _ = chrono::DateTime::parse_from_rfc3339(&ts);
        }
    }
}
