# Daemon

The jig daemon monitors spawned workers and takes action when they need attention — nudging idle sessions, notifying on failures, and tracking state across the event-driven pipeline.

## Do I need the daemon?

**For interactive use, no.** `jig ps --watch` runs the full orchestration loop on each refresh. If you have a terminal open watching your workers, you're covered.

**For unattended use, yes.** If you spawn workers and walk away, the daemon ensures they get nudged when stuck and you get notified when something needs attention.

## How it works

Every tick (default 30s), the daemon:

1. Scans `~/.config/jig/state/events/` for worker event logs
2. Replays each worker's JSONL events to derive current state
3. Compares against previous state (stored in `~/.config/jig/state/workers.json`)
4. Dispatches actions based on state transitions:
   - **Nudge** idle/stuck workers via tmux (using templates from `~/.config/jig/templates/`)
   - **Notify** when a worker hits max nudges or fails
5. Saves updated state

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

### OpenRC (Gentoo/Alpine)

```bash
cat > ~/.local/bin/start-jig-daemon << 'EOF'
#!/bin/sh
exec /path/to/jig daemon 2>> /tmp/jig-daemon.log
EOF
chmod +x ~/.local/bin/start-jig-daemon
```

Add to your session startup (`.bash_profile`, `.xinitrc`, etc.):

```bash
nohup ~/.local/bin/start-jig-daemon &
```

### Any system

```bash
nohup jig daemon 2>> /tmp/jig-daemon.log &
```

## Configuration

The daemon reads `~/.config/jig/config.toml`:

```toml
[health]
silence_threshold_seconds = 300  # seconds before marking a worker stalled
max_nudges = 3                   # nudges before escalating to notification

[notify]
# hook = "~/.config/jig/hooks/notify.sh"  # script called on notification events
# events = ["needs_intervention", "worker_failed"]  # filter which events trigger the hook
```

## PR Nudges

When a worker has an open PR, the daemon monitors it for problems:

- **ci** — CI checks are failing
- **conflicts** — PR has merge conflicts
- **reviews** — PR has unresolved review comments
- **commits** — PR has non-conventional commit messages

**Draft PRs** receive nudges for these problems — the agent is still actively working and can act on them. The STATE column shows `draft` (blue) for these workers.

**Non-draft PRs** do not receive nudges — they're in human review. The STATE column shows `review` (cyan). Health problems still appear in the HEALTH column for visibility, but the daemon won't interrupt the agent.

## Troubleshooting

**No workers discovered:**
Events live in `~/.config/jig/state/events/<repo>-<worker>/events.jsonl`. If empty, hooks aren't installed — run `jig init <agent>` to set them up.

**Workers stuck in "spawned":**
The worker hasn't produced any events yet. Check that Claude Code hooks are installed (`ls ~/.claude/hooks/`) and git hooks are in place (`ls .git/hooks/post-commit`).

**Nudges not sending:**
The daemon sends nudges via tmux. The worker's tmux window must exist. Check with `jig ps`.
