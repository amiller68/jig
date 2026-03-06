# `jig ps -g` should show which repo each worker belongs to

**Status:** Planned
**Priority:** Urgent
**Category:** Chores
**Auto:** true

## Objective

When running `jig ps -g`, workers from multiple repos are shown in a single flat table with no indication of which repo they belong to. Add repo context to the global ps view.

## Current State

- `WorkerDisplayInfo` has no `repo_name` field
- The daemon tick loop has `repo_name` available (from `discover_workers`) but doesn't pass it through to `WorkerDisplayInfo`
- `render_worker_table()` in `ui.rs` renders a single flat table with columns: WORKER, STATE, COMMITS, PR, HEALTH, ISSUE
- Local `jig ps` (no `-g`) filters to a single repo, so repo name isn't needed there

## Design

Two options — pick whichever feels cleaner:

### Option A: Add REPO column (simplest)

Add a `repo` field to `WorkerDisplayInfo` and a REPO column to the table. Only show the column when rendering global view (pass a flag or check if workers span multiple repos).

```
REPO          WORKER        STATE     COMMITS  PR   HEALTH  ISSUE
jig           test-spawn    working   3*       -    -       chores/test
jax-fs        fuse-fix      review    7        #42  ok      bugs/fuse
```

### Option B: Group tables by repo (prettier)

Render separate tables per repo with a bold repo header, similar to how `jig ls -g` groups worktrees.

```
jig
  WORKER        STATE     COMMITS  PR   HEALTH  ISSUE
  test-spawn    working   3*       -    -       chores/test

jax-fs
  WORKER        STATE     COMMITS  PR   HEALTH  ISSUE
  fuse-fix      review    7        #42  ok      bugs/fuse
```

Either approach works. Option A is simpler. Option B matches `jig ls -g` style.

## Implementation

### 1. Add `repo` to `WorkerDisplayInfo`

In `crates/jig-core/src/daemon/mod.rs`:

```rust
pub struct WorkerDisplayInfo {
    pub repo: String,  // NEW
    pub name: String,
    // ... rest unchanged
}
```

Populate it at ~line 500 where `WorkerDisplayInfo` is constructed — `repo_name` is already in scope from the `process_worker` call.

### 2. Update `render_worker_table` in `crates/jig-cli/src/ui.rs`

Add a `global: bool` parameter. When true, either:
- Add a REPO column (Option A), or
- Group workers by `repo` and render sub-tables with headers (Option B)

### 3. Update call sites in `crates/jig-cli/src/commands/ps.rs`

Pass `global` context to `render_worker_table`. The `run_global` path sets it true, `run` sets it false.

## Acceptance Criteria

- [ ] `jig ps -g` shows which repo each worker belongs to
- [ ] `jig ps` (local, no `-g`) is unchanged — no repo column/header
- [ ] Watch mode (`jig ps -gw`) also shows repo context
- [ ] Works with single-repo and multi-repo scenarios
