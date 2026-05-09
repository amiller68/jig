---
description: Review branch changes against project conventions. Use when preparing to merge, checking code quality, or validating changes before PR.
allowed-tools:
  - Bash(git diff:*)
  - Bash(git log:*)
  - Bash(git status)
  - Bash(git branch:*)
  - Bash(jig pr:*)
  - Read
  - Glob
  - Grep
---

Review the current branch's changes against project conventions before merge.

## Steps

### 1. Gather Context

Read project documentation to understand conventions:
- `AGENTS.md` — project guide
- `docs/index.md` — agent instructions and coding conventions

### 2. Collect Changes

Get the full picture of what this branch changes:
```
git log main..HEAD --oneline
git diff main...HEAD --stat
git diff main...HEAD
```
If `main` doesn't exist, try `origin/main`.

### 3. Check Review Feedback

If a PR exists for this branch, check for unaddressed review comments:
```
jig pr comments
```
Flag any unresolved feedback that should be addressed before merge.

### 4. Commit Message Audit

Check each commit message:
```
git log main..HEAD --format="%h %s"
```
Verify they are clear, descriptive, and follow the project's conventions.

### 5. Code Review

Review the diff for:
- **Correctness**: Does the logic do what the commit messages claim?
- **Code quality**: Follows existing patterns and conventions?
- **Error handling**: Appropriate for the context?
- **Security**: No credentials, injection risks, or unsafe operations?
- **Tests**: Are changes covered by tests? Are new tests needed?
- **Dead code**: Any leftover debug code, commented-out blocks, or unused imports?

### 6. Documentation Check

- `AGENTS.md` — Does quick reference need updating?
- `docs/PATTERNS.md` — Do any documented patterns need revision?
- `docs/SUCCESS_CRITERIA.md` — Did build/test/lint commands change?
- `docs/CONTRIBUTING.md` — Did contribution workflow change?
- `docs/index.md` — Do documentation references need new entries for added files?
- README — Does the README need updates for new features?

### 7. Skills Check

If behavior changed that affects skills:
- `/check` — Did build, test, or lint commands change?
- `/review` — Did review criteria or conventions change?
- `/draft` — Did PR workflow change?
- `/issues` — Did issue tracking conventions change?

Skills must stay in sync with actual project behavior.

## Output Format

```
## Review Feedback
- [PASS/WARN] Unaddressed comments: (list or "None")

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

## Skills
- [PASS/WARN] Updates needed: (list or "None")

## Summary
[Overall assessment and recommended actions before merge]
```

Be specific — reference file paths and line numbers where relevant.
