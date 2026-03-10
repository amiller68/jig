# Linear Integration

Connect jig's issue system to Linear so you can `jig spawn --issue ENG-123` and have agents work directly from Linear tickets.

## Overview

jig supports two issue providers:

- **file** (default) — Reads markdown files from `issues/`
- **linear** — Fetches issues from the Linear GraphQL API

When `provider = "linear"` is set in `jig.toml`, commands like `jig issues` and `jig spawn --issue` talk to Linear instead of the local filesystem.

## Setup

### 1. Create a Linear API key

Go to **Linear > Settings > API > Personal API keys** and create a key. It will look like `lin_api_xxxxxxxxxxxx`.

### 2. Add the key to global config

Edit `~/.config/jig/config.toml` and add a named profile:

```toml
[linear.profiles.work]
api_key = "lin_api_xxxxxxxxxxxx"
```

You can have multiple profiles for different workspaces:

```toml
[linear.profiles.work]
api_key = "lin_api_xxxxxxxxxxxx"

[linear.profiles.personal]
api_key = "lin_api_yyyyyyyyyyyy"
```

### 3. Configure your repo

In `jig.toml`, point the issues system at Linear:

```toml
[issues]
provider = "linear"

[issues.linear]
profile = "work"          # references the global profile name
team = "ENG"              # your Linear team key
projects = ["Backend"]    # optional: filter to specific projects
```

The `projects` list is optional. If omitted, all issues for the team are returned.

## Usage

Once configured, existing commands work transparently:

```bash
# List issues from Linear
jig issues

# Filter by status
jig issues --status planned        # backlog + unstarted
jig issues --status in-progress    # started

# Filter by priority
jig issues --priority urgent
jig issues --priority high

# Filter by project (category)
jig issues --category Backend

# Filter by label (all must match)
jig issues --label backend
jig issues --label backend --label sprint-1

# View a single issue
jig issues ENG-123

# Spawn a worker from a Linear issue
jig spawn auth-jwt --issue ENG-123
```

## Status mapping

Linear states map to jig statuses:

| Linear state type | jig status |
|-------------------|------------|
| `backlog` | Planned |
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

## Field mapping

| jig field | Linear source |
|-----------|---------------|
| `id` | Issue identifier (e.g. `ENG-123`) |
| `title` | Issue title |
| `status` | Mapped from state type |
| `priority` | Mapped from priority number |
| `category` | Project name, or team name if no project |
| `depends_on` | Blocking relation identifiers |
| `body` | `# Title` heading + description markdown |
| `source` | Linear issue URL |
| `children` | Sub-issue identifiers |
| `labels` | Linear label names |

## Switching back to file-based issues

Change the provider in `jig.toml`:

```toml
[issues]
provider = "file"
```

Or remove the `[issues]` section entirely — `file` is the default.

## Troubleshooting

**"Linear profile 'X' not found"** — The profile name in `jig.toml` doesn't match any entry in `~/.config/jig/config.toml`. Check spelling and ensure the `[linear.profiles.X]` section exists.

**No issues returned** — Verify the `team` key matches your Linear team (e.g. `ENG`, not the full team name). Check that `projects` contains valid project names if set.

**Authentication errors** — Ensure your API key is valid and has read access. Regenerate it from Linear settings if needed.
