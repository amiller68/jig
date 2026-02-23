# Epic: Event System

**Status:** Planned
**Priority:** High
**Category:** Features
**Milestone:** milestones/event-driven-pipeline/

## Objective

Implement the core event system: global state storage, event log format, Claude Code hooks, and state derivation from events.

## Background

This epic provides the central nervous system for the event-driven pipeline. Instead of observing agents through tmux scraping, agents emit structured events that jig processes deterministically.

## Tickets

| # | Ticket | Status | Priority |
|---|--------|--------|----------|
| 0 | [Global state structure](./0-global-state.md) | Planned | High |
| 1 | [Event log format](./1-event-log-format.md) | Planned | High |
| 2 | [Claude Code hooks](./2-claude-hooks.md) | Planned | High |
| 3 | [Worker status states](./3-worker-status-states.md) | Planned | High |
| 4 | [State derivation](./4-state-derivation.md) | Planned | High |
| 5 | [Action dispatch](./5-action-dispatch.md) | Planned | Medium |
| 6 | [Notification queue](./6-notification-queue.md) | Planned | Medium |
| 7 | [Notification hooks](./7-notification-hooks.md) | Planned | Medium |

## Dependencies

- `features/global-commands.md` — repo registry, GlobalContext
- `epics/git-hooks/` — hook installation pattern

## Acceptance Criteria

- [ ] Global state directory at `~/.config/jig/state/`
- [ ] Event log format defined and documented
- [ ] Claude Code hooks emit events to global state
- [ ] WorkerStatus includes `WaitingInput` and `Stalled`
- [ ] State derived from event log, not tmux scraping
- [ ] Notifications queryable by external processes
- [ ] Notification hooks can trigger alerts
