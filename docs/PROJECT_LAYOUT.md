# Project Layout

Overview of the codebase structure. Update this as the project evolves.

## Directory Structure

```
jig/
├── crates/
│   ├── jig-core/           # Core library
│   │   └── src/
│   │       ├── lib.rs      # Public API exports
│   │       ├── error.rs    # Error types (thiserror)
│   │       ├── git.rs      # Git operations (shell commands)
│   │       ├── worktree.rs # Worktree abstraction
│   │       ├── config.rs   # Configuration management
│   │       ├── worker.rs   # Worker state and lifecycle
│   │       ├── spawn.rs    # Spawn operations
│   │       ├── session.rs  # Tmux session management
│   │       ├── state.rs    # Orchestrator state persistence
│   │       ├── adapter.rs  # Agent adapters (Claude, etc.)
│   │       └── terminal.rs # Terminal detection
│   │
│   ├── jig-cli/            # CLI binary
│   │   └── src/
│   │       ├── main.rs     # Entry point, error handling
│   │       ├── cli.rs      # Clap argument definitions
│   │       └── commands/   # One file per command
│   │           ├── mod.rs
│   │           ├── create.rs
│   │           ├── list.rs
│   │           ├── spawn.rs
│   │           └── ...
│   │
│   └── jig-tui/            # Terminal UI
│       └── src/
│           ├── main.rs
│           ├── app.rs      # Application state
│           └── ui.rs       # Rendering
│
├── templates/              # Templates for jig init
│   ├── PROJECT.md          # -> CLAUDE.md
│   ├── docs/               # Documentation templates
│   ├── issues/             # Issue tracking templates
│   ├── skills/             # Claude Code skills
│   └── adapters/           # Agent-specific config
│
├── tests/
│   └── integration_tests.rs # CLI integration tests
│
├── docs/                   # This documentation
├── issues/                 # Work item tracking
├── .claude/                # Claude Code config for this repo
│   ├── settings.json
│   └── skills/
│
├── Cargo.toml              # Workspace definition
└── jig.toml                # Jig configuration
```

## Key Files

- `Cargo.toml` — Workspace root, defines crates and shared dependencies
- `jig.toml` — Repository configuration for jig itself
- `crates/jig-core/src/lib.rs` — Public API surface for the core library
- `crates/jig-cli/src/main.rs` — CLI entry point
- `crates/jig-core/src/error.rs` — All error types for the project
- `crates/jig-core/src/adapter.rs` — Agent adapter definitions

## Entry Points

- **CLI**: `crates/jig-cli/src/main.rs`
  - Parses args with clap
  - Dispatches to command handlers
  - Handles errors and exit codes

- **TUI**: `crates/jig-tui/src/main.rs`
  - Launches ratatui-based terminal UI

- **Library**: `crates/jig-core/src/lib.rs`
  - Exports public types and functions

## Configuration

- `~/.config/jig/config` — Global user configuration (base branch, hooks)
- `jig.toml` or `wt.toml` — Per-repository configuration
- `.claude/settings.json` — Claude Code settings (when initialized with jig)

## Tests

- `tests/integration_tests.rs` — CLI integration tests
- `crates/jig-core/src/*.rs` — Unit tests inline in modules (under `#[cfg(test)]`)

## Build Output

- `target/debug/` — Debug builds
- `target/release/` — Release builds
- Binary names: `wt` (CLI), `jig-tui` (TUI)
