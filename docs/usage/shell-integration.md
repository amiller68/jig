# Shell Integration

How `jig` integrates with your shell — tab completion, directory changing, and troubleshooting.

## Setup

Add shell integration to your profile:

```bash
# For bash (~/.bashrc)
eval "$(jig shell-init bash)"

# For zsh (~/.zshrc)
eval "$(jig shell-init zsh)"

# For fish (~/.config/fish/config.fish)
eval (jig shell-init fish)
```

This enables:
- The `jig` command with directory-changing support
- The `sc` short alias
- Tab completion for both

## Tab Completion

Both bash and zsh get tab completion:

```bash
jig <TAB>           # Shows: create list open remove exit config spawn ps...
jig open <TAB>      # Shows available worktrees
jig remove <TAB>    # Shows available worktrees
jig config <TAB>    # Shows: base on-create --list
sc <TAB>               # Same completions work with the alias
```

## How the `-o` Flag Works

The `jig` shell function wraps the underlying binary. When you use `open` or the `-o` flag, the binary outputs a `cd` command that the shell function `eval`s:

```bash
# What happens internally:
jig open my-feature  # outputs: cd "/path/to/.worktrees/my-feature"
eval "cd ..."           # shell function evals it
```

This is why `jig open` can change your current directory — it's a shell function, not just an external binary.

## The `sc` Alias

`sc` is a short alias for `jig`:

```bash
sc create feature-auth -o    # Same as: jig create feature-auth -o
sc ps                        # Same as: jig ps
sc open my-feature           # Same as: jig open my-feature
```

The alias supports the same tab completion as `jig`.

## Finding the Binary

Since `jig` is a shell function (required for `cd` functionality), `which jig` shows the function definition instead of a path. Use this instead:

```bash
jig which    # Shows path to the jig executable
```
