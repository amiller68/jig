# Planned Issue Management

**Status:** Planned  
**Priority:** Medium  
**Category:** Improvements

## Objective

Improve how jig handles planned issues: auto-spawning high/urgent issues, listing pending issues, and allowing batch approval.

## Background

The issue-grinder currently:
- Auto-spawns issues with `Priority: High` or `Priority: Urgent`
- Collects other planned issues and reports them
- Requires manual spawning for non-urgent issues

This could be improved with:
- Better filtering and display of planned issues
- Batch spawning
- Priority queues
- Issue dependencies

## Acceptance Criteria

### Repo Registry Integration
- [ ] Depends on: `issues/features/global-commands.md` (repo registry)
- [ ] Discover issues in all auto-registered repos or specific `--repo <path>`
- [ ] Use `GlobalContext` for efficient multi-repo operations
- [ ] Issue directory configured in `jig.toml`:
  ```toml
  [issues]
  directory = "issues"  # default
  autoSpawnPriorities = ["Urgent", "High"]
  maxConcurrentWorkers = 5  # 0 = unlimited
  ```
- [ ] Support `-g` flag to show issues across all registered repos

### Issue Discovery
- [ ] `jig issues list` - show all planned issues (current repo)
- [ ] `jig issues list -g` - show issues across all registered repos
- [ ] `jig issues list --priority high` - filter by priority
- [ ] `jig issues list --category features` - filter by category
- [ ] Display: repo, name, title, priority, category, description excerpt

### Priority-Based Auto-Spawn
- [ ] Configurable auto-spawn priorities per-repo (in `jig.toml`)
- [ ] Default: spawn `High` and `Urgent` on `jig health`
- [ ] Respect per-repo max concurrent workers
- [ ] Global setting: `~/.config/jig/config` for default max workers

### Batch Spawning
- [ ] `jig issues spawn --batch` - spawn multiple issues interactively
- [ ] Show list with checkboxes
- [ ] Confirm before spawning
- [ ] Track which issues are already being worked on

### Issue Dependencies
- [ ] Add `Depends-On:` field to issue frontmatter
- [ ] `jig issues list` shows blocked issues
- [ ] Auto-spawn only when dependencies done
- [ ] Example:
  ```markdown
  Priority: High
  Depends-On: issues/features/github-integration.md
  ```

### Issue Templates
- [ ] Support subdirectories: `issues/features/`, `issues/bugs/`, etc.
- [ ] Template validation: require certain fields
- [ ] Auto-categorize based on directory
- [ ] Generate issue from template: `jig issues create --template feature`

### Pending Queue
- [ ] `jig issues queue` - show pending issues in priority order
- [ ] `jig issues queue --spawn-next` - spawn next in queue
- [ ] Track spawn history (don't re-spawn recently failed issues)

## Commands

```bash
# List all planned issues (current repo)
jig issues list

# List issues across all registered repos
jig issues list -g

# Filter by priority
jig issues list --priority high [--repo <path>]

# Filter by category/subdirectory
jig issues list --category features [--repo <path>]

# Show next issue in queue
jig issues next [--repo <path>]

# Spawn next issue
jig issues spawn-next [--repo <path>]

# Spawn specific issue
jig issues spawn features/github-integration [--repo <path>]

# Batch spawn (interactive, current repo or -g)
jig issues spawn --batch [--repo <path>]
jig issues spawn --batch -g  # across all registered repos

# Create new issue from template
jig issues create --template feature [--repo <path>]
jig issues create --template bug [--repo <path>]
```

## Issue Frontmatter Schema

```markdown
# Title

**Status:** Planned | In Progress | In Review | Done  
**Priority:** Urgent | High | Medium | Low  
**Category:** Features | Improvements | Bugs | Chores  
**Depends-On:** issues/path/to/dependency.md (optional)  
**Assigned-To:** worker-name (auto-populated)  

## Objective
...
```

## Configuration

**Per-repo settings in `jig.toml`:**

```toml
[issues]
# Auto-spawn priorities (list)
autoSpawnPriorities = ["Urgent", "High"]

# Max concurrent workers (0 = unlimited)
maxConcurrentWorkers = 5

# Issue directory (relative to repo root)
directory = "issues"

# Require issue file for all workers
requireIssueFile = true

# Template directory (relative to repo root)
templateDirectory = "issues/_templates"
```

**Global defaults in `~/.config/jig/config`:**
- `issues.maxConcurrentWorkers=5` - default max across all repos
- Per-repo settings override global defaults

## Implementation Notes

- Depends on: `issues/features/global-commands.md` (repo registry + GlobalContext)

1. Add issue parsing utilities to jig-core
2. Parse frontmatter (YAML or key-value)
3. Issue discovery: recursive scan of configured `issues/` directory per repo
4. Use `GlobalContext` for efficient `-g` flag operations
5. Priority queue: sort by priority + age (per-repo or global)
6. Dependency graph: check `Depends-On` field (can reference issues in other repos)
7. Interactive batch spawn: use `inquire` crate
8. Repo-specific settings from `jig.toml`, fallback to global config

## Example Output

**Single repo:**
```
$ jig issues list

PLANNED ISSUES (jig):
┌─────────────────────────────────────┬──────────┬──────────┬─────────────────────────────────┐
│ Issue                               │ Priority │ Category │ Description                     │
├─────────────────────────────────────┼──────────┼──────────┼─────────────────────────────────┤
│ features/github-integration         │ High     │ Features │ Native GitHub API integration   │
│ features/worker-heartbeat           │ High     │ Features │ Built-in health checks          │
│ improvements/activity-metrics       │ Medium   │ Improve  │ Track worker activity           │
│ bugs/auto-spawn-blocked             │ Medium   │ Bugs     │ Fix permission prompt blocking  │
└─────────────────────────────────────┴──────────┴──────────┴─────────────────────────────────┘

Run 'jig issues spawn-next' to spawn the next high-priority issue.
Run 'jig issues spawn --batch' to spawn multiple issues.
```

**Multi-repo:**
```
$ jig issues list -g

PLANNED ISSUES ACROSS ALL REPOS:
┌──────────┬─────────────────────────────────────┬──────────┬──────────┬─────────────────────────────────┐
│ Repo     │ Issue                               │ Priority │ Category │ Description                     │
├──────────┼─────────────────────────────────────┼──────────┼──────────┼─────────────────────────────────┤
│ jig      │ features/github-integration         │ High     │ Features │ Native GitHub API integration   │
│ jig      │ features/worker-heartbeat           │ High     │ Features │ Built-in health checks          │
│ jax-fs   │ features/streaming-encryption       │ High     │ Features │ Streaming encrypted updates     │
│ jig      │ improvements/activity-metrics       │ Medium   │ Improve  │ Track worker activity           │
└──────────┴─────────────────────────────────────┴──────────┴──────────┴─────────────────────────────────┘

Run 'jig issues spawn-next -g' to spawn across all repos.
```

## Related Issues

- #TBD: Worker heartbeat system
- #TBD: Worker lifecycle management
- #TBD: Issue templates system

## References

- Current implementation: `~/.openclaw/workspace/skills/issue-grinder/grind.sh` (lines 511-597)
