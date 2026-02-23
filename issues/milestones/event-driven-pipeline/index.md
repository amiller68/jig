# Milestone: Event-Driven Pipeline

**Status:** Planned
**Priority:** High

## Vision

Replace polling-based agent observation (tmux scraping, heuristic pattern matching) with an event-driven architecture where agents emit structured events and jig reacts deterministically.

**Core insight:** Instead of asking "what is the agent doing?" (inference), ask "what events have happened?" (observation). This inverts control — agents tell jig what's happening through hooks, git commits, and state files.

## Architecture

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│  Claude Hooks   │────▶│   Event Log      │────▶│  State Daemon   │
│  (emit events)  │     │  (append-only)   │     │  (derive state) │
└─────────────────┘     └──────────────────┘     └────────┬────────┘
                                                          │
                        ┌──────────────────┐              │
                        │  TmuxController  │◀─────────────┤
                        │  (spawn, nudge,  │              │
                        │   kill)          │              │
                        └──────────────────┘              │
                                                          ▼
                                                 ┌─────────────────┐
                                                 │  Notification   │
                                                 │  Queue + State  │
                                                 └────────┬────────┘
                                                          │
                        ┌─────────────────────────────────┼─────────────────────────────────┐
                        │                                 │                                 │
                        ▼                                 ▼                                 ▼
               ┌─────────────────┐              ┌─────────────────┐              ┌─────────────────┐
               │ External agents │              │  Notification   │              │  Direct exec/   │
               │ poll state file │              │  hooks trigger  │              │  webhook push   │
               └─────────────────┘              └─────────────────┘              └─────────────────┘
```

### Event Sources

- **Claude Code hooks** — `PreToolUse`, `PostToolUse`, `Notification`, `Stop`
- **Git hooks** — `post-commit`, `post-merge`, `pre-commit`
- **GitHub API** — CI status, PR state, review comments

### State Derivation

| State | Trigger |
|-------|---------|
| `Running` | Tool use events flowing |
| `Idle` | Stop hook fired, at shell prompt |
| `WaitingInput` | Notification hook fired |
| `Stalled` | Silence threshold exceeded (fallback) |
| `WaitingReview` | PR opened, awaiting human |

### Global State Location

Hooks and notifications configured outside VCS at user level:

```
~/.config/jig/
├── config.toml              # Global settings (notify, thresholds)
├── repos.json               # Known repositories
├── hooks/                   # Global hook scripts
└── state/
    ├── notifications.jsonl  # Aggregated notifications (all repos)
    └── workers.json         # Aggregated worker state (all repos)

~/.claude/hooks/             # Claude Code hooks (user-level)
├── Notification.sh          # → writes to ~/.config/jig/state/
└── Stop.sh
```

## Key Design Decisions

### "Never block" agent constraint

System prompt instructs agents:
> Never ask for clarification. If blocked, commit with `BLOCKED:` prefix and exit.

Converts observability problem (PTY blocked on stdin) into git commit problem (detect `BLOCKED:` prefix).

### Silence threshold as fallback

If `last_event_timestamp > N seconds` AND process running AND no new commits → `Stalled` state. Heuristic, but catches edge cases where hooks fail.

### Adapter-agnostic events

Event log format is common interface. Adapters translate framework-specific signals:
- **Claude Code:** hooks → JSONL
- **Aider:** stdout parsing + git monitoring → JSONL
- **Others:** PTY monitoring + silence heuristics → JSONL

### Notification consumption patterns

1. **External agents/processes** poll state file or tail notification queue
2. **Notification hooks** exec user scripts on events
3. **Webhook push** POSTs to configured endpoint

jig writes structured data. Alert logic and delivery are externalized.

## Success Criteria

- [ ] Agents emit structured events via hooks
- [ ] State derived from event log, not inferred from tmux
- [ ] `WaitingInput` and `Stalled` states trigger appropriate actions
- [ ] Silence threshold catches hook failures
- [ ] GitHub events feed same pipeline
- [ ] Tmux used only for control (spawn, input, kill), not observation
- [ ] Notification queue enables external process polling/tailing
- [ ] Notification hooks can trigger alerts directly

## Open Questions

1. Event log rotation/cleanup policy?
2. Should state daemon be separate process or integrated into `jig` CLI?
3. How to handle agent restarts mid-task (resume from event log)?
4. Notification queue cleanup — rotation vs. consumer marking "seen"?

## Roadmap

See [roadmap.md](./roadmap.md) for execution order of all issues.

## References

- Original analysis: `wiki/appendix/autonomous-issues.md`
- Git hooks epic: `epics/git-hooks/index.md`
- Worker heartbeat epic: `epics/worker-heartbeat/index.md`
- GitHub integration epic: `epics/github-integration/index.md`
