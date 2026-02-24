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
│   │       ├── context.rs  # RepoContext (derived once, threaded through)
│   │       ├── git.rs      # Git operations (shell commands)
│   │       ├── worktree.rs # Worktree abstraction
│   │       ├── config.rs   # Configuration management
│   │       ├── worker.rs   # Worker state and lifecycle
│   │       ├── spawn.rs    # Spawn operations
│   │       ├── session.rs  # Tmux session management
│   │       ├── state.rs    # Orchestrator state persistence
│   │       ├── adapter.rs  # Agent adapters (Claude, etc.)
│   │       ├── registry.rs # Repository registry for global mode
│   │       ├── terminal.rs # Terminal detection
│   │       ├── events/     # Event log system (JSONL per worker)
│   │       │   ├── mod.rs      # Re-exports
│   │       │   ├── schema.rs   # Event/EventType structs
│   │       │   ├── log.rs      # EventLog JSONL reader/writer
│   │       │   ├── derive.rs   # State derivation from events
│   │       │   └── reducer.rs  # WorkerState reducer
│   │       ├── dispatch/   # Action dispatch for state transitions
│   │       │   ├── mod.rs      # Re-exports
│   │       │   ├── actions.rs  # Action enum
│   │       │   └── rules.rs    # Dispatch rules
│   │       ├── hooks/      # Hook management
│   │       │   ├── mod.rs      # Re-exports
│   │       │   ├── claude.rs   # Claude Code hook installation
│   │       │   └── templates/  # Shell script templates
│   │       ├── notify/     # Notification system
│   │       │   ├── mod.rs      # Re-exports
│   │       │   ├── events.rs   # NotificationEvent types
│   │       │   ├── queue.rs    # NotificationQueue (JSONL)
│   │       │   └── hook.rs     # Notifier with hook execution
│   │       └── global/     # Global state infrastructure (~/.config/jig/)
│   │           ├── mod.rs      # Re-exports
│   │           ├── paths.rs    # XDG path helpers
│   │           ├── config.rs   # Structured TOML config (config.toml)
│   │           └── state.rs    # Aggregated worker state (workers.json)
│   │
│   ├── jig-cli/            # CLI binary
│   │   └── src/
│   │       ├── main.rs     # Entry point, error handling
│   │       ├── cli.rs      # Clap argument definitions
│   │       ├── op.rs       # Op trait and OpContext (holds RepoContext)
│   │       └── commands/   # One file per command
│   │           ├── mod.rs
│   │           ├── create.rs
│   │           ├── list.rs
│   │           ├── spawn.rs
│   │           └── ...
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

- **Library**: `crates/jig-core/src/lib.rs`
  - Exports public types and functions

## Configuration

- `~/.config/jig/config` — Legacy flat key-value user configuration (base branch, hooks)
- `~/.config/jig/config.toml` — Structured global configuration (health, notify)
- `~/.config/jig/state/workers.json` — Aggregated worker state across repos
- `~/.config/jig/hooks/` — User hook scripts
- `~/.config/jig/state/events/` — Per-worker event logs
- `jig.toml` or `jig.toml` — Per-repository configuration
- `.claude/settings.json` — Claude Code settings (when initialized with jig)

## Tests

- `tests/integration_tests.rs` — CLI integration tests
- `crates/jig-core/src/*.rs` — Unit tests inline in modules (under `#[cfg(test)]`)

## Build Output

- `target/debug/` — Debug builds
- `target/release/` — Release builds
- Binary name: `jig`
