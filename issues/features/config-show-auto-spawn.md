# Show auto-spawn config in `jig config` and add `jig issues --auto`

**Status:** Planned
**Priority:** High
**Labels:** auto

## Objective

Make auto-spawn configuration visible and debuggable so users can understand why issues are or aren't being auto-spawned.

## Context

When auto-spawn isn't working, there's no easy way to diagnose it. `jig config show` only displays base branch and on-create hook — it says nothing about spawn settings or issue spawn labels. Users also can't quickly see which issues the daemon considers eligible for spawning without manually cross-referencing `spawn_labels`, issue status, and dependency state.

Two gaps:

1. **`jig config show`** doesn't display any auto-spawn configuration (auto_spawn enabled/disabled, max_concurrent_workers, auto_spawn_interval, spawn_labels).
2. **`jig issues`** has no way to filter to just the issues that would be auto-spawned by the daemon.

## Implementation

### Part 1: Show auto-spawn config in `jig config show`

1. Extend `ConfigDisplay` in `crates/jig-core/src/config.rs` with spawn fields:
   - `auto_spawn: bool` (resolved: jig.toml override > global default)
   - `auto_spawn_source: String` (where the value came from: "jig.toml", "global config", "default")
   - `max_concurrent_workers: usize` (resolved)
   - `auto_spawn_interval: u64` (resolved)
   - `spawn_labels: Vec<String>` (from jig.toml)
   - `auto_start: bool` (jig.toml `[spawn] auto`)

2. Update `ConfigDisplay::load()` to populate these from `JigToml` + `GlobalConfig`.

3. Update `show_config()` in `crates/jig-cli/src/commands/config.rs` to render a new "Auto-spawn" section after the existing output:
   ```
   Auto-spawn:
     Enabled:            true (jig.toml)
     Auto-start Claude:  true
     Max workers:        3 (global default)
     Poll interval:      120s (global default)
     Spawn labels:       jig-auto
   ```
   When `spawn_labels` is empty, show a warning hint: `(none — no issues will be eligible)`

### Part 2: Add `--auto` flag to `jig issues`

1. Add `--auto` flag to the `Issues` struct in `crates/jig-cli/src/commands/issues.rs`:
   ```rust
   /// Show only auto-spawn candidates (planned, labeled, deps satisfied)
   #[arg(long)]
   pub auto: bool,
   ```

2. When `--auto` is set, use `provider.list_spawnable(&spawn_labels)` instead of `provider.list(&filter)` to show exactly what the daemon would pick up. This reuses the existing `list_spawnable` method which already filters by status=Planned, spawn_labels match, and dependency satisfaction.

3. The `--auto` flag should compose with other filters (`--priority`, `--category`, etc.) by applying them after the spawnable filter.

## Files

- `crates/jig-core/src/config.rs` — Extend `ConfigDisplay` with spawn fields and update `load()`
- `crates/jig-cli/src/commands/config.rs` — Render auto-spawn section in `show_config()`
- `crates/jig-cli/src/commands/issues.rs` — Add `--auto` flag, use `list_spawnable` path

## Acceptance Criteria

- [ ] `jig config show` displays auto-spawn enabled/disabled with source
- [ ] `jig config show` displays max workers, poll interval, and spawn labels
- [ ] `jig config show` warns when spawn_labels is empty
- [ ] `jig issues --auto` lists only issues eligible for auto-spawn
- [ ] `jig issues --auto` returns nothing when spawn_labels is not configured (matches daemon behavior)
- [ ] `--auto` composes with `--priority`, `--category`, and other existing filters

## Verification

```bash
# Check config output includes spawn section
jig config show

# Compare auto-spawn candidates with full list
jig issues
jig issues --auto

# Verify empty spawn_labels shows warning in config and empty list in issues
# (remove spawn_labels from jig.toml temporarily)
jig config show
jig issues --auto
```
