---
title: Core Concepts
slug: core-concepts
date: 2025-03-11
---

jig is built around five pillars: **worktrees**, **documentation**, **issues**, **quality**, and **skills**.

## Worktrees

Git worktrees are the foundation. Each worktree is an isolated checkout of your repository with its own working directory and branch.

```text
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

```text
docs/
├── index.md           # Documentation hub
├── PATTERNS.md        # Coding conventions
└── CONTRIBUTING.md    # How to contribute
```

Plus an `AGENTS.md` at the repo root with:

- Quick reference commands
- Workflow instructions
- Code style guidelines

**Key insight:** Documentation you write for agents is documentation that helps humans too. Invest in it.

## Issues

Well-scoped tickets are the input to agent work. jig uses [Linear](/docs/issues/) as its issue provider — commands like `jig issues` and `jig spawn --issue ENG-123` talk directly to the Linear API.

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

Put these in your `AGENTS.md` or success criteria docs so agents know what "done" means.

### Patterns

Document your conventions in `PATTERNS.md`:

- Error handling approach
- Module structure
- Naming conventions
- Common abstractions

Agents follow patterns they can find. If it's not documented, they'll invent something—possibly something inconsistent with your codebase.

### Review

You're the final gate. When an agent opens a PR, review it on GitHub. Check for:
- Correct implementation
- Adherence to patterns
- No hallucinated requirements
- Test coverage
- No security issues

Leave review comments on draft PRs — the daemon will nudge the agent to address them. Merge via GitHub when satisfied.

## Skills

jig ships with safe defaults for getting a project up and running, but is extensible through bespoke skills.

### What are skills?

Skills are prompt templates that agents can invoke. They live in your agent's config directory and encode workflows, integrations, and conventions specific to your team.

```text
<agent-config>/skills/
├── issues/      # How to work with issues
├── review/      # Code review workflow
├── draft/       # PR drafting conventions
├── check/       # Run project checks
└── your-skill/  # Whatever you need
```

### Extending jig

Skills are starting points — adapt them to how your team works. You can customize existing skills or add entirely new ones for your workflow.

### Built-in skills

jig scaffolds these skills by default:

| Skill | Purpose |
|-------|---------|
| `issues` | Find, create, and manage work items |
| `review` | Review branch changes against conventions |
| `draft` | Create PRs with consistent formatting |
| `check` | Run build, test, lint, format checks |

Each can be customized or replaced entirely.
