# Spawn Workflow Guide

Best practices for orchestrating parallel Claude agents with `wt spawn --auto`.

## Quick Start

```bash
# 1. Initialize wt for your repo (one-time)
wt init

# 2. Edit agents/INDEX.md with project-specific instructions

# 3. Create integration worktree
wt -o create epic-123

# 4. Spawn autonomous workers
wt spawn task-1 --context "Implement feature X..." --auto
wt spawn task-2 --context "Add tests for Y..." --auto
wt spawn task-3 --context "Update docs for Z..." --auto

# 5. Monitor progress
wt ps

# 6. Review and merge completed work
wt review task-1
wt merge task-1
```

## Setup

### 1. Initialize with `wt init`

```bash
wt init
```

This creates:
- `wt.toml` - Spawn configuration (auto mode enabled by default)
- `agents/INDEX.md` - Agent instructions template
- `.claude/settings.json` - Claude permissions
- `.claude/commands/` - Slash commands

### 2. Customize wt.toml (optional)

Edit the generated `wt.toml` to add project-specific permissions:

```toml
[spawn]
auto = true  # All spawns use auto mode by default

[agents]
dir = "./agents"

[setup]
allow = [
    "pnpm *",
    "npm *",
    "make *",
    "cargo *",
    "git add",
    "git commit",
    "git push"
]
deny = [
    "rm -rf *",
    "sudo *"
]
```

Then re-run `wt init --force` to update `.claude/settings.json`.

### 3. Customize agents/INDEX.md

Edit the generated template with project-specific instructions:

```markdown
# Agent Instructions

You are working on [Project Name].

## Project Context
- This is a [type of project]
- Key technologies: [list them]
- Tests run with: [command]

## Workflow
1. Read the task carefully
2. Explore relevant files
3. Implement changes following our patterns
4. Run tests: `[test command]`
5. Commit with conventional commit format

## Conventions
- [Your coding standards]
- [PR/commit conventions]
```

### 4. Add more context files (optional)

```
agents/
├── INDEX.md           # Required - main instructions
├── ARCHITECTURE.md    # System architecture overview
├── CONVENTIONS.md     # Coding standards
└── API.md             # API documentation
```

## Orchestration Workflow

### Parent Session

The parent Claude session (you're talking to it now) orchestrates:

```bash
# Create an integration branch for the epic
wt -o create epic-AUT-4475

# Fetch task details (if using Linear MCP)
# The parent reads issue descriptions and creates spawn commands

# Spawn workers with specific context
wt spawn AUT-4478 --context "$(cat <<'EOF'
Implement the Thread Source Model.

Requirements:
- Create ThreadSource enum in src/models/
- Add threadTs field to Message type
- Update serialization logic

Acceptance criteria:
- All existing tests pass
- New unit tests for ThreadSource
EOF
)" --auto

wt spawn AUT-4480 --context "$(cat <<'EOF'
Add threadTs support to SlackClient.

Requirements:
- Modify SlackClient.sendMessage() to accept threadTs
- Update existing call sites
- Add integration test

Dependencies:
- Builds on Thread Source Model (will be merged first)
EOF
)" --auto
```

### Monitoring

```bash
# Check status of all workers
wt ps

# Output:
# TASK         STATUS     BRANCH        COMMITS  DIRTY
# ----         ------     ------        -------  -----
# AUT-4478     running    AUT-4478      3        no
# AUT-4480     running    AUT-4480      1        yes
# AUT-4481     exited     AUT-4481      5        no

# Watch workers in real-time
wt attach

# Switch between windows in tmux: Ctrl-b n (next), Ctrl-b p (prev)
# Detach: Ctrl-b d
```

### Review and Merge

```bash
# Review completed work
wt review AUT-4478

# Shows:
# - Commit count and history
# - Changed files summary
# - Dirty state warning if uncommitted changes

# Full diff
wt review AUT-4478 --full

# Merge into current branch (your epic branch)
wt merge AUT-4478

# Clean up
wt remove AUT-4478
```

## Context Best Practices

### Task Context Structure

```markdown
<One-line summary of the task>

## Requirements
- Specific requirement 1
- Specific requirement 2

## Files to Modify
- src/models/foo.ts
- src/services/bar.ts

## Acceptance Criteria
- Tests pass
- Feature works as specified

## Notes
- Any special considerations
- Dependencies on other tasks
```

### Using Linear MCP

If you have Linear MCP configured, the parent can fetch issue details:

```bash
# Parent fetches issue and spawns with full context
# (This happens in the parent Claude conversation)

# Example parent prompt:
# "Fetch AUT-4478 from Linear and spawn a worker for it"
```

### Dependency Management

For tasks with dependencies, spawn them in order and merge as they complete:

```bash
# Task 2 depends on Task 1
wt spawn task-1 --context "Foundation work..." --auto
# Wait for task-1 to complete
wt merge task-1

wt spawn task-2 --context "Builds on task-1..." --auto
```

Or spawn all and let workers handle:

```bash
wt spawn task-1 --context "Foundation..." --auto
wt spawn task-2 --context "Depends on task-1. If blocked, implement what you can..." --auto
```

## Tips

### 1. Keep context focused
Each spawned agent should have a clear, scoped task. Avoid "implement the whole feature" - break it down.

### 2. Include file hints
Mentioning specific files helps agents find the right code faster:
```
Modify src/services/auth.ts to add OAuth support
```

### 3. Set acceptance criteria
Clear criteria help agents know when they're done:
```
Acceptance: All tests pass, OAuth flow works end-to-end
```

### 4. Use --no-agents for simple tasks
Skip agent context for trivial tasks:
```bash
wt spawn quick-fix --context "Fix typo in README" --auto --no-agents
```

### 5. Review before merge
Always review spawned work before merging:
```bash
wt review task-name
wt review task-name --full  # See complete diff
```

### 6. Kill stuck workers
If a worker is stuck or going in the wrong direction:
```bash
wt kill task-name
wt remove task-name
# Re-spawn with better context
wt spawn task-name --context "Clearer instructions..." --auto
```

## Troubleshooting

### Worker not starting in auto mode
- Check that `--context` is provided (required for auto mode)
- Verify agents/INDEX.md exists if not using `--no-agents`

### Permission errors
- Run `wt setup` to configure Claude permissions
- Check `.claude/settings.json` for allowed commands

### tmux session issues
```bash
# List sessions
tmux ls

# Kill the wt session and start fresh
tmux kill-session -t wt-spawned
```

### Worker finished but no commits
- Check `wt ps` for dirty state
- Attach and check worker: `wt attach task-name`
- The agent may have hit an error or be waiting for input
