# Issue Grinder Analysis & Integration Plan

## Overview

The issue-grinder script (`~/.openclaw/workspace/skills/issue-grinder/grind.sh`) is a 958-line bash script that monitors worker health and automates issue management. It works well but is external to jig, brittle, and not easily extensible.

This document analyzes what the grinder does and proposes a path to integrate its functionality into jig itself.

---

## What the Grinder Does

### Priority 0: Cleanup (lines 204-237)
- **Scans all worktrees** for branches with closed/merged PRs
- **Auto-cleans merged PRs**: kills worker, removes worktree
- **Alerts on closed PRs**: human closed it, likely intentional
- **State cleanup**: removes nudge counts, worker start times

**Takeaway**: Lifecycle management should be automatic in jig.

---

### Priority 1: Resume Stuck/Exited Workers (lines 239-505)

**Activity Tracking (lines 118-150):**
- Tracks per-worker:
  - Worker age (time since spawn)
  - Last commit timestamp
  - Commit count
  - Last file modification time
- Derives activity status from metrics

**Stuck Detection (lines 85-111):**
- Scrapes tmux output for stuck patterns:
  - "Would you like to proceed"
  - "ctrl-g to edit"
  - Multiple-choice menus
- Auto-approves safe prompts (sends `1` + Enter)

**Idle Detection (lines 290-330):**
- Checks if worker is at shell prompt
- Nudges if:
  - No commits after 3h (new workers)
  - No commits in 6h (existing workers)
- Smart nudge messages based on context

**Stale Detection (lines 332-340):**
- Workers not at prompt, no file changes in >2h
- Logged but not nudged (might be thinking/testing)

**Resume Logic (lines 350-505):**
- Workers with `exited` or `no-window` status
- Checks if PR exists (merged, closed, or open)
- If no PR: nudges up to 3 times, then respawns with resume context
- Resume context includes:
  - Uncommitted changes (if any)
  - Instructions to commit, create PR, update issue, call /review
  - Rebase instructions for conflicts

**Takeaway**: This should be `jig health --watch` with built-in nudging.

---

### Priority 2: Status Report (lines 506-509)
- Just logs if workers are running

**Takeaway**: This is already `jig ps`.

---

### Priority 3: New Issues (lines 511-597)

**Issue Discovery:**
- Recursively scans `issues/` directory
- Finds issues with `Status: Planned`
- Auto-spawns `Priority: High` or `Priority: Urgent`
- Collects others for pending report

**Issue Context:**
- Injects full issue content
- Adds workflow instructions:
  - Update issue status
  - Log progress
  - Commit with conventional commits
  - Create PR with `Addresses: issues/<path>`
  - Update issue with PR number
  - Call `/review`

**Takeaway**: Should be `jig issues list` and `jig issues spawn`.

---

### Priority 4: Failing CI (lines 599-673)

**Detection:**
- Polls all open PRs for CI status
- Detects `FAILURE` or `ERROR` checks
- Fetches error logs from failed runs

**Handling:**
- If worker exists: nudges with error details
- If worker doesn't exist: respawns with CI context
- Tracks nudge count (max 3)

**Takeaway**: Should be part of `jig health` with GitHub integration.

---

### Priority 4.5: Merge Conflicts (lines 675-749)

**Detection:**
- Checks PR `mergeable` status via GitHub API
- Detects `CONFLICTING` state

**Handling:**
- Nudges worker with rebase instructions
- Includes step-by-step conflict resolution
- Tracks nudge count (max 3)

**Takeaway**: Should be part of `jig health` with GitHub integration.

---

### Priority 5: Review Comments (lines 751-857)

**Detection:**
- Checks PR `reviewDecision` (CHANGES_REQUESTED)
- Counts inline review comments via API
- Counts general review comments

**Handling:**
- Fetches inline comments with file/line context
- Fetches review body text
- Nudges worker with full review context
- Prefers nudging original worker vs spawning new one

**Takeaway**: Should be part of `jig health` with GitHub integration.

---

### Priority 6: Non-Conventional Commits (lines 858-920)

**Detection:**
- Validates all commits on PR branch
- Regex: `^(feat|fix|docs|style|refactor|perf|test|chore|ci)(\(.+\))?!?:`
- Lists commits that don't match

**Handling:**
- Nudges worker with rebase instructions
- Explains conventional commit format
- Includes example

**Takeaway**: Should be pre-commit hook + `jig health` check.

---

## State Management

**File:** `~/.openclaw/workspace/issue-grinder-state.json`

**Schema:**
```json
{
  "nudges": {
    "jax-fs:prompt:worker-name": 2,
    "jax-fs:ci:123": 1
  },
  "seenPRs": {
    "jax-fs:pr:123": 1234567890
  },
  "workerStarts": {
    "jax-fs:worker-name": 1234567890
  }
}
```

**Operations:**
- `get_nudge_count(key)` - retrieve nudge count
- `inc_nudge_count(key)` - increment nudge count
- `clear_nudge_count(key)` - reset nudge count
- `track_worker_start(repo, worker)` - record spawn time
- `get_worker_age_hours(repo, worker)` - calculate age

**Takeaway**: Should be part of jig's state directory.

---

## Prompting Strategy

The grinder has evolved sophisticated, context-aware prompts:

### Spawn Context (lines 554-580)
- Full issue content
- Workflow instructions
- Commit message guidelines
- PR creation steps
- Issue update requirements

### Resume Context (lines 436-486)
- Different messages for clean vs dirty worktree
- Instructions based on current state
- Workflow reminders
- Conflict resolution guidance

### Nudge Context (lines throughout)
- **Idle**: status check, ask what's blocking
- **Stuck**: auto-approve or ask for help
- **CI failure**: error logs + fix instructions
- **Conflicts**: step-by-step rebase guide
- **Reviews**: inline comments + resolution steps
- **Bad commits**: rebase guide + conventional commit format

**Takeaway**: Should be templated in jig with variable injection.

---

## Integration Plan

### Phase 1: Core Infrastructure
**Tickets:**
- `issues/features/worker-heartbeat-system.md`
- `issues/improvements/worker-activity-metrics.md`

**Deliverables:**
- `jig health` command with periodic checks
- Worker metrics tracking (age, commits, files, time)
- State persistence in jig's state directory
- `jig ps --metrics` to show activity

### Phase 2: GitHub Integration
**Tickets:**
- `issues/features/github-integration.md`

**Deliverables:**
- Native GitHub API client
- PR status checks (CI, conflicts, reviews, commits)
- Auto-cleanup of merged PRs
- `jig pr status` and `jig pr list` commands

### Phase 3: Smart Context
**Tickets:**
- `issues/features/smart-context-injection.md`

**Deliverables:**
- Template system for spawn/resume/nudge contexts
- Variable injection based on worker state
- User-customizable templates
- Default templates shipped with jig

### Phase 4: Issue Management
**Tickets:**
- `issues/improvements/planned-issue-management.md`

**Deliverables:**
- `jig issues list` to discover planned issues
- Priority-based auto-spawning
- Batch spawning with interactive selection
- Issue dependency tracking

### Phase 5: Cleanup
**Tickets:**
- `issues/bugs/conventional-commit-regex-warnings.md`

**Deliverables:**
- Fix grep warnings in grinder script (interim)
- Eventually deprecate grinder in favor of jig built-ins

---

## Migration Strategy

**Short-term (keep grinder):**
1. Fix immediate bugs (grep warnings)
2. Improve grinder prompts
3. Add jig repo to grinder REPOS list ✓

**Medium-term (hybrid):**
1. Implement Phase 1 (heartbeat + metrics) in jig
2. Run both grinder and `jig health --watch` in parallel
3. Compare results, tune jig implementation
4. Gradually move checks from grinder to jig

**Long-term (deprecate grinder):**
1. Complete all phases in jig
2. Make grinder optional, off by default
3. Document migration path for users
4. Remove grinder entirely

---

## Key Improvements in jig

**vs Grinder:**
- ✅ Native Rust, faster and more reliable
- ✅ Proper error handling
- ✅ Built-in GitHub client with rate limit handling
- ✅ Structured state management (not external JSON)
- ✅ Configurable via `jig config`
- ✅ Templated contexts (user-customizable)
- ✅ Better CLI interface
- ✅ Cross-platform (not just bash)
- ✅ Testable (unit tests for all logic)

**Unique to jig:**
- Worker metrics in `jig ps`
- Health scoring system
- Issue dependency tracking
- Interactive batch spawning
- Pre-commit hooks
- Real-time webhook support (future)

---

## Tickets Created

1. **`issues/features/worker-heartbeat-system.md`** - Core health checking, nudging, auto-approval
2. **`issues/features/github-integration.md`** - Native GitHub API for PR checks, CI, reviews, conflicts
3. **`issues/features/smart-context-injection.md`** - Templated contexts for spawn/resume/nudge
4. **`issues/improvements/worker-activity-metrics.md`** - Track and display worker activity
5. **`issues/improvements/planned-issue-management.md`** - Better issue discovery and spawning
6. **`issues/bugs/conventional-commit-regex-warnings.md`** - Fix grep warnings in grinder

---

## Recommendations

**High Priority:**
1. Start with **worker-heartbeat-system** - this is the foundation
2. Add **github-integration** - this handles the majority of grinder's checks
3. Implement **worker-activity-metrics** - this makes health checks useful

**Medium Priority:**
4. Add **smart-context-injection** - this improves worker prompts
5. Implement **planned-issue-management** - this improves issue workflow

**Low Priority:**
6. Fix **conventional-commit-regex-warnings** - minor but annoying

**Overall:**
- The grinder logic is solid, just needs to be in jig
- Most work is integrating GitHub API and tmux scraping
- The prompting/context system is well-thought-out and should be preserved
- State management should use jig's existing state infrastructure

Let me know which ticket to prioritize first!
