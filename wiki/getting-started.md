---
layout: page
title: Getting Started
nav_order: 5
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
jig init
```

This scaffolds:

```
.
├── jig.toml              # jig configuration
├── CLAUDE.md             # Agent instructions (if using Claude Code)
├── docs/
│   ├── index.md
│   ├── PATTERNS.md
│   ├── CONTRIBUTING.md
│   └── ...
└── issues/
    └── _template.md
```

## Configuration

Edit `jig.toml` to customize:

```toml
[worktree]
dir = ".worktrees"        # Where worktrees are created
base_branch = "main"      # Branch to base worktrees on

[worktree.copy]
paths = [".env.local"]    # Gitignored files to copy into worktrees
```

## Your first spawn

1. **Create an issue:**

```bash
cp issues/_template.md issues/001-hello-world.md
```

Edit it with a clear description and acceptance criteria.

2. **Spawn an agent:**

```bash
jig spawn hello-world --context "Complete the task in issues/001-hello-world.md"
```

3. **Monitor:**

```bash
jig list                  # See all worktrees
jig attach hello-world    # Attach to the agent's tmux session
```

4. **Review and merge:**

```bash
jig review hello-world    # See the diff
jig merge hello-world     # Merge if approved
jig remove hello-world    # Clean up
```

## Commands reference

| Command | Description |
|---------|-------------|
| `jig init` | Initialize jig in a repository |
| `jig create <name>` | Create a worktree |
| `jig spawn <name>` | Create worktree + launch agent |
| `jig list` | List all worktrees |
| `jig attach <name>` | Attach to agent's tmux session |
| `jig review <name>` | Show diff for review |
| `jig merge <name>` | Merge worktree branch |
| `jig remove <name>` | Remove worktree |

## Next steps

- Read [Core Concepts](/concepts) to understand the jig philosophy
- Check out [Workflow](/workflow) for the full development loop
- Explore [Background](/background) for the "why" behind jig
