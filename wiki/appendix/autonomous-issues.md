---
layout: page
title: Autonomous Issue Resolution
nav_order: 1
parent: Appendix
---

# Autonomous Issue Resolution

What if your AI assistant could not only track issues but actually work on them—spawning autonomous coding agents, monitoring their progress, and intervening when they get stuck?

This is possible by combining jig's worktree-based workflow with a scheduling system that periodically scans for work, spawns agents, and supervises their progress.

## The concept

A cron-driven automation that:

1. Scans repositories for markdown-based issue files
2. Spawns Claude Code workers to tackle high-priority tasks
3. Monitors worker progress
4. Intervenes when things go sideways
5. Cleans up completed work

Think of it as an autonomous dev manager that never sleeps.

## The execution loop

Every few hours, the automation runs through priorities:

### Priority 0: Clean up completed work

- Detect merged PRs and remove workers
- Detect closed (not merged) PRs, kill workers, alert you
- Prevent zombie processes from cluttering tmux sessions

### Priority 1: Check on active workers

- List all running jig workers via `jig list`
- Identify workers with PRs already created (done, waiting for review)
- Find stuck workers (no PR, no commits, no progress)

### Priority 2: Nudge stuck workers

- Before killing stuck workers, send them a message via `tmux send-keys`
- Ask contextual questions: "What's blocking you?" or "Ready to commit?"
- Only respawn after multiple failed nudges (gentle, not aggressive)

### Priority 3: Spawn new workers

- Scan `issues/` directory for `status: ready` + high priority
- Spawn fresh workers with full context from issue files
- Use jig's `--auto` mode for hands-off execution

### Priority 4+: Monitor existing PRs

- Check CI status, respawn workers for failing builds
- Detect unaddressed PR review comments, nudge original workers
- Validate conventional commit messages, nudge for fixes

## Why this works

The system composes tools that each do one thing well:

**Scheduler provides:**
- Reliable scheduling (cron with isolated sessions)
- Cross-session coordination
- Persistent state tracking
- Alerts (Telegram, Discord, etc.)

**jig provides:**
- Git worktree management (one worktree per task, no branch conflicts)
- Claude Code lifecycle (spawn, attach, status, kill)
- Persistent tmux sessions (automation can send keys to active workers)

**Claude Code provides:**
- Autonomous coding (reads issue, writes code, commits, creates PR)
- `/review` workflow (workers call this when ready)
- Auto-resume on restart

## Integration pattern

The automation is just a bash script that calls external CLIs:

### 1. Store state in a persistent location

```bash
STATE_FILE="$HOME/.automation/issue-resolver-state.json"
```

Track:
- Nudge counts per worker (avoid infinite nudging)
- Seen PRs (only alert on new PRs)
- Last run timestamp (rate limiting)

### 2. Call external CLIs

```bash
# jig CLI for worker status
jig list

# GitHub CLI for PR info
gh pr list --state open --json number,headRefName

# tmux for sending messages to workers
tmux send-keys -t "jig-reponame:workername" "STATUS CHECK..." Enter
```

No custom integrations needed—just shell commands.

### 3. Parse output for alerts

```bash
alert() {
    ALERTS="${ALERTS}\n$1"
}

# Later...
echo "=== ALERTS ==="
echo -e "$ALERTS"
```

The scheduler parses these and routes them to your notification system.

## Lessons learned

Building autonomous agent supervision teaches hard lessons:

### Nudge before killing

Early versions immediately respawned stuck workers. Wasteful—often the worker was just planning or waiting for a long build. Send multiple contextual nudges via tmux before giving up.

### Conventional commits are mandatory

Add automatic commit message linting. If a worker makes non-conventional commits, respawn with instructions to fix. This ensures clean changelogs and automated releases.

### Closed PRs mean stop immediately

The biggest source of zombie workers: continuing to work on tasks where the human already merged or closed the PR manually. Check for closed PRs first and kill workers immediately.

### Workers must update issue files

Require workers to document progress in the issue markdown as they work (status changes, progress logs, PR numbers). This creates an audit trail and helps future workers if the task restarts.

### State management is critical

Without persistent state, the automation spams you with alerts about the same PR every cycle. Track seen PRs, nudge counts, and last-run state to make it feel intelligent instead of noisy.

## Results

With this approach:

- **Active workers:** 2-5 at any given time
- **Manual intervention:** Only for complex issues or design decisions
- **False positives:** <5%

You go from manually checking issues daily to getting periodic summaries. When something needs attention, you know immediately. When work completes, PRs show up ready for review.

The automation isn't perfect—workers still get confused, CI still breaks, humans still need to review code. But it handles the grunt work of issue tracking, worker lifecycle, and basic supervision autonomously.

## Hurdles

### Worker context limits

Long-running workers accumulate context and eventually hit limits. The automation needs to detect this and gracefully restart workers with summarized context.

### Conflicting changes

Multiple workers on related issues can create merge conflicts. The automation needs to detect overlapping file changes and either serialize the work or alert for human intervention.

### Flaky CI

Workers get respawned repeatedly when CI is flaky rather than genuinely broken. Add backoff logic and distinguish between "test failed" and "infrastructure failed."

### Issue scope creep

Workers sometimes expand scope beyond the original issue. The automation should detect when diff size exceeds expected bounds and pause for human review.

### tmux session management

tmux sessions can get into weird states. The automation needs robust session detection and cleanup logic.

## Conclusion

Autonomous issue resolution shows how jig can be orchestrated by external tools without tight integration. By combining cron scheduling, tmux control, and CLI shelling, you can build sophisticated automation that feels native to your workflow.

The key insight: the orchestrator doesn't need to integrate with everything. It just needs to schedule agents, manage state, and route alerts. The rest is bash and CLIs.

Start simple (check status every hour), then layer in intelligence (nudge, respawn, alert). You'll be surprised how much you can automate.
