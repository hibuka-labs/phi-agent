//! Session management: identity, directory lifecycle, message persistence,
//! listing, snapshots, and workspace provenance.
//!
//! The public surface lives here and is re-exported from the submodules;
//! each submodule owns one responsibility.

mod context;
mod cwd;
mod listing;
mod messages;
mod snapshots;

#[cfg(test)]
mod proptests;
#[cfg(test)]
mod tests;

pub use context::{
    SessionContext, acquire_session_lock, cleanup_expired_sessions, generate_session_id, get_or_create_session_dir,
    read_session_title, resolve_session, resolve_session_id, validate_session_id, write_session_title,
};
pub use cwd::{CwdSource, read_session_cwd, read_session_cwd_source, write_session_cwd};
pub use listing::{SessionInfo, list_sessions};
pub use messages::{clear_messages_jsonl, load_session_messages, persist_window_messages};
pub use snapshots::{
    SnapshotInfo, create_snapshot, delete_snapshot, list_snapshots, restore_snapshot, validate_snapshot_name,
};
