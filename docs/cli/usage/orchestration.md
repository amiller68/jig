# Multi-Agent Orchestration

Use `jig` to orchestrate parallel Claude Code workers for large tasks.

## Quick Start

```bash
# 1. Create an integration worktree for the epic
jig -o create epic-123

# 2. Tell the orchestrating agent what to work on
#    It uses /issues to discover work and /jig to delegate
claude

# 3. Monitor and manage workers
jig ps                        # check worker status
jig attach                    # watch workers in tmux
jig review <task>             # review completed work
jig merge <task>              # merge into epic branch
```

## Recommended Workflow

### 1. Discovery with `/issues`

The orchestrator starts by understanding the work. Running `/issues` scans the `issues/` directory (or queries an external tracker like Linear via MCP) to find epics and tickets, their status, and what's ready.

### 2. Delegation with `/jig`

Once the orchestrator understands the work, it decomposes tasks and delegates them:

```bash
jig spawn auth-jwt --context "Implement JWT token generation.

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
jig ps                    # See status of all workers
jig ps -g                 # Global view — workers grouped by repo
jig attach                # Open tmux session to watch progress
jig attach auth-jwt       # Jump to a specific worker
jig review auth-jwt       # Review the diff when done
jig merge auth-jwt        # Merge into your integration branch
jig kill auth-jwt         # Stop a stuck worker
jig remove auth-jwt       # Clean up the worktree
```

## Commands Reference

| Command | Description |
|---------|-------------|
| `jig spawn <name> --context "..." --auto` | Spawn autonomous worker |
| `jig ps` | Check worker status |
| `jig ps -g` | Global view grouped by repo |
| `jig attach [name]` | Watch workers in tmux |
| `jig review <name>` | Review worker's changes |
| `jig merge <name>` | Merge into current branch |
| `jig kill <name>` | Stop a worker |
| `jig remove <name>` | Delete worktree |

## Writing Good Spawn Context

Each spawn should include focused, specific context:

- **One-line summary** of the task
- **Files to modify** (if known)
- **Specific requirements** with enough detail for autonomous work
- **Acceptance criteria** so the worker knows when it's done

## Worker States

The `STATE` column in `jig ps` reflects where each worker is in its lifecycle:

| State | Color | Meaning |
|-------|-------|---------|
| `spawned` | blue | Just created, agent hasn't started yet |
| `running` | green | Agent is actively using tools |
| `idle` | yellow | Agent stopped, sitting at shell prompt |
| `waiting` | magenta | Agent is waiting for user input |
| `stalled` | red | No activity for 5+ minutes |
| `draft` | blue | PR is open as draft — agent is nudged about CI/conflicts/reviews |
| `review` | cyan | PR is ready for human review — no nudges |
| `approved` | green | PR approved |
| `merged` | green | PR merged |
| `failed` | red | Worker failed |

### Draft vs review

Workers with **draft PRs** show `draft` and receive automatic nudges for CI failures, merge conflicts, and unresolved review comments. The agent can fix these autonomously.

Workers with **non-draft PRs** show `review` and do not receive nudges — the PR is in human review. Health problems still appear in the HEALTH column for visibility.

## Tips

- Run `/issues` first to understand the full scope before spawning
- Keep tasks independent so workers don't conflict
- Spawn 2-4 workers at a time, merge as they complete
- Include specific file paths when you know them
- Set clear acceptance criteria
- Use `jig attach` to monitor progress
