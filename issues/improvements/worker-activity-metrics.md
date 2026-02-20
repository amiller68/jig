# Worker Activity Metrics

**Status:** Planned  
**Priority:** Medium  
**Category:** Improvements

## Objective

Track detailed activity metrics for each worker to enable smart health checks and better status reporting.

## Background

The issue-grinder tracks several useful metrics:
- Worker age (time since spawn)
- Last commit timestamp
- Commit count
- Last file modification time

These metrics help detect:
- Idle workers (at prompt, no commits)
- Stale workers (no file activity)
- Active workers (recent commits/changes)

This should be built into jig and displayed in `jig ps`.

## Acceptance Criteria

### Repo Registry Integration
- [ ] Metrics tracked per-worker in repo's state file
- [ ] `jig ps --metrics` shows current repo or `--repo <path>`
- [ ] `jig ps --metrics --all` aggregates across all registered repos

### Core Metrics
- [ ] Track per-worker:
  - `started_at` - timestamp when worker spawned
  - `last_commit_at` - timestamp of most recent commit
  - `commit_count` - number of commits on branch
  - `last_file_mod_at` - timestamp of most recent file change
  - `last_activity_at` - most recent of commit/file mod/tmux activity
- [ ] Store in worker state (persist across restarts)
- [ ] Update on each `jig health` check

### Display in `jig ps`
- [ ] Add `--metrics` flag to show activity metrics
- [ ] Columns:
  - Age (e.g., "3h", "2d")
  - Commits (count)
  - Last Activity (e.g., "15m ago", "3h ago")
  - Status (idle/active/stale)
- [ ] Color coding:
  - Green: active (activity <30m)
  - Yellow: idle (activity 30m-2h)
  - Red: stale (activity >2h)

### Smart Status Detection
- [ ] Derive worker status from metrics:
  - **Active**: recent file changes or commits
  - **Idle**: at prompt, no recent activity
  - **Stale**: no activity for >threshold
  - **Stuck**: at non-prompt, no activity
- [ ] Configurable thresholds

### Health Scoring
- [ ] Calculate health score (0-100) based on:
  - Time since last activity
  - Commit progress
  - File modification frequency
- [ ] Display in `jig ps --health`
- [ ] Alert when health drops below threshold

## Commands

```bash
# Show detailed metrics
jig ps --metrics

# Show health scores
jig ps --health

# Show only problematic workers
jig ps --unhealthy

# Export metrics as JSON
jig ps --metrics --json
```

## Implementation Notes

1. Add `ActivityMetrics` struct to `WorkerState`
2. Update metrics on each health check
3. Use `git log` for commit tracking
4. Use `find` for file modification tracking
5. Add formatting utilities for human-readable durations

## Example Output

```
WORKER                         STATUS    AGE    COMMITS  LAST ACTIVITY  HEALTH
features/desktop-sidecar       idle      8h     0        3h ago         20/100
improvements/blobs-store       active    2h     3        15m ago        95/100
chores/update-workflows        stale     6h     1        4h ago         15/100
```

## Configuration

```toml
[health.thresholds]
# Activity age before considered idle (minutes)
idleAfter = 30

# Activity age before considered stale (minutes)
staleAfter = 120

# Health score threshold for alerts
unhealthyBelow = 30
```

## Related Issues

- #TBD: Worker heartbeat system
- #TBD: Alert/notification system

## References

- Current implementation: `~/.openclaw/workspace/skills/issue-grinder/grind.sh` (lines 118-150, 251-340)
