# Daemon crash recovery and worker resume

**Status:** Planned
**Priority:** Urgent
**Auto:** true

## Objective

Make jig resilient to daemon crashes, Ctrl+C shutdowns, and computer reboots. Workers whose git worktrees and event logs survive should be automatically re-launched, not left as orphans.

## Context

Currently when the daemon stops, tmux sessions may die (reboot) or persist (Ctrl+C), but the Claude Code agent process is gone either way. The worktree, branch, and full event history survive on disk, yet there is no mechanism to re-launch the agent. Workers appear as `Stalled` with no distinction from genuinely stuck agents. There is also no signal handling — Ctrl+C kills the daemon immediately with no cleanup.

## Implementation

### 1. Record spawn context in Spawn events

Thread `auto: bool` and `context: Option<&str>` into `register()` in `spawn.rs` and persist them as fields on the Spawn event. Without these, recovery cannot reconstruct the original Claude session.

### 2. Add `Resume` event type

New `EventType::Resume` variant in `events/schema.rs`. The reducer treats it like `Spawn` (transitions to `Spawned`) but does NOT reset issue_ref, commit_count, or the event log.

### 3. `resume_worker()` function

New function in `spawn.rs` that:
- Verifies worktree exists on disk
- Appends a `Resume` event (preserving history)
- Calls `launch_tmux_window()` to create a new tmux window and start Claude

### 4. Graceful daemon shutdown

Add `ctrlc` crate. Install SIGTERM/SIGINT handler that sets the existing `quit` flag in `run_with()`. The flag is already checked between workers in `tick()`. Add the same pattern to `run()`.

### 5. Daemon lifecycle log

New `daemon/lifecycle.rs` with a separate JSONL log at `~/.config/jig/state/daemon.jsonl`. Events: `Started { ts, pid }`, `Stopped { ts, pid, reason }`. On startup, check if last event was `Stopped` — if not, previous run crashed.

### 6. Startup recovery

New `daemon/recovery.rs`:
- `find_orphaned_workers()` — discover worktrees, check tmux status, identify workers with dead tmux but non-terminal state
- `recover_orphaned_workers()` — auto re-spawn active workers (Spawned/Running/Stalled), skip Idle/WaitingInput/WaitingReview/Terminal
- All workers are auto-recovered regardless of original spawn method
- Gated by `auto_recover: bool` config (default `true`)

### 7. `jig resume` CLI command

New `commands/resume.rs`:
- Takes worker name, optional `--context` override, `--auto` flag
- Errors if tmux window already exists (use `jig attach` instead)
- Reads original context from Spawn event in log
- Calls `spawn::resume_worker()`

### 8. Steady-state dead detection

In `process_worker()`, after deriving state but before dispatch: if worker is active (Running/Spawned/Stalled) AND tmux window is dead → trigger recovery instead of nudging. Wire the existing `Action::Restart` (currently unimplemented) to call `resume_worker()`.

## Files

- `crates/jig-core/src/events/schema.rs` — Add `Resume` variant
- `crates/jig-core/src/events/reducer.rs` — Handle `Resume`
- `crates/jig-core/src/events/derive.rs` — Handle `Resume`
- `crates/jig-core/src/spawn.rs` — Context/auto in Spawn event, `resume_worker()`
- `crates/jig-core/src/daemon/mod.rs` — Signal handler, lifecycle, startup recovery, dead detection
- `crates/jig-core/src/daemon/lifecycle.rs` (new) — Daemon lifecycle log
- `crates/jig-core/src/daemon/recovery.rs` (new) — Orphan detection + recovery
- `crates/jig-core/src/global/paths.rs` — `daemon_log_path()`
- `crates/jig-core/src/config.rs` — `auto_recover` config
- `crates/jig-core/src/dispatch/actions.rs` — Wire `Action::Restart`
- `crates/jig-cli/src/commands/resume.rs` (new) — CLI command
- `crates/jig-cli/src/commands/mod.rs` — Register module
- `crates/jig-cli/src/cli.rs` — Register in `command_enum!`
- `crates/jig-core/Cargo.toml` — Add `ctrlc`

## Acceptance Criteria

- [ ] Spawn events record `auto` and `context` fields
- [ ] `Resume` event type exists and reducer handles it correctly
- [ ] `resume_worker()` re-launches Claude in existing worktree without resetting event log
- [ ] Daemon installs SIGTERM/SIGINT handler and exits gracefully
- [ ] `daemon.jsonl` records Started/Stopped lifecycle events
- [ ] Unclean shutdown detected on next startup (missing Stopped event)
- [ ] Orphaned active workers auto-recovered on daemon startup
- [ ] `jig resume <name>` works for manual recovery
- [ ] `jig resume` errors cleanly when tmux window already exists
- [ ] Dead tmux detection during steady-state ticks triggers recovery instead of nudging
- [ ] `auto_recover` config option allows opt-out
- [ ] Integration test for `jig resume`

## Verification

```bash
# Build and test
cargo build && cargo test && cargo clippy

# Graceful shutdown
# 1. Start daemon, Ctrl+C, check daemon.jsonl has Stopped event

# Startup recovery
# 2. Spawn a worker, kill its tmux window, restart daemon → worker auto-resumes

# Manual resume
# 3. jig resume <name> on dead worker → new tmux window, Claude starts
# 4. jig resume <name> on running worker → error message

# Steady-state
# 5. Kill tmux window while daemon running → daemon recovers on next tick
```
