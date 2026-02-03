# Agent Instructions

You are working on `scribe` — a Rust CLI for managing git worktrees, designed for parallel Claude Code sessions.

## Key Files

```
crates/
├── wt-core/           # Core library
│   └── src/
│       ├── config.rs   # Config management (scribe.toml, ~/.config/scribe/)
│       ├── git.rs      # Git operations (worktrees, branches)
│       ├── spawn.rs    # Spawn state tracking
│       ├── session.rs  # tmux session/window management
│       ├── state.rs    # Persistent orchestrator state
│       └── worker.rs   # Worker/task model
├── wt-cli/            # CLI binary (scribe)
│   └── src/
│       ├── cli.rs      # Command definitions (clap)
│       └── commands/   # Command handlers
└── wt-tui/            # TUI binary (placeholder)
templates/
├── skills/            # Claude Code skills for init
│   ├── check/         # /check skill
│   ├── draft/         # /draft skill
│   ├── issues/        # /issues skill
│   ├── review/        # /review skill
│   └── scribe/        # /scribe skill
└── CLAUDE.md          # Project guide template
tests/                 # Integration tests
```

## Development

```bash
cargo build --release    # Build
cargo test               # Run all tests
cargo clippy             # Lint
cargo fmt                # Format
```

## Commands

| Command | Purpose |
|---------|---------|
| `create` | Create a new worktree branch |
| `list` | List worktrees (local or --all) |
| `open` | cd into a worktree (--all opens tabs) |
| `remove` | Remove worktree(s), supports glob patterns |
| `exit` | Remove current worktree and return to base |
| `config` | Get/set base branch, on-create hooks |
| `spawn` | Create worktree + launch Claude in tmux |
| `ps` | Show status of spawned sessions |
| `attach` | Attach to a tmux spawn session |
| `review` | Show diff for spawned worktree |
| `merge` | Merge spawned work into current branch |
| `kill` | Kill a spawned tmux window |
| `init` | Initialize scribe config for a repo |
| `health` | Check dependencies and config |
| `shell-init` | Print shell integration code |

## Architecture

- **Worktrees** live in `.worktrees/` (auto-excluded via `.git/info/exclude`)
- **Config** stored in `~/.config/scribe/config`
- **Spawn state** tracked in `.worktrees/.wt-state.json`
- **tmux** manages spawned sessions: `scribe-<reponame>` session, one window per task
- **Shell integration**: wrapper function evals `cd` commands from stdout
- **stdout** is reserved for eval-able output — all user-facing messages go to stderr

## Testing

- **Run:** `cargo test`
- **Structure:** Integration tests in `tests/` directory
- **Isolation:** Tests use temporary git repos

## Workflow

1. Read the task description
2. Explore the codebase for context and patterns
3. Implement following existing conventions
4. Run `cargo test`
5. Commit with a clear message

## When Complete

Your work will be reviewed and merged by the parent session.
Ensure all tests pass before finishing.
