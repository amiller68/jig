---
layout: page
title: Skill File Examples
nav_order: 3
parent: Appendix
---

# Skill File Examples

Claude Code skill files live at `.claude/skills/<name>/SKILL.md` inside your repo. They teach Claude how to perform domain-specific workflows by combining tools, MCP servers, and CLI commands into a single repeatable action.

Each skill file is a markdown document with YAML frontmatter:

```yaml
---
description: Short description shown in /skills list
tools: [tool1, tool2]
---
```

The body contains instructions Claude follows when the skill is invoked.

## Example: Create Linear Issues via MCP

This example assumes you've set up a user-defined MCP server in Claude Code for the Linear API. In this case, the server is named `linear-aut` — but yours could be called anything. The tool names follow Claude Code's MCP naming convention: `mcp__<server-name>__<method>`.

For instance, if you named your Linear MCP server `my-linear`, the tools would be `mcp__my-linear__list_teams`, `mcp__my-linear__save_issue`, etc.

**Prerequisites:**
- A Linear MCP server configured in your `.claude/settings.json` (see [Claude Code MCP docs](https://docs.anthropic.com/en/docs/claude-code/mcp))
- Linear integration configured in `jig.toml` (see [Linear Integration](linear-integration))

### `.claude/skills/create-issues/SKILL.md`

````markdown
---
description: Create Linear issues with appropriate labels
tools: [mcp__linear-aut__list_teams, mcp__linear-aut__list_issue_labels, mcp__linear-aut__save_issue, Bash]
---

# Create Linear Issues

When the user describes work to be done, create well-structured Linear issues.

## Steps

1. **Discover context** — Use `mcp__linear-aut__list_teams` to find the team configured in `jig.toml`. Use `mcp__linear-aut__list_issue_labels` to find available labels for that team. Check `jig.toml` `[issues] spawn_labels` to see which labels are required for auto-spawn.

2. **Create issues** — For each piece of work, use `mcp__linear-aut__save_issue` with:
   - `teamId`: from step 1
   - `title`: concise summary (under 80 characters)
   - `description`: markdown body following the project's issue format (see below)
   - `labelIds`: include the labels matching `spawn_labels` from `jig.toml`

3. **Verify** — Run `jig issues` to confirm the new issues appear in the local issue list.

## Issue description format

Write the description in the same structure as file-based issues:

```markdown
## Objective

One sentence: what this accomplishes.

## Implementation

1. Step-by-step guide
2. With file paths and code snippets

## Files

- `path/to/file.rs` — Description of changes

## Acceptance Criteria

- [ ] Criterion that can be verified
- [ ] Another criterion
```

## Guidelines

- One issue per logical unit of work — if the user describes multiple tasks, create multiple issues
- Reference specific file paths in the implementation steps
- Write acceptance criteria that an agent can verify programmatically
````

### Usage

```
/create-issues Add rate limiting to the API — per-user limits on POST endpoints,
return 429 with Retry-After header, and add integration tests
```

Claude will discover your team, create the issue with the appropriate labels, and confirm it appears in `jig issues`.

{: .note }
> **Future improvement:** This workflow will be simplified by a native `jig issues create` command that reads team, project, and label config directly from `jig.toml` — no MCP discovery step needed.
