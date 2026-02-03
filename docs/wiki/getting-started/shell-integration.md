# Shell Integration

The install script sets up shell integration automatically. Here's what it provides.

## Tab Completion

Works after install:

```bash
wt <TAB>           # Shows: create list open remove exit config update version
wt open <TAB>      # Shows available worktrees
wt remove <TAB>    # Shows available worktrees
```

## How `wt open` Works

The `wt` shell function wraps the underlying `_wt` script. When you use `open` or the `-o` flag, the script outputs a `cd` command that the shell function evals:

```bash
wt open my-feature    # cd's into the worktree
wt create feature -o  # creates and cd's into it
```

This is why `wt open` can change your directory — it's a shell function, not an external script.

## `wt which`

Since `wt` is a shell function, `which wt` shows the function definition. Use this instead:

```bash
wt which    # Shows path to the underlying _wt script
```
