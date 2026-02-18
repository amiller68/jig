# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## v0.6.0 (2026-02-18)

### Chore

 - <csr-id-bb4b4e85af9349e0a6e01ddcb21f54138da44797/> bump version to 0.6.0
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
 - <csr-id-8cce0fba090be552af7b0186f96ad03ffa8b5d81/> restructure issue tracking with categories and templates
   - Add directory-based issue organization (epics/, features/, bugs/, chores/)
   - Add issue templates (_templates/): standalone.md, epic-index.md, ticket.md
   - Create plan-and-execute epic for orchestration vision
   - Update issues/README.md with comprehensive documentation
   - Update /issues skill for new directory structure
   - Remove old flat issue files and _template.md
   - Add .backup/ to .gitignore
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
 - <csr-id-7bf25cd45434e6c0c9388ac70aadf0cc85cec04e/> improve backup, audit prompt, and review skill
   - Backup now copies files to .backup/ directory preserving path structure
   - Audit prompt is detailed and opinionated about what to fill in each doc
   - Review skill now checks for documentation and skills updates
 - <csr-id-badb4164208b05b288a36391ef046cb7b643ca3e/> upgrade jig init scaffolding to language-agnostic skeletons
   - Move issue-tracking.md to issues/README.md, fix "wt" → "jig"
   - Rename skills/jig → skills/spawn for consistency
   - Remove name: field from skill frontmatter
   - Add skeleton docs: PATTERNS.md, CONTRIBUTING.md, SUCCESS_CRITERIA.md, PROJECT_LAYOUT.md
   - Expand docs/index.md as documentation hub
   - Make CLAUDE.md template a skeleton with guidance comments
   - Upgrade settings.json: add $schema, ask tier for destructive ops, better secret patterns
   - Add issues/_template.md ticket template
 - <csr-id-80f3bccb70cdd146ab2eccbeec224a8104db8c61/> add Claude Code skills and simplify permissions
   - Add skills for check, draft, issues, review, and spawn commands
   - Simplify .claude/settings.json using wildcard permissions
   - Add jig.toml with spawn auto-configuration
   - Fix formatting in init.rs
 - <csr-id-4dd791fdfc3ce463b6642ae45d57062e10f9026b/> use actual templates for jig init instead of bare-bones placeholders
   - Embed templates from templates/ directory using include_str!
   - Add all 5 skills: check, draft, issues, review, spawn
   - Expand permissions to cover tools used by skills
   - Set spawn.auto = true by default
   - Use exec() on Unix for --audit flag (full terminal control)
   
   The init command now creates a complete scaffolding that matches
   the documentation, instead of empty placeholder comments.
 - <csr-id-3a78670c102178f25db9dc4020b534370fc36f84/> add --audit flag to init command that launches Claude interactively
   Uses exec() on Unix to replace the current process with Claude Code,
   giving it full terminal control for interactive documentation audit.
 - <csr-id-f05d75ea429a873ac6f749928f49cb9d850b22eb/> add shell-setup command and fix shell completions
   - Add `jig shell-setup` command to automatically configure shell integration
     - Detects user's shell from $SHELL
     - Finds appropriate config file (~/.bashrc, ~/.zshrc, ~/.config/fish/config.fish)
     - Adds eval line with markers for easy identification
     - Places integration after PATH setup when possible
     - Supports --dry-run flag to preview changes
   
   - Rewrite shell completions with dynamic worktree completion
     - `jig open/attach/review/merge/kill/status <TAB>` shows actual worktrees
     - Context-aware completions for all subcommands
     - Simplified zsh completion using _arguments -C
   
   - Update docs/usage/shell-integration.md
     - Add quick setup section for shell-setup command
     - Add troubleshooting section for common issues
     - Remove stale `sc` alias references (legacy from "scribe" name)
 - <csr-id-0ab34082c061a8ffba63413c3a6b7e397d12de6f/> rewrite health check to validate repo setup and agent scaffolding
   Replace terminal-detection-focused health check with structured validation
   of system deps (git, tmux, claude), repository config (jig.toml, base
   branch, .worktrees), and agent scaffolding (CLAUDE.md, settings, skills).
   Remove unused jq/gh dependency checks and dead required field. Exit
   non-zero when checks fail.
 - <csr-id-5a59d80324580c092cdda14ce2e2faebf535b444/> add shell completions for bash, zsh, and fish
   Shell completions are now emitted alongside the shell wrapper function
   in `jig shell-init`. Completions cover all subcommands, aliases,
   per-command flags, nested config subcommands, and dynamic worktree
   name completion via `command jig list`.

### Bug Fixes

 - <csr-id-bd9a6c99600670089a646b2e32cb6448d0b234bd/> make --audit print command instead of trying to launch claude
   Spawning claude programmatically was causing terminal issues and hangs.
   Now --audit just prints the command for the user to run manually.
 - <csr-id-196774225c8eba52fdb9382f98418ecf82c48567/> prevent shell-setup from corrupting shell config files
   The previous byte-slicing approach in find_path_line_end() calculated
   offsets incorrectly because lines() strips newlines but the code assumed
   +1 byte per line. This could corrupt or truncate config files.

### Commit Statistics

<csr-read-only-do-not-edit/>

 - 30 commits contributed to the release over the course of 15 calendar days.
 - 18 commits were understood as [conventional](https://www.conventionalcommits.org).
 - 0 issues like '(#ID)' were seen in commit messages

### Commit Details

<csr-read-only-do-not-edit/>

<details><summary>view details</summary>

 * **Uncategorized**
    - Bump version to 0.6.0 ([`bb4b4e8`](https://github.com/amiller68/jig/commit/bb4b4e85af9349e0a6e01ddcb21f54138da44797))
    - Merge pull request #21 from amiller68/release-automation ([`f8e5fc4`](https://github.com/amiller68/jig/commit/f8e5fc42ca9c3b7127a0af47794019c6e5e49676))
    - Bump jig-core v0.5.0, jig-cli v0.5.0 ([`2f76138`](https://github.com/amiller68/jig/commit/2f761383f982d3bcf363ed78bf7b6e680471850d))
    - Bump version to 0.5.0 ([`72ff9fc`](https://github.com/amiller68/jig/commit/72ff9fcf89d38f5e74d6d06c128226d2f094feb1))
    - Merge pull request #19 from amiller68/upgrade-docs-scaffolding ([`fb95d76`](https://github.com/amiller68/jig/commit/fb95d763c98264dab6671384569cd854b5f1a0d0))
    - Add worktree.copy for gitignored files ([`a685a48`](https://github.com/amiller68/jig/commit/a685a48ac6c1b1d693e440d4e565e0bbd3ea49c0))
    - Add worktree config to jig.toml ([`823eeb1`](https://github.com/amiller68/jig/commit/823eeb1a83ac668fe54b7dbb28a0d062c4f91e9a))
    - Remove jig-tui crate and wt references ([`d38e493`](https://github.com/amiller68/jig/commit/d38e493e16a264b81885608389452aa889ddfc6b))
    - Restructure issue tracking with categories and templates ([`8cce0fb`](https://github.com/amiller68/jig/commit/8cce0fba090be552af7b0186f96ad03ffa8b5d81))
    - Improve adapter architecture and audit templates ([`4c9f318`](https://github.com/amiller68/jig/commit/4c9f3184c27cab9ddfc835fdde711ba6af2539ca))
    - Add agent-agnostic adapter architecture ([`60460d8`](https://github.com/amiller68/jig/commit/60460d876900a1fca4dda6e7763127965d7dcb50))
    - Improve backup, audit prompt, and review skill ([`7bf25cd`](https://github.com/amiller68/jig/commit/7bf25cd45434e6c0c9388ac70aadf0cc85cec04e))
    - Upgrade jig init scaffolding to language-agnostic skeletons ([`badb416`](https://github.com/amiller68/jig/commit/badb4164208b05b288a36391ef046cb7b643ca3e))
    - Merge pull request #17 from amiller68/add-claude-skills ([`ebb5f28`](https://github.com/amiller68/jig/commit/ebb5f2875a8c91f01939076c9bdb4ff6ff17ccdf))
    - Add Claude Code skills and simplify permissions ([`80f3bcc`](https://github.com/amiller68/jig/commit/80f3bccb70cdd146ab2eccbeec224a8104db8c61))
    - Make --audit print command instead of trying to launch claude ([`bd9a6c9`](https://github.com/amiller68/jig/commit/bd9a6c99600670089a646b2e32cb6448d0b234bd))
    - Use actual templates for jig init instead of bare-bones placeholders ([`4dd791f`](https://github.com/amiller68/jig/commit/4dd791fdfc3ce463b6642ae45d57062e10f9026b))
    - Add --audit flag to init command that launches Claude interactively ([`3a78670`](https://github.com/amiller68/jig/commit/3a78670c102178f25db9dc4020b534370fc36f84))
    - Prevent shell-setup from corrupting shell config files ([`1967742`](https://github.com/amiller68/jig/commit/196774225c8eba52fdb9382f98418ecf82c48567))
    - Merge pull request #11 from amiller68/release-automation ([`461c28b`](https://github.com/amiller68/jig/commit/461c28b127a61081442cec9b356efc6f4ea08792))
    - Bump jig-core v0.4.0, jig-cli v0.4.0 ([`1ae3f1c`](https://github.com/amiller68/jig/commit/1ae3f1ca0e27e1cc25c8b5029e77504cf673368d))
    - Merge pull request #9 from amiller68/fix/shell-completion ([`a1ccdf1`](https://github.com/amiller68/jig/commit/a1ccdf17f789c2904c78bd4f7a0d621ce734d1d6))
    - Add shell-setup command and fix shell completions ([`f05d75e`](https://github.com/amiller68/jig/commit/f05d75ea429a873ac6f749928f49cb9d850b22eb))
    - Merge pull request #7 from amiller68/chore/update-health-check ([`da9e49a`](https://github.com/amiller68/jig/commit/da9e49a8510f72366fee47b73f92de54e2e672b7))
    - Rewrite health check to validate repo setup and agent scaffolding ([`0ab3408`](https://github.com/amiller68/jig/commit/0ab34082c061a8ffba63413c3a6b7e397d12de6f))
    - Add shell completions for bash, zsh, and fish ([`5a59d80`](https://github.com/amiller68/jig/commit/5a59d80324580c092cdda14ce2e2faebf535b444))
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

<csr-id-8cce0fba090be552af7b0186f96ad03ffa8b5d81/>
<csr-id-4c9f3184c27cab9ddfc835fdde711ba6af2539ca/>
<csr-id-60460d876900a1fca4dda6e7763127965d7dcb50/>
<csr-id-7bf25cd45434e6c0c9388ac70aadf0cc85cec04e/>
<csr-id-badb4164208b05b288a36391ef046cb7b643ca3e/>
<csr-id-80f3bccb70cdd146ab2eccbeec224a8104db8c61/>
<csr-id-4dd791fdfc3ce463b6642ae45d57062e10f9026b/>
<csr-id-3a78670c102178f25db9dc4020b534370fc36f84/>
<csr-id-f05d75ea429a873ac6f749928f49cb9d850b22eb/>
<csr-id-0ab34082c061a8ffba63413c3a6b7e397d12de6f/>
<csr-id-5a59d80324580c092cdda14ce2e2faebf535b444/>

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
- Add directory-based issue organization (epics/, features/, bugs/, chores/)
- Add issue templates (_templates/): standalone.md, epic-index.md, ticket.md
- Create plan-and-execute epic for orchestration vision
- Update issues/README.md with comprehensive documentation
- Update /issues skill for new directory structure
- Remove old flat issue files and _template.md
- Add .backup/ to .gitignore
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
- Backup now copies files to .backup/ directory preserving path structure
- Audit prompt is detailed and opinionated about what to fill in each doc
- Review skill now checks for documentation and skills updates
- Move issue-tracking.md to issues/README.md, fix "wt" → "jig"
- Rename skills/jig → skills/spawn for consistency
- Remove name: field from skill frontmatter
- Add skeleton docs: PATTERNS.md, CONTRIBUTING.md, SUCCESS_CRITERIA.md, PROJECT_LAYOUT.md
- Expand docs/index.md as documentation hub
- Make CLAUDE.md template a skeleton with guidance comments
- Upgrade settings.json: add $schema, ask tier for destructive ops, better secret patterns
- Add issues/_template.md ticket template
- Add skills for check, draft, issues, review, and spawn commands
- Simplify .claude/settings.json using wildcard permissions
- Add jig.toml with spawn auto-configuration
- Fix formatting in init.rs
- Embed templates from templates/ directory using include_str!
- Add all 5 skills: check, draft, issues, review, spawn
- Expand permissions to cover tools used by skills
- Set spawn.auto = true by default
- Use exec() on Unix for --audit flag (full terminal control)
- Add `jig shell-setup` command to automatically configure shell integration
     - Detects user's shell from $SHELL
     - Finds appropriate config file (~/.bashrc, ~/.zshrc, ~/.config/fish/config.fish)
     - Adds eval line with markers for easy identification
     - Places integration after PATH setup when possible
     - Supports --dry-run flag to preview changes
- Finds appropriate config file (~/.bashrc, ~/.zshrc, ~/.config/fish/config.fish)
- Adds eval line with markers for easy identification
- Places integration after PATH setup when possible
- Supports --dry-run flag to preview changes
- `jig open/attach/review/merge/kill/status <TAB>` shows actual worktrees
- Context-aware completions for all subcommands
- Simplified zsh completion using _arguments -C
- Add quick setup section for shell-setup command
- Add troubleshooting section for common issues
- Remove stale `sc` alias references (legacy from "scribe" name)

### Bug Fixes

 - <csr-id-bd9a6c99600670089a646b2e32cb6448d0b234bd/> make --audit print command instead of trying to launch claude
   Spawning claude programmatically was causing terminal issues and hangs.
   Now --audit just prints the command for the user to run manually.
 - <csr-id-196774225c8eba52fdb9382f98418ecf82c48567/> prevent shell-setup from corrupting shell config files
   The previous byte-slicing approach in find_path_line_end() calculated
   offsets incorrectly because lines() strips newlines but the code assumed
   +1 byte per line. This could corrupt or truncate config files.

<csr-unknown>
 restructure issue tracking with categories and templates improve adapter architecture and audit templatesAdapter improvements:Template improvements: add agent-agnostic adapter architectureThis architecture allows future support for other agents (cursor, etc.)by adding new adapter constants. improve backup, audit prompt, and review skill upgrade jig init scaffolding to language-agnostic skeletons add Claude Code skills and simplify permissions use actual templates for jig init instead of bare-bones placeholdersThe init command now creates a complete scaffolding that matchesthe documentation, instead of empty placeholder comments. add –audit flag to init command that launches Claude interactivelyUses exec() on Unix to replace the current process with Claude Code,giving it full terminal control for interactive documentation audit. add shell-setup command and fix shell completionsRewrite shell completions with dynamic worktree completionUpdate docs/usage/shell-integration.md rewrite health check to validate repo setup and agent scaffoldingReplace terminal-detection-focused health check with structured validationof system deps (git, tmux, claude), repository config (jig.toml, basebranch, .worktrees), and agent scaffolding (CLAUDE.md, settings, skills).Remove unused jq/gh dependency checks and dead required field. Exitnon-zero when checks fail. add shell completions for bash, zsh, and fishShell completions are now emitted alongside the shell wrapper functionin jig shell-init. Completions cover all subcommands, aliases,per-command flags, nested config subcommands, and dynamic worktreename completion via command jig list.<csr-unknown/>

## v0.4.0 (2026-02-12)

### New Features

<csr-id-0ab34082c061a8ffba63413c3a6b7e397d12de6f/>
<csr-id-5a59d80324580c092cdda14ce2e2faebf535b444/>

 - <csr-id-f05d75ea429a873ac6f749928f49cb9d850b22eb/> add shell-setup command and fix shell completions
   - Add `jig shell-setup` command to automatically configure shell integration
- Detects user's shell from $SHELL

<csr-unknown>
Finds appropriate config file (~/.bashrc, ~/.zshrc, ~/.config/fish/config.fish)Adds eval line with markers for easy identificationPlaces integration after PATH setup when possibleSupports –dry-run flag to preview changesjig open/attach/review/merge/kill/status <TAB> shows actual worktreesContext-aware completions for all subcommandsSimplified zsh completion using _arguments -CAdd quick setup section for shell-setup commandAdd troubleshooting section for common issuesRemove stale sc alias references (legacy from “scribe” name)<csr-unknown>
Rewrite shell completions with dynamic worktree completionUpdate docs/usage/shell-integration.md rewrite health check to validate repo setup and agent scaffoldingReplace terminal-detection-focused health check with structured validationof system deps (git, tmux, claude), repository config (jig.toml, basebranch, .worktrees), and agent scaffolding (CLAUDE.md, settings, skills).Remove unused jq/gh dependency checks and dead required field. Exitnon-zero when checks fail. add shell completions for bash, zsh, and fishShell completions are now emitted alongside the shell wrapper functionin jig shell-init. Completions cover all subcommands, aliases,per-command flags, nested config subcommands, and dynamic worktreename completion via command jig list.<csr-unknown/>
<csr-unknown/>

