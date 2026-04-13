---
title: Epics
slug: epics
date: 2026-04-13
releases: ["v1.12"]
---

When a ticket is too big for one agent, break it into an epic: a parent issue with child sub-issues. jig's daemon orchestrates the entire lifecycle — creating the integration branch, spawning children in dependency order, accumulating their work, and running a final wrap-up agent to ship the result.

This page explains what triggers epic resolution, how to set one up, and what the daemon does at each stage.

## What makes an issue an epic?

The daemon treats an issue as an epic parent when **all** of these are true:

1. The issue has **at least one sub-issue** (Linear parent/child relation)
2. At least one child has status **Backlog**, **InProgress**, or **Planned**
3. The parent itself is **Planned** or **InProgress**

That's it. No config flag specific to epics. The detection is structural — if an issue has active children, it's an epic.

The parent does **not** need the `auto_spawn_labels` label (that's only for children entering the spawn pipeline). However, the parent **does** need to pass through the provider's configured filters. If your Linear profile or per-repo config has `labels = ["jig"]`, the parent must carry that label to be visible to the daemon's `list()` call at all. Same goes for `team`, `projects`, and `assignee` filters — the parent must match whatever the provider is scoped to.

## The lifecycle

```text
Parent (Planned)
  ├── Child A (Planned, auto label)
  ├── Child B (Planned, auto label, blocked-by A)
  └── Child C (Planned, auto label, blocked-by B)

  Tick 1: Daemon creates integration branch, pushes to origin.
          Parent flipped to InProgress. No parent worker spawned.

  Tick 2: Linear associates the pushed branch with the parent issue.
          Child A's parent now has a branch_name. Child A spawns.

  Tick N: Child A completes, PR merges into parent branch.
          Child B's dependency clears, Child B spawns.
          ...

  Final:  All children Complete + merged into parent branch.
          Daemon spawns a wrap-up worker on the parent.
          Wrap-up agent verifies integration, opens PR to main.
```

### Stage 1: Branch creation

On the first tick where the parent is detected, the daemon:

1. Derives a branch name from the issue (uses Linear's `branchName` if set, otherwise generates one from the issue ID)
2. Creates the branch from `origin/<base_branch>` (whatever the repo's configured base branch is)
3. Pushes to origin
4. Flips the parent issue to **InProgress**

No worker is spawned for the parent at this stage. The branch exists bare on origin — the daemon manages it without a worktree.

### Stage 2: Child spawning

Children become spawnable when three conditions are met:

- The child's eagerly-loaded **parent status is InProgress** (set in stage 1)
- The child's eagerly-loaded **parent branch_name is populated** (Linear associates the branch after the push — there's a one-tick delay)
- The parent branch's **remote tracking ref exists** in the daemon's local clone (populated by the push in stage 1, or by a subsequent fetch)

Plus the normal spawn requirements: the child must be Planned, carry the repo's auto-spawn label, and have all `blocked-by` dependencies satisfied.

Children branch off the parent's integration branch (not main) and their PRs target it.

### Stage 3: Integration

Each time a child PR merges into the parent branch, the daemon fast-forwards the local ref to match the remote. During most of the epic lifecycle there is no parent worktree — the branch is managed bare. If a parent worktree does exist (during wrap-up), the daemon also pulls into it and nudges the parent worker.

Child merges accumulate on the parent branch. No merge commits — fast-forward only.

### Stage 4: Wrap-up

The daemon spawns a wrap-up worker for the parent when:

- All children are **Complete**
- Every child's branch is **reachable from** (merged into) the parent branch tip

The wrap-up agent gets a special preamble. Its job: verify the integrated result builds and passes tests, write any last-mile glue code, draft the PR description, and `jig pr` targeting main.

### Stage 5: Done

The parent PR merges into main. The epic is complete.

## Setting up an epic

### 1. Create the parent issue

```bash
jig issues create "Auth system overhaul" \
  -p high -c Engineering
```

### 2. Create children as sub-issues

```bash
jig issues create "Add JWT token generation" \
  --parent AUTH-1 -l auto -p high

jig issues create "Add refresh token rotation" \
  --parent AUTH-1 -l auto -p high

jig issues create "Add auth middleware" \
  --parent AUTH-1 -l auto -p high \
  --blocked-by AUTH-2   # depends on token generation
```

Replace `auto` with whatever your repo's `auto_spawn_labels` value is (e.g., `jig`).

### 3. Set children to Planned

Children should be in **Planned** (Todo in Linear) to be auto-spawnable. Backlog children are visible to the daemon for parent detection but won't spawn until moved to Planned.

### 4. Start the daemon

```bash
jig daemon
# or
jig ps --watch
```

The daemon handles everything from here: branch creation, status flip, child spawning, integration, and wrap-up.

## Auto vs manual children

Not every child needs to be auto-spawned. Mix and match:

| | Auto child | Manual child |
|---|---|---|
| **Label** | Has the auto-spawn label | No auto-spawn label |
| **How spawned** | Daemon spawns when deps clear | `jig spawn --issue <child-id>` |
| **Worker type** | Autonomous agent | Human or manually-launched agent |
| **Branch base** | Parent branch (automatic) | Parent branch (automatic via `jig spawn --issue`) |
| **PR target** | Parent branch (automatic via `jig pr`) | Parent branch (automatic via `jig pr`) |

Both types count equally toward wrap-up readiness. The daemon tracks completion regardless of who did the work.

### Manual child workflow

```bash
# Spawn a worktree for the child — branches off parent automatically
jig spawn --issue AUTH-5

# Work normally
jig open auth-5
# ... edit, commit, test ...

# PR targets the parent branch automatically
jig pr

# Mark complete when done (or let auto_complete_on_merge handle it)
jig issues complete AUTH-5
```

## PR base resolution

`jig pr` detects the parent relationship and targets the parent branch automatically. No `--base` flag needed. The resolution order:

1. Look up the current worktree's linked issue
2. If the issue has a parent with a `branch_name`, use that as the PR base
3. Otherwise fall back to the repo's configured base branch

This means child PRs always target the parent's integration branch — agents don't need to know about it.

## Dependency ordering

Use `--blocked-by` to control the order children spawn:

```bash
# B waits for A to complete before spawning
jig issues update AUTH-3 --blocked-by AUTH-2

# C waits for both A and B
jig issues update AUTH-4 --blocked-by AUTH-2,AUTH-3
```

The daemon checks `blocked-by` dependencies alongside the parent gate. A child only spawns when both its parent is ready (InProgress + branch exists) **and** all its `blocked-by` issues are Complete.

## Requirements

- **Linear provider** — The parent-child relation, eagerly-loaded child metadata, and `branchName` association all come from Linear's API. The file provider does not support parent-child orchestration.
- **`auto_spawn_labels` configured** — The repo's `jig.toml` (or `jig.local.toml`) must have `auto_spawn_labels` set under `[issues]` for children to be auto-spawned.
- **GitHub integration on Linear** — Linear populates `branchName` on the parent issue when it detects a matching branch push via the GitHub integration. Without this, children see `parent.branch_name = None` and can't spawn (they'll self-heal once the association appears, typically within seconds).

## Limitations

- **No nested epics** — An epic whose parent is itself a child of another epic is undefined. Only one level of parent-child is supported.
- **No parent cancellation** — If you cancel a parent while children are in-flight, the daemon doesn't automatically stop children. Kill them manually.
- **One-tick delay** — After the daemon pushes the parent branch, Linear needs to associate the branch name. Children can't spawn until the next tick when the association is visible in the API response.
- **Linear only** — File-based issues don't store branch names or parent metadata. Epic orchestration requires the Linear provider.
