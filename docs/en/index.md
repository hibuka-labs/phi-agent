---
hide:
  - toc
  - navigation
---

<h1>
  <img src="favicon.svg" style="height: 1.2em; vertical-align: middle; margin-right: 0.3em;">
  phi-agent
</h1>

<div class="phi-hero" markdown>

**Don't just let AI chat — let it finish the job**

Generic agents are everywhere, but the real work — your work — needs an agent that knows your business. phi-agent is the lightweight runtime foundation for building it — give an LLM tools, memory, and an execution loop, and it finishes the job on its own, you bring the tools, prompts, and domain knowledge.

<a href="guide/getting-started/" class="md-button md-button--primary" style="margin-right: 0.5rem">
  :octicons-arrow-right-24: &nbsp; Get Started
</a>
<a href="https://github.com/hibuka-labs/phi-agent" class="md-button" target="_blank">
  :octicons-mark-github-16: &nbsp; View on GitHub
</a>

</div>

---

<div class="grid cards col-1" markdown>

-   :material-target:{ .lg .middle } **Your domain, your rules**

    ---

    Generic agents know a little about everything — except your business. And you can't change them. No black box. No compromise. No “wait for the next release.” Your experience becomes the prompt, your business becomes the tools — your agent grows out of your domain.

-   :material-rocket-launch-outline:{ .lg .middle } **One file. Minimal. Fast.**

    ---

    A tool is three methods: `name()`, `definition()`, `call()`. Compiles to a single binary — instant startup, zero wasted memory. From cloud servers to edge devices, wherever Rust runs, it runs.

-   :material-chart-line:{ .lg .middle } **Every step, on the record**

    ---

    Every LLM call is logged. Every tool execution leaves a trace. Sessions can be snapshotted, behavior replayed, incidents pinpointed — you build it, you see it.

</div>

---

## Ten lines, one agent

Below: the user says *”I feel a bit cold”* — the agent raises the temperature. Your `SmartAc` tool wraps the air conditioner protocol. Your prompt defines the butler. The framework handles the rest.

```rust
let llm = Arc::new(OpenAiClient::new(api_key, “gpt-4o”.into(), None));

let agent = PhiAgent::build(
    base_agent_builder(llm)
        .system_prompt(format!(
            “You control a smart home.\n\n{}”,
            // framework: agent loop
            build_system_prompt()
        ))
        // your tool
        .register_tool(SmartAc),
    PhiAgentConfig::default(),
)?;

let session = agent.create_session().await;
let mut renderer = create_stdout_renderer(&OutputFormat::default());
agent.run_turn(session, “I feel a bit cold”, |e| renderer.render(e)).await?;
```

```bash
cargo add phi-agent
```

Full runnable version: [`examples/minimal/landing.rs`](https://github.com/hibuka-labs/phi-agent/blob/master/examples/minimal/landing.rs).

---

## Architecture

```mermaid
graph TB
    AB[agent-base<br/>Runtime kernel<br/>Tool trait · LLM clients]

    AB --> AW[agent-works<br/>MCP · Skills · Focus]
    AB --> PKT["phi-kernel-tools<br/>Kernel tools"]
    AB --> YT[your-tools<br/>Custom Tool impls]

    AW --> PA
    PKT --> PA
    YT --> PA

    PA[phi-agent<br/>Builder factory · Renderers<br/>Config · Session · CLI]
```

Each crate is a separate repository under [hibuka-labs](https://github.com/hibuka-labs). All built on Rust's async runtime with `Arc<dyn LlmClient>` for provider abstraction. File tools and MCP are on by default; shell and multi-agent are opt-in — see [Kernel Tools](guide/tools/file-tools/) for details.

## Links

<div class="grid cards" markdown>

-   [:octicons-mark-github-16: GitHub](https://github.com/hibuka-labs/phi-agent)

    Source code, issues, discussions.

-   [:simple-rust: crates.io](https://crates.io/crates/phi-agent)

    Add with `cargo add phi-agent`.

-   [:material-bookshelf: API Docs](https://docs.rs/phi-agent)

    Full Rustdoc on docs.rs.

</div>

---

:material-email-outline: **Contact** &nbsp; [phiagent@hibuka.com](mailto:phiagent@hibuka.com)
