---
description: Review branch changes against project conventions. Use when preparing to merge, checking code quality, or validating changes before PR.
allowed-tools:
  - Bash(git diff:*)
  - Bash(git log:*)
  - Bash(git status)
  - Bash(git branch:*)
  - Read
  - Glob
  - Grep
---

Review the current branch's changes against project conventions before merge.

## Steps

### 1. Gather Context

Read project documentation to understand conventions:
- `CLAUDE.md` — project guide
- `docs/index.md` — agent instructions and coding conventions

### 2. Collect Changes

Get the full picture of what this branch changes:
```
git log main..HEAD --oneline
git diff main...HEAD --stat
git diff main...HEAD
```
If `main` doesn't exist, try `origin/main`.

### 3. Commit Message Audit

Check each commit message:
```
git log main..HEAD --format="%h %s"
```
Verify they are clear, descriptive, and follow the project's conventions.

### 4. Code Review

Review the diff for:
- **Correctness**: Does the logic do what the commit messages claim?
- **Code quality**: Follows existing patterns and conventions?
- **Error handling**: Appropriate for the context?
- **Security**: No credentials, injection risks, or unsafe operations?
- **Tests**: Are changes covered by tests? Are new tests needed?
- **Dead code**: Any leftover debug code, commented-out blocks, or unused imports?

### 5. Documentation Check

- Do changes require README or docs updates?
- Are new functions/commands documented?
- Do existing docs need corrections?

### 6. Issue Cross-Reference

If `issues/` exists, check for related tickets:
- Should any issue status be updated?
- Are there follow-up items to track?

## Output Format

```
## Commit Messages
- [PASS/FAIL] Format and clarity
- Issues: (list or "None")

## Code Review
- [PASS/WARN/FAIL] Correctness
- [PASS/WARN/FAIL] Conventions
- [PASS/WARN/FAIL] Error handling
- [PASS/WARN/FAIL] Security
- [PASS/WARN/FAIL] Test coverage
- Suggestions: (list or "None")

## Documentation
- [PASS/WARN] Updates needed: (list or "None")

## Summary
[Overall assessment and recommended actions before merge]
```

Be specific — reference file paths and line numbers where relevant.
