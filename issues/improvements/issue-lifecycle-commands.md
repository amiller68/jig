# Issue Lifecycle Commands

**Status:** Complete
**Priority:** Low
**Labels:** auto

## Objective

Add CLI commands for managing issue state: create from template, update status, mark complete, show stats.

## Current State

- `jig issues` lists/browses issues (read-only)
- No commands to create, update, or complete issues
- Status updates must be done by hand-editing files or via Linear UI

## Commands

```bash
# Create from template
jig issues create --template feature "Add verbose flag"
jig issues create --template bug --priority High "Fix crash on exit"

# Update status
jig issues status features/my-feature --status "In Progress"

# Mark complete (sets status, optionally deletes file)
jig issues complete features/my-feature
jig issues complete features/my-feature --delete

# Stats
jig issues stats
jig issues stats -g
```

### Stats output

```
By Status:  Planned: 8  In Progress: 3  Complete: 12  Blocked: 2
By Priority: Urgent: 1  High: 4  Medium: 6  Low: 5
```

## Acceptance Criteria

- [ ] `jig issues create` from templates with title, priority, category args
- [ ] `jig issues status` updates frontmatter in file issues
- [ ] `jig issues complete` sets status to Complete
- [ ] `jig issues stats` shows breakdown by status and priority
- [ ] Global mode (`-g`) aggregates across repos
- [ ] For Linear issues, status changes go through API (not file editing)
