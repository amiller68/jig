# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## v0.6.0 (2026-02-20)

### Chore

 - <csr-id-3b5c017b302164bf6c9837d968ab96a092908a4c/> bump version to 0.6.0
 - <csr-id-72ff9fcf89d38f5e74d6d06c128226d2f094feb1/> bump version to 0.5.0
 - <csr-id-d38e493e16a264b81885608389452aa889ddfc6b/> remove jig-tui crate and wt references
   - Remove jig-tui crate entirely (was just a stub)
   - Remove Tui command from CLI
   - Rename all wt references to jig throughout codebase
   - Remove outdated wiki docs and spawn guides
   - Remove deprecated .claude/commands (replaced by skills)
   - Update tests to use jig binary name and init claude arg
   - Remove wt.toml (replaced by jig.toml)

### New Features

 - <csr-id-a685a48ac6c1b1d693e440d4e565e0bbd3ea49c0/> add worktree.copy for gitignored files
   Adds `worktree.copy` config to copy gitignored files (like .env)
   to new worktrees:
   
   ```toml
   [worktree]
   copy = [".env", ".env.local"]
   ```
   
   Files are copied after worktree creation, before on_create hook runs.
 - <csr-id-823eeb1a83ac668fe54b7dbb28a0d062c4f91e9a/> add worktree config to jig.toml
   jig.toml now supports worktree configuration:
   - `worktree.base` — base branch for new worktrees (overrides global)
   - `worktree.on_create` — command to run after worktree creation
 - <csr-id-4c9f3184c27cab9ddfc835fdde711ba6af2539ca/> improve adapter architecture and audit templates
   Adapter improvements:
   - Add AgentType enum for compile-time safe matching
   - Rename template to PROJECT.md (agent-agnostic name)
   - Dynamic audit prompt uses adapter.project_file and adapter.skills_dir
   - Validate agent is installed before init (warns if not in PATH)
   - Fix settings.json schema URL
   
   Template improvements:
   - Fix settings.json to use correct schemastore.org URL
   - Add WebFetch, WebSearch, mcp__*, jig:* to default permissions
   - Update review skill to check jig-specific docs and skills
   - Update issues skill to reference issues/README.md
 - <csr-id-60460d876900a1fca4dda6e7763127965d7dcb50/> add agent-agnostic adapter architecture
   - Add adapter module with AgentAdapter struct for pluggable agent support
   - jig init now requires agent argument: `jig init claude`
   - jig.toml stores agent type in [agent] section
   - spawn command uses adapter to build agent-specific commands
   - Move settings.json to templates/adapters/claude-code/
   
   This architecture allows future support for other agents (cursor, etc.)
   by adding new adapter constants.
 - <csr-id-0ab34082c061a8ffba63413c3a6b7e397d12de6f/> rewrite health check to validate repo setup and agent scaffolding
   Replace terminal-detection-focused health check with structured validation
   of system deps (git, tmux, claude), repository config (jig.toml, base
   branch, .worktrees), and agent scaffolding (CLAUDE.md, settings, skills).
   Remove unused jq/gh dependency checks and dead required field. Exit
   non-zero when checks fail.

### Commit Statistics

<csr-read-only-do-not-edit/>

 - 18 commits contributed to the release over the course of 16 calendar days.
 - 8 commits were understood as [conventional](https://www.conventionalcommits.org).
 - 0 issues like '(#ID)' were seen in commit messages

### Commit Details

<csr-read-only-do-not-edit/>

<details><summary>view details</summary>

 * **Uncategorized**
    - Bump version to 0.6.0 ([`3b5c017`](https://github.com/amiller68/jig/commit/3b5c017b302164bf6c9837d968ab96a092908a4c))
    - Merge pull request #21 from amiller68/release-automation ([`f8e5fc4`](https://github.com/amiller68/jig/commit/f8e5fc42ca9c3b7127a0af47794019c6e5e49676))
    - Bump jig-core v0.5.0, jig-cli v0.5.0 ([`2f76138`](https://github.com/amiller68/jig/commit/2f761383f982d3bcf363ed78bf7b6e680471850d))
    - Bump version to 0.5.0 ([`72ff9fc`](https://github.com/amiller68/jig/commit/72ff9fcf89d38f5e74d6d06c128226d2f094feb1))
    - Merge pull request #19 from amiller68/upgrade-docs-scaffolding ([`fb95d76`](https://github.com/amiller68/jig/commit/fb95d763c98264dab6671384569cd854b5f1a0d0))
    - Add worktree.copy for gitignored files ([`a685a48`](https://github.com/amiller68/jig/commit/a685a48ac6c1b1d693e440d4e565e0bbd3ea49c0))
    - Add worktree config to jig.toml ([`823eeb1`](https://github.com/amiller68/jig/commit/823eeb1a83ac668fe54b7dbb28a0d062c4f91e9a))
    - Remove jig-tui crate and wt references ([`d38e493`](https://github.com/amiller68/jig/commit/d38e493e16a264b81885608389452aa889ddfc6b))
    - Improve adapter architecture and audit templates ([`4c9f318`](https://github.com/amiller68/jig/commit/4c9f3184c27cab9ddfc835fdde711ba6af2539ca))
    - Add agent-agnostic adapter architecture ([`60460d8`](https://github.com/amiller68/jig/commit/60460d876900a1fca4dda6e7763127965d7dcb50))
    - Merge pull request #11 from amiller68/release-automation ([`461c28b`](https://github.com/amiller68/jig/commit/461c28b127a61081442cec9b356efc6f4ea08792))
    - Bump jig-core v0.4.0, jig-cli v0.4.0 ([`1ae3f1c`](https://github.com/amiller68/jig/commit/1ae3f1ca0e27e1cc25c8b5029e77504cf673368d))
    - Merge pull request #7 from amiller68/chore/update-health-check ([`da9e49a`](https://github.com/amiller68/jig/commit/da9e49a8510f72366fee47b73f92de54e2e672b7))
    - Rewrite health check to validate repo setup and agent scaffolding ([`0ab3408`](https://github.com/amiller68/jig/commit/0ab34082c061a8ffba63413c3a6b7e397d12de6f))
    - Merge pull request #3 from amiller68/release-automation ([`0ff806e`](https://github.com/amiller68/jig/commit/0ff806e431b898f6cd115a68f84d932f33d86e64))
    - Adjusting changelogs prior to release of jig-core v0.4.0, jig-cli v0.4.0 ([`b6c1bf7`](https://github.com/amiller68/jig/commit/b6c1bf747dd39da6023524e43d096f21535db8a3))
    - Merge pull request #1 from amiller68/claude/rename-update-ci-setup-xuDYK ([`b5d7430`](https://github.com/amiller68/jig/commit/b5d7430b3a2ea515dbe677cb7f15056a798a325f))
    - Rename internal crates and state file to jig naming ([`5529bf8`](https://github.com/amiller68/jig/commit/5529bf802af7cc1f0c6d4c40849075f0248e8a09))
</details>

## v0.5.0 (2026-02-13)

<csr-id-72ff9fcf89d38f5e74d6d06c128226d2f094feb1/>
<csr-id-d38e493e16a264b81885608389452aa889ddfc6b/>

### Chore

 - <csr-id-72ff9fcf89d38f5e74d6d06c128226d2f094feb1/> bump version to 0.5.0
 - <csr-id-d38e493e16a264b81885608389452aa889ddfc6b/> remove jig-tui crate and wt references
   - Remove jig-tui crate entirely (was just a stub)
   - Remove Tui command from CLI
   - Rename all wt references to jig throughout codebase
   - Remove outdated wiki docs and spawn guides
   - Remove deprecated .claude/commands (replaced by skills)
   - Update tests to use jig binary name and init claude arg
   - Remove wt.toml (replaced by jig.toml)

### New Features

<csr-id-4c9f3184c27cab9ddfc835fdde711ba6af2539ca/>
<csr-id-60460d876900a1fca4dda6e7763127965d7dcb50/>
<csr-id-0ab34082c061a8ffba63413c3a6b7e397d12de6f/>

 - <csr-id-a685a48ac6c1b1d693e440d4e565e0bbd3ea49c0/> add worktree.copy for gitignored files
   Adds `worktree.copy` config to copy gitignored files (like .env)
   to new worktrees:
   
   ```toml
   [worktree]
   copy = [".env", ".env.local"]
   ```
   
   Files are copied after worktree creation, before on_create hook runs.
 - <csr-id-823eeb1a83ac668fe54b7dbb28a0d062c4f91e9a/> add worktree config to jig.toml
   jig.toml now supports worktree configuration:
   - `worktree.base` — base branch for new worktrees (overrides global)
- `worktree.on_create` — command to run after worktree creation
- Add AgentType enum for compile-time safe matching
- Rename template to PROJECT.md (agent-agnostic name)
- Dynamic audit prompt uses adapter.project_file and adapter.skills_dir
- Validate agent is installed before init (warns if not in PATH)
- Fix settings.json schema URL
- Fix settings.json to use correct schemastore.org URL
- Add WebFetch, WebSearch, mcp__*, jig:* to default permissions
- Update review skill to check jig-specific docs and skills
- Update issues skill to reference issues/README.md
- Add adapter module with AgentAdapter struct for pluggable agent support
- jig init now requires agent argument: `jig init claude`
- jig.toml stores agent type in [agent] section
- spawn command uses adapter to build agent-specific commands
- Move settings.json to templates/adapters/claude-code/

<csr-unknown>
 improve adapter architecture and audit templatesAdapter improvements:Template improvements: add agent-agnostic adapter architectureThis architecture allows future support for other agents (cursor, etc.)by adding new adapter constants. rewrite health check to validate repo setup and agent scaffoldingReplace terminal-detection-focused health check with structured validationof system deps (git, tmux, claude), repository config (jig.toml, basebranch, .worktrees), and agent scaffolding (CLAUDE.md, settings, skills).Remove unused jq/gh dependency checks and dead required field. Exitnon-zero when checks fail.<csr-unknown/>

## v0.4.0 (2026-02-12)

### New Features

 - <csr-id-0ab34082c061a8ffba63413c3a6b7e397d12de6f/> rewrite health check to validate repo setup and agent scaffolding
   Replace terminal-detection-focused health check with structured validation
   of system deps (git, tmux, claude), repository config (jig.toml, base
   branch, .worktrees), and agent scaffolding (CLAUDE.md, settings, skills).
   Remove unused jq/gh dependency checks and dead required field. Exit
   non-zero when checks fail.

