# attach fails outside a git repo without -g flag

**Status:** Planned
**Labels:** auto, urgent, ux, completions

## Objective

Make `jig attach` auto-detect global mode when outside a git repository, matching the fix applied to `jig open`. Drop the `-g` flag from `attach` since auto-detection makes it redundant.

## Problem

Same behavior as the `open` bug: `jig attach <TAB>` outside a git repo produces no completions, and `jig attach <name>` fails without `-g`. The shell completion fix (context-aware `_jig_worktrees`) already covers the tab-completion side, but the `attach` command itself still requires `-g` to discover global worktrees outside a repo.

## Implementation

1. Make `jig attach` auto-detect whether it's inside a git repo
2. If outside a repo, automatically use global discovery
3. Drop the `-g`/`--global` flag from `attach` (redundant once auto-detection is in place)

## Files

- `crates/jig-cli/src/commands/attach.rs` — Auto-detect global mode, remove `-g` flag

## Acceptance Criteria

- [ ] `jig attach <TAB>` completes session names when outside a git repo (already fixed by completions change)
- [ ] `jig attach <name>` works outside a repo without needing `-g`
- [ ] `-g` flag removed from `attach`

## Verification

```bash
cd ~
jig attach <TAB>   # Should show sessions
jig attach <name>  # Should work without -g
```
