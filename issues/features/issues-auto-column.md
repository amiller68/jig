# Show auto-spawn indicator in `jig issues` table

**Status:** Planned
**Priority:** Urgent
**Auto:** true

## Objective

Add an AUTO column to the `jig issues` table output so users can see at a glance which issues are tagged for auto-completion by the daemon.

## Context

The `jig issues` command currently shows STATUS, PRI, CATEGORY, and ISSUE columns. There is no way to tell from the listing whether an issue will be auto-spawned. Users must open individual issue files or check Linear labels to determine this.

Both providers already parse the auto flag:
- **File provider:** reads the `**Auto:**` frontmatter field (true/false/1/yes)
- **Linear provider:** checks for the `jig-auto` label on the issue

The `Issue` struct already carries `auto: bool` — it just isn't rendered in the table.

## Implementation

1. Add an `AUTO` column header in `render_table()` in `crates/jig-cli/src/commands/issues.rs`
2. For each issue row, render a compact indicator (e.g. checkmark or dot) when `issue.auto` is true, empty when false
3. Place the column after STATUS and before PRI to keep related metadata grouped

## Files

- `crates/jig-cli/src/commands/issues.rs` — Add AUTO column to `render_table()`

## Acceptance Criteria

- [ ] `jig issues` table includes an AUTO column
- [ ] Auto-tagged issues show an indicator; non-auto issues show blank
- [ ] Works for both file and Linear providers (both already set `issue.auto`)
- [ ] No change to `--ids` or detail output modes

## Verification

```bash
# List issues and verify AUTO column appears
jig issues

# Check a known auto-tagged issue shows the indicator
# Check a non-auto issue shows blank in the column
```
