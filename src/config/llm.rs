//! LLM configuration types and resolution helpers.
//!
//! Supports multi-source config resolution: CLI flags > environment
//! variables > `.env` file > built-in defaults.

use agent_base::{AgentError, AgentResult};

const DEFAULT_MODEL: &str = "copilot";
const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// Resolved LLM configuration.
#[derive(Clone, Debug)]
pub struct LlmConfig {
    /// API key for the LLM provider.
    pub api_key: String,
    /// Model name (e.g. `"opus"`, `"gpt-4o"`).
    pub model: String,
    /// Base URL for the LLM API endpoint.
    pub base_url: String,
}

/// Resolve LLM configuration (API key, model, base_url).
///
/// Priority: CLI arg > environment variable (.env) > default
pub fn resolve_llm_config(model: Option<&str>, base_url: Option<&str>) -> AgentResult<LlmConfig> {
    let api_key =
        super::optional_env("LLM_API_KEY").or_else(|| super::optional_env("OPENAI_API_KEY")).ok_or_else(|| {
            AgentError::config_error("Missing environment variable LLM_API_KEY. Please configure it in .env.")
        })?;

    let resolved_model = model
        .map(|s| s.to_string())
        .or_else(|| super::optional_env("LLM_MODEL"))
        .or_else(|| super::optional_env("OPENAI_MODEL"))
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());

    let resolved_base_url = base_url
        .map(|s| s.to_string())
        .or_else(|| super::optional_env("LLM_BASE_URL"))
        .or_else(|| super::optional_env("OPENAI_BASE_URL"))
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

    Ok(LlmConfig { api_key, model: resolved_model, base_url: resolved_base_url })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_config_debug_clone() {
        let cfg = LlmConfig {
            api_key: "sk-test".into(),
            model: "gpt-4".into(),
            base_url: "https://api.openai.com/v1".into(),
        };
        let cloned = cfg.clone();
        assert_eq!(cloned.api_key, "sk-test");
        assert_eq!(cloned.model, "gpt-4");
        let _ = format!("{:?}", cfg);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;

    struct EnvGuard {
        keys: Vec<&'static str>,
        saved: Vec<Option<String>>,
    }

    impl EnvGuard {
        fn new(keys: &[&'static str]) -> Self {
            let saved: Vec<Option<String>> = keys.iter().map(|k| std::env::var(k).ok()).collect();
            for k in keys {
                unsafe { std::env::remove_var(k) };
            }
            Self { keys: keys.to_vec(), saved }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (i, k) in self.keys.iter().enumerate() {
                unsafe { std::env::remove_var(k) };
                if let Some(ref v) = self.saved[i] {
                    unsafe { std::env::set_var(k, v) };
                }
            }
        }
    }

    proptest::proptest! {
        #[test]
        fn resolve_llm_config_never_panics(
            model in proptest::option::of("[a-zA-Z0-9_.-]{0,50}"),
            base_url in proptest::option::of("https://[a-z]{1,20}\\.example\\.com/v[0-9]"),
        ) {
            let vars = &["LLM_API_KEY", "OPENAI_API_KEY", "LLM_MODEL", "OPENAI_MODEL", "LLM_BASE_URL", "OPENAI_BASE_URL"];
            let _guard = EnvGuard::new(vars);
            unsafe { std::env::set_var("LLM_API_KEY", "sk-proptest"); }
            let _ = resolve_llm_config(model.as_deref(), base_url.as_deref());
        }
    }
}
