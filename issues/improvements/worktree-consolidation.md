# Worktree consolidation and worker naming fix

**Status:** Planned
**Priority:** High
**Auto:** true

## Objective

Make `Worktree` the single abstraction for a worker's physical state — its repo, branch, path, tmux session, spawn context, and lifecycle — eliminating scattered logic across `spawn.rs`, `daemon/mod.rs`, and `git.rs`. Also fix `derive_worker_name()` stripping category prefixes from issue IDs.

## Context

Worktree management is scattered across four files:
- `worktree.rs` — `Worktree` struct exists but is underused
- `spawn.rs` — owns register/launch/resume separately
- `daemon/mod.rs` — duplicates worktree creation logic
- `git.rs` — `Repo::discover()` (CWD-dependent) leaks into daemon paths

Additionally, `derive_worker_name()` strips category prefixes, turning `features/global-attach` into `global-attach`, which loses the issue category context.

## Implementation

### 1. Fix `derive_worker_name()` to preserve category prefix

**`crates/jig-core/src/daemon/issue_actor.rs`**:
```rust
fn derive_worker_name(issue_id: &str) -> String {
    issue_id.to_lowercase()
}
```
Preserves `features/global-attach`, `bugs/fix-foo`. Linear IDs like `ENG-123` pass through unchanged. Update tests.

### 2. Expand `Worktree` struct

**`crates/jig-core/src/worktree.rs`** — Make `Worktree` the single source of truth:

```rust
pub struct Worktree {
    pub name: String,          // relative path from .jig/ (e.g. "features/global-attach")
    pub path: PathBuf,         // full filesystem path
    pub branch: String,        // git branch name
    pub repo_root: PathBuf,    // parent repo root
    pub session_name: String,  // tmux session (jig-<repo>)
    pub auto_spawned: bool,    // daemon-created vs manual
}
```

#### Lifecycle methods (absorb from spawn.rs + daemon/mod.rs):

- **`Worktree::create(repo_root, name, branch, base_branch, auto, copy_files, on_create_hook) -> Result<Self>`**
  - Opens `Repo::open(repo_root)` — no CWD dependence
  - Calls `repo.create_worktree()`
  - Copies files, runs hook
  - Returns populated struct

- **`Worktree::open(repo_root, worktrees_dir, name) -> Result<Self>`**
  - Opens existing worktree from disk
  - Reads `auto_spawned` from event log's Spawn event (if available)

- **`Worktree::list(repo_root, worktrees_dir) -> Result<Vec<Self>>`**
  - Scans `.jig/` for worktree dirs

- **`Worktree::remove(&self, force: bool) -> Result<()>`**
  - Uses `Repo::open(&self.repo_root)` — never `Repo::discover()`
  - Cleans up empty parent dirs

#### Tmux methods (absorb from spawn.rs):

- **`self.has_tmux_window() -> bool`**
- **`self.is_agent_running() -> bool`** — pane has non-shell process
- **`self.launch(&self, context: Option<&str>, auto: bool) -> Result<()>`**
  - Creates tmux window, renders preamble, sends claude command
  - Absorbs `spawn::launch_tmux_window()`

- **`self.resume(&self, context: Option<&str>) -> Result<()>`**
  - Appends `Resume` event (does NOT reset event log)
  - Calls `self.launch()`

#### Orphan detection:

- **`self.is_orphaned() -> bool`** — auto_spawned && !has_tmux_window() && worktree exists on disk

#### Registration (absorb from spawn.rs):

- **`self.register(&self, context: Option<&str>, issue_ref: Option<&str>) -> Result<()>`**
  - Creates `OrchestratorState` entry + emits Spawn event
  - Records `auto` and `context` in the Spawn event for recovery

- **`self.unregister(&self) -> Result<()>`**
  - Removes from state + cleans up event log

### 3. Add `Resume` event type

**`crates/jig-core/src/events/schema.rs`** — Add `Resume` to `EventType` enum.

**`crates/jig-core/src/events/reducer.rs`** — Handle `Resume`: transition to `Spawned`, preserve `issue_ref`, `commit_count`, event history.

### 4. Slim down `spawn.rs`

After moving lifecycle logic to `Worktree`, `spawn.rs` retains only:
- `TaskStatus` enum and `TaskInfo` struct (for `jig ps`)
- `list_tasks()` / `get_worker_status()` / `cleanup_stale_workers()` — tmux status queries
- `attach()` / `kill_window()` — thin wrappers

### 5. Update daemon to use `Worktree`

**`crates/jig-core/src/daemon/mod.rs`**:
- `auto_spawn_worker()` → `Worktree::create()` + `wt.register()` + `wt.launch()`
- `process_worker()` → load as `Worktree`, use `wt.is_orphaned()` for dead detection
- Orphaned worktrees get `wt.resume()` instead of nudging

### 6. Fix `Repo::remove_worktree()` signature

**`crates/jig-core/src/git.rs`** — Take `repo_root: &Path` instead of falling back to `Repo::discover()`. Or make it a `&self` method.

### 7. Update CLI callers

- `crates/jig-cli/src/commands/create.rs` → `Worktree::create()` + `wt.register()` + `wt.launch()`
- `crates/jig-cli/src/commands/remove.rs` → `Worktree::open()` + `wt.remove()`
- `crates/jig-cli/src/commands/spawn.rs` → use `Worktree` methods
- `crates/jig-core/src/daemon/prune_actor.rs` → pass repo_root to remove

## Files

- `crates/jig-core/src/worktree.rs` — Expand struct, absorb lifecycle/tmux/register methods
- `crates/jig-core/src/spawn.rs` — Slim down, delegate to Worktree
- `crates/jig-core/src/git.rs` — Fix `remove_worktree()` CWD fallback
- `crates/jig-core/src/events/schema.rs` — Add `Resume` event type
- `crates/jig-core/src/events/reducer.rs` — Handle `Resume`
- `crates/jig-core/src/daemon/issue_actor.rs` — Fix `derive_worker_name()` + tests
- `crates/jig-core/src/daemon/mod.rs` — Use `Worktree` in auto_spawn + process_worker
- `crates/jig-core/src/daemon/prune_actor.rs` — Update remove call
- `crates/jig-cli/src/commands/create.rs` — Use `Worktree::create()`
- `crates/jig-cli/src/commands/remove.rs` — Use `Worktree::open()` + `remove()`
- `crates/jig-cli/src/commands/spawn.rs` — Use `Worktree` methods

## Acceptance Criteria

- [ ] `Worktree` struct is the single source of truth for worker physical state
- [ ] `derive_worker_name("features/global-attach")` → `"features/global-attach"`
- [ ] `derive_worker_name("ENG-123")` → `"eng-123"`
- [ ] `Resume` event type exists and reducer handles it correctly
- [ ] No `spawn::register()` or `spawn::launch_tmux_window()` calls outside `Worktree`
- [ ] `Repo::discover()` only used in CLI `context.rs`, not in daemon paths
- [ ] `Worktree::remove()` uses `Repo::open(repo_root)`, not `Repo::discover()`
- [ ] Daemon uses `Worktree::create()` + `wt.launch()` for auto-spawn
- [ ] Orphan detection via `wt.is_orphaned()` works in daemon ticks
- [ ] CLI commands (`create`, `remove`, `spawn`) use `Worktree` methods
- [ ] `cargo build && cargo test && cargo clippy` passes

## Verification

```bash
# Build and test
cargo build && cargo test && cargo clippy

# Worker naming
# derive_worker_name("features/global-attach") → "features/global-attach"
# derive_worker_name("ENG-123") → "eng-123"

# No CWD-dependent repo access in daemon
grep -r "Repo::discover" crates/jig-core/src/daemon/ # should return nothing

# No direct spawn calls outside Worktree
grep -r "spawn::register\|spawn::launch_tmux_window" crates/ # should return nothing
```
