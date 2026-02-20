# Auto-Cleanup Lifecycle

**Status:** Planned  
**Priority:** Medium  
**Category:** Features  
**Epic:** issues/epics/github-integration/index.md  
**Depends-On:** issues/epics/github-integration/0-octorust-client.md

## Objective

Auto-cleanup workers for merged/closed PRs.

## Implementation

On `jig health` run, check all workers against GitHub:
- If PR merged: kill worker, remove worktree, reset nudges
- If PR closed (not merged): alert human, optional auto-cleanup after N hours

Configuration:
```toml
[github]
autoCleanupMerged = true
closedPrCleanupAfter = 24  # hours
```

## Acceptance Criteria

- [ ] Detect merged PRs
- [ ] Kill worker and remove worktree for merged PRs
- [ ] Detect closed PRs (not merged)
- [ ] Alert on closed PRs
- [ ] Auto-cleanup closed PRs after age threshold
- [ ] Reset all nudge counts on cleanup

## Testing

Create PR, merge via GitHub, run `jig health`. Verify worker cleaned up.
