<p align="center">
  <img src="assets/logo.svg" width="72" height="72" alt="jig logo">
</p>

# jig

[![CI](https://github.com/amiller68/jig/actions/workflows/test.yml/badge.svg)](https://github.com/amiller68/jig/actions/workflows/test.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-violet.svg)](https://opensource.org/licenses/MIT)
[![Docs](https://img.shields.io/badge/docs-jig.krondor.org-violet)](https://jig.krondor.org)

**Multiply yourself across parallel agent sessions.**

Git worktree manager for orchestrating Agentic Coding Assistants in parallel. Scale your skills across multiple AI coding sessions—spend your time deciding *what* to build, not the minutiae of *how*.

**[Read the wiki](https://jig.krondor.org)** for background, concepts, and workflow guides.

## Features

- **Simple commands** - Create, list, open, and remove worktrees with short commands
- **Auto-isolation** - Worktrees stored in `.jig/` (automatically git-ignored)
- **Configurable base branch** - Set per-repo or global default base branch
- **On-create hooks** - Run setup commands automatically after worktree creation
- **Shell integration** - Tab completion for commands and worktree names
- **Multi-agent workflow** - Spawn parallel agent sessions with tmux integration
- **Event-driven orchestration** - JSONL event logs track worker lifecycle, derive state, and trigger actions
- **Daemon loop** - Background orchestrator monitors workers, nudges idle/stuck sessions, and sends notifications
- **GitHub integration** - Detect CI failures, merge conflicts, and review comments via `gh` CLI
- **Live dashboard** - `jig ps --watch` shows real-time worker status with event-derived state

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/amiller68/jig/main/install.sh | bash
```

Then add shell integration to your profile:

```bash
# For bash (~/.bashrc)
eval "$(jig shell-init bash)"

# For zsh (~/.zshrc)
eval "$(jig shell-init zsh)"
```

## Commands

### Worktrees

| Command | Description |
|---------|-------------|
| `jig create <branch>` | Create a worktree with a new branch |
| `jig create <branch> -o` | Create and cd into the worktree |
| `jig open <branch>` | cd into an existing worktree |
| `jig list` | List worktrees in `.jig/` |
| `jig list --all` | List all git worktrees |
| `jig remove <pattern>` | Remove worktree(s) matching pattern (supports glob) |
| `jig exit [--force]` | Exit current worktree (removes it, returns to base) |
| `jig home` | Navigate to base repository root |

### Sessions

| Command | Description |
|---------|-------------|
| `jig spawn <branch> [options]` | Create worktree + launch agent in tmux |
| `jig spawn --context <text>` | Provide task context for the agent |
| `jig spawn --issue ENG-123` | Link a Linear issue to the worker |
| `jig spawn --auto` | Auto-start the agent with full prompt |
| `jig ps` | Show status of spawned sessions |
| `jig ps -w` | Live dashboard (updates every 2s) |
| `jig ps -g` | Global mode — workers across all repos |
| `jig attach [branch]` | Attach to tmux session |
| `jig kill <branch>` | Kill a running tmux window |
| `jig kill --all` | Kill all workers |
| `jig nuke` | Nuke all workers and state (keeps config) |
| `jig pr` | Push current branch and create a draft PR |
| `jig pr comments` | Show review feedback on the current PR |

### Configuration

| Command | Description |
|---------|-------------|
| `jig init <agent>` | Initialize `AGENTS.md`, docs/, skills, and `jig.toml` |
| `jig config` | Show config for current repo |
| `jig config base <branch>` | Set base branch for current repo |
| `jig config on-create <cmd>` | Set on-create hook for current repo |
| `jig issues` | Browse and manage Linear issues |
| `jig health` | Show dependency and agent status |
| `jig hooks` | Manage git/agent hooks |
| `jig shell-init <shell>` | Print shell integration script |
| `jig shell-setup` | Auto-configure shell integration |
| `jig update` | Update jig to latest version |
| `jig version` | Show version |
| `jig which` | Show path to jig executable |

## Quick Start

```bash
cd ~/projects/my-app
jig init claude                        # Bootstrap AGENTS.md, docs/, skills
jig spawn feature-auth --auto          # Create worktree + launch agent
jig ps -w                              # Watch the dashboard
```

## Development

Build from source:

```bash
cargo build --release
./target/release/jig --help
```

Run tests:

```bash
cargo test
```

## Updating

Reinstall from the install script:

```bash
curl -fsSL https://raw.githubusercontent.com/amiller68/jig/main/install.sh | bash
```

Or rebuild from source:

```bash
cargo install --git https://github.com/amiller68/jig
```

## Uninstall

```bash
rm ~/.local/bin/jig
rm -rf ~/.config/jig
# Remove eval line from ~/.bashrc and ~/.zshrc
```

## Requirements

- Git
- Bash or Zsh

**For `jig spawn` (optional):**
- `tmux` - Terminal multiplexer
- An AI coding assistant CLI (e.g. `claude`)

## License

MIT
