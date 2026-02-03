# Initialize a Repo

`wt init` bootstraps a repository for worktree management and Claude Code. It creates project documentation, slash commands, permissions, and issue tracking — everything an AI coding agent needs to be productive.

## Basic Setup

```bash
cd your-repo
wt init
```

This creates:

```
your-repo/
├── CLAUDE.md                 # Entry point — points Claude at docs/
├── wt.toml                   # Spawn configuration
├── docs/
│   ├── index.md              # Agent instructions
│   └── issue-tracking.md     # File-based issue tracking
├── issues/                   # Work items as markdown files
└── .claude/
    ├── settings.json         # Permissions
    └── commands/
        ├── check.md          # /check — run build/test/lint
        ├── draft.md          # /draft — push and create draft PR
        ├── review.md         # /review — review branch changes
        ├── issues.md         # /issues — manage work items
        └── spawn.md          # /spawn — orchestrate parallel workers
```

## The `--audit` Flag

Instead of writing `docs/index.md` by hand, let Claude explore your codebase and fill it in:

```bash
wt init --audit
```

The audit launches Claude to:
1. Explore the codebase (languages, frameworks, structure, build system)
2. Write project-specific content in `docs/index.md`
3. Update `CLAUDE.md` with relevant sections
4. Tailor `.claude/commands/` to your toolchain
5. Commit the result

This is the recommended workflow for existing projects.

## Updating Templates

When `wt` ships new templates, apply them without losing customizations:

```bash
wt init --force --backup --audit
```

This:
1. **Backs up** existing files to `.wt-backup/`
2. **Overwrites** with fresh templates
3. **Audits** — Claude merges your customizations into the new structure

## Flags

| Flag | Description |
|------|-------------|
| `--force` | Overwrite existing files |
| `--backup` | Save existing files to `.wt-backup/` before overwriting |
| `--audit` | Launch Claude to explore and populate docs |

## After Init

The generated docs are a starting point. Iterate on them:

```bash
claude
# "Read docs/index.md and improve it — we use pnpm not npm,
#  and tests need postgres via docker compose up -d"
```

Edit `docs/index.md` to capture knowledge that makes agents effective in your repo.

## Next Steps

- [Create your first worktree](../usage/worktrees.md)
- [Configure base branches](../usage/configuration.md)
