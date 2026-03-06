# Test Auto-Spawn

**Status:** Planned
**Priority:** High
**Category:** Chores
**Auto:** true

## Objective

This is a test issue to verify the daemon's auto-spawn functionality works end-to-end. The worker spawned for this issue should:

1. Read this issue file
2. Create a new file `test-auto-spawn-proof.txt` in the repo root containing "auto-spawn works!"
3. Commit the file
4. Then exit

## Acceptance Criteria

- [ ] Worker was auto-spawned by the daemon
- [ ] `test-auto-spawn-proof.txt` exists with expected content

## Verification

Check `jig ps` to see the spawned worker, then check the worktree for the proof file.
