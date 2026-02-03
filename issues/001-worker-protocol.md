# Worker Protocol

## Description

Implement a per-worktree worker protocol for better status tracking and task communication between the orchestrator and spawned workers.

## Current State

Currently, worker status is tracked centrally in `.worktrees/.wt-state.json` and status is inferred from tmux process state. This doesn't allow workers to communicate back to the orchestrator.

## Proposed Design

### Per-Worktree Files

Each spawned worktree would have:

```
.worktrees/<name>/
├── .scribe/
│   ├── task.md         # Task context written by orchestrator
│   └── status.json     # Status written by worker
```

### task.md

Written by orchestrator when spawning:

```markdown
# Task: Add JWT authentication

## Context
Implement JWT token generation for the auth module.

## Files to Modify
- src/auth/tokens.ts
- src/auth/config.ts

## Requirements
- Generate and validate JWT tokens
- Support refresh tokens
- Add unit tests

## Acceptance Criteria
- All tests pass
- Token flow works end-to-end
```

### status.json

Written by worker to communicate state:

```json
{
  "status": "working",
  "updated_at": "2024-01-15T10:30:00Z",
  "message": "Implementing token validation",
  "progress": {
    "files_modified": ["src/auth/tokens.ts"],
    "tests_passing": true
  }
}
```

### Status Values

| Status | Description |
|--------|-------------|
| `working` | Worker is actively working on the task |
| `blocked` | Worker encountered an issue requiring human input |
| `question` | Worker has a clarifying question |
| `done` | Worker completed the task |

## Implementation

1. Update `scribe spawn` to write `task.md` from `--context`
2. Update `scribe ps` to read `status.json` for rich status display
3. Create `/scribe` skill instructions for workers to update `status.json`
4. Add `scribe status <name>` for detailed worker status

## Benefits

- Workers can communicate progress and blockers
- Orchestrator gets richer status information
- Task context persists with the worktree
- Enables future features like automatic retries, escalation
