---
description: "phi-agent memory is file-based and transparent: the agent persists facts as markdown between turns and sessions — no vector DB, no hidden state."
---

# Memory

phi-agent supports file-based memory — the agent can persist information across conversation turns and sessions by reading and writing markdown files.

## How it works

Memory uses a **prompt-injection** approach (same as Claude Code Memory):

1. Memory instructions are injected into the system prompt
2. The agent reads and writes `.md` files in `.phi/memory/`
3. No dedicated `remember`/`recall`/`forget` tools — the agent uses the standard `read_file` and `write_file` kernel tools

This keeps the tool surface small and predictable. The agent decides what to remember and when to recall, just like it decides when to read any other file.

## Memory files

Memory files live in the project directory:

```
.phi/memory/
  user-preferences.md
  project-context.md
  decisions.md
```

Each file is a plain markdown file with optional YAML frontmatter:

```markdown
---
name: my-memory
description: What this memory is about
---

The actual memory content here...
```

## Using memory

Memory is enabled by default (requires the `file` feature). The system prompt instructs the agent to:

- **Before acting**: check `.phi/memory/` for relevant context
- **After important decisions**: write a memory file to persist the context
- **When user asks**: "remember X" or "what do you know about Y?"

## Templates

phi-tools ships with pre-built memory templates for common use cases:

| Template | Purpose |
|----------|---------|
| `user-preferences` | User's preferences, style, conventions |
| `project-context` | Project architecture, tech stack, key decisions |
| `session-notes` | Notes from the current working session |

Use `phi memory init` to scaffold the `.phi/memory/` directory with templates.

## What memory is NOT

- **Not a vector database** — no embeddings, no semantic search. The agent reads and writes plain markdown files in `.phi/memory/`. No vector database needed.
- **Not indiscriminate logging** — the agent intelligently chooses what's worth remembering rather than recording everything. You can also explicitly ask it to remember or recall things.
- **Not hidden state** — all memory files are plain markdown in your project. You can read, edit, or delete them anytime.

For RAG or semantic memory, bring your own vector DB (Qdrant, pgvector, LanceDB) and register it as a tool.
