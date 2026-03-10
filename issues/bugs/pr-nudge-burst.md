# PR nudges fire in rapid burst instead of spacing out

**Status:** Planned
**Priority:** Urgent
**Labels:** auto

## Objective

When a PR has unresolved review comments, all 3 nudges fire in quick succession (~2s apart) instead of being spaced out. The agent barely has time to read the first nudge before the second and third arrive.

## Observed Behavior

```
❯ Your PR has unresolved review comments (nudge 1/3). Address all feedback...
⏺ Let me fix the two issues from the review.
⏺ Reading 1 file…
❯ Your PR has unresolved review comments (nudge 2/3). Address all feedback...
❯ Your PR has unresolved review comments (nudge 3/3). Address all feedback...
```

All 3 nudges arrive within seconds. The agent starts working on the feedback after nudge 1, but nudges 2 and 3 still fire because the tick loop doesn't wait for the agent to respond.

## Root Cause

The PR nudge path in the tick (`crates/jig-core/src/daemon/mod.rs:536-565`) checks cached PR results every tick. If `reviews` has a problem:

1. **Tick N**: count=0 < max_nudges(3) → dispatch nudge → Nudge event appended → count becomes 1
2. **Tick N+1** (2s later): count=1 < 3 → dispatch nudge → count becomes 2
3. **Tick N+2** (2s later): count=2 < 3 → dispatch nudge → count becomes 3

The nudge count correctly increments, but there's no cooldown between PR nudges. The idle/stalled nudge path has a natural gate (the worker status changes from Stalled → Running once the agent responds), but PR nudges keep firing as long as the cached check result says `has_problem: true` — which doesn't update until the next GitHub actor poll.

## Expected Behavior

After sending a PR nudge, wait for a reasonable interval before sending the next one. The agent needs time to read the feedback, make changes, commit, push, and wait for CI. Firing all nudges in 6 seconds defeats the purpose.

## Proposed Fix

Add a per-nudge-type cooldown that prevents re-nudging the same type within a configurable interval. Options:

### Option A: Cooldown based on last nudge timestamp

Track `last_nudge_at` per nudge type (alongside `nudge_counts` in `WorkerState`). Skip the nudge if less than `nudge_cooldown_seconds` has elapsed since the last nudge of that type.

The Nudge event already has a timestamp, so this can be derived from the event log without new state:

```rust
// In the PR nudge dispatch path:
if let Some(last_nudge_ts) = last_nudge_timestamp_for_type(&events, &nudge_type) {
    let elapsed = now - last_nudge_ts;
    if elapsed < nudge_cooldown_seconds {
        continue; // too soon, skip
    }
}
```

A reasonable default cooldown could be `silence_threshold_seconds` (5 minutes), matching the stall detection interval — the idea being "give the agent as much time to respond as we'd give it before calling it stalled."

### Option B: Gate on worker activity since last nudge

Only send the next PR nudge if the worker has shown activity (ToolUseStart, Commit, Push) since the last nudge of that type. This ensures the agent actually processed the previous nudge before getting another one.

Option A is simpler and more predictable. Option B is more precise but adds complexity.

## Files

- `crates/jig-core/src/daemon/mod.rs` — Add cooldown check before dispatching PR nudges
- `crates/jig-core/src/daemon/pr.rs` — Same cooldown check in the blocking path
- `crates/jig-core/src/events/reducer.rs` — Optionally track `last_nudge_at` per type in `WorkerState`

## Acceptance Criteria

- [ ] PR nudges are spaced out by at least `silence_threshold_seconds` (or a dedicated cooldown config)
- [ ] First nudge still fires immediately when a problem is detected
- [ ] Nudge count still increments correctly toward `max_nudges`
- [ ] Idle/stalled nudges unaffected (they already have natural gating)

## Verification

```bash
# Open a draft PR with review comments
# Watch jig ps -w and verify:
# - Nudge 1/3 fires immediately
# - Nudge 2/3 fires after ~5 minutes (not 2 seconds)
# - If the agent pushes a fix, nudges pause until the next GitHub poll detects the problem persists
```
