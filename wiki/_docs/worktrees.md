---
title: Worktrees
slug: worktrees
date: 2025-05-09
---

Create isolated worktrees for parallel development — each gets its own branch and working directory.

## Create

```bash
jig create feature-auth        # Create worktree
jig create feature-auth -o     # Create and cd into it
```

The `-o` flag can be placed anywhere:

```bash
jig -o create feature-auth
jig create -o feature-auth
```

## List

```bash
jig list                       # Worktrees in current repo
jig list -g                    # Global — all repos
jig list --all                 # Include the base repo
```

## Open

```bash
jig open feature-auth          # cd into an existing worktree
```

## Remove

```bash
jig remove feature-auth        # Remove exact match
jig remove 'test*'             # Glob pattern
jig remove 'feature/*'         # Remove all under feature/
jig remove feature-auth -f     # Force remove
```

## Exit

```bash
jig exit                       # Remove current worktree and cd to repo root
```

## Home

```bash
jig home                       # cd to base repository root
```

## Nested paths

Branch names with slashes create nested directories:

```bash
jig create feature/auth/oauth -o
# Creates .jig/feature/auth/oauth/
```

## How it works

Worktrees are stored in `.jig/` inside your repo (auto-added to `.git/info/exclude`):

```text
my-repo/
├── .jig/
│   ├── feature-a/
│   ├── feature-b/
│   └── feature/
│       └── auth/
│           └── oauth/
├── src/
└── ...
```

Each worktree is a full checkout of your repo on its own branch. Changes in one worktree don't affect others until you merge.

## Parallel sessions

Each worktree is independent. Run multiple agent sessions side by side:

Terminal 1:
```bash
jig create feature-auth -o
your-agent
```

Terminal 2:
```bash
jig create fix-bug-123 -o
your-agent
```

Both work independently with their own branches. For managed parallel sessions with monitoring and daemon supervision, use `jig spawn` instead — see [Spawning](/docs/spawning/).
