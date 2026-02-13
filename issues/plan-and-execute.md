# Plan and Execute: Issue-Driven Parallel Spawn

**Status:** Planned

## Background

Inspired by [Anthropic's C compiler experiment](https://www.anthropic.com/engineering/building-c-compiler), where 16 parallel Claude agents built a 100K-line compiler by self-coordinating through git and file-based task claiming.

Our use case is different — we're not building compilers. Features are 5-10K lines at worst, decomposable into 5-10 subtasks. We don't need agents self-selecting from a pool. We want an orchestrator that plans the work, assigns it, and fires off workers — but without the current manual merge ceremony.

## Core Idea

Two changes to how jig works:

1. **`jig plan`** — an orchestrator reads an epic issue, decomposes it into subtask issues, and spawns a worker per subtask
2. **Shared target branch** — all workers land their work on the same branch via auto-rebase, eliminating the manual review/merge loop

## Current Flow (What Changes)

```
Today:
  Human writes context strings → jig spawn x --context "..." (per worker)
  Each worker gets its own branch
  Parent manually: jig review x → jig merge x (per worker)
  Conflicts discovered late, at merge time

Proposed:
  Human writes an epic issue in issues/
  jig plan issues/feature.md
    → Planner agent reads epic
    → Creates subtask issue files (issues/feature-01-*.md, etc.)
    → Spawns N workers, one per subtask, all targeting the same branch
  Workers auto-rebase onto target branch and push when done
  No manual merge step — work lands continuously
```

## Design

### 1. Epic and Subtask Issues

Epics already exist in `issues/`. The change is that subtasks become the formal interface between the orchestrator and workers. A subtask issue IS the worker's task context.

Subtask format (extends current ticket format):

```markdown
# Add JWT Token Generation

**Status:** Planned | Assigned | Complete
**Epic:** [feature-auth.md](./feature-auth.md)
**Assigned-to:** (worker name, written by orchestrator)
**Target:** feature/auth

## Objective

Add JWT token generation and validation to the auth module.

## Implementation Steps

1. Create src/auth/jwt.rs with generate/validate functions
2. Add token expiry (24h) and refresh support
3. Wire into src/auth/mod.rs

## Files to Modify/Create

- `src/auth/jwt.rs` — new, token logic
- `src/auth/mod.rs` — add jwt submodule

## Acceptance Criteria

- [ ] `cargo test auth::jwt` passes
- [ ] Tokens expire after 24h
- [ ] Refresh flow works end-to-end

## Verification

cargo test auth::jwt
```

Key differences from today:
- `Assigned-to` field links subtask to a specific worker
- `Target` field specifies the shared branch
- The issue file replaces `--context` strings — workers read their task from the issue file directly

### 2. `jig plan <epic>` Command

Reads an epic issue, runs a planner agent that:

1. Reads the epic and any referenced docs
2. Decomposes into N subtask issue files
3. Determines the target branch name
4. Creates the target branch from base
5. Spawns N workers, each assigned to one subtask

The planner decides N — the human doesn't need to specify worker count. The planner knows the shape of the work.

```bash
# Human writes issues/feature-auth.md (the epic)
# Then:
jig plan issues/feature-auth.md [--auto]

# Output:
# ✓ Created 4 subtasks from feature-auth
#   → issues/feature-auth-01-jwt.md
#   → issues/feature-auth-02-oauth.md
#   → issues/feature-auth-03-middleware.md
#   → issues/feature-auth-04-tests.md
# ✓ Created target branch: feature/auth
# ✓ Spawned 4 workers targeting feature/auth
#   → feature-auth-01-jwt (running)
#   → feature-auth-02-oauth (running)
#   → feature-auth-03-middleware (running)
#   → feature-auth-04-tests (waiting on 01, 02, 03)
```

### 3. Shared Target Branch

Workers use ephemeral per-worker branches (git requires this for worktrees) but the destination is a shared target branch:

```
Worker "feature-auth-01-jwt":
  worktree branch: _jig/feature-auth-01-jwt  (ephemeral)
  target branch:   feature/auth               (shared)

  On completion:
    git fetch origin feature/auth
    git rebase origin/feature/auth
    git push origin _jig/feature-auth-01-jwt:feature/auth
    # Clean up worktree + ephemeral branch
```

If the rebase has conflicts, the worker resolves them (agents are decent at this). If it fails, the worker's status goes to `Blocked` and `jig ps` surfaces it.

### 4. Worker Lifecycle Changes

```
Current:   Spawned → Running → WaitingReview → Approved → Merged
Proposed:  Spawned → Running → Landing → Complete
                                  ↓
                               Blocked (conflict / test failure)
```

- `Landing` — worker finished its task, now rebasing + pushing to target
- `Complete` — work is on the target branch, worktree can be cleaned up
- `Blocked` — rebase conflict or test failure, needs attention

No more `WaitingReview` / `Approved` / manual `Merged` states. The review happens on the target branch (or PR) after all workers are done.

### 5. `jig ps` Updates

```
$ jig ps
NAME                      STATUS     TARGET          COMMITS  DIRTY
feature-auth-01-jwt       complete   feature/auth    3
feature-auth-02-oauth     running    feature/auth    1        *
feature-auth-03-middleware landing    feature/auth    2
feature-auth-04-tests     waiting    feature/auth    0

Target: feature/auth (7 commits ahead of origin/main)
```

## What This Replaces

- `jig merge` — no longer needed for plan-spawned workers (keep for manual spawn)
- `jig review` — still useful, but moves to reviewing the target branch as a whole
- `--context` strings — replaced by issue files (more structured, persistent, readable)

## What This Keeps

- `jig spawn` — still works for ad-hoc single-worker tasks (unchanged)
- Worktree isolation — workers still can't see each other's uncommitted changes
- tmux integration — unchanged
- `jig ps` / `jig attach` / `jig kill` — unchanged, just richer status

## Implementation Sequence

1. **Subtask issue format** — formalize the format, update `docs/issue-tracking.md`
2. **`--target` flag on spawn** — workers rebase+push to a target branch on completion
3. **Auto-landing flow** — the rebase+push+cleanup logic in jig-core
4. **`jig plan` command** — planner agent reads epic, creates subtasks, calls spawn
5. **Updated `/jig` skill** — teach the skill about `jig plan` workflow
6. **`jig ps` enhancements** — show target branch, aggregate progress

## Open Questions

- Should the planner agent be a Claude session (via tmux) or a direct CLI invocation? Direct invocation is simpler — it just needs to read the epic and write files.
- How do we handle subtask dependencies? Some tasks depend on others (e.g., tests depend on the code being written). The planner could mark dependencies and jig holds those workers until deps are `Complete`.
- Should workers read their task from the issue file, or should jig still pass context via CLI arg? Reading from the issue file is cleaner and doesn't have shell escaping issues.
