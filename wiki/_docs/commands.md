---
title: Commands
slug: commands
date: 2025-05-09
---

Quick reference for all jig commands.

## Worktree management

| Command | Description |
|---------|-------------|
| `jig create <name>` | Create a worktree (`-o` to cd into it) |
| `jig list` | List all worktrees (`-g` for global, `--all` to include base repo) |
| `jig open <name>` | Navigate to worktree directory |
| `jig remove <name>` | Remove worktree(s) — supports glob patterns (`-f` to force) |
| `jig exit` | Remove current worktree and cd to repo root |
| `jig home` | Navigate to base repository root |

## Sessions

| Command | Description |
|---------|-------------|
| `jig spawn <name>` | Create worktree + launch agent session (`--issue`, `--context`) |
| `jig ps` | Show worker status dashboard |
| `jig ps -w` | Watch mode with live updates |
| `jig ps -gw` | Global watch — all repos, live updates, runs daemon lifecycle |
| `jig attach <name>` | Attach to agent's tmux session |
| `jig kill <name>` | Kill a worker's tmux session (`-a` for all) |
| `jig nuke` | Kill all workers and clear state (keeps config/hooks) |

## Issues

| Command | Description |
|---------|-------------|
| `jig issues` | List issues from configured provider |
| `jig issues <ID>` | View a specific issue |
| `jig issues create "title"` | Create a new issue (`--body`, `--priority`, `--label`, `--category`, `--parent`) |
| `jig issues update <ID>` | Update an issue (`--body`, `--append`, `--parent`, `--blocked-by`) |
| `jig issues status <ID> <status>` | Set issue status |
| `jig issues complete <ID>` | Mark issue complete |

### Issue filters

```bash
jig issues --status planned           # by status
jig issues --priority high            # by priority
jig issues --category Backend         # by project/category
jig issues --label backend            # by label (all must match)
jig issues --label backend --label bug
```

## PRs

| Command | Description |
|---------|-------------|
| `jig pr` | Push and create a draft PR for the current worktree |
| `jig pr comments` | Show review feedback on the current branch's PR |
| `jig pr comments --pr 42` | Show review feedback for a specific PR |
| `jig pr comments --between HEAD~3..HEAD` | Filter comments to files changed in a range |

`jig pr` auto-detects parent issues and targets the parent branch instead of main. See [Parent-Child](/docs/parent-child/) for details.

## Configuration

| Command | Description |
|---------|-------------|
| `jig init` | Initialize jig in a repository (`--audit`, `--force`, `--backup`) |
| `jig config` | View/edit configuration (`base`, `on-create`, `--list`) |

## System

| Command | Description |
|---------|-------------|
| `jig health` | Check system dependencies and repo setup |
| `jig hooks` | Manage git/agent hooks (`init`, `uninstall`) |
| `jig shell-init <shell>` | Print shell integration script |
| `jig shell-setup` | Auto-configure shell integration |
| `jig update` | Update jig to latest version |
| `jig version` | Show version information |
| `jig which` | Show path to jig executable |
