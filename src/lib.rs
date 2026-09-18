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
    AgentError, AgentResult, AgentRuntime, AllowAllApprovalHandler, ApprovalDecision, ApprovalHandler, ApprovalRequest,
    ChatMessage, CheckpointData, CheckpointStep, ConsecutiveFailureRecovery, Content, ContextCompaction,
    ContextWindowManager, DenyAllApprovalHandler, FinishReason, Language, Middleware, PlanItem, PlanStepStatus,
    PostLlmCtx, PreLlmCtx, ReasoningConfig, ReasoningEffort, RetryOnError, RiskLevel, RunOutcome, RuntimeEvent,
    SafetyConfig, SessionId, Tool, ToolContext, ToolDecision, ToolMetadata, ToolPolicy, ToolRegistry,
    TurnFactMiddleware, TurnToolLimitMiddleware, UpdatePlanTool, UserMessageCtx, estimate_messages_tokens,
    first_system_prompt,
};
// Token-budget window strategy — a pure strategy in agent-works (agent-base
// stays strategy-free: contract + primitives only).
pub use agent_works::AgentBuilder;
pub use agent_works::rotation_policy::{
    DEFAULT_SEED_MESSAGE, TokenBudgetAction, TokenBudgetConfig, TokenBudgetCore, TokenBudgetState,
    build_context_window_info, token_budget_base_overhead,
};

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
pub use agent_works::multi_agent::{
    ChildPermissionMode, ChildReport, ChildResultEvent, ControlConfig, MultiAgentConfig,
};
// Child-result fan-in delivery policy — data-only routes; the consumer renders
// the words. Sunk down from phimint's UI so any multi-agent UI reuses it.
#[cfg(feature = "multi-agent")]
pub use agent_works::multi_agent::fan_in::{ChildResultRoute, ChildResultRouter};
#[cfg(feature = "multi-agent")]
pub use agent_works::multi_agent::registry::{AgentSnapshot, RegistrySnapshot};

// ── Persistent auto-memory (Phase 9b, feature-gated) ──
/// Memory configuration for persistent auto-memory: consumer-injected policy
/// (storage root, index filename, prompt template). Hand it to
/// `AgentBuilder::memory_config` to register the four `memory_*` tools and
/// append the MEMORY.md index snapshot + tool guidance to the system prompt.
#[cfg(feature = "memory")]
pub use agent_works::memory::MemoryConfig;

// ── Framework pass-through (facade completion) ──
// These are the remaining types a product needs to name directly, so its
// Cargo.toml can depend on `phi-agent` alone. Only actually-consumed items
// are re-exported — no catch-all facade.
/// Plan / user lifecycle event types surfaced to consumers.
pub use agent_base::UserEvent;
/// Middleware that nudges the model when it nears the turn limit.
pub use agent_base::engine::max_turns_nudge::{MaxTurnsNudgeConfig, MaxTurnsNudgeMiddleware};
/// Middleware that breaks repeated identical tool-call loops (e.g. polling).
pub use agent_base::engine::repeat_tool_limit::{RepeatToolLimitConfig, RepeatToolLimitMiddleware};
/// LLM provider trait family (`LlmProvider`, `Protocol`, `config::LlmConfig`):
/// build a provider with [`create_provider`] and hand it to the runtime.
pub use agent_base::llm_trait;
/// Guard policies from agent-works (tool gating, reasoning-only enforcement).
pub use agent_works::guard::{DefaultGuard, DefaultGuardConfig, ReasoningOnlyAction};
/// LLM provider factory (resolves protocol/client from an [`llm_trait`] config).
pub use llm_unified::create_provider;

// ── Kernel tools (feature-gated) ──
#[cfg(feature = "shell")]
pub use phi_kernel_tools::local_shell::LocalShellTool;

// ── Skills (feature-gated) ──
#[cfg(feature = "skill")]
pub use agent_works::skill::{
    MAX_CATALOG_SKILLS, Skill, SkillCatalogRefreshMiddleware, SkillResolver, SkillTelemetry, SkillTool,
    catalog::demote_h2_headings, prompt_skill::PromptSkill, prompt_skill::SkillScope, refresh_catalog, render_catalog,
    strip_catalog,
};

// ── MCP (feature-gated) ──
#[cfg(feature = "mcp")]
pub use agent_works::mcp::{McpServeConfig, McpServer, McpServerConfig, McpServerTransport, McpTransport};

// ── phi-agent types ──
#[cfg(feature = "compression")]
pub use agent::CompressionMiddleware;
pub use agent::{
    PhiAgent, PhiAgentConfig, base_agent_builder, base_agent_builder_no_compression, base_agent_builder_with_excludes,
    base_agent_builder_with_options, clear_compression_cache, run_compact_session,
};
pub use agent_works::prompt::{
    DynamicToolsFragment, EnvironmentFragment, FragmentContext, PromptFragment, compose_fragments,
};
pub use cli::{ApprovalItem, ApprovalMode, AutoApprovalHandler, QueuedApprovalHandler};
pub use config::{LlmConfig, resolve_llm_config};
pub use event_log::{event_to_jsonl, event_to_value, save_turn_log};
pub use prompt::{build_system_prompt, build_system_prompt_cn, build_system_prompt_with_fragments};
pub use render::{
    EventRenderer, JsonStreamRenderer, NullRenderer, OutputFormat, create_renderer, create_stdout_renderer,
};
pub use session::{
    SessionContext, SessionInfo, SnapshotInfo, cleanup_expired_sessions, clear_messages_jsonl, create_snapshot,
    delete_snapshot, list_sessions, list_snapshots, load_session_messages, persist_window_messages, read_session_title,
    resolve_session, restore_snapshot, validate_session_id, validate_snapshot_name, write_session_title,
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
