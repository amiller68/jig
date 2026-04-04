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

#### 5a. Doc index staleness

Read `docs/index.md` and check the **Documentation Map**. For each file changed in the diff:
- Does it appear in a doc's **Sources** column?
- If yes, read that doc and check if the content is still accurate given the changes
- If a new file was added to a directory that a doc covers (e.g., a new actor in `daemon/`), check if the doc mentions it or needs updating
- If a new module/command was added with no doc coverage, flag it as a gap

#### 5b. Doc content

- `CLAUDE.md` — Does quick reference need updating?
- `docs/PATTERNS.md` — Do any documented patterns need revision?
- `docs/SUCCESS_CRITERIA.md` — Did build/test/lint commands change?
- `docs/CONTRIBUTING.md` — Did contribution workflow change?
- `docs/index.md` — Do Sources columns need new entries for added files?
- README — Does the README need updates for new features?

### 6. Skills Check

If behavior changed that affects skills in `.claude/skills/`:
- `/check` — Did build, test, or lint commands change?
- `/review` — Did review criteria or conventions change?
- `/draft` — Did PR workflow change?
- `/spawn` — Did worker spawning behavior change?
- `/issues` — Did issue tracking conventions change?

Skills must stay in sync with actual project behavior.

### 7. Issue Cross-Reference

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

## Skills
- [PASS/WARN] Updates needed: (list or "None")

## Summary
[Overall assessment and recommended actions before merge]
```

Be specific — reference file paths and line numbers where relevant.
