# Review System Behavior

## Trigger Flow

```
Worker pushes to draft PR branch
        |
        v
Daemon tick detects HEAD != last_reviewed_sha
        |
        v
    Review enabled?  --no--> skip
        |yes
        v
    PR is draft?  --no--> skip (human review territory)
        |yes
        v
    Review already in flight?  --yes--> skip
        |no
        v
    At max rounds?  --yes--> NeedsIntervention notification
        |no
        v
    Send ReviewRequest to review actor
        |
        v
    Review actor runs ephemeral Claude Code session
        |
        v
    Agent writes review via `jig review submit`
        |
        v
    Daemon drains ReviewComplete
        |
        v
    Read verdict from `.jig/reviews/NNN.md`
        |
   +----+----+
   |         |
Approve   ChangesRequested
   |         |
   v         v
gh pr    Send AutoReview
ready    nudge to worker
   |         |
   v         v
Update   Worker reads
issue    review, fixes,
status   responds, pushes
         → next cycle
```

## Review Lifecycle

### 1. Trigger (daemon tick loop)

For each worker with review enabled:
- Check if HEAD SHA differs from `last_reviewed_sha` in `WorkerEntry`
- Check PR is in draft state
- Check no review already pending (prevents duplicate review runs)
- Check review count < `max_rounds`
- If all pass: send `ReviewRequest` to the review actor

### 2. Review Execution (review actor)

The review actor:
1. Validates the worktree exists
2. Gets the current branch via `git rev-parse`
3. Computes the diff: `git diff <base_branch>...HEAD`
4. Loads prior review history from `.jig/reviews/`
5. Builds a prompt with project conventions
6. Runs an ephemeral Claude Code session with restricted tools
7. Validates a new review file was created

### 3. Verdict Processing (daemon)

After draining `ReviewComplete`:

**Approve:**
- Run `gh pr ready` to mark PR ready for review
- Emit `ReviewApproved` notification
- Update issue status to "In Review" if issue reference exists

**ChangesRequested:**
- Build `AutoReview` nudge context with review-specific fields
- Render `nudge-auto-review` template
- Send nudge to worker's tmux session via nudge actor
- Update `last_reviewed_sha` in `WorkerEntry`

**Max rounds reached:**
- Emit `NeedsIntervention` notification with context

### 4. Worker Response

The implementation agent receives the `AutoReview` nudge, which instructs it to:
1. Read the review at `.jig/reviews/NNN.md`
2. Fix issues or prepare dispute explanations
3. Write a response via `jig review respond --review N`
4. Commit and push — triggering the next review cycle

### 5. Next Cycle

On push, the daemon detects HEAD changed and triggers a new review. The review agent sees both the previous review and the response, preventing re-raising of addressed or reasonably disputed findings.

## Comment Routing Matrix

```
                    PR in Draft              PR Ready for Review
                    -----------              -------------------
Review ON     Human comments: HELD       Human comments: nudged to worker
              Review agent: ACTIVE        Review agent: INACTIVE

Review OFF    Human comments: nudged      Human comments: not nudged
              Review agent: N/A           Review agent: N/A
```

When review is on and PR is in draft, the review agent is the gatekeeper. Human comments on draft PRs are held (not nudged to worker) to avoid conflicting with the automated review process. Once the PR exits draft (either via auto-approve or manual), the review agent becomes inactive and human review takes over.

## Nudge Details

### AutoReview Nudge

Template: `nudge-auto-review`

Context variables:
| Variable | Type | Description |
|----------|------|-------------|
| `review_round` | number | Current review cycle number |
| `review_file` | string | Filename (e.g., "002.md") |
| `review_number` | number | Numeric review ID |
| `max_rounds` | number | Configured max rounds |
| `is_final_round` | bool | True if at max_rounds |
| `nudge_count` | number | Current nudge count |
| `max_nudges` | number | Max nudges for this type |
| `is_final_nudge` | bool | True if at max nudges |

### Review Nudge Suppression

When automated review is enabled and the PR is in draft:
- Human `reviews` comments are suppressed (review agent is gatekeeper)
- Other nudge types (CI, conflict, bad-commits) still apply

When the PR exits draft:
- Human review comments trigger the standard `Review` nudge
- Review agent becomes inactive
