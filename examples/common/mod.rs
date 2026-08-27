//! Shared environment setup for the examples.

use std::sync::Arc;

use agent_base::llm_trait::response::FinishReason;
use agent_base::llm_trait::{Capabilities, ChatRequest, ChatResponse, ChatStream, LlmError, LlmProvider, ProviderInfo};
/// LLM connection settings resolved from the environment.
#[allow(dead_code)]
pub struct LlmEnv {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
}

/// Resolve the LLM API key, model, and base URL used by the examples.
#[allow(dead_code)]
pub fn resolve_llm_env() -> LlmEnv {
    dotenvy::dotenv().ok();

    let api_key = std::env::var("LLM_API_KEY")
        .or_else(|_| std::env::var("OPENAI_API_KEY"))
        .expect("Set LLM_API_KEY or OPENAI_API_KEY environment variable");
    let model = std::env::var("LLM_MODEL").unwrap_or_else(|_| "opus".into());
    let base_url = std::env::var("LLM_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".into());

    LlmEnv { api_key, model, base_url }
}

struct ExampleProvider;

#[async_trait::async_trait]
impl LlmProvider for ExampleProvider {
    async fn stream(&self, _request: ChatRequest) -> Result<ChatStream, LlmError> {
        Ok(ChatStream::new(Box::pin(futures_util::stream::empty())))
    }

    async fn chat(&self, _request: ChatRequest) -> Result<ChatResponse, LlmError> {
        Ok(ChatResponse {
            content: String::new(),
            reasoning_content: None,
            tool_calls: vec![],
            usage: agent_base::UsageInfo::default(),
            finish_reason: FinishReason::Stop,
            raw: None,
            thinking_signature: None,
        })
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn info(&self) -> ProviderInfo {
        ProviderInfo { name: "example".into(), model: "example".into(), version: None }
    }
}

/// Build the LLM provider used by the examples.
#[allow(dead_code)]
pub fn client() -> Arc<dyn LlmProvider> {
    Arc::new(ExampleProvider)
}
