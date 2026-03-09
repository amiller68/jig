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
│   ├── SUCCESS_CRITERIA.md
│   └── PROJECT_LAYOUT.md
└── issues/
    ├── README.md
    ├── _templates/
    ├── features/
    ├── bugs/
    └── chores/
```

To also have the agent audit the codebase and populate the skeleton docs automatically:

```bash
jig init claude --audit
```

For existing repos with customized docs, back up first so the agent can use them as reference:

```bash
jig init claude --force --backup --audit
```

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
| `jig create <name>` | Create a worktree |
| `jig list` | List all worktrees |
| `jig open <name>` | Open a worktree directory |
| `jig remove <name>` | Remove a worktree |
| `jig exit` | Remove current worktree (run from inside one) |

### Sessions

| Command | Description |
|---------|-------------|
| `jig spawn <name>` | Create worktree + launch agent session |
| `jig ps` | Show worker status dashboard |
| `jig ps -w` | Live watch mode (updates every 2s) |
| `jig ps -g` | Global mode — workers across all repos |
| `jig attach <name>` | Attach to agent's tmux session |
| `jig kill <name>` | Kill a worker's tmux session |
| `jig nuke <name>` | Kill session + remove worktree |

### Review & merge

| Command | Description |
|---------|-------------|
| `jig review <name>` | Show diff for review |
| `jig merge <name>` | Merge worktree branch |

### Configuration

| Command | Description |
|---------|-------------|
| `jig init` | Initialize jig in a repository |
| `jig config` | View/edit configuration |
| `jig repos` | List registered repos |
| `jig issues` | Browse and manage issues |

### System

| Command | Description |
|---------|-------------|
| `jig daemon` | Run the background daemon |
| `jig health` | Check system health |
| `jig hooks` | Manage git/agent hooks |
| `jig shell-init <shell>` | Print shell integration script |
| `jig shell-setup` | Interactive shell setup |

## Next steps

- Read [Core Concepts](/concepts) to understand the jig philosophy
- Explore [Background](/background) for the "why" behind jig
