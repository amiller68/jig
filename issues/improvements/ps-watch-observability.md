# Improve ps -w observability: timers and nudge messages

**Status:** Done
**Priority:** Medium
**Labels:** auto

## Objective

Make `jig ps -w` a proper dashboard by surfacing daemon timing state and nudge activity, so operators don't have to guess whether the daemon is waiting on a cooldown or stuck.

## Context

Currently `jig ps -w` shows static state — worker status, nudge count, PR health. But there's no visibility into *when* things will happen next. The daemon has 4 distinct timers:

| Timer | Default | Scope |
|---|---|---|
| Tick interval | 30s | Global |
| Nudge cooldown | 300s (5min) | Per worker, per nudge type |
| Git sync interval | 60s | Global |
| Auto-spawn poll interval | 120s | Global |

When staring at `ps -w`, you can't tell if a nudge is about to fire or if you're 4 minutes away. You also can't see *what* the daemon nudged the worker with.

## Implementation

### 1. Per-worker nudge cooldown countdown

In the NUDGE column (or a new NEXT column), show time until next eligible nudge when cooldown is active:

- `2/3` — no cooldown active, next nudge fires on next tick if conditions met
- `2/3 (3m12s)` — cooldown active, next nudge eligible in 3m12s
- `3/3` — exhausted, no more nudges

This requires `last_nudge_at` timestamps and `cooldown_seconds` to be available in `WorkerDisplayInfo`.

### 2. Surface nudge messages in ps -w

When the daemon sends a nudge, show a transient message line below the worker row in `ps -w`:

```
 ● feature/foo                running  1/3    2  #42  ci     AUT-123
   ↳ nudge (review): You have 3 unresolved review comments. Address the feedback and push.
```

This requires the watch loop to track recent nudge events (from the notification queue or event log) and display them for a short window (e.g. until the next tick clears them).

### 3. Global timer footer

Add a footer line to `ps -w` showing global daemon timers:

```
tick: 12s  sync: 45s  poll: 1m38s
```

This requires the daemon to expose `last_tick`, `last_sync`, `last_issue_poll` timestamps in its state, and the CLI to compute countdowns.

## Files

- `crates/jig-core/src/daemon/mod.rs` — Expose `last_nudge_at` and cooldown info in `WorkerDisplayInfo`
- `crates/jig-core/src/daemon/runtime.rs` — Expose global timer state for CLI consumption
- `crates/jig-cli/src/ui.rs` — Render cooldown countdown, nudge messages, footer timers
- `crates/jig-cli/src/commands/ps.rs` — Pass timer state through to UI in watch mode

## Acceptance Criteria

- [ ] NUDGE column shows cooldown countdown when active (e.g. `2/3 (3m12s)`)
- [ ] Recent nudge messages appear below worker rows in `ps -w`
- [ ] Global timer footer shows time until next tick/sync/poll
- [ ] `cargo build && cargo test && cargo clippy` passes

## Verification

```bash
cargo build && cargo test && cargo clippy

# Visual check with ps -w:
# - See cooldown countdown ticking down
# - See nudge message appear when daemon nudges a worker
# - See global timer footer updating
```
