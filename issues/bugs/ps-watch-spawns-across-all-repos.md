# `ps -w` auto-spawns issues from all repos, not just the current one

**Status:** Planned
**Priority:** Urgent
**Labels:** auto

## Objective

When running `jig ps -w` inside a repo, auto-spawn should only act on that repo's issues — not every repo in the registry.

## Observed Behavior

Running `jig ps -w` in `work/quotient` triggers auto-spawning of issues from `krondor/jig` (and any other registered repo with eligible issues). This is unexpected — the user is working in quotient and only wants to manage quotient workers.

## Root Cause

`ps -w` resolves `auto_spawn = true` from the current repo's `jig.toml` and passes it as `RuntimeConfig.auto_spawn`. But the issue polling path (`runtime.maybe_trigger_issue_poll()` at `crates/jig-core/src/daemon/runtime.rs:212`) sends **all repos** from the registry to the issue actor, ignoring the `DaemonConfig.repo_filter`.

The worker display is correctly filtered by `repo_filter` (line 234 of `mod.rs`), but the issue poll and spawn path has no such filter — it always polls and spawns across the entire registry.

## Design Consideration

There are two valid models:

### Option A: Respect repo_filter in issue polling

When `repo_filter` is set (i.e. running from a specific repo), only poll and spawn issues for that repo. This is the minimal fix and matches user expectations for `ps -w`.

Thread the `repo_filter` from `DaemonConfig` into `maybe_trigger_issue_poll()` and filter the repos list before sending to the issue actor.

### Option B: Independent daemon per repo

Run the daemon loop scoped entirely to one repo. Each `ps -w` invocation manages only its repo. Multiple `ps -w` sessions in different repos run independent daemon loops.

This is a larger change but is conceptually cleaner — each repo is self-contained.

### Recommendation

Option A for now — it's a one-line filter and fixes the immediate problem. Option B is a bigger architectural decision for later.

## Implementation

1. In `DaemonRuntime::maybe_trigger_issue_poll()` (`crates/jig-core/src/daemon/runtime.rs`), accept an optional `repo_filter: Option<&str>` parameter

2. Filter the repos list to only include repos matching the filter before sending to the issue actor:
   ```rust
   let repos: Vec<_> = registry.repos().iter()
       .filter(|entry| {
           repo_filter.map_or(true, |filter| {
               entry.path.file_name()
                   .map(|n| n.to_string_lossy() == filter)
                   .unwrap_or(false)
           })
       })
       // ...
   ```

3. Pass `self.daemon_config.repo_filter.as_deref()` from the tick into `maybe_trigger_issue_poll()`

4. Same filter should apply to the sync actor (`maybe_trigger_sync`) and prune path for consistency

## Files

- `crates/jig-core/src/daemon/runtime.rs` — Add repo_filter param to `maybe_trigger_issue_poll`
- `crates/jig-core/src/daemon/mod.rs` — Pass repo_filter from daemon_config into runtime methods

## Acceptance Criteria

- [ ] `jig ps -w` in repo X only auto-spawns issues from repo X
- [ ] `jig ps -w --global` (or future global daemon) still polls all repos
- [ ] Workers from other repos still display correctly (display filter already works)

## Verification

```bash
# In repo A with auto_spawn = true and eligible issues
cd work/quotient
jig ps -w
# Should only spawn quotient issues

# In repo B with auto_spawn = true and eligible issues
cd krondor/jig
jig ps -w
# Should only spawn jig issues

# Neither should cross-pollinate
```
