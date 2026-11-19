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
mod env_fixture {
    use std::sync::{Mutex, MutexGuard};

    /// Every environment variable `resolve_llm_config` consults.
    pub(super) const TOUCHED: &[&str] =
        &["LLM_API_KEY", "OPENAI_API_KEY", "LLM_MODEL", "OPENAI_MODEL", "LLM_BASE_URL", "OPENAI_BASE_URL"];

    static SERIALIZED: Mutex<()> = Mutex::new(());

    /// Serialize tests that mutate process-global environment state.
    pub(super) fn guard() -> MutexGuard<'static, ()> {
        SERIALIZED.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Snapshot the listed variables, clear them, restore on drop.
    pub(super) struct Cleared {
        vars: Vec<&'static str>,
        prior: Vec<Option<String>>,
    }

    impl Cleared {
        pub(super) fn take(vars: &[&'static str]) -> Self {
            let prior = vars.iter().map(|k| std::env::var(k).ok()).collect();
            for k in vars {
                unsafe { std::env::remove_var(k) };
            }
            Self { vars: vars.to_vec(), prior }
        }
    }

    impl Drop for Cleared {
        fn drop(&mut self) {
            for (i, k) in self.vars.iter().enumerate() {
                unsafe { std::env::remove_var(k) };
                if let Some(v) = &self.prior[i] {
                    unsafe { std::env::set_var(k, v) };
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::env_fixture::{Cleared, TOUCHED, guard};
    use super::*;

    fn put(key: &str, value: &str) {
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
    fn explicit_model_argument_beats_all_env_sources() {
        let _lock = guard();
        let _wipe = Cleared::take(TOUCHED);
        put("LLM_API_KEY", "test-key");
        put("LLM_MODEL", "env-model");

        let config = resolve_llm_config(Some("cli-model"), None).expect("config should resolve");

        assert_eq!(config.model, "cli-model");
    }

    #[test]
    fn llm_prefixed_env_wins_over_openai_prefixed_env() {
        let _lock = guard();
        let _wipe = Cleared::take(TOUCHED);
        put("LLM_API_KEY", "test-key");
        put("LLM_MODEL", "llm-model");
        put("OPENAI_MODEL", "openai-model");
        put("LLM_BASE_URL", "https://llm.example.com/v1");
        put("OPENAI_BASE_URL", "https://openai.example.com/v1");

        let config = resolve_llm_config(None, None).expect("config should resolve");

        assert_eq!(config.model, "llm-model");
        assert_eq!(config.base_url, "https://llm.example.com/v1");
    }

    #[test]
    fn api_key_prefers_llm_namespace_over_openai_namespace() {
        let _lock = guard();
        let _wipe = Cleared::take(TOUCHED);
        put("LLM_API_KEY", "llm-key");
        put("OPENAI_API_KEY", "openai-key");

        let config = resolve_llm_config(None, None).expect("config should resolve");

        assert_eq!(config.api_key, "llm-key");
    }

    #[test]
    fn openai_model_env_fills_in_when_llm_model_absent() {
        let _lock = guard();
        let _wipe = Cleared::take(TOUCHED);
        put("LLM_API_KEY", "test-key");
        put("OPENAI_MODEL", "openai-model");

        let config = resolve_llm_config(None, None).expect("config should resolve");

        assert_eq!(config.model, "openai-model");
    }

    #[test]
    fn built_in_defaults_apply_when_no_model_or_url_set() {
        let _lock = guard();
        let _wipe = Cleared::take(TOUCHED);
        put("LLM_API_KEY", "test-key");

        let config = resolve_llm_config(None, None).expect("config should resolve");

        assert_eq!(config.model, "copilot");
        assert_eq!(config.base_url, "https://api.openai.com/v1");
    }

    #[test]
    fn absent_api_key_names_the_variable_in_the_error() {
        let _lock = guard();
        let _wipe = Cleared::take(TOUCHED);

        let error = resolve_llm_config(None, None).expect_err("missing API key should fail");

        assert!(error.to_string().contains("LLM_API_KEY"));
    }
}

#[cfg(test)]
mod proptests {
    use super::env_fixture::{Cleared, TOUCHED, guard};
    use super::*;

    proptest::proptest! {
        #[test]
        fn resolve_llm_config_never_panics(
            model in proptest::option::of("[a-zA-Z0-9_.-]{0,50}"),
            base_url in proptest::option::of("https://[a-z]{1,20}\\.example\\.com/v[0-9]"),
        ) {
            let _lock = guard();
            let _wipe = Cleared::take(TOUCHED);
            unsafe { std::env::set_var("LLM_API_KEY", "sk-proptest"); }
            let _ = resolve_llm_config(model.as_deref(), base_url.as_deref());
        }
    }
}
