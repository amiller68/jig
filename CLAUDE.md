# Project Guide

jig is a git worktree manager for running parallel Claude Code sessions. It creates isolated worktrees with tmux integration for multi-agent orchestration.

## Quick Reference

```bash
cargo build                    # Build all crates
cargo test                     # Run tests
cargo clippy                   # Lint
cargo fmt --check              # Check formatting
cargo build --release          # Release build
```

## Architecture

```
crates/
├── jig-core/     # Core library (worktree, worker, config, git, spawn)
├── jig-cli/      # CLI binary (commands/)
└── jig-tui/      # Terminal UI (not yet fully implemented)

templates/        # Templates for `jig init` (CLAUDE.md, skills, docs)
tests/            # Integration tests
```

### Key Modules

- `jig-core::worktree` — Git worktree operations (create, list, remove)
- `jig-core::worker` — Worker state machine (Spawned → Running → WaitingReview → Merged)
- `jig-core::spawn` — Tmux session management and worker registration
- `jig-core::config` — Global config (~/.config/jig/config) and repo config (jig.toml)
- `jig-core::git` — Low-level git operations

### CLI Commands

Commands are in `crates/jig-cli/src/commands/`. Each command is a separate module with a `run()` function.

## Conventions

- Use `anyhow::Result` for CLI commands, `jig_core::Result` for library code
- Errors go to stderr, machine-readable output (like `cd` paths) goes to stdout
- The `-o` flag outputs a `cd` command to stdout for shell integration
- Worktrees live in `.worktrees/` (automatically gitignored)
- Tests use `tempfile` for isolated git repos

## Documentation

- `docs/index.md` — Agent instructions for spawned workers

## Issues

Track work items in `issues/`. See `issues/README.md` for the convention.
