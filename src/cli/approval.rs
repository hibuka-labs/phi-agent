//! Approval strategies for tool calls.
//!
//! Provides [`AutoApprovalHandler`] which implements a pure-strategy
//! approval model — automatically approve all calls or deny all calls
//! — suitable for CLI, CI, and headless environments.
//!
//! Also provides [`QueuedApprovalHandler`] for UI-fronted runtimes: instead
//! of touching the terminal it pushes each request into a queue the UI
//! drains, and waits on a per-request oneshot for the user's decision.

use agent_base::{AgentError, AgentResult, ApprovalDecision, ApprovalHandler, ApprovalRequest};
use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};

/// Approval strategy.
#[derive(Clone, Debug)]
pub enum ApprovalMode {
    /// Automatically approve all tool calls without user interaction.
    Auto,
    /// Reject all tool calls that require approval.
    DenyAll,
}

/// I/O-free approval handler — pure strategy, suitable for all consumers.
///
/// Use [`ApprovalMode::Auto`] for automated/CI scenarios, or
/// [`ApprovalMode::DenyAll`] for read-only / preview modes.
pub struct AutoApprovalHandler {
    mode: ApprovalMode,
}

impl AutoApprovalHandler {
    /// Create a new handler with the given approval strategy.
    pub fn new(mode: ApprovalMode) -> Self {
        Self { mode }
    }
}

#[async_trait]
impl ApprovalHandler for AutoApprovalHandler {
    async fn approve(
        &self,
        request: ApprovalRequest,
        _cancel_token: tokio_util::sync::CancellationToken,
    ) -> AgentResult<ApprovalDecision> {
        match &self.mode {
            ApprovalMode::Auto => {
                tracing::info!(decision = "AllowAlways", tool = %request.title, "auto-approved");
                Ok(ApprovalDecision::AllowAlways)
            },
            ApprovalMode::DenyAll => {
                tracing::info!(decision = "Deny", tool = %request.title, "auto-denied");
                Ok(ApprovalDecision::Deny)
            },
        }
    }
}

// ── QueuedApprovalHandler ───────────────────────────────────────────────────

/// A pending approval request handed to the UI, carrying the channel the
/// user's decision is returned on.
#[derive(Debug)]
pub struct ApprovalItem {
    /// The request to render (title, message, risk level, raw args).
    pub request: ApprovalRequest,
    /// Complete this to deliver the user's decision back to the runtime.
    pub decision_tx: oneshot::Sender<ApprovalDecision>,
}

/// Queued approval handler: enqueue each request and let the UI render one
/// prompt at a time, then return the user's decision.
///
/// Unlike [`AutoApprovalHandler`] (a pure strategy) this never decides on its
/// own and never touches the terminal. It pushes the request into a queue the
/// UI drains and waits on a per-request oneshot. Parallel sub-agents each call
/// `approve`; the queue serializes them so only one prompt is shown at a time
/// (fixing interleaved multi-sub-agent prompts in REPL UIs).
#[derive(Debug, Clone)]
pub struct QueuedApprovalHandler {
    queue_tx: mpsc::UnboundedSender<ApprovalItem>,
}

impl QueuedApprovalHandler {
    /// Create a handler wired to `queue_tx`. The caller keeps the matching
    /// `UnboundedReceiver` and feeds it to the UI.
    pub fn new(queue_tx: mpsc::UnboundedSender<ApprovalItem>) -> Self {
        Self { queue_tx }
    }
}

#[async_trait]
impl ApprovalHandler for QueuedApprovalHandler {
    async fn approve(
        &self,
        request: ApprovalRequest,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> AgentResult<ApprovalDecision> {
        let (decision_tx, decision_rx) = oneshot::channel();
        let _ = self.queue_tx.send(ApprovalItem { request, decision_tx });

        tokio::select! {
            _ = cancel_token.cancelled() => Err(AgentError::Cancelled),
            result = decision_rx => result.map_err(|_| AgentError::Cancelled),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_base::{ApprovalRequest, RiskLevel};

    #[tokio::test]
    async fn test_auto_mode_approves_all() {
        let handler = AutoApprovalHandler::new(ApprovalMode::Auto);
        let request = ApprovalRequest {
            title: "Delete file".into(),
            message: "This will delete /tmp/important.txt".into(),
            action_key: None,
            risk_level: RiskLevel::Destructive,
            raw: None,
        };
        let cancel = tokio_util::sync::CancellationToken::new();
        let decision = handler.approve(request, cancel).await.unwrap();
        assert_eq!(decision, ApprovalDecision::AllowAlways);
    }

    #[tokio::test]
    async fn test_deny_all_mode_denies() {
        let handler = AutoApprovalHandler::new(ApprovalMode::DenyAll);
        let request = ApprovalRequest {
            title: "Read file".into(),
            message: "Read /tmp/safe.txt".into(),
            action_key: None,
            risk_level: RiskLevel::Safe,
            raw: None,
        };
        let cancel = tokio_util::sync::CancellationToken::new();
        let decision = handler.approve(request, cancel).await.unwrap();
        assert_eq!(decision, ApprovalDecision::Deny);
    }

    #[tokio::test]
    async fn test_deny_all_denies_even_safe_operations() {
        let handler = AutoApprovalHandler::new(ApprovalMode::DenyAll);
        let request = ApprovalRequest {
            title: "Safe thing".into(),
            message: "Perfectly safe".into(),
            action_key: None,
            risk_level: RiskLevel::Safe,
            raw: None,
        };
        let cancel = tokio_util::sync::CancellationToken::new();
        let decision = handler.approve(request, cancel).await.unwrap();
        assert_eq!(decision, ApprovalDecision::Deny);
    }

    #[tokio::test]
    async fn queued_handler_roundtrips_decision() {
        let (queue_tx, mut queue_rx) = tokio::sync::mpsc::unbounded_channel();
        let handler = QueuedApprovalHandler::new(queue_tx);
        let cancel = tokio_util::sync::CancellationToken::new();

        let request = ApprovalRequest {
            title: "write_file".to_string(),
            message: "Write file: src/lib.rs".to_string(),
            action_key: Some("write_file:src/lib.rs".to_string()),
            risk_level: RiskLevel::Sensitive,
            raw: None,
        };

        let handle = {
            let handler = handler.clone();
            let request = request.clone();
            let cancel = cancel.clone();
            tokio::spawn(async move { handler.approve(request, cancel).await })
        };

        // The UI side receives the queued item and answers "allow always".
        let item = queue_rx.recv().await.expect("request should be queued");
        assert_eq!(item.request.title, "write_file");
        item.decision_tx
            .send(ApprovalDecision::AllowAlways)
            .expect("UI should be able to answer");

        let decision = handle.await.expect("handler task").expect("approve");
        assert_eq!(decision, ApprovalDecision::AllowAlways);
    }

    #[tokio::test]
    async fn queued_handler_respects_cancel() {
        let (queue_tx, mut queue_rx) = tokio::sync::mpsc::unbounded_channel();
        let handler = QueuedApprovalHandler::new(queue_tx);
        let cancel = tokio_util::sync::CancellationToken::new();

        let request = ApprovalRequest {
            title: "t".to_string(),
            message: "m".to_string(),
            action_key: None,
            risk_level: RiskLevel::Safe,
            raw: None,
        };

        let handle = {
            let handler = handler.clone();
            let request = request.clone();
            let cancel = cancel.clone();
            tokio::spawn(async move { handler.approve(request, cancel).await })
        };

        let _item = queue_rx.recv().await.expect("request queued");
        cancel.cancel();

        let result = handle.await.expect("handler task");
        assert!(matches!(result, Err(AgentError::Cancelled)));
    }
}
