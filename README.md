# jig

Git worktree manager for running parallel Claude Code sessions.

## Features

- **Simple commands** - Create, list, open, and remove worktrees with short commands
- **Auto-isolation** - Worktrees stored in `.worktrees/` (automatically git-ignored)
- **Configurable base branch** - Set per-repo or global default base branch
- **On-create hooks** - Run setup commands automatically after worktree creation
- **Shell integration** - Tab completion for commands and worktree names
- **Nested paths** - Supports branch names like `feature/auth/login`
- **Multi-agent workflow** - Spawn parallel Claude Code sessions with tmux integration

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

| Command | Description |
|---------|-------------|
| `jig create <name> [branch]` | Create a worktree with a new branch |
| `jig create <name> -o` | Create and cd into the worktree |
| `jig create <name> --no-hooks` | Create without running on-create hook |
| `jig open <name>` | cd into an existing worktree |
| `jig open --all` | Open all worktrees in new terminal tabs |
| `jig list` | List worktrees in `.worktrees/` |
| `jig list --all` | List all git worktrees |
| `jig remove <pattern>` | Remove worktree(s) matching pattern (supports glob) |
| `jig exit [--force]` | Exit current worktree (removes it, returns to base) |
| `jig health` | Show terminal detection and dependency status |
| `jig config` | Show config for current repo |
| `jig config base <branch>` | Set base branch for current repo |
| `jig config base --global <branch>` | Set global default base branch |
| `jig config on-create <cmd>` | Set on-create hook for current repo |
| `jig config on-create --unset` | Remove on-create hook |
| `jig config --list` | List all configuration |
| `jig spawn <name> [options]` | Create worktree + launch Claude in tmux |
| `jig spawn --context <text>` | Provide task context for Claude |
| `jig spawn --auto` | Auto-start Claude with full prompt |
| `jig ps` | Show status of spawned sessions |
| `jig attach [name]` | Attach to tmux session (optionally to specific window) |
| `jig review <name>` | Show diff for parent review |
| `jig merge <name>` | Merge reviewed worktree into current branch |
| `jig kill <name>` | Kill a running tmux window |
| `jig init [--force] [--backup]` | Initialize jig.toml, docs/, issues/, and .claude/ |
| `jig update` | Show update instructions |
| `jig version` | Show version |
| `jig which` | Show path to jig executable |

## Quick Start

```bash
cd ~/projects/my-app
jig create feature-auth -o    # Creates worktree, cd's into it
claude                        # Start Claude Code in isolation
```

Open a second terminal and do the same — both sessions work independently on their own branches.

## Guides

- [Working with Worktrees](docs/usage/worktrees.md) — Create, open, remove, glob patterns, terminal tabs
- [Configuration and Hooks](docs/usage/configuration.md) — Base branch, config file format, on-create hooks
- [Setting Up a Repo with `jig init`](docs/usage/init.md) — Bootstrap docs, skills, and permissions
- [Multi-Agent Orchestration](docs/usage/orchestration.md) — Spawn parallel Claude workers with tmux
- [Shell Integration](docs/usage/shell-integration.md) — Tab completion, how `-o` works, `jig which`

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
- `claude` CLI

## License

MIT
