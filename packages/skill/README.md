# @nodespaceai/skill

Install the NodeSpace skill into PTY agents (Claude Code, Codex, Gemini CLI, OpenCode) with a single command.

## Installation

```bash
npx @nodespaceai/skill install
```

Detects which agents are present on your machine and installs the NodeSpace `SKILL.md` into each agent's skills directory.

## Usage

```bash
# Install for all detected agents
npx @nodespaceai/skill install

# Install for a specific agent
npx @nodespaceai/skill install claude-code
npx @nodespaceai/skill install codex
npx @nodespaceai/skill install gemini
npx @nodespaceai/skill install opencode

# Uninstall
npx @nodespaceai/skill uninstall
npx @nodespaceai/skill uninstall claude-code
```

## Supported Agents

| Agent | Detection | Install path |
|-------|-----------|--------------|
| Claude Code | `~/.claude/` exists | `~/.claude/skills/nodespace/SKILL.md` |
| Codex | `~/.codex/` exists | `~/.codex/skills/nodespace/SKILL.md` |
| Gemini CLI | `~/.gemini/` exists | `~/.gemini/skills/nodespace/SKILL.md` |
| OpenCode | `~/.opencode/` exists | `~/.opencode/skills/nodespace/SKILL.md` |

## Prerequisites

The `nodespace` CLI must be on `$PATH`. Install it via the [NodeSpace desktop app](https://nodespace.ai) or the shell installer.

## Programmatic API

```ts
import { install, uninstall } from '@nodespaceai/skill';

// Install for all detected agents
const results = install();

// Install for specific agents
const results = install(['claude-code', 'codex']);

// Uninstall
const results = uninstall();
```
