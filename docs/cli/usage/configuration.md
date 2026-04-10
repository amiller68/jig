# Configuration and Hooks

Configure base branches, on-create hooks, and file copying per-repo or globally.

## jig.toml (Recommended)

The recommended way to configure jig is via `jig.toml` in your repo root. This file is committed and shared with your team:

```toml
[worktree]
base = "origin/main"           # Base branch for new worktrees
on_create = "npm install"      # Command to run after worktree creation
copy = [".env", ".env.local"]  # Gitignored files to copy to new worktrees

[agent]
type = "claude"                # Agent framework (claude, cursor)

[issues]
provider = "linear"            # Issue provider ("file" or "linear")
auto_spawn_labels = ["jig-auto"] # Only auto-spawn issues with all these labels

[issues.linear]
profile = "work"               # References ~/.config/jig/config.toml profile
team = "ENG"                   # Linear team key
projects = ["Backend"]         # Optional project filter

[triage]
enabled = true                 # Enable triage auto-spawn (default: true)
model = "sonnet"               # Model for triage agents (default: "sonnet")
timeout_seconds = 600          # Max triage duration before stuck detection
```

**Priority:** jig.toml settings override global config.

---

## jig.local.toml (Machine-Specific Overrides)

For settings that shouldn't be committed to version control (like `auto_spawn_labels` on a dev machine), create `jig.local.toml` alongside `jig.toml`:

```toml
# jig.local.toml — gitignored, not shared with team
[issues]
auto_spawn_labels = []    # enable auto-spawn on this machine only
```

`jig.local.toml` is deep-merged on top of `jig.toml`:
- **Tables** merge recursively — keys from both files are combined.
- **Scalars and arrays** from `jig.local.toml` replace the base value.
- `jig.local.toml` alone (without `jig.toml`) has no effect — the base file must exist.

`jig init` automatically adds `jig.local.toml` to `.gitignore`.

When `jig.local.toml` is active, `jig config` shows source attribution as `(jig.toml + jig.local.toml)`.

---

## Global Config

You can also configure settings globally via `jig config`. This is stored in `~/.config/jig/config`.

### Base Branch

By default, new branches are created from `origin/main`. Override this per-repo or globally:

```bash
# Set base branch for current repo
jig config base origin/develop

# Set global default (used when no repo config exists)
jig config base --global origin/main

# View current config
jig config

# List all configuration
jig config --list

# Unset repo config
jig config base --unset

# Unset global default
jig config base --global --unset
```

**Resolution order:**
1. Repo-specific config
2. Global default
3. Hardcoded fallback (`origin/main`)

## Config File Format

Configuration is stored in `~/.config/jig/config` (follows XDG spec). The file uses simple `key=value` pairs, one per line:

```bash
# View config file
cat ~/.config/jig/config

# Edit manually
$EDITOR ~/.config/jig/config
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
jig config on-create 'pnpm install'

# View current hook
jig config on-create

# Create without running hook
jig create feature-branch --no-hooks

# Unset hook
jig config on-create --unset
```

Hooks run in the new worktree directory after creation. If a hook fails, a warning is displayed but the worktree remains usable.

**Examples:**

```bash
jig config on-create 'npm install'           # Node.js project
jig config on-create 'uv sync'               # Python UV project
jig config on-create 'make install'          # Makefile-based project
jig config on-create 'bundle install'        # Ruby project
```

## Copying Gitignored Files

Some files like `.env` are gitignored but needed in worktrees. Configure jig.toml to copy them automatically:

```toml
[worktree]
copy = [".env", ".env.local", ".secrets"]
```

Files are copied from the repo root to the new worktree after creation, before the on_create hook runs. Missing files are silently skipped.

## Issue Provider

By default, jig uses file-based issues from the `issues/` directory. You can switch to Linear by setting `provider = "linear"` in `jig.toml` and adding a Linear API key to your global config.

See [Linear Integration](./linear-integration.md) for full setup instructions.

### Labels

Issues support labels for tagging and filtering. File-based issues use `**Labels:**` frontmatter:

```markdown
**Labels:** backend, sprint-1, auth
```

Linear issues use their native label system — labels are fetched from the GraphQL API.

Filter by label with the `--label` / `-l` flag:

```bash
jig issues --label backend                    # issues with "backend" label
jig issues --label backend --label sprint-1   # must have BOTH labels
```

Label matching is case-insensitive.

### Auto-spawn with `auto_spawn_labels`

The `auto_spawn_labels` config in `[issues]` controls which issues the daemon auto-spawns:

```toml
[issues]
auto_spawn_labels = ["jig-auto"]           # only auto-spawn issues labeled "jig-auto"
auto_spawn_labels = ["backend", "sprint-1"] # must have BOTH labels
auto_spawn_labels = []                      # spawn ALL planned issues
# (omit auto_spawn_labels entirely to disable auto-spawn)
```

When `auto_spawn_labels` is absent (the default), auto-spawn is disabled. When set to `[]`, all planned issues with satisfied dependencies are eligible. The AUTO column in `jig issues` shows `✓` for issues matching the configured labels.

## Triage Configuration

The `[triage]` section controls triage worker behavior — lightweight, read-only agents that investigate new issues before they're queued for implementation:

```toml
[triage]
enabled = true           # Enable triage auto-spawn (default: true)
model = "sonnet"         # Model for triage agents (default: "sonnet")
timeout_seconds = 600    # Max duration before stuck detection (default: 600)
```

Triage workers run in ephemeral (one-shot) mode with restricted tool access: `Read`, `Glob`, `Grep`, `Bash(jig *)`, and `mcp__linear*`. They investigate the issue, append findings to the issue description, transition it to Backlog, then exit. See [daemon docs](../../daemon.md#triage-verification) for the post-spawn lifecycle.
