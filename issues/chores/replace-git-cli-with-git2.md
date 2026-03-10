# Replace git CLI shelling with git2 library

**Status:** Done
**Priority:** Urgent
**Auto:** true

## Objective

Replace `Command::new("git")` subprocess calls with the `git2` crate for worktree operations, branch checks, and other git interactions.

## Motivation

Shelling out to the git CLI is fragile — error messages vary across git versions, parsing stdout/stderr is brittle, and subprocess overhead adds up in the daemon's tight tick loop. The `git2` crate provides a proper Rust API backed by libgit2.

## Scope

Key callsites to migrate in `crates/jig-core/src/git.rs`:
- `create_worktree` — `git worktree add`, `git worktree prune`
- `branch_exists` — `git branch --list`
- `find_valid_start_point` — `git rev-parse`
- `current_branch` — `git rev-parse --abbrev-ref`
- `main_branch` — `git symbolic-ref`, `git branch -r`
- `remote_default_branch` — `git remote show`

Also in `crates/jig-core/src/daemon/prune_actor.rs`:
- `git worktree remove`
- `git worktree prune`

## Files

- `crates/jig-core/Cargo.toml` — Add `git2` dependency
- `crates/jig-core/src/git.rs` — Rewrite to use `git2::Repository`
- `crates/jig-core/src/daemon/prune_actor.rs` — Use git2 for worktree prune/remove

## Acceptance Criteria

- [x] No `Command::new("git")` calls remain for worktree/branch operations
- [x] All existing tests pass
- [ ] Daemon auto-spawn and prune work correctly with git2

## Verification

```bash
cargo test
cargo clippy
# Manual: jig spawn, close PR, verify re-spawn works
```
