# Fix Conventional Commit Regex Warnings

**Status:** Done
**Priority:** Low
**Category:** Bugs

## Objective

Fix the `grep: warning: stray \ before !` warnings that appear on every grinder run when checking conventional commits.

## Resolution

The external `grind.sh` script (`~/.openclaw/workspace/skills/issue-grinder/grind.sh`)
that contained the buggy bash regex has been superseded by jig's native Rust
implementation in `crates/jig-core/src/github/detect.rs`.

The Rust regex is correct and has no warnings:
```rust
r"^(feat|fix|docs|style|refactor|perf|test|chore|ci|build|revert)(\(.+\))?!?: .+"
```

This handles all conventional commit formats including breaking changes (`!`)
and is covered by unit tests (`conventional_commit_regex_valid`,
`conventional_commit_regex_invalid`).

## Original Problem

The bash grinder used `\!` in an extended regex, which caused `grep` warnings:
```bash
# Bad — \! is a stray escape in ERE
conv_regex="^(feat|fix|docs|style|refactor|perf|test|chore|ci)(\(.+\))?\!?:"
echo "$msg" | grep -qE "$conv_regex"
```

The fix would have been to use `!` unescaped (`!?:`), but since jig now handles
this natively in Rust, the grind.sh script is no longer in use.
