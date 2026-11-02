---
description: "Run phi-agent as an MCP server with phi serve: bridge protocol over stdio or HTTP so CI pipelines and orchestrators can drive your agent."
---

# phi serve (Bridge Protocol)

`phi serve` exposes phi-agent as an MCP server via the bridge protocol. External orchestrators, CI pipelines, or other tools can interact with the agent over stdio or HTTP.

## Modes

### stdio

```bash
phi serve --transport stdio
```

The agent listens on stdin/stdout using NDJSON (newline-delimited JSON). Each line is a complete JSON-RPC 2.0 message. Suitable for subprocess integration — the orchestrator spawns `phi serve` as a child process and communicates over pipes.

### HTTP

```bash
phi serve --transport http --port 8080
```

The agent listens on an HTTP endpoint accepting JSON-RPC 2.0 requests. Suitable for network-based integrations and remote access.

## Protocol

The bridge protocol uses JSON-RPC 2.0 over the chosen transport:

```
→ {"jsonrpc":"2.0","method":"tools/list","id":1}
← {"jsonrpc":"2.0","result":{"tools":[{"name":"run",...}]},"id":1}

→ {"jsonrpc":"2.0","method":"tools/call","params":{"name":"run","arguments":{"prompt":"..."}},"id":2}
← (progress notifications during execution)
← {"jsonrpc":"2.0","result":{"content":[{"type":"text","text":"..."}]},"id":2}
```

## Exposed tool

The server exposes a single `run` tool:

```json
{
  "name": "run",
  "description": "Execute a task using the phi-agent runtime",
  "inputSchema": {
    "type": "object",
    "properties": {
      "prompt": {
        "type": "string",
        "description": "The task to execute"
      }
    },
    "required": ["prompt"]
  }
}
```

## Programmatic usage

```rust
use phi_agent::PhiAgent;

let agent = PhiAgent::build(builder, config)?;

// Get an MCP server handle for programmatic use
let mcp_server = agent.into_mcp_server();

// Configure and run
let config = phi_agent::McpServerConfig {
    transport: phi_agent::McpServerTransport::Stdio,
    ..Default::default()
};
mcp_server.serve(config).await?;
```

## Why expose the agent, not the tools

| Approach | What's exposed | Problem |
|----------|---------------|---------|
| Expose tool list | Individual tools (search, code_exec, etc.) | phi-agent becomes a tool container; its reasoning and orchestration are bypassed |
| Expose the agent | A single `run` entry point | External orchestration + phi-agent execution, each doing what they do best |

This follows the same pattern as Claude Code's `claude_code()` function and Codex.
