# Native issue creation command

**Status:** Complete
**Labels:** auto

## Objective

Add a `jig issues create` command that creates Linear issues using per-repo config from `jig.toml`, eliminating the need for raw Linear MCP tool calls.

## Context

Currently, creating Linear issues from within Claude Code requires a skill file that calls `mcp__linear-aut__list_teams`, `mcp__linear-aut__list_issue_labels`, and `mcp__linear-aut__save_issue` directly. The user (or skill) must discover team IDs and label IDs at runtime. This is unnecessary friction — `jig.toml` already has the team, project, and profile config.

## Implementation

1. Add `create` subcommand to the issues CLI in `crates/jig-cli/src/commands/issues.rs`
2. Accept `--title` and `--body` (or positional title + stdin body)
3. Read team, project, and profile from `jig.toml` `[issues.linear]` config
4. Resolve the `jig-auto` label ID from the configured team
5. Call the Linear API to create the issue with title, body, team, and label
6. Print the issue ID to stdout for scripting (`ENG-456`)
7. Support `--spawn` flag to immediately spawn a worker for the new issue

### CLI interface

```bash
# Basic usage
jig issues create --title "Add rate limiting" --body "Per-user limits on POST endpoints..."

# With auto-spawn
jig issues create --title "Add rate limiting" --body "..." --spawn --auto

# Pipe body from stdin
echo "Full description here" | jig issues create --title "Add rate limiting"
```

## Files

- `crates/jig-cli/src/commands/issues.rs` — Add `Create` subcommand
- `crates/jig-core/src/github/` — May need Linear API create method if not already present

## Acceptance Criteria

- [ ] `jig issues create --title "..." --body "..."` creates a Linear issue using repo config
- [ ] Issue is created with the `jig-auto` label automatically
- [ ] Issue ID is printed to stdout
- [ ] `--spawn` flag spawns a worker for the new issue
- [ ] Errors clearly if Linear provider is not configured
- [ ] Update `wiki/appendix/skill-examples.md` to reference this command once shipped

## Verification

```bash
# Create an issue
jig issues create --title "Test issue" --body "Testing native creation"

# Verify it appears
jig issues

# Create and spawn in one step
jig issues create --title "Auto task" --body "..." --spawn --auto
jig ls  # should show new worker
```
