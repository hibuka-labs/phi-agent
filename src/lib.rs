//! phi-agent: Rust AI Agent runtime framework — orchestration, sessions, streaming all built-in.
//! You only define tools, prompts, and domain knowledge.
//!
//! Built on agent-base and agent-works, providing builder factory, renderer,
//! config resolution, session management, and other infrastructure.
//! **Ships with zero application tools.** Kernel tools (file I/O, shell,
//! multi-agent) are available via `phi-kernel-tools` behind feature flags —
//! all off by default. Application tools are injected by consumers.

#![warn(missing_docs)]

pub mod agent;
pub mod bridge;
pub mod cli;
pub mod config;
pub mod event_log;
/// System prompt generation (EN/CN).
pub mod prompt;
pub mod render;
/// Session management — ID resolution, locking, snapshots, cleanup.
pub mod session;

// ── Common agent-base types ──
// Only re-export the types consumers use most often.
// For the full type set, import directly from agent-base.
//
// Note: AgentBuilder re-exports agent_works::AgentBuilder (not agent_base::AgentBuilder)
// because phi-agent is a full-stack framework that includes multi-agent, skills, MCP, etc.
// For the bare runtime builder, use agent_base::AgentBuilder directly.
pub use agent_base::{
    AgentError, AgentResult, AgentRuntime, ApprovalDecision, ApprovalHandler, ApprovalRequest,
    ConsecutiveFailureRecovery, Content, FinishReason, Middleware, PlanItem, PlanStepStatus, PostLlmCtx, PreLlmCtx,
    ReasoningConfig, ReasoningEffort, RiskLevel, RunOutcome, RuntimeEvent, SafetyConfig, SessionId, Tool, ToolContext,
    ToolMetadata, ToolPolicy, TurnFactMiddleware, TurnToolLimitMiddleware, UpdatePlanTool, UserMessageCtx,
};
pub use agent_works::AgentBuilder;

// ── phi-telemetry (metrics types and storage) ──
#[cfg(feature = "telemetry")]
pub use phi_telemetry::{
    SessionMetrics, SessionOutcome, SessionSummary, TurnMetrics, TurnOutcome, list_all_metrics, load_metrics,
    save_metrics, try_load_metrics,
};

// ── agent-works ──
#[cfg(feature = "focus")]
pub use agent_works::focus::{Context as FocusContext, Focus, FocusError, FocusInput, FocusOutput};
#[cfg(feature = "multi-agent")]
pub use agent_works::multi_agent::{ChildPermissionMode, MultiAgentConfig};

// ── MCP (feature-gated) ──
#[cfg(feature = "mcp")]
pub use agent_works::mcp::{McpServeConfig, McpServer, McpServerConfig, McpServerTransport, McpTransport};

// ── phi-agent types ──
#[cfg(feature = "compression")]
pub use agent::CompressionMiddleware;
pub use agent::{
    PhiAgent, PhiAgentConfig, base_agent_builder, base_agent_builder_with_excludes, base_agent_builder_with_options,
    clear_compression_cache, run_compact_session,
};
pub use cli::{ApprovalMode, AutoApprovalHandler};
pub use config::{LlmConfig, resolve_llm_config};
pub use event_log::{event_to_jsonl, event_to_value, save_turn_log};
pub use prompt::{build_system_prompt, build_system_prompt_cn};
pub use render::{
    EventRenderer, JsonStreamRenderer, NullRenderer, OutputFormat, create_renderer, create_stdout_renderer,
};
pub use session::{
    SessionContext, SnapshotInfo, cleanup_expired_sessions, create_snapshot, delete_snapshot, list_snapshots,
    resolve_session, restore_snapshot, validate_session_id, validate_snapshot_name,
};

/// Format a number with K/M suffixes for display.
///
/// - `n < 1_000` → plain number
/// - `1_000 <= n < 1_000_000` → `{:.1}K`
/// - `n >= 1_000_000` → `{:.1}M`
pub fn format_number(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod proptests {
    use super::*;

    proptest::proptest! {
        #[test]
        fn format_number_never_panics(n: u64) {
            let _ = format_number(n);
        }

        #[test]
        fn format_number_correct_suffix(n: u64) {
            let s = format_number(n);
            if n < 1_000 {
                proptest::prop_assert_eq!(s, n.to_string());
            } else if n < 1_000_000 {
                proptest::prop_assert!(s.ends_with('K'), "expected K suffix for {}, got '{}'", n, s);
            } else {
                proptest::prop_assert!(s.ends_with('M'), "expected M suffix for {}, got '{}'", n, s);
            }
        }

        #[test]
        fn format_number_boundary_correct(n in 0u64..1_001) {
            // At the exact boundary (1000), must switch to K
            let s = format_number(n);
            if n < 1_000 {
                proptest::prop_assert!(!s.contains('K'));
                proptest::prop_assert!(!s.contains('M'));
            } else {
                proptest::prop_assert!(s.ends_with('K'));
            }
        }
    }
}
