# Review Implementation Details

Per-area implementation details for the automated review system.

## Data Model (JIG-21)

**Files:** `crates/jig-core/src/review.rs`, `crates/jig-core/src/config.rs`

The `Review` and `ReviewResponse` types handle bidirectional markdown serialization. Key design points:

- All six `ReviewCategory` variants are required in every review — `from_markdown` returns `MissingSection` if any are absent
- Section `status` is derived from the worst severity among findings (Pass < Warn < Fail)
- `ReviewVerdict::from_str` is case-insensitive and handles both `approve` and `changes_requested`
- Path helpers (`next_review_path`, `review_count`, etc.) use filesystem scanning, not state files

## CLI Commands (JIG-22)

**Files:** `crates/jig-cli/src/commands/review.rs`, `review/submit.rs`, `review/respond.rs`

### `jig review submit`

1. Reads markdown from stdin
2. Parses with `Review::from_markdown()` (validates format, returns specific errors)
3. Creates `.jig/reviews/` if needed
4. Computes next filename via `next_review_path()`
5. Re-serializes to normalize formatting
6. Writes to disk

### `jig review respond --review N`

1. Validates review file `NNN.md` exists
2. Reads response markdown from stdin
3. Parses with `ReviewResponse::from_markdown()`
4. Writes to `NNN-response.md`

### `jig review show <name>` (backward compat)

Shows diff for a worktree. `jig review <name>` is preserved as shorthand.

## Adapter Ephemeral Mode (JIG-24)

**Files:** `crates/jig-core/src/adapter.rs`

The `AgentAdapter` struct has an `ephemeral_flags` field:
- Claude Code: `"--print --no-session-persistence --dangerously-skip-permissions"`
- `supports_ephemeral()` returns true when flags are non-empty
- `build_ephemeral_command(prompt, allowed_tools)` constructs the full shell command

This is a first-class abstraction, not Claude Code-specific. Future adapters can set their own ephemeral flags.

## Review Actor (JIG-25)

**Files:** `crates/jig-core/src/daemon/review_actor.rs`

Follows the standard actor pattern:
- Background thread with flume channels
- Receives `ReviewRequest`, returns `ReviewComplete`
- Runs `run_review_inner()` which:
  1. Gets branch and diff
  2. Loads review history
  3. Builds prompt with conventions reference
  4. Calls `build_ephemeral_command()` with restricted tools
  5. Validates review file count increased

Allowed tools for the review agent:
- `Read`, `Grep`, `Glob` (code navigation)
- `Bash(jig review submit:*)` (submit review)
- `Bash(git diff:*)`, `Bash(git log:*)` (git queries)

## AutoReview Nudge (JIG-25)

**Files:** `crates/jig-core/src/nudge.rs`, `crates/jig-core/src/templates/builtin.rs`

`NudgeType::AutoReview`:
- Template: `nudge-auto-review`
- Count key: `auto_review`
- Not classified by `classify_nudge()` — dispatched directly by the daemon after review completion

The nudge template instructs the worker to:
1. Read the review file
2. Fix issues or prepare disputes
3. Respond via `jig review respond --review N`
4. Commit and push

## Daemon Integration (JIG-26)

**Files:** `crates/jig-core/src/daemon/mod.rs`, `crates/jig-core/src/daemon/runtime.rs`

### Runtime Plumbing

- `review_tx` / `review_rx`: flume channels for ReviewRequest/ReviewComplete
- `review_pending`: HashMap tracking in-flight reviews by worker_key
- `send_review()`: sends request, prevents duplicates
- `drain_reviews()`: non-blocking drain of completed reviews

### Review Dispatch (tick loop)

For each worker where review is enabled, PR is draft, and no review pending:
1. Compare HEAD SHA to `last_reviewed_sha`
2. If different and under max rounds: send `ReviewRequest`
3. If different and at max rounds: emit `NeedsIntervention`

### Review Completion (tick loop, before worker processing)

1. Drain `ReviewComplete` responses
2. Read verdict from latest review file
3. On Approve: `gh pr ready`, emit notification, update issue status
4. On ChangesRequested: build and send AutoReview nudge
5. Update `last_reviewed_sha` in WorkerEntry

### Comment Routing

When processing GitHub review comments in the dispatch loop:
- If review is enabled and PR is draft: suppress human review nudges (review agent is gatekeeper)
- Otherwise: dispatch standard Review nudge

### State Tracking

`WorkerEntry` fields for review:
- `last_reviewed_sha`: SHA of last reviewed commit
- `review_feedback_count`: tracks human review comments for nudge reset

## Skills and Templates (JIG-23)

**Files:** `templates/skills/review/SKILL.md`

The `/review` skill serves manual reviews. The automated review system uses the review actor's built-in prompt, not this skill. The skill references the automated review system for context.

## Notification Events

- `ReviewApproved { pr_url }` — emitted when review verdict is Approve
- `NeedsIntervention` — emitted when max review rounds reached without approval
