# Show nudge state in jig ps output

**Status:** Complete
**Priority:** Medium
**Labels:** auto

## Objective

Make nudge state visible in `jig ps` so operators can tell at a glance when a worker has been nudged, how many times, and whether it's hit the max (stalled out silently).

## Context

`WorkerDisplayInfo` already carries `nudge_count: u32` but `worker_row()` in `ui.rs` doesn't display it. When a worker stops responding to PR feedback or goes idle, the only way to know nudges were exhausted is to dig through event logs. This makes it hard to tell what's happening.

## Implementation

### 1. Add nudge column to ps table

**`crates/jig-cli/src/ui.rs`**:

- Add `"NUDGE"` to `table_header()`
- In `worker_row()`, format nudge display:
  - `0` nudges → `"-"` (grey)
  - `1-2` of 3 → `"2/3"` (yellow)
  - `3/3` (at max) → `"3/3"` (red) — immediately signals exhausted
- Place column between STATE and COMMITS

### 2. Surface max_nudges for display

**`crates/jig-core/src/daemon/mod.rs`** — Add `max_nudges: u32` to `WorkerDisplayInfo` so the UI can render `count/max`.

## Files

- `crates/jig-cli/src/ui.rs` — Add NUDGE column to `worker_row()` and `table_header()`
- `crates/jig-core/src/daemon/mod.rs` — Add `max_nudges` to `WorkerDisplayInfo`

## Acceptance Criteria

- [ ] `jig ps` shows a NUDGE column
- [ ] Nudge count displayed as `count/max` (e.g. `2/3`)
- [ ] Zero nudges shown as `-` in grey
- [ ] Max nudges shown in red to signal intervention needed
- [ ] `cargo build && cargo test && cargo clippy` passes

## Verification

```bash
cargo build && cargo test && cargo clippy

# Visual check:
# jig ps shows NUDGE column with count/max format
```
