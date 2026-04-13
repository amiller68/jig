---
description: Push current branch and create a draft PR. Use when ready to share work for review or collaborate on a branch.
allowed-tools:
  - Bash(git:*)
  - Bash(jig pr:*)
  - Read
  - Glob
  - Grep
---

Create a draft pull request for the current branch.

## Steps

1. Check for uncommitted changes:
   ```
   git status --porcelain
   ```
   If there are uncommitted changes (modified, added, or untracked files):
   a. Run the project's formatter/linter if applicable (check CLAUDE.md or docs/index.md for commands)
   b. Stage all changes: `git add -A`
   c. Create a commit with a descriptive message based on the changes
   d. Use conventional commit format (feat:, fix:, docs:, refactor:, test:, chore:)

2. Create the draft PR:
   ```
   jig pr
   ```
   This automatically pushes the branch and creates a draft PR with the correct base branch (including parent branch resolution for epic children).

3. Return the PR URL to the user.

## Viewing PR feedback

After the PR is created, use `jig pr comments` to view review feedback:

```
jig pr comments              # unaddressed feedback on current branch's PR
jig pr comments --pr 123     # explicit PR number
```

## Important

- **Commit ALL uncommitted changes** before pushing — don't leave anything behind
- Do NOT use `--no-verify` when pushing — let git hooks run
- If the linter/formatter finds issues, fix them before committing
- Use `jig pr` to create PRs — NEVER use `gh pr create` directly
