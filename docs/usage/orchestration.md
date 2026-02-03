# Multi-Agent Orchestration

Use `scribe` to orchestrate parallel Claude Code workers for large tasks.

## Quick Start

```bash
# 1. Create an integration worktree for the epic
scribe -o create epic-123

# 2. Tell the orchestrating agent what to work on
#    It uses /issues to discover work and /scribe to delegate
claude

# 3. Monitor and manage workers
scribe ps                        # check worker status
scribe attach                    # watch workers in tmux
scribe review <task>             # review completed work
scribe merge <task>              # merge into epic branch
```

## Recommended Workflow

### 1. Discovery with `/issues`

The orchestrator starts by understanding the work. Running `/issues` scans the `issues/` directory (or queries an external tracker like Linear via MCP) to find epics and tickets, their status, and what's ready.

### 2. Delegation with `/scribe`

Once the orchestrator understands the work, it decomposes tasks and delegates them:

```bash
scribe spawn auth-jwt --context "Implement JWT token generation.

Files to modify:
- src/auth/tokens.ts
- src/auth/config.ts

Requirements:
- Generate and validate JWT tokens
- Support refresh tokens
- Add unit tests

Acceptance criteria:
- All tests pass
- Token flow works end-to-end" --auto
```

Each spawned worker gets its own worktree and runs autonomously.

### 3. Monitor and Merge

```bash
scribe ps                    # See status of all workers
scribe attach                # Open tmux session to watch progress
scribe attach auth-jwt       # Jump to a specific worker
scribe review auth-jwt       # Review the diff when done
scribe merge auth-jwt        # Merge into your integration branch
scribe kill auth-jwt         # Stop a stuck worker
scribe remove auth-jwt       # Clean up the worktree
```

## Commands Reference

| Command | Description |
|---------|-------------|
| `scribe spawn <name> --context "..." --auto` | Spawn autonomous worker |
| `scribe ps` | Check worker status |
| `scribe attach [name]` | Watch workers in tmux |
| `scribe review <name>` | Review worker's changes |
| `scribe merge <name>` | Merge into current branch |
| `scribe kill <name>` | Stop a worker |
| `scribe remove <name>` | Delete worktree |

## Writing Good Spawn Context

Each spawn should include focused, specific context:

- **One-line summary** of the task
- **Files to modify** (if known)
- **Specific requirements** with enough detail for autonomous work
- **Acceptance criteria** so the worker knows when it's done

## Tips

- Run `/issues` first to understand the full scope before spawning
- Keep tasks independent so workers don't conflict
- Spawn 2-4 workers at a time, merge as they complete
- Include specific file paths when you know them
- Set clear acceptance criteria
- Use `scribe attach` to monitor progress
