# Communicate worker initialization state and on-create failures

**Status:** Planned
**Priority:** High
**Labels:** auto

## Objective

Make the worker lifecycle visible during initialization so users can tell when a worker is setting up, and diagnose when setup fails.

## Context

When the daemon auto-spawns a worker, the sequence is:
1. `git worktree add` — worktree appears on disk (visible in `jig ls`)
2. On-create hook runs synchronously (e.g. `pnpm i && pnpm db setup && pnpm build`)
3. `wt.register()` — Spawn event emitted, worker appears in event log
4. `wt.launch()` — tmux window created, agent starts

If step 2 fails, the worker gets stuck: worktree exists on disk but there's no event log, no tmux window, and no error surfaced. `jig ls` shows the worker with `-` for branch and commits. `jig attach` says "Worker not found". The user has no idea what happened.

## Implementation

### 1. Emit an `Initializing` event before the on-create hook

In `auto_spawn_worker()` (`crates/jig-core/src/daemon/mod.rs:875`), emit a lightweight event right after `Worktree::create()` succeeds but before `wt.register()` / `wt.launch()`. Alternatively, split `Worktree::create()` so the event is emitted after `git worktree add` but before the on-create hook runs.

Add an `Initializing` variant to `EventType` in `crates/jig-core/src/events/schema.rs`, or reuse `Spawn` with a `phase: "initializing"` field.

### 2. Add `Initializing` worker status

Add `Initializing` to `WorkerStatus` in `crates/jig-core/src/worker.rs`. The reducer should set this status when an `Initializing` event is seen, and transition to `Spawned` on the normal `Spawn` event.

### 3. Show initializing state in `jig ls` and `jig ps`

- `jig ls`: when a worktree exists on disk but has no tmux window and either no event log or status is `Initializing`, show a status hint (e.g. `[init]` or `setting up...` in the BRANCH column)
- `jig ps`: show `Initializing` in the STATE column

### 4. Record on-create hook failures

In `Worktree::create()` (`crates/jig-core/src/worktree.rs:79-81`), when `run_on_create_hook` fails, emit a `Terminal` event with reason `"on-create hook failed"` (or a new `SetupFailed` event type) before propagating the error. This way the worker appears in `jig ps` with a `Failed` status and a reason.

### 5. Improve `jig attach` error message

In `crates/jig-core/src/spawn.rs` (or equivalent attach path), when a worktree exists but has no tmux window:
- Check if the worker has an `Initializing` status → "Worker 'X' is still initializing (running on-create hook)"
- Check if the worker has a `Failed` status → "Worker 'X' failed during setup: on-create hook failed"
- Otherwise → current "Worker not found" message

## Files

- `crates/jig-core/src/events/schema.rs` — Add `Initializing` event type (or `SetupFailed`)
- `crates/jig-core/src/events/reducer.rs` — Handle new event in state derivation
- `crates/jig-core/src/worker.rs` — Add `Initializing` status variant
- `crates/jig-core/src/worktree.rs` — Emit event before on-create hook, emit failure on hook error
- `crates/jig-core/src/daemon/mod.rs` — Emit initializing event in `auto_spawn_worker()`
- `crates/jig-cli/src/commands/list.rs` — Show init state hint
- `crates/jig-cli/src/ui.rs` — Render `Initializing` and `Failed` states in ps table
- `crates/jig-core/src/spawn.rs` — Better attach error messages

## Acceptance Criteria

- [ ] Worker shows as "Initializing" in `jig ps` while on-create hook is running
- [ ] On-create hook failure records a failed state with reason in the event log
- [ ] `jig ls` indicates when a worktree is still setting up
- [ ] `jig attach` gives a helpful message instead of "Worker not found" for initializing/failed workers
- [ ] `jig ps` shows failed setup workers so they don't silently disappear

## Verification

```bash
# Configure a slow on-create hook to observe initializing state
# jig.toml: on_create = "sleep 30"
# Trigger auto-spawn, then immediately:
jig ps        # should show Initializing
jig attach X  # should say "still initializing"

# Configure a failing on-create hook
# jig.toml: on_create = "exit 1"
# Trigger auto-spawn, then:
jig ps        # should show Failed with reason
jig attach X  # should say "failed during setup"
```
