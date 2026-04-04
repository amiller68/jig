# Project Guide

Git worktree manager for parallel Claude Code sessions.

## Quick Reference

```bash
cargo build              # Build all crates
cargo test               # Run all tests
cargo clippy             # Lint
cargo fmt --check        # Check formatting
cargo fmt                # Fix formatting
cargo run -- <args>      # Run CLI (e.g., cargo run -- list)
```

## Documentation

**Read `docs/index.md` first.** It has a source-file-aware map of all docs — find the right doc by which files you're touching.

Key docs:
- `docs/PATTERNS.md` — Coding conventions (error handling, Op trait, output, actors)
- `docs/SUCCESS_CRITERIA.md` — CI gate commands
- `docs/daemon.md` — Daemon architecture (`crates/jig-core/src/daemon/`)

## Issues

Track work items in `issues/`. See `issues/README.md` for the convention.

## Constraints

- Use `thiserror` for typed errors in jig-core, `anyhow::Result` at CLI level
- Integration tests required for new CLI commands
- Status messages go to stderr, machine-readable output to stdout
- CLI binary is named `jig`

## Do Not

- Push directly to main
- Skip CI checks with --no-verify
- Add new dependencies without justification
- Output ANSI color codes to stdout (breaks shell integration)
