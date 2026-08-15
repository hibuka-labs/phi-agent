//! Agent construction and lifecycle management.
//!
//! This module provides:
//! - [`base_agent_builder`] — a pre-configured `AgentBuilder` factory
//! - [`PhiAgent`] — a thin wrapper around `AgentRuntime` with a simplified API
//! - [`PhiAgentConfig`] — tool-agnostic agent configuration
//! - [`SummarizingMiddleware`] — LLM-based context compression for long sessions

pub mod builder;
/// LLM-based context compression middleware for long sessions.
pub mod compression;
/// PhiAgent struct and configuration — the primary public API.
pub mod factory;

pub use builder::{base_agent_builder, base_agent_builder_with_excludes};
pub use compression::{CompressionConfig, SummarizingMiddleware};
pub use factory::{PhiAgent, PhiAgentConfig};
