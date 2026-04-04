---
description: Discover and manage work items. Use to explore tasks before spawning workers or to track project progress. Works with any configured issue provider.
allowed-tools:
  - Bash(jig:*)
  - Bash(ls:*)
  - Bash(find:*)
  - Bash(grep:*)
  - Bash(mkdir:*)
  - Bash(cp:*)
  - Read
  - Write
  - Edit
  - Glob
  - Grep
---

Discover and manage work items. Works with any configured provider (file, Linear).

If `jig` CLI is available, prefer `jig issues` commands. Otherwise fall back to raw file operations for the file provider.

## Actions

### List issues

```bash
# Via jig CLI
jig issues
jig issues --status planned
jig issues --priority high

# Via file system (fallback)
find issues -name "*.md" -not -path "*/_templates/*" -not -name "README.md"
grep -r "Status.*Planned" issues/
```

Display with status indicators:
- `[ ]` Planned
- `[~]` In Progress
- `[x]` Complete
- `[!]` Blocked

### Show issue

```bash
# Via jig CLI
jig issues <id>

# Via file system (fallback)
# Read the issue file directly
```

### Create issue

```bash
# Via jig CLI
jig issues create "Add verbose flag"
jig issues create "Fix crash on exit" --priority high --category bugs
jig issues create "Refactor auth" --label backend --label auto

# Via file system (fallback, file provider only)
cp issues/_templates/standalone.md issues/features/my-feature.md
# or issues/bugs/, issues/chores/
```

### Update status

```bash
# Via jig CLI
jig issues status <id> --status in-progress

# Via file system (fallback)
# Change the **Status:** field in the issue file
```

### Complete issue

```bash
# Via jig CLI
jig issues complete <id>
jig issues complete <id> --delete
```

### Manage dependencies

```bash
# Via jig CLI — add a blocker
jig issues update <id> --blocked-by <blocker-id>

# Add multiple blockers at once (comma-separated)
jig issues update <id> --blocked-by dep-a,dep-b

# Remove a blocker
jig issues update <id> --remove-blocked-by <blocker-id>

# Via file system (fallback)
# Edit the **Depends-On:** field in the issue file
```

### Stats

```bash
jig issues stats
jig issues stats -g
```

## Convention

See `issues/README.md` for full documentation.
