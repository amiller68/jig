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

- Automatic `.jig/` directory for tracking worktrees (gitignored)
- Copying gitignored files (like `.env`) into new worktrees
- tmux integration for spawning and managing agent sessions
- Scaffolding for documentation and issue tracking

If you only need worktrees, use `git worktree` directly. jig helps when you're orchestrating multiple agents and want conventions across your team.

## Why terminal-based?

I like working in my terminal. jig is opinionated about the *general form* of how you work (worktrees, documentation, issues, quality checks) but not *how* you like to do it.

Use whatever editor you want. Use whatever ACA you want. jig manages the orchestration layer.

See [Tips](/tips) for patterns that help agents succeed.
