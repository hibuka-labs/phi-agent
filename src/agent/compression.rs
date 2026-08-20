//! LLM-based context compression middleware.
//!
//! Long tool-heavy conversations balloon the message list that gets sent to the
//! LLM on every turn, which slows each call down (and, past the window, fails).
//! This middleware observes the per-LLM-call message list in
//! [`Middleware::on_pre_llm`](agent_base::Middleware::on_pre_llm)
//! and, once it exceeds [`CompressionConfig::trigger_tokens`], summarises the
//! *earlier* portion of the conversation into a single compact message.
//!
//! Design notes:
//! - It only mutates the per-call message copy (`PreLlmCtx.messages`), never the
//!   stored session history — the JSONL turn log keeps full fidelity.
//! - The cut between "old" and "recent" messages is **tool-pairing safe**: it never
//!   separates an `Assistant{tool_calls}` message from its `Tool` results, so the
//!   resulting message list stays valid for OpenAI-compatible APIs.
//! - Summaries are cached per `(session, transcript)` so repeated LLM calls within
//!   one turn (after each tool result) don't re-pay the summarization LLM call.
//! - If summarization fails, the old block is dropped entirely rather than failing
//!   the turn — the block is the oldest context, so losing it is the graceful fallback.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use agent_base::{ChatMessage, ContextWindowManager, Middleware, PreLlmCtx};
use async_trait::async_trait;

/// Tuning knobs for [`SummarizingMiddleware`].
#[derive(Clone, Debug)]
pub struct CompressionConfig {
    /// Master switch. When `false`, `on_pre_llm` is a no-op.
    pub enabled: bool,
    /// Compress when the estimated token count of the message list exceeds this.
    pub trigger_tokens: usize,
    /// Always keep the most recent N messages intact (the agent's working context).
    pub keep_last_messages: usize,
    /// Hard cap on the raw transcript we hand to the summarizer.
    pub max_transcript_chars: usize,
    /// Target max length of the generated summary.
    pub max_summary_chars: usize,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            trigger_tokens: 30_000,
            keep_last_messages: 40,
            max_transcript_chars: 20_000,
            max_summary_chars: 2_000,
        }
    }
}

/// Compresses the earlier part of a long conversation via LLM summarization.
pub struct SummarizingMiddleware {
    client: Arc<dyn agent_base::StreamClient>,
    config: CompressionConfig,
    /// `(session_id, prefix_hash) -> (transcript_len, summary)` cache. Keyed on a hash of a
    /// *stable prefix* of the old-block transcript: the oldest messages never change, so the
    /// summary is reused across the tool-call iterations of a turn (and across turns) instead
    /// of re-paying the summarization LLM call each time. `transcript_len` guards against
    /// reusing a summary whose block has since grown a lot (see `on_pre_llm`).
    cache: Mutex<HashMap<(u64, u64), (usize, String)>>,
}

#[allow(missing_docs)]
impl SummarizingMiddleware {
    pub fn new(client: Arc<dyn agent_base::StreamClient>) -> Self {
        Self { client, config: CompressionConfig::default(), cache: Mutex::new(HashMap::new()) }
    }

    pub fn with_config(mut self, config: CompressionConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_trigger_tokens(mut self, tokens: usize) -> Self {
        self.config.trigger_tokens = tokens;
        self
    }

    pub fn with_keep_last_messages(mut self, n: usize) -> Self {
        self.config.keep_last_messages = n;
        self
    }

    pub fn with_max_summary_chars(mut self, chars: usize) -> Self {
        self.config.max_summary_chars = chars;
        self
    }

    pub fn config(&self) -> &CompressionConfig {
        &self.config
    }
}

#[async_trait]
impl Middleware for SummarizingMiddleware {
    async fn on_pre_llm(&self, ctx: &mut PreLlmCtx) -> agent_base::AgentResult<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let messages = &ctx.messages;
        if messages.len() <= self.config.keep_last_messages + 2 {
            return Ok(());
        }

        let total_tokens: usize = messages.iter().map(estimate_message_tokens).sum();
        if total_tokens <= self.config.trigger_tokens {
            return Ok(());
        }

        // Keep the leading system prompt untouched.
        let keep_first = if matches!(messages.first(), Some(ChatMessage::System { .. })) { 1 } else { 0 };

        let mut recent_start = messages.len().saturating_sub(self.config.keep_last_messages);
        if recent_start <= keep_first + 1 {
            return Ok(());
        }
        recent_start = safe_cut_index(messages, keep_first, recent_start);
        if recent_start <= keep_first {
            return Ok(());
        }

        let old = &messages[keep_first..recent_start];
        if old.is_empty() {
            return Ok(());
        }
        // Defensive: never start the compressed-away block on an orphaned tool result.
        if matches!(old.first(), Some(ChatMessage::Tool { .. })) {
            tracing::warn!("context compression skipped: old block starts with a tool result");
            return Ok(());
        }

        let transcript = serialize_block(old, self.config.max_transcript_chars);
        if transcript.trim().is_empty() {
            return Ok(());
        }

        // Key the cache on a stable prefix of the transcript. The old block only ever
        // grows at its tail (new tool results append), so its oldest content — and thus
        // this prefix — stays fixed. That lets us reuse a summary across the tool-call
        // iterations of a turn without calling the summarizer again.
        const CACHE_PREFIX_CHARS: usize = 4096;
        let prefix: String = transcript.chars().take(CACHE_PREFIX_CHARS).collect();
        let key = (ctx.session_id.id, transcript_hash(&prefix));

        let cached = self.cache.lock().ok().and_then(|c| c.get(&key).cloned());
        let summary = match cached {
            Some((cached_len, s)) if cached_len <= transcript.len() => s,
            _ => {
                let s = match summarize(self.client.as_ref(), &transcript, self.config.max_summary_chars).await {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!(
                            session_id = ctx.session_id.id,
                            "context compression summarization failed, dropping old block: {e}"
                        );
                        String::new()
                    },
                };
                if !s.is_empty()
                    && let Ok(mut cache) = self.cache.lock()
                {
                    cache.insert(key, (transcript.len(), s.clone()));
                }
                s
            },
        };

        let mut new_messages: Vec<ChatMessage> = messages[..keep_first].to_vec();
        let trimmed = summary.trim();
        if !trimmed.is_empty() {
            new_messages.push(ChatMessage::user(format!("[Earlier conversation summary]\n{trimmed}")));
        }
        new_messages.extend_from_slice(&messages[recent_start..]);

        tracing::info!(
            session_id = ctx.session_id.id,
            before = messages.len(),
            after = new_messages.len(),
            estimated_tokens = total_tokens,
            "context compressed"
        );
        ctx.messages = new_messages;
        Ok(())
    }
}

/// Walk `cut` backward until the boundary is tool-pairing safe: the message just left of
/// the cut is not an `Assistant` with pending tool calls, and the message just right of
/// the cut is not a `Tool` result (whose `Assistant{tool_calls}` would be cut away).
fn safe_cut_index(messages: &[ChatMessage], keep_first: usize, mut cut: usize) -> usize {
    while cut > keep_first {
        let left_is_tool_call = matches!(messages[cut - 1], ChatMessage::Assistant { tool_calls: Some(_), .. });
        let right_is_tool = matches!(messages[cut], ChatMessage::Tool { .. });
        if !left_is_tool_call && !right_is_tool {
            break;
        }
        cut -= 1;
    }
    cut
}

/// CJK-aware token estimate for a single message, mirroring
/// `ContextWindowManager::message_tokens` (which is `pub(crate)` and not reachable here).
fn estimate_message_tokens(msg: &ChatMessage) -> usize {
    match msg {
        ChatMessage::System { content, .. } => ContextWindowManager::estimate_tokens(content),
        ChatMessage::User { content, images, .. } => {
            // OpenAI Vision fixed per-image overhead, same constant as agent-base.
            ContextWindowManager::estimate_tokens(content) + images.len() * 85
        },
        ChatMessage::Assistant { content, reasoning_content, tool_calls } => {
            let mut tokens = content.as_deref().map(ContextWindowManager::estimate_tokens).unwrap_or(0);
            if let Some(rc) = reasoning_content {
                tokens += ContextWindowManager::estimate_tokens(rc);
            }
            if let Some(calls) = tool_calls {
                for c in calls {
                    tokens += ContextWindowManager::estimate_tokens(&c.id);
                    tokens += ContextWindowManager::estimate_tokens(&c.name);
                    tokens += ContextWindowManager::estimate_tokens(&c.arguments);
                }
            }
            tokens
        },
        ChatMessage::Tool { tool_call_id, content } => {
            ContextWindowManager::estimate_tokens(tool_call_id) + ContextWindowManager::estimate_tokens(content)
        },
        ChatMessage::Custom { role, data } => {
            ContextWindowManager::estimate_tokens(role) + ContextWindowManager::estimate_tokens(&data.to_string())
        },
    }
}

/// Render an old message block as a compact transcript for the summarizer.
fn serialize_block(messages: &[ChatMessage], max_chars: usize) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(messages.len());
    for msg in messages {
        let line = match msg {
            ChatMessage::System { content, .. } => format!("[system] {}", truncate(content, 400)),
            ChatMessage::User { content, .. } => format!("[user] {}", truncate(content, 400)),
            ChatMessage::Assistant { content, tool_calls, .. } => match tool_calls {
                Some(calls) if !calls.is_empty() => {
                    let calls: Vec<String> =
                        calls.iter().map(|c| format!("{}({})", c.name, truncate(&c.arguments, 150))).collect();
                    format!("[assistant tool_call] {}", calls.join("; "))
                },
                _ => format!("[assistant] {}", content.as_deref().map(|c| truncate(c, 400)).unwrap_or_default()),
            },
            ChatMessage::Tool { tool_call_id, content } => format!("[tool:{tool_call_id}] {}", truncate(content, 300)),
            ChatMessage::Custom { role, data } => format!("[custom:{role}] {}", truncate(&data.to_string(), 400)),
        };
        parts.push(line);
    }
    truncate(&parts.join("\n"), max_chars)
}

fn truncate(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    let head: String = s.chars().take(max_chars).collect();
    format!("{head}…")
}

fn transcript_hash(s: &str) -> u64 {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// One-shot summarization call. Returns the model's text, or an error.
async fn summarize(
    client: &dyn agent_base::StreamClient,
    transcript: &str,
    max_chars: usize,
) -> agent_base::AgentResult<String> {
    let system = ChatMessage::system(
        "You are a conversation summarizer for an AI agent that can call tools \
         (browser, shell, search, etc.).",
    );
    let user = ChatMessage::user(format!(
        "Compress the earlier portion of this agent conversation. Preserve:\n\
         - the user's original goal and any constraints they stated;\n\
         - every important fact, decision and intermediate result;\n\
         - which tools were used and their key findings/returned data;\n\
         - blockers, errors, and anything the agent still needs to remember to continue.\n\
         Detect the conversation language and write the summary in that same language.\n\
         Output ONLY the summary text, no preamble, about {max_chars} characters max.\n\n\
         === CONVERSATION ===\n{transcript}",
    ));

    let summary = client.chat(&[system, user], &[], None, None).await?;
    if summary.is_empty() { Ok(String::new()) } else { Ok(summary) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_base::LlmClient;
    use serde_json::Value;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockClient {
        summary: String,
        calls: AtomicUsize,
    }

    #[async_trait]
    impl LlmClient for MockClient {
        async fn chat(
            &self,
            _messages: &[ChatMessage],
            _tools: &[Value],
            _reasoning: Option<&agent_base::ReasoningConfig>,
            _response_format: Option<&agent_base::ResponseFormat>,
        ) -> agent_base::AgentResult<Value> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(serde_json::json!({
                "choices": [{ "message": { "content": self.summary } }]
            }))
        }

        async fn chat_stream(
            &self,
            _messages: &[ChatMessage],
            _tools: &[Value],
            _reasoning: Option<&agent_base::ReasoningConfig>,
            _response_format: Option<&agent_base::ResponseFormat>,
        ) -> agent_base::AgentResult<
            std::pin::Pin<
                Box<dyn futures_core::Stream<Item = agent_base::AgentResult<agent_base::StreamChunk>> + Send>,
            >,
        > {
            unreachable!("not used in tests")
        }

        fn capabilities(&self) -> agent_base::LlmCapabilities {
            agent_base::LlmCapabilities::default()
        }
    }

    fn sample_messages() -> Vec<ChatMessage> {
        vec![
            ChatMessage::system("sys"),
            ChatMessage::user("q1"),
            ChatMessage::assistant_tool_call("call_1", "browser_navigate", r#"{"url":"a"}"#),
            ChatMessage::tool("call_1", "loaded ok"),
            ChatMessage::assistant("found the page"),
            ChatMessage::user("q2"),
            ChatMessage::assistant("done"),
        ]
    }

    #[test]
    fn test_safe_cut_never_splits_tool_pair() {
        let msgs = sample_messages();
        // A naive cut at 3 would split the tool pair (left = assistant_tool_call).
        let cut = safe_cut_index(&msgs, 1, 3);
        assert!(cut < 3, "must walk backward from an unsafe cut");
        assert!(!matches!(msgs[cut - 1], ChatMessage::Assistant { tool_calls: Some(_), .. }));
        assert!(!matches!(msgs[cut], ChatMessage::Tool { .. }));
    }

    #[test]
    fn test_safe_cut_prefers_given_boundary_when_safe() {
        let msgs = vec![
            ChatMessage::system("sys"),
            ChatMessage::user("q1"),
            ChatMessage::assistant("a1"),
            ChatMessage::user("q2"),
            ChatMessage::assistant("a2"),
        ];
        let cut = safe_cut_index(&msgs, 1, 3);
        assert_eq!(cut, 3);
    }

    #[test]
    fn test_estimate_message_tokens_cjk_and_tool() {
        let sys = ChatMessage::system("中文系统提示");
        let tool = ChatMessage::tool("call_9", "hello 世界".repeat(100));
        assert!(estimate_message_tokens(&sys) < estimate_message_tokens(&tool));
        assert!(estimate_message_tokens(&sys) > 0);
    }

    #[test]
    fn test_serialize_block_preserves_tool_calls() {
        let msgs =
            vec![ChatMessage::user("short question"), ChatMessage::assistant_tool_call("c1", "browser_navigate", "{}")];
        let out = serialize_block(&msgs, 1000);
        assert!(out.contains("tool_call"));
        assert!(out.contains("browser_navigate"));
    }

    #[test]
    fn test_serialize_block_truncates_oversized_fields() {
        let long = "x".repeat(1000);
        let msgs = vec![ChatMessage::user(long.clone())];
        let out = serialize_block(&msgs, 200);
        assert!(out.chars().count() <= 201); // 200 + ellipsis
        assert!(!out.contains(&long), "full payload must not leak through");
    }

    #[test]
    fn test_serialize_block_custom_message() {
        let msgs = vec![ChatMessage::Custom {
            role: "artifact".to_string(),
            data: serde_json::json!({"id": "art-1", "content": "generated image"}),
        }];
        let out = serialize_block(&msgs, 500);
        assert!(out.contains("[custom:artifact]"), "custom message should be serialized with role label");
        assert!(out.contains("art-1"), "custom message data should appear in output");
    }

    #[test]
    fn test_estimate_message_tokens_custom() {
        let custom =
            ChatMessage::Custom { role: "notification".to_string(), data: serde_json::json!({"msg": "hello"}) };
        let tokens = estimate_message_tokens(&custom);
        assert!(tokens > 0, "custom message should have non-zero token estimate");
    }

    #[tokio::test]
    async fn test_on_pre_llm_noop_when_under_threshold() {
        let client =
            agent_base::llm::adapt(Arc::new(MockClient { summary: "S".to_string(), calls: AtomicUsize::new(0) }));
        let mw = SummarizingMiddleware::new(client);
        let mut ctx = PreLlmCtx {
            emit_fn: None,
            session_id: agent_base::SessionId { id: 1, external_id: None },
            messages: vec![ChatMessage::system("sys"), ChatMessage::user("hi")],
            tools: vec![],
        };
        mw.on_pre_llm(&mut ctx).await.unwrap();
        assert_eq!(ctx.messages.len(), 2);
    }

    #[tokio::test]
    async fn test_on_pre_llm_compresses_and_caches() {
        let mock_client = Arc::new(MockClient {
            summary: "The user wanted to scrape articles.".to_string(),
            calls: AtomicUsize::new(0),
        });
        let client = agent_base::llm::adapt(mock_client.clone());
        let mw = SummarizingMiddleware::new(client.clone())
            .with_trigger_tokens(1) // always compress
            .with_keep_last_messages(2);

        let make_ctx = || PreLlmCtx {
            emit_fn: None,
            session_id: agent_base::SessionId { id: 1, external_id: None },
            messages: sample_messages(),
            tools: vec![],
        };

        let mut ctx = make_ctx();
        mw.on_pre_llm(&mut ctx).await.unwrap();
        assert!(ctx.messages.len() < 7, "should have compressed, got {}", ctx.messages.len());
        assert_eq!(mock_client.calls.load(Ordering::SeqCst), 1, "summarizer called once");
        // Summary injected as a User message right after the system prompt.
        assert!(matches!(ctx.messages[1], ChatMessage::User { .. }));
        // No orphaned tool message anywhere in the result.
        for m in &ctx.messages {
            assert!(!matches!(m, ChatMessage::Tool { .. }), "no tool orphan after compression");
        }

        // Same original old block again → cache hit, no new LLM call.
        let mut ctx2 = make_ctx();
        mw.on_pre_llm(&mut ctx2).await.unwrap();
        assert_eq!(mock_client.calls.load(Ordering::SeqCst), 1, "cache reused");
        assert_eq!(ctx.messages.len(), ctx2.messages.len());
    }

    #[tokio::test]
    async fn test_summarization_failure_drops_old_block() {
        // A client that always errors on chat() → middleware must fall back to dropping
        // the old block instead of failing the turn.
        let failing = agent_base::llm::adapt(Arc::new(FailingClient));
        let mw = SummarizingMiddleware::new(failing).with_trigger_tokens(1).with_keep_last_messages(2);

        let mut ctx = PreLlmCtx {
            emit_fn: None,
            session_id: agent_base::SessionId { id: 1, external_id: None },
            messages: sample_messages(),
            tools: vec![],
        };
        let result = mw.on_pre_llm(&mut ctx).await;
        assert!(result.is_ok(), "must not fail the turn");
        // Old block dropped, no summary message inserted.
        assert!(ctx.messages.len() < 7);
        assert!(ctx.messages.iter().all(|m| !matches!(
            m,
            ChatMessage::User { content, .. } if content.contains("Earlier conversation summary")
        )));
    }

    #[tokio::test]
    async fn test_default_threshold_fires_on_long_conversation() {
        // Guards against the "wired but never fires" regression: the default
        // trigger_tokens must compress a realistic long tool-heavy conversation.
        let mock_client =
            Arc::new(MockClient { summary: "Earlier context summarised.".to_string(), calls: AtomicUsize::new(0) });
        let client = agent_base::llm::adapt(mock_client.clone());
        let mw = SummarizingMiddleware::new(client.clone()); // default config

        let mut messages = vec![ChatMessage::system("sys")];
        // 1440 CJK chars ≈ ~960 tokens each (estimate at ~1 token / 1.5 CJK chars).
        let chunk = "这是用于验证压缩默认阈值的中文文本。".repeat(80);
        for i in 0..45 {
            messages.push(ChatMessage::user(format!("q{i} {chunk}")));
            messages.push(ChatMessage::assistant(format!("a{i}")));
        }
        let mut ctx = PreLlmCtx {
            emit_fn: None,
            session_id: agent_base::SessionId { id: 1, external_id: None },
            messages,
            tools: vec![],
        };
        let before = ctx.messages.len();
        assert!(before > 42, "test fixture must exceed the message-count gate");
        mw.on_pre_llm(&mut ctx).await.unwrap();
        assert!(
            ctx.messages.len() < before,
            "default threshold should compress a long conversation ({} → {})",
            before,
            ctx.messages.len()
        );
        // Compression must have actually invoked the summarizer LLM (not just passed
        // the message-count gate while the token estimate stayed at/below the threshold).
        assert!(mock_client.calls.load(Ordering::SeqCst) >= 1, "summarizer must be invoked for a long conversation");
        // Summary injected as the first message after the system prompt.
        assert!(matches!(ctx.messages[1], ChatMessage::User { .. }));
    }

    struct FailingClient;

    #[async_trait]
    impl LlmClient for FailingClient {
        async fn chat(
            &self,
            _messages: &[ChatMessage],
            _tools: &[Value],
            _reasoning: Option<&agent_base::ReasoningConfig>,
            _response_format: Option<&agent_base::ResponseFormat>,
        ) -> agent_base::AgentResult<Value> {
            Err(agent_base::AgentError::internal("summarization failed"))
        }

        async fn chat_stream(
            &self,
            _messages: &[ChatMessage],
            _tools: &[Value],
            _reasoning: Option<&agent_base::ReasoningConfig>,
            _response_format: Option<&agent_base::ResponseFormat>,
        ) -> agent_base::AgentResult<
            std::pin::Pin<
                Box<dyn futures_core::Stream<Item = agent_base::AgentResult<agent_base::StreamChunk>> + Send>,
            >,
        > {
            unreachable!("not used in tests")
        }

        fn capabilities(&self) -> agent_base::LlmCapabilities {
            agent_base::LlmCapabilities::default()
        }
    }
}
