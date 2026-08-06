//! Shared environment setup for the examples.

use phi_agent::OpenAiClient;
use std::sync::Arc;

/// LLM connection settings resolved from the environment.
pub struct LlmEnv {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
}

/// Resolve the LLM API key, model, and base URL used by the examples.
pub fn resolve_llm_env() -> LlmEnv {
    dotenvy::dotenv().ok();

    let api_key = std::env::var("LLM_API_KEY")
        .or_else(|_| std::env::var("OPENAI_API_KEY"))
        .expect("Set LLM_API_KEY or OPENAI_API_KEY environment variable");
    let model = std::env::var("LLM_MODEL").unwrap_or_else(|_| "opus".into());
    let base_url = std::env::var("LLM_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".into());

    LlmEnv { api_key, model, base_url }
}

/// Build the OpenAI-compatible client used by the examples.
pub fn client() -> Arc<OpenAiClient> {
    let env = resolve_llm_env();
    Arc::new(OpenAiClient::new(env.api_key, env.model, Some(env.base_url)))
}
