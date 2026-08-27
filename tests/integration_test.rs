//! Integration tests for phi-agent.
//!
//! These tests cover the public API without requiring a real LLM connection.

mod common;
use common::EmptyStream;

use std::sync::Arc;

use agent_base::llm_trait::response::FinishReason;
use agent_base::llm_trait::{Capabilities, ChatRequest, ChatResponse, ChatStream, LlmError, LlmProvider, ProviderInfo};
use agent_base::{AgentResult, Content, ReasoningEffort, Tool, ToolContext, UsageInfo};
use async_trait::async_trait;
use phi_agent::{
    PhiAgentConfig, SafetyConfig, base_agent_builder, build_system_prompt, build_system_prompt_cn, resolve_llm_config,
    session::validate_session_id,
};
use serde_json::Value;

// ── Mock LLM client ──

/// A simple mock LLM client that always returns "mock response".
struct SimpleMockLlmClient;

#[async_trait]
impl LlmProvider for SimpleMockLlmClient {
    async fn stream(&self, _request: ChatRequest) -> Result<ChatStream, LlmError> {
        Ok(ChatStream::new(Box::pin(EmptyStream)))
    }
    async fn chat(&self, _request: ChatRequest) -> Result<ChatResponse, LlmError> {
        Ok(ChatResponse {
            content: "mock response".to_string(),
            reasoning_content: None,
            tool_calls: vec![],
            usage: UsageInfo::default(),
            finish_reason: FinishReason::Stop,
            raw: None,
            thinking_signature: None,
        })
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
    fn info(&self) -> ProviderInfo {
        ProviderInfo { name: "mock".to_string(), model: "mock-model".to_string(), version: None }
    }
}

// ── Agent builder ──

#[tokio::test(flavor = "multi_thread")]
async fn test_base_agent_builder_constructs() {
    let client = Arc::new(SimpleMockLlmClient);
    let builder = base_agent_builder(client)
        .system_prompt("You are a helpful assistant.")
        .register_tool(agent_base::UpdatePlanTool::new());
    let config = PhiAgentConfig {
        model: "test-model".into(),
        enable_thinking: false,
        thinking_budget: None,
        thinking_effort: ReasoningEffort::Medium,
        safety: SafetyConfig::default(),
        max_turns: Some(100),
    };
    let agent = phi_agent::PhiAgent::build(builder, config);
    assert!(agent.is_ok(), "Agent should build successfully");
}

// ── System prompt ──

#[test]
fn test_build_system_prompt_non_empty() {
    let prompt = build_system_prompt();
    assert!(!prompt.is_empty(), "System prompt should not be empty");
    assert!(prompt.len() > 100, "System prompt should be substantial");
}

#[test]
fn test_build_system_prompt_cn_non_empty() {
    let prompt = build_system_prompt_cn();
    assert!(!prompt.is_empty(), "Chinese system prompt should not be empty");
}

// ── Config ──

#[test]
fn test_resolve_llm_config_with_env() {
    // Without env vars set, should fall back gracefully
    let result = resolve_llm_config(None, None);
    // May error if no env vars — that's expected behavior
    // The important thing is it doesn't panic
    let _ = result;
}

// ── Session ──

#[test]
fn test_validate_session_id_valid() {
    assert!(validate_session_id("my-session-123").is_ok());
    assert!(validate_session_id("test_456").is_ok());
    assert!(validate_session_id("a").is_ok());
}

#[test]
fn test_validate_session_id_invalid() {
    assert!(validate_session_id("").is_err());
    assert!(validate_session_id("my session").is_err());
    assert!(validate_session_id("../etc").is_err());
    assert!(validate_session_id("path/traversal").is_err());
}

// ── PhiAgentConfig ──

#[test]
fn test_config_default_values() {
    let config = PhiAgentConfig {
        model: "opus".into(),
        enable_thinking: true,
        thinking_budget: Some(32000),
        thinking_effort: ReasoningEffort::High,
        safety: SafetyConfig::default(),
        max_turns: None,
    };
    assert_eq!(config.model, "opus");
    assert!(config.enable_thinking);
    assert_eq!(config.thinking_budget, Some(32000));
}

// ── Tool metadata ──

struct CustomTool;

#[async_trait]
impl Tool for CustomTool {
    fn name(&self) -> &'static str {
        "custom_tool"
    }

    fn description(&self) -> &'static str {
        "A user-defined custom tool"
    }

    fn schema(&self) -> Value {
        serde_json::json!({ "type": "object", "properties": {} })
    }

    async fn call(&self, _args: &Value, _ctx: &ToolContext) -> AgentResult<Vec<Content>> {
        Ok(vec![Content::text("ok".to_string())])
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_list_tools_returns_metadata() {
    let client = Arc::new(SimpleMockLlmClient);
    let builder = base_agent_builder(client)
        .system_prompt("You are a helpful assistant.")
        .register_tool(agent_base::UpdatePlanTool::new())
        .register_tool(CustomTool);
    let config = PhiAgentConfig {
        model: "test-model".into(),
        enable_thinking: false,
        thinking_budget: None,
        thinking_effort: ReasoningEffort::Medium,
        safety: SafetyConfig::default(),
        max_turns: Some(100),
    };
    let agent = phi_agent::PhiAgent::build(builder, config).expect("build agent");
    let tools = agent.list_tools().await;

    assert!(!tools.is_empty(), "should return at least one tool");

    // Crate-backed tool
    let update_plan = tools.iter().find(|t| t.name == "update_plan").expect("update_plan should be registered");
    assert_eq!(update_plan.origin, "agent-base", "framework tool should report its crate origin");
    assert!(!update_plan.version.is_empty(), "framework tool should report a version");
    assert!(update_plan.version != "unknown", "framework tool version should not be 'unknown'");

    // Custom tool
    let custom = tools.iter().find(|t| t.name == "custom_tool").expect("custom_tool should be registered");
    assert_eq!(custom.origin, "custom", "user-defined tool origin should be 'custom'");
    assert_eq!(custom.version, "unknown", "user-defined tool version should be 'unknown'");
    assert!(custom.description.contains("user-defined"), "description should come from definition");
}

// ── PhiAgent lifecycle ──

fn build_test_agent() -> phi_agent::PhiAgent {
    let client = Arc::new(SimpleMockLlmClient);
    let builder = base_agent_builder(client).system_prompt("You are a helpful assistant.");
    let config = PhiAgentConfig {
        model: "test-model".into(),
        enable_thinking: false,
        thinking_budget: None,
        thinking_effort: ReasoningEffort::Medium,
        safety: SafetyConfig::default(),
        max_turns: Some(10),
    };
    phi_agent::PhiAgent::build(builder, config).expect("build agent")
}

#[tokio::test(flavor = "multi_thread")]
async fn test_phi_agent_create_session() {
    let agent = build_test_agent();
    let sid = agent.create_session().await;
    assert!(sid.id > 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_phi_agent_is_cancelled_initially_false() {
    let agent = build_test_agent();
    assert!(!agent.is_cancelled());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_phi_agent_set_reasoning_effort() {
    let agent = build_test_agent();
    agent.set_reasoning_effort(ReasoningEffort::Low).await;
    // Should not panic
}

#[tokio::test(flavor = "multi_thread")]
async fn test_phi_agent_list_tools_sorted() {
    let client = Arc::new(SimpleMockLlmClient);
    let builder = base_agent_builder(client)
        .system_prompt("You are a helpful assistant.")
        .register_tool(agent_base::UpdatePlanTool::new())
        .register_tool(CustomTool);
    let config = PhiAgentConfig {
        model: "test-model".into(),
        enable_thinking: false,
        thinking_budget: None,
        thinking_effort: ReasoningEffort::Medium,
        safety: SafetyConfig::default(),
        max_turns: Some(10),
    };
    let agent = phi_agent::PhiAgent::build(builder, config).expect("build agent");
    let tools = agent.list_tools().await;
    // Should be sorted by name
    for i in 1..tools.len() {
        assert!(
            tools[i - 1].name <= tools[i].name,
            "tools should be sorted: {} > {}",
            tools[i - 1].name,
            tools[i].name
        );
    }
}

// ── Phase 1: AgentError propagation test ──

/// Verify that phi-agent functions can be used with `?` in an
/// `anyhow::Result` context — AgentError implements std::error::Error
/// so the conversion is automatic.
#[test]
fn test_agent_error_converts_to_anyhow() {
    let err = validate_session_id("");
    assert!(err.is_err());

    // AgentError implements std::error::Error, so anyhow::Error::from
    // works automatically
    let anyhow_err: anyhow::Error = err.unwrap_err().into();
    assert!(anyhow_err.to_string().contains("Session ID"));
}

// ── Phase 5: Memory prompt injection ──

#[test]
fn test_system_prompt_contains_memory_instructions() {
    let prompt = build_system_prompt();
    assert!(prompt.contains("## Memory"), "System prompt should contain Memory section");
    assert!(prompt.contains(".phi/memory/"), "System prompt should mention .phi/memory/ directory");
    assert!(prompt.contains("MEMORY.md"), "System prompt should mention MEMORY.md index");
    assert!(prompt.contains("read_file"), "System prompt should instruct LLM to use read_file for memory");
    assert!(prompt.contains("write_file"), "System prompt should instruct LLM to use write_file for memory");
}

#[test]
fn test_system_prompt_cn_also_has_memory_instructions() {
    let prompt = build_system_prompt_cn();
    assert!(prompt.contains("## Memory"), "Chinese system prompt should also contain Memory section");
    assert!(
        prompt.contains("[Network Environment]"),
        "Chinese system prompt should retain network environment section"
    );
}

// ── Phase 5: File tools registration ──

#[cfg(feature = "file")]
#[tokio::test(flavor = "multi_thread")]
async fn test_base_agent_builder_registers_file_tools() {
    let client = Arc::new(SimpleMockLlmClient);
    let builder = base_agent_builder(client).system_prompt("test");

    let config = PhiAgentConfig {
        model: "test-model".into(),
        enable_thinking: false,
        thinking_budget: None,
        thinking_effort: ReasoningEffort::Medium,
        safety: SafetyConfig::default(),
        max_turns: Some(10),
    };
    let agent = phi_agent::PhiAgent::build(builder, config).expect("build agent");
    let tools = agent.list_tools().await;

    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"read_file"), "read_file tool should be registered");
    assert!(names.contains(&"write_file"), "write_file tool should be registered");
    assert!(names.contains(&"edit_file"), "edit_file tool should be registered");
    assert!(names.contains(&"list_files"), "list_files tool should be registered");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_base_agent_builder_registers_update_plan() {
    let client = Arc::new(SimpleMockLlmClient);
    let builder = base_agent_builder(client).system_prompt("test");

    let config = PhiAgentConfig {
        model: "test-model".into(),
        enable_thinking: false,
        thinking_budget: None,
        thinking_effort: ReasoningEffort::Medium,
        safety: SafetyConfig::default(),
        max_turns: Some(10),
    };
    let agent = phi_agent::PhiAgent::build(builder, config).expect("build agent");
    let tools = agent.list_tools().await;

    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"update_plan"), "update_plan should be registered by the base builder");
}

// ── Phase 5: Skills in prompt-injection mode (no skill-specific tools) ──

#[tokio::test(flavor = "multi_thread")]
async fn test_no_skill_specific_tools_registered() {
    let client = Arc::new(SimpleMockLlmClient);
    let builder = base_agent_builder(client).system_prompt("test");

    let config = PhiAgentConfig {
        model: "test-model".into(),
        enable_thinking: false,
        thinking_budget: None,
        thinking_effort: ReasoningEffort::Medium,
        safety: SafetyConfig::default(),
        max_turns: Some(10),
    };
    let agent = phi_agent::PhiAgent::build(builder, config).expect("build agent");
    let tools = agent.list_tools().await;

    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(!names.contains(&"list_skills"), "list_skills should NOT be registered (prompt-injection mode)");
    assert!(!names.contains(&"get_skill_detail"), "get_skill_detail should NOT be registered (prompt-injection mode)");
    assert!(!names.contains(&"apply_skill"), "apply_skill should NOT be registered (prompt-injection mode)");
}

// ── Phase 5: System prompt contains skill list (LazySkillPrompter) ──

#[test]
fn test_build_system_prompt_structure() {
    let prompt = build_system_prompt();

    // Should contain core sections
    assert!(prompt.contains("[Role]"), "should have Role section");
    assert!(prompt.contains("[Execution Guidelines]"), "should have Execution Guidelines");
    assert!(prompt.contains("[File Operation Guidelines]"), "should have File Operation Guidelines");
    assert!(prompt.contains("## Memory"), "should have Memory section");

    // Sections should be separated by ---
    let sections: Vec<&str> = prompt.split("\n---\n").collect();
    assert!(sections.len() >= 2, "should have at least 2 sections separated by ---");
}
