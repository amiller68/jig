# Daemon

The jig daemon monitors spawned workers and takes action when they need attention — nudging idle sessions, monitoring PRs, auto-spawning from issues, and cleaning up merged work.

## Do I need the daemon?

**For interactive use, no.** `jig ps --watch` runs the full orchestration loop on each refresh. If you have a terminal open watching your workers, you're covered.

**For unattended use, yes.** If you spawn workers and walk away, the daemon ensures they get nudged when stuck and you get notified when something needs attention.

## How it works

Every tick (default 30s), the daemon:

1. **Drains actor responses** — Collects results from background actors (GitHub, nudge, prune, spawn, sync, review, triage)
2. **Syncs repos** — Background `git fetch` via the sync actor (includes parent branches)
3. **Updates parent worktrees** — Pulls merged child work into parent worktrees (see [Parent worktree auto-update](#parent-worktree-auto-update))
4. **Discovers workers** — Scans event logs for active workers
5. **Processes each worker:**
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
| **sync** | `jig-sync` | `git fetch` for registered repos and parent branches |
| **github** | `jig-github` | PR status, CI checks, review comments |
| **issue** | `jig-issue` | Poll for spawnable issues (file/Linear) |
| **spawn** | `jig-spawn` | Create worktrees and launch agents |
| **prune** | `jig-prune` | Remove worktrees for merged/closed PRs |
| **nudge** | `jig-nudge` | Deliver nudge messages via tmux |
| **review** | `jig-review` | Run ephemeral AI review sessions |
| **triage** | `jig-triage` | Run triage agents as direct subprocesses |

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

Per-type nudge config (`idle`, `stuck`, `ci`, `conflict`, `review`, `bad_commits`, `auto_review`, `parent_update`) can set `max` and `cooldown_seconds` independently. Resolution order: per-type repo config → repo defaults → global config → hardcoded defaults.

## PR nudges

When a worker has an open **draft** PR, the daemon monitors it for problems:

- **ci** — CI checks are failing
- **conflicts** — PR has merge conflicts
- **reviews** — PR has unresolved review comments
- **commits** — PR has non-conventional commit messages

**Draft PRs** receive nudges — the agent is still actively working and can act on them. The STATE column shows `draft` (blue).

**Non-draft PRs** do not receive nudges — they're in human review. The STATE column shows `review` (cyan). Health problems still appear in the HEALTH column for visibility.

## Auto-complete on merge

When a worker's PR merges, the daemon can automatically mark the linked issue as Complete. Enable this per-repo in `jig.toml`:

```toml
[issues]
auto_complete_on_merge = true  # default: false
```

When enabled and a PR merges, the daemon:

1. Checks whether the worker has a linked issue (`issue` field in `workers.json`).
2. Fetches the issue's current status — skips if already Complete.
3. Calls `update_status` on the issue provider to set status to Complete.
4. Logs the result. Failure is non-fatal — the worker is still cleaned up even if the status update fails.

This works for both child and parent workers in [parent-child](parent-child.md) flows: child PR merges → child issue auto-completes → wrap-up auto-spawns → parent PR merges → parent issue auto-completes.

## Automated review

When enabled, the daemon triggers an AI review agent on draft PRs whenever new commits are pushed. This automates the review-revise cycle before human review.

### Configuration

```toml
# jig.toml
[review]
enabled = true
max_rounds = 3          # max review cycles before human escalation
# model = "opus"        # optional model override for review agent
```

### How it works

1. **Trigger**: On each tick, if a worker has a draft PR and `review.enabled = true`, the daemon compares the worktree's HEAD SHA against `last_reviewed_sha` in `workers.json`. If HEAD has moved (or no review has been done yet), a review is dispatched to the review actor.

2. **Review**: The review actor runs an ephemeral AI agent that diffs the branch against the base, evaluates correctness and conventions, and writes a verdict file to `.jig/reviews/`.

3. **Drain**: On the next tick, the daemon reads the verdict:
   - **Approve** — The PR is marked ready for review (`gh pr ready`), the linked issue (if any) is transitioned to "In Review", and a `ReviewApproved` notification is emitted.
   - **ChangesRequested** — An `AutoReview` nudge is sent to the worker's tmux session, pointing it at the review findings. The worker can then address them and push new commits, triggering another review cycle.

4. **Max rounds**: If the review cycle reaches `max_rounds` without approval, the daemon emits a `NeedsIntervention` notification and stops triggering reviews.

### Comment routing

When automated review is active on a draft PR, human review comment nudges (`NudgeType::Review`) are suppressed. The review agent is the gatekeeper — human feedback comes after the PR exits draft. When review is disabled or the PR is non-draft, comment nudges behave normally.

### State tracking

The `last_reviewed_sha` field in `workers.json` persists across daemon restarts, preventing duplicate reviews on the same commit.

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

### Tool restrictions

Spawned workers are blocked from using `gh pr create` and `gh pr merge` directly — they must use `jig pr` instead. This is enforced via `--disallowedTools` and is not configurable; workers that bypass `jig pr` miss parent-branch targeting, issue linking, and other orchestration hooks.

Additional tools can be blocked per-repo:

```toml
[agent]
disallowed_tools = ["Bash(gh issue create:*)"]
```

Triage workers use a stricter allowlist: `Read`, `Glob`, `Grep`, and `Bash(jig *)` only — no code modification, no general shell access.

## Triage

The daemon can automatically triage issues in **Triage** status by running lightweight subprocess agents. Enable this per-repo:

```toml
# jig.toml
[triage]
enabled = true
timeout_seconds = 600    # max time before a triage subprocess is considered stuck (default 600)
```

### How it works

Triage runs as direct subprocesses via the triage actor — no tmux window, no worktree, no worker budget consumed. This keeps triage lightweight and avoids the overhead of a full worker lifecycle.

1. **Discovery**: Issue actor returns triageable issues (status=Triage) separately from spawnable ones
2. **Dedup**: `TriageTracker` (in-memory, on `DaemonRuntime`) filters out issues already being triaged (`is_active`)
3. **Dispatch**: Triage issues are sent to the triage actor, which runs each as a subprocess with restricted tool access (`Read`, `Glob`, `Grep`, `Bash(jig *)`)
4. **Registration**: The tracker records the issue ID, worker name, and timestamp
5. **Stuck detection**: Each tick, entries older than the repo's `timeout_seconds` emit `NeedsIntervention` and the tracker entry is cleared so a fresh triage can be dispatched on a later tick
6. **Completion**: When the triage actor reports a subprocess has finished, the tracker removes the entry

The triage workflow:
1. Triage subprocess investigates the issue, appends findings to the description (`jig issues update <id> --body "..." --append`)
2. Subprocess transitions the issue to Backlog (`jig issues status <id> --status backlog`)
3. Subprocess exits
4. Daemon clears the tracker entry

No auto-retry on failure. Failed triage requires human attention.

### Persistence

The tracker is in-memory only. On daemon restart, active triage entries are not restored — any in-flight triages at shutdown are simply re-discovered on the next issue poll if still in Triage status.

## Parent branch auto-update

When a child worker's PR is merged into its parent branch, the parent branch needs to pick up those changes. The daemon handles this automatically, whether or not a parent worktree exists.

### How it works

1. **Sync**: The sync actor fetches parent branches alongside repo base branches. Parent branch fetch failures are non-fatal (logged but skipped).
2. **Fast-forward**: After sync completes, the tick loop identifies parent branches by matching branch names from active workers. Two paths:
   - **Worktree exists**: Fast-forward the worktree via git2 checkout (original behavior). The parent worker receives a `parent_update` nudge via tmux.
   - **No worktree** (integration branch): Fast-forward the local branch ref directly using git2's reference API (`Repo::fast_forward_branch_ref`), then push to origin so other daemons/repos stay in sync. No nudge is sent (no worker to nudge).

### Data flow

When a child worker is spawned from a parent issue, the `parent_issue` and `parent_branch` fields are recorded in the spawn event. The reducer extracts these into `WorkerState`, and they persist in `WorkerEntry` (`workers.json`) across ticks.

### Safety

- Fast-forward only — if the update can't fast-forward (conflict/divergence), the daemon logs a warning and skips the update
- Bare ref updates verify ancestry via `graph_descendant_of` before advancing the ref
- Parent branch fetch failures in the sync actor are non-fatal

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
