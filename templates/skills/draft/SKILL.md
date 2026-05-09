---
description: Push current branch and create a draft PR. Use when ready to share work for review or collaborate on a branch.
allowed-tools:
  - Bash(git:*)
  - Bash(jig:*)
  - Read
  - Glob
  - Grep
---

Create a draft pull request for the current branch.

## Steps

1. Get the current branch name:
   ```
   git branch --show-current
   ```

2. Check for uncommitted changes:
   ```
   git status --porcelain
   ```
   If there are uncommitted changes (modified, added, or untracked files):
   a. Run the project's formatter/linter if applicable (check AGENTS.md or docs/index.md for commands)
   b. Stage all changes: `git add -A`
   c. Create a commit with a descriptive message based on the changes
   d. Use conventional commit format (feat:, fix:, docs:, refactor:, test:, chore:)

3. Create the draft PR:
   ```
   jig pr
   ```
   This automatically pushes the branch and creates a draft PR with the correct base branch.

4. Check for existing review feedback:
   ```
   jig pr comments
   ```
   If there are unaddressed review comments, fix them before marking the PR ready for review.

5. Return the PR URL to the user.

## Important

- **Commit ALL uncommitted changes** before pushing — don't leave anything behind
- Do NOT use `--no-verify` when pushing — let git hooks run
- If the linter/formatter finds issues, fix them before committing
- Use `jig pr comments` to check for review feedback — don't use `gh` directly
