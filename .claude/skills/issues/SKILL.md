---
description: Discover and manage work items. Use to explore tasks before spawning workers or to track project progress.
allowed-tools:
  - Bash(jig:*)
  - Bash(ls:*)
  - Bash(mkdir:*)
  - Read
  - Write
  - Edit
  - Glob
  - Grep
---

Discover and manage work items via `jig issues`. This is the first-class way to understand what work exists, what's in progress, and what to do next.

Issues are tracked in **Linear**. Use `jig issues` to query Linear tickets directly — no need to open the Linear UI. The CLI handles filtering, dependency resolution, and status transitions.

## Discovery workflow

Start here when picking up work or building context:

```bash
# See all active issues for the repo
jig issues

# High-priority items needing attention
jig issues --priority high
jig issues --priority urgent

# What's planned and ready to start (dependencies satisfied)
jig issues --unblocked --status planned

# What's currently blocked
jig issues --blocked

# Auto-spawn candidates (planned + labeled + deps satisfied)
jig issues --auto

# IDs only (for scripting / piping to spawn)
jig issues --ids --status planned
```

## Actions

### Show issue

```bash
jig issues <id>
# e.g. jig issues AUT-5044
```

### Filter by category, label, status

```bash
# By project/category
jig issues --category Engineering

# By label (all must match)
jig issues --label backend --label auto

# Combine filters
jig issues --status planned --priority high --label auto
```

### Create issue

```bash
# Basic — creates in Linear (or file provider, depending on config)
jig issues create "Fix auth crash"

# With body and metadata
jig issues create "Fix auth crash" \
  --body "Stack trace in session handler" \
  --priority high \
  --label backend --label bug

# With project (Linear) or directory (file)
jig issues create "Fix auth crash" --category Backend

# Body from stdin
echo "detailed description" | jig issues create "Title" --body -
```

Prints the created identifier to stdout (e.g. `AUT-1234`).

### Update status

```bash
jig issues status <id> --status in-progress
jig issues status <id> --status blocked
```

### Complete issue

```bash
jig issues complete <id>
```

### Create issue

```bash
# Basic
jig issues create "Issue title"

# With priority, labels, and body
jig issues create -p high -l auto -b "Description body in markdown" "Issue title"

# Read body from stdin
echo "body" | jig issues create -b - "Issue title"

# With project/category
jig issues create -c Engineering "Issue title"

# As a sub-issue of another issue
jig issues create "Sub-task title" --parent AUT-19
```

### Update issue fields

```bash
# Set parent on existing issue
jig issues update AUT-21 --parent AUT-19

# Remove parent relation
jig issues update AUT-21 --remove-parent

# Update title, priority, labels, category, body
jig issues update AUT-21 --title "New title" --priority high
jig issues update AUT-21 --label backend --label auto
jig issues update AUT-21 --body "New description"
```

### Stats

```bash
jig issues stats
jig issues stats -g   # across all tracked repos
```

### Global scope

```bash
# Any command works across all repos with -g
jig issues -g
jig issues -g --status in-progress
```

## Parent / sub-issue relations

Issues can be linked as sub-issues of a parent via `--parent`:

```bash
# Create sub-issue
jig issues create "Review data model" --parent AUT-19

# Set parent on existing issue
jig issues update AUT-21 --parent AUT-19

# Remove parent
jig issues update AUT-21 --remove-parent
```

When viewing a single issue (`jig issues <id>`), the parent is shown if set:

```
Parent: AUT-19 — spec: automated review process
```

For the file provider, parent is stored as a `**Parent:**` frontmatter field. For Linear, it uses the native parent/sub-issue relation.

## Dependencies

Issues can depend on other issues via Linear's `is_blocked_by` relations. Dependencies must be `Complete` before the dependent issue is spawnable.

### Manage dependencies

```bash
# Add a blocking dependency (JIG-22 is blocked by JIG-21)
jig issues update JIG-22 --blocked-by JIG-21

# Add multiple blockers at once
jig issues update JIG-22 --blocked-by JIG-21,JIG-24

# Remove a dependency
jig issues update JIG-22 --remove-blocked-by JIG-21
```

### View dependencies

```bash
# Single issue view shows blockers
jig issues JIG-22
# Output includes: Blocked by: JIG-21, JIG-24
```

## Convention

Typically for straightforward and well-defined tasks, we prefer setting the `auto` label such that said tasks are picked up and spawned by the jig daemon.
