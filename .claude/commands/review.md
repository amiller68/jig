---
description: Review branch changes against project conventions
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
  - Functions named `snake_case`, handlers named `handle_<command>()`
  - Errors to stderr with color codes, stdout reserved for eval-able output
  - Variables quoted, conditionals use `[[ ... ]]`
- **Error handling**: Appropriate for the context?
- **Security**: No credentials, injection risks, or unsafe operations?
- **Tests**: Are changes covered by tests in `tests/test_*.sh`? Are new tests needed?
- **Dead code**: Any leftover debug code, commented-out blocks, or unused variables?
- **Versioning**: Does this change warrant a version bump in `manifest.toml`?

### 5. Documentation Check

- Do changes require updates to usage docs in `docs/usage/`? Match changed topic to the right article:
  - `docs/usage/worktrees.md` — creating, opening, removing worktrees; `-o` flag; glob patterns; terminal tabs
  - `docs/usage/configuration.md` — base branch config, config file format, on-create hooks
  - `docs/usage/init.md` — `wt init`, `--audit`, `--backup`, template files
  - `docs/usage/orchestration.md` — `wt spawn`, `wt ps`, `wt attach`, `wt review`, `wt merge`, `wt kill`
  - `docs/usage/shell-integration.md` — tab completion, how `-o` works, `wt which`
- Does `README.md` need updates (new commands in the table, changed install steps, new guide links)?
- Are new functions/commands documented?
- Do `docs/index.md` or `CLAUDE.md` need corrections?

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
