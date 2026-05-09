---
title: Init & Setup
slug: init
date: 2025-05-09
---

Installation, shell integration, repository setup, and configuration.

## Installation

```bash
curl -fsSL https://raw.githubusercontent.com/amiller68/jig/main/install.sh | bash
```

Or build from source:

```bash
git clone https://github.com/amiller68/jig
cd jig
cargo install --path crates/jig-cli
```

## Shell Integration

### Quick setup

```bash
jig shell-setup
```

This detects your shell and adds integration automatically. Use `--dry-run` to preview changes.

### Manual setup

```bash
# For bash (~/.bashrc)
eval "$(jig shell-init bash)"

# For zsh (~/.zshrc)
eval "$(jig shell-init zsh)"

# For fish (~/.config/fish/config.fish)
jig shell-init fish | source
```

This enables the `jig` command with directory-changing support and tab completion.

### Tab completion

```bash
jig <TAB>           # Shows: create list open remove exit config spawn ps...
jig open <TAB>      # Shows available worktrees
jig remove <TAB>    # Shows available worktrees
```

### How the `-o` flag works

The `jig` shell function wraps the binary. When you use `open` or the `-o` flag, the binary outputs a `cd` command that the shell function evals — that's why `jig open` can change your directory.

### Troubleshooting

If `type jig` shows a binary path instead of "jig is a shell function", the shell wrapper isn't active. The `-o` flag won't change directories and tab completion may not work.

**Fix:** Ensure the eval line comes AFTER your PATH is set:

```bash
# ~/.zshrc or ~/.bashrc
export PATH="$HOME/.cargo/bin:$PATH"
eval "$(jig shell-init zsh)"
```

Since `jig` is a shell function, `which jig` shows the function definition. Use `jig which` to find the binary path.

## Initialize a Repository

```bash
cd ~/projects/my-app
jig init
```

This scaffolds:

```text
my-project/
├── AGENTS.md                 # Project guide — points agents at docs/
├── jig.toml                  # jig spawn configuration
├── docs/
│   └── index.md              # Agent instructions (key files, conventions, how to build/test)
└── <agent-config>/
    ├── settings              # Permissions (git, gh, file ops, etc.)
    └── skills/
        ├── check/            # /check — run build/test/lint
        ├── draft/            # /draft — push and create draft PR
        └── issues/           # /issues — discover and manage work items
```

If files already exist, init skips them. Use `--force` to overwrite, `--backup` to save existing files first:

```bash
jig init --force --backup
```

### Auditing with `--audit`

Let the agent populate your docs automatically:

```bash
jig init --audit
```

This launches the agent in a `jig-init` tmux session to audit the codebase and fill in the skeleton docs. Attach with `tmux attach -t jig-init`. Pass extra instructions:

```bash
jig init --audit "We use pnpm, not npm. The API is actix-web."
```

Combine with backup to give the agent your existing files as reference:

```bash
jig init --force --backup --audit
```

### What each file does

| File | Purpose |
|------|---------|
| `AGENTS.md` | Entry point agents read. Points to `docs/` for details. |
| `docs/index.md` | Core instructions: project structure, build commands, conventions. |
| Agent config | Permissions and settings for your agent framework. |
| Skills | Slash commands available in agent sessions (`/check`, `/draft`, `/issues`). |
| `jig.toml` | Spawn and issue configuration (see below). |

## Configuration

### jig.toml

The primary config file, committed to your repo:

```toml
[worktree]
base = "origin/main"           # Base branch for new worktrees
on_create = "npm install"      # Command to run after worktree creation
copy = [".env", ".env.local"]  # Gitignored files to copy into new worktrees

[agent]
type = "claude"                # Agent framework (claude, codex, cursor)

[issues]
provider = "linear"            # Issue provider
[issues.linear]
profile = "work"               # References ~/.config/jig/config.toml profile
team = "ENG"                   # Linear team key
projects = ["Backend"]         # Optional project filter

[triage]
enabled = true                 # Enable triage auto-spawn (default: false)
model = "sonnet"               # Model for triage agents
timeout_seconds = 600          # Max triage duration
```

### jig.local.toml

Machine-specific overrides that shouldn't be committed. Useful for setting a Linear profile per-developer:

```toml
# jig.local.toml — gitignored, not shared with team
[issues.linear]
profile = "personal"    # use a different Linear profile on this machine
```

Deep-merged on top of `jig.toml`: tables merge recursively, scalars and arrays replace. `jig init` adds `jig.local.toml` to `.gitignore` automatically.

### Global config

Override settings globally via `jig config`:

```bash
jig config base origin/develop        # Set base branch for current repo
jig config base --global origin/main  # Set global default
jig config on-create 'npm install'    # Set on-create hook
jig config                            # View current config
jig config --list                     # List all configuration
```

Resolution order: repo-specific `jig.toml` > global config > hardcoded fallback (`origin/main`).

### Copying gitignored files

Files like `.env` are gitignored but needed in worktrees. Configure in `jig.toml`:

```toml
[worktree]
copy = [".env", ".env.local", ".secrets"]
```

Files are copied from repo root to new worktrees before the `on_create` hook runs. Missing files are silently skipped.

## Iterating after init

`jig init` is a first pass. Iterate on the generated docs — ask your agent to improve `docs/index.md`, or edit by hand. Consider adding:

- **Code patterns** — error handling, naming, module structure
- **Dev workflows** — local environment setup, migrations, seed data
- **PR criteria** — what must pass before merge
- **Architecture decisions** — why things are structured this way

The skills (`/check`, `/draft`, `/issues`) are also generic defaults. Customize them for your project's actual build commands and workflows.
