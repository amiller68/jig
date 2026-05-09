---
title: Hooks & Notifications
slug: hooks-and-notifications
date: 2025-05-09
---

jig uses hooks to track agent activity and notifications to alert you when workers need attention.

## Git hooks

jig installs git hooks that emit events to the worker's event log. These power the daemon's state derivation — without them, the daemon can't detect commits, pushes, or agent activity.

### Installing hooks

```bash
jig hooks init
```

This installs hooks in the current repo. `jig init` does this automatically.

### What gets installed

| Hook | Events emitted |
|------|---------------|
| `post-commit` | `Commit` — code was committed |
| `post-merge` | `Push` — code was pushed |

### Uninstalling

```bash
jig hooks uninstall
```

## Agent hooks

Agent hooks track tool use activity. When an agent starts or finishes using a tool, hooks emit `ToolUseStart` and `ToolUseEnd` events. These are what let the daemon distinguish `running` from `idle` or `stalled`.

Agent hooks also detect when the agent hits an interactive prompt (`Notification` event) — this triggers `waiting` state and a `stuck` nudge.

Hook installation is agent-framework-specific and handled by `jig init` based on the `[agent] type` in `jig.toml`.

## Notifications

When the daemon can't fix a problem by nudging, it sends you a notification.

### Triggers

- **Nudge escalation** — a nudge type exceeded `max_nudges` (default 3)
- **Worker done** — PR merged, agent completed
- **Triage failure** — triage subprocess crashed or timed out

### Configuration

```toml
# ~/.config/jig/config.toml

[notify]
exec = "notify-send 'jig' '$MESSAGE'"     # shell command
# webhook = "https://hooks.slack.com/..."  # or a webhook URL
# events = ["worker.done"]                 # filter which events trigger
```

The `exec` command receives a JSON payload on stdin with event details:

```toml
[notify]
exec = "jq -r '.reason' | terminal-notifier -title 'jig'"
```

### Webhook

For Slack or other integrations, use a webhook URL:

```toml
[notify]
webhook = "https://hooks.slack.com/services/T.../B.../xxx"
```

The webhook receives a POST with the same JSON payload.

## State files

| Path | Purpose |
|------|---------|
| `~/.config/jig/state/events/<repo>-<worker>/events.jsonl` | Per-worker event stream |
| `~/.config/jig/workers.json` | Last-known state of all workers |
| `~/.config/jig/notifications.jsonl` | Notification log |
