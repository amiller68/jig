# Contributing

Guide for both human contributors and AI agents working on this project.

## For All Contributors

### Getting Started

1. Clone the repository
   ```bash
   git clone https://github.com/amiller68/jig.git
   cd jig
   ```

2. Build the project
   ```bash
   cargo build
   ```

3. Run tests
   ```bash
   cargo test
   ```

4. Run the CLI
   ```bash
   cargo run -- --help
   ```

### Making Changes

1. Create a feature branch from `main`
   ```bash
   git checkout -b feature/my-feature
   ```

2. Make your changes following the patterns in `docs/PATTERNS.md`

3. Run all checks
   ```bash
   cargo build && cargo test && cargo clippy && cargo fmt --check
   ```

4. Commit with a clear message describing the change

5. Open a pull request

### Commit Message Format

Use conventional commits:
- `feat:` — New feature
- `fix:` — Bug fix
- `docs:` — Documentation only
- `style:` — Formatting, no code change
- `refactor:` — Code restructuring
- `test:` — Adding or updating tests
- `chore:` — Maintenance tasks

Examples:
```
feat: add shell-setup command for automatic shell integration
fix: prevent color codes in stdout when using -o flag
docs: update PATTERNS.md with output conventions
```

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

- Don't modify CI/CD configuration without approval
- Don't add new dependencies without discussion
- Don't refactor unrelated code
- Don't skip tests or use --no-verify
- Always run `cargo fmt` before committing
- Keep stdout free of color codes (shell integration depends on this)

## Code Review

- All PRs require review before merge
- CI must pass (build, test, clippy, fmt)
- Squash commits on merge when appropriate
