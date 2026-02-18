# Restructure jig state directory layout and clean up stale workers

**Status:** Planned
**Priority:** Urgent

## Objective

Move jig's repo-level state from `<repo>/.worktrees/` to `<repo>/.jig/`, with worktrees as subdirectories and state files under `<repo>/.jig/.state/`. Also fix `jig kill` to clean up workers whose tmux windows no longer exist.

## Background

Currently:
- Worktrees live in `<repo>/.worktrees/<name>/`
- State file lives at `<repo>/.worktrees/.jig-state.json`
- `jig ps` shows tons of stale `no-window` entries because `jig kill` only archives workers in state — it never removes them, and externally killed windows (e.g. `tmux kill-window`) leave orphan entries forever

## Design

New directory layout:

```
<repo>/.jig/
├── .state/              # State files (gitignored internals)
│   └── state.json       # Orchestrator state (was .jig-state.json)
├── <worker-name>/       # Git worktree directories
├── <worker-name>/
└── ...
```

## Implementation

### 1. Update directory constants

- `crates/jig-core/src/git.rs` — Change `get_worktrees_dir()` to return `<repo>/.jig/` instead of `<repo>/.worktrees/`
- `crates/jig-core/src/state.rs` — Change `state_file_path()` to return `<repo>/.jig/.state/state.json` instead of `<repo>/.worktrees/.jig-state.json`

### 2. Update `jig kill` to clean up stale workers

- `crates/jig-core/src/spawn.rs` — In `kill()`:
  - After killing the tmux window, **remove** the worker from state (not just archive it)
- Add a new `cleanup_stale()` function in `spawn.rs`:
  - Iterate all workers in state
  - For any worker with `no-window` status (tmux window gone), remove it from state
  - Call this from `list_tasks()` before returning, or expose as a separate command

### 3. Update `jig ps` to auto-clean

- `crates/jig-core/src/spawn.rs` — In `list_tasks()`:
  - Before building the task list, prune workers whose tmux window no longer exists
  - Save state after pruning
  - Only return workers that are actually live

### 4. Update `.gitignore`

- Ensure `<repo>/.jig/` is handled correctly — worktrees themselves should already be gitignored, add `.jig/.state/` explicitly

### 5. Migration

- On first load, if `<repo>/.worktrees/.jig-state.json` exists but `<repo>/.jig/.state/state.json` does not, migrate automatically
- Move worktree directories from `.worktrees/` to `.jig/`
- Move state file to new location
- Remove old `.worktrees/` directory if empty

## Files

- `crates/jig-core/src/state.rs` — New state file path (`<repo>/.jig/.state/state.json`)
- `crates/jig-core/src/git.rs` — New worktrees dir (`<repo>/.jig/`)
- `crates/jig-core/src/spawn.rs` — Stale worker cleanup in `kill()`, `list_tasks()`, and new `cleanup_stale()`
- `crates/jig-core/src/config.rs` — Update any path references if needed
- `.gitignore` / template `.gitignore` — Add `.jig/.state/`

## Acceptance Criteria

- [ ] `<repo>/.jig/` is the root for all jig-managed worktrees
- [ ] State files live under `<repo>/.jig/.state/`
- [ ] `jig kill <name>` removes the worker from state entirely (not just archives)
- [ ] `jig ps` does not show workers whose tmux windows are gone
- [ ] Workers killed externally (outside `jig kill`) are cleaned up on next `jig ps`
- [ ] Existing repos with `.worktrees/` are migrated automatically on first use
- [ ] `jig spawn`, `jig ps`, `jig kill`, `jig remove` all work with new paths

## Verification

```bash
# Spawn a worker, verify it lands in .jig/
jig spawn test-worker
ls .jig/test-worker/

# State file is in new location
cat .jig/.state/state.json

# Kill externally, verify ps cleans up
tmux kill-window -t jig-<repo>:test-worker
jig ps  # should NOT show test-worker

# Kill via jig, verify removal from state
jig spawn test-worker-2
jig kill test-worker-2
jig ps  # should NOT show test-worker-2
cat .jig/.state/state.json  # should not contain test-worker-2
```
