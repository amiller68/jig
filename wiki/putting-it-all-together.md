---
layout: page
title: Putting It All Together
nav_order: 5
---

# Putting It All Together

You've read the concepts. You've installed jig. Now what does the actual workflow look like — end to end — when you sit down to ship a feature?

This page walks through a real session: writing issues, spawning agents, monitoring from your terminal, reviewing PRs, and merging. It's the workflow jig was built for.

## The loop

```
Think → Write tickets → Spawn agents → Monitor → Review → Merge → Repeat
```

Most of your time is spent on the first and last steps. The middle is where agents earn their keep.

## 1. Start with issues

Good tickets are the input. Whether you use file-based issues or Linear, the pattern is the same: write a clear description, call out the files involved, and define what "done" looks like.

### File-based

```bash
# Create from template
cp issues/_template.md issues/features/jwt-auth.md
```

```markdown
# Implement JWT authentication

**Status:** Planned
**Priority:** High
**Category:** features

## Objective

Add JWT-based auth to the API. Users POST credentials to /auth/login and
receive a token pair (access + refresh).

## Files to modify

- src/auth/tokens.rs
- src/auth/middleware.rs
- src/api/routes/auth.rs

## Acceptance criteria

- POST /auth/login returns access + refresh tokens
- Middleware validates access tokens on protected routes
- Refresh endpoint rotates tokens
- Unit tests for token generation and validation
- Integration test for the full login flow
```

### Linear

If your team uses Linear, point jig at it and skip the markdown:

```toml
# jig.toml
[issues]
provider = "linear"

[issues.linear]
profile = "work"
team = "ENG"
```

Then browse and spawn directly from Linear tickets:

```bash
jig issues                          # list from Linear
jig issues --status planned         # what's ready to pick up
jig issues ENG-123                  # view a specific ticket
```

See the [Linear Integration](/appendix/linear-integration) appendix for full setup.

### What makes a good ticket?

Agents work best with:

- **A one-line summary** — What is this?
- **Specific files** — Where should changes go?
- **Acceptance criteria** — How does the agent know it's done?
- **Context about patterns** — Link to relevant docs or examples

The more you invest in the ticket, the less you'll spend reviewing the output.

## 2. Spawn workers

Each spawn creates an isolated worktree, opens a tmux window, and starts an agent with the ticket as context.

```bash
# From a file-based issue
jig spawn jwt-auth --issue features/jwt-auth --auto

# From a Linear ticket
jig spawn jwt-auth --issue ENG-123 --auto

# With free-text context (no issue)
jig spawn fix-typos --context "Fix typos in the README and docs/" --auto

# Multiple workers in parallel
jig spawn jwt-auth --issue ENG-123 --auto
jig spawn pagination --issue ENG-124 --auto
jig spawn test-coverage --issue ENG-125 --auto
```

The `--auto` flag gives the agent full autonomy. It will:

1. Read the issue description
2. Plan its approach
3. Implement the changes
4. Commit with conventional commit format
5. Push and create a draft PR
6. Respond to nudges if it stalls

Without `--auto`, the agent starts in interactive mode and waits for your input. Good for exploratory or sensitive work.

## 3. Your terminal is mission control

This is where jig shines. You don't alt-tab between browser tabs or IDE windows. Everything is in the terminal.

### The dashboard

```bash
jig ps -w
```

This starts the live watch display:

```
jig ps --watch — 4 workers  (every 2s)

WORKER                STATE    COMMITS  PR     HEALTH  ISSUE
● jwt-auth            running        2  -      -       ENG-123
● pagination          running        0  -      -       ENG-124
● test-coverage       draft          3  #42    ci      ENG-125
● error-pages         review         5  #43    ok      ENG-126

                                                [l]ogs  [q]uit
```

At a glance you can see:

- **Which agents are active** — `running` means tool use is flowing
- **Who's stuck** — `stalled` means silence for 5+ minutes, the daemon will nudge
- **Draft vs review** — `draft` (blue) means the PR is a draft and the agent is still working; `review` (cyan) means the PR is ready for human review
- **PR health** — `ci` means checks are failing, `conflicts` means merge conflicts
- **Progress** — Commit count tells you how far along each worker is

### Attach to a worker

Want to see what an agent is doing? Drop into its tmux window:

```bash
jig attach jwt-auth
```

You're now watching the agent work in real time. Read its output, see its tool calls, intervene if needed. Detach with `Ctrl-b d` to return to your session.

### Log view

Press `l` in watch mode to see what the daemon is actually doing:

```
[14:32:05] tick: 3 workers, 1 action, 1 nudge, 0 errors
[14:32:05]   myrepo/jwt-auth PR: -
[14:32:05]   myrepo/test-coverage PR: ci
[14:32:35] tick: 3 workers, 0 actions, 0 nudges, 0 errors
```

This is useful for understanding why a worker got nudged or when a PR was discovered.

## 4. The daemon works for you

While you're writing the next batch of tickets — or reviewing code, or taking a break — the daemon is:

- **Detecting idle agents** and nudging them with status check messages
- **Monitoring PRs** for CI failures, merge conflicts, and review comments
- **Nudging draft PRs** — agents with draft PRs get nudged about CI failures, conflicts, and review comments so they can fix them autonomously
- **Leaving non-draft PRs alone** — once a PR is marked ready for review, nudges stop; the human is in control
- **Cleaning up** workers whose PRs get merged
- **Notifying you** when a worker has been nudged too many times and needs human attention

You don't need to babysit. The daemon handles the supervision loop.

### Draft vs review

The daemon treats draft and non-draft PRs differently:

| PR state | STATE column | Nudges? | Rationale |
|----------|-------------|---------|-----------|
| Draft | `draft` (blue) | Yes — CI, conflicts, reviews, commits | Agent is still working, can act on problems |
| Ready for review | `review` (cyan) | No | Human is reviewing, don't interrupt the agent |

This means the typical agent workflow is: work → push → create draft PR → fix any CI/conflict issues → mark ready for review → human takes over. The daemon nudges through the draft phase and backs off once the PR is promoted.

### Nudge escalation

Each nudge type (idle, CI, conflicts, reviews) has an independent counter. After 3 nudges of the same type (configurable), the daemon stops nudging and alerts you instead. This prevents infinite loops where an agent keeps failing at the same thing.

## 5. Review PRs

When a worker opens a PR, you'll see it in the dashboard. The HEALTH column tells you if it's ready for review or still has issues.

### Quick review from the terminal

```bash
# See the diff
jig review jwt-auth

# Or use gh directly
gh pr view --web
```

### What to look for

- **Does it match the ticket?** — Check against acceptance criteria
- **Does it follow patterns?** — Consistent with `PATTERNS.md`
- **Are there tests?** — Agents sometimes skip edge cases
- **No hallucinated requirements** — Agents occasionally add features nobody asked for
- **Security** — SQL injection, XSS, hardcoded secrets

### Requesting changes

If the PR is still a draft, leave review comments on GitHub. The daemon will detect unresolved comments and nudge the agent to address them. You don't need to manually tell the agent — the nudge includes the feedback.

Once the PR is marked ready for review, the daemon stops nudging. At that point, use `jig attach` to interact with the agent directly if you need changes.

### Approving and merging

```bash
# Approve via gh
gh pr review --approve

# Merge
jig merge jwt-auth

# Or merge via GitHub and let the daemon auto-cleanup
```

If `github.auto_cleanup_merged = true` (the default), the daemon will detect the merge, kill the tmux window, and archive the worker. No manual cleanup needed.

## 6. Repeat

The cycle is:

1. **Morning**: Review overnight PRs. Merge what's good. Leave comments on what's not.
2. **Write tickets**: Break down the next chunk of work into parallelizable issues.
3. **Spawn**: Launch 2-4 agents on independent tasks.
4. **Monitor**: Keep `jig ps -w` running. Intervene when needed.
5. **Review**: As PRs come in, review and merge or send back.
6. **End of day**: Check dashboard, clean up stale workers, plan tomorrow's tickets.

## Putting it all together: a real session

Here's what a productive 2-hour block looks like:

```bash
# You've got 4 tickets triaged and ready in Linear

# Spawn workers
jig spawn jwt-auth --issue ENG-123 --auto
jig spawn rate-limiting --issue ENG-124 --auto
jig spawn error-pages --issue ENG-125 --auto
jig spawn api-docs --issue ENG-126 --auto

# Open the dashboard
jig ps -w

# While agents work, you:
# - Write the next batch of tickets in Linear
# - Review a colleague's PR
# - Respond to Slack messages
# - Think about architecture for next sprint

# 20 minutes later: jwt-auth has a PR, CI is green
gh pr view jwt-auth --web
# Looks good — approve and merge
gh pr review 42 --approve
gh pr merge 42

# rate-limiting is stalled — attach and check
jig attach rate-limiting
# Agent is confused about middleware ordering. Send it a hint.
# Detach, let it continue.

# error-pages has a PR but CI is failing
# No action needed — daemon already nudged the agent about it

# api-docs finished, PR looks clean
gh pr merge 44

# 90 minutes in: 2 merged, 1 close to done, 1 needs another nudge
# You've also written 3 more tickets for tomorrow
```

Four features shipped in under two hours, and most of your time was spent on review and planning — not implementation.

## Tips for success

- **Keep tasks independent.** Agents working on overlapping files create merge conflicts.
- **Start small.** Spawn 2 workers your first time. Scale up as you build confidence.
- **Invest in docs.** `PATTERNS.md` and `CLAUDE.md` pay compounding returns across every agent session.
- **Use auto mode.** Interactive mode is for exploratory work. For well-scoped tickets, let agents run.
- **Trust the daemon.** It will nudge stuck agents. Don't micro-manage unless the dashboard says there's a problem.
- **Write better tickets, not more tickets.** One clear ticket beats three vague ones.
