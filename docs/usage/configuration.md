# Configuration and Hooks

Configure base branches and on-create hooks per-repo or globally.

## Base Branch

By default, new branches are created from `origin/main`. Override this per-repo or globally:

```bash
# Set base branch for current repo
scribe config base origin/develop

# Set global default (used when no repo config exists)
scribe config base --global origin/main

# View current config
scribe config

# List all configuration
scribe config --list

# Unset repo config
scribe config base --unset

# Unset global default
scribe config base --global --unset
```

**Resolution order:**
1. Repo-specific config
2. Global default
3. Hardcoded fallback (`origin/main`)

## Config File Format

Configuration is stored in `~/.config/scribe/config` (follows XDG spec). The file uses simple `key=value` pairs, one per line:

```bash
# View config file
cat ~/.config/scribe/config

# Edit manually
$EDITOR ~/.config/scribe/config
```

**Format reference:**

```ini
# Global default base branch
_default=origin/main

# Per-repo base branch (key is the absolute repo path)
/Users/you/projects/my-app=origin/develop
/Users/you/projects/api=origin/main

# Per-repo on-create hooks (key is repo path + ":on_create" suffix)
/Users/you/projects/my-app:on_create=pnpm install
/Users/you/projects/api:on_create=make deps
```

**Key patterns:**

| Key | Description |
|-----|-------------|
| `_default` | Global default base branch |
| `/path/to/repo` | Repo-specific base branch |
| `/path/to/repo:on_create` | Repo-specific on-create hook |

## On-Create Hooks

Run commands automatically when creating worktrees. Useful for installing dependencies:

```bash
# Set on-create hook for current repo
scribe config on-create 'pnpm install'

# View current hook
scribe config on-create

# Create without running hook
scribe create feature-branch --no-hooks

# Unset hook
scribe config on-create --unset
```

Hooks run in the new worktree directory after creation. If a hook fails, a warning is displayed but the worktree remains usable.

**Examples:**

```bash
scribe config on-create 'npm install'           # Node.js project
scribe config on-create 'uv sync'               # Python UV project
scribe config on-create 'make install'          # Makefile-based project
scribe config on-create 'bundle install'        # Ruby project
```
