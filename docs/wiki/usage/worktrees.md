# Working with Worktrees

Create isolated worktrees for parallel development — each gets its own branch and working directory.

## Create and Open

```bash
cd ~/projects/my-app
wt create feature-auth -o    # Creates worktree, cd's into it
claude                       # Start Claude Code
```

The `-o` flag can be placed anywhere:

```bash
wt -o create feature-auth    # Same as above
wt create -o feature-auth    # Also works
```

## Parallel Sessions

Each worktree is independent. Run multiple Claude Code sessions side by side:

Terminal 1:
```bash
wt create feature-auth -o
claude
```

Terminal 2:
```bash
wt create fix-bug-123 -o
claude
```

Both instances work independently with their own branches.

## Open Existing Worktree

```bash
wt open my-feature
```

## Open All in Tabs

Open every worktree in a new terminal tab:

```bash
wt open --all
```

Check terminal compatibility with `wt health`.

## List Worktrees

```bash
wt list          # Worktrees in .worktrees/
wt list --all    # All git worktrees
```

## Remove Worktrees

```bash
wt remove test1              # Remove exact match
wt remove 'test*'            # Remove all starting with "test"
wt remove 'feature/*'        # Remove all under feature/
```

## Nested Paths

Branch names with slashes create nested directories:

```bash
wt create feature/auth/oauth -o
# Creates .worktrees/feature/auth/oauth/
```

## How It Works

Worktrees are stored in `.worktrees/` inside your repo:

```
my-repo/
├── .worktrees/           # Auto-added to .git/info/exclude
│   ├── feature-a/
│   └── feature-b/
├── src/
└── ...
```

Each worktree is a full checkout on its own branch. Changes don't affect other worktrees until you merge.
