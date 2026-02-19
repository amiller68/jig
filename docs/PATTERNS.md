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

    fn execute(&self, ctx: &OpContext) -> Result<Self::Output, Self::Error> {
        // ...
    }
}
```

## Module Organization

- **Workspace structure**: Separate crates for different concerns
  - `jig-core` — Pure library with no I/O assumptions
  - `jig-cli` — CLI binary, depends on jig-core
  - `jig-tui` — TUI binary, depends on jig-core

- **Within crates**: One module per domain concept
  - `git.rs` — Low-level git operations via shell commands
  - `worktree.rs` — High-level worktree abstraction
  - `config.rs` — Configuration loading and management
  - `worker.rs` — Worker state and lifecycle

- **Commands**: One file per CLI command in `crates/jig-cli/src/commands/`
  - Each command implements the `Op` trait from `crates/jig-cli/src/op.rs`
  - Commands are registered via `command_enum!` macro in `cli.rs`
  - Doc comments on Args struct become CLI help text (no duplication)

## Naming Conventions

- **Files/modules**: `snake_case.rs`
- **Types/structs**: `PascalCase`
- **Functions/methods**: `snake_case`
- **Constants**: `SCREAMING_SNAKE_CASE`
- **CLI command names**: kebab-case (e.g., `shell-init`, `shell-setup`)

## Output Conventions

- **stderr**: Status messages, progress, errors (with color)
  - Use `eprintln!` with `colored` crate for formatting
  - Prefix success with green checkmark: `"✓".green()`
  - Prefix errors with `"error:".red().bold()`

- **stdout**: Machine-readable output only
  - Shell commands that need to be eval'd (e.g., `cd '/path'`)
  - Data that might be piped to other tools
  - Never include ANSI color codes in stdout

```rust
// Status message (stderr)
eprintln!("{} Created worktree '{}'", "✓".green(), name.cyan());

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

## Common Idioms

- **Git operations**: Shell out to `git` command via `std::process::Command`
  - Parse output with `String::from_utf8_lossy`
  - Check `output.status.success()` before using output

- **Path handling**: Use `PathBuf` for owned paths, `&Path` for references
  - Canonicalize paths before displaying to users
  - Use `to_string_lossy()` when converting to string for git commands

- **Config cascading**: Repo-specific > Global > Default
  - Check repo config first, fall back to global, then hardcoded default

- **Agent adapters**: Use `AgentAdapter` struct for agent-specific behavior
  - Defined in `crates/jig-core/src/adapter.rs`
  - Currently supports Claude Code, extensible for others
