//! Benchmarks: Agent construction with varying tool counts.

use agent_base::{
    AgentResult, ChatMessage, Content, LlmCapabilities, LlmClient, ReasoningConfig, ResponseFormat, StreamChunk, Tool,
    ToolContext, ToolMetadata,
};
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
impl LlmClient for BenchLlmClient {
    async fn chat(
        &self,
        _: &[ChatMessage],
        _: &[serde_json::Value],
        _: Option<&ReasoningConfig>,
        _: Option<&ResponseFormat>,
    ) -> AgentResult<serde_json::Value> {
        Ok(serde_json::json!({}))
    }
    async fn chat_stream(
        &self,
        _: &[ChatMessage],
        _: &[serde_json::Value],
        _: Option<&ReasoningConfig>,
        _: Option<&ResponseFormat>,
    ) -> AgentResult<Pin<Box<dyn Stream<Item = AgentResult<StreamChunk>> + Send>>> {
        struct EmptyStream;
        impl Stream for EmptyStream {
            type Item = AgentResult<StreamChunk>;
            fn poll_next(self: Pin<&mut Self>, _: &mut std::task::Context<'_>) -> std::task::Poll<Option<Self::Item>> {
                std::task::Poll::Ready(None)
            }
        }
        Ok(Box::pin(EmptyStream))
    }
    fn capabilities(&self) -> LlmCapabilities {
        LlmCapabilities {
            supports_thinking: false,
            supports_streaming: false,
            supports_tools: true,
            supports_vision: false,
            max_context_tokens: Some(4096),
            max_output_tokens: Some(4096),
        }
    }
}

fn bench_build_empty(c: &mut Criterion) {
    let client = agent_base::llm::adapt(Arc::new(BenchLlmClient));
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
    let client = agent_base::llm::adapt(Arc::new(BenchLlmClient));
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
