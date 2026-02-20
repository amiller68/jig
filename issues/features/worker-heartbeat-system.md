# Worker Heartbeat System

**Status:** Planned  
**Priority:** High  
**Category:** Features  
**Depends-On:** issues/features/global-commands.md

## Objective

Add a built-in heartbeat system to `jig` that periodically checks worker health, detects stuck threads, and automatically nudges or escalates issues. Replace external monitoring scripts with native, configurable health checks.

## Background

Currently, health monitoring is handled externally via shell scripts that:
- Scrape tmux output to detect stuck prompts
- Track worker age, commit activity, file modifications
- Nudge idle workers via tmux send-keys
- Escalate to human after max nudges

This should be native to jig, configurable per-repo, and integrated with git hooks for event-driven checks.

## Architecture

### Nudge State Tracking

**New state file: `.worktrees/.jig-health.json`**

```json
{
  "workers": {
    "features/auth": {
      "started_at": 1708358400,
      "last_commit_at": 1708362000,
      "commit_count": 3,
      "last_file_mod_at": 1708363200,
      "nudges": {
        "idle": 2,
        "ci_failure": 1,
        "conflict": 0
      }
    }
  },
  "max_nudges": 3
}
```

**Nudge types:** idle, stuck_prompt, ci_failure, conflict, review, bad_commits

### Git Hooks Integration

**post-commit hook:**
- Update `last_commit_at` and `commit_count`
- Reset idle nudge count (worker made progress)
- Trigger immediate health check (optional, configurable)

**post-merge/post-rebase hook:**
- Reset conflict nudge count
- Update metrics

**Setup during `jig init`:**
```bash
jig init  # First time: installs hooks
jig init  # Second time: idempotent, verifies/updates hooks without error
```

Hooks stored in `.git/hooks/` with jig marker comments for safe reinstall.

## Configuration

**Per-repo in `jig.toml`:**

```toml
[health]
# Auto-approve patterns (worker stuck at these prompts)
autoApprove = [
    "Would you like to proceed",
    "ctrl-g to edit",
]

# Max nudges before human escalation
maxNudges = 3

# Idle thresholds (hours)
idleThresholds.newWorker = 3   # no commits after 3h
idleThresholds.existing = 6     # no commits in 6h
idleThresholds.stale = 2        # no file changes in 2h

# Run health check after git operations
checkOnCommit = false
checkOnMerge = true

# Heartbeat interval (minutes, 0 = manual only)
watchInterval = 15
```

**Global fallback in `~/.config/jig/config`:**
```
health.maxNudges=3
health.watchInterval=15
```

## Commands

```bash
# Single check (current repo)
jig health

# Global check (all registered repos)
jig health -g

# Watch mode (periodic checks, current repo)
jig health --watch

# Global watch (all registered repos)
jig health --watch -g

# Manual nudge
jig nudge <worker> [--message "custom message"]

# Reset nudge count
jig health reset <worker>

# Show health metrics
jig health status
```

## Nudge Messages

### Idle Worker (at prompt, no activity)

**Context:** Worker at shell prompt, no commits for >threshold

```
STATUS CHECK: You've been idle for a while (no commits after 3h). 
What's the current state? 

If stuck, explain the blocker. 
If ready to finish: commit (conventional format), push, create PR with 
'Addresses: issues/<issue-name>.md', update issue status, call /review.
```

### Stuck at Prompt (interactive approval)

**Context:** Detects "Would you like to proceed" pattern in tmux output

**Action:** Auto-approve by sending `1` + Enter (configurable patterns)

### Uncommitted Changes (no PR yet)

**Context:** Dirty worktree, no commits, no PR

```
STATUS CHECK: You have uncommitted changes but no PR yet. What's blocking you?

1. If ready: commit (use conventional commits), push, create PR, update issue, call /review
2. If stuck: explain what you need help with
3. If complete but confused: review the workflow in your context and finish the PR
```

### No Activity (clean worktree)

**Context:** Clean worktree, no commits, no PR

```
STATUS CHECK: No commits yet, no PR. What's the current state?

1. Still working? Give a brief status update and continue
2. Stuck on something? Explain what's blocking you
3. Done but forgot to create PR? Commit your work, push, create PR, call /review
```

## Detection Logic

### Stuck Patterns (tmux scraping)

```rust
fn is_stuck_at_prompt(tmux_output: &str) -> bool {
    let patterns = [
        "Would you like to proceed",
        "ctrl-g to edit",
        // Match numbered menus: "1. Yes  2. Yes  3. Yes"
        r"❯.*\d+\.\s+Yes.*\d+\.\s+Yes",
    ];
    
    patterns.iter().any(|p| {
        Regex::new(p).unwrap().is_match(tmux_output)
    })
}
```

### Idle Detection

```rust
fn should_nudge_idle(worker: &WorkerState, thresholds: &IdleThresholds) -> bool {
    let age_hours = worker.age_hours();
    let hours_since_commit = worker.hours_since_last_commit();
    
    if worker.commit_count == 0 && age_hours >= thresholds.new_worker {
        return true;
    }
    
    if worker.commit_count > 0 && hours_since_commit >= thresholds.existing {
        return true;
    }
    
    false
}
```

### Stale Detection (not at prompt)

```rust
fn is_stale(worker: &WorkerState, threshold_hours: u64) -> bool {
    !worker.at_shell_prompt() && 
    worker.hours_since_file_mod() > threshold_hours
}
```

## Implementation Phases

### Phase 0: Dependencies
- issues/features/global-commands.md (registry + GlobalContext)

### Phase 1: Core Infrastructure
1. Add `WorkerHealthState` struct to jig-core
2. Persist in `.worktrees/.jig-health.json`
3. Basic `jig health` command (current repo)
4. Tmux scraping utilities

### Phase 2: Git Hooks
1. Hook templates in jig binary
2. `jig init` installs hooks with markers
3. Idempotent reinstall (check markers, update if needed)
4. Hooks update health state on commit/merge

### Phase 3: Detection & Nudging
1. Stuck/idle/stale detection logic
2. `jig nudge` command with tmux send-keys
3. Auto-approval for safe patterns
4. Per-nudge-type counters and thresholds

### Phase 4: Global Operations
1. Use GlobalContext for `-g` flag
2. `jig health -g` checks all registered repos
3. `jig health --watch -g` daemon mode

## Acceptance Criteria

### Core
- [ ] Health state tracked in `.worktrees/.jig-health.json`
- [ ] `jig health` runs single check on current repo
- [ ] `jig health -g` checks all registered repos efficiently
- [ ] Nudge counts tracked per-worker, per-type
- [ ] Max nudges configurable per-repo in `jig.toml`

### Git Hooks
- [ ] `jig init` installs post-commit, post-merge, post-rebase hooks
- [ ] Hooks update health metrics (commit time, count, reset nudges)
- [ ] Idempotent: running `jig init` twice is safe
- [ ] Hooks marked with `# jig-managed` comments for detection

### Detection
- [ ] Stuck detection via tmux scraping
- [ ] Auto-approval for configured patterns
- [ ] Idle detection (no commits after threshold)
- [ ] Stale detection (no file changes, not at prompt)

### Nudging
- [ ] `jig nudge <worker>` sends contextual message via tmux
- [ ] Automatic nudging on `jig health` run
- [ ] Smart messages based on worker state (dirty, clean, at prompt, etc.)
- [ ] Escalation after max nudges (alert to stdout/webhook)

### Configuration
- [ ] Per-repo settings in `jig.toml` `[health]` section
- [ ] Global fallback in `~/.config/jig/config`
- [ ] Configurable auto-approve patterns
- [ ] Configurable thresholds (idle, stale, max nudges)

### Watch Mode
- [ ] `jig health --watch` runs periodic checks (configurable interval)
- [ ] `jig health --watch -g` watches all registered repos
- [ ] Graceful shutdown on SIGINT

## Testing

```bash
# Install hooks
cd test-repo && jig init

# Verify hooks installed
ls -la .git/hooks/post-commit

# Create worker and make it idle
jig spawn features/test
# Wait 3+ hours (or mock time in tests)

# Trigger health check
jig health

# Verify nudge sent (check tmux output)
tmux capture-pane -p -t jig-test-repo:features/test

# Verify nudge count incremented
cat .worktrees/.jig-health.json | jq '.workers["features/test"].nudges.idle'

# Make a commit (should reset nudge count)
cd .worktrees/features/test && git commit --allow-empty -m "test"
cat ../.jig-health.json | jq '.workers["features/test"].nudges.idle'  # should be 0
```

## Open Questions

1. Should hooks be optional? (Some users might want manual-only health checks)
2. Parallel nudging across repos in `-g` mode? (Might overwhelm tmux)
3. Alert delivery: stdout only, or webhook support? (Leave for future notification system)
4. Should stale workers (not at prompt) be nudged differently? (Currently just logged)

## Related Issues

- issues/features/github-integration.md (CI/conflict/review nudges)
- issues/features/smart-context-injection.md (nudge message templates)
- issues/improvements/worker-activity-metrics.md (metrics used for detection)
