# Daemon crash recovery and worker resume

**Status:** Blocked
**Priority:** Urgent
**Depends-On:** improvements/worktree-consolidation
**Auto:** true

## Objective

Make jig resilient to daemon crashes, Ctrl+C shutdowns, and computer reboots. Workers whose git worktrees and event logs survive should be automatically re-launched, not left as orphans.

## Context

Currently when the daemon stops, tmux sessions may die (reboot) or persist (Ctrl+C), but the Claude Code agent process is gone either way. The worktree, branch, and full event history survive on disk, yet there is no mechanism to re-launch the agent. Workers appear as `Stalled` with no distinction from genuinely stuck agents. There is also no signal handling — Ctrl+C kills the daemon immediately with no cleanup.

This ticket assumes `improvements/worktree-consolidation` is done first. That ticket moves the `Resume` event, spawn context recording, orphan detection, and worker resume logic into `Worktree` methods. This ticket builds on those primitives to add graceful shutdown, lifecycle logging, startup recovery, steady-state dead detection, and the `jig resume` CLI.

## Implementation

### 1. Graceful daemon shutdown

Add `ctrlc` crate. Install SIGTERM/SIGINT handler that sets the existing `quit` flag in `run_with()`. The flag is already checked between workers in `tick()`. Add the same pattern to `run()`.

### 2. Daemon lifecycle log

New `daemon/lifecycle.rs` with a separate JSONL log at `~/.config/jig/state/daemon.jsonl`. Events: `Started { ts, pid }`, `Stopped { ts, pid, reason }`. On startup, check if last event was `Stopped` — if not, previous run crashed.

### 3. Startup recovery

New `daemon/recovery.rs`:
- `find_orphaned_workers()` — use `Worktree::list()` + `wt.is_orphaned()` to identify workers with dead tmux but non-terminal state
- `recover_orphaned_workers()` — call `wt.resume()` on active workers (Spawned/Running/Stalled), skip Idle/WaitingInput/WaitingReview/Terminal
- All workers are auto-recovered regardless of original spawn method
- Gated by `auto_recover: bool` config (default `true`)

### 4. `jig resume` CLI command

New `commands/resume.rs`:
- Takes worker name, optional `--context` override, `--auto` flag
- Errors if tmux window already exists (use `jig attach` instead)
- Opens worktree via `Worktree::open()`, calls `wt.resume(context)`
- Reads original context from Spawn event in log

### 5. Steady-state dead detection

In daemon `process_worker()`: if worker is active (Running/Spawned/Stalled) AND `wt.is_orphaned()` → call `wt.resume()` instead of nudging. Wire the existing `Action::Restart` (currently unimplemented) to use `Worktree::resume()`.

## Prerequisites from worktree-consolidation

The following are handled by `improvements/worktree-consolidation` and assumed complete:
- `Resume` event type in `events/schema.rs` + reducer handling
- `auto` and `context` fields recorded in Spawn events
- `Worktree::resume()` — appends Resume event, launches tmux window
- `Worktree::is_orphaned()` — auto_spawned && !has_tmux_window()
- `Worktree::list()` — scans worktrees directory

## Files

- `crates/jig-core/src/daemon/mod.rs` — Signal handler, dead detection via `wt.is_orphaned()` + `wt.resume()`
- `crates/jig-core/src/daemon/lifecycle.rs` (new) — Daemon lifecycle log
- `crates/jig-core/src/daemon/recovery.rs` (new) — Startup orphan detection + recovery using `Worktree::list()` + `wt.resume()`
- `crates/jig-core/src/global/paths.rs` — `daemon_log_path()`
- `crates/jig-core/src/config.rs` — `auto_recover` config
- `crates/jig-core/src/dispatch/actions.rs` — Wire `Action::Restart` to `Worktree::resume()`
- `crates/jig-cli/src/commands/resume.rs` (new) — CLI command using `Worktree::open()` + `wt.resume()`
- `crates/jig-cli/src/commands/mod.rs` — Register module
- `crates/jig-cli/src/cli.rs` — Register in `command_enum!`
- `crates/jig-core/Cargo.toml` — Add `ctrlc`

## Acceptance Criteria

- [ ] Daemon installs SIGTERM/SIGINT handler and exits gracefully
- [ ] `daemon.jsonl` records Started/Stopped lifecycle events
- [ ] Unclean shutdown detected on next startup (missing Stopped event)
- [ ] Orphaned active workers auto-recovered on daemon startup via `Worktree` methods
- [ ] `jig resume <name>` works for manual recovery using `Worktree::open()` + `wt.resume()`
- [ ] `jig resume` errors cleanly when tmux window already exists
- [ ] Dead tmux detection during steady-state ticks triggers `wt.resume()` instead of nudging
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
