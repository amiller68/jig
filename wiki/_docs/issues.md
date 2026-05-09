---
title: Issues & Linear
slug: issues
date: 2025-05-09
---

Issues are a first-class concept in jig. Well-scoped tickets are the input to agent work — `jig issues` and `jig spawn --issue` integrate issue tracking directly into the spawn pipeline.

## Why first-class issues?

Instead of copy-pasting ticket descriptions into agent prompts, jig fetches them for you:

```bash
jig spawn auth-jwt --issue ENG-123```

The agent gets the full issue description as its working context. The daemon can auto-spawn workers from your issue board. Dependencies between issues control spawn order. It's the bridge between your planning workflow and parallel agent execution.

## Setup

### 1. Create an API key

Go to **Linear > Settings > API > Personal API keys** and create a key.

### 2. Add it to global config

```toml
# ~/.config/jig/config.toml

[linear.profiles.work]
api_key = "lin_api_xxxxxxxxxxxx"
team = "ENG"                          # default team filter
projects = ["Backend", "Platform"]    # default project filter
assignee = "me"                       # "me" = API key owner
labels = ["auto"]                     # default label filter
```

API keys live in global config — never in committed files.

### 3. Point your repo at Linear

```toml
# jig.toml

[issues]
provider = "linear"

[issues.linear]
profile = "work"
# team = "ENG"               # override profile default
# projects = ["Backend"]     # narrow profile default
```

Per-repo fields override profile-level defaults.

### Multiple profiles

For multiple Linear workspaces:

```toml
# ~/.config/jig/config.toml

[linear.profiles.work]
api_key = "lin_api_xxxxxxxxxxxx"

[linear.profiles.oss]
api_key = "lin_api_yyyyyyyyyyyy"
```

Reference by name: `profile = "oss"`.

### Profile-level filters

Profiles carry default filters so you don't repeat config in every repo:

```toml
[linear.profiles.work]
api_key = "lin_api_xxxxxxxxxxxx"
team = "ENG"
projects = ["Backend", "Platform"]
assignee = "me"
labels = ["auto"]
```

Per-repo `jig.toml` overrides these:

| Field | Resolution |
|-------|-----------|
| `team` | jig.toml > profile > *(error if missing)* |
| `projects` | jig.toml > profile > *(no filter)* |
| `assignee` | jig.toml > profile > *(no filter)* |
| `labels` | jig.toml > profile > *(no filter)* |

`assignee = "me"` resolves to the API key owner via Linear's `viewer` query.

## Day-to-day usage

```bash
# Browse issues
jig issues

# Filter
jig issues --status planned --priority high
jig issues --category Backend
jig issues --label backend --label sprint-1

# View a ticket
jig issues ENG-123

# Spawn with full Linear context
jig spawn auth-jwt --issue ENG-123```

### Creating issues

```bash
jig issues create "Fix auth crash" \
  --body "Stack trace shows null pointer in session handler" \
  --priority high \
  --label backend --label bug \
  --category Backend

# Body from stdin
echo "detailed description" | jig issues create "Fix auth crash" --body -

# Create as sub-issue
jig issues create "Review data model" --parent ENG-19
```

### Managing dependencies

```bash
# Add blockers
jig issues update ENG-22 --blocked-by ENG-21
jig issues update ENG-22 --blocked-by ENG-21,ENG-24

# Remove a blocker
jig issues update ENG-22 --remove-blocked-by ENG-21

# View (shown in single issue view)
jig issues ENG-22
# Blocked by: ENG-21, ENG-24
```

### Updating issues

```bash
# Append to description
jig issues update ENG-123 --body "## Findings..." --append

# Change status
jig issues status ENG-123 backlog
jig issues status ENG-123 planned

# Set/remove parent
jig issues update ENG-21 --parent ENG-19
jig issues update ENG-21 --remove-parent
```

## Auto-spawn

The daemon automatically spawns workers for issues in **Planned** status with all dependencies satisfied.

### Labels

Labels are fetched from Linear's native label system. Filter with `--label`:

```bash
jig issues --label backend                    # issues with "backend" label
jig issues --label backend --label sprint-1   # must have BOTH labels
```

## Triage

The daemon can auto-triage new issues. When an issue enters Linear's **Triage** status, jig spawns a lightweight read-only agent to investigate the codebase and enrich the issue.

```toml
[triage]
enabled = true
model = "sonnet"
timeout_seconds = 600
```

### How it works

1. Issue enters Triage status in Linear
2. Daemon spawns a one-shot subprocess agent (no worktree, no tmux)
3. Agent reads the codebase with restricted tools (`Read`, `Glob`, `Grep`, `Bash(jig *)`)
4. Agent appends structured findings to the issue description
5. Agent transitions the issue to **Backlog**
6. Human reviews the enriched issue and decides next steps

The triage agent produces a structured analysis: affected files, proposed approach, complexity assessment, suggested priority, and risks. This gets appended to the Linear issue body.

### Triage vs spawn

| | Triage | Spawn |
|---|--------|-------|
| **Trigger** | Issue in Triage status | Issue in Planned status |
| **Execution** | One-shot subprocess | Persistent tmux session |
| **Worktree** | None | Dedicated git worktree |
| **Tools** | Read-only | Full access |
| **Output** | Updates issue description | Commits code, opens PR |
| **Duration** | Minutes | Hours to days |

An issue flows: **Triage** → Backlog → *(human promotes)* → **Planned** → Spawn.

## Status mapping

| Linear state type | jig status |
|-------------------|------------|
| `triage` | Triage |
| `backlog` | Backlog |
| `unstarted` | Planned |
| `started` | In Progress |
| `completed` | Complete |
| `canceled` | Complete |

## Priority mapping

| Linear priority | jig priority |
|----------------|-------------|
| 1 (Urgent) | Urgent |
| 2 (High) | High |
| 3 (Normal) | Medium |
| 4 (Low) | Low |
| 0 (None) | *(omitted)* |

## Troubleshooting

**"Linear profile 'X' not found"** — Check spelling in `jig.toml` against `~/.config/jig/config.toml` profile names.

**No issues returned** — Verify the `team` key matches your Linear team key (e.g. `ENG`, not the full name). Check `projects` if set.

**Authentication errors** — Regenerate your API key from Linear settings.
