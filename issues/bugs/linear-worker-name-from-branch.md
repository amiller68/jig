# Linear auto-spawn uses short issue ID instead of branch name for worker

**Status:** Complete
**Priority:** High
**Labels:** auto

## Objective

When auto-spawning from Linear issues, use Linear's suggested branch name (e.g. `feature/aut-4969-spawn-agent-thread-is-broken`) instead of the lowercased issue ID (e.g. `aut-4969`) for the worker/worktree name.

## Context

`derive_worker_name()` in `crates/jig-core/src/daemon/issue_actor.rs:133` simply lowercases the issue ID:

```rust
fn derive_worker_name(issue_id: &str) -> String {
    issue_id.to_lowercase()
}
```

For file-based issues this works fine — the ID is already descriptive (e.g. `features/smart-context-injection`). But for Linear issues the ID is just `AUT-4969`, producing a worker named `aut-4969`. This makes it hard to:

- **Discover** what a worker is doing from `jig ls` or `jig ps`
- **Search** for workers by topic (e.g. "which worker is fixing the spawn thread?")
- **Match** workers to their Linear issues at a glance

Linear provides a `branchName` field on every issue (e.g. `feature/aut-4969-spawn-agent-thread-is-broken`) which is human-readable and includes both the ID and a slug of the title.

## Implementation

1. Add a `branch_name` field (optional) to the `Issue` struct in `crates/jig-core/src/issues/types.rs`

2. In the Linear provider (`crates/jig-core/src/issues/linear_provider.rs`), fetch `branchName` from the Linear API and populate `Issue.branch_name`

3. Add a `branch_name` field to `SpawnableIssue` in `crates/jig-core/src/daemon/messages.rs`

4. Update `derive_worker_name()` in `crates/jig-core/src/daemon/issue_actor.rs` to prefer `branch_name` when available, falling back to the current lowercased ID

5. Ensure the branch name is valid for git worktree names (no leading dots, no `.lock`, etc.)

## Files

- `crates/jig-core/src/issues/types.rs` — Add `branch_name: Option<String>` to `Issue`
- `crates/jig-core/src/issues/linear_provider.rs` — Fetch and populate `branchName`
- `crates/jig-core/src/daemon/messages.rs` — Add `branch_name` to `SpawnableIssue`
- `crates/jig-core/src/daemon/issue_actor.rs` — Update `derive_worker_name()` to use branch name

## Acceptance Criteria

- [ ] Linear auto-spawned workers use the Linear branch name (e.g. `feature/aut-4969-spawn-agent-thread-is-broken`)
- [ ] File-based issue workers are unaffected
- [ ] Worker name is valid for git worktree/branch usage
- [ ] `jig ls` and `jig ps` show the descriptive name

## Verification

```bash
# Auto-spawn a Linear issue and check the worker name
jig ps -w
jig ls
# Worker should be named like "feature/aut-XXXX-descriptive-slug"
# not just "aut-xxxx"
```
