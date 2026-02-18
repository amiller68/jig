---
layout: page
title: Why jig?
nav_order: 6
---

# Why jig?

## Why not a full product like Cursor?

Every team is different. Sometimes different repos within a team have different standards. jig doesn't try to be an everything tool—it's a lightweight way to bootstrap repositories to fit a parallel-agent workflow.

jig is:
- **Malleable** — Extend it with your own skills and conventions
- **Minimal** — A thin layer over git worktrees and tmux
- **Terminal-native** — Stays out of your way

If you want an integrated IDE experience, use Cursor or Windsurf. If you want a lightweight framework that works with any terminal-based ACA and respects your existing tooling, use jig.

## Why not just use git worktrees directly?

You can! jig is mostly conveniences:

- Automatic `.worktrees/` directory (gitignored)
- Copying gitignored files (like `.env`) into new worktrees
- tmux integration for spawning and managing agent sessions
- Scaffolding for documentation and issue tracking

If you only need worktrees, use `git worktree` directly. jig helps when you're orchestrating multiple agents and want conventions across your team.

## Why terminal-based?

I like working in my terminal. jig is opinionated about the *general form* of how you work (worktrees, documentation, issues, quality checks) but not *how* you like to do it.

Use whatever editor you want. Use whatever ACA you want. jig manages the orchestration layer.

## Why Rust?

| Shell scripts | Rust binary |
|---------------|-------------|
| Requires jq, specific bash version | Single binary, zero runtime deps |
| Silent failures, string errors | Proper types, clear messages |
| `curl \| bash` distribution | `cargo install`, binary download |
| Painful to extend | Straightforward |

## Hot tips for agent-friendly repos

From experience, these patterns help agents succeed:

### Dependency injection

Inject dependencies everywhere. It makes code:
- Testable (agents can verify their work)
- Documentable (clear interfaces)
- Extendable (agents understand how to add functionality)

### Runtime configuration

Make configuration injectable and discoverable. Agents can then understand and modify behavior without hunting through hardcoded values.

### Local development tools

Use local tools when possible. Write tooling for:
- Dynamic port allocation
- Service discovery
- Environment setup

This helps agents spin up isolated environments per worktree without conflicts.

### Document the patterns

If agents can find your patterns documented, they'll follow them. If they can't, they'll invent their own—possibly inconsistent with your codebase.

Invest time in:
- `PATTERNS.md` with coding conventions
- `CONTRIBUTING.md` with workflow
- `CLAUDE.md` with quick reference

The compounding returns across agent sessions are worth it.
