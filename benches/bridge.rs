//! Benchmarks: bridge protocol server overhead.

use agent_base::llm_trait::response::FinishReason;
use agent_base::llm_trait::{Capabilities, ChatRequest, ChatResponse, ChatStream, LlmError, LlmProvider, ProviderInfo};
use agent_base::{StreamChunk, UsageInfo};
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use futures_core::Stream;
use phi_agent::bridge::server::ProtocolServer;
use phi_agent::{base_agent_builder, build_system_prompt};
use std::pin::Pin;
use std::sync::Arc;

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

fn bench_build_server(c: &mut Criterion) {
    let client = Arc::new(BenchLlmClient);
    let prompt = build_system_prompt();

    c.bench_function("bridge/build_from_builder", |b| {
        b.iter(|| {
            let builder = base_agent_builder(client.clone()).system_prompt(prompt.clone());
            let server = ProtocolServer::from_builder(builder).unwrap();
            black_box(server);
        });
    });
}

fn bench_create_session(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let client = Arc::new(BenchLlmClient);
    let builder = base_agent_builder(client).system_prompt(build_system_prompt());
    let server = ProtocolServer::from_builder(builder).unwrap();
    let mut counter = 0u64;

    c.bench_function("bridge/get_or_create_session", |b| {
        b.iter(|| {
            counter += 1;
            let sid = rt.block_on(server.get_or_create_session(Some(format!("ext-{}", counter))));
            black_box(sid);
        });
    });
}

criterion_group! {
    name = bridge_benches;
    config = Criterion::default().sample_size(200);
    targets = bench_build_server, bench_create_session
}
criterion_main!(bridge_benches);
