# Event-Driven Pipeline Roadmap

Execution order for all issues in this milestone. Each row is a discrete unit of work.

## Phase 0: Global Infrastructure

Foundation for user-level configuration and cross-repo state.

| # | Issue | Status | Blocks | Notes |
|---|-------|--------|--------|-------|
| 0.1 | `features/global-commands.md` | Planned | 0.2 | Repo registry, GlobalContext |
| 0.2 | `epics/event-system/0-global-state.md` | New | 1.1 | `~/.config/jig/state/` structure |

## Phase 1: Git Hooks

Install hooks that can emit events on git operations.

| # | Issue | Status | Blocks | Notes |
|---|-------|--------|--------|-------|
| 1.1 | `epics/git-hooks/0-hook-wrapper-pattern.md` | Planned | 1.2 | Templates, marker system |
| 1.2 | `epics/git-hooks/1-registry-storage.md` | Planned | 1.3 | Track installed hooks |
| 1.3 | `epics/git-hooks/2-idempotent-init.md` | Planned | 1.4 | Safe install/reinstall |
| 1.4 | `epics/git-hooks/3-hook-handlers.md` | Planned | 2.2 | `jig hooks <name>` commands |
| 1.5 | `epics/git-hooks/4-uninstall-rollback.md` | Planned | — | Cleanup, restore backups |

## Phase 2: Event System

Structured event logging and state derivation.

| # | Issue | Status | Blocks | Notes |
|---|-------|--------|--------|-------|
| 2.1 | `epics/event-system/1-event-log-format.md` | New | 2.2 | JSONL schema, worker events |
| 2.2 | `epics/event-system/2-claude-hooks.md` | New | 2.3 | User-level hooks → event log |
| 2.3 | `epics/event-system/3-worker-status-states.md` | New | 2.4 | Add WaitingInput, Stalled |
| 2.4 | `epics/event-system/4-state-derivation.md` | New | 3.1 | Event log → WorkerStatus |

## Phase 3: Control Plane

Actions triggered by state changes.

| # | Issue | Status | Blocks | Notes |
|---|-------|--------|--------|-------|
| 3.1 | `features/smart-context-injection.md` | Planned | 3.3 | Template system for prompts |
| 3.2 | `improvements/tmux-integration-library.md` | Planned | 3.3 | Refactor: controller only |
| 3.3 | `epics/worker-heartbeat/2-nudge-system.md` | Planned | 3.4 | Uses templates, event-driven triggers |
| 3.4 | `epics/event-system/5-action-dispatch.md` | New | — | State → controller actions |

## Phase 4: Notifications

External integration for alerts.

| # | Issue | Status | Blocks | Notes |
|---|-------|--------|--------|-------|
| 4.1 | `epics/event-system/6-notification-queue.md` | New | 4.2 | notifications.jsonl |
| 4.2 | `epics/event-system/7-notification-hooks.md` | New | — | exec/webhook on events |

## Phase 5: GitHub Integration

GitHub events feed the same pipeline.

| # | Issue | Status | Blocks | Notes |
|---|-------|--------|--------|-------|
| 5.1 | `epics/github-integration/0-octorust-client.md` | Planned | 5.2 | API client, caching |
| 5.2 | `epics/github-integration/1-ci-status-detection.md` | Planned | 5.3 | CI as events |
| 5.3 | `epics/github-integration/2-conflict-review-detection.md` | Planned | — | Conflicts, reviews as events |

## Superseded Issues

These issues are replaced or reframed by this milestone:

| Issue | Disposition |
|-------|-------------|
| `epics/worker-heartbeat/0-health-state-storage.md` | **Superseded** by `event-system/0-global-state.md` |
| `epics/worker-heartbeat/1-tmux-detection.md` | **Archive** — replaced by event log |
| `epics/worker-heartbeat/3-git-hooks-integration.md` | **Merged** into `git-hooks/3-hook-handlers.md` |
| `epics/worker-heartbeat/4-watch-mode.md` | **Superseded** by `event-system/4-state-derivation.md` |
| `improvements/worker-activity-metrics.md` | **Superseded** — metrics derived from events |

## Dependency Graph

```
global-commands ──┬──▶ global-state ──▶ event-log-format ──┐
                  │                                         │
git-hooks (0-4) ──┴──────────────────────────────────────▶ hook-handlers
                                                            │
                                                            ▼
                    claude-hooks ──▶ worker-status-states ──▶ state-derivation
                                                                     │
                    smart-context-injection ◀────────────────────────┤
                                                                     │
                    tmux-controller ◀────────────────────────────────┤
                                                                     │
                    nudge-system ◀───── smart-context-injection ─────┤
                            │                                        │
                    action-dispatch ◀────────────────────────────────┘
                           │
                           ▼
                    notification-queue ──▶ notification-hooks
                           │
                           ▼
                    github-integration (0-2)
```
