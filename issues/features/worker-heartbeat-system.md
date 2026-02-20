# Worker Heartbeat System

**Status:** Planned  
**Priority:** High  
**Category:** Features

## Objective

Add a built-in heartbeat system to `jig` that periodically checks worker health, detects stuck threads, and automatically nudges or escalates issues without requiring external scripts.

## Background

Currently, the issue-grinder script (external to jig) does all the health monitoring:
- Scrapes tmux output to detect stuck prompts
- Tracks worker age, commit activity, file modifications
- Nudges idle workers via tmux send-keys
- Escalates to human after max nudges

This should be native to jig, not bolted on externally.

## Acceptance Criteria

### Repo Registry Integration
- [ ] Operate on all registered repos (via `jig register <path>`)
- [ ] Support `--repo <path>` to target specific repo
- [ ] Default: check all registered repos in single run
- [ ] Store per-repo health settings in repo's `jig.toml`

### Core Heartbeat
- [ ] `jig health --watch` command that runs periodic checks (default: every 15min)
- [ ] `jig health` (single run) - check all registered repos once
- [ ] Track per-worker metrics internally:
  - Worker start time
  - Last commit timestamp
  - Last file modification time
  - Commit count
  - Nudge count and history
- [ ] Persist state in repo's `.worktrees/.jig-state.json` (per-repo, not global)

### Stuck Detection
- [ ] Detect workers stuck at interactive prompts by scraping tmux output
- [ ] Patterns to detect:
  - "Would you like to proceed"
  - "ctrl-g to edit"
  - Multiple choice approval menus
- [ ] Configurable stuck patterns via jig config

### Idle Detection
- [ ] Detect idle workers (at shell prompt with no activity)
- [ ] Thresholds (configurable):
  - No commits after 3h (new workers)
  - No commits in 6h (existing workers)
  - No file changes in 2h (any worker)

### Auto-Nudge
- [ ] `jig nudge <worker>` command to send contextual message via tmux
- [ ] Automatic nudging for stuck/idle workers
- [ ] Smart nudge messages based on context:
  - Has uncommitted changes? → "commit and create PR"
  - Clean worktree? → "status update or ask for help"
  - Stuck at prompt? → auto-approve or ask for confirmation
- [ ] Max nudge limit (default: 3) before human escalation

### Auto-Approval
- [ ] Configurable auto-approval for known safe prompts (per-repo in `jig.toml`)
- [ ] Global fallback in `~/.config/jig/config`
- [ ] Example `jig.toml`:
  ```toml
  [health]
  autoApprove = ["Would you like to proceed", "ctrl-g to edit"]
  maxNudges = 3
  ```
- [ ] Safety: never auto-approve destructive operations

### Alerts
- [ ] Alert when worker hits max nudges (needs human)
- [ ] Alert when worker has been stuck/idle for >threshold
- [ ] Integration with existing alert system (stdout for now, webhook/notify later)

## Implementation Notes

**Phase 0: Repo Registry (prerequisite)**
1. Implement `jig register <path>` to track repos
2. Store registry in `~/.config/jig/repos` or global config
3. `jig repos list` to show registered repos
4. Health system iterates over registered repos

**Phase 1: Core Infrastructure**
1. Add `WorkerMetrics` struct to jig-core
2. Persist metrics in per-repo state (`.worktrees/.jig-state.json`)
3. Basic `jig health` command that checks all registered repos
4. Support `--repo <path>` to target specific repo

**Phase 2: Detection**
1. Tmux scraping utilities (reuse from grinder)
2. Stuck/idle detection logic
3. Activity tracking (commits, files, time)
4. Per-repo thresholds from `jig.toml`

**Phase 3: Auto-Nudge**
1. `jig nudge <worker>` command
2. Automatic nudging on `jig health --watch`
3. Escalation to human after max nudges (repo-configurable)

**Phase 4: Integration**
1. Make `jig spawn --watch` use heartbeat system
2. Optional: `jig daemon` that runs heartbeat in background across all repos

## Related Issues

- #TBD: GitHub integration (PR checks, CI, reviews)
- #TBD: Worker lifecycle management
- #TBD: Alert/notification system

## References

- Current implementation: `~/.openclaw/workspace/skills/issue-grinder/grind.sh` (lines 85-330)
- Nudge logic: lines 58-83
- Activity tracking: lines 118-150
