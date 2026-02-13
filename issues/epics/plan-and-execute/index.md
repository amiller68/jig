# Epic: Plan and Execute

**Status:** Planned
**Type:** Epic

## Vision

Transform jig from a manual spawn-review-merge workflow into an automated plan-execute-land pipeline. A human writes an epic, jig decomposes it into subtasks, spawns parallel workers, and lands their work continuously onto a shared branch.

```
Today:    Human → spawn → spawn → spawn → review → merge → review → merge → ...
Tomorrow: Human → plan → [workers auto-land] → review final branch → merge to main
```

## Motivation

Anthropic's C compiler experiment showed 16 parallel Claude agents building 100K lines by self-coordinating through git. Our use case is smaller (5-10K line features, 5-10 subtasks) but the insight applies: **the orchestrator should plan and assign, not babysit**.

Current pain points:
- Manual `--context` strings per worker
- Each worker on its own branch
- Manual `jig review` / `jig merge` ceremony per worker
- Conflicts discovered late, at merge time
- Human is the bottleneck

## Design

### Issue Files as Task Interface

Issue files become the contract between orchestrator and workers:

```markdown
---
epic: feature-auth
status: planned
assigned:
target: feature/auth
---

# Add JWT Token Generation

## Objective
Add JWT token generation and validation to the auth module.

## Acceptance Criteria
- [ ] `cargo test auth::jwt` passes
- [ ] Tokens expire after 24h

## Context
Builds on existing auth module in `src/auth/`.
```

Workers read their task from the issue file. No `--context` strings, no `.jig/task.md`.

### Shared Target Branch

All workers land on the same branch continuously:

```
main
 └── feature/auth                    ← shared target
      ├── _jig/feature-auth-01-jwt   ← worker 1 (ephemeral)
      ├── _jig/feature-auth-02-oauth ← worker 2 (ephemeral)
      └── _jig/feature-auth-03-tests ← worker 3 (ephemeral)
```

On completion, workers rebase onto target and push. Conflicts surface early and often.

### Worker Lifecycle

```
Spawned → Running → Landing → Complete
              ↓
           Blocked (writes status, waits for help)
```

No WaitingReview state. No manual merge. Review happens once on the final branch.

### Runtime Status

Workers write `.jig/status.json` for visibility:

```json
{"status": "working", "message": null}
{"status": "blocked", "message": "Can't find auth module"}
{"status": "done", "message": null}
```

`jig ps` reads this. Simple enum, no progress tracking.

---

## Subtasks

### 1. Issue Schema and Parsing
Define YAML frontmatter schema for epics and subtasks. Parse in Rust.
- Fields: `epic`, `status`, `assigned`, `target`, `depends`
- Validation on parse
- CLI: `jig issues list`, `jig issues show <id>`

### 2. `jig plan` Command
Orchestrator reads epic, spawns planner agent, creates subtask issues.
```bash
jig plan issues/feature-auth.md
# → Creates issues/feature-auth-01-*.md, etc.
# → Creates target branch
# → Spawns workers (or waits for --auto)
```

### 3. `jig spawn --issue <path>`
Spawn worker assigned to specific issue file.
- Reads task from issue, not `--context`
- Sets `assigned` field in issue
- Worker knows its target branch from issue frontmatter

### 4. Auto-Landing
Worker skill/hook to land work on completion:
```bash
git fetch origin
git rebase origin/$TARGET
git push origin HEAD:$TARGET
```
- Handle conflicts (mark blocked, notify)
- Update issue status to `complete`

### 5. `jig ps` Status Integration
Read `.jig/status.json` from each worktree.
```
┌────────────┬──────────┬─────────┬──────────┐
│ Worker     │ Status   │ Issue   │ Changes  │
├────────────┼──────────┼─────────┼──────────┤
│ auth-01    │ working  │ #01-jwt │ +142 -23 │
│ auth-02    │ blocked  │ #02-oau │ —        │
└────────────┴──────────┴─────────┴──────────┘
auth-02 blocked: "Need clarification on OAuth flow"
```

### 6. `/jig` Skill for Workers
Skill that teaches workers the protocol:
- Read task from issue file
- Write status.json on state changes
- Auto-land when done
- How to signal blocked/question

### 7. `jig status` Command
Detailed view of epic execution:
```bash
jig status feature-auth
# Epic: feature-auth (3/4 complete)
# Target: feature/auth (+523 -89)
#
# ✓ 01-jwt      Complete
# ✓ 02-oauth    Complete
# ● 03-middleware  Running
# ○ 04-tests    Waiting (depends: 01, 02, 03)
```

---

## Success Criteria

- [ ] `jig plan` decomposes epic into subtask issues
- [ ] Workers read task from issue file, not --context
- [ ] Workers auto-land to shared target branch
- [ ] `jig ps` shows worker status from status.json
- [ ] Conflicts surface immediately (blocked status)
- [ ] Human reviews final branch once, not per-worker
- [ ] `jig spawn` still works for ad-hoc tasks (unchanged)

## Non-Goals

- Pool-based self-selection (overkill for our scale)
- Complex dependency resolution (keep it simple: linear or parallel)
- Automatic conflict resolution (surface it, don't solve it)

## Open Questions

1. How does planner agent get invoked? Claude session or subprocess?
2. Issue numbering scheme for subtasks?
3. Should `jig plan --auto` spawn immediately or require confirmation?
4. How to handle worker that finishes but can't land (conflicts)?
