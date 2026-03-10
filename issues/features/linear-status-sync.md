# Sync Linear issue status from worker lifecycle

**Status:** Planned

## Objective

When the daemon auto-spawns a worker for a Linear issue, automatically update the Linear issue status to reflect the worker's lifecycle — move to "In Progress" on spawn, and to "Done" when the worker's PR merges.

## Context

Currently jig reads from Linear but never writes back. A user labels an issue `jig-auto`, the daemon spawns a worker, but the Linear board still shows the issue as Backlog/Unstarted. The user has to manually drag it to In Progress and later to Done. This defeats the purpose of automation.

## Design

### Status transitions

| Worker event | Linear status update |
|---|---|
| Worker spawned for issue | Move to **Started** (In Progress) |
| Worker's PR merges | Move to **Done** (Completed) |
| Worker pruned without PR | No change (leave as Started for triage) |

### Implementation approach

The daemon already tracks worker-to-issue mappings via `spawn::register` which stores the `issue_id`. The status updates should happen:

1. **On spawn** — In `Daemon::auto_spawn_worker()`, after successful spawn, call the Linear API to transition the issue to Started.
2. **On PR merge** — The daemon already monitors PRs via `github_actor`. When a PR merges and the worker has an associated issue, transition to Done.

### API

Add a `update_issue_status` method to `LinearClient`:

```graphql
mutation UpdateIssue($id: String!, $stateId: String!) {
  issueUpdate(id: $id, input: { stateId: $stateId }) {
    issue { identifier state { name } }
  }
}
```

This requires resolving the state ID from the team's workflow states. Add a `list_workflow_states` query to look up state IDs by team, cache per session.

### Provider interface

Add an optional `update_status` method to `IssueProvider`:

```rust
fn update_status(&self, id: &str, status: IssueStatus) -> Result<()> {
    // Default: no-op (file provider doesn't need this)
    Ok(())
}
```

The Linear provider implements it by mapping `IssueStatus` to the appropriate workflow state and calling the mutation.

## Files

- `crates/jig-core/src/issues/linear_client.rs` — Add `update_issue_status`, `list_workflow_states` methods
- `crates/jig-core/src/issues/linear_provider.rs` — Implement `update_status`
- `crates/jig-core/src/issues/provider.rs` — Add `update_status` to trait with default no-op
- `crates/jig-core/src/daemon/mod.rs` — Call `update_status` after auto-spawn and on PR merge

## Acceptance Criteria

- [ ] Auto-spawned worker moves its Linear issue to In Progress
- [ ] Merged PR moves the associated Linear issue to Done
- [ ] File provider is unaffected (no-op)
- [ ] Errors updating Linear status are logged but don't block the daemon
- [ ] Works with any team's workflow states (not hardcoded state IDs)

## Verification

```bash
# Label an issue jig-auto in Linear, leave as Backlog
jig ps -w --auto-spawn

# Watch Linear — issue should move to In Progress when worker spawns
# After worker creates and merges PR, issue should move to Done
```
