# Coordination Protocol

## File formats

All files are Markdown with YAML frontmatter. Body is freeform markdown — real newlines, no escaping.

### Message (`agents/<to>/inbox/<ts>-<from>-<subject>.md`)
```markdown
---
from: orch
to: worker-auth
ts: 20260413T100000Z
kind: task-assign | status-request | reply | fyi
ref: T-017
---
Body here. Be concrete. State what you want and by when.
```

### Task (`tasks/open/<id>.md`)
```markdown
---
id: T-017
title: Add OAuth state validation
created_by: orch
created_at: 20260413T100000Z
assignee: null
files_expected:
  - crates/app/src/http/auth/google/callback.rs
priority: normal
---
## Goal
<one paragraph>

## Acceptance
- [ ] criterion 1

## Out of scope
- <bounds>

## Notes
<append-only log>
```

### Status (`agents/<name>/STATUS.md`) — overwrite on each heartbeat
```markdown
---
name: worker-auth
state: active | idle | blocked | gone
updated_at: 20260413T100000Z
current_task: T-017
blockers: null
---
One-line description of current activity.
```

### Role (`agents/<name>/ROLE.md`) — written once at join
```markdown
---
name: worker-auth
scope: "Auth handlers under crates/app/src/http/auth"
files_owned:
  - crates/app/src/http/auth/**
constraints:
  - Do not touch database/models
---
```

## Rules

1. **One writer per file.** `files_owned` is a contract. Never edit a file owned by another agent — send a message instead.
2. **Atomic moves, not copies.** `claim`/`release`/`close` move task files between directories; never duplicate.
3. **Append-only logs.** The `## Notes` section of a task and `broadcast/` entries are append-only.
4. **Heartbeat or go.** If you stop working, set `state: gone` and `leave`. Stale status blocks others.
5. **Real newlines.** Use actual line breaks in YAML/markdown. No `\n` escapes.
6. **UTC timestamps.** Always `date -u +%Y%m%dT%H%M%SZ`.
7. **The orchestrator does not implement.** It coordinates via tasks and messages only.
