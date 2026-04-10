# Documentation Index

Central hub for project documentation. **Read this first** to find the right docs for your task.

## Quick Start

```bash
cargo build              # Build all crates
cargo test               # Run all tests
cargo clippy             # Lint
cargo fmt --check        # Check formatting
cargo run -- <args>      # Run CLI (e.g., cargo run -- list)
```

## Documentation Map

Find the right doc by what you're working on. The **Sources** column tells you which source files each doc covers — if your task touches those files, read the doc first.

### Core Conventions

| Document | Summary | Sources |
|----------|---------|---------|
| [PATTERNS.md](./PATTERNS.md) | Error handling, Op trait, module layout, output conventions, actor pattern, naming | `crates/jig-cli/src/op.rs`, `crates/jig-cli/src/ui.rs`, `crates/jig-core/src/error.rs`, `crates/jig-core/src/adapter.rs` |
| [CONTRIBUTING.md](./CONTRIBUTING.md) | Commit format, PR workflow, agent constraints | — |
| [SUCCESS_CRITERIA.md](./SUCCESS_CRITERIA.md) | CI gate: build, test, clippy, fmt commands | — |

### Architecture

| Document | Summary | Sources |
|----------|---------|---------|
| [daemon.md](./daemon.md) | Tick loop, actor threads, nudging, auto-spawn, auto-prune, PR monitoring | `crates/jig-core/src/daemon/*.rs` |
| [Automated Review](../design/review/index.md) | Review agent, data model, trigger flow, comment routing | `crates/jig-core/src/review.rs`, `crates/jig-core/src/daemon/review_actor.rs`, `crates/jig-cli/src/commands/review/*.rs` |
| [CLI Output Formatting](./cli/ui/STDOUT-FORMATTING.md) | Op trait pattern, Display impls, comfy-table usage, color conventions | `crates/jig-cli/src/op.rs`, `crates/jig-cli/src/ui.rs`, `crates/jig-cli/src/commands/*.rs` |

### CLI Usage

| Document | Summary | Sources |
|----------|---------|---------|
| [Shell Integration](./cli/usage/shell-integration.md) | Shell function, tab completion, `-o` flag, troubleshooting | `crates/jig-cli/src/commands/shell_init.rs`, `crates/jig-cli/src/commands/shell_setup.rs` |
| [Worktrees](./cli/usage/worktrees.md) | Create, open, remove worktrees; glob patterns; nested paths | `crates/jig-cli/src/commands/create.rs`, `crates/jig-cli/src/commands/remove.rs`, `crates/jig-core/src/worktree.rs` |
| [Orchestration](./cli/usage/orchestration.md) | Multi-agent workflow: spawn, monitor, review, merge workers | `crates/jig-cli/src/commands/spawn.rs`, `crates/jig-cli/src/commands/merge.rs`, `crates/jig-cli/src/commands/review.rs` |
| [Configuration](./cli/usage/configuration.md) | jig.toml, jig.local.toml, global config, on-create hooks, file copying | `crates/jig-core/src/config.rs`, `crates/jig-core/src/global/*.rs` |
| [Init](./cli/usage/init.md) | `jig init` bootstrapping, --audit, --backup, template system | `crates/jig-cli/src/commands/init.rs`, `templates/` |
| [Linear Integration](./cli/usage/linear-integration.md) | Linear API setup, status/priority mapping, label-based auto-spawn | `crates/jig-core/src/issues/linear.rs` |

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
