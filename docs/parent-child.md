# Parent-Child Epics

How jig orchestrates multi-ticket epics using parent-child issue relationships.

## Model: Parent-as-Integrator

The parent issue owns the integration branch. Children do the work; the parent wraps up.

```
Parent (epic)     ──────────────────────────────────────────▶ wrap-up PR → main
  │                                                           ▲
  ├─ Child A (auto)   branch off parent ──▶ PR → parent      │
  ├─ Child B (auto)   branch off parent ──▶ PR → parent ─────┘ (all merged)
  └─ Child C (manual) branch off parent ──▶ PR → parent
```

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

## Auto vs Manual Children

Choose between auto-spawned and manual children based on the nature of the work:

| | Auto | Manual |
|---|---|---|
| **When to use** | Well-scoped, scriptable, independent tasks | Exploratory, sensitive, needs human judgment |
| **Label** | Add `auto` label to the issue | No `auto` label |
| **How spawned** | Daemon spawns automatically when deps clear | User runs `jig spawn --issue <child-id>` |
| **Worker type** | Autonomous Claude Code agent | Human in a jig worktree (or manual agent) |
| **Branch base** | Parent branch (automatic) | Parent branch (automatic via `jig spawn --issue`) |
| **PR target** | Parent branch (automatic via `jig pr`) | Parent branch (automatic via `jig pr`) |
| **Wrap-up readiness** | Counts when Complete + merged | Counts when Complete + merged |

Both types count equally toward wrap-up readiness. The daemon tracks child completion regardless of who did the work.

## Manual-Child Flow

Step-by-step commands for working on a child issue manually.

### 1. Create the child issue

```bash
# Create a child issue under the parent epic
jig issues create "Investigate auth edge cases" \
  --parent JIG-60 \
  --category features
```

### 2. Spawn a worktree

```bash
# Spawn creates a worktree branched from the parent's integration branch
jig spawn --issue JIG-64

# Or, if using the file provider (no branch_name on parent), specify --base:
jig spawn --issue features/investigate-auth --base origin/al/jig-60-parent-epic
```

The worktree is created from the parent branch. For Linear-backed issues, the parent branch is resolved automatically. For the file provider, use `--base` to specify the parent branch explicitly.

### 3. Do the work

```bash
# Navigate to the worktree
jig open <child-name>

# Work normally — edit, commit, test
cargo test
git add -A && git commit -m "fix: handle auth edge case"
```

### 4. Create a PR targeting the parent branch

```bash
# From inside the worktree:
jig pr

# jig pr automatically detects the parent relationship and targets
# the parent's branch instead of main.
```

### 5. Review and merge

The PR targets the parent branch. After review and merge:
- The daemon fast-forwards the parent branch to include the child's commits.
- The child is counted toward wrap-up readiness.

### 6. Mark complete

With `auto_complete_on_merge = true` in `[issues]`, the daemon marks the child issue as Complete automatically when the child PR merges — no manual step needed. Without it:

```bash
jig issues complete <child-id>
```

Or, if using Linear, the status syncs automatically when the PR merges.

## PR Base Resolution

`jig pr` resolves the PR base branch automatically:

1. Detects the current worktree and looks up the associated worker in orchestrator state.
2. Fetches the issue from the provider and checks for a parent relationship.
3. If the issue has a parent with a `branch_name`, uses the parent branch as the PR base.
4. Falls back to the repo's configured base branch (usually `origin/main`).

This means child PRs always target the parent branch — no manual `--base` flag needed.

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

## Parent Worktree Auto-Update

When a child PR is merged into the parent branch, the daemon automatically pulls the changes into the parent worktree (if one exists at wrap-up time):

- Uses `git pull --ff-only` — never creates merge commits.
- Sends a `parent_update` nudge to inform the parent worker of new commits.
- Parent branch fetch failures are non-fatal.

See [daemon.md](./daemon.md) for full details on the auto-update mechanism.

## Non-goals

- **Nested parents** (epic of epics): undefined, future work.
- **Parent cancellation**: undefined, future work.
- **Parent blocking children**: Parents should not block children. The parent worker only runs at wrap-up time. If you need work done before children start, create it as a separate child (e.g., "T0: setup").

## File Provider Limitations

The file-based issue provider (`issues/` directory) does not store branch names on issues. This means:
- The parent's `branch_name` is `None` when resolved from the file provider.
- `jig spawn --issue <child>` falls back to the repo base branch unless `--base` is specified.
- `jig pr` also cannot auto-resolve the parent branch for file-provider issues.

For full parent-child orchestration, use the **Linear provider** which populates branch names from the API.

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
