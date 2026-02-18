---
layout: page
title: Tips & FAQ
nav_order: 7
---

# Tips & FAQ

## FAQ

### Is this the end of software engineering?

No. But the role is changing.

I'm biased—I never saw myself writing code full-time 10 years into my career. I'm impatient and hate waiting for things to get done. If I'm honest, it's more worth my time to iterate on product than implementation details.

That said: balancing quality, modularity, performance, and extensibility are skills needed to iterate quickly. It's worth taking a day to generify an interface if you'll extend it later. This makes code:

- Easier to maintain
- Easier to document
- Easier for agents to extend
- While keeping review manageable for humans

The future isn't agents replacing engineers. It's engineers who leverage agents replacing those who don't.

### What actually helps agents succeed?

- Clear, easy-to-follow documentation
- Opinionated implementation patterns with examples
- Callable checks: formatters, linters, tests
- Local dev tools with dynamic port allocation
- Dependency injection everywhere
- Injectable, discoverable configuration

---

## Tips for Agent-Friendly Repos

### Dependency injection

Inject dependencies everywhere. It makes code:

- Testable (agents can verify their work)
- Documentable (clear interfaces)
- Extendable (agents understand how to add functionality)

### Runtime configuration

Make configuration injectable and discoverable. Agents can understand and modify behavior without hunting through hardcoded values.

### Local development tools

Use local tools when possible. Write tooling for:

- Dynamic port allocation
- Service discovery
- Environment setup

This helps agents spin up isolated environments per worktree without conflicts.

### Document the patterns

If agents can find your patterns documented, they'll follow them. If they can't, they'll invent their own—possibly inconsistent with your codebase.

Invest time in:

- `PATTERNS.md` with coding conventions
- `CONTRIBUTING.md` with workflow
- `CLAUDE.md` with quick reference

The compounding returns across agent sessions are worth it.

### Write testable code

Agents can run tests to verify their work. The more comprehensive your test suite, the more confidently agents can iterate.

- Unit tests for pure functions
- Integration tests for API endpoints
- Snapshot tests for UI components

### Keep modules focused

Small, focused modules are easier for agents to understand and modify. If a file is doing too much, agents will struggle to make targeted changes.

### Use consistent naming

Agents pattern-match on names. Consistent naming conventions get followed. Inconsistent naming gets ignored.

### Provide examples

When documenting patterns, include examples. Agents learn from examples faster than abstract descriptions.
