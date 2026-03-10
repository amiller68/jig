# Use checkmark instead of asterisk for AUTO column

**Status:** Planned
**Priority:** Low
**Labels:** auto

## Objective

Change the AUTO column indicator in `jig issues` from `*` to a checkmark for better readability.

## Implementation

1. In `crates/jig-cli/src/commands/issues.rs`, line 233, change:
   ```rust
   let auto_indicator = if issue.auto(spawn_labels) { "✓" } else { "" };
   ```

## Acceptance Criteria

- [ ] AUTO column shows `✓` instead of `*` for auto-eligible issues
