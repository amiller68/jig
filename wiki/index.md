---
layout: home
title: Home
---

# jig

Git worktree manager for parallel Claude Code sessions.

## Installation

```bash
cargo install jig-cli
```

## Quick Start

```bash
# Initialize jig in your repo
jig init

# Create a worktree for a task
jig create feature-x

# Spawn a Claude Code session
jig spawn feature-x --context "implement feature X"

# List active worktrees
jig list

# Clean up when done
jig remove feature-x
```

## Documentation

- [GitHub Repository](https://github.com/amiller68/jig)
