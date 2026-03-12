# Tab completions fail outside a git repo without -g flag

**Status:** Complete
**Labels:** auto, urgent, ux, completions

## Objective

Make `jig open` (and other worktree-completing commands) tab-complete against global worktrees when the user is outside a git repository, without requiring `-g`. Drop the `-g` flag from `open` since auto-detection makes it redundant.

## Problem

Currently, `jig open <TAB>` outside a git repo produces no completions because:

1. The `_jig_worktrees` helper in all three shells (bash/zsh/fish) calls `jig list --plain` first
2. Outside a repo, `jig list --plain` fails with "Not in a git repository"
3. The fallback to `jig list -gp` exists but the indented output parsing (`sed -n 's/^  //p'`) is fragile
4. The user must manually pass `-g` to `jig open` to get it to work, but completions don't know about `-g`

## Implementation

### 1. Context-aware completions in shell scripts

Update `_jig_worktrees` in all three shell completion scripts (`shell_init.rs`) to detect whether cwd is inside a git repo:

- **Inside a repo**: complete with `jig list --plain` (local worktrees)
- **Outside a repo**: complete with `jig list -gp` (global worktrees)

```bash
# bash/zsh
if git rev-parse --is-inside-work-tree &>/dev/null; then
    wts=$(command jig list --plain 2>/dev/null)
else
    wts=$(command jig list -gp 2>/dev/null | sed -n 's/^  //p')
fi
```

```fish
if git rev-parse --is-inside-work-tree &>/dev/null
    # local
else
    # global
end
```

### 2. Auto-detect global mode in `jig open`

Make `jig open` automatically use global discovery when outside a repo, so `-g` is no longer needed at runtime.

### 3. Drop `-g` flag from `open`

Remove the `-g`/`--global` flag from `jig open` since it's now redundant — the command auto-detects based on cwd.

## Files

- `crates/jig-cli/src/commands/shell_init.rs` — All three completion script constants (BASH_INIT, ZSH_INIT, FISH_INIT)
- `crates/jig-cli/src/commands/open.rs` — Auto-detect global mode, remove `-g` flag

## Acceptance Criteria

- [ ] `jig open <TAB>` completes worktree names when outside a git repo
- [ ] Inside a repo, completions still show local worktrees
- [ ] `jig open <name>` works outside a repo without needing `-g`
- [ ] `-g` flag removed from `open`
- [ ] All three shells (bash, zsh, fish) updated

## Verification

```bash
cd ~
jig open <TAB>  # Should show global worktrees
cd /some/repo
jig open <TAB>  # Should show local worktrees
```
