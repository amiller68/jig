---
description: Discover and manage file-based work items. Use to explore tasks before spawning workers or to track project progress.
allowed-tools:
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

Scan `issues/` and show all work items:

```bash
# All issues
find issues -name "*.md" -not -path "*/_templates/*" -not -name "README.md"

# By status
grep -r "Status.*Planned" issues/
```

Display with status indicators:
- `[ ]` Planned
- `[~]` In Progress
- `[x]` Complete
- `[!]` Blocked

### Show issue

Read and display a specific issue file.

### Create standalone issue

```bash
cp issues/_templates/standalone.md issues/features/my-feature.md
# or issues/bugs/, issues/chores/
```

### Create epic

```bash
mkdir issues/epics/my-epic
cp issues/_templates/epic-index.md issues/epics/my-epic/index.md
cp issues/_templates/ticket.md issues/epics/my-epic/0-first-task.md
```

### Create ticket in epic

```bash
cp issues/_templates/ticket.md issues/epics/my-epic/1-next-task.md
```

Update the epic's `index.md` ticket table.

### Update status

Change the `**Status:**` field:
- `Planned` → `In Progress` → `Complete`
- Or `Blocked` if waiting on something

### Check dependencies

Issues can depend on other issues via path:

```markdown
**Depends-On:** issues/epics/git-hooks/0-wrapper-pattern.md
```

Check if dependencies are satisfied:
```bash
# Read the dependency file and check its status
grep "Status:" issues/epics/git-hooks/0-wrapper-pattern.md
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

## External Trackers

For Linear, Jira, or GitHub Issues, use their MCP tools or CLI instead of file scanning.
