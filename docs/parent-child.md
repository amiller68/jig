# Parent-Child Epics

How jig orchestrates multi-ticket epics using parent-child issue relationships.

## Model: Parent-as-Integrator

The parent issue owns the integration branch. Children do the work; the parent wraps up.

### Lifecycle

1. **Setup**: User creates a parent issue (Todo) with child issues (Backlog), wiring `blocked-by` dependencies.
2. **Branch creation**: Daemon creates the parent branch from `origin/main`, pushes it, and moves the parent to InProgress. **No parent worker is spawned.**
3. **Child execution**: Daemon walks the blocked-by DAG, auto-spawning children as their dependencies clear. Children branch off the parent branch and PR into it.
4. **Integration**: Each child PR merge is fast-forwarded into the parent branch (bare, no worktree needed). See [Parent worktree auto-update](./daemon.md#parent-worktree-auto-update).
5. **Wrap-up**: When all children are Complete *and* merged into the parent branch, the daemon spawns the parent worker with a wrap-up preamble. Its job: verify the integrated result, write last-mile code if needed, draft the PR description, and `jig pr` targeting main.
6. **Done**: Parent PR merges into main — epic complete.

### Key rules

- **Parent status**: `InProgress` for the entire epic lifetime (branch creation through wrap-up). No new status needed.
- **Parent worktree**: Created only at wrap-up time. During integration, the branch is fast-forwarded bare.
- **Parent filter**: An issue is treated as a parent (excluded from normal auto-spawn) iff it has ≥1 child in status Backlog or InProgress.
- **Wrap-up timing**: Immediate on the first tick where all children are Complete + merged. No debounce.
- **Parent's own code**: Allowed only as last-mile work during wrap-up. Parents must not block children.

### Manual children

Not all children need the `auto` label. You can `jig spawn` a child manually, work on it, `jig pr` into the parent branch, and merge. The daemon counts the child toward wrap-up readiness regardless of who did the work.

## Data model

### Issue fields

```
parent: { id, title, branch_name, status, body }   # child → parent reference
children: [id, ...]                                  # parent → child listing
```

### Worker state (`workers.json`)

```
parent_branch: Option<String>    # set on child workers; identifies integration branch
issue: Option<String>            # linked issue ID
```

The daemon uses `parent_branch` to:
- Identify which branches need fetching during sync
- Find parent worktrees for fast-forward pulls after child merges
- Deliver `parent_update` nudges

## Non-goals

- **Nested parents** (epic of epics): undefined, future work.
- **Parent cancellation**: undefined, future work.

## Migration from the Old Model

> **Applies to**: Epics started before the parent-as-integrator model landed (pre-JIG-60). Under the old model, the parent worker was spawned immediately alongside children.

### What changed

| Aspect | Old model | New model |
|--------|-----------|-----------|
| Parent worker spawn | Immediately when parent set to Todo | Only at wrap-up (all children done) |
| Parent's role | Worker like any other — races with children | Integration owner — runs last |
| Integration branch | Parent worktree manages it | Daemon manages it bare |
| Child PR target | Could accidentally target main | Always targets parent branch |

### Known failure modes (old model)

These are eliminated by construction in the new model:

- **Race condition**: Parent worker drafts a PR to main before children finish. Nothing tells it to wait.
- **Misrouted PRs**: Child uses `gh pr create` directly, targeting main instead of the parent branch. Merging carries parent commits into main as a side effect.

### Defensive guard: no double-spawn

The daemon includes a safety check: before spawning a parent worker for wrap-up, it verifies no active worker already exists for that parent issue. If an old-model parent worker is still running, the wrap-up spawn is skipped — the existing worker handles it.

This is implemented via `has_active_parent_worker()` in the daemon tick loop. The check matches by issue ID against all non-terminal workers in `workers.json`.

### Cutover walkthrough

When upgrading to the new model with in-flight epics:

**Step 1: Inventory**

List active workers and identify parent workers spawned under the old model:

```bash
jig ps
```

Look for workers whose linked issue has children still in Backlog or InProgress.

**Step 2: For each in-flight epic, choose one path**

**Option A — Let the old parent worker finish.**
If the parent worker is making progress and children are nearly done, let it complete naturally. The daemon will not double-spawn thanks to the active-worker guard.

```bash
# Monitor the parent worker
jig ps --watch
# When it's done, it will create its PR as usual
```

**Option B — Stop the old parent and let the new model take over.**
If children still have significant work remaining, stop the parent worker to avoid the race condition.

```bash
# Kill the parent worker's tmux window
jig kill <parent-worker-name>

# The daemon will:
# 1. Continue managing the integration branch (fast-forward child merges)
# 2. Auto-spawn the parent for wrap-up once all children are Complete + merged
```

**Step 3: Verify**

After cutover, confirm the epic is tracking correctly:

```bash
jig ps              # parent should show InProgress, no worker (unless wrap-up)
jig issues list     # children should show their actual status
```

### Safety guarantees

- The `has_active_parent_worker()` check prevents double-spawning regardless of cutover path.
- `git pull --ff-only` on the integration branch never creates merge commits — conflicts fail safely.
- Child PR target is enforced by the spawn system's `parent.branch_name` field — children always branch from and PR into the parent branch.
