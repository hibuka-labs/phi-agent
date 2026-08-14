//! Benchmarks: RuntimeEvent → JSON serialization throughput.

use agent_base::{RuntimeEvent, SessionId};
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use phi_agent::event_log::{event_to_jsonl, event_to_value};

fn make_thought_event(sid: &SessionId) -> RuntimeEvent {
    RuntimeEvent::ThoughtDelta {
        session_id: sid.clone(),
        text: "Let me analyze the code structure carefully...".repeat(5),
        agent_id: None,
        trace_id: None,
    }
}

fn make_tool_start_event(sid: &SessionId) -> RuntimeEvent {
    RuntimeEvent::ToolCallStarted {
        session_id: sid.clone(),
        tool_name: "read_file".into(),
        args_json: r#"{"path":"/src/main.rs","offset":0,"limit":500}"#.into(),
        agent_id: None,
        trace_id: None,
    }
}

fn make_tool_finish_event(sid: &SessionId) -> RuntimeEvent {
    RuntimeEvent::ToolCallFinished {
        session_id: sid.clone(),
        tool_name: "read_file".into(),
        summary: format!("Read {} bytes", "fn main() {}\n".repeat(20).len()),
        agent_id: None,
        trace_id: None,
        denied: false,
    }
}

fn bench_event_to_jsonl(c: &mut Criterion) {
    let sid = SessionId::new(1);
    let events = [make_thought_event(&sid), make_tool_start_event(&sid), make_tool_finish_event(&sid)];

    c.bench_function("serialization/jsonl_batch_3", |b| {
        b.iter(|| {
            let mut total = 0usize;
            for event in &events {
                total += event_to_jsonl(event).len();
            }
            black_box(total);
        });
    });
}

fn bench_event_to_value(c: &mut Criterion) {
    let sid = SessionId::new(2);
    let event = make_tool_finish_event(&sid);

    c.bench_function("serialization/to_value", |b| {
        b.iter(|| {
            black_box(event_to_value(&event));
        });
    });
}

fn bench_jsonl_bulk(c: &mut Criterion) {
    // 150 events — realistic turn
    let sid = SessionId::new(3);
    let events: Vec<RuntimeEvent> = (0..50)
        .flat_map(|i| {
            vec![
                RuntimeEvent::ThoughtDelta {
                    session_id: sid.clone(),
                    text: format!("Step {}: thinking...", i),
                    agent_id: None,
                    trace_id: None,
                },
                RuntimeEvent::ToolCallStarted {
                    session_id: sid.clone(),
                    tool_name: "search".into(),
                    args_json: format!(r#"{{"query":"pattern {}"}}"#, i),
                    agent_id: None,
                    trace_id: None,
                },
                RuntimeEvent::ToolCallFinished {
                    session_id: sid.clone(),
                    tool_name: "search".into(),
                    summary: format!("Found results for query {}", i),
                    agent_id: None,
                    trace_id: None,
                    denied: false,
                },
            ]
        })
        .collect();

    c.bench_function("serialization/jsonl_bulk_150", |b| {
        b.iter(|| {
            let mut total = 0usize;
            for event in &events {
                total += event_to_jsonl(event).len();
            }
            black_box(total);
        });
    });
}

criterion_group! {
    name = serialization_benches;
    config = Criterion::default().sample_size(500);
    targets = bench_event_to_jsonl, bench_event_to_value, bench_jsonl_bulk
}
criterion_main!(serialization_benches);
