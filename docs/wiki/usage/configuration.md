# Configuration

Configure base branches and on-create hooks per-repo or globally.

## Base Branch

By default, new branches are created from `origin/main`. Override per-repo or globally:

```bash
# Set base branch for current repo
wt config base origin/develop

# Set global default
wt config base --global origin/main

# View current config
wt config

# Unset repo config
wt config base --unset
```

**Resolution order:**
1. Repo-specific config
2. Global default
3. Fallback (`origin/main`)

## On-Create Hooks

Run commands automatically after creating worktrees:

```bash
# Set on-create hook for current repo
wt config on-create 'pnpm install'

# View current hook
wt config on-create

# Create without running hook
wt create feature-branch --no-hooks

# Unset hook
wt config on-create --unset
```

Hooks run in the new worktree directory. If a hook fails, a warning is shown but the worktree remains.

**Examples:**

```bash
wt config on-create 'npm install'       # Node.js
wt config on-create 'uv sync'           # Python UV
wt config on-create 'bundle install'    # Ruby
```

## Config File

Configuration is stored in `~/.config/wt/config`:

```ini
# Global default
_default=origin/main

# Per-repo base branch
/Users/you/projects/my-app=origin/develop

# Per-repo on-create hook
/Users/you/projects/my-app:on_create=pnpm install
```
