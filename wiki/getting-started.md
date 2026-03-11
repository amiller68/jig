---
layout: page
title: Getting Started
nav_order: 4
---

# Getting Started

## Requirements

- **Git** — For worktree management
- **tmux** — For agent session management
- **An ACA** — Claude Code recommended, but jig is designed to work with any terminal-based coding assistant

## Installation

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

Or build from source:

```bash
git clone https://github.com/amiller68/jig
cd jig
cargo install --path crates/jig-cli
```

## Initialize a repository

In your project root:

```bash
jig init claude
```

This scaffolds:

```text
.
├── jig.toml
├── CLAUDE.md
├── docs/
│   ├── index.md
│   ├── PATTERNS.md
│   ├── CONTRIBUTING.md
│   └── SUCCESS_CRITERIA.md
└── issues/
    ├── README.md
    ├── _templates/
    ├── features/
    ├── bugs/
    └── chores/
```

To also have the agent audit the codebase and populate the skeleton docs automatically, `--audit` launches the agent in a `jig-init` tmux session:

```bash
jig init claude --audit
```

You can pass extra instructions to guide the audit:

```bash
jig init claude --audit "We use pnpm, not npm. The API is actix-web."
```

For existing repos with customized docs, back up first so the agent can use them as reference:

```bash
jig init claude --force --backup --audit
```

Attach to the audit session with `tmux attach -t jig-init`.

## Configuration

Edit `jig.toml` to customize:

```toml
[worktree]
# base = "origin/main"
# on_create = "npm install"
# copy = [".env"]
```

## Your first spawn

1. **Create an issue:**

```bash
cp issues/_templates/feature.md issues/features/hello-world.md
```

Edit it with a clear description and acceptance criteria.

2. **Spawn an agent:**

```bash
jig spawn hello-world --context "Complete the task in issues/001-hello-world.md"
```

3. **Monitor:**

```bash
jig ps                    # See all workers and their status
jig ps -w                 # Live watch mode (updates every 2s)
jig ps -g                 # Global mode — workers across all repos
jig attach hello-world    # Attach to the agent's tmux session
```

4. **Review and merge:**

```bash
jig review hello-world    # See the diff
jig merge hello-world     # Merge if approved
jig remove hello-world    # Clean up
```

## Commands reference

### Worktree management

| Command | Description |
|---------|-------------|
| `jig create <name>` | Create a worktree (`-o` to cd into it) |
| `jig list` | List all worktrees (`-g` for global, `--all` to include base repo) |
| `jig open <name>` | Navigate to worktree directory (`--all` to open all in tabs) |
| `jig remove <name>` | Remove worktree(s) — supports glob patterns (`-f` to force) |
| `jig exit` | Remove current worktree and cd to repo root |
| `jig home` | Navigate to base repository root |

### Sessions

| Command | Description |
|---------|-------------|
| `jig spawn <name>` | Create worktree + launch agent session (`--auto`, `--issue`) |
| `jig ps` | Show worker status dashboard |
| `jig ps -w` | Live watch mode with daemon loop (l=logs, q=quit) |
| `jig ps -g` | Global mode — workers across all repos |
| `jig attach <name>` | Attach to agent's tmux session |
| `jig kill <name>` | Kill a worker's tmux session (`-a` for all) |
| `jig nuke` | Kill all workers and clear state (keeps config/hooks) |

### Review & merge

| Command | Description |
|---------|-------------|
| `jig review <name>` | Show diff for review (`--full` for complete diff) |
| `jig merge <name>` | Merge worktree branch into current branch |

### Configuration

| Command | Description |
|---------|-------------|
| `jig init <agent>` | Initialize jig in a repository (`--audit` to auto-populate docs) |
| `jig config` | View/edit configuration (`base`, `on-create`, `show`) |
| `jig repos` | List registered repos |
| `jig issues` | Browse issues (`-i` interactive, `--auto` spawnable only) |

### System

| Command | Description |
|---------|-------------|
| `jig daemon` | Run the background daemon (`--interval`, `--once`) |
| `jig health` | Check system dependencies and repo setup |
| `jig hooks` | Manage git/agent hooks (`init`, `uninstall`, `install-claude`) |
| `jig shell-init <shell>` | Print shell integration script |
| `jig shell-setup` | Auto-configure shell integration |
| `jig update` | Update jig to latest version |
| `jig version` | Show version information |
| `jig which` | Show path to jig executable |

## Next steps

- Read [Core Concepts](/concepts) to understand the jig philosophy
- Explore [Background](/background) for the "why" behind jig
