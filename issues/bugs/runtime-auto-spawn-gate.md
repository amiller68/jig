# Runtime global gate blocks per-repo auto-spawn

**Status:** Planned
**Labels:** daemon, auto

## Objective

Fix the daemon runtime so that per-repo `auto_spawn = true` in `jig.toml` is respected without requiring a global config override.

## Problem

There are two gates for auto-spawning in the watch-mode daemon path:

1. **`DaemonRuntime::maybe_trigger_issue_poll()`** (`runtime.rs:269`) — checks `self.config.auto_spawn` from `RuntimeConfig`, which is resolved from the **global** config default (`false`). If this is false, the method returns early and never sends a request to the issue actor.

2. **`issue_actor::process_request()`** (`issue_actor.rs:51`) — checks each repo's `jig.toml` via `resolve_auto_spawn()`, correctly respecting per-repo overrides.

Gate 1 prevents gate 2 from ever running. A repo with `auto_spawn = true` in its `jig.toml` will show issues as auto-eligible (`jig issues --auto` shows ✓), but the daemon will never spawn them unless the global config also has `auto_spawn = true`.

The `tick_once()` path (non-watch mode) does NOT have this problem — it calls `issue_actor::process_request()` directly.

Additionally, `jig ps --watch` in global mode (`run_global`) uses `RuntimeConfig::default()` which hardcodes `auto_spawn: false`, so even `--auto-spawn` flag only works in per-repo mode.

## Implementation

1. Remove the `auto_spawn` field from `RuntimeConfig` — per-repo filtering already happens in the issue actor
2. Remove the `if !self.config.auto_spawn` early return from `maybe_trigger_issue_poll()`
3. Always show the poll timer in `timer_info()` since any registered repo might have auto-spawn enabled
4. Update `ps.rs` — remove `auto_spawn` from `RuntimeConfig` construction, derive the "auto" UI label from whether any registered repo has auto-spawn enabled (or just always show it)

## Files

- `crates/jig-core/src/daemon/runtime.rs` — Remove `auto_spawn` from `RuntimeConfig`, remove global gate
- `crates/jig-cli/src/commands/ps.rs` — Update `RuntimeConfig` construction and auto label logic

## Verification

1. Configure a repo with `auto_spawn = true` in `jig.toml` but no global config override
2. Run `jig ps --watch` in global mode
3. Confirm issues are polled and workers are auto-spawned
