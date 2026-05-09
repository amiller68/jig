# Documentation Index

Central hub for project documentation. AI agents should read this first.

## Quick Start

<!-- Add your quick start commands here. Examples:

```bash
# Install dependencies
npm install

# Run development server
npm run dev

# Run tests
npm test
```
-->

## Documentation

| Document | Purpose |
|----------|---------|
| [PATTERNS.md](./PATTERNS.md) | Coding conventions and patterns |
| [CONTRIBUTING.md](./CONTRIBUTING.md) | How to contribute (agents + humans) |
| [SUCCESS_CRITERIA.md](./SUCCESS_CRITERIA.md) | CI checks that must pass |

## For AI Agents

You are an autonomous coding agent working on a focused task.

### Workflow

1. **Understand** — Read the task description and relevant docs
2. **Explore** — Search the codebase to understand context
3. **Plan** — Break down work into small steps
4. **Implement** — Follow existing patterns in `PATTERNS.md`
5. **Verify** — Run checks from `SUCCESS_CRITERIA.md`
6. **Commit** — Clear, atomic commits using conventional format
7. **Draft** — Push and create a draft PR with `/draft`

### Guidelines

- Follow existing code patterns and conventions
- Make atomic commits (one logical change per commit)
- Add tests for new functionality
- Update documentation if behavior changes
- If blocked, commit what you have and note the blocker

### Working on sub-issues

If your task is a sub-issue of a parent epic, `jig pr` automatically targets the parent's integration branch — not main. Don't override the base branch manually.

### When Complete

Run `/review` to self-check, then `/draft` to push and open a draft PR. The daemon monitors draft PRs and will nudge you about CI failures or review comments.
