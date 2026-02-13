# Documentation Index

Central hub for project documentation. AI agents should read this first.

## Quick Start

```bash
# Build the project
cargo build

# Run tests
cargo test

# Run linter
cargo clippy

# Check formatting
cargo fmt --check

# Run the CLI
cargo run -- list
cargo run -- create <name>
cargo run -- spawn <name> --context "task description"
```

## Documentation

| Document | Purpose |
|----------|---------|
| [PATTERNS.md](./PATTERNS.md) | Coding conventions and patterns |
| [CONTRIBUTING.md](./CONTRIBUTING.md) | How to contribute (agents + humans) |
| [SUCCESS_CRITERIA.md](./SUCCESS_CRITERIA.md) | CI checks that must pass |
| [PROJECT_LAYOUT.md](./PROJECT_LAYOUT.md) | Codebase structure overview |
| [RELEASING.md](./RELEASING.md) | Release workflow and automation |

## For AI Agents

You are an autonomous coding agent working on a focused task.

### Workflow

1. **Understand** — Read the task description and relevant docs
2. **Explore** — Search the codebase to understand context
3. **Plan** — Break down work into small steps
4. **Implement** — Follow existing patterns in `PATTERNS.md`
5. **Verify** — Run checks from `SUCCESS_CRITERIA.md`
6. **Commit** — Clear, atomic commits

### Guidelines

- Follow existing code patterns and conventions
- Make atomic commits (one logical change per commit)
- Add tests for new functionality
- Update documentation if behavior changes
- If blocked, commit what you have and note the blocker

### When Complete

Your work will be reviewed and merged by the parent session.
Ensure all checks pass before finishing.
