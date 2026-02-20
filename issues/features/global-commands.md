# Global Commands

**Status:** Planned

## Objective

Enable jig to run commands across all tracked repositories via `-g`/`--global` flag, with shared context to avoid redundant state operations.

## Background

Currently jig operates on a single repository at a time. Power users with many repos want to:
- List all active workers across all projects
- Clean up worktrees globally
- Get a unified view of work in progress

Running commands repo-by-repo is inefficient—each invocation reloads config, re-parses state, etc.

## Design

### Repository Registry

Store known repositories in XDG config (`~/.config/jig/repos.json`):

```json
{
  "repos": [
    {
      "path": "/Users/al/projects/foo",
      "added": "2026-02-19T12:00:00Z",
      "last_used": "2026-02-19T14:30:00Z"
    }
  ]
}
```

**Auto-registration:** When jig runs in a repo, register it automatically.

### Shared Context

Introduce a `GlobalContext` that:
- Loads config once
- Loads registry once
- Caches repo state as it iterates
- Batches writes (e.g., update registry `last_used` once at end)

```rust
struct GlobalContext {
    config: Config,
    registry: RepoRegistry,
    repo_states: HashMap<PathBuf, RepoState>,
}

impl GlobalContext {
    fn for_each_repo<F>(&mut self, f: F) -> Result<()>
    where F: FnMut(&mut RepoState) -> Result<()>;

    fn collect_from_repos<T, F>(&mut self, f: F) -> Result<Vec<T>>
    where F: FnMut(&RepoState) -> Result<T>;
}
```

This avoids:
- Reloading `~/.config/jig/config` per repo
- Re-parsing `jig.toml` multiple times
- Multiple registry writes

### Global Flag

```bash
jig -g list      # List worktrees in all known repos
jig -g ps        # Show workers across all repos
jig -g clean     # Clean worktrees in all repos
jig -g status    # Summary of all repos
```

### Output Format

Prefix output with repo identifier:

```
[foo]
  feature/auth (worker: idle)
  fix/bug-123 (no worker)

[bar]
  main-worktree (worker: running)
```

## Implementation

1. **Add RepoRegistry to jig-core**
   - `crates/jig-core/src/registry.rs`
   - Load/save `repos.json`
   - Auto-register on any jig command
   - Prune invalid paths on load

2. **Add GlobalContext to jig-core**
   - `crates/jig-core/src/context.rs`
   - Single load of config + registry
   - Lazy-load repo states
   - Batch state updates

3. **Add global flag to CLI**
   - `crates/jig-cli/src/main.rs` — add `--global` to top-level args
   - Construct `GlobalContext` when flag present

4. **Refactor commands for dual mode**
   - Commands accept either single repo OR global context
   - Extract core logic into functions that work on `RepoState`
   - Wrap with single-repo or multi-repo iteration

5. **Add registry management commands**
   - `jig repos list` — show tracked repos
   - `jig repos add <path>` — manually add repo
   - `jig repos remove <path>` — untrack repo
   - `jig repos prune` — remove invalid paths

## Files

- `crates/jig-core/src/registry.rs` — RepoRegistry struct
- `crates/jig-core/src/context.rs` — GlobalContext for shared state
- `crates/jig-core/src/lib.rs` — export new modules
- `crates/jig-cli/src/main.rs` — add `--global` flag
- `crates/jig-cli/src/commands/repos.rs` — registry management
- `crates/jig-cli/src/commands/list.rs` — support global mode
- `crates/jig-cli/src/commands/ps.rs` — support global mode

## Acceptance Criteria

- [ ] Repos auto-registered when jig runs
- [ ] `jig -g list` shows worktrees from all repos
- [ ] `jig -g ps` shows workers from all repos
- [ ] Global commands load config/registry only once
- [ ] `jig repos list` shows tracked repos
- [ ] `jig repos prune` removes stale entries
- [ ] Invalid repo paths skipped with warning

## Verification

```bash
# Register some repos
cd ~/project-a && jig list
cd ~/project-b && jig list

# Check registry
jig repos list

# Global commands
jig -g list
jig -g ps
```

## Open Questions

- Should `jig -g spawn` be supported? (Probably not—too ambiguous)
- Parallel iteration over repos? (Could speed up, but complicates output)
