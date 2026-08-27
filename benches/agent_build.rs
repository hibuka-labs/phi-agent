//! Benchmarks: Agent construction with varying tool counts.

use agent_base::llm_trait::response::FinishReason;
use agent_base::llm_trait::{Capabilities, ChatRequest, ChatResponse, ChatStream, LlmError, LlmProvider, ProviderInfo};
use agent_base::{AgentResult, Content, StreamChunk, Tool, ToolContext, ToolMetadata, UsageInfo};
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use futures_core::Stream;
use phi_agent::{PhiAgent, PhiAgentConfig, base_agent_builder, build_system_prompt};
use std::pin::Pin;
use std::sync::Arc;

/// A no-op tool for measuring registration overhead.
#[derive(Clone)]
struct NoopTool {
    name: &'static str,
    description: &'static str,
}

impl NoopTool {
    fn new(i: usize) -> Self {
        Self {
            name: Box::leak(format!("tool_{:04}", i).into_boxed_str()),
            description: Box::leak(format!("No-op tool {}", i).into_boxed_str()),
        }
    }
}

#[async_trait::async_trait]
impl Tool for NoopTool {
    fn name(&self) -> &'static str {
        self.name
    }
    fn description(&self) -> &'static str {
        self.description
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({})
    }
    async fn call(&self, _args: &serde_json::Value, _ctx: &ToolContext) -> AgentResult<Vec<Content>> {
        Ok(vec![Content::text("ok".to_string())])
    }
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: self.name.to_string(),
            description: self.description.to_string(),
            origin: "bench".into(),
            version: "0.0.0".into(),
            requirements: vec![],
        }
    }
}

/// Minimal mock LLM client.
struct BenchLlmClient;
#[async_trait::async_trait]
impl LlmProvider for BenchLlmClient {
    async fn stream(&self, _request: ChatRequest) -> Result<ChatStream, LlmError> {
        struct EmptyStream;
        impl Stream for EmptyStream {
            type Item = Result<StreamChunk, LlmError>;
            fn poll_next(self: Pin<&mut Self>, _: &mut std::task::Context<'_>) -> std::task::Poll<Option<Self::Item>> {
                std::task::Poll::Ready(None)
            }
        }
        Ok(ChatStream::new(Box::pin(EmptyStream)))
    }
    async fn chat(&self, _request: ChatRequest) -> Result<ChatResponse, LlmError> {
        Ok(ChatResponse {
            content: String::new(),
            reasoning_content: None,
            tool_calls: vec![],
            usage: UsageInfo::default(),
            finish_reason: FinishReason::Stop,
            raw: None,
            thinking_signature: None,
        })
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
    fn info(&self) -> ProviderInfo {
        ProviderInfo { name: "mock".to_string(), model: "mock-model".to_string(), version: None }
    }
}

fn bench_build_empty(c: &mut Criterion) {
    let client = Arc::new(BenchLlmClient);
    let prompt = build_system_prompt();
    let config = PhiAgentConfig::default();

    c.bench_function("agent/build_empty", |b| {
        b.iter(|| {
            let builder = base_agent_builder(client.clone()).system_prompt(prompt.clone());
            let agent = PhiAgent::build(builder, config.clone()).unwrap();
            black_box(agent);
        });
    });
}

fn bench_build_with_tools(c: &mut Criterion) {
    let client = Arc::new(BenchLlmClient);
    let prompt = build_system_prompt();
    let config = PhiAgentConfig::default();

    for n in [10, 50, 100] {
        let nop_tools: Vec<NoopTool> = (0..n).map(NoopTool::new).collect();
        let client = client.clone();
        let sp = prompt.clone();
        let cfg = config.clone();

        c.bench_function(&format!("agent/build_{}_tools", n), move |b| {
            b.iter(|| {
                let mut builder = base_agent_builder(client.clone()).system_prompt(sp.clone());
                for t in &nop_tools {
                    builder = builder.register_tool(t.clone());
                }
                let agent = PhiAgent::build(builder, cfg.clone()).unwrap();
                black_box(agent);
            });
        });
    }
}

criterion_group! {
    name = agent_build_benches;
    config = Criterion::default().sample_size(100);
    targets = bench_build_empty, bench_build_with_tools
}
criterion_main!(agent_build_benches);
