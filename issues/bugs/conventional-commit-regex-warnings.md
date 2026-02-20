# Fix Conventional Commit Regex Warnings

**Status:** Planned  
**Priority:** Low  
**Category:** Bugs

## Objective

Fix the `grep: warning: stray \ before !` warnings that appear on every grinder run when checking conventional commits.

## Background

The issue-grinder checks commit messages for conventional commit format using this regex:
```bash
conv_regex="^(feat|fix|docs|style|refactor|perf|test|chore|ci)(\(.+\))?\!?:"
```

The `\!` is causing warnings because `!` doesn't need escaping in basic grep (only in extended regex). This warning appears 10+ times per grinder run.

## Root Cause

In the script:
```bash
if ! echo "$commit_msg" | grep -qE "$conv_regex"; then
```

The `-E` flag enables extended regex, but `\!` should just be `!` (or `\\!` if we want to be explicit).

## Fix

Change the regex to:
```bash
conv_regex="^(feat|fix|docs|style|refactor|perf|test|chore|ci)(\(.+\))?!?:"
```

Or more explicitly:
```bash
conv_regex="^(feat|fix|docs|style|refactor|perf|test|chore|ci)(\(.+\))?(\\!)?:"
```

## Acceptance Criteria

- [ ] Fix regex in issue-grinder script
- [ ] Test with various commit messages:
  - `feat: add feature` ✓
  - `feat!: breaking change` ✓
  - `feat(scope): add feature` ✓
  - `feat(scope)!: breaking change` ✓
  - `bad commit message` ✗
- [ ] No grep warnings in output

## File Location

`~/.openclaw/workspace/skills/issue-grinder/grind.sh` line ~880

## Testing

```bash
# Test the regex
conv_regex="^(feat|fix|docs|style|refactor|perf|test|chore|ci)(\(.+\))?!?:"

echo "feat: add feature" | grep -qE "$conv_regex" && echo "✓ Match" || echo "✗ No match"
echo "feat!: breaking" | grep -qE "$conv_regex" && echo "✓ Match" || echo "✗ No match"
echo "feat(scope)!: breaking" | grep -qE "$conv_regex" && echo "✓ Match" || echo "✗ No match"
echo "bad commit" | grep -qE "$conv_regex" && echo "✗ Match" || echo "✓ No match"
```

## Related Issues

- #TBD: GitHub integration (should include proper commit validation)
- #TBD: Pre-commit hooks for commit message validation
