# Add `jig home` command to navigate to base repo root

**Status:** Planned
**Priority:** Medium
**Labels:** auto

## Objective

Add a `jig home` (alias `jig h`) command that prints the base repo root, so users can `cd` to the root of their base repo from anywhere in a worktree.

## Context

When working in a jig-managed worktree, there's no quick way to navigate back to the base repository root. `jig home` solves this by printing the `repo_root` path from `RepoContext`. Combined with shell integration (`cd $(jig home)`), this gives fast navigation.

This command is **not global** — it operates on the current repo only.

## Implementation

1. Create `crates/jig-cli/src/commands/home.rs`:
   - Define `Home` struct (no args) with `/// Go to base repository root` doc comment
   - `HomeOutput(PathBuf)` with `Display` that prints the path
   - `HomeError` with a `jig_core::Error` variant for the not-in-repo case
   - Implement `Op::run()`: call `ctx.repo()?.repo_root` and return it

2. Register in `crates/jig-cli/src/commands/mod.rs`:
   - Add `pub mod home;` and `pub use home::Home;`

3. Register in `crates/jig-cli/src/cli.rs`:
   - Add to `command_enum!`:
     ```rust
     #[command(visible_alias = "h")]
     (Home, commands::Home),
     ```

## Files

- `crates/jig-cli/src/commands/home.rs` — New command implementation
- `crates/jig-cli/src/commands/mod.rs` — Module registration
- `crates/jig-cli/src/cli.rs` — Command enum registration with alias

## Acceptance Criteria

- [ ] `jig home` prints the base repo root path to stdout
- [ ] `jig h` works as an alias
- [ ] Works from within a worktree (prints parent repo root, not worktree path)
- [ ] Works from the base repo root itself
- [ ] Errors with `NotInGitRepo` when run outside a git repo
- [ ] Not a global command (no `-g` behavior needed)
- [ ] `cd $(jig home)` navigates to the base repo root

## Verification

```bash
# From base repo
jig home        # prints /path/to/repo

# From a worktree
cd $(jig home)  # navigates to base repo root
pwd             # confirms base repo root

# Alias works
jig h

# Outside git repo
cd /tmp && jig home  # errors
```
