use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use agent_base::{AgentError, AgentResult};
use regex::Regex;

use super::context::{SessionContext, acquire_session_lock, generate_session_id, get_or_create_session_dir};

static SNAPSHOT_NAME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9\-_]+$").unwrap());

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
