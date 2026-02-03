# Shell Integration

How `scribe` integrates with your shell — tab completion, directory changing, and troubleshooting.

## Setup

Add shell integration to your profile:

```bash
# For bash (~/.bashrc)
eval "$(scribe shell-init bash)"

# For zsh (~/.zshrc)
eval "$(scribe shell-init zsh)"

# For fish (~/.config/fish/config.fish)
eval (scribe shell-init fish)
```

This enables:
- The `scribe` command with directory-changing support
- The `sc` short alias
- Tab completion for both

## Tab Completion

Both bash and zsh get tab completion:

```bash
scribe <TAB>           # Shows: create list open remove exit config spawn ps...
scribe open <TAB>      # Shows available worktrees
scribe remove <TAB>    # Shows available worktrees
scribe config <TAB>    # Shows: base on-create --list
sc <TAB>               # Same completions work with the alias
```

## How the `-o` Flag Works

The `scribe` shell function wraps the underlying binary. When you use `open` or the `-o` flag, the binary outputs a `cd` command that the shell function `eval`s:

```bash
# What happens internally:
scribe open my-feature  # outputs: cd "/path/to/.worktrees/my-feature"
eval "cd ..."           # shell function evals it
```

This is why `scribe open` can change your current directory — it's a shell function, not just an external binary.

## The `sc` Alias

`sc` is a short alias for `scribe`:

```bash
sc create feature-auth -o    # Same as: scribe create feature-auth -o
sc ps                        # Same as: scribe ps
sc open my-feature           # Same as: scribe open my-feature
```

The alias supports the same tab completion as `scribe`.

## Finding the Binary

Since `scribe` is a shell function (required for `cd` functionality), `which scribe` shows the function definition instead of a path. Use this instead:

```bash
scribe which    # Shows path to the scribe executable
```
