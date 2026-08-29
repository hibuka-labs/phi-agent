//! System prompt construction using composable fragments.
//!
//! The primary entry points are:
//!
//! - [`build_system_prompt`] — backward-compatible English prompt.
//! - [`build_system_prompt_cn`] — prompt with China-specific network hints.
//! - [`build_system_prompt_with_fragments`] — full fragment-based assembly.
//!
//! # Architecture
//!
//! phi-agent defines application-specific fragments in the [`crate::prompt::fragments`] module.
//! Generic fragments (environment, dynamic tools) come from
//! [`agent_works::prompt`]. Consumers inject custom fragments via
//! [`build_system_prompt_with_fragments`].

pub mod fragments;

use agent_works::prompt::{FragmentContext, PromptFragment, compose_fragments};

use fragments::{CoreInstructionsFragment, MemoryFragment, NetworkEnvironmentFragment};

/// Build the general-purpose system prompt (English).
///
/// Backward-compatible wrapper around [`build_system_prompt_with_fragments`]
/// with no extra fragments and `include_cn = false`.
///
/// Does not include host-specific info (consumers append that as needed).
pub fn build_system_prompt() -> String {
    build_system_prompt_with_fragments(&[], &[], false)
}

/// Build system prompt for users in China (GFW-aware).
///
/// Appends network environment hints so the agent prefers domestic services
/// when foreign sites are inaccessible.
pub fn build_system_prompt_cn() -> String {
    build_system_prompt_with_fragments(&[], &[], true)
}

/// Build a system prompt from fragments.
///
/// # Arguments
///
/// * `extra_fragments` — consumer-injected custom fragments (merged into the
///   fragment list before composition).
/// * `tool_definitions` — tool definitions as JSON (OpenAI function-calling
///   format). Passed to fragments that need tool info (e.g., `DynamicToolsFragment`).
/// * `include_cn` — whether to include China-specific network environment hints.
pub fn build_system_prompt_with_fragments(
    extra_fragments: &[Box<dyn PromptFragment>],
    tool_definitions: &[serde_json::Value],
    include_cn: bool,
) -> String {
    let ctx = FragmentContext {
        tool_definitions,
        session_id: "", // not used by current fragments
    };

    let mut fragments: Vec<Box<dyn PromptFragment>> = vec![Box::new(CoreInstructionsFragment)];

    if include_cn {
        fragments.push(Box::new(NetworkEnvironmentFragment));
    }

    // Append separator before memory section
    fragments.push(Box::new(MemoryFragment));

    // Merge consumer-injected fragments
    fragments.extend(extra_fragments.iter().map(|f| dyn_clone::clone_box(f.as_ref())));

    let body = compose_fragments(&fragments, &ctx);

    // Insert separator between core and memory (preserves original format)
    // The memory fragment already renders its content; we add "---" separator
    // before it to match the original `build_system_prompt()` format.
    body.replacen("\n\n## Memory", "\n---\n\n## Memory", 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_system_prompt() {
        let prompt = build_system_prompt();
        assert!(prompt.contains("versatile AI assistant"));
        assert!(prompt.contains("[Conversation Type Detection]"));
        assert!(prompt.contains("[Execution Guidelines]"));
        assert!(prompt.contains("[Wrap-Up]"));
        // Memory instructions are appended after a separator.
        assert!(prompt.contains("\n---\n"));
    }

    #[test]
    fn test_build_system_prompt_cn() {
        let prompt = build_system_prompt_cn();
        // Base prompt is preserved, then the CN network hints are appended.
        assert!(prompt.contains("versatile AI assistant"));
        assert!(prompt.contains("[Network Environment]"));
        assert!(prompt.contains("mainland China"));
        assert!(prompt.contains("Baidu"));
        assert!(prompt.contains("gitee.com"));
    }

    #[test]
    fn test_build_system_prompt_with_custom_fragments() {
        #[derive(Clone)]
        struct CustomFragment;
        impl PromptFragment for CustomFragment {
            fn name(&self) -> &str {
                "custom"
            }
            fn priority(&self) -> i32 {
                200
            }
            fn render(&self, _ctx: &FragmentContext) -> Option<String> {
                Some("[Custom Section]".to_string())
            }
        }

        let extra: Vec<Box<dyn PromptFragment>> = vec![Box::new(CustomFragment)];
        let prompt = build_system_prompt_with_fragments(&extra, &[], false);
        assert!(prompt.contains("[Custom Section]"));
        assert!(prompt.contains("versatile AI assistant"));
    }

    #[test]
    fn test_backward_compat_output_matches_original() {
        // The new build_system_prompt() should produce output containing
        // all the same key sections as the original hardcoded version.
        let prompt = build_system_prompt();
        let original_sections = [
            "You are a versatile AI assistant",
            "[Role]",
            "[Conversation Type Detection]",
            "[Thinking Approach]",
            "[Execution Guidelines]",
            "[File Operation Guidelines]",
            "[Wrap-Up]",
            "## Memory",
            "MEMORY.md",
        ];
        for section in &original_sections {
            assert!(prompt.contains(section), "Missing section: {}", section);
        }
    }
}
