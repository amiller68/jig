---
title: PRs & Review
slug: prs
date: 2025-05-09
---

How PRs flow through jig — from agent draft to human merge.

## Creating PRs

From inside a worktree:

```bash
jig pr
```

This pushes the branch and creates a draft PR. `jig pr` auto-detects parent issue relationships and targets the correct base branch — the parent's integration branch for child issues, or the repo's base branch otherwise.

## The PR lifecycle

```text
Agent works → Pushes → Opens draft PR → Daemon monitors → Agent fixes issues → You review → Merge
```

### Draft vs ready for review

The daemon treats draft and non-draft PRs differently:

| PR state | Worker STATE | Daemon nudges? | Rationale |
|----------|-------------|----------------|-----------|
| Draft | `draft` | Yes — CI, conflicts, reviews, commits | Agent is still working |
| Ready for review | `review` | No | Human is reviewing |

The typical flow: agent works → pushes → creates draft PR → fixes CI/conflict issues → marks ready for review → human takes over.

### What the daemon monitors

For every worker with an open PR, the daemon checks on each tick:

- **CI status** — Queries GitHub for check run results. Failing checks trigger a `ci` nudge with failure details.
- **Merge conflicts** — Checks the PR's `mergeable` state. Conflicts trigger a `conflict` nudge.
- **Review comments** — Unresolved comments or changes-requested reviews trigger a `review` nudge.
- **Commit format** — Non-conventional commits trigger a `bad-commits` nudge.

The HEALTH column in `jig ps` shows these at a glance: `ok` (all green), problem names in red, or `-` if no PR.

### PR discovery

Workers don't need to tell jig about their PR. The daemon proactively checks GitHub for PRs matching the worker's branch name and emits a `PrOpened` event when found.

## Reviewing

### Check review feedback

```bash
jig pr comments              # show review comments on current branch's PR
jig pr comments --pr 42      # specific PR number
jig pr comments --between HEAD~3..HEAD  # filter to files changed in range
```

This fetches review-level feedback (approvals, change requests) and inline comments from unresolved threads, formatted for the terminal.

### What to look for

- **Does it match the ticket?** — Check against acceptance criteria
- **Does it follow patterns?** — Consistent with your documented conventions
- **Are there tests?** — Agents sometimes skip edge cases
- **No hallucinated requirements** — Agents occasionally add features nobody asked for
- **Security** — SQL injection, XSS, hardcoded secrets

### Requesting changes

If the PR is still a **draft**, leave review comments on GitHub. The daemon detects unresolved comments and nudges the agent to address them — you don't need to manually tell the agent.

Once the PR is **ready for review**, the daemon stops nudging. Use `jig attach` to interact with the agent directly if you need further changes.

## PR base resolution

`jig pr` resolves the base branch automatically:

1. Looks up the current worktree's linked issue
2. If the issue has a parent with a `branch_name`, uses the parent branch as base
3. Falls back to the repo's configured base branch (usually `origin/main`)

This means child PRs always target the parent's integration branch — no `--base` flag needed. See [Parent-Child](/docs/parent-child/) for the full epic workflow.
