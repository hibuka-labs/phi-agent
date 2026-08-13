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
