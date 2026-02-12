# Agent Instructions

You are an autonomous coding agent working on jig, a git worktree manager for parallel Claude Code sessions.

## Build & Test

```bash
cargo build                    # Build all crates
cargo test                     # Run all tests
cargo clippy                   # Run linter
cargo fmt --check              # Check formatting
```

Always run `cargo test` before committing to verify your changes work.

## Project Structure

```
crates/
├── jig-core/                  # Core library
│   └── src/
│       ├── config.rs          # Config management (jig.toml, ~/.config/jig/)
│       ├── git.rs             # Git operations
│       ├── spawn.rs           # Worker spawning and tmux integration
│       ├── worker.rs          # Worker state machine
│       └── worktree.rs        # Worktree operations
├── jig-cli/                   # CLI binary
│   └── src/
│       ├── cli.rs             # Clap argument definitions
│       └── commands/          # One module per command
└── jig-tui/                   # Terminal UI

templates/                     # Templates for `jig init`
tests/integration_tests.rs     # Integration tests
```

## Workflow

1. **Understand** - Read the task description and related code
2. **Explore** - Search for existing patterns (`Grep`, `Glob`)
3. **Plan** - Break down the work into small steps
4. **Implement** - Follow existing conventions
5. **Test** - Run `cargo test` to verify
6. **Commit** - One logical change per commit

## Coding Conventions

### Error Handling
- Use `anyhow::Result` in CLI commands
- Use `jig_core::Result` (wraps `jig_core::Error`) in library code
- Define new error variants in `crates/jig-core/src/error.rs`

### Output Conventions
- Errors and status messages go to **stderr** (use `eprintln!`)
- Machine-readable output (like paths for shell `cd`) goes to **stdout**
- Use `colored` crate for terminal colors (only on stderr)

### CLI Commands
- Each command lives in `crates/jig-cli/src/commands/<name>.rs`
- Export a `pub fn run(...) -> Result<()>` function
- Register in `commands/mod.rs` and `cli.rs`

### Tests
- Integration tests use `TestRepo` struct from `tests/integration_tests.rs`
- Tests create isolated temp git repos with `tempfile`
- Use `assert_cmd` for CLI testing

## Key Patterns

### Adding a New Command
1. Create `crates/jig-cli/src/commands/mycommand.rs`
2. Add to `crates/jig-cli/src/commands/mod.rs`
3. Add enum variant to `Commands` in `cli.rs`
4. Add match arm in `main.rs`

### Worktree Operations
```rust
use jig_core::{git, Worktree};

let worktrees_dir = git::get_worktrees_dir()?;
let worktree = Worktree::create(&worktrees_dir, &git_dir, name, branch, base)?;
```

### Worker State
Workers have a state machine: `Spawned → Running → WaitingReview → Approved → Merged`

## When Complete

Ensure all tests pass (`cargo test`) before finishing. Your work will be reviewed and merged by the parent session.
