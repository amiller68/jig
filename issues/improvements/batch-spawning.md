# Batch Spawning

**Status:** Planned
**Priority:** Low
**Category:** Improvements

## Objective

Add `jig issues spawn-next` to spawn the highest-priority unblocked issue, and `jig issues spawn --batch` for interactive multi-select.

## Current State

- `jig spawn` creates a worktree + launches Claude for a named worker
- `jig spawn -I <issue>` spawns a worker tied to an issue
- No way to spawn "the next most important issue" or batch-select from planned issues

## Design

### `jig issues spawn-next`

```bash
# Spawn the highest-priority unblocked auto issue
jig issues spawn-next

# Spawn next N
jig issues spawn-next --count 3
```

Uses `provider.list_spawnable()`, applies priority sort, picks top N, calls spawn for each.

### `jig issues spawn --batch`

Interactive multi-select using `inquire` crate:

```
$ jig issues spawn --batch --priority High

Select issues to spawn:
  [x] ENG-123  High    Fix auth token refresh
  [ ] ENG-456  High    Add rate limiting
  [x] ENG-789  Medium  Update error messages

2 selected. Spawn? [y/N]
```

## Acceptance Criteria

- [ ] `jig issues spawn-next` spawns highest-priority planned issue
- [ ] `--count N` spawns up to N issues
- [ ] `jig issues spawn --batch` interactive multi-select
- [ ] Pre-filter with `--priority`, `--label`, `--category`
- [ ] Works with both file and Linear providers
