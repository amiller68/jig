# Spawn Architecture: Parent-Orchestrated Multi-Agent Workflow

## Overview

The spawn feature enables a parent Claude Code session to orchestrate multiple child Claude Code sessions running in parallel. Each child session works in its own git worktree, and the parent explicitly reviews and merges their work.

## Core Design Principle

**The parent Claude Code session orchestrates; `wt` is its tool.**

The parent session has:
- Linear MCP access (or other data sources)
- Intelligence for task decomposition
- Ability to review and make decisions
- Full conversation history for auditability

`wt` provides simple, reliable primitives that the parent uses.

## Components

### lib/spawn.sh - State Management

Tracks which worktrees were spawned with context.

```
~/.config/wt/spawned/<hash>.json
```

State structure:
```json
{
  "spawned": [
    {
      "name": "AUT-4478",
      "branch": "epic-AUT-4475",
      "context": "Implement the Thread Source Model...",
      "created": "2024-01-15T10:30:00Z"
    }
  ]
}
```

Functions:
- `get_spawn_dir()` - Config directory path
- `get_spawn_state_file()` - State file for current repo (hashed)
- `load_spawn_state()` / `save_spawn_state()` - State I/O
- `register_spawn()` / `unregister_spawn()` - Track spawned worktrees
- `is_spawned()` - Check if worktree was spawned
- `get_spawned_names()` - List all spawned worktrees
- `get_spawn_info()` - Get details for specific spawn
- `write_context_file()` - Write `.claude-task` file

### lib/tmux.sh - Session Management

All spawned tasks share one tmux session (`wt-spawned`), each as a window.

Functions:
- `check_tmux()` - Verify tmux is installed
- `ensure_spawn_session()` - Create session if needed
- `spawn_window()` - Create window, cd to worktree, launch claude
- `kill_window()` - Kill specific window
- `attach_spawn()` - Attach to session, optionally switch window
- `spawn_session_exists()` - Check if session exists
- `list_spawn_windows()` - List all windows
- `get_window_status()` - Check if window is running/exited

### Commands in _wt.sh

| Command | Function | Description |
|---------|----------|-------------|
| `spawn` | `handle_spawn()` | Create worktree + tmux window |
| `ps` | `handle_ps()` | Show spawned session status |
| `attach` | `handle_attach()` | Attach to tmux session |
| `review` | `handle_review()` | Show diff for parent review |
| `merge` | `handle_merge()` | Merge into current branch |
| `kill` | `handle_kill()` | Kill tmux window |

## Data Flow

### Spawn Flow

```
User: wt spawn AUT-4478 --context "Implement..."
         │
         ├─► Create worktree (branching from current branch)
         │     └─► .worktrees/AUT-4478/
         │
         ├─► Write context file
         │     └─► .worktrees/AUT-4478/.claude-task
         │
         ├─► Register in state
         │     └─► ~/.config/wt/spawned/<hash>.json
         │
         └─► Launch in tmux
               └─► wt-spawned session, AUT-4478 window
                     └─► cd <worktree> && claude
```

### Review Flow

```
User: wt review AUT-4478
         │
         ├─► Find worktree
         │     └─► .worktrees/AUT-4478/
         │
         ├─► Get branch info
         │     └─► git branch --show-current
         │
         ├─► Count commits
         │     └─► git rev-list --count base..HEAD
         │
         ├─► Show commit log
         │     └─► git log --oneline base..HEAD
         │
         └─► Show diff summary (or --full)
               └─► git diff --stat base...HEAD
```

### Merge Flow

```
User: wt merge AUT-4478
         │
         ├─► Verify worktree exists
         │
         ├─► Check not dirty
         │
         ├─► Get source branch
         │
         ├─► Perform merge (in current directory)
         │     └─► git merge --no-ff <branch>
         │
         ├─► Unregister from state
         │
         └─► Kill tmux window
```

## Context Injection

When `--context` is provided, the text is written to `.claude-task` in the worktree.

To use this, configure CLAUDE.md in your project:
```markdown
If a `.claude-task` file exists in the worktree, read it for task instructions.
```

The `.claude-task` file is:
- Written to the worktree root
- Automatically added to `.gitignore`
- Readable by Claude on session start

## tmux Session Structure

```
wt-spawned (session)
├── AUT-4478 (window) ─► running claude
├── AUT-4480 (window) ─► running claude
├── AUT-4481 (window) ─► exited
└── AUT-4482 (window) ─► running claude
```

Users can:
- Attach with `wt attach`
- Switch windows with tmux shortcuts (Ctrl-b n/p)
- Monitor all workers simultaneously
- Detach and continue later

## Auditability

Everything is tracked:

1. **Parent conversation** - Full history of spawn, review, merge commands
2. **Git history** - Each task as a merge commit
3. **`.claude-task` files** - Document what was requested
4. **State files** - Track when spawned and from which branch

## Comparison with Old Epic System

| Aspect | Old Epic | New Spawn |
|--------|----------|-----------|
| Linear integration | Built into wt | Parent handles via MCP |
| Orchestration | wt manages | Parent manages |
| State | Complex epic state | Simple spawn tracking |
| Dependencies | Automatic unlocking | Parent decides order |
| tmux | Per-epic session | Single shared session |
| Context | Auto-generated | User-provided |

## Error Handling

- **Worktree exists**: Reuse existing, warn user
- **tmux not installed**: Clear error, exit
- **Merge conflicts**: Fail merge, user resolves
- **Dirty worktree merge**: Reject, require commit
- **Missing worktree**: Clear error for review/merge/kill

## Security Considerations

- State files stored in user config directory
- No credentials stored
- Context files may contain task details (gitignored)
- tmux sessions are local only

## Auto Mode

Auto mode enables fully autonomous spawned sessions using `--auto` flag:

```bash
wt spawn AUT-4478 --context "Implement the webhook" --auto
```

This uses `claude --dangerously-skip-permissions -p "prompt"` to:
- Pass the full prompt directly to Claude
- Skip all permission prompts (true autonomous mode)
- Claude starts working immediately without user interaction

### Agent Context Loading

When auto mode is enabled, `wt` can load additional context from an `./agents/` directory:

```
./agents/
├── INDEX.md          # Required - entry point
├── CONCEPTS.md       # Optional domain context
├── ARCHITECTURE.md   # Optional technical context
└── ...
```

The prompt is built by concatenating:
1. All markdown files from `./agents/` (INDEX.md first, then alphabetically)
2. The `--context` value from the spawn command

Use `--no-agents` to skip loading agent context.

### wt.toml Configuration

Repos can include a `wt.toml` file for spawn configuration:

```toml
[agents]
dir = "./agents"  # Custom agents directory (default: ./agents)

[spawn]
auto = true       # Always use auto mode (default: false)

[setup]
# Bash commands to allow during wt setup
allow = ["pnpm *", "make *", "git *"]
deny = ["rm -rf *", "sudo *"]
```

## Setup Command

`wt setup` initializes Claude Code settings for the repo:

```bash
wt setup              # Use defaults or wt.toml
wt setup --force      # Overwrite existing
```

What it does:
1. Creates `.claude/` directory
2. Merges permissions from `wt.toml` into `.claude/settings.json`
3. Copies command files from wt installation to `.claude/commands/`

### Default Commands

Commands shipped with wt:
- `check.md` - Run project checks (make check, npm test, cargo test)
- `review.md` - Review staged changes before commit
- `draft.md` - Draft commit message for staged changes

## Future Considerations

1. **Parallel spawn**: `wt spawn` multiple at once
2. **Progress tracking**: Better status indicators
3. **Auto-cleanup**: Remove merged worktrees automatically
4. **Context from file**: `--context-file` flag
5. **Session persistence**: Survive terminal close
