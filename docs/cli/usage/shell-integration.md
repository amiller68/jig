# Shell Integration

How `jig` integrates with your shell — tab completion, directory changing, and troubleshooting.

## Quick Setup

The easiest way to configure shell integration:

```bash
jig shell-setup
```

This automatically detects your shell and adds the integration to your config file.

Use `--dry-run` to preview changes without modifying anything:
```bash
jig shell-setup --dry-run
```

## Manual Setup

Alternatively, add shell integration manually to your profile:

```bash
# For bash (~/.bashrc)
eval "$(jig shell-init bash)"

# For zsh (~/.zshrc)
eval "$(jig shell-init zsh)"

# For fish (~/.config/fish/config.fish)
jig shell-init fish | source
```

This enables:
- The `jig` command with directory-changing support
- Tab completion

## Tab Completion

Both bash and zsh get tab completion:

```bash
jig <TAB>           # Shows: create list open remove exit config spawn ps...
jig open <TAB>      # Shows available worktrees
jig remove <TAB>    # Shows available worktrees
jig config <TAB>    # Shows: base on-create --list
```

## How the `-o` Flag Works

The `jig` shell function wraps the underlying binary. When you use `open` or the `-o` flag, the binary outputs a `cd` command that the shell function `eval`s:

```bash
# What happens internally:
jig open my-feature  # outputs: cd "/path/to/.worktrees/my-feature"
eval "cd ..."           # shell function evals it
```

This is why `jig open` can change your current directory — it's a shell function, not just an external binary.

## Troubleshooting

### Shell function not loading

If `type jig` shows a binary path instead of "jig is a shell function", the shell wrapper isn't active. This means:
- The `-o` flag won't change directories (it will print `cd '/path'` instead)
- Tab completion may not work

**Common cause:** The eval line runs before PATH includes the jig binary location.

**Fix:** Ensure the eval line comes AFTER your PATH is set:

```bash
# ~/.zshrc or ~/.bashrc

# First: set up PATH (e.g., cargo, homebrew, etc.)
export PATH="$HOME/.cargo/bin:$PATH"

# Then: initialize jig shell integration
eval "$(jig shell-init zsh)"
```

After fixing, open a new terminal and verify:
```bash
type jig  # Should show: jig is a shell function
```

## Finding the Binary

Since `jig` is a shell function (required for `cd` functionality), `which jig` shows the function definition instead of a path. Use this instead:

```bash
jig which    # Shows path to the jig executable
```
