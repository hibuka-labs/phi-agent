use std::path::{Path, PathBuf};

/// Where a session's `cwd` came from. Stored beside `cwd` in
/// `session_meta.json` so a value stitched together after the fact is never
/// mistaken for one recorded at birth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CwdSource {
    /// Written at creation from the process working directory. Authoritative.
    Recorded,
    /// Filed later, from the paths the session touched. Best evidence
    /// available — not proof.
    Recovered,
    /// A later pass looked and found no defensible answer. Never retried:
    /// the evidence in a finished session does not change.
    Unrecoverable,
}

impl CwdSource {
    fn as_str(self) -> &'static str {
        match self {
            CwdSource::Recorded => "recorded",
            CwdSource::Recovered => "recovered",
            CwdSource::Unrecoverable => "unrecoverable",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "recorded" => Some(CwdSource::Recorded),
            "recovered" => Some(CwdSource::Recovered),
            "unrecoverable" => Some(CwdSource::Unrecoverable),
            _ => None,
        }
    }
}

/// Record a session's home workspace, and where that value came from.
///
/// `cwd` is `None` only with [`CwdSource::Unrecoverable`]: a pass looked for
/// a home and declined to guess. Writing `None` there is what keeps the pass
/// from re-reading every finished session on every launch.
///
/// Best-effort: a metadata write failure must not fail the caller.
pub fn write_session_cwd(session_dir: &Path, cwd: Option<&Path>, source: CwdSource) {
    let meta_path = session_dir.join("session_meta.json");
    let mut meta = match std::fs::read_to_string(&meta_path)
        .ok()
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
    {
        Some(meta) => meta,
        None => return,
    };
    if let Some(cwd) = cwd {
        meta["cwd"] = serde_json::json!(cwd.to_string_lossy());
    }
    meta["cwd_source"] = serde_json::json!(source.as_str());
    if let Err(e) = std::fs::write(&meta_path, serde_json::to_string_pretty(&meta).unwrap_or_default()) {
        tracing::warn!(error = %e, "failed to record session workspace");
    }
}

/// Read the session's home workspace from `session_meta.json`.
///
/// `None` for sessions created before the field existed, and for sessions a
/// recovery pass declined to file.
pub fn read_session_cwd(session_dir: &Path) -> Option<PathBuf> {
    let content = std::fs::read_to_string(session_dir.join("session_meta.json")).ok()?;
    let meta: serde_json::Value = serde_json::from_str(&content).ok()?;
    meta["cwd"].as_str().filter(|s| !s.is_empty()).map(PathBuf::from)
}

/// Where the session's `cwd` came from, if it was written with provenance.
///
/// `None` for metadata written before provenance existed — those values were
/// recorded at creation.
pub fn read_session_cwd_source(session_dir: &Path) -> Option<CwdSource> {
    let content = std::fs::read_to_string(session_dir.join("session_meta.json")).ok()?;
    let meta: serde_json::Value = serde_json::from_str(&content).ok()?;
    meta["cwd_source"].as_str().and_then(CwdSource::parse)
}
