# Automated Review System

Design documents for the automated code review system in jig.

## Overview

A review agent runs ephemerally in the worker's worktree after each push to a draft PR. It writes structured markdown findings, and the daemon orchestrates the feedback loop until the code is approved or escalated to a human.

## Key Decisions

1. **Reviews are markdown files** in the worktree at `.jig/reviews/`, not GitHub PR comments. This keeps review history local, avoids the disconnected-comment problem from the `review-experiment` branch, and gives both agents direct filesystem access.

2. **Review agent runs ephemerally** using `--print --no-session-persistence --dangerously-skip-permissions` with restricted `--allowed-tools`. No persistent session, no conflict with the implementation agent's resumable session.

3. **Format validation via CLI** (`jig review submit` / `jig review respond`), not string matching. The agent gets immediate feedback on format errors and can retry.

4. **Bidirectional communication** prevents infinite loops. The review agent writes findings; the implementation agent responds with Addressed/Disputed/Deferred per finding. The next review cycle sees both, so dismissed suggestions aren't re-raised.

5. **Daemon orchestrates** transitions: trigger on HEAD change, dispatch nudge on ChangesRequested, mark PR ready on Approve, escalate at max rounds.

## Documents

| Document | Contents |
|----------|----------|
| [data-model.md](./data-model.md) | Types, file layout, markdown format |
| [behavior.md](./behavior.md) | Trigger flow, comment routing, interaction loop |
| [implementation.md](./implementation.md) | Per-area implementation details |

## Configuration

Opt-in per repo via `jig.toml`:

```toml
[review]
enabled = true
max_rounds = 5
# model = "sonnet"  # optional model override
```

## Lessons from the Experiment

The `alex/review-experiment` branch (commit `88ba985`) tried automated review via GitHub PR comments. It failed because:

1. GitHub comments were disconnected from the worker's tmux session
2. No memory between review cycles led to infinite loops
3. No execution environment for the review agent to validate suggestions
4. Each review cycle overwrote the previous one

Bug fixes from that branch were cherry-picked in commit `6d4388a` (JIG-20).
