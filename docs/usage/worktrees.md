# Working with Worktrees

Create isolated worktrees for parallel development — each gets its own branch and working directory.

## Create and Open

```bash
cd ~/projects/my-app
scribe create feature-auth -o    # Creates worktree, cd's into it
claude                           # Start Claude Code
```

The `-o` flag can be placed anywhere:

```bash
scribe -o create feature-auth    # Same as above
scribe create -o feature-auth    # Also works
```

## Parallel Sessions

Each worktree is independent. Run multiple Claude Code sessions side by side:

Terminal 1:
```bash
cd ~/projects/my-app
scribe create feature-auth -o
claude
```

Terminal 2:
```bash
cd ~/projects/my-app
scribe create fix-bug-123 -o
claude
```

Both instances work independently with their own branches.

## Open All in Tabs

Open every worktree in a new terminal tab:

```bash
scribe open --all
```

This detects your terminal emulator and opens each worktree in a new tab. Check compatibility with:

```bash
scribe health
```

**Supported terminals:**

| Terminal | Method | Notes |
|----------|--------|-------|
| iTerm2 | AppleScript | Full support |
| Terminal.app | AppleScript | Full support |
| Ghostty | `open -a` | Opens at directory |
| Kitty | `kitten @` | Requires `allow_remote_control yes` in kitty.conf |
| WezTerm | `wezterm cli` | Full support |
| Alacritty | New window | No native tabs (opens windows instead) |

## Existing Branches

Create a worktree using a branch that already exists:

```bash
scribe create my-worktree existing-branch
```

## Nested Paths

Branch names with slashes create nested directories:

```bash
scribe create feature/auth/oauth -o
# Creates .worktrees/feature/auth/oauth/
```

## Remove with Glob Patterns

```bash
scribe remove test1              # Remove exact match
scribe remove 'test*'            # Remove all starting with "test"
scribe remove 'feature/*'        # Remove all under feature/
```

## How It Works

Worktrees are stored in `.worktrees/` inside your repo:

```
my-repo/
├── .worktrees/           # Auto-added to .git/info/exclude
│   ├── feature-a/
│   ├── feature-b/
│   └── feature/
│       └── auth/
│           └── oauth/
├── src/
└── ...
```

Each worktree is a full checkout of your repo on its own branch. Changes in one worktree don't affect others until you merge.
