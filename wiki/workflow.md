---
layout: page
title: Workflow
nav_order: 4
---

# The jig Workflow

jig enables a development loop optimized for parallel agent work.

## The loop

```
┌─────────────────────────────────────────────────────────────┐
│  1. PLAN                                                    │
│     Break work into well-scoped issues                      │
│     Write detailed descriptions + acceptance criteria       │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│  2. SPAWN                                                   │
│     Create worktrees for each issue                         │
│     Launch agents with task context                         │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│  3. SUPERVISE                                               │
│     Monitor agent progress                                  │
│     Unblock when stuck, answer questions                    │
│     Steer away from bad decisions                           │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│  4. REVIEW                                                  │
│     Check agent work against acceptance criteria            │
│     Verify code quality and patterns                        │
│     Request changes or approve                              │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│  5. MERGE                                                   │
│     Integrate completed work                                │
│     Clean up worktrees                                      │
│     Repeat                                                  │
└─────────────────────────────────────────────────────────────┘
```

## In practice

### 1. Plan

Start your session by identifying what needs to be done:

```bash
# Check existing issues
ls issues/

# Or create new ones
cp issues/_template.md issues/004-new-feature.md
```

Break large features into parallelizable subtasks. The more independent the tasks, the better they parallelize.

### 2. Spawn

Create worktrees and launch agents:

```bash
# Spawn agents for multiple issues
jig spawn issue-001 --context "Implement the auth API per issues/001-auth.md"
jig spawn issue-002 --context "Fix pagination bug per issues/002-pagination.md"
jig spawn issue-003 --context "Add unit tests for user service"
```

Each agent gets:
- Its own worktree (isolated branch + directory)
- A tmux window you can attach to
- The context you provided

### 3. Supervise

Check on your workers:

```bash
# List all worktrees and their status
jig list

# Attach to a specific agent's session
jig attach issue-001
```

When supervising:
- Answer questions agents raise
- Unblock them when they hit issues
- Correct course if they're going in the wrong direction
- Don't micromanage—let them work

You can supervise multiple agents from your main session, switching between them as needed.

### 4. Review

When an agent signals completion:

```bash
# See what changed
jig review issue-001
```

Check:
- Does it meet acceptance criteria?
- Does it follow codebase patterns?
- Are there tests?
- Any security concerns?
- Any unnecessary changes?

If changes needed, give feedback via the agent session. If approved, proceed to merge.

### 5. Merge

```bash
# Merge the worktree's branch
jig merge issue-001

# Clean up
jig remove issue-001
```

Update issue status to `done` and move on.

## Tips

**Start small.** Run 2-3 agents until you're comfortable supervising in parallel.

**Invest in documentation.** Time spent on clear CLAUDE.md and PATTERNS.md pays off across every agent session.

**Write better tickets.** The quality of agent output correlates directly with the quality of your issue descriptions.

**Don't wait.** While one agent works, spawn another. While reviewing one PR, check on others. Stay active.

**Trust but verify.** Agents do good work, but they make mistakes. Always review before merging.
