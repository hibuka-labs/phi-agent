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

    /// All env-var tests run sequentially in one function to avoid races.
    #[test]
    fn test_env_var_resolution_chain() {
        let vars = &["LLM_API_KEY", "OPENAI_API_KEY", "LLM_MODEL", "OPENAI_MODEL", "LLM_BASE_URL", "OPENAI_BASE_URL"];
        let _guard = EnvGuard::new(vars);

        let set = |k: &str, v: &str| unsafe { std::env::set_var(k, v) };
        let rm = |k: &str| unsafe { std::env::remove_var(k) };

        // 1. Error when no API key set at all
        assert!(resolve_llm_config(None, None).is_err());
        // Verify it returns AgentError::ConfigError so callers can match the variant
        let api_err = resolve_llm_config(None, None).unwrap_err();
        assert!(matches!(api_err, agent_base::AgentError::ConfigError(_)));

        // 2. LLM_API_KEY only
        set("LLM_API_KEY", "sk-llm");
        let cfg = resolve_llm_config(None, None).unwrap();
        assert_eq!(cfg.api_key, "sk-llm");
        assert_eq!(cfg.model, DEFAULT_MODEL);
        rm("LLM_API_KEY");

        // 3. OPENAI_API_KEY fallback
        set("OPENAI_API_KEY", "sk-openai");
        let cfg = resolve_llm_config(None, None).unwrap();
        assert_eq!(cfg.api_key, "sk-openai");
        rm("OPENAI_API_KEY");

        // 4. LLM_API_KEY preferred over OPENAI_API_KEY
        set("LLM_API_KEY", "sk-llm");
        set("OPENAI_API_KEY", "sk-openai");
        let cfg = resolve_llm_config(None, None).unwrap();
        assert_eq!(cfg.api_key, "sk-llm");
        rm("LLM_API_KEY");
        rm("OPENAI_API_KEY");

        // 5. CLI model arg takes priority over env
        set("LLM_API_KEY", "sk-test");
        set("LLM_MODEL", "env-model");
        let cfg = resolve_llm_config(Some("cli-model"), None).unwrap();
        assert_eq!(cfg.model, "cli-model");
        rm("LLM_MODEL");

        // 6. LLM_MODEL env var
        set("LLM_MODEL", "gpt-4");
        let cfg = resolve_llm_config(None, None).unwrap();
        assert_eq!(cfg.model, "gpt-4");
        rm("LLM_MODEL");

        // 7. OPENAI_MODEL fallback
        set("OPENAI_MODEL", "gpt-3.5");
        let cfg = resolve_llm_config(None, None).unwrap();
        assert_eq!(cfg.model, "gpt-3.5");
        rm("OPENAI_MODEL");

        // 8. DEFAULT_MODEL when nothing set
        let cfg = resolve_llm_config(None, None).unwrap();
        assert_eq!(cfg.model, DEFAULT_MODEL);

        // 9. CLI base_url takes priority
        set("LLM_BASE_URL", "https://env.example.com/v1");
        let cfg = resolve_llm_config(None, Some("https://cli.example.com/v1")).unwrap();
        assert_eq!(cfg.base_url, "https://cli.example.com/v1");
        rm("LLM_BASE_URL");

        // 10. LLM_BASE_URL env var
        set("LLM_BASE_URL", "https://llm.example.com/v1");
        let cfg = resolve_llm_config(None, None).unwrap();
        assert_eq!(cfg.base_url, "https://llm.example.com/v1");
        rm("LLM_BASE_URL");

        // 11. DEFAULT_BASE_URL fallback
        let cfg = resolve_llm_config(None, None).unwrap();
        assert_eq!(cfg.base_url, DEFAULT_BASE_URL);

        // 12. Empty env var treated as unset
        set("LLM_MODEL", "");
        let cfg = resolve_llm_config(None, None).unwrap();
        assert_eq!(cfg.model, DEFAULT_MODEL);
        rm("LLM_MODEL");

        // 13. OPENAI_BASE_URL fallback
        set("OPENAI_BASE_URL", "https://openai.example.com/v1");
        let cfg = resolve_llm_config(None, None).unwrap();
        assert_eq!(cfg.base_url, "https://openai.example.com/v1");
        rm("OPENAI_BASE_URL");

        set("LLM_API_KEY", "sk-test");
        // ── guard drops here, original env vars restored ──
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
