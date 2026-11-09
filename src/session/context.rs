use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use agent_base::{AgentError, AgentResult};
use fs2::FileExt;
use regex::Regex;

use super::cwd::{CwdSource, write_session_cwd};

static SESSION_ID_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9\-_]+$").unwrap());

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
    pub(super) _lock: Option<File>,
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

    // Birthplace: the working directory the session was first opened in.
    // Recorded once, at creation — a session keeps its home workspace for
    // life, so resuming it from somewhere else must not move it into that
    // workspace's list. Sessions that predate this field stay untagged here;
    // a product layer may file them later from other evidence, but never
    // from wherever they happen to be reopened.
    if is_new && let Some(cwd) = current_workspace_dir() {
        write_session_cwd(&session_dir, Some(&cwd), CwdSource::Recorded);
    }

    Ok((session_dir, is_new))
}

/// The home workspace of a session about to be created: the process working
/// directory, canonicalized so a symlinked launch path and its real path land
/// in the same bucket.
fn current_workspace_dir() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    Some(std::fs::canonicalize(&cwd).unwrap_or(cwd))
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
