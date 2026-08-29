//! phi-agent specific prompt fragments.
//!
//! These fragments implement the phi-agent system prompt using the
//! [`PromptFragment`] trait from agent-works.
//!
//! # Priority map
//!
//! | Fragment                     | Priority | Content                     |
//! |------------------------------|----------|-----------------------------|
//! | `CoreInstructionsFragment`   | 10       | Role, guidelines, wrap-up   |
//! | `MemoryFragment`             | 95       | Memory system instructions  |
//! | `NetworkEnvironmentFragment` | 90       | China GFW-aware hints       |

use agent_works::prompt::{FragmentContext, PromptFragment};

// ── Core Instructions ──────────────────────────────────────────────────────

/// The main system prompt content — role, conversation detection, thinking
/// approach, execution guidelines, file operation guidelines, wrap-up.
///
/// Priority: 10 (renders first).
#[derive(Clone)]
pub struct CoreInstructionsFragment;

impl PromptFragment for CoreInstructionsFragment {
    fn name(&self) -> &str {
        "core_instructions"
    }

    fn priority(&self) -> i32 {
        10
    }

    fn render(&self, _ctx: &FragmentContext) -> Option<String> {
        Some(CORE_INSTRUCTIONS.to_string())
    }
}

const CORE_INSTRUCTIONS: &str = r#"You are a versatile AI assistant with strong autonomous problem-solving abilities.

[Role]
You get things done, not chat. Take initiative — don't ask for confirmation repeatedly. Reply with conclusions only.

[Conversation Type Detection]
- Greetings / small talk (hello, thanks, goodbye) → Friendly response, no tools.
- Questions / discussion → Give analysis and advice, don't execute destructive actions directly.
- Dev / ops tasks → Take action directly.

[Thinking Approach]
Each turn, quickly assess: what phase am I in → what's the next step → do it.
For complex tasks (3+ steps), use update_plan to show the plan and let the user see progress.

[Execution Guidelines]
- Check state before acting. Probe current state before making changes.
- Verify results after operations.
- Text replies should only contain analysis and conclusions — don't repeat tool output.
- When your current task is active and not fully complete, each reply must include tool calls that advance the work — a text-only reply then ends the run. Reserve text-only replies for: (a) reporting results after the task is complete, (b) genuinely needing the user's input or confirmation before proceeding, or (c) handling a hard blocker that requires handing control back to the user.
- Avoid narrating next steps ("let me check X", "next I'll do Y") without emitting the matching tool call in the same reply — think aloud in your reasoning block, not in the visible reply.
- If more work is needed, keep calling tools — don't stop after one step.
- Independent operations can run in parallel; dependent ones must be serial.
- On error: analyze the cause, find a fix, and apply it directly. Stop after 2 consecutive failures of the same approach and explain to the user.

[File Operation Guidelines]
- Confirm the file exists before reading.
- Confirm the directory exists before writing (create if needed).
- Back up or verify content before modifying files.
- Verify file state after operations.

[Wrap-Up]
Report a final conclusion once the entire user request is complete. If work remains and you can proceed without user input, keep calling tools instead of wrapping up. After confirming results, report the conclusion concisely."#;

// ── Memory ─────────────────────────────────────────────────────────────────

/// Memory system prompt from agent-works.
///
/// Priority: 95 (renders near the end).
#[derive(Clone)]
pub struct MemoryFragment;

impl PromptFragment for MemoryFragment {
    fn name(&self) -> &str {
        "memory"
    }

    fn priority(&self) -> i32 {
        95
    }

    fn render(&self, _ctx: &FragmentContext) -> Option<String> {
        Some(agent_works::build_memory_system_prompt())
    }
}

// ── Network Environment (China) ────────────────────────────────────────────

/// GFW-aware network hints for users in mainland China.
///
/// Priority: 90 (after core instructions, before memory).
/// Only injected when `include_cn` is true.
#[derive(Clone)]
pub struct NetworkEnvironmentFragment;

impl PromptFragment for NetworkEnvironmentFragment {
    fn name(&self) -> &str {
        "network_environment_cn"
    }

    fn priority(&self) -> i32 {
        90
    }

    fn render(&self, _ctx: &FragmentContext) -> Option<String> {
        Some(NETWORK_ENV_CN.to_string())
    }
}

const NETWORK_ENV_CN: &str = r#"[Network Environment]
You are operating in mainland China. Google, YouTube, Twitter, BBC, and many foreign sites are inaccessible. Prefer domestic alternatives:
- Search: Bing (cn.bing.com) or Baidu (baidu.com)
- News: Toutiao, Baidu News, The Paper (thepaper.cn), Zaobao (zaobao.com)
- Dev: mirrors.tuna.tsinghua.edu.cn, gitee.com
When a foreign site times out, switch to a domestic alternative immediately — don't retry."#;

#[cfg(test)]
mod tests {
    use super::*;
    use agent_works::prompt::compose_fragments;

    #[test]
    fn test_core_instructions_fragment() {
        let frag = CoreInstructionsFragment;
        let ctx = FragmentContext { tool_definitions: &[], session_id: "test" };
        let output = frag.render(&ctx).unwrap();
        assert!(output.contains("versatile AI assistant"));
        assert!(output.contains("[Conversation Type Detection]"));
        assert!(output.contains("[Execution Guidelines]"));
        assert!(output.contains("[Wrap-Up]"));
    }

    #[test]
    fn test_memory_fragment() {
        let frag = MemoryFragment;
        let ctx = FragmentContext { tool_definitions: &[], session_id: "test" };
        let output = frag.render(&ctx).unwrap();
        assert!(output.contains("Memory"));
        assert!(output.contains("MEMORY.md"));
    }

    #[test]
    fn test_network_environment_fragment() {
        let frag = NetworkEnvironmentFragment;
        let ctx = FragmentContext { tool_definitions: &[], session_id: "test" };
        let output = frag.render(&ctx).unwrap();
        assert!(output.contains("mainland China"));
        assert!(output.contains("Baidu"));
        assert!(output.contains("gitee.com"));
    }

    #[test]
    fn test_all_fragments_compose() {
        let fragments: Vec<Box<dyn PromptFragment>> =
            vec![Box::new(CoreInstructionsFragment), Box::new(MemoryFragment), Box::new(NetworkEnvironmentFragment)];
        let ctx = FragmentContext { tool_definitions: &[], session_id: "test" };
        let result = compose_fragments(&fragments, &ctx);
        // Core (10) comes first, then Network (90), then Memory (95)
        let core_pos = result.find("versatile AI assistant").unwrap();
        let cn_pos = result.find("mainland China").unwrap();
        let mem_pos = result.find("MEMORY.md").unwrap();
        assert!(core_pos < cn_pos);
        assert!(cn_pos < mem_pos);
    }

    #[test]
    fn test_fragment_names() {
        assert_eq!(CoreInstructionsFragment.name(), "core_instructions");
        assert_eq!(MemoryFragment.name(), "memory");
        assert_eq!(NetworkEnvironmentFragment.name(), "network_environment_cn");
    }

    #[test]
    fn test_fragment_priorities() {
        assert_eq!(CoreInstructionsFragment.priority(), 10);
        assert_eq!(NetworkEnvironmentFragment.priority(), 90);
        assert_eq!(MemoryFragment.priority(), 95);
    }
}
