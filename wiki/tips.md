---
layout: page
title: Tips
nav_order: 7
---

# Tips for Agent-Friendly Repos

Patterns that help agents succeed.

## Dependency injection

Inject dependencies everywhere. It makes code:

- Testable (agents can verify their work)
- Documentable (clear interfaces)
- Extendable (agents understand how to add functionality)

## Runtime configuration

Make configuration injectable and discoverable. Agents can understand and modify behavior without hunting through hardcoded values.

## Local development tools

Use local tools when possible. Write tooling for:

- Dynamic port allocation
- Service discovery
- Environment setup

This helps agents spin up isolated environments per worktree without conflicts.

## Document the patterns

If agents can find your patterns documented, they'll follow them. If they can't, they'll invent their own—possibly inconsistent with your codebase.

Invest time in:

- `PATTERNS.md` with coding conventions
- `CONTRIBUTING.md` with workflow
- `CLAUDE.md` with quick reference

The compounding returns across agent sessions are worth it.

## Write testable code

Agents can run tests to verify their work. The more comprehensive your test suite, the more confidently agents can iterate.

- Unit tests for pure functions
- Integration tests for API endpoints
- Snapshot tests for UI components

## Keep modules focused

Small, focused modules are easier for agents to understand and modify. If a file is doing too much, agents will struggle to make targeted changes.

## Use consistent naming

Agents pattern-match on names. If your codebase uses consistent naming conventions, agents will follow them. If naming is inconsistent, agents will invent their own.

## Provide examples

When documenting patterns, include examples. Agents learn from examples faster than from abstract descriptions.
