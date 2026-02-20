# Worker Activity Metrics

**Status:** Planned  
**Priority:** Medium  
**Category:** Improvements  
**Depends-On:** issues/features/global-commands.md, issues/features/worker-heartbeat-system.md

## Objective

Track detailed activity metrics for each worker and display them in `jig ps` with health scoring. Enable smart health checks, idle detection, and progress monitoring.

## Architecture

### Metrics Storage

**Extends `.worktrees/.jig-health.json`:**

```json
{
  "workers": {
    "features/auth": {
      "started_at": 1708358400,
      "last_commit_at": 1708362000,
      "last_commit_hash": "a3f2d1c",
      "commit_count": 3,
      "last_file_mod_at": 1708363200,
      "last_activity_at": 1708363200,
      "last_tmux_check_at": 1708363100,
      "at_prompt": true,
      "health_score": 85,
      "metrics": {
        "commits_per_hour": 0.75,
        "avg_commit_interval_minutes": 80,
        "total_lines_changed": 247,
        "files_changed_count": 8
      }
    }
  }
}
```

### Metrics Collection

**Git-based metrics:**
```rust
fn collect_git_metrics(worktree_path: &Path) -> Result<GitMetrics> {
    let repo = git2::Repository::open(worktree_path)?;
    
    // Last commit timestamp
    let head = repo.head()?.peel_to_commit()?;
    let last_commit_at = head.time().seconds();
    let last_commit_hash = head.id().to_string();
    
    // Commit count on branch
    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;
    revwalk.hide_ref("refs/remotes/origin/main")?;
    let commit_count = revwalk.count();
    
    // Lines changed
    let main = repo.find_reference("refs/remotes/origin/main")?.peel_to_commit()?;
    let diff = repo.diff_tree_to_tree(
        Some(&main.tree()?),
        Some(&head.tree()?),
        None
    )?;
    
    let stats = diff.stats()?;
    let total_lines_changed = stats.insertions() + stats.deletions();
    let files_changed_count = stats.files_changed();
    
    Ok(GitMetrics {
        last_commit_at,
        last_commit_hash,
        commit_count,
        total_lines_changed,
        files_changed_count,
    })
}
```

**File system metrics:**
```rust
fn collect_fs_metrics(worktree_path: &Path) -> Result<FsMetrics> {
    let mut last_mod = 0i64;
    
    for entry in WalkDir::new(worktree_path)
        .into_iter()
        .filter_entry(|e| !e.path().starts_with(".git"))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        if let Ok(metadata) = entry.metadata() {
            if let Ok(modified) = metadata.modified() {
                let mod_time = modified.duration_since(UNIX_EPOCH)?.as_secs() as i64;
                last_mod = last_mod.max(mod_time);
            }
        }
    }
    
    Ok(FsMetrics {
        last_file_mod_at: last_mod,
    })
}
```

**Tmux activity:**
```rust
fn check_tmux_activity(tmux_target: &str) -> Result<TmuxActivity> {
    let output = Command::new("tmux")
        .args(&["capture-pane", "-p", "-t", tmux_target, "-S", "-3"])
        .output()?;
    
    let pane_text = String::from_utf8_lossy(&output.stdout);
    
    // Check if at shell prompt
    let at_prompt = pane_text.lines().last()
        .map(|l| l.contains("❯") || l.ends_with("$ ") || l.ends_with("# "))
        .unwrap_or(false);
    
    Ok(TmuxActivity {
        at_prompt,
        checked_at: now(),
    })
}
```

### Health Scoring

**Algorithm:**
```rust
fn calculate_health_score(worker: &WorkerState) -> u32 {
    let mut score = 100;
    
    let now = now();
    let age_hours = (now - worker.started_at) / 3600;
    let hours_since_commit = if worker.last_commit_at > 0 {
        (now - worker.last_commit_at) / 3600
    } else {
        age_hours
    };
    let minutes_since_file_mod = (now - worker.last_file_mod_at) / 60;
    
    // Penalize long idle times
    if worker.commit_count == 0 {
        // New worker, no commits yet
        if age_hours > 3 {
            score -= 30;
        }
        if age_hours > 6 {
            score -= 40;  // Total: 70
        }
    } else {
        // Existing worker, has commits
        if hours_since_commit > 6 {
            score -= 20;
        }
        if hours_since_commit > 12 {
            score -= 30;  // Total: 50
        }
    }
    
    // Penalize stale file activity
    if minutes_since_file_mod > 120 {
        score -= 10;
    }
    if minutes_since_file_mod > 240 {
        score -= 15;  // Total: 25
    }
    
    // Penalize if not at prompt and stale
    if !worker.at_prompt && minutes_since_file_mod > 120 {
        score -= 20;
    }
    
    // Bonus for recent activity
    if minutes_since_file_mod < 30 {
        score = (score + 10).min(100);
    }
    
    score.max(0)
}
```

## Display Formats

### jig ps --metrics

**Compact format (default):**
```
WORKER                       STATUS    AGE   COMMITS  ACTIVITY  HEALTH
features/auth                idle      8h    3        15m ago   85/100
improvements/metrics         active    2h    7        2m ago    95/100
chores/cleanup               stale     12h   1        6h ago    20/100
```

**Verbose format (-v):**
```
WORKER: features/auth
  Status:        idle (at shell prompt)
  Age:           8h 23m
  Started:       2026-02-19 10:32:15
  
  Git Activity:
    Commits:     3
    Last commit: 3h 15m ago (a3f2d1c: feat(auth): add login)
    Lines:       +142 -35
    Files:       8
  
  File Activity:
    Last mod:    15m ago
    Modified:    src/auth.rs, src/main.rs
  
  Health:        85/100 (Good)
  Nudges:        0/3
```

### jig ps --health

**Health-focused view:**
```
WORKER                       HEALTH    STATUS        ISSUE
features/auth                85/100    Active        None
improvements/metrics         95/100    Active        None
chores/cleanup               20/100    Stale         No activity 6h
bugs/permission-prompt       15/100    Stuck         At approval prompt 8h
```

### jig ps -g --metrics

**Global view:**
```
REPO       WORKER                       STATUS    AGE   COMMITS  ACTIVITY  HEALTH
jig        features/heartbeat           active    4h    12       5m ago    92/100
jig        improvements/metrics         idle      2h    5        45m ago   78/100
jax-fs     features/encryption          active    6h    18       8m ago    88/100
jax-fs     bugs/sync-race               stale     10h   2        4h ago    25/100
sites      chores/update-deps           idle      3h    1        2h ago    65/100

Total: 5 workers across 3 repos
Unhealthy: 1 (jax-fs: bugs/sync-race)
```

## Configuration

**Per-repo in `jig.toml`:**

```toml
[metrics]
# Update metrics on every jig command (vs only on health checks)
alwaysUpdate = false

# Metrics retention (days, 0 = forever)
retentionDays = 90

# Health score thresholds
[metrics.health]
unhealthy = 30  # Below this = unhealthy
warning = 60    # Below this = warning
good = 80       # Above this = good

# Activity age thresholds for scoring
[metrics.thresholds]
idleNewWorkerHours = 3
idleExistingHours = 6
staleMinutes = 120
```

## Commands

```bash
# Show metrics in ps
jig ps --metrics
jig ps -m  # shorthand

# Verbose metrics
jig ps --metrics -v

# Health-focused view
jig ps --health
jig ps -h  # shorthand

# Show only unhealthy workers
jig ps --health --unhealthy
jig ps -h -u

# Global metrics
jig ps -g --metrics
jig ps -g --health

# Export metrics as JSON
jig ps --metrics --json > metrics.json

# Show metrics history for worker
jig metrics history features/auth

# Show metrics chart (terminal graph)
jig metrics chart features/auth --field commits

# Show productivity stats
jig metrics stats
jig metrics stats -g  # all repos
```

## Metrics History

**Track metrics over time:**

```json
{
  "workers": {
    "features/auth": {
      "history": [
        {
          "timestamp": 1708358400,
          "commit_count": 1,
          "health_score": 90
        },
        {
          "timestamp": 1708362000,
          "commit_count": 3,
          "health_score": 85
        }
      ]
    }
  }
}
```

**Visualization:**
```
$ jig metrics chart features/auth --field commits

Commits over time (features/auth):

3 │                                        ●
  │
2 │                    ●
  │
1 │         ●
  │
0 └─────────────────────────────────────────────────→
  10:00   11:00   12:00   13:00   14:00   15:00
```

## Productivity Stats

```
$ jig metrics stats

PRODUCTIVITY STATISTICS (jig):

Workers: 4 active, 2 completed this week

Average time to completion: 3.2 days
Average commits per worker: 8.5
Average health score: 74/100

Most productive hours:
  10:00-12:00: 12 commits
  14:00-16:00: 8 commits
  16:00-18:00: 5 commits

Least productive hours:
  00:00-06:00: 0 commits
  12:00-14:00: 2 commits (lunch break)

Completed this week:
  • features/github-api (4 days, 12 commits, 247 lines)
  • bugs/auth-token (1 day, 3 commits, 45 lines)
```

## Integration with Health System

**On `jig health` run:**

1. Update metrics for all workers
2. Calculate health scores
3. Detect unhealthy workers (score < threshold)
4. Trigger nudges based on activity patterns
5. Alert on workers below critical threshold

**Example alert:**
```
⚠️  Worker features/auth is unhealthy (health: 25/100)
    Last commit: 8h ago
    Last file mod: 6h ago
    Status: stale (not at prompt)
    
    Consider checking on this worker or killing it if stuck.
```

## Implementation Phases

### Phase 1: Core Metrics
1. Extend health state with metrics fields
2. Git metrics collection (commits, lines, files)
3. File system metrics (last mod time)
4. Tmux activity (at prompt detection)
5. Basic health scoring algorithm

### Phase 2: Display
1. `jig ps --metrics` flag
2. Compact and verbose formats
3. Color coding (green/yellow/red)
4. Global view (`-g` flag)

### Phase 3: Health View
1. `jig ps --health` flag
2. Health-focused display
3. Filter by health status
4. Integration with nudge system

### Phase 4: History & Analytics
1. Track metrics history
2. `jig metrics history` command
3. Terminal charts (ratatui or similar)
4. Productivity stats

### Phase 5: Advanced
1. JSON export
2. Prometheus metrics endpoint (future)
3. Custom health scoring (user-defined weights)
4. Anomaly detection (sudden inactivity)

## Acceptance Criteria

### Core
- [ ] Metrics stored in `.worktrees/.jig-health.json`
- [ ] Git metrics: commits, hash, lines, files
- [ ] FS metrics: last file modification time
- [ ] Tmux activity: at prompt detection
- [ ] Health scoring algorithm implemented

### Display
- [ ] `jig ps --metrics` shows activity columns
- [ ] Compact format (default)
- [ ] Verbose format (`-v`)
- [ ] Color coding (green=good, yellow=warning, red=unhealthy)
- [ ] `jig ps -g --metrics` aggregates across repos

### Health View
- [ ] `jig ps --health` shows health-focused view
- [ ] Filter by health status (`--unhealthy`, `--warning`)
- [ ] Health score displayed with context
- [ ] Integration with nudge system

### Configuration
- [ ] Per-repo thresholds in `jig.toml` `[metrics]`
- [ ] Configurable health thresholds
- [ ] Configurable activity age thresholds
- [ ] Global fallback in `~/.config/jig/config`

### History
- [ ] Track metrics over time
- [ ] `jig metrics history <worker>` shows timeline
- [ ] `jig metrics chart <worker>` shows terminal graph
- [ ] Retention policy (delete old metrics)

### Stats
- [ ] `jig metrics stats` shows productivity analytics
- [ ] Average completion time
- [ ] Commits per worker
- [ ] Most/least productive hours

## Testing

```bash
# Create worker and check metrics
jig spawn features/test
jig ps --metrics

# Make some commits
cd .worktrees/features/test
git commit --allow-empty -m "test"
sleep 60
git commit --allow-empty -m "test2"

# Check updated metrics
jig ps --metrics
# Should show: commit_count=2, last_activity=<1m ago, health=95+

# Wait a while (or mock time)
# Check health degradation
jig ps --health
# Should show lower health score after idle time

# Check history
jig metrics history features/test

# Check productivity stats
jig metrics stats
```

## Open Questions

1. Should metrics be updated on every `jig` command or only on `jig health`? (Only on health, configurable)
2. Should we track LOC per file type? (Future enhancement)
3. Should we integrate with git hooks to update metrics? (Yes, see worker-heartbeat-system)
4. Should health scores be normalized per repo? (No, use absolute scale)

## Related Issues

- issues/features/worker-heartbeat-system.md (uses metrics for health checks)
- issues/features/github-integration.md (could add PR metrics)
- issues/features/global-commands.md (multi-repo metrics aggregation)
