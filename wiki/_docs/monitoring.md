---
title: Monitoring
slug: monitoring
date: 2025-05-09
---

Monitor all your workers from the terminal with `jig ps`. The global watch mode (`jig ps -gw`) is the primary way to supervise agents — it runs the daemon lifecycle inline, showing a live dashboard while actively monitoring workers, nudging stuck agents, and tracking PR health.

## The dashboard

```bash
jig ps -gw
```

This starts the global live watch display — all workers across all repos, updating in real time:

```text
jig ps --watch — 4 workers  (every 2s)

WORKER              STATE    COMMITS  PR     HEALTH  ISSUE
● jwt-auth          running        2  -      -       ENG-123
● pagination        running        0  -      -       ENG-124
● test-coverage     draft          3  #42    ci      ENG-125
● error-pages       review         5  #43    ok      ENG-126

                                              [l]ogs  [q]uit
```

### Columns

| Column | Description |
|--------|-------------|
| **WORKER** | Name with tmux indicator: `●` running, `○` exited |
| **STATE** | Derived worker status from the event stream |
| **COMMITS** | Commits ahead of base branch (`*` = uncommitted changes) |
| **PR** | PR number if one exists |
| **HEALTH** | PR check results: `ok`, problem names in red, `-` if no PR |
| **ISSUE** | Linked issue reference |

### At a glance

- **Which agents are active** — `running` means tool use is flowing
- **Who's stuck** — `stalled` means silence for 5+ minutes, the daemon will nudge
- **Draft vs review** — `draft` means agent is still working; `review` means ready for human review
- **PR health** — `ci` means checks failing, `conflicts` means merge conflicts
- **Progress** — Commit count shows how far along each worker is

### Log view

Press `l` in watch mode to see daemon activity:

```text
[14:32:05] tick: 3 workers, 1 action, 1 nudge, 0 errors
[14:32:05]   myrepo/jwt-auth PR: ok
[14:32:05]   myrepo/test-coverage PR: ci, conflicts
[14:32:35] tick: 3 workers, 0 actions, 0 nudges, 0 errors
```

Press `t` or `l` again to switch back. Press `q` to quit.

## The daemon

`jig ps -gw` runs the daemon inline — the live display is a UI layer on top of the daemon's tick loop. Every 30 seconds, the daemon fetches repos, scans event logs to derive worker state, discovers PRs via GitHub, and dispatches actions (nudges, notifications, cleanup).

The daemon uses background actor threads for blocking I/O: syncing repos, querying GitHub, polling for spawnable issues, creating worktrees, pruning merged workers, and delivering nudges via tmux.

## Nudges

When agents get stuck, the daemon intervenes via `tmux send-keys`. Each nudge type has an independent counter and escalates after `max_nudges` (default 3) to a notification instead.

### Nudge types

| Type | Trigger | Action |
|------|---------|--------|
| **idle** | Worker stalled or idle, no PR | Asks for status update, pushes toward committing |
| **stuck** | Worker waiting (interactive prompt) | Sends auto-approve keystroke, then message |
| **ci** | CI failing on open PR | Lists failing checks, tells agent to fix |
| **conflict** | Merge conflicts on PR | Tells agent to rebase and resolve |
| **review** | Unresolved review comments | Tells agent to address feedback |
| **bad-commits** | Non-conventional commits | Lists bad commits, tells agent to reword |

### Escalation

After `max_nudges` of the same type, the daemon stops nudging and fires a notification — alerting you that the worker needs human attention. This prevents infinite loops where an agent keeps failing at the same thing.

### Draft-only nudging

Nudges only fire for **draft PRs**. Once a PR is marked ready for review, the daemon backs off — the human is in control. Health problems still appear in the HEALTH column for visibility.

## Auto-cleanup

When a PR is merged or closed, the daemon automatically:

- Kills the tmux window
- Removes the worktree and event logs
- Emits a `Terminal` event

Pruning skips worktrees with uncommitted changes (logs a warning). On startup, the daemon scans for PRs merged/closed while it was offline and prunes stale workers.

Configure cleanup behavior:

```toml
[github]
auto_cleanup_merged = true       # default: kill workers when PR merges
auto_cleanup_closed = false      # kill workers when PR closed without merge
```

## Configuration

### Health thresholds

```toml
# ~/.config/jig/config.toml (global) or jig.toml (per-repo)

[health]
silence_threshold_seconds = 300  # 5 minutes before "stalled"
max_nudges = 3                   # per nudge type before escalation
```

### Per-type nudge config

```toml
# jig.toml

[health.nudge.idle]
max = 5
cooldown_seconds = 600

[health.nudge.ci]
max = 2
cooldown_seconds = 180
```

Available types: `idle`, `stuck`, `ci`, `conflict`, `review`, `bad_commits`. Resolution: per-type repo → repo defaults → global → hardcoded defaults.

### Custom nudge templates

Override any built-in template by placing files in `.jig/templates/`:

```text
.jig/templates/
├── nudge-idle.hbs
├── nudge-ci.hbs
└── spawn-preamble.hbs
```

Templates use Handlebars and always receive `nudge_count`, `max_nudges`, and `is_final_nudge`.

## The event system

Every worker has a JSONL event log at `~/.config/jig/state/events/<repo>-<worker>/events.jsonl`. Events are appended by git hooks and the daemon.

| Event | Source | Meaning |
|-------|--------|---------|
| `Spawn` | `jig spawn` | Worker created |
| `ToolUseStart` / `ToolUseEnd` | Agent hooks | Tool use activity |
| `Commit` | post-commit hook | Code committed |
| `Push` | post-commit hook | Code pushed |
| `PrOpened` | Daemon | PR discovered for branch |
| `Notification` | Agent | Hit an interactive prompt |
| `Stop` | Agent exit | Session ended |
| `Nudge` | Daemon | Nudge delivered |
| `Terminal` | Daemon | Worker cleaned up |

Worker state is derived by replaying the event stream — there's no mutable state database.

## Quick reference

```bash
jig ps                   # Status snapshot
jig ps -w                # Watch mode (current repo)
jig ps -gw               # Global watch — all repos, runs daemon
jig daemon               # Headless daemon
jig daemon --once        # Single tick
jig daemon --interval 60 # Custom interval
```
