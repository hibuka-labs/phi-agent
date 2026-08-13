//! Protocol server — adapts [`AgentRuntime`] to the bridge protocol.
//!
//! Tool calls use a single-slot pattern: the serve loop pushes a receiver
//! before each tool call, and ProxyTool pops it.  This handles sequential
//! tool calls cleanly; parallel calls can be added later via a FIFO queue.

use std::collections::HashMap;
use std::sync::Arc;

use agent_base::{
    AgentResult, AgentRuntime, Content, RunOutcome, RuntimeEvent, SessionId, Tool, ToolContext, ToolMetadata,
};
use agent_works::AgentBuilder;
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::{Mutex, mpsc};

/// A tool-call result delivered back through the bridge slot: either the
/// tool's content or an execution error.
type ToolCallResult = AgentResult<Vec<Content>>;

/// Bridge protocol server — wraps an [`AgentRuntime`] and exposes it over
/// the NDJSON bridge protocol for external SDK consumption.
///
/// Manages sessions, tool registration via proxy tools, and event forwarding.
/// Tool calls use a single-slot pattern: the serve loop pushes a receiver
/// before each tool call, and ProxyTool pops it.
#[derive(Clone)]
pub struct ProtocolServer {
    runtime: AgentRuntime,
    /// Single-slot: the next tool call's response receiver.
    /// serve loop pushes, ProxyTool pops.
    slot: Arc<Mutex<Option<mpsc::UnboundedReceiver<ToolCallResult>>>>,
    /// Map external_id → SessionId so that runs with the same
    /// external_id reuse the same session.
    sessions: Arc<Mutex<HashMap<String, SessionId>>>,
}

impl ProtocolServer {
    /// Wrap an existing [`AgentRuntime`] in a protocol server.
    pub fn new(runtime: AgentRuntime) -> Self {
        Self { runtime, slot: Arc::new(Mutex::new(None)), sessions: Arc::new(Mutex::new(HashMap::new())) }
    }

    /// Build a protocol server from an [`AgentBuilder`].
    pub fn from_builder(builder: AgentBuilder) -> Result<Self, agent_base::AgentError> {
        let runtime = builder.build()?;
        Ok(Self::new(runtime))
    }

    /// Register a tool implemented on the SDK side.
    ///
    /// The tool's `call` will block until the SDK sends a `tool_result`
    /// message through the bridge.
    pub async fn register_tool(&self, name: String, description: String, parameters: Value) {
        let proxy = ProxyTool { name, description, parameters, slot: self.slot.clone() };
        let tools_arc = self.runtime.tools_mut();
        let mut tools = tools_arc.write().await;
        tools.register(proxy);
    }

    /// Set up the response channel for the NEXT tool call.
    /// Returns the sender — keep it; send the result when the SDK replies.
    pub async fn prepare_tool_call(&self) -> mpsc::UnboundedSender<ToolCallResult> {
        let (tx, rx) = mpsc::unbounded_channel();
        *self.slot.lock().await = Some(rx);
        tx
    }

    /// Create a new session, optionally with an external ID for reuse.
    pub async fn create_session(&self, external_id: Option<String>) -> (SessionId, Option<String>) {
        let sid = self.runtime.create_session().await;
        // NOTE: We intentionally do NOT set sid.external_id because
        // agent_base's run_turn() hangs when external_id is Some.
        // Session reuse is handled by get_or_create_session() which
        // maintains its own external_id → SessionId map.
        let ext = external_id.clone();
        (sid, ext)
    }

    /// Get or create a session by external_id.
    ///
    /// If ``external_id`` is ``Some`` and a session with that id already
    /// exists, it is reused (preserving conversation history).  Otherwise
    /// a new session is created and registered.
    pub async fn get_or_create_session(&self, external_id: Option<String>) -> SessionId {
        if let Some(ref ext) = external_id {
            let mut sessions = self.sessions.lock().await;
            if let Some(sid) = sessions.get(ext) {
                return sid.clone();
            }
            // Create new and register
            let (sid, _) = self.create_session(Some(ext.clone())).await;
            sessions.insert(ext.clone(), sid.clone());
            return sid;
        }
        // No external_id — always create new
        self.create_session(None).await.0
    }

    /// Subscribe to runtime events broadcast by the agent.
    pub fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<RuntimeEvent> {
        self.runtime.subscribe_runtime_events()
    }

    /// Run a turn on the given session, forwarding events to the callback.
    pub async fn run_turn<F>(&self, sid: &SessionId, input: &str, f: F) -> AgentResult<RunOutcome>
    where
        F: FnMut(RuntimeEvent) -> AgentResult<()> + Send,
    {
        self.runtime.run_turn(sid.clone(), input, f).await
    }

    /// Cancel the currently running turn.
    pub fn cancel(&self) {
        self.runtime.cancel();
    }

    /// List all registered tools with their metadata, sorted by name.
    pub async fn list_tools(&self) -> Vec<ToolMetadata> {
        let tools = self.runtime.tools_mut();
        let registry = tools.read().await;
        registry.metadatas()
    }
}

// ── ProxyTool ─────────────────────────────────────────────────────────

struct ProxyTool {
    name: String,
    description: String,
    parameters: Value,
    slot: Arc<Mutex<Option<mpsc::UnboundedReceiver<ToolCallResult>>>>,
}

#[async_trait]
impl Tool for ProxyTool {
    fn name(&self) -> &'static str {
        Box::leak(self.name.clone().into_boxed_str())
    }

    fn description(&self) -> &'static str {
        Box::leak(self.description.clone().into_boxed_str())
    }

    fn schema(&self) -> Value {
        self.parameters.clone()
    }

    async fn call(&self, _args: &Value, _ctx: &ToolContext) -> AgentResult<Vec<Content>> {
        let mut rx = self
            .slot
            .lock()
            .await
            .take()
            .ok_or_else(|| agent_base::AgentError::internal("no tool call slot prepared"))?;

        match rx.recv().await {
            Some(result) => result,
            None => Ok(vec![Content::text("Tool call cancelled".to_string())]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::builder::base_agent_builder;
    use agent_base::ToolContext;
    use async_trait::async_trait;
    use futures_core::Stream;
    use serde_json::json;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::task::{Context, Poll};

    struct StubClient;

    /// Yields one `Text` chunk, one `Stop` chunk, then ends — enough for the
    /// react loop to complete a turn.
    struct StopStream {
        state: u8,
    }

    impl Stream for StopStream {
        type Item = agent_base::AgentResult<agent_base::StreamChunk>;

        fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            match self.state {
                0 => {
                    self.state = 1;
                    Poll::Ready(Some(Ok(agent_base::StreamChunk::Text("hello".to_string()))))
                },
                1 => {
                    self.state = 2;
                    Poll::Ready(Some(Ok(agent_base::StreamChunk::Stop { finish_reason: Some("stop".to_string()) })))
                },
                _ => Poll::Ready(None),
            }
        }
    }

    #[async_trait]
    impl agent_base::StreamClient for StubClient {
        async fn stream(
            &self,
            _messages: &[agent_base::ChatMessage],
            _tools: &[serde_json::Value],
            _reasoning: Option<&agent_base::ReasoningConfig>,
            _response_format: Option<&agent_base::ResponseFormat>,
        ) -> agent_base::AgentResult<Pin<Box<dyn Stream<Item = agent_base::AgentResult<agent_base::StreamChunk>> + Send>>>
        {
            Ok(Box::pin(StopStream { state: 0 }))
        }

        fn capabilities(&self) -> agent_base::LlmCapabilities {
            agent_base::LlmCapabilities::default()
        }
    }

    fn client() -> Arc<dyn agent_base::StreamClient> {
        Arc::new(StubClient)
    }

    fn runtime() -> agent_base::AgentRuntime {
        base_agent_builder(client()).build().unwrap()
    }

    /// Register an "echo" proxy tool and return its handle from the registry.
    async fn register_echo(server: &ProtocolServer, rt: &agent_base::AgentRuntime) -> Arc<dyn agent_base::Tool> {
        server.register_tool("echo".to_string(), "echo tool".to_string(), json!({ "type": "object" })).await;
        let tools = rt.tools_mut();
        let registry = tools.read().await;
        registry.get("echo").expect("echo tool should be registered")
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_from_builder() {
        let server = ProtocolServer::from_builder(base_agent_builder(client())).unwrap();
        let _ = server;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_register_and_list_tools() {
        let rt = runtime();
        let server = ProtocolServer::new(rt);
        server.register_tool("echo".to_string(), "echo tool".to_string(), json!({ "type": "object" })).await;

        let tools = server.list_tools().await;
        let echo = tools.iter().find(|t| t.name == "echo").expect("echo tool should be listed");
        assert_eq!(echo.description, "echo tool");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_proxy_tool_call_without_slot_errors() {
        let rt = runtime();
        let server = ProtocolServer::new(rt.clone());
        let tool = register_echo(&server, &rt).await;

        let result = tool.call(&json!({}), &ToolContext::for_test()).await;
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_proxy_tool_call_delivers_result() {
        let rt = runtime();
        let server = ProtocolServer::new(rt.clone());
        let tool = register_echo(&server, &rt).await;

        let tx = server.prepare_tool_call().await;
        let args = json!({});
        let ctx = ToolContext::for_test();
        let call = tool.call(&args, &ctx);
        tx.send(Ok(vec![Content::text("result".to_string())])).unwrap();
        let result = call.await.unwrap();

        assert_eq!(result.len(), 1);
        match &result[0] {
            Content::Text { text } => assert_eq!(text, "result"),
            other => panic!("expected text content, got {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_proxy_tool_call_cancelled_when_sender_dropped() {
        let rt = runtime();
        let server = ProtocolServer::new(rt.clone());
        let tool = register_echo(&server, &rt).await;

        let tx = server.prepare_tool_call().await;
        drop(tx); // dropping the only sender closes the channel
        let result = tool.call(&json!({}), &ToolContext::for_test()).await.unwrap();

        assert_eq!(result.len(), 1);
        match &result[0] {
            Content::Text { text } => assert_eq!(text, "Tool call cancelled"),
            other => panic!("expected text content, got {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_create_session() {
        let rt = runtime();
        let server = ProtocolServer::new(rt);

        let (_, ext) = server.create_session(None).await;
        assert!(ext.is_none());

        let (_, ext) = server.create_session(Some("ext".to_string())).await;
        assert_eq!(ext.as_deref(), Some("ext"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_or_create_session_reuse() {
        let rt = runtime();
        let server = ProtocolServer::new(rt);

        let a = server.get_or_create_session(Some("shared".to_string())).await;
        let b = server.get_or_create_session(Some("shared".to_string())).await;
        assert_eq!(a, b);

        let c = server.get_or_create_session(None).await;
        let d = server.get_or_create_session(None).await;
        assert_ne!(c, d);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_subscribe_events_and_run_turn() {
        let rt = runtime();
        let server = ProtocolServer::new(rt);

        let _rx = server.subscribe_events();
        let sid = server.create_session(None).await.0;
        let outcome = server.run_turn(&sid, "hi", |_| Ok(())).await;
        assert!(outcome.is_ok());

        server.cancel();
        let _ = sid;
    }
}
