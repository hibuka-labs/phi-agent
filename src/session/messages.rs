use std::fs::File;
use std::io::{self, BufRead, Write as _};
use std::path::{Path, PathBuf};

use agent_base::ChatMessage;

// ── messages.jsonl persistence (resume support) ──

/// Path to the `messages.jsonl` file inside a session directory.
pub(super) fn messages_jsonl_path(session_dir: &Path) -> PathBuf {
    session_dir.join("messages.jsonl")
}

/// Strip stale reasoning metadata from an Assistant message so that a resumed
/// conversation never replays stale `reasoning_content` / `thinking_signature`.
fn strip_stale_fields(msg: &mut ChatMessage) {
    if let ChatMessage::Assistant { reasoning_content, thinking_signature, .. } = msg {
        *reasoning_content = None;
        *thinking_signature = None;
    }
}

/// Persist the current window's full `ChatMessage` list as a JSONL snapshot.
///
/// Writes atomically (tmp → rename) so a crash mid-write never corrupts the
/// existing file.  Skipped: ephemeral messages, and the leading `System` row
/// (the system prompt — the resumed session bakes a fresh one).  Mid-list
/// non-ephemeral `System` rows are **kept**: context rotation stores the
/// window context (`<context_window>`, the hint slot, guidance) that way, and
/// stripping them here erased the handoff on every process restart (the
/// 3-row window archives of session 20261103_4206f951).
pub fn persist_window_messages(session_dir: &Path, messages: &[ChatMessage]) -> io::Result<()> {
    let path = messages_jsonl_path(session_dir);
    let tmp = path.with_extension("jsonl.tmp");

    let mut file = File::create(&tmp)?;
    for (i, mut msg) in messages.iter().cloned().enumerate() {
        match &msg {
            ChatMessage::System { ephemeral: true, .. } => continue,
            ChatMessage::User { ephemeral: true, .. } => continue,
            // The system prompt is position zero of the engine's message
            // list; the resumed session re-bakes it, so it must not be
            // replayed alongside the fresh copy.
            ChatMessage::System { .. } if i == 0 => continue,
            _ => {},
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
/// - Keep `System` messages: the file never carries the system prompt (see
///   [`persist_window_messages`]), so any `System` row in it is window
///   context — the rotation skeleton's `<context_window>`, hint slot and
///   guidance. Dropping those here erased the handoff whenever a window was
///   resumed mid-flight (the 3-row window archives of session
///   20261103_4206f951); the fresh prompt is prepended by
///   [`crate::agent::PhiAgent::resume_session`] around them.
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
            },
        };
        strip_stale_fields(&mut msg);
        messages.push(msg);
    }

    // Drop Tool messages whose call id was never issued by a preceding
    // Assistant. A recovered or archived transcript can carry tool results
    // whose originating `tool_calls` did not survive, and
    // `validate_message_sequence` rejects an unanswered Tool outright — so
    // without this the sessions that most need recovering fail to resume at
    // all. `retain` runs left to right, so each id is claimed by the
    // Assistant that issued it and a duplicate Tool answer is dropped too
    // (that is exactly what the validator would reject).
    let mut issued: std::collections::HashSet<String> = std::collections::HashSet::new();
    let before = messages.len();
    messages.retain(|msg| match msg {
        ChatMessage::Assistant { tool_calls: Some(tcs), .. } => {
            for tc in tcs {
                issued.insert(tc.id.clone());
            }
            true
        },
        ChatMessage::Tool { tool_call_id, .. } => issued.remove(tool_call_id),
        _ => true,
    });
    let dropped = before - messages.len();
    if dropped > 0 {
        tracing::info!(dropped, "dropped orphan tool results with no issuing tool_call");
    }

    // Patch dangling tool calls at the tail.  A cancelled turn may leave an
    // Assistant with `tool_calls` but no matching Tool response — synthesize
    // an "interrupted" Tool message so `validate_message_sequence` passes.
    let mut pending: Vec<String> = Vec::new();
    for msg in &messages {
        match msg {
            ChatMessage::Assistant { tool_calls: Some(tcs), .. } => {
                pending = tcs.iter().map(|t| t.id.clone()).collect();
            },
            ChatMessage::Tool { tool_call_id, .. } => {
                pending.retain(|id| id != tool_call_id);
            },
            _ => {},
        }
    }
    for id in pending {
        tracing::info!(tool_call_id = %id, "patching dangling tool_call with interrupted response");
        messages.push(ChatMessage::Tool {
            tool_call_id: id,
            name: None,
            content: "interrupted".to_string(),
            images: Vec::new(),
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
