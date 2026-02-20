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

### Issue Discovery
- [ ] `jig issues list` - show all planned issues
- [ ] `jig issues list --priority high` - filter by priority
- [ ] `jig issues list --category features` - filter by category
- [ ] Display: name, title, priority, category, description excerpt

### Priority-Based Auto-Spawn
- [ ] Configurable auto-spawn priorities
- [ ] Default: spawn `High` and `Urgent` on `jig health`
- [ ] Option: `jig config set autoSpawn.priorities ["Urgent", "High"]`
- [ ] Respect max concurrent workers (e.g., max 5 at once)

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
# List all planned issues
jig issues list

# Filter by priority
jig issues list --priority high

# Filter by category/subdirectory
jig issues list --category features

# Show next issue in queue
jig issues next

# Spawn next issue
jig issues spawn-next

# Spawn specific issue
jig issues spawn features/github-integration

# Batch spawn (interactive)
jig issues spawn --batch

# Create new issue from template
jig issues create --template feature
jig issues create --template bug
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

```toml
[issues]
# Auto-spawn priorities (list)
autoSpawnPriorities = ["Urgent", "High"]

# Max concurrent workers (0 = unlimited)
maxConcurrentWorkers = 5

# Issue directory
directory = "issues"

# Require issue file for all workers
requireIssueFile = true

# Template directory
templateDirectory = "issues/_templates"
```

## Implementation Notes

1. Add issue parsing utilities to jig-core
2. Parse frontmatter (YAML or key-value)
3. Issue discovery: recursive scan of `issues/` directory
4. Priority queue: sort by priority + age
5. Dependency graph: check `Depends-On` field
6. Interactive batch spawn: use `inquire` crate

## Example Output

```
$ jig issues list

PLANNED ISSUES:
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

## Related Issues

- #TBD: Worker heartbeat system
- #TBD: Worker lifecycle management
- #TBD: Issue templates system

## References

- Current implementation: `~/.openclaw/workspace/skills/issue-grinder/grind.sh` (lines 511-597)
