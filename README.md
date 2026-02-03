# scribe

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
curl -fsSL https://raw.githubusercontent.com/amiller68/scribe-rs/main/install.sh | bash
```

Then add shell integration to your profile:

```bash
# For bash (~/.bashrc)
eval "$(scribe shell-init bash)"

# For zsh (~/.zshrc)
eval "$(scribe shell-init zsh)"
```

## Commands

| Command | Description |
|---------|-------------|
| `scribe create <name> [branch]` | Create a worktree with a new branch |
| `scribe create <name> -o` | Create and cd into the worktree |
| `scribe create <name> --no-hooks` | Create without running on-create hook |
| `scribe open <name>` | cd into an existing worktree |
| `scribe open --all` | Open all worktrees in new terminal tabs |
| `scribe list` | List worktrees in `.worktrees/` |
| `scribe list --all` | List all git worktrees |
| `scribe remove <pattern>` | Remove worktree(s) matching pattern (supports glob) |
| `scribe exit [--force]` | Exit current worktree (removes it, returns to base) |
| `scribe health` | Show terminal detection and dependency status |
| `scribe config` | Show config for current repo |
| `scribe config base <branch>` | Set base branch for current repo |
| `scribe config base --global <branch>` | Set global default base branch |
| `scribe config on-create <cmd>` | Set on-create hook for current repo |
| `scribe config on-create --unset` | Remove on-create hook |
| `scribe config --list` | List all configuration |
| `scribe spawn <name> [options]` | Create worktree + launch Claude in tmux |
| `scribe spawn --context <text>` | Provide task context for Claude |
| `scribe spawn --auto` | Auto-start Claude with full prompt |
| `scribe ps` | Show status of spawned sessions |
| `scribe attach [name]` | Attach to tmux session (optionally to specific window) |
| `scribe review <name>` | Show diff for parent review |
| `scribe merge <name>` | Merge reviewed worktree into current branch |
| `scribe kill <name>` | Kill a running tmux window |
| `scribe init [--force] [--backup]` | Initialize scribe.toml, docs/, issues/, and .claude/ |
| `scribe update` | Show update instructions |
| `scribe version` | Show version |
| `scribe which` | Show path to scribe executable |

**Short alias:** `sc` is available as an alias for `scribe` (e.g., `sc ps`, `sc create foo`).

## Quick Start

```bash
cd ~/projects/my-app
scribe create feature-auth -o    # Creates worktree, cd's into it
claude                           # Start Claude Code in isolation
```

Open a second terminal and do the same — both sessions work independently on their own branches.

## Guides

- [Working with Worktrees](docs/usage/worktrees.md) — Create, open, remove, glob patterns, terminal tabs
- [Configuration and Hooks](docs/usage/configuration.md) — Base branch, config file format, on-create hooks
- [Setting Up a Repo with `scribe init`](docs/usage/init.md) — Bootstrap docs, skills, and permissions
- [Multi-Agent Orchestration](docs/usage/orchestration.md) — Spawn parallel Claude workers with tmux
- [Shell Integration](docs/usage/shell-integration.md) — Tab completion, how `-o` works, `scribe which`

## Development

Build from source:

```bash
cargo build --release
./target/release/scribe --help
```

Run tests:

```bash
cargo test
```

## Updating

Reinstall from the install script:

```bash
curl -fsSL https://raw.githubusercontent.com/amiller68/scribe-rs/main/install.sh | bash
```

Or rebuild from source:

```bash
cargo install --git https://github.com/amiller68/scribe-rs
```

## Uninstall

```bash
rm ~/.local/bin/scribe
rm ~/.local/bin/sc
rm -rf ~/.config/scribe
# Remove eval line from ~/.bashrc and ~/.zshrc
```

## Requirements

- Git
- Bash or Zsh

**For `scribe spawn` (optional):**
- `tmux` - Terminal multiplexer
- `claude` CLI

## License

MIT
