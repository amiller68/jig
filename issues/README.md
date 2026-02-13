# Issues

File-based issue tracking for AI agents and contributors.

---

## Directory Structure

```
issues/
├── README.md                       # This file
├── _templates/                     # Issue templates
│   ├── standalone.md
│   ├── epic-index.md
│   └── ticket.md
├── epics/                          # Large multi-ticket features
│   └── plan-and-execute/
│       ├── index.md
│       ├── 0-issue-schema.md
│       ├── 1-plan-command.md
│       └── 2-auto-landing.md
├── features/                       # Single-ticket features
│   └── add-cursor-adapter.md
├── bugs/                           # Bug fixes
│   └── spawn-context-escaping.md
└── chores/                         # Maintenance, refactoring, docs
    └── update-dependencies.md
```

---

## Issue Categories

| Directory | Purpose | Examples |
|-----------|---------|----------|
| `epics/` | Large features, multiple tickets | New subsystem, major refactor |
| `features/` | Single-ticket features | Add a flag, new command |
| `bugs/` | Bug fixes | Crash fix, incorrect behavior |
| `chores/` | Maintenance | Deps update, docs, refactoring |

---

## Issue Types

### Standalone Issues

Simple tasks in a single file.

```
issues/features/add-verbose-flag.md
issues/bugs/fix-worktree-cleanup.md
issues/chores/update-clap-v5.md
```

**Use when:** Task is small enough for one session.

### Epics

Large features as directories with multiple tickets.

```
issues/epics/plan-and-execute/
├── index.md                    # Epic overview
├── 0-issue-schema.md           # First ticket
├── 1-plan-command.md           # Second ticket
└── 2-auto-landing.md           # Third ticket
```

**Use when:** Feature needs multiple sessions or parallel workers.

---

## Formats

### Standalone / Ticket

```markdown
# [Title]

**Status:** Planned | In Progress | Complete | Blocked

## Objective

One sentence: what this accomplishes.

## Implementation

1. Step-by-step guide
2. With file paths and code snippets

## Files

- `path/to/file.rs` — Description of changes

## Acceptance Criteria

- [ ] Criterion that can be verified
- [ ] Another criterion

## Verification

How to test this works.
```

### Epic Index

```markdown
# [Epic Title]

**Status:** Planned | In Progress | Complete

## Background

Why this work is needed. Context and motivation.

## Design

Key technical decisions and architecture.

## Tickets

| # | Ticket | Status |
|---|--------|--------|
| 0 | [Issue schema](./0-issue-schema.md) | Complete |
| 1 | [Plan command](./1-plan-command.md) | In Progress |
| 2 | [Auto-landing](./2-auto-landing.md) | Planned |

## Success Criteria

- [ ] Overall epic acceptance criteria
- [ ] That spans multiple tickets

## Open Questions

Unresolved decisions or unknowns.
```

---

## Status Values

| Status | Meaning |
|--------|---------|
| `Planned` | Ready to work on |
| `In Progress` | Currently being implemented |
| `Complete` | Done and verified |
| `Blocked` | Waiting on dependency or decision |

---

## Workflow

### Finding work

```bash
# List all issues
ls issues/*/

# Find planned work
grep -r "Status.*Planned" issues/
```

### Starting work

1. Find a `Planned` issue
2. Check dependencies (epic index or ticket header)
3. Update status to `In Progress`
4. Do the work
5. Update status to `Complete`

### Creating work

**Standalone:**
```bash
cp issues/_templates/standalone.md issues/features/my-feature.md
```

**Epic:**
```bash
mkdir issues/epics/my-epic
cp issues/_templates/epic-index.md issues/epics/my-epic/index.md
cp issues/_templates/ticket.md issues/epics/my-epic/0-first-task.md
```

### Completing work

- Mark status as `Complete`
- Delete the file to keep things clean, OR
- Keep it for audit trail

---

## Best Practices

- Keep tickets completable in one session
- Use 0-indexed numbering for ticket order
- Reference specific file paths
- Update status immediately
- Delete completed issues to reduce noise
