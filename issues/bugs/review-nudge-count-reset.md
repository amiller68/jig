# Reset review nudge count when new comments arrive

**Status:** Done
**Priority:** High
**Auto:** true

## Objective

Reset the `review` nudge count when new review comments appear on a PR, so workers get fresh nudges for each round of feedback instead of exhausting their nudge budget across rounds.

## Context

Nudge counts are per-type and never reset. If a worker uses 2 of 3 nudges addressing round 1 of review feedback, it only has 1 nudge left for round 2. After that, the daemon goes silent and the worker sits ignoring feedback with no further prodding. This makes multi-round PR reviews unreliable.

## Implementation

### 1. Track seen review comment count in WorkerState

**`crates/jig-core/src/global/state.rs`** — Add `review_comment_count: Option<u32>` to `WorkerEntry`. Records how many review comments were present when the last review nudge cycle started.

### 2. Detect new comments in PR check

**`crates/jig-core/src/daemon/pr.rs`** — In `check_reviews()` (or wherever PR checks feed into dispatch), return the current comment count alongside the nudge type. Pass this through `WorkerTickInfo` or a new field on `PrCheck`.

### 3. Reset review nudge count on new comments

**`crates/jig-core/src/daemon/mod.rs`** — In `process_worker()`, before dispatch: if current review comment count > stored count, reset `nudge_counts["review"]` to 0 and update stored count. This gives the worker a fresh set of nudges for the new feedback.

### 4. Also reset on new `ChangesRequested` review

If a reviewer submits a new `ChangesRequested` review (even without new inline comments), the count should also reset. Track latest review timestamp or review count similarly.

## Files

- `crates/jig-core/src/global/state.rs` — Add `review_comment_count` to `WorkerEntry`
- `crates/jig-core/src/daemon/pr.rs` — Surface comment count from review check
- `crates/jig-core/src/daemon/mod.rs` — Reset logic in `process_worker()`
- `crates/jig-core/src/github/detect.rs` — Return comment count from `check_reviews()`

## Acceptance Criteria

- [ ] Review nudge count resets when new comments appear on the PR
- [ ] Review nudge count resets when a new `ChangesRequested` review is submitted
- [ ] Workers get `max_nudges` fresh nudges per round of feedback
- [ ] Existing nudge behavior for other types (idle, ci, conflict) unchanged
- [ ] `cargo build && cargo test && cargo clippy` passes

## Verification

```bash
cargo build && cargo test && cargo clippy

# Scenario:
# 1. Worker has PR with review comments, gets nudged 3x (max)
# 2. Reviewer adds new comment
# 3. Worker gets nudged again (count reset to 0, fresh cycle)
```
