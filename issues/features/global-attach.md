# Global attach for unambiguous branch names

**Status:** Planned
**Priority:** Urgent
**Auto:** true

## Objective

Allow `jig attach <name>` to work with `-g`/`--global` so users can attach to a worktree from anywhere, without being inside the owning repo.

## Context

Currently `jig attach` only implements `Op::run` (single-repo mode). The default `run_global` rejects with "this command does not support -g/--global". Users who run `jig ls -g` can see worktrees across all repos but cannot attach to them without first `cd`-ing into the correct repo.

`GlobalCtx::repo_for_worktree` already resolves a worktree name to its owning repo, so the plumbing is in place.

## Implementation

1. Add `run_global` to the `Op` impl for `Attach` in `crates/jig-cli/src/commands/attach.rs`
2. Require `self.name` in global mode (error if omitted — there's no "current repo" to default to)
3. Use `ctx.repo_for_worktree(name)` to find the owning repo
4. Call `spawn::attach(repo, Some(name))` as in local mode

```rust
fn run_global(&self, ctx: &GlobalCtx) -> Result<Self::Output, Self::Error> {
    let name = self.name.as_deref().ok_or_else(|| {
        AttachError::Core(jig_core::Error::Other(
            "worktree name required in global mode".into(),
        ))
    })?;
    let repo = ctx.repo_for_worktree(name)?;
    spawn::attach(repo, Some(name))?;
    Ok(NoOutput)
}
```

## Files

- `crates/jig-cli/src/commands/attach.rs` — Add `run_global` implementation

## Acceptance Criteria

- [ ] `jig attach <name> -g` attaches to the correct tmux window when the name is unambiguous across repos
- [ ] `jig attach -g` (no name) prints a clear error message
- [ ] `jig attach <unknown> -g` prints "worktree not found" error

## Verification

```bash
# From outside any repo:
jig ls -g                          # see worktree names
jig attach <worktree-name> -g     # should open tmux session
jig attach -g                      # should error: name required
jig attach nonexistent -g         # should error: not found
```
