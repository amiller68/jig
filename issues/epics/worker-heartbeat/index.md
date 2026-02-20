# Epic: Worker Heartbeat System

**Status:** Planned  
**Priority:** High  
**Category:** Features  
**Depends-On:** issues/features/global-commands.md, issues/epics/git-hooks/index.md

## Objective

Add built-in heartbeat system that periodically checks worker health, detects stuck threads, and automatically nudges or escalates issues.

## Why This Matters

Replace external monitoring scripts with native health checks:
- Detect idle workers (no commits after threshold)
- Detect stuck workers (waiting at interactive prompts)
- Auto-nudge with contextual messages
- Escalate to human after max nudges
- Integrate with git hooks for event-driven checks

## Tickets

| # | Ticket | Status | Priority |
|---|--------|--------|----------|
| 0 | Health state storage | Planned | High |
| 1 | Tmux detection | Planned | High |
| 2 | Nudge system | Planned | High |
| 3 | Git hooks integration | Planned | High |
| 4 | Watch mode | Planned | Medium |

## Design Principles

1. **Adapter-agnostic** - works with any agent (Claude, Cursor, future)
2. **Configurable** - thresholds and patterns in `jig.toml`
3. **Fast** - health checks complete in <1s
4. **Event-driven** - git hooks trigger checks automatically
5. **Escalation-aware** - max nudges before human intervention

## Dependencies

- `issues/features/global-commands.md` - multi-repo operations
- `issues/epics/git-hooks/index.md` - hook integration for metrics

## Acceptance Criteria

- [ ] Health state tracked in `.worktrees/.jig-health.json`
- [ ] Detect stuck workers (interactive prompts)
- [ ] Detect idle workers (at shell prompt, no activity)
- [ ] Auto-nudge with contextual messages
- [ ] Escalate after max nudges (default 3)
- [ ] Git hooks update metrics on commit/merge
- [ ] Watch mode for periodic checks

## References

- Tmux library: `issues/improvements/tmux-integration-library.md`
- Original spec: docs/GRINDER-ANALYSIS.md (heartbeat section)
