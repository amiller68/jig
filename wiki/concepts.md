---
layout: page
title: Core Concepts
nav_order: 3
---

# Core Concepts

jig is built around five pillars: **worktrees**, **documentation**, **issues**, **quality**, and **skills**.

## Worktrees

Git worktrees are the foundation. Each worktree is an isolated checkout of your repository with its own working directory and branch.

```
main (your orchestration session)
 └── feature-auth/      # Agent working on auth
 └── fix-pagination/    # Agent fixing pagination bug
 └── add-tests/         # Agent writing tests
```

**Why worktrees for agents?**

- **Isolation** — Agents can't step on each other's work
- **Parallelism** — Run multiple agents simultaneously
- **Clean merges** — Each worktree has its own branch, making integration straightforward
- **Easy cleanup** — Remove a worktree when done, no lingering files

jig manages worktree lifecycle:

```bash
jig create feature-x     # Create worktree
jig spawn feature-x      # Create + launch agent
jig list                 # See all worktrees
jig remove feature-x     # Clean up
```

## Documentation

Agents need context. The more discoverable and well-organized your documentation, the faster agents can be productive.

jig scaffolds a `docs/` structure:

```
docs/
├── index.md           # Documentation hub
├── PATTERNS.md        # Coding conventions
├── CONTRIBUTING.md    # How to contribute
└── PROJECT_LAYOUT.md  # Codebase structure
```

Plus a `CLAUDE.md` (or agent-specific config) at the repo root with:

- Quick reference commands
- Workflow instructions
- Code style guidelines

**Key insight:** Documentation you write for agents is documentation that helps humans too. Invest in it.

## Issues

Well-scoped tickets are the input to agent work. jig supports file-based issue tracking in `issues/`:

```
issues/
├── _template.md
├── 001-implement-auth.md
├── 002-fix-pagination.md
└── 003-add-tests.md
```

Each issue has frontmatter:

```markdown
---
id: 001
title: Implement JWT authentication
status: ready
priority: 2
---

## Description

Add JWT-based authentication to the API.

## Acceptance Criteria

- [ ] POST /auth/login returns JWT
- [ ] Middleware validates tokens
- [ ] Tests cover happy path and errors
```

**Status values:**
- `draft` — Not ready
- `ready` — Can be picked up
- `in_progress` — Being worked on
- `review` — Done, needs review
- `done` — Complete

The discipline of writing detailed issue descriptions pays dividends. Agents work better with clear scope, explicit acceptance criteria, and relevant context.

## Quality

Agents write code. You ensure it's good code. jig emphasizes:

### Checks

Define runnable checks that agents (and humans) can execute:

```bash
cargo build              # Does it compile?
cargo test               # Do tests pass?
cargo clippy             # Linter happy?
cargo fmt --check        # Formatted correctly?
```

Put these in your `CLAUDE.md` or success criteria docs so agents know what "done" means.

### Patterns

Document your conventions in `PATTERNS.md`:

- Error handling approach
- Module structure
- Naming conventions
- Common abstractions

Agents follow patterns they can find. If it's not documented, they'll invent something—possibly something inconsistent with your codebase.

### Review

You're the final gate. When an agent marks work done:

```bash
jig review feature-x    # See the diff
```

Check for:
- Correct implementation
- Adherence to patterns
- No hallucinated requirements
- Test coverage
- No security issues

Then merge or send back with feedback.

## Skills

jig ships with safe defaults for getting a project up and running, but is extensible through bespoke skills.

### What are skills?

Skills are prompt templates that agents can invoke. They live in `.claude/skills/` and encode workflows, integrations, and conventions specific to your team.

```
.claude/skills/
├── issues/      # How to work with issues
├── review/      # Code review workflow
├── draft/       # PR drafting conventions
├── check/       # Run project checks
└── your-skill/  # Whatever you need
```

### Extending jig

Don't want file-based issue tracking? Rewrite the issues skill to integrate with an MCP server of your choice:

```markdown
# issues skill (Linear integration)

Use the Linear MCP server to find and manage issues.

## Finding issues
- Use `mcp__linear__list_issues` to find ready issues
- Filter by assignee "me" for your queue

## Updating status
- Move to "In Progress" when starting
- Move to "In Review" when done
```

The same applies to any workflow. jig's defaults are starting points—adapt them to how your team works.

### Built-in skills

jig scaffolds these skills by default:

| Skill | Purpose |
|-------|---------|
| `issues` | Find, create, and manage work items |
| `review` | Review branch changes against conventions |
| `draft` | Create PRs with consistent formatting |
| `check` | Run build, test, lint, format checks |
| `spawn` | Spawn parallel workers for tasks |

Each can be customized or replaced entirely.
