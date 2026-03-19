---
description: Discover and manage work items. Use to explore tasks before spawning workers or to track project progress. Works with any configured issue provider.
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

Discover and manage work items via `jig issues`. Works with any configured provider (file, Linear).

## Actions

### List issues

```bash
# All issues
jig issues

# Filter by status
jig issues --status planned

# Filter by priority
jig issues --priority high

# Show IDs only
jig issues --ids
```

### Show issue

```bash
jig issues <id>
# e.g. jig issues features/my-feature (file provider)
# e.g. jig issues ENG-123 (Linear provider)
```

### Create issue

```bash
# Basic (defaults to features/ category)
jig issues create "Add verbose flag"

# With options
jig issues create "Fix crash on exit" --priority high --category bugs

# With labels
jig issues create "Refactor auth" --label backend --label auto
```

### Update status

```bash
jig issues status <id> --status in-progress
jig issues status <id> --status blocked
```

### Complete issue

```bash
# Mark complete
jig issues complete <id>

# Mark complete and delete file (file provider only)
jig issues complete <id> --delete
```

### Stats

```bash
# Local repo stats
jig issues stats

# Global stats across all repos
jig issues stats -g
```

### Check dependencies

Issues can depend on other issues via path:

```markdown
**Depends-On:** issues/epics/git-hooks/0-wrapper-pattern.md
```

Dependencies must be `Complete` before starting dependent issue.

### Epic tickets (file provider)

Epic tickets in `issues/epics/name/N-ticket.md` are ordered:
- `0-*.md` must complete before `1-*.md`
- `1-*.md` must complete before `2-*.md`
- Numbering implies dependency order

Epic `index.md` tracks overall progress and ticket status.

## Convention

See `issues/README.md` for full documentation.

Typically for straightforward and well-defined tasks,
 we prefer setting the `auto` label such that said tasks
 are picked up and spawned by the jig daemon.
