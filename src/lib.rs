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
    AgentError, AgentResult, AgentRuntime, ApprovalHandler, ApprovalRequest, ConsecutiveFailureRecovery, LlmClient,
    Middleware, OpenAiClient, PlanItem, PlanStepStatus, PostLlmCtx, PreLlmCtx, ReasoningConfig, ReasoningEffort,
    RunOutcome, RuntimeEvent, SafetyConfig, SessionId, Tool, ToolContext, ToolControlFlow, ToolMetadata, ToolOutput,
    ToolPolicy, TurnFactMiddleware, TurnToolLimitMiddleware, UpdatePlanTool, UserMessageCtx,
};
pub use agent_works::AgentBuilder;

// ── phi-telemetry (metrics types and storage) ──
#[cfg(feature = "telemetry")]
pub use phi_telemetry::{
    SessionMetrics, SessionOutcome, SessionSummary, TurnMetrics, TurnOutcome, list_all_metrics, load_metrics,
    save_metrics, try_load_metrics,
};

// ── agent-works ──
pub use agent_works::focus::{Context as FocusContext, Focus, FocusError, FocusInput, FocusOutput};

// ── MCP (feature-gated) ──
#[cfg(feature = "mcp")]
pub use agent_works::mcp::{McpServeConfig, McpServer, McpServerConfig, McpServerTransport, McpTransport};

// ── phi-agent types ──
pub use agent::{CompressionConfig, PhiAgent, PhiAgentConfig, SummarizingMiddleware, base_agent_builder};
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
