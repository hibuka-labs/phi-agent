//! CLI helpers — approval strategies and utilities shared across consumers.

pub mod approval;

pub use approval::{ApprovalItem, ApprovalMode, AutoApprovalHandler, QueuedApprovalHandler};
