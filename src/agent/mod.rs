//! Agent construction and lifecycle management.
//!
//! This module provides:
//! - [`base_agent_builder`] — a pre-configured `AgentBuilder` factory
//! - [`PhiAgent`] — a thin wrapper around `AgentRuntime` with a simplified API
//! - [`PhiAgentConfig`] — tool-agnostic agent configuration
//! - Context compression middleware (feature-gated)

pub mod builder;
/// LLM-based context compression middleware for long sessions.
pub mod compression;
/// PhiAgent struct and configuration — the primary public API.
pub mod factory;

#[cfg(feature = "compression")]
pub use agent_works::compression::CompressionMiddleware;
pub use builder::{base_agent_builder, base_agent_builder_with_excludes, clear_compression_cache, run_compact_session};
pub use compression::{CompressionConfig, SummarizingMiddleware};
pub use factory::{PhiAgent, PhiAgentConfig};
