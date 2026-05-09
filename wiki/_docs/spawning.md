---
title: Spawning
slug: spawning
date: 2025-05-09
---

`jig spawn` creates a worktree, opens a tmux window, and starts an agent session — all in one command. Unlike `jig create`, spawned workers are tracked by the daemon for monitoring, nudging, and cleanup.

## Basic usage

```bash
# With free-text context
jig spawn feature-auth --context "Implement JWT authentication"
# From an issue
jig spawn feature-auth --issue ENG-123
# Multiple workers in parallel
jig spawn jwt-auth --issue ENG-123jig spawn pagination --issue ENG-124jig spawn test-coverage --issue ENG-125```

## What happens under the hood

When you spawn a worker:

1. Creates the worktree and branch
2. Records the worker in orchestrator state
3. Emits a `Spawn` event to the worker's event log
4. Opens a tmux window in a `jig-<repo>` session
5. Sends the agent command with a structured preamble

The preamble tells the agent it's autonomous, that a daemon is watching, and what its goal is. It's a Handlebars template (`spawn-preamble`) that can be customized per-repo.

## Writing good spawn context

Each spawn should include:

- **One-line summary** of the task
- **Files to modify** (if known)
- **Specific requirements** with enough detail for autonomous work
- **Acceptance criteria** so the worker knows when it's done

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
- Token flow works end-to-end"```

## Spawning from issues

When you pass `--issue ENG-123`, jig fetches the issue and passes the full description as the agent's working context:

```bash
jig spawn auth-jwt --issue ENG-123```

The agent sees the issue title and body as markdown. This works with both Linear and file-based issues.

### Auto-spawn

The daemon can automatically spawn workers from your issue board. See [Issues & Linear](/docs/issues/) for details.

## Worker states

The `STATE` column in `jig ps` reflects where each worker is:

| State | Meaning |
|-------|---------|
| `spawned` | Just created, agent hasn't started yet |
| `running` | Agent is actively using tools |
| `idle` | Agent stopped, sitting at shell prompt |
| `waiting` | Agent is waiting for user input |
| `stalled` | No activity for 5+ minutes |
| `draft` | PR is open as draft — daemon nudges about CI/conflicts/reviews |
| `review` | PR is ready for human review — no nudges |
| `approved` | PR approved |
| `merged` | PR merged |
| `failed` | Worker failed |

State is derived by replaying the event stream — there's no mutable state database. The silence check is key: if no events arrive for `silence_threshold_seconds` (default 300s), the worker transitions to `stalled`.

## Attaching to workers

```bash
jig attach jwt-auth          # Attach to a specific worker's tmux session
jig attach                   # Attach to the tmux session group
```

You're watching the agent work in real time. Read its output, see its tool calls, intervene if needed. Detach with `Ctrl-b d`.

## Managing workers

```bash
jig ps                       # Status dashboard
jig kill jwt-auth            # Kill a specific worker
jig kill -a                  # Kill all workers
jig remove jwt-auth          # Clean up the worktree
jig nuke                     # Kill all workers and clear state
```

## Tips

- Keep tasks independent — agents working on overlapping files create merge conflicts
- Start small — spawn 2 workers your first time, scale up as you build confidence
- Include file paths when you know them
- Set clear acceptance criteria
