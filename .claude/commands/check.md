---
description: Run project checks (build, test, lint, format)
allowed-tools:
  - Bash(./test.sh:*)
  - Bash(ls:*)
  - Read
  - Glob
  - Grep
---

Run the test suite to validate code quality.

## Steps

1. Run the test suite:
   ```bash
   ./test.sh
   ```

2. Report a summary of pass/fail status.

3. If any tests fail, read the relevant test module in `tests/` and the code under test to diagnose the issue.

This is the gate for all PRs — all tests must pass before merge.
There is no build step, linter, or formatter for this project. `./test.sh` is the only check.
