# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## v0.4.0 (2026-02-12)

### Bug Fixes

 - <csr-id-196774225c8eba52fdb9382f98418ecf82c48567/> prevent shell-setup from corrupting shell config files
   The previous byte-slicing approach in find_path_line_end() calculated
   offsets incorrectly because lines() strips newlines but the code assumed
   +1 byte per line. This could corrupt or truncate config files.

### New Features

<csr-id-0ab34082c061a8ffba63413c3a6b7e397d12de6f/>
<csr-id-5a59d80324580c092cdda14ce2e2faebf535b444/>

 - <csr-id-f05d75ea429a873ac6f749928f49cb9d850b22eb/> add shell-setup command and fix shell completions
   - Add `jig shell-setup` command to automatically configure shell integration
   - Detects user's shell from $SHELL
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
 - <csr-id-3a78670c102178f25db9dc4020b534370fc36f84/> add --audit flag to init command that launches Claude interactively
   Uses exec() on Unix to replace the current process with Claude Code,
   giving it full terminal control for interactive documentation audit.

### Commit Statistics

<csr-read-only-do-not-edit/>

 - 13 commits contributed to the release.
 - 5 commits were understood as [conventional](https://www.conventionalcommits.org).
 - 0 issues like '(#ID)' were seen in commit messages

### Commit Details

<csr-read-only-do-not-edit/>

<details><summary>view details</summary>

 * **Uncategorized**
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

<csr-unknown>
Rewrite shell completions with dynamic worktree completionUpdate docs/usage/shell-integration.md rewrite health check to validate repo setup and agent scaffoldingReplace terminal-detection-focused health check with structured validationof system deps (git, tmux, claude), repository config (jig.toml, basebranch, .worktrees), and agent scaffolding (CLAUDE.md, settings, skills).Remove unused jq/gh dependency checks and dead required field. Exitnon-zero when checks fail. add shell completions for bash, zsh, and fishShell completions are now emitted alongside the shell wrapper functionin jig shell-init. Completions cover all subcommands, aliases,per-command flags, nested config subcommands, and dynamic worktreename completion via command jig list.<csr-unknown/>

