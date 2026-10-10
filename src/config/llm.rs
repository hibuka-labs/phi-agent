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
mod test_env {
    use std::sync::{Mutex, MutexGuard};

    pub(super) const LLM_ENV_VARS: &[&str] =
        &["LLM_API_KEY", "OPENAI_API_KEY", "LLM_MODEL", "OPENAI_MODEL", "LLM_BASE_URL", "OPENAI_BASE_URL"];

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    pub(super) fn lock() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(super) struct EnvGuard {
        keys: Vec<&'static str>,
        saved: Vec<Option<String>>,
    }

    impl EnvGuard {
        pub(super) fn new(keys: &[&'static str]) -> Self {
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
}

#[cfg(test)]
mod tests {
    use super::test_env::{EnvGuard, LLM_ENV_VARS, lock};
    use super::*;

    fn set_env(key: &str, value: &str) {
        unsafe { std::env::set_var(key, value) };
    }

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

    #[test]
    fn cli_model_wins_over_environment() {
        let _lock = lock();
        let _guard = EnvGuard::new(LLM_ENV_VARS);
        set_env("LLM_API_KEY", "test-key");
        set_env("LLM_MODEL", "env-model");

        let config = resolve_llm_config(Some("cli-model"), None).expect("config should resolve");

        assert_eq!(config.model, "cli-model");
    }

    #[test]
    fn llm_model_and_base_url_win_over_openai_values() {
        let _lock = lock();
        let _guard = EnvGuard::new(LLM_ENV_VARS);
        set_env("LLM_API_KEY", "test-key");
        set_env("LLM_MODEL", "llm-model");
        set_env("OPENAI_MODEL", "openai-model");
        set_env("LLM_BASE_URL", "https://llm.example.com/v1");
        set_env("OPENAI_BASE_URL", "https://openai.example.com/v1");

        let config = resolve_llm_config(None, None).expect("config should resolve");

        assert_eq!(config.model, "llm-model");
        assert_eq!(config.base_url, "https://llm.example.com/v1");
    }

    #[test]
    fn llm_api_key_wins_over_openai_api_key() {
        let _lock = lock();
        let _guard = EnvGuard::new(LLM_ENV_VARS);
        set_env("LLM_API_KEY", "llm-key");
        set_env("OPENAI_API_KEY", "openai-key");

        let config = resolve_llm_config(None, None).expect("config should resolve");

        assert_eq!(config.api_key, "llm-key");
    }

    #[test]
    fn openai_model_wins_over_default() {
        let _lock = lock();
        let _guard = EnvGuard::new(LLM_ENV_VARS);
        set_env("LLM_API_KEY", "test-key");
        set_env("OPENAI_MODEL", "openai-model");

        let config = resolve_llm_config(None, None).expect("config should resolve");

        assert_eq!(config.model, "openai-model");
    }

    #[test]
    fn model_and_base_url_use_defaults() {
        let _lock = lock();
        let _guard = EnvGuard::new(LLM_ENV_VARS);
        set_env("LLM_API_KEY", "test-key");

        let config = resolve_llm_config(None, None).expect("config should resolve");

        assert_eq!(config.model, "copilot");
        assert_eq!(config.base_url, "https://api.openai.com/v1");
    }

    #[test]
    fn missing_api_key_returns_actionable_error() {
        let _lock = lock();
        let _guard = EnvGuard::new(LLM_ENV_VARS);

        let error = resolve_llm_config(None, None).expect_err("missing API key should fail");

        assert!(error.to_string().contains("LLM_API_KEY"));
    }
}

#[cfg(test)]
mod proptests {
    use super::test_env::{EnvGuard, LLM_ENV_VARS, lock};
    use super::*;

    proptest::proptest! {
        #[test]
        fn resolve_llm_config_never_panics(
            model in proptest::option::of("[a-zA-Z0-9_.-]{0,50}"),
            base_url in proptest::option::of("https://[a-z]{1,20}\\.example\\.com/v[0-9]"),
        ) {
            let _lock = lock();
            let _guard = EnvGuard::new(LLM_ENV_VARS);
            unsafe { std::env::set_var("LLM_API_KEY", "sk-proptest"); }
            let _ = resolve_llm_config(model.as_deref(), base_url.as_deref());
        }
    }
}
