---
layout: page
title: Linear Integration
nav_order: 2
parent: Appendix
---

# Linear Integration

jig ships with a built-in Linear provider so teams already using Linear for project management can skip file-based issues entirely. Once configured, `jig issues` and `jig spawn --issue` talk directly to the Linear API.

## Why integrate?

File-based issues work well for solo projects and repos where agents manage their own backlog. But many teams already have a Linear board with triaged, prioritized work. Duplicating that into markdown files is busywork.

With the Linear provider:

- **No duplication** — Your Linear board is the source of truth
- **Spawn from tickets** — `jig spawn auth-jwt --issue ENG-123` pulls the description straight from Linear
- **Filter naturally** — `jig issues --status planned --priority high` queries Linear directly
- **Same workflow** — Everything else (worktrees, daemon, PR monitoring) works identically

## Quick setup

### 1. Get an API key

Linear > Settings > API > Personal API keys. Create one.

### 2. Add it to global config

```toml
# ~/.config/jig/config.toml

[linear.profiles.work]
api_key = "lin_api_xxxxxxxxxxxx"
team = "ENG"                          # default team filter
projects = ["Backend", "Platform"]    # default project filter
assignee = "me"                       # "me" = API key owner; or use email
```

API keys live in the global config — never in committed files. The `team`, `projects`, and `assignee` fields are optional profile-level defaults that apply to every repo using this profile.

### 3. Point your repo at Linear

```toml
# jig.toml

[issues]
provider = "linear"

[issues.linear]
profile = "work"
# team = "ENG"               # override profile default
# projects = ["Backend"]     # narrow profile default
# assignee = "alice@co.com"  # override profile default
```

Per-repo fields override profile-level defaults. If the profile already has `team` set, you can omit it from `jig.toml`. `jig issues` now lists your Linear tickets.

## Day-to-day usage

```bash
# Browse your team's issues
jig issues

# Filter to what's ready to work on
jig issues --status planned --priority high

# Look at a specific ticket
jig issues ENG-123

# Spawn a worker with full Linear context
jig spawn auth-jwt --issue ENG-123 --auto

# The agent gets the issue title + description as its context
# and works autonomously from there
```

### Spawning with `--issue`

When you pass `--issue ENG-123`, jig fetches the issue from Linear and passes the full description as the agent's working context. The agent sees:

```markdown
# Implement JWT authentication

Add JWT-based authentication to the API...

## Acceptance Criteria
- POST /auth/login returns JWT
- Middleware validates tokens
- Tests cover happy path and errors
```

This is the same experience as file-based issues — the agent gets a markdown body with title and description — but sourced from Linear.

## Multiple profiles

If you work across multiple Linear workspaces, set up named profiles:

```toml
# ~/.config/jig/config.toml

[linear.profiles.work]
api_key = "lin_api_xxxxxxxxxxxx"

[linear.profiles.oss]
api_key = "lin_api_yyyyyyyyyyyy"
```

Then reference the profile by name in each repo's `jig.toml`:

```toml
[issues.linear]
profile = "oss"
team = "JIG"
```

## Profile-level filters

Profiles can carry default filters so you don't repeat the same team/project/assignee in every repo:

```toml
# ~/.config/jig/config.toml

[linear.profiles.work]
api_key = "lin_api_xxxxxxxxxxxx"
team = "ENG"
projects = ["Backend", "Platform"]
assignee = "me"
```

Any repo that uses `profile = "work"` inherits these filters. Per-repo `jig.toml` fields override them:

| Field | Resolution |
|-------|-----------|
| `team` | jig.toml > profile > *(error if missing)* |
| `projects` | jig.toml > profile > *(no filter)* |
| `assignee` | jig.toml > profile > *(no filter)* |

### `assignee = "me"`

The special value `"me"` resolves to the authenticated user (the owner of the API key) via Linear's `viewer` query. This is useful for personal issue feeds — only see issues assigned to you.

You can also use an email address (e.g. `assignee = "alice@company.com"`) to filter by a specific person.

## How it maps

Linear and jig have different vocabularies. Here's how they translate:

### Status

| Linear | jig |
|--------|-----|
| Backlog, Unstarted | Planned |
| Started | In Progress |
| Completed, Canceled | Complete |

### Priority

| Linear | jig |
|--------|-----|
| 1 (Urgent) | Urgent |
| 2 (High) | High |
| 3 (Normal) | Medium |
| 4 (Low) | Low |
| 0 (None) | *(omitted)* |

### Category

If the Linear issue belongs to a project, the project name becomes the jig category. Otherwise, the team name is used.

### Relations

Blocking relations in Linear map to `depends_on` in jig. Sub-issues map to `children`.

## Switching providers

Switching between file and Linear is a one-line change in `jig.toml`:

```toml
[issues]
provider = "file"    # or "linear"
```

Both providers implement the same interface. The rest of jig — spawning, the daemon, PR monitoring — doesn't care where the issue came from.
