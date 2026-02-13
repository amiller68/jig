# Contributing

Guide for both human contributors and AI agents working on this project.

## For All Contributors

### Getting Started

<!-- How to set up the development environment. Examples:
1. Clone the repository
2. Install dependencies: `npm install` / `cargo build` / etc.
3. Run tests: `npm test` / `cargo test`
4. Start development server: `npm run dev`
-->

### Making Changes

1. Create a feature branch from `main`
2. Make your changes following the patterns in `docs/PATTERNS.md`
3. Run checks: <!-- your check command -->
4. Commit with a clear message describing the change
5. Open a pull request

### Commit Message Format

<!-- Your commit conventions. Examples:
- Conventional commits: `feat:`, `fix:`, `docs:`, `refactor:`
- Reference issues: `Fix login bug (#123)`
- Include scope: `feat(api): add user endpoint`
-->

## For AI Agents

### Context to Gather First

Before making changes, read:
- `CLAUDE.md` — Project overview and quick commands
- `docs/PATTERNS.md` — Coding conventions
- `docs/SUCCESS_CRITERIA.md` — CI checks that must pass
- Related code files to understand existing patterns

### Workflow

1. **Understand** — Read the task and relevant code
2. **Plan** — Break down into small steps
3. **Implement** — Follow existing patterns
4. **Verify** — Run tests and checks
5. **Commit** — Clear, atomic commits

### Constraints

<!-- What agents should NOT do. Examples:
- Don't modify CI/CD configuration without approval
- Don't add new dependencies without discussion
- Don't refactor unrelated code
- Don't skip tests or use --no-verify
-->

## Code Review

<!-- Your review process. Examples:
- All PRs require one approval
- CI must pass before merge
- Squash commits on merge
-->
