# Skills

Skills are reusable agent behaviors defined as markdown files. phi-agent follows the [agentskills.io](https://agentskills.io) open standard — the same format used by Claude Code, Codex, and other modern agent frameworks.

## Why `.claude` directory

The Skills spec was pioneered by Claude Code, and `.claude/skills/` has become the community's common path. phi-agent follows this convention instead of inventing `.phi/skills/` — so skills you've built for Claude Code work in phi-agent out of the box, with zero migration.

## How skills work

Skills use **prompt-injection** mode:

1. Skill definitions are scanned from `.claude/skills/` and `~/.claude/skills/`
2. The skill list (name + description) is injected into the system prompt
3. When the agent needs a skill, it uses `read_file` to load the full `SKILL.md`
4. No dedicated skill tools — just file I/O + prompt instructions

This aligns with the Claude Code / Codex model: give the agent file access, and skills become discoverable content, not hardcoded tools.

## Directory structure

```
.claude/skills/
  deploy/
    SKILL.md              # Required: YAML frontmatter + markdown body
    scripts/              # Optional: executable helper scripts
    references/           # Optional: on-demand reference docs
    templates/            # Optional: template files
  code-review/
    SKILL.md
    references/
      checklist.md
```

## SKILL.md format

```markdown
---
name: deploy
description: Deploy the current project to production
version: 1.0.0
author: team
tags: [deploy, ops]
allowed-tools: shell, read_file, write_file
user-invocable: true
disable-model-invocation: false
arguments: [branch, env]
---

# Deploy Skill

Deploy the project to the target environment.

## Steps

1. Read the current branch: `read_file .git/HEAD`
2. Run tests before deploying
3. Execute the deploy script: `scripts/deploy.sh $branch $env`

## Scripts

- `scripts/deploy.sh` — main deploy script
- `scripts/rollback.sh` — rollback on failure
```

## Frontmatter fields

| Field | Required | Description |
|-------|----------|-------------|
| `name` | ✅ | kebab-case, ≤64 chars, must match directory name |
| `description` | ✅ | One-line summary used for LLM discovery |
| `version` | No | Semantic version |
| `author` | No | Author name or handle |
| `tags` | No | Comma-separated keywords |
| `allowed-tools` | No | Tool whitelist when skill is active |
| `disallowed-tools` | No | Tools to block when skill is active |
| `model` | No | Model override (`inherit` by default) |
| `user-invocable` | No | Can user trigger via `/skill-name`? (default: true) |
| `disable-model-invocation` | No | Prevent LLM from auto-triggering (for side-effectful skills) |
| `arguments` | No | Parameter placeholders like `[branch, env]` |

## Progressive disclosure

Skills use a three-layer model to save tokens:

| Layer | Content | When |
|-------|---------|------|
| **Discovery** | name + description (~50 tokens per skill) | Always in system prompt |
| **Activation** | SKILL.md body (≤5000 tokens recommended) | When agent reads the file |
| **Execution** | scripts/ + references/ | On-demand during skill execution |

## User-level vs project-level

- `~/.claude/skills/` — user-level, shared across all projects
- `.claude/skills/` — project-level, version-controlled with your code

Project-level skills take priority on name conflicts.

## Creating a skill

```bash
# Scaffold a new skill
phi skill init my-skill

# Result:
# .claude/skills/my-skill/
#   SKILL.md
```

Then edit `SKILL.md` with your skill's instructions and any helper scripts.
