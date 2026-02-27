---
layout: page
title: Autonomous Orchestration
nav_order: 1
parent: Appendix
---

# Autonomous Orchestration

jig includes a built-in daemon that supervises workers, monitors their PRs, and intervenes when things go wrong — no cron jobs, no bash scripts, no manual babysitting.

This page explains how it all works: spawn, the event system, the daemon tick loop, nudges, PR lifecycle monitoring, and how `jig ps --watch` ties it together.

## The big picture

```
You write a ticket
  → jig spawn launches an agent in a tmux window
    → Agent works autonomously (commits, pushes, opens PR)
      → Daemon watches activity via events
        → If idle/stuck: nudge the agent via tmux
        → If PR has issues: nudge about CI/conflicts/reviews
        → If max nudges exceeded: notify you
        → If PR merged/closed: clean up automatically
```

You stay in the loop through `jig ps --watch`, which shows a live dashboard of all workers, their state, and PR health.

## Spawn: launching autonomous workers

`jig spawn` creates a git worktree, opens a tmux window, and starts an agent session:

```bash
# Spawn with free-text context
jig spawn feature-auth --context "Implement JWT authentication"

# Spawn from an issue file
jig spawn feature-auth --issue features/jwt-auth

# Spawn in auto mode (fully autonomous, no human interaction)
jig spawn feature-auth --issue features/jwt-auth --auto
```

### Auto mode

The `--auto` flag (or `spawn.auto = true` in `jig.toml`) launches the agent with `--dangerously-skip-permissions` and a structured preamble that tells the agent:

- It's autonomous — don't ask for confirmation, don't enter plan mode
- Its goal is to complete the task and create a draft PR
- A daemon is watching — if it goes idle for ~5 minutes, it'll get nudged
- If it's truly stuck, it should explain what's blocking it

This preamble is a Handlebars template (`spawn-preamble`) that can be customized per-repo.

### What spawn registers

When you spawn a worker, jig:

1. Creates the worktree and branch
2. Records the worker in `.jig/state.json` (local orchestrator state)
3. Emits a `Spawn` event to the worker's event log
4. Opens a tmux window in a `jig-<repo>` session
5. Sends the agent command to the window

## The event system

Every worker has a JSONL event log at `~/.config/jig/events/<repo>-<worker>/events.jsonl`. Events are appended by git hooks (post-commit, post-merge) and by the daemon itself.

### Event types

| Event | Source | Meaning |
|-------|--------|---------|
| `Spawn` | `jig spawn` | Worker created |
| `ToolUseStart` / `ToolUseEnd` | Claude hooks | Agent is actively using tools |
| `Commit` | post-commit hook | Code committed |
| `Push` | post-commit hook | Code pushed |
| `PrOpened` | Daemon discovery | PR found for the branch |
| `Notification` | Agent | Agent hit an interactive prompt |
| `Stop` | Agent exit | Agent session ended |
| `Nudge` | Daemon | Nudge delivered |
| `Terminal` | Daemon | Worker cleaned up |

### State derivation

Worker state is derived by replaying the event stream — there's no mutable state database. The reducer walks events in order and produces a `WorkerState`:

| Status | Meaning |
|--------|---------|
| `spawned` | Just created, no activity yet |
| `running` | Tool use events flowing |
| `idle` | Agent exited, at shell prompt |
| `waiting` | Agent hit an interactive prompt |
| `stalled` | No events for 5+ minutes (configurable) |
| `review` | PR opened, waiting on human review |
| `approved` | PR approved |
| `merged` | PR merged (terminal) |
| `failed` | Error or killed (terminal) |
| `archived` | Cleaned up (terminal) |

The silence check is key: if no events arrive for `silence_threshold_seconds` (default 300s), and the worker isn't terminal or in review, it transitions to `stalled`. This is what triggers idle nudges.

## The daemon tick loop

The daemon runs a periodic loop (default every 30 seconds). Each tick:

1. **Sync repos** — `git fetch` the base branch for each registered repo
2. **Discover workers** — Scan `~/.config/jig/events/` for active event logs
3. **For each worker:**
   - Read the event log and derive current state
   - Discover PRs if not already known (via `gh pr list`)
   - Compare against previous state (from `~/.config/jig/workers.json`)
   - Dispatch actions based on state transitions
   - Check PR lifecycle (CI, conflicts, reviews, commits)
   - Execute actions (nudge, notify, cleanup)
4. **Save state** — Write updated `workers.json`

### Running the daemon

The daemon runs in two ways:

- **`jig ps --watch`** — Runs the tick loop inline, displaying a live table with keypress-driven log toggle. Best for active supervision.
- **`jig daemon`** — Runs headless. Good for persistent background supervision.

Both share the same `run_with()` entrypoint — `ps --watch` just adds the display layer and keypress handling.

## Nudges: intervening when agents get stuck

Nudges are messages sent to agents via `tmux send-keys`. The daemon classifies what kind of nudge is needed and renders a template with contextual information.

### Nudge types

| Type | Trigger | What it does |
|------|---------|--------------|
| **idle** | Worker is `stalled` or `idle`, no PR | Asks for a status update. If there are uncommitted changes, pushes toward committing and opening a PR. |
| **stuck** | Worker is `waiting` (interactive prompt) | Sends an auto-approve keystroke to dismiss the prompt, then a message. |
| **ci** | CI failing on open PR | Lists the failing checks, tells the agent to fix and push. |
| **conflict** | Merge conflicts on open PR | Tells the agent to rebase and resolve. |
| **review** | Unresolved review comments on PR | Tells the agent to address feedback. |
| **bad-commits** | Non-conventional commits on PR | Lists the bad commits, tells the agent to reword them. |

### Escalation

Each nudge type has an independent counter. After `max_nudges` (default 3) of the same type, the daemon stops nudging and fires a `Notify` action instead — alerting you that the worker needs human attention.

The nudge templates are Handlebars and can be overridden per-repo by placing custom templates in your repo's template directory.

### Example: idle nudge

```
STATUS CHECK: You've been idle for a while (nudge 2/3).

You have uncommitted changes but no PR yet. What's blocking you?

1. If ready: commit (conventional format), push, create PR, update issue, call /review
2. If stuck: explain what you need help with
3. If complete but confused: finish the PR
```

### Example: CI nudge

```
CI is failing on your PR (nudge 1/3).

Fix these issues:
  - lint: cargo clippy found 3 warnings
  - test: 2 tests failing in auth module

STEPS:
1. Fix the failing checks
2. Commit using conventional commits: fix(ci): fix linting errors
3. Push to your branch: git push
4. Verify CI passes
5. Call /review when green
```

## PR lifecycle monitoring

When a worker has an open PR, the daemon runs four checks every tick:

### CI status
Queries GitHub for check run results. If any required check is failing, fires a `ci` nudge with the failure details.

### Merge conflicts
Checks the PR's `mergeable` state. If the PR has conflicts with the base branch, fires a `conflict` nudge.

### Review comments
Checks for unresolved review comments or changes-requested reviews. If found, fires a `review` nudge.

### Commit format
Validates that all commits follow conventional commit format (`type(scope): description`). Non-conforming commits trigger a `bad-commits` nudge.

### PR state transitions

The daemon also handles terminal PR states:

- **Merged** — If `github.auto_cleanup_merged` is true (default), kills the tmux window and emits a `Terminal` event. Sends a notification.
- **Closed without merge** — Sends a notification. If `github.auto_cleanup_closed` is true, also cleans up.

### PR discovery

Workers don't need to tell jig about their PR. The daemon proactively checks GitHub for PRs matching the worker's branch name. When found, it emits a `PrOpened` event and the worker transitions to `review` status.

## The watch display

`jig ps --watch` shows a live table updated every tick:

```
jig ps --watch — 3 workers  (every 2s)

NAME            TMUX  STATE    ISSUE       BRANCH           COMMITS  DIRTY  NUDGES  PR    HEALTH
feature-auth    ●     running  jwt-auth    feature-auth           3    -       -     #42   ok
fix-pagination  ●     stalled  fix-page    fix-pagination         1    ●       2     #43   ci conflicts
add-tests       ○     idle     add-tests   add-tests              0    -       -     -     -

                                                                              [l]ogs  [q]uit
```

Column meanings:

| Column | Description |
|--------|-------------|
| **TMUX** | Is the tmux window alive? `●` running, `○` exited, `✗` missing |
| **STATE** | Derived worker status from the event stream |
| **ISSUE** | Linked issue reference |
| **COMMITS** | Commits ahead of base branch |
| **DIRTY** | Uncommitted changes in the worktree |
| **NUDGES** | Total nudges sent across all types |
| **PR** | PR number if one exists |
| **HEALTH** | PR check results: `ok` (all green), problem names in red, `?` if GitHub unavailable, `-` if no PR |

The HEALTH column gives you at-a-glance visibility into what's wrong with each worker's PR, independent of whether nudges have fired.

### Log view

Press `l` in watch mode to switch to the log view. This shows timestamped daemon activity — which nudges fired, PR check results, errors — so you can see what the daemon is actually doing:

```
jig ps --watch — logs  (every 2s)

[14:32:05] tick: 3 workers, 1 action, 1 nudge, 0 errors
[14:32:05]   myrepo/feature-auth PR: ok
[14:32:05]   myrepo/fix-pagination PR: ci, conflicts
[14:32:35] tick: 3 workers, 0 actions, 0 nudges, 0 errors
[14:32:35]   myrepo/feature-auth PR: ok
[14:32:35]   myrepo/fix-pagination PR: ok

                                                    [t]able  [q]uit
```

Press `t` or `l` again to switch back to the table. Press `q` to quit cleanly.

## Global configuration

The daemon reads `~/.config/jig/config.toml`:

```toml
[health]
silence_threshold_seconds = 300  # 5 minutes before "stalled"
max_nudges = 3                   # per nudge type before escalation

[github]
auto_cleanup_merged = true       # kill workers when PR merges
auto_cleanup_closed = false      # kill workers when PR closed without merge

[notify]
exec = "notify-send 'jig' '$MESSAGE'"  # shell command for notifications
# webhook = "https://hooks.slack.com/..." # or a webhook URL
# events = ["worker.done"]              # filter which events trigger
```

### State files

| Path | Purpose |
|------|---------|
| `~/.config/jig/config.toml` | Global daemon configuration |
| `~/.config/jig/workers.json` | Last-known state of all workers (for diff-based dispatch) |
| `~/.config/jig/events/<repo>-<worker>/events.jsonl` | Per-worker event stream |
| `~/.config/jig/notifications.jsonl` | Notification log |

## Putting it all together

A typical autonomous workflow:

```bash
# 1. Write issues with clear scope and acceptance criteria
# 2. Spawn workers
jig spawn feature-auth --issue features/jwt-auth --auto
jig spawn fix-pagination --issue bugs/pagination --auto
jig spawn add-tests --issue tasks/test-coverage --auto

# 3. Watch them work
jig ps -w

# 4. Workers autonomously:
#    - Read the issue, plan, implement
#    - Commit with conventional format
#    - Push and create draft PRs
#    - Get nudged if they stall
#    - Fix CI failures, conflicts, review comments

# 5. You review PRs when they show up
#    - HEALTH column tells you what's ready
#    - Attach to a worker if you need to intervene: jig attach feature-auth
#    - Merge when satisfied

# 6. Daemon auto-cleans merged workers
```

The daemon handles the supervision loop that used to require manual checking, cron scripts, or custom bash automation. You focus on writing good tickets and reviewing good code.

## Customization

### Custom nudge templates

Override any built-in template by placing a file in your repo's `.jig/templates/` directory:

```
.jig/templates/
├── nudge-idle.hbs
├── nudge-ci.hbs
└── spawn-preamble.hbs
```

Available template variables vary by nudge type but always include `nudge_count`, `max_nudges`, and `is_final_nudge`.

### Notification hooks

The `[notify]` config supports shell commands and webhooks. The command receives a JSON payload on stdin with event details:

```toml
[notify]
exec = "jq -r '.reason' | terminal-notifier -title 'jig'"
```

### Tuning thresholds

- **Lower `silence_threshold_seconds`** if your agents are fast and you want quicker intervention
- **Raise `max_nudges`** if agents often recover on their own after a few tries
- **Set `auto_cleanup_closed = true`** if you want aggressive cleanup of abandoned work
