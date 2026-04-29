# Coding Patterns

Document your team's coding patterns and conventions here. This helps AI agents and contributors follow consistent practices.

## Error Handling

- **jig-core**: Use `thiserror` for typed errors with `#[derive(Error)]`
  - Define domain-specific errors in `crates/jig-core/src/error.rs`
  - Return `Result<T>` using the crate's custom `Result` type alias
  - Errors should have clear, user-facing messages

- **jig-cli**: Use the `Op` trait with typed errors per command
  - Each command has its own error enum wrapping core errors
  - Infallible commands use `std::convert::Infallible`
  - Main function catches errors and prints to stderr with color
  - Exit with code 1 on any error

```rust
// In jig-core (typed errors)
#[derive(Error, Debug)]
pub enum Error {
    #[error("Worktree '{0}' does not exist")]
    WorktreeNotFound(String),
}

// In jig-cli (Op trait with typed output and errors)
#[derive(Args, Debug, Clone)]
pub struct Create { /* args */ }

#[derive(Debug, thiserror::Error)]
pub enum CreateError {
    #[error(transparent)]
    Core(#[from] jig_core::Error),
}

impl Op for Create {
    type Error = CreateError;
    type Output = CreateOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        // ...
    }
}
```

## Module Organization

- **Workspace structure**: Separate crates for different concerns
  - `jig-core` — Pure library: git, GitHub, issues, mux, agents, prompt
  - `jig-cli` — CLI binary with config, daemon, worker, hooks, notify

- **jig-core modules**: One submodule directory per domain
  - `git/` — Git operations via git2: `Repo`, `Worktree`, `Branch`, `WorktreeRef`
  - `mux/` — Multiplexer traits (`MuxSession`, `MuxWindow`) + tmux implementation
  - `issues/` — Issue provider trait + Linear implementation
  - `agents/` — Agent adapters (Claude Code)
  - `github/` — GitHub API client and queries
  - `prompt/` — Handlebars-based prompt rendering

- **jig-cli modules**:
  - `cli/` — CLI framework: `op.rs` (Op trait + command_enum! macro), `ui.rs` (rendering), `commands/` (one file per command)
  - `config/` — Configuration loading and management
  - `worker/` — Worker state, lifecycle, events
  - `daemon/` — Background daemon with actor threads
  - `hooks/` — Git and agent hook management
  - `notify/` — Notification system

- **Commands**: One file per CLI command in `crates/jig-cli/src/cli/commands/`
  - Each command implements the `Op` trait from `crates/jig-cli/src/cli/op.rs`
  - Commands are registered via `command_enum!` macro in `cli/mod.rs`
  - Doc comments on Args struct become CLI help text (no duplication)

## Naming Conventions

- **Files/modules**: `snake_case.rs`
- **Types/structs**: `PascalCase`
- **Functions/methods**: `snake_case`
- **Constants**: `SCREAMING_SNAKE_CASE`
- **CLI command names**: kebab-case (e.g., `shell-init`, `shell-setup`)

## Output Conventions

- **stderr**: Status messages, progress, errors (with color)
  - Use shared helpers from `crates/jig-cli/src/cli/ui.rs` instead of inline `colored` calls
  - `ui::success("msg")` — green ✓ prefix
  - `ui::progress("msg")` — cyan → prefix
  - `ui::warning("msg")` — yellow ! prefix
  - `ui::failure("msg")` — red ✗ prefix
  - `ui::detail("msg")` — indented → for sub-items
  - `ui::header("msg")` — bold section header
  - `ui::highlight("val")`, `ui::bold("val")`, `ui::dim("val")` — inline color helpers
  - All helpers respect `--plain` flag (no colors when enabled)

- **stdout**: Machine-readable output only
  - Shell commands that need to be eval'd (e.g., `cd '/path'`)
  - Data that might be piped to other tools
  - Never include ANSI color codes in stdout

- **`--plain` flag**: Global flag for scriptable output
  - Disables all colors and decorations
  - Tables output as tab-separated values
  - Status symbols still appear but without color

```rust
// Status message (stderr) — use ui helpers
ui::success(&format!("Created worktree '{}'", ui::highlight(name)));

// Tables — use ui::new_table for consistent styling
let mut table = ui::new_table(&["NAME", "BRANCH", "COMMITS"]);

// Machine-readable output (stdout)
println!("cd '{}'", canonical.display());
```

## Testing Patterns

- **Unit tests**: Inline in source files with `#[cfg(test)]` modules
  - Test pure functions and internal logic
  - Located at bottom of the file being tested

- **Integration tests**: In `tests/` directory
  - Use `assert_cmd` for CLI testing
  - Use `tempfile` for isolated test repos
  - Test helper: `TestRepo` struct creates isolated git repos

```rust
#[test]
fn test_create_worktree() {
    let repo = TestRepo::new();
    repo.jig()
        .args(["create", "test1"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Created worktree"));
}
```

## Actor Pattern (Daemon)

The daemon (`crates/jig-cli/src/daemon/`) uses background actor threads for blocking I/O. Each actor implements the `Actor` trait:

```rust
pub trait Actor: Default + Send + Sync + 'static {
    type Request: Send + 'static;
    type Response: Send + 'static;

    const NAME: &'static str;
    const QUEUE_SIZE: usize;

    fn handle(&self, req: Self::Request) -> Self::Response;
}
```

Actors are managed via `ActorHandle<A>`, which owns the channel pair, background thread, and pending state.

Key conventions:
- Actor owns its own resources (e.g., `GitHubClient`)
- Communication is non-blocking on the tick thread (`try_send`, `try_recv`)
- Drop requests on backpressure when appropriate (nudges are best-effort)
- Bounded channels prevent unbounded memory growth

## Common Idioms

- **Git operations**: Use `git::Repo` wrapper around `git2::Repository`
  - Instance methods for operations requiring repo context (branch, worktree, merge)
  - Associated functions for path-scoped operations (diff, status, commits ahead)
  - Errors propagate via `#[from] git2::Error` in the `Error` enum

- **Multiplexer abstraction**: `MuxSession` and `MuxWindow` traits in `jig-core/src/mux/`
  - `Worker<W: MuxWindow = TmuxWindow>` is generic over the mux backend
  - Use `TmuxWorker` type alias for the concrete tmux-backed worker

- **Path handling**: Use `PathBuf` for owned paths, `&Path` for references
  - Canonicalize paths before displaying to users
  - Use `to_string_lossy()` when converting to string for git commands

- **Config cascading**: Repo-specific > Global > Default
  - Check repo config first, fall back to global, then hardcoded default

- **Unified Config**: Load all config once, thread through all operations
  - `Config::from_cwd()` loads global config, repo config (jig.toml + jig.local.toml), and git-derived paths
  - CLI `RepoCtx` holds `config: Option<Config>` (None when not in a git repo)
  - Commands call `ctx.config()?` to get `&Config`, pass it to jig-core functions
  - Methods on Config: `base_branch()`, `session_name()`, `issue_provider()`, `linear_provider()`

- **Agent adapters**: Use `Agent` struct for agent-specific behavior
  - Defined in `crates/jig-core/src/agents/`
  - Currently supports Claude Code, extensible for others
