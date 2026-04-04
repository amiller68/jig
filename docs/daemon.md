# Daemon

The jig daemon monitors spawned workers and takes action when they need attention — nudging idle sessions, monitoring PRs, auto-spawning from issues, and cleaning up merged work.

## Do I need the daemon?

**For interactive use, no.** `jig ps --watch` runs the full orchestration loop on each refresh. If you have a terminal open watching your workers, you're covered.

**For unattended use, yes.** If you spawn workers and walk away, the daemon ensures they get nudged when stuck and you get notified when something needs attention.

## How it works

Every tick (default 30s), the daemon:

1. **Drains actor responses** — Collects results from background actors (GitHub, nudge, prune, spawn, sync)
2. **Syncs repos** — Background `git fetch` via the sync actor
3. **Discovers workers** — Scans event logs for active workers
4. **Processes each worker:**
   - Reads JSONL event log and derives current state
   - Discovers PRs via GitHub actor (non-blocking)
   - Compares against previous state
   - Dispatches actions (nudge, notify, cleanup)
   - Checks PR lifecycle (CI, conflicts, reviews, commits)
5. **Executes actions** — Nudges are dispatched to the nudge actor (async), notifications sent inline
6. **Saves state** — Writes `workers.json`
7. **Triggers auto-spawn** — Polls for eligible issues if configured

### Actor architecture

The daemon offloads blocking I/O to background threads (actors) so the tick loop stays responsive:

| Actor | Thread name | Purpose |
|-------|------------|---------|
| **sync** | `jig-sync` | `git fetch` for registered repos |
| **github** | `jig-github` | PR status, CI checks, review comments |
| **issue** | `jig-issue` | Poll for spawnable issues (file/Linear) |
| **spawn** | `jig-spawn` | Create worktrees and launch agents |
| **prune** | `jig-prune` | Remove worktrees for merged/closed PRs |
| **nudge** | `jig-nudge` | Deliver nudge messages via tmux |

Each actor uses `flume` channels for non-blocking communication with the tick thread. The nudge actor is particularly important — it prevents `tmux send-keys` from blocking the tick thread when a pane can't accept input.

### Tmux timeout

All tmux subprocess calls have a 5-second timeout. If a `tmux` command hangs (e.g., `send-keys` to a stuck pane), the child process is killed and reaped. This prevents the daemon from becoming unresponsive.

## Running manually

```bash
# Run in foreground (Ctrl+C to stop)
jig daemon

# Custom poll interval
jig daemon --interval 10

# Single pass (useful for cron or testing)
jig daemon --once
```

## Running as a background service

### launchd (macOS)

```bash
cat > ~/Library/LaunchAgents/org.krondor.jig.daemon.plist << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>org.krondor.jig.daemon</string>
  <key>ProgramArguments</key>
  <array>
    <string>/path/to/jig</string>
    <string>daemon</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardErrorPath</key>
  <string>/tmp/jig-daemon.log</string>
</dict>
</plist>
EOF

launchctl load -w ~/Library/LaunchAgents/org.krondor.jig.daemon.plist
```

To stop: `launchctl unload ~/Library/LaunchAgents/org.krondor.jig.daemon.plist`

### systemd (Linux)

```bash
mkdir -p ~/.config/systemd/user

cat > ~/.config/systemd/user/jig-daemon.service << 'EOF'
[Unit]
Description=jig daemon

[Service]
ExecStart=/path/to/jig daemon
Restart=on-failure

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload
systemctl --user enable --now jig-daemon
```

Logs: `journalctl --user -u jig-daemon -f`

To stop: `systemctl --user disable --now jig-daemon`

### Any system

```bash
nohup jig daemon 2>> /tmp/jig-daemon.log &
```

## Configuration

### Global config (`~/.config/jig/config.toml`)

```toml
[health]
silence_threshold_seconds = 300  # seconds before marking a worker stalled
max_nudges = 3                   # nudges per type before escalating to notification

[github]
auto_cleanup_merged = true       # clean up workers when PR merges (default)
auto_cleanup_closed = false      # clean up workers when PR closed without merge

[notify]
# hook = "~/.config/jig/hooks/notify.sh"  # script called on notification events
# events = ["needs_intervention", "worker_failed"]  # filter which events trigger the hook
```

### Per-repo config (`jig.toml`)

Repos can override health and nudge settings:

```toml
[health]
silence_threshold_seconds = 600  # longer threshold for slow repos

[health.nudge.idle]
max = 5               # more patience for idle workers
cooldown_seconds = 600 # wait longer between nudges

[health.nudge.ci]
max = 2
cooldown_seconds = 180
```

Per-type nudge config (`idle`, `stuck`, `ci`, `conflict`, `review`, `bad_commits`) can set `max` and `cooldown_seconds` independently. Resolution order: per-type repo config → repo defaults → global config → hardcoded defaults.

## PR nudges

When a worker has an open **draft** PR, the daemon monitors it for problems:

- **ci** — CI checks are failing
- **conflicts** — PR has merge conflicts
- **reviews** — PR has unresolved review comments
- **commits** — PR has non-conventional commit messages

**Draft PRs** receive nudges — the agent is still actively working and can act on them. The STATE column shows `draft` (blue).

**Non-draft PRs** do not receive nudges — they're in human review. The STATE column shows `review` (cyan). Health problems still appear in the HEALTH column for visibility.

## Auto-spawn

The daemon can automatically spawn workers for eligible issues:

```toml
# jig.toml
[spawn]
max_concurrent_workers = 3
auto_spawn_interval = 120    # seconds between issue polls

[issues]
provider = "file"            # or "linear"
auto_spawn_labels = ["auto"] # only spawn issues with these labels
# auto_spawn_labels = []     # spawn ALL planned issues
# (omit auto_spawn_labels to disable auto-spawn)
```

The `auto_spawn_labels` field in `[issues]` controls auto-spawning:

- **Absent** (default): auto-spawn is disabled
- **`[]`** (empty): spawn all planned issues with satisfied dependencies
- **`["x", "y"]`**: spawn only issues carrying all listed labels

The issue actor polls at the configured interval and the spawn actor creates worktrees + launches agents for eligible issues (status: planned, has required labels, dependencies satisfied).

## Triage verification

When a worker with a linked issue exits (tmux window gone), the daemon checks whether the issue is still in **Triage** status. If so, the triage worker failed silently — the daemon emits a `NeedsIntervention` notification so a human can investigate.

This check only fires once on the transition (not repeatedly each tick). If the issue has moved to **Backlog** or any other status, the triage is considered successful and no notification is emitted.

The triage workflow:
1. Triage worker spawns, investigates the issue, appends findings to the description (`jig issues update <id> --body "..." --append`)
2. Worker transitions the issue to Backlog (`jig issues status <id> --status backlog`)
3. Worker exits
4. Daemon detects exit, checks issue status — Backlog means success, Triage means failure

No auto-retry on failure. Failed triage requires human attention.

## Auto-pruning

The daemon automatically cleans up worktrees when their PRs are merged or closed.

**What triggers pruning:**
- A worker's PR is merged and `auto_cleanup_merged` is enabled (default)
- A worker's PR is closed without merge and `auto_cleanup_closed` is enabled

**What gets cleaned up:**
- The git worktree (`git worktree remove` — no `--force`)
- Event log directory under `~/.config/jig/state/events/`
- Worker entry in `workers.json`

**Safety:**
- Uses `git worktree remove` without `--force` — fails gracefully if the worktree has uncommitted changes
- The tmux window is killed before worktree removal
- A `Terminal` event is emitted so the worker won't be re-processed

**Recovery path:**
On each tick the daemon scans its GitHub cache for merged/closed PRs that still have worktrees on disk. This catches PRs that were merged while the daemon was off.

## Troubleshooting

**No workers discovered:**
Events live in `~/.config/jig/state/events/<repo>-<worker>/events.jsonl`. If empty, hooks aren't installed — run `jig init <agent>` to set them up.

**Workers stuck in "spawned":**
The worker hasn't produced any events yet. Check that Claude Code hooks are installed (`ls ~/.claude/hooks/`) and git hooks are in place (`ls .git/hooks/post-commit`).

**Nudges not sending:**
The daemon sends nudges via tmux through the nudge actor. The worker's tmux window must exist and the pane must be running a command (not at a shell prompt). Check with `jig ps`. If tmux calls are timing out, you'll see warnings in the log view.

**`jig ps -w` unresponsive:**
All tmux calls now have a 5-second timeout and nudge delivery runs on a background thread. If you still see hangs, check for tmux server issues (`tmux list-sessions`).
