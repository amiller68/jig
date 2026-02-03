# Project Guide

Guide for AI agents and developers working on jig - a git worktree manager.

## Project Overview

`jig` is a Rust CLI tool for managing git worktrees, designed for parallel Claude Code sessions.

## Key Files

**Rust crates:**
- `crates/wt-core/` — Core library (git ops, config, state, spawn)
- `crates/wt-cli/` — CLI binary (`jig`)
- `crates/wt-tui/` — TUI binary (`jig-tui`, placeholder)

**Configuration:**
- `Cargo.toml` — Workspace configuration
- `jig.toml` — Per-repo spawn configuration

**Templates:**
- `templates/skills/` — Claude Code skills for init
- `templates/CLAUDE.md` — Project guide template

**Tests:**
- `tests/` — Integration tests (`cargo test`)

## Development

```bash
cargo build --release    # Build
cargo test               # Run tests
cargo clippy             # Lint
cargo fmt                # Format
```

## Testing

Run all tests:
```bash
cargo test
```

## Documentation

Project docs live in `docs/`:
- `docs/index.md` — Agent instructions for spawned workers
- `docs/issue-tracking.md` — File-based issue tracking convention
- `docs/usage/` — User documentation

## Issues

Track work items in `issues/`. See `docs/issue-tracking.md` for the convention.
