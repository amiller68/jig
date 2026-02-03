# Setting Up a Repo with `scribe init`

`scribe init` bootstraps a repository for use with `scribe` and Claude Code. It creates project documentation, skills, sensible permissions, and issue tracking — everything an AI coding agent needs to be productive from the start.

## New Repo

```bash
mkdir my-project && cd my-project
git init
scribe init
```

This creates:

```
my-project/
├── CLAUDE.md                 # Project guide — points Claude at docs/
├── scribe.toml               # scribe spawn configuration
├── docs/
│   ├── index.md              # Agent instructions (key files, conventions, how to build/test)
│   └── issue-tracking.md     # File-based issue tracking convention
├── issues/                   # Work items as markdown files
└── .claude/
    ├── settings.json         # Permissions (git, gh, file ops, etc.)
    └── skills/
        ├── check/            # /check — run build/test/lint
        ├── draft/            # /draft — push and create draft PR
        ├── issues/           # /issues — discover and manage work items
        ├── review/           # /review — review branch changes
        └── scribe/           # /scribe — orchestrate parallel workers
```

After init, edit `docs/index.md` with your project's specifics — key files, build commands, conventions. This is what every spawned worker reads first.

## Existing Repo

```bash
cd ~/projects/my-app
scribe init
```

If files already exist (`CLAUDE.md`, `docs/`, `.claude/`), init skips them. Use `--force` to overwrite:

```bash
scribe init --force
```

## Updating Templates with `--backup`

When `scribe` ships new templates, apply them without losing your customizations:

```bash
scribe init --force --backup
```

This:
1. **Backs up** your existing `CLAUDE.md`, `scribe.toml`, `docs/`, and `.claude/` to `.scribe-backup/`
2. **Overwrites** with fresh templates (`--force`)

After verifying the result, remove the backup:

```bash
rm -rf .scribe-backup/
```

## Iterating After Init

`scribe init` is a first pass. The generated docs and skills are a starting point — you should iterate on them to match how your team actually works.

The fastest way to iterate is to work with Claude directly:

```bash
claude
# "Read docs/index.md and improve it — we use pnpm, not npm, and our
#  tests require a running postgres container via docker compose up -d"
```

Or edit the files by hand. Either way, the goal is to capture the knowledge that makes agents effective in your repo.

### What to add to `docs/`

Beyond `index.md`, consider adding articles for things agents get wrong without guidance:

- **Code patterns** — error handling conventions, naming rules, how you structure modules, preferred libraries over alternatives
- **Development workflows** — how to set up a local environment, how to run specific subsystems, database migrations, seed data
- **PR success criteria** — what must pass before merge (CI checks, review requirements, changelog updates, version bumps)
- **Architecture decisions** — why the codebase is structured this way, what not to refactor, boundaries between subsystems

Reference these from `CLAUDE.md` so agents discover them:

```markdown
## Documentation

Project documentation lives in `docs/`:
- `docs/index.md` — Agent instructions
- `docs/patterns.md` — Code patterns and conventions
- `docs/workflows.md` — Development workflows
- `docs/pr-criteria.md` — PR success criteria
```

### What to tune in `.claude/skills/`

The default skills are generic. You might want to:

- Add project-specific steps to `/check` (e.g., `docker compose up -d` before tests)
- Add PR description templates to `/draft`
- Add project-specific review criteria to `/review` (e.g., "check that all API changes have OpenAPI spec updates")
- Create entirely new skills for your workflow (e.g., `/migrate`, `/deploy-staging`, `/release`)

## What Each File Does

### `CLAUDE.md`

The entry point Claude reads when starting a session. Points to `docs/` for detailed instructions. Add project-specific sections here (versioning rules, release process, etc.).

### `docs/index.md`

The core agent instructions document. Spawned workers read this to understand:
- What the project is and how it's structured
- How to build, test, and run it
- Code conventions to follow
- Common gotchas

### `.claude/settings.json`

Permissions for Claude Code. The defaults allow common safe operations (git, gh, file reads/writes, tmux) and deny dangerous ones (rm -rf, sudo, force push, reading secrets). Edit this to add project-specific tools.

### `.claude/skills/`

Skills available in Claude Code sessions via slash commands. The defaults are generic — tailor them to your project (e.g., replacing multi-language detection in `/check` with your actual `npm test` or `cargo test`).

### `scribe.toml`

Spawn configuration:

```toml
[spawn]
auto = true    # Always use auto mode (--auto flag)
```

### `docs/issue-tracking.md`

Convention for file-based issue tracking in `issues/`. If you use Linear, Jira, or GitHub Issues instead, you can ignore or remove this.

## Flags

| Flag | Description |
|------|-------------|
| `--force` | Overwrite existing files |
| `--backup` | Save existing files to `.scribe-backup/` before overwriting |

Flags combine: `scribe init --force --backup` is the recommended way to apply template updates.
