---
description: Discover and manage file-based work items. Use to explore tasks before spawning workers or to track project progress.
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

Discover and manage work items in `issues/`.

## Directory Structure

```
issues/
├── _templates/           # Issue templates
├── epics/                # Multi-ticket features (directories)
│   └── feature-name/
│       ├── index.md      # Epic overview
│       └── 0-task.md     # Tickets (0-indexed)
├── features/             # Single-ticket features
├── bugs/                 # Bug fixes
└── chores/               # Maintenance tasks
```

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
# e.g. jig issues features/my-feature
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

### Create epic

Epics still use manual directory setup:

```bash
mkdir issues/epics/my-epic
cp issues/_templates/epic-index.md issues/epics/my-epic/index.md
cp issues/_templates/ticket.md issues/epics/my-epic/0-first-task.md
```

Update the epic's `index.md` ticket table.

### Update status

```bash
jig issues status features/my-feature --status in-progress
jig issues status bugs/crash-fix --status blocked
```

### Complete issue

```bash
# Mark complete (keeps file)
jig issues complete features/my-feature

# Mark complete and delete file
jig issues complete features/my-feature --delete
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

### Epic tickets

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

## External Trackers

For Linear, Jira, or GitHub Issues, use their MCP tools or CLI instead of file scanning.
