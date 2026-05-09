# Documentation Index

Central hub for project documentation. **Read this first** to find the right docs for your task.

For usage guides (shell integration, worktrees, orchestration, configuration, Linear integration), see the [wiki](https://jig.krondor.org/docs/getting-started/).

## Documentation Map

Find the right doc by what you're working on. The **Sources** column tells you which source files each doc covers — if your task touches those files, read the doc first.

### Core Conventions

| Document | Summary | Sources |
|----------|---------|---------|
| [PATTERNS.md](./PATTERNS.md) | Error handling, Op trait, module layout, output conventions, actor pattern, naming | `crates/jig-cli/src/cli/op.rs`, `crates/jig-cli/src/cli/ui.rs`, `crates/jig-core/src/error.rs` |
| [CONTRIBUTING.md](./CONTRIBUTING.md) | Commit format, PR workflow, agent constraints | — |
| [SUCCESS_CRITERIA.md](./SUCCESS_CRITERIA.md) | CI gate: build, test, clippy, fmt commands | — |

### Architecture

| Document | Summary | Sources |
|----------|---------|---------|
| [daemon.md](./daemon.md) | Tick loop, actor threads, nudging, auto-spawn, auto-prune, PR monitoring | `crates/jig-cli/src/daemon/` |
| [STDOUT-FORMATTING.md](./STDOUT-FORMATTING.md) | Op trait pattern, Display impls, comfy-table usage, color conventions | `crates/jig-cli/src/cli/op.rs`, `crates/jig-cli/src/cli/ui.rs`, `crates/jig-cli/src/cli/commands/*.rs` |

### Operations

| Document | Summary | Sources |
|----------|---------|---------|
| [RELEASING.md](./RELEASING.md) | Conventional commits, cargo-smart-release, CI release workflow | `.github/workflows/` |

## For AI Agents

You are an autonomous coding agent working on a focused task.

### Before Coding

1. Check the **Documentation Map** above — if your task touches files in a Sources column, read those docs
2. Read `PATTERNS.md` for coding conventions
3. Read `SUCCESS_CRITERIA.md` for the CI gate

### Workflow

1. **Understand** — Read the task description and relevant docs
2. **Explore** — Search the codebase to understand context
3. **Plan** — Break down work into small steps
4. **Implement** — Follow existing patterns
5. **Verify** — Run `cargo build && cargo test && cargo clippy && cargo fmt --check`
6. **Commit** — Clear, atomic commits with conventional format

### Guidelines

- Follow existing code patterns and conventions
- Make atomic commits (one logical change per commit)
- Add tests for new functionality
- Update documentation if behavior changes — check if your changed files appear in the Sources column above, and update those docs if the content is now stale
- If blocked, commit what you have and note the blocker
