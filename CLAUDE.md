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

## Project Structure

```
crates/
├── jig-core/      # Core library (worktree, config, worker, git)
├── jig-cli/       # CLI binary (wt command)
└── jig-tui/       # Terminal UI

templates/         # Templates for jig init
tests/             # Integration tests
docs/              # Documentation
issues/            # File-based issue tracking
```

For detailed structure, see `docs/PROJECT_LAYOUT.md`.

## Documentation

- `docs/index.md` — Documentation hub and agent instructions
- `docs/PATTERNS.md` — Coding conventions
- `docs/SUCCESS_CRITERIA.md` — CI checks
- `docs/CONTRIBUTING.md` — Contribution guide
- `docs/PROJECT_LAYOUT.md` — Codebase structure

## Issues

Track work items in `issues/`. See `issues/README.md` for the convention.

## Constraints

- Use `thiserror` for typed errors in jig-core, `anyhow::Result` at CLI level
- Integration tests required for new CLI commands
- Status messages go to stderr, machine-readable output to stdout
- CLI binary is named `wt` (not `jig`)

## Do Not

- Push directly to main
- Skip CI checks with --no-verify
- Add new dependencies without justification
- Output ANSI color codes to stdout (breaks shell integration)
