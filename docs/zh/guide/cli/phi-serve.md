# phi 服务（Bridge 协议）

`phi serve` 通过 Bridge 协议将 phi-agent 暴露为 MCP 服务器。外部编排器、CI 流水线或其他工具可通过 stdio 或 HTTP 与 Agent 交互。

## 模式

### stdio

```bash
phi serve --transport stdio
```

Agent 通过 stdin/stdout 使用 NDJSON（换行分隔的 JSON）进行通信。每行是一个完整的 JSON-RPC 2.0 消息。适用于子进程集成 — 编排器将 `phi serve` 作为子进程启动，通过管道通信。

### HTTP

```bash
phi serve --transport http --port 8080
```

Agent 在 HTTP 端点上监听 JSON-RPC 2.0 请求。适用于基于网络的集成和远程访问。

## 协议

Bridge 协议在所选传输层上使用 JSON-RPC 2.0：

```
→ {"jsonrpc":"2.0","method":"tools/list","id":1}
← {"jsonrpc":"2.0","result":{"tools":[{"name":"run",...}]},"id":1}

→ {"jsonrpc":"2.0","method":"tools/call","params":{"name":"run","arguments":{"prompt":"..."}},"id":2}
← （执行过程中的 progress 通知）
← {"jsonrpc":"2.0","result":{"content":[{"type":"text","text":"..."}]},"id":2}
```

## 暴露的工具

服务器暴露单个 `run` 工具：

```json
{
  "name": "run",
  "description": "使用 phi-agent 运行时执行任务",
  "inputSchema": {
    "type": "object",
    "properties": {
      "prompt": {
        "type": "string",
        "description": "要执行的任务"
      }
    },
    "required": ["prompt"]
  }
}
```

## 编程式使用

```rust
use phi_agent::PhiAgent;

let agent = PhiAgent::build(builder, config)?;

// 获取 MCP server 句柄用于编程式使用
let mcp_server = agent.into_mcp_server();

// 配置并运行
let config = phi_agent::McpServerConfig {
    transport: phi_agent::McpServerTransport::Stdio,
    ..Default::default()
};
mcp_server.serve(config).await?;
```

## 为什么暴露 Agent 而非工具列表

| 做法 | 暴露内容 | 问题 |
|------|---------|------|
| 暴露工具列表 | 单个工具（search、code_exec 等） | phi-agent 沦为工具容器，推理和编排能力被绕过 |
| 暴露 Agent | 单个 `run` 入口 | 外部编排 + phi-agent 执行，各司其职 |

这与 Claude Code 的 `claude_code()` 函数和 Codex 遵循相同的模式。
