---
title: Parent-Child Epics
slug: parent-child
date: 2025-05-09
---

When a ticket is too big for one agent, break it into an epic: a parent issue with child sub-issues. jig's daemon orchestrates the lifecycle — creating the integration branch, spawning children in dependency order, and accumulating their work on the parent branch.

## What makes an issue an epic?

The daemon treats an issue as an epic parent when **all** of these are true:

1. The issue has **at least one sub-issue** (Linear parent/child relation)
2. At least one child has status **Backlog**, **InProgress**, or **Planned**
3. The parent itself is **Planned** or **InProgress**

No config flag needed — detection is structural. If an issue has active children, it's an epic.

The parent **does** need to pass through the provider's configured filters (`team`, `projects`, `labels`, `assignee`).

## The lifecycle

```text
Parent (Planned)
  ├── Child A (Planned)
  ├── Child B (Planned, blocked-by A)
  └── Child C (Planned, blocked-by B)

  Tick 1: Daemon creates integration branch, pushes to origin.
          Parent → InProgress. No parent worker spawned.

  Tick 2: Linear associates branch with parent issue.
          Child A's parent now has a branch_name. Child A spawns.

  Tick N: Child A completes, PR merges into parent branch.
          Child B's dependency clears, Child B spawns.
          ...

  Final:  All children Complete + merged into parent branch.
          Human opens PR from parent branch to main.
```

### Branch creation

On the first tick where the parent is detected:

1. Derives a branch name from the issue (uses Linear's `branchName` if set, otherwise generates from issue ID)
2. Creates the branch from `origin/<base_branch>`
3. Pushes to origin
4. Flips the parent to **InProgress**

No worker is spawned — the branch exists bare on origin.

### Child spawning

Children become spawnable when:

- Parent status is **InProgress**
- Parent `branch_name` is populated (Linear associates the branch after push — one-tick delay)
- The parent branch's remote tracking ref exists locally
- Normal spawn requirements met: Planned status, `blocked-by` dependencies satisfied

Children branch off the parent's integration branch (not main) and their PRs target it.

### Integration

Each child PR merges into the parent branch, accumulating work. The parent branch is managed without a worktree during this phase.

## Setting up an epic

### 1. Create the parent issue

```bash
jig issues create "Auth system overhaul" \
  -p high -c Engineering
```

### 2. Create children as sub-issues

```bash
jig issues create "Add JWT token generation" \
  --parent AUTH-1 -p high

jig issues create "Add refresh token rotation" \
  --parent AUTH-1 -p high

jig issues create "Add auth middleware" \
  --parent AUTH-1 -p high \
  --blocked-by AUTH-2
```

### 3. Set children to Planned

Children should be **Planned** to be auto-spawnable. Backlog children are visible for parent detection but won't spawn until promoted.

### 4. Start the daemon

```bash
jig ps -gw
```

The daemon creates the integration branch and spawns children as dependencies clear.

## Manual child workflow

```bash
# 1. Create child issue
jig issues create "Investigate auth edge cases" \
  --parent JIG-60 --category features

# 2. Spawn worktree (branches from parent automatically)
jig spawn --issue JIG-64

# 3. Work normally
jig open <child-name>
cargo test
git add -A && git commit -m "fix: handle auth edge case"

# 4. PR targets parent branch automatically
jig pr

# 5. After merge, mark complete
jig issues complete JIG-64
```

## Dependency ordering

Use `--blocked-by` to control spawn order:

```bash
# B waits for A
jig issues update AUTH-3 --blocked-by AUTH-2

# C waits for both A and B
jig issues update AUTH-4 --blocked-by AUTH-2,AUTH-3
```

The daemon checks `blocked-by` alongside the parent gate. A child only spawns when its parent is ready **and** all blockers are Complete.

## PR base resolution

`jig pr` resolves the base branch automatically:

1. Looks up the current worktree's linked issue
2. If the issue has a parent with a `branch_name`, uses the parent branch
3. Falls back to the repo's configured base branch

Child PRs always target the parent's integration branch — no `--base` flag needed.

## Requirements

- **Linear provider** — parent-child relations, eagerly-loaded metadata, and `branchName` association all require Linear's API
- **GitHub integration on Linear** — Linear populates `branchName` when it detects a matching branch push via the GitHub integration

## Limitations

- **No nested epics** — only one level of parent-child is supported
- **No parent cancellation** — canceling a parent doesn't automatically stop children; kill them manually
- **One-tick delay** — after branch push, Linear needs to associate the branch name; children can't spawn until the next tick
- **Linear only** — file-based issues don't support parent-child orchestration

## Future work

- **Wrap-up worker** — when all children are Complete and merged into the parent branch, the daemon could automatically spawn a wrap-up agent on the parent worktree. Its job: verify the integrated result builds and passes tests, write any last-mile glue code, and `jig pr` targeting main.
- **Parent worktree auto-update** — when a child PR merges into the parent branch, the daemon could `git pull --ff-only` into the parent worktree (if one exists) and send a `parent_update` nudge so the agent knows about new code.
- **Parent cancellation** — canceling or deleting a parent could automatically kill child workers and clean up their worktrees.
- **Nested epics** — support multiple levels of parent-child nesting for large-scale decomposition.
