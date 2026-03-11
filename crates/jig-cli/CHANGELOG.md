# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## v1.2.0 (2026-03-11)

### Chore

 - <csr-id-f4c14ae0b8d2a2a34e536071b55ec0f2b1fb1786/> bump version to 1.2.0
 - <csr-id-3d809c6a1b58f3d438c3d279592005947ad50438/> bump version to 1.1.1
 - <csr-id-0d3f00fefd29350c51e4671b9de14d230b809931/> bump version to 1.1.0
 - <csr-id-639e712803a8d13d5f8c84728d0410a17b47561e/> bump all outdated crates to latest major versions
   - thiserror 1 → 2 (no API changes needed)
   - colored 2 → 3 (MSRV bump only, dropped lazy_static)
   - dirs 5 → 6 (API compatible)
   - toml 0.8 → 1.0 (API compatible)
   - handlebars 5 → 6 (RenderError refactored, no impact on our usage)
   - which 6 → 8 (API compatible)
   - nix 0.28 → 0.31 (no breaking changes for process feature)
   - flume 0.11 → 0.12 (API compatible)
 - <csr-id-f39d6b5fb56180c8cc9f40adf812138f8824b64d/> bump version to 1.0.0
 - <csr-id-72ff9fcf89d38f5e74d6d06c128226d2f094feb1/> bump version to 0.5.0
 - <csr-id-d38e493e16a264b81885608389452aa889ddfc6b/> remove jig-tui crate and wt references
   - Remove jig-tui crate entirely (was just a stub)
   - Remove Tui command from CLI
   - Rename all wt references to jig throughout codebase
   - Remove outdated wiki docs and spawn guides
   - Remove deprecated .claude/commands (replaced by skills)
   - Update tests to use jig binary name and init claude arg
   - Remove wt.toml (replaced by jig.toml)

### Documentation

 - <csr-id-3520041197353776dd5999f805866d7c18da9298/> audit and update docs/wiki for release, remove PROJECT_LAYOUT.md and GRINDER-ANALYSIS.md
   Update daemon.md with actor architecture, tmux timeouts, and per-repo config.
   Add actor pattern to PATTERNS.md. Fix SUCCESS_CRITERIA.md pre-commit claim.
   Update wiki with correct commands, new worker states, and actor details.
   Remove PROJECT_LAYOUT.md (derivable from codebase) and GRINDER-ANALYSIS.md
   (historical, all functionality now integrated) from docs, templates, init,
   skills, and all references.
 - <csr-id-b0f93bcf7cc499835d82f0944a93ccb4a4d3e3b9/> document overlapping branch name behavior in run_global
   Add doc comment explaining that when multiple repos have a worktree with
   the same name, the first match (in repo discovery order) is used,
   consistent with other global commands.

### New Features

 - <csr-id-4be2cafcf2b1c7bcb9a42192a636aaf84d6fbcfc/> add per-repo nudge configuration in jig.toml
   Add [health] section to jig.toml supporting per-repo overrides of
   silence_threshold_seconds and max_nudges, plus per-nudge-type
   [health.nudge.<type>] sections with independent max and
   cooldown_seconds settings.
   
   Resolution order: jig.toml [health.nudge.<type>] > jig.toml [health]
   > global config > defaults. When cooldown_seconds is not set, falls
   back to silence_threshold_seconds.
   
   - Add RepoHealthConfig, NudgeTypeConfigs, NudgeTypeConfig structs
   - Add ResolvedNudgeConfig with resolver for per-type config
   - Thread resolved config through nudge classify, dispatch, and execute
   - Apply per-type cooldown to both idle/stalled nudges and PR nudges
   - Display effective nudge config in `jig config show`
   - Fixes PR nudge burst bug by enforcing per-type cooldowns
 - <csr-id-37a59d2d8c02dbc87bbff0fcf4f92aef768bd996/> add jig home command to print base repo root
   Adds `jig home` (alias `jig h`) that prints the base repository root
   path, enabling `cd $(jig home)` navigation from worktrees.
 - <csr-id-df0a3be811b27f8afce047bd088cad410d09e081/> communicate worker initialization state and on-create failures
   Add Initializing event type and worker status to make the worker
   lifecycle visible during setup. When the daemon auto-spawns a worker,
   it now registers the worker as Initializing before running the
   on-create hook, then transitions to Spawned on success or Failed on
   hook failure.
 - <csr-id-45fe8b1e0bf4d16c7d8fc267c150d8dfb506f914/> show auto-spawn config in `jig config` and add `jig issues --auto`
   Display auto-spawn settings (enabled, auto-start, max workers, poll
   interval, spawn labels) in `jig config show` with source attribution.
   Add `--auto` flag to `jig issues` to filter to only daemon-eligible
   auto-spawn candidates using the existing `list_spawnable` method.
 - <csr-id-52208bf4ef7efc35cac4726bd4fa73e2713b7bb5/> use checkmark instead of asterisk for AUTO column
   Change the AUTO column indicator in `jig issues` from `*` to `✓`
   for better readability.
 - <csr-id-bd1a1faeca5a7634224bef836154791819b4903b/> use checkmark instead of asterisk for AUTO column
   Change the AUTO column indicator in `jig issues` from `*` to `✓`
   for better readability.
 - <csr-id-462f05eaf29929899631125c733738cd8f93e558/> move auto-spawn to background thread to keep ps -w responsive
   The on-create hook (e.g. pnpm install) was running synchronously on
   the tick thread, freezing the ps --watch UI for the entire duration.
   Introduces a spawn_actor following the same pattern as prune_actor,
   issue_actor, etc. The tick now sends spawnable issues to the background
   thread and drains results on the next tick.
   
   Also adds:
   - Spawning worker names shown below the ps table during setup
   - WorkerStatus::Initializing variant for future use
   - spawn_labels config in jig.toml
   - Three new issues (config-show-auto-spawn, worker-initializing-state,
     auto-column-checkmark)
 - <csr-id-d5f79bd94e27cf82bc4e5b70f977eea258b62a92/> add labels field for issue tagging and filtering
   Add `labels: Vec<String>` to Issue and IssueFilter types. Linear
   provider now passes all label names through from GraphQL (auto field
   derivation unchanged). File provider parses `**Labels:**` comma-separated
   frontmatter. CLI gains `--label/-l` flag for filtering (all must match).
   Shell completions updated for bash, zsh, and fish.
 - <csr-id-2e4d781ae7aeda559884ea980cacdd5fae423d0c/> add shared UI module with consistent formatting and --plain flag
   Expand ui.rs into a centralized formatting module with:
   - Status symbol constants (✓, →, ✗, !)
   - Formatted output helpers (success, progress, failure, warning, detail, header)
   - Color helpers (highlight, bold, dim) that respect plain mode
   - Table builder helper (new_table) for consistent table creation
   - Global --plain flag for scriptable output (no colors, no decorations)
   - Error display with cause chain formatting
   
   Migrate all 20 command files from inline colored::Colorize calls to
   shared ui:: helpers. Add --plain support to list, repos, and issues
   commands with tab-separated output for piping.
 - <csr-id-1f4553fd7f0a7e21cfb5234e3800a8152f6dcca1/> add AUTO column to `jig issues` table output
   Show a green dot indicator for issues tagged for auto-spawn, making it
   visible at a glance whether file-provider Auto flag or Linear jig-auto
   label is set.
 - <csr-id-3ab73b1d1c7e20c25898ed021a50d7aebf2d0dd1/> support -g/--global flag for attaching from anywhere
   Add run_global implementation to the Attach command so users can attach
   to a worktree from outside the owning repo using `jig attach <name> -g`.
   Resolves the owning repo via GlobalCtx::repo_for_worktree.
 - <csr-id-5745c0d00da47a05a7a4b98d1bca6d9985afc25b/> jig init --audit launches agent in tmux to populate docs
   --audit now spawns the configured agent in a jig-init:<repo> tmux
   session with the audit prompt instead of just printing instructions.
   --backup enhances the prompt to reference .backup/ files. --audit
   accepts an optional string for extra instructions.
 - <csr-id-5217bfa9423f54a27b9e0badef98c4a72e2e273e/> block auto-spawn on unresolved dependencies
   Add `is_spawnable_with_deps()` to IssueProvider trait that checks all
   depends_on entries resolve to Complete before allowing spawn. Applied in
   both FileProvider and LinearProvider's list_spawnable(). Also adds
   --blocked/--unblocked flags to `jig issues` CLI for filtering.
 - <csr-id-057e8dc3675610e75e826910d051774f32f63cee/> group workers by repo in `jig ps -g` output
   Add `repo` field to `WorkerDisplayInfo` and render grouped tables with
   bold repo headers when running in global mode (`jig ps -g` / `jig ps -gw`).
   Local `jig ps` output is unchanged.
 - <csr-id-feb9d6068256ec7e2298a08e798a5913396a615d/> daemon periodically prunes stale worktrees
   Workers in terminal state (merged/archived/failed) with dead tmux
   sessions now get their git worktrees, event logs, and global state
   entries cleaned up automatically. Prune runs every 120s during watch
   mode. Pruned workers are reported in the tick status and log view.
   
   Also includes snake_case fixes for auto-spawn-filtering ticket.
 - <csr-id-23c5b4f9f732bb70616b95e95c5b1d7c946e43d1/> default table view for `jig ls` and pretty grouped `jig ls -g`
   - `jig ls` now shows a table with name, branch, and commits ahead
   - `jig ls -g` shows tables grouped by repo with bold headers
   - Add `--plain/-p` flag for bare name output (old behavior)
   - Shell completions use `--plain` and fall back to `-gp` outside a repo
   - Branch column only shown when it differs from worktree name
 - <csr-id-780632c2fff774e3f968ee8254f5b57a46abaa55/> show draft vs review state, document PR nudge behavior
   Workers with draft PRs now show "draft" (blue) in the STATE column
   instead of "review" (cyan). This makes it visually clear which workers
   will receive PR nudges (draft) vs which are in human review (non-draft).
   
   Add PR Nudges section to daemon docs explaining the draft/non-draft
   nudge policy and what each health check means.
 - <csr-id-61339c359884180d22d04a206be57d7b28d6fa9a/> unify daemon/ps tick loops and add log toggle to watch mode
   Extract run_with() callback API from daemon so ps --watch shares the
   same setup code path instead of duplicating Daemon/Notifier/TmuxClient
   construction. The callback controls inter-tick delay and can signal
   stop, which enables keypress handling during the sleep window.
   
   Add log view toggle to watch mode: press 'l' to see timestamped daemon
   activity (nudges fired, PR check results, errors), 't' to switch back
   to the table, 'q' to quit cleanly. Uses crossterm raw mode with 100ms
   poll intervals for responsive input.
   
   Also allows spawned workers to transition to stalled (previously
   Spawned status was excluded from silence detection).
 - <csr-id-c34254a3c119de72e0c472c5bf814059547fdbd6/> surface PR health in ps --watch display
   Add a HEALTH column to the watch table showing per-worker PR check
   results (ci, conflicts, reviews, commits) so problems are visible at a
   glance without needing RUST_LOG=debug. Upgrade silent debug-level PR
   errors to info-level logging.
 - <csr-id-8c92e5a1faa6992a14fb494640fb263d6cbc7049/> add --base flag to spawn and create for custom branch base
   Allow overriding the default base branch (from jig.toml) per-command
   with --base/-b. Includes shell completions for branch names across
   bash, zsh, and fish. Also fixes spawn status message to show the
   actual base branch used instead of the current branch.
 - <csr-id-e33ab3dfa06347d2aee13dc6d53d422cc462117c/> wire issues into spawn pipeline with --issue flag
   Add `jig spawn --issue <id>` to resolve file-based issues and use their
   body as Claude context. Thread issue_ref through the full pipeline:
   spawn CLI → register() → Spawn event → WorkerState reducer → daemon
   workers.json → ps watch table.
   
   Also adds:
   - `jig issues` CLI command with --ids flag for scripting
   - IssuesConfig in jig.toml for configurable issues directory
   - ISSUE column in ps --watch table (shortened last path segment)
   - Shell completions for --issue in bash, zsh, and fish
   - issue_ref tests in reducer and daemon roundtrip
 - <csr-id-d790a8101173e5797d7f331b56e0a0f5b06566a4/> add watch mode to ps command for live dashboard
   `jig ps --watch` clears and refreshes the worker table every 2s.
   Shows enriched state from event logs alongside tmux status:
   - TMUX column (●/○/✗) for session liveness
   - STATE column from event-derived WorkerStatus
   - NUDGES count and PR number from event log
   - Configurable interval: `jig ps -w 5` for 5s refresh
 - <csr-id-1a8faafa772e7c9014347f6802936d7d9a817bcb/> add daemon loop to orchestrate event-driven pipeline
   The missing conductor: `jig daemon` runs a periodic loop that:
   - Discovers workers by scanning event log directories
   - Replays events to derive current WorkerState per worker
   - Compares old vs new state to dispatch actions
   - Executes nudges via tmux and notifications via hooks
   - Persists state to workers.json between ticks
   
   Supports --once for single-pass mode and --interval for tuning.
 - <csr-id-73dc3fbbf0178af964a9f0481a5e85fc0e66cde1/> add git hook management (install, uninstall, handlers)
   Implements the git-hooks epic (tickets 0-4):
   - Hook wrapper templates that chain jig logic with user hooks
   - Registry tracking installed hooks at jig-hooks.json
   - Idempotent init with backup/restore of existing hooks
   - Post-commit/merge handlers that emit events to worker logs
   - Uninstall with rollback to original user hooks
 - <csr-id-13e44044ea08a91eb24e4b1b38c43c695a2fadc4/> expand WorkerStatus with event-driven states
   Add Idle, WaitingInput, Stalled variants. Make all variants unit types
   (remove associated data from WaitingReview/Failed). Add needs_attention(),
   is_active(), is_terminal(), from_legacy() methods. Snake_case serialization.
 - <csr-id-1bb57f9c0543cd7af986dd2303f34395980019f4/> add event log format and Claude Code hooks
   Implement event-system tickets 1 and 2:
   - Event schema with typed EventType enum and flat JSONL serialization
   - EventLog append-only reader/writer with per-worker JSONL files
   - Claude Code hook templates (PostToolUse, Notification, Stop)
   - `jig hooks install-claude` CLI command to install hooks to ~/.claude/hooks/
 - <csr-id-82c654ab1137ec963121638f6741617c59ee0c04/> add global state infrastructure for cross-repo aggregation
   Introduces ~/.config/jig/ directory structure with structured TOML config,
   aggregated JSON worker state, and event log directories for the event-driven
   pipeline. Ensures global dirs are created at CLI startup.
 - <csr-id-d878b9792a36f7c0d1157296401ca80af7f86f30/> introduce RepoContext and thread repo state through all operations
   Derive repo_root, worktrees_dir, git_common_dir, base_branch, and
   session_name once at startup via RepoContext::from_cwd(), eliminating
   redundant git subprocess calls (e.g. spawn called get_base_repo() 8x).
   OpContext now holds Option<RepoContext>, and all jig-core functions
   accept &RepoContext instead of re-deriving from cwd. Also adds repo
   registry for global mode auto-registration, removes dead spawn::kill(),
   and updates docs/patterns/issue status.
 - <csr-id-5b776f40ef697de1ecb06c16e97feb4102b23103/> implement smart jig update command
   Rewrite update command to:
   - Detect installation method (script, cargo, source, unknown)
   - Check latest version from GitHub releases API
   - Auto-update for script installations (~/.local/bin)
   - Prompt dev builds to install release binaries
   - Offer cleanup of old cargo bin after source build updates
   - Add --force flag to skip version check
 - <csr-id-357f9a6dfb6ab792078fc900f9b1bb956b3a4e4a/> prettify jig ps with Op pattern and comfy-table
   Introduce the Op trait to separate command logic from presentation.
   Rewrite `jig ps` as the first adopter: ops return typed data, Display
   impls own all formatting via comfy-table with terminal-width-aware
   column layout and color-coded status indicators.
   
   - Add Op trait in crates/jig-cli/src/op.rs
   - Rewrite ps command with PsOutput, PsError, and Op impl
   - Add comfy-table dependency for dynamic table rendering
   - Update main.rs dispatch to use Op::execute()
   - Add docs/ui/STDOUT-FORMATTING.md documenting the pattern
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

 - <csr-id-57e94d35e5e961a5fc68624b2646720f315327a2/> accept trailing args in git hook subcommands
   Git passes arguments to hooks (e.g. post-merge receives a squash flag
   "0" or "1"), which the hook wrapper forwards via "$@". The CLI
   subcommands rejected these unexpected args. Add trailing_var_arg to
   PostCommit, PostMerge, and PreCommit to accept and ignore them.
 - <csr-id-46949f98b3f25a53067d7845b8e85e299e7e1909/> output cd command from jig home instead of bare path
   Matches the pattern used by `jig open` and `jig exit` — outputs
   `cd '/path'` to stdout for shell eval, not just the path.
 - <csr-id-c970409ad61f8b48cbeb51dfe99371a225f9a4f7/> recover from stale git worktree registrations on spawn and prune
   When a worktree directory is removed but git still tracks the entry,
   `git worktree add` fails with "missing but already registered". Now
   create_worktree runs `git worktree prune` first, and prune_actor
   handles the missing-directory case instead of skipping cleanup.
   
   Also extracts prune_actor into its own module and adds urgent issue
   to replace git CLI shelling with git2.
 - <csr-id-1a36eb384a4ca2b5aab12a518e98daa472022859/> add issues command to shell completions
   The `issues` command was missing from all three shells' command lists
   and had no flag/argument completions. Adds command entry, issue ID
   positional completions, and flag completions (status, priority,
   category, interactive, ids) for bash, zsh, and fish.
 - <csr-id-d720fcaa0d1f1e0a327ae5d3c90dfe49323b198a/> use if-let instead of unwrap to satisfy clippy
 - <csr-id-52c77af3da99153a3ff98e580f419a70f8500d93/> daemon PR discovery, tmux targeting, and nudge delivery
   - Add proactive PR discovery: daemon queries GitHub for open PRs on
     worker branches when pr_url is unknown, emits PrOpened events to
     make state durable across restarts
   - Create per-repo GitHub clients via registry path lookup instead of
     ambient remote detection (fixes multi-repo daemon)
   - Extract real branch name from spawn events for tmux window lookup
     (spawn creates windows with slashes, e.g. feature/foo, not dashes)
   - Run all four PR checks (CI, conflicts, reviews, commits) on open PRs
   - Nudge on every tick, not just state transitions, so polling daemon
     retries delivery until max_nudges
   - Collapse multiline nudge templates to single line before tmux send
     to prevent premature submission in TUIs
   - Fix tracing init: RUST_LOG now properly overrides default warn level
   - Add stderr tick summary in continuous daemon mode for visibility
     without RUST_LOG
   - Add debug logging for tmux window misses and notification pipeline
 - <csr-id-378031a0afe019f57edc9bae469bf8168e05de29/> register Claude hooks in settings.json, add kill --all and nuke
   Claude Code hooks were installed as scripts but never registered in
   ~/.claude/settings.json, so they never fired. Now jig init registers
   them properly. Also fixes: hook templates read JSON from stdin (not
   env vars), spawned workers no longer nudged as stalled, event logs
   reset on respawn, row ordering stabilized in ps --watch, kill/unregister
   cleans up event logs, and nuke command added for full repo cleanup.
 - <csr-id-61dd7ff112e0cb63885649b399e764578f99e4b2/> address review findings and wire up event pipeline end-to-end
   Fix 6 issues from code review: UTF-8 safe truncate, stable status
   serialization via as_str/from_legacy, stuck nudge sends message after
   auto-approve, notification errors logged, branch names URL-encoded,
   tmux commands check exit status.
   
   Wire up missing pipeline links: jig spawn emits Spawn event, jig init
   auto-installs git+Claude hooks (idempotent on re-run), ps --watch runs
   daemon tick on each refresh for integrated orchestration.
   
   Add docs/daemon.md with background service setup for launchd, systemd,
   OpenRC, and generic nohup.
 - <csr-id-a41b92cb77141469539658c133da79f79f714452/> remove unnecessary return statement
 - <csr-id-bd9a6c99600670089a646b2e32cb6448d0b234bd/> make --audit print command instead of trying to launch claude
   Spawning claude programmatically was causing terminal issues and hangs.
   Now --audit just prints the command for the user to run manually.
 - <csr-id-196774225c8eba52fdb9382f98418ecf82c48567/> prevent shell-setup from corrupting shell config files
   The previous byte-slicing approach in find_path_line_end() calculated
   offsets incorrectly because lines() strips newlines but the code assumed
   +1 byte per line. This could corrupt or truncate config files.

### Other

 - <csr-id-8abff4b7ca2031d3232127b93febb92eb07cd9c5/> fmt
 - <csr-id-f7c5d5451126c55a29a5742b0ac55e5d2357dc36/> fmt

### Refactor

 - <csr-id-e33f0420bcba0c6abd5758cfafd756fff91515ad/> remove auto field, normalize on label-based spawn filtering
   Remove the `auto: bool` field from the `Issue` struct and the `**Auto:**`
   frontmatter / `jig-auto` label special-casing. Auto-spawn eligibility is
   now determined purely by `spawn_labels` in `[issues]` config in jig.toml.
   
   - Remove `auto` field from Issue struct
   - Remove `**Auto:** true` parsing from file provider
   - Remove `jig-auto` label detection from Linear client
   - Remove `i.auto` filter from list_spawnable in both providers
   - Remove `**Auto:**` from all existing issue files
   - Add `**Labels:**` field to issue templates
   - Update issues README with Labels field in format example
   - Update wiki skill-examples to reference spawn_labels config
   - Update auto-spawn-filtering issue to reflect new state
 - <csr-id-7bdb392c7ffa5727e951f18377022e7d596c4151/> consolidate worktree management into Worktree struct
   Make Worktree the single abstraction for a worker's physical state —
   repo, branch, path, tmux session, spawn context, and lifecycle.
   
   - Expand Worktree struct with repo_root, session_name, auto_spawned fields
   - Add lifecycle methods: launch(), resume(), register(), unregister()
   - Add tmux methods: has_tmux_window(), is_agent_running()
   - Add orphan detection: is_orphaned()
   - Add Resume event type to EventType enum and handle in reducer/derive
   - Fix derive_worker_name() to preserve category prefixes (features/foo)
   - Fix Repo::remove_worktree() to accept optional repo_root, avoiding
     Repo::discover() in daemon paths
   - Update daemon auto_spawn_worker to use Worktree::create + wt.register
     + wt.launch
   - Update CLI create/remove/spawn commands to use Worktree methods
   - Remove all spawn::register/spawn::launch_tmux_window calls outside
     Worktree
   - Eliminate Repo::discover() from all daemon code paths
 - <csr-id-1345768accb7711e0a333c0c7a3da55dfb3afd1d/> wrap git2 in Repo struct, remove Git(String) error variant
   Address PR feedback:
   - Wrap git2::Repository in a `Repo` struct with domain methods instead
     of free functions passing repo handles around
   - Remove Error::Git(String) variant — use Error::Git2(#[from] git2::Error)
     directly instead of mapping git2 errors to strings
   - Add Error::MergeConflict for merge-specific errors
   - DRY up duplicated prune_stale_worktrees and find_worktree_by_path
     code — prune_actor.rs now calls Repo methods from git.rs
   - Update all call sites across CLI commands, daemon, worktree, spawn,
     and context modules
   - Update PATTERNS.md and PROJECT_LAYOUT.md to reflect git2 usage
 - <csr-id-85d9e1de3500d926401b726017ee07199e5ff863/> move spawn daemon settings to global config with per-repo overrides
   Per-developer settings (auto_spawn, max_concurrent_workers,
   auto_spawn_interval) now live in ~/.config/jig/config.toml instead of
   jig.toml, since they shouldn't be committed to the repo. Per-repo
   jig.toml can still override via optional fields.
 - <csr-id-12f9c10b9f61aa2054a2d5c2d559553d3af50069/> remove `jig status` command (redundant with `jig ps`)
 - <csr-id-f694a0ce3f1a96ad9fc8b38d1c947924e6acaeaf/> drop -g support from attach/merge/review, deduplicate ps
   attach, merge, and review don't make sense in global mode — worktree
   names can conflict across repos. Extract shared ps logic into
   execute_ps() helper to eliminate duplication between run/run_global.
 - <csr-id-a0c69ed63f57649a00d0484505bafc9c644ca7e9/> split Op trait into run/run_global for -g flag dispatch
   Replace OpContext (single struct with global bool + repos vec) with two
   distinct context types: RepoCtx for single-repo operations and GlobalCtx
   for cross-repo -g mode. The Op trait now has run() and run_global()
   methods, with the default run_global() rejecting unsupported commands.
   
   11 global commands (list, ps, kill, remove, review, merge, attach,
   status, nuke, issues, open) implement both methods. 14 non-global
   commands only implement run(). The command_enum! macro dispatches both,
   and main.rs branches on cli.global to build the right context.
 - <csr-id-78cff84a46db59e266f2fa4affdaafb3c5857708/> unify CLI rendering with shared ui module and daemon-backed ps
   Extract duplicated table rendering, color mappings, and truncation into
   a shared crates/jig-cli/src/ui.rs module. Non-watch `jig ps` now uses a
   single daemon tick (once:true) to get the same rich WorkerDisplayInfo as
   watch mode — same columns (WORKER/STATE/COMMITS/PR/HEALTH/ISSUE) for
   both paths. Merge tmux status indicator into the WORKER name cell
   (colored dot prefix) instead of a separate cryptic column.
   
   Also includes: actor-based daemon runtime, issue/github/sync actors,
   Linear integration, session management, and various daemon improvements
   that were pending on this branch.
 - <csr-id-80401de003d427eeb057c8f64805b91060278fe5/> extract daemon.rs into struct-based daemon/ submodule
   Split the 675-line daemon.rs into a daemon/ directory with three files:
   - mod.rs: Daemon struct with tick/process_worker/sync_repos methods
   - discovery.rs: worker discovery and directory name splitting
   - pr.rs: PrMonitor struct for PR lifecycle checks
   
   This eliminates #[allow(clippy::too_many_arguments)] by moving shared
   state into the Daemon struct. All 7 tests preserved, public API updated
   from daemon::tick() to Daemon::new().tick().
 - <csr-id-225e9a6d7b8837652cae0da672f7b4b6a0cd069b/> implement Op trait and command_enum! macro for CLI
   Introduce a trait-based pattern for CLI commands that provides:
   - Typed errors per command (vs anyhow::Result everywhere)
   - Typed output per command (Display impl for stdout)
   - Unified execution via command_enum! macro
   - Infallible commands use std::convert::Infallible
   
   The macro generates Command enum, OpOutput, OpError, and Op impl,
   reducing boilerplate in main.rs dispatch. Doc comments on Args structs
   are picked up by clap (no duplication needed in cli.rs).
   
   Adds thiserror dependency to jig-cli for per-command error enums.
   Updates docs/PATTERNS.md to document the new pattern.

### Style

 - <csr-id-f7e016757eb9b899cd43b37b42b01164c8bd0fc7/> fix rustfmt formatting in init command

### New Features (BREAKING)

 - <csr-id-0f3fd3073b7b06f30e4cb6c0ebe1320433a68dff/> restructure jig state directory from .worktrees/ to .jig/
   Move all jig-managed worktrees from <repo>/.worktrees/ to <repo>/.jig/
   and state files to <repo>/.jig/.state/state.json. This provides a
   cleaner directory layout with state files separated from worktrees.
   
   Key changes:
   - Worktrees now live under .jig/ instead of .worktrees/
   - State file moved to .jig/.state/state.json
   - Auto-migration from .worktrees/ layout on first load
   - jig kill/unregister now removes workers from state entirely
     (instead of archiving them)
   - jig ps auto-cleans stale workers whose tmux windows are gone
   - Hidden directories (.state) are skipped when listing worktrees
   - .jig/.state/ added to .gitignore, .jig/ added to git exclude

### Commit Statistics

<csr-read-only-do-not-edit/>

 - 112 commits contributed to the release over the course of 36 calendar days.
 - 77 commits were understood as [conventional](https://www.conventionalcommits.org).
 - 0 issues like '(#ID)' were seen in commit messages

### Commit Details

<csr-read-only-do-not-edit/>

<details><summary>view details</summary>

 * **Uncategorized**
    - Bump version to 1.2.0 ([`f4c14ae`](https://github.com/amiller68/jig/commit/f4c14ae0b8d2a2a34e536071b55ec0f2b1fb1786))
    - Audit and update docs/wiki for release, remove PROJECT_LAYOUT.md and GRINDER-ANALYSIS.md ([`3520041`](https://github.com/amiller68/jig/commit/3520041197353776dd5999f805866d7c18da9298))
    - Accept trailing args in git hook subcommands ([`57e94d3`](https://github.com/amiller68/jig/commit/57e94d35e5e961a5fc68624b2646720f315327a2))
    - Merge pull request #136 from amiller68/features/home-command ([`94f7af5`](https://github.com/amiller68/jig/commit/94f7af51be421c9f4d12dfe5284214c33be15ba8))
    - Merge pull request #137 from amiller68/features/per-repo-nudge-config ([`f3e03d3`](https://github.com/amiller68/jig/commit/f3e03d38f65061576462cff1f03140508f6ccddc))
    - Output cd command from jig home instead of bare path ([`46949f9`](https://github.com/amiller68/jig/commit/46949f98b3f25a53067d7845b8e85e299e7e1909))
    - Add per-repo nudge configuration in jig.toml ([`4be2caf`](https://github.com/amiller68/jig/commit/4be2cafcf2b1c7bcb9a42192a636aaf84d6fbcfc))
    - Add jig home command to print base repo root ([`37a59d2`](https://github.com/amiller68/jig/commit/37a59d2d8c02dbc87bbff0fcf4f92aef768bd996))
    - Merge pull request #126 from amiller68/features/worker-initializing-state ([`e474c7c`](https://github.com/amiller68/jig/commit/e474c7c25cfe1cf7a5166a87cee47879053d6ee3))
    - Merge pull request #125 from amiller68/features/config-show-auto-spawn ([`a31afe6`](https://github.com/amiller68/jig/commit/a31afe65d93edd8b0640af6b0bd0abc926e1bc48))
    - Communicate worker initialization state and on-create failures ([`df0a3be`](https://github.com/amiller68/jig/commit/df0a3be811b27f8afce047bd088cad410d09e081))
    - Show auto-spawn config in `jig config` and add `jig issues --auto` ([`45fe8b1`](https://github.com/amiller68/jig/commit/45fe8b1e0bf4d16c7d8fc267c150d8dfb506f914))
    - Use checkmark instead of asterisk for AUTO column ([`52208bf`](https://github.com/amiller68/jig/commit/52208bf4ef7efc35cac4726bd4fa73e2713b7bb5))
    - Use checkmark instead of asterisk for AUTO column ([`bd1a1fa`](https://github.com/amiller68/jig/commit/bd1a1faeca5a7634224bef836154791819b4903b))
    - Move auto-spawn to background thread to keep ps -w responsive ([`462f05e`](https://github.com/amiller68/jig/commit/462f05eaf29929899631125c733738cd8f93e558))
    - Merge pull request #115 from amiller68/worktree-consolidation ([`31aa3d0`](https://github.com/amiller68/jig/commit/31aa3d0d1d2d5fbd55bcc9e2457c345e09391e23))
    - Merge pull request #99 from amiller68/labels-and-tags ([`ac1de1d`](https://github.com/amiller68/jig/commit/ac1de1db3b6eaf04b204fc0fd7a975490c807356))
    - Remove auto field, normalize on label-based spawn filtering ([`e33f042`](https://github.com/amiller68/jig/commit/e33f0420bcba0c6abd5758cfafd756fff91515ad))
    - Add labels field for issue tagging and filtering ([`d5f79bd`](https://github.com/amiller68/jig/commit/d5f79bd94e27cf82bc4e5b70f977eea258b62a92))
    - Consolidate worktree management into Worktree struct ([`7bdb392`](https://github.com/amiller68/jig/commit/7bdb392c7ffa5727e951f18377022e7d596c4151))
    - Merge pull request #112 from amiller68/cli-ui-improvements ([`f247223`](https://github.com/amiller68/jig/commit/f247223bdd6e8ec71f6713622b2b9b826cec1a98))
    - Add shared UI module with consistent formatting and --plain flag ([`2e4d781`](https://github.com/amiller68/jig/commit/2e4d781ae7aeda559884ea980cacdd5fae423d0c))
    - Merge pull request #85 from amiller68/replace-git-cli-with-git2 ([`9767cd4`](https://github.com/amiller68/jig/commit/9767cd41b20a659f568da70a7636203b14ce30a5))
    - Wrap git2 in Repo struct, remove Git(String) error variant ([`1345768`](https://github.com/amiller68/jig/commit/1345768accb7711e0a333c0c7a3da55dfb3afd1d))
    - Merge branch 'main' into global-attach ([`9e70fcd`](https://github.com/amiller68/jig/commit/9e70fcde225869d5684b53bed3b5da388cf1847c))
    - Document overlapping branch name behavior in run_global ([`b0f93bc`](https://github.com/amiller68/jig/commit/b0f93bcf7cc499835d82f0944a93ccb4a4d3e3b9))
    - Add AUTO column to `jig issues` table output ([`1f4553f`](https://github.com/amiller68/jig/commit/1f4553fd7f0a7e21cfb5234e3800a8152f6dcca1))
    - Support -g/--global flag for attaching from anywhere ([`3ab73b1`](https://github.com/amiller68/jig/commit/3ab73b1d1c7e20c25898ed021a50d7aebf2d0dd1))
    - Fix rustfmt formatting in init command ([`f7e0167`](https://github.com/amiller68/jig/commit/f7e016757eb9b899cd43b37b42b01164c8bd0fc7))
    - Jig init --audit launches agent in tmux to populate docs ([`5745c0d`](https://github.com/amiller68/jig/commit/5745c0d00da47a05a7a4b98d1bca6d9985afc25b))
    - Move spawn daemon settings to global config with per-repo overrides ([`85d9e1d`](https://github.com/amiller68/jig/commit/85d9e1de3500d926401b726017ee07199e5ff863))
    - Block auto-spawn on unresolved dependencies ([`5217bfa`](https://github.com/amiller68/jig/commit/5217bfa9423f54a27b9e0badef98c4a72e2e273e))
    - Merge pull request #75 from amiller68/ps-global-repo-grouping ([`4d97f18`](https://github.com/amiller68/jig/commit/4d97f1896e88d2fd195f7a50a819914e3c331305))
    - Group workers by repo in `jig ps -g` output ([`057e8dc`](https://github.com/amiller68/jig/commit/057e8dc3675610e75e826910d051774f32f63cee))
    - Recover from stale git worktree registrations on spawn and prune ([`c970409`](https://github.com/amiller68/jig/commit/c970409ad61f8b48cbeb51dfe99371a225f9a4f7))
    - Daemon periodically prunes stale worktrees ([`feb9d60`](https://github.com/amiller68/jig/commit/feb9d6068256ec7e2298a08e798a5913396a615d))
    - Add issues command to shell completions ([`1a36eb3`](https://github.com/amiller68/jig/commit/1a36eb384a4ca2b5aab12a518e98daa472022859))
    - Merge pull request #64 from amiller68/alex/pretty-ls-table ([`f26d885`](https://github.com/amiller68/jig/commit/f26d8852f309c8bc947a9143e452b30ee70a0a06))
    - Default table view for `jig ls` and pretty grouped `jig ls -g` ([`23c5b4f`](https://github.com/amiller68/jig/commit/23c5b4f9f732bb70616b95e95c5b1d7c946e43d1))
    - Merge pull request #63 from amiller68/release-automation ([`864df49`](https://github.com/amiller68/jig/commit/864df495c9557f10f7eaf387040c9ef30aefa9a3))
    - Bump jig-cli v1.1.1 ([`9ae55c6`](https://github.com/amiller68/jig/commit/9ae55c6879031eeda0289b95bc0c9bc13ce6572b))
    - Bump version to 1.1.1 ([`3d809c6`](https://github.com/amiller68/jig/commit/3d809c6a1b58f3d438c3d279592005947ad50438))
    - Merge pull request #62 from amiller68/alex/fucking-around ([`a04f6d9`](https://github.com/amiller68/jig/commit/a04f6d9d07092e99d5b873a682cb0177351b393e))
    - Use if-let instead of unwrap to satisfy clippy ([`d720fca`](https://github.com/amiller68/jig/commit/d720fcaa0d1f1e0a327ae5d3c90dfe49323b198a))
    - Remove `jig status` command (redundant with `jig ps`) ([`12f9c10`](https://github.com/amiller68/jig/commit/12f9c10b9f61aa2054a2d5c2d559553d3af50069))
    - Drop -g support from attach/merge/review, deduplicate ps ([`f694a0c`](https://github.com/amiller68/jig/commit/f694a0ce3f1a96ad9fc8b38d1c947924e6acaeaf))
    - Fmt ([`8abff4b`](https://github.com/amiller68/jig/commit/8abff4b7ca2031d3232127b93febb92eb07cd9c5))
    - Split Op trait into run/run_global for -g flag dispatch ([`a0c69ed`](https://github.com/amiller68/jig/commit/a0c69ed63f57649a00d0484505bafc9c644ca7e9))
    - Merge pull request #61 from amiller68/release-automation ([`0465ba2`](https://github.com/amiller68/jig/commit/0465ba24c6f86f7a963ab0e0c5a9052c5deeaee6))
    - Bump jig-cli v1.1.0 ([`d8eb08e`](https://github.com/amiller68/jig/commit/d8eb08ee783a98170c09d7f251abf619a172bcf2))
    - Bump version to 1.1.0 ([`0d3f00f`](https://github.com/amiller68/jig/commit/0d3f00fefd29350c51e4671b9de14d230b809931))
    - Merge pull request #60 from amiller68/alex/fucking-around ([`46fd1ad`](https://github.com/amiller68/jig/commit/46fd1ad6955edae5b27ea01f2a2a1aa0649594d9))
    - Show draft vs review state, document PR nudge behavior ([`780632c`](https://github.com/amiller68/jig/commit/780632c2fff774e3f968ee8254f5b57a46abaa55))
    - Bump all outdated crates to latest major versions ([`639e712`](https://github.com/amiller68/jig/commit/639e712803a8d13d5f8c84728d0410a17b47561e))
    - Unify CLI rendering with shared ui module and daemon-backed ps ([`78cff84`](https://github.com/amiller68/jig/commit/78cff84a46db59e266f2fa4affdaafb3c5857708))
    - Fmt ([`f7c5d54`](https://github.com/amiller68/jig/commit/f7c5d5451126c55a29a5742b0ac55e5d2357dc36))
    - Unify daemon/ps tick loops and add log toggle to watch mode ([`61339c3`](https://github.com/amiller68/jig/commit/61339c359884180d22d04a206be57d7b28d6fa9a))
    - Surface PR health in ps --watch display ([`c34254a`](https://github.com/amiller68/jig/commit/c34254a3c119de72e0c472c5bf814059547fdbd6))
    - Extract daemon.rs into struct-based daemon/ submodule ([`80401de`](https://github.com/amiller68/jig/commit/80401de003d427eeb057c8f64805b91060278fe5))
    - Daemon PR discovery, tmux targeting, and nudge delivery ([`52c77af`](https://github.com/amiller68/jig/commit/52c77af3da99153a3ff98e580f419a70f8500d93))
    - Add --base flag to spawn and create for custom branch base ([`8c92e5a`](https://github.com/amiller68/jig/commit/8c92e5a1faa6992a14fb494640fb263d6cbc7049))
    - Wire issues into spawn pipeline with --issue flag ([`e33ab3d`](https://github.com/amiller68/jig/commit/e33ab3dfa06347d2aee13dc6d53d422cc462117c))
    - Register Claude hooks in settings.json, add kill --all and nuke ([`378031a`](https://github.com/amiller68/jig/commit/378031a0afe019f57edc9bae469bf8168e05de29))
    - Address review findings and wire up event pipeline end-to-end ([`61dd7ff`](https://github.com/amiller68/jig/commit/61dd7ff112e0cb63885649b399e764578f99e4b2))
    - Add watch mode to ps command for live dashboard ([`d790a81`](https://github.com/amiller68/jig/commit/d790a8101173e5797d7f331b56e0a0f5b06566a4))
    - Add daemon loop to orchestrate event-driven pipeline ([`1a8faaf`](https://github.com/amiller68/jig/commit/1a8faafa772e7c9014347f6802936d7d9a817bcb))
    - Add git hook management (install, uninstall, handlers) ([`73dc3fb`](https://github.com/amiller68/jig/commit/73dc3fbbf0178af964a9f0481a5e85fc0e66cde1))
    - Expand WorkerStatus with event-driven states ([`13e4404`](https://github.com/amiller68/jig/commit/13e44044ea08a91eb24e4b1b38c43c695a2fadc4))
    - Add event log format and Claude Code hooks ([`1bb57f9`](https://github.com/amiller68/jig/commit/1bb57f9c0543cd7af986dd2303f34395980019f4))
    - Add global state infrastructure for cross-repo aggregation ([`82c654a`](https://github.com/amiller68/jig/commit/82c654ab1137ec963121638f6741617c59ee0c04))
    - Introduce RepoContext and thread repo state through all operations ([`d878b97`](https://github.com/amiller68/jig/commit/d878b9792a36f7c0d1157296401ca80af7f86f30))
    - Merge pull request #54 from amiller68/release-automation ([`d74a34e`](https://github.com/amiller68/jig/commit/d74a34e684725d0bc4aeac357e04b9a2bbb44630))
    - Bump jig-cli v1.0.0 ([`0a925f6`](https://github.com/amiller68/jig/commit/0a925f66fb1630eb55e1066cb208be3cb8806ee4))
    - Bump version to 1.0.0 ([`f39d6b5`](https://github.com/amiller68/jig/commit/f39d6b5fb56180c8cc9f40adf812138f8824b64d))
    - Merge pull request #22 from amiller68/patch/jig-state-directory-restructure ([`bf006bf`](https://github.com/amiller68/jig/commit/bf006bf4b71deae294eff25ed77f3e47d8566368))
    - Restructure jig state directory from .worktrees/ to .jig/ ([`0f3fd30`](https://github.com/amiller68/jig/commit/0f3fd3073b7b06f30e4cb6c0ebe1320433a68dff))
    - Merge pull request #42 from amiller68/chores/command-enum ([`b2a3faf`](https://github.com/amiller68/jig/commit/b2a3fafbbc4debca4f3c5d86b2b103ab797fcff9))
    - Implement Op trait and command_enum! macro for CLI ([`225e9a6`](https://github.com/amiller68/jig/commit/225e9a6d7b8837652cae0da672f7b4b6a0cd069b))
    - Merge pull request #39 from amiller68/ui/prettify-ps ([`614f423`](https://github.com/amiller68/jig/commit/614f4230923b5dcbd76a8010cf79c5922a290b99))
    - Remove unnecessary return statement ([`a41b92c`](https://github.com/amiller68/jig/commit/a41b92cb77141469539658c133da79f79f714452))
    - Merge pull request #29 from amiller68/patch/cli-cleanup ([`ab3dcda`](https://github.com/amiller68/jig/commit/ab3dcda3bfcfcffeb58bb1077a3ce174ce273956))
    - Implement smart jig update command ([`5b776f4`](https://github.com/amiller68/jig/commit/5b776f40ef697de1ecb06c16e97feb4102b23103))
    - Prettify jig ps with Op pattern and comfy-table ([`357f9a6`](https://github.com/amiller68/jig/commit/357f9a6dfb6ab792078fc900f9b1bb956b3a4e4a))
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

## v1.1.1 (2026-03-04)

<csr-id-3d809c6a1b58f3d438c3d279592005947ad50438/>
<csr-id-0d3f00fefd29350c51e4671b9de14d230b809931/>
<csr-id-639e712803a8d13d5f8c84728d0410a17b47561e/>
<csr-id-f39d6b5fb56180c8cc9f40adf812138f8824b64d/>
<csr-id-72ff9fcf89d38f5e74d6d06c128226d2f094feb1/>
<csr-id-d38e493e16a264b81885608389452aa889ddfc6b/>
<csr-id-8abff4b7ca2031d3232127b93febb92eb07cd9c5/>
<csr-id-f7c5d5451126c55a29a5742b0ac55e5d2357dc36/>
<csr-id-12f9c10b9f61aa2054a2d5c2d559553d3af50069/>
<csr-id-f694a0ce3f1a96ad9fc8b38d1c947924e6acaeaf/>
<csr-id-a0c69ed63f57649a00d0484505bafc9c644ca7e9/>
<csr-id-78cff84a46db59e266f2fa4affdaafb3c5857708/>
<csr-id-80401de003d427eeb057c8f64805b91060278fe5/>
<csr-id-225e9a6d7b8837652cae0da672f7b4b6a0cd069b/>

### Chore

 - <csr-id-3d809c6a1b58f3d438c3d279592005947ad50438/> bump version to 1.1.1
 - <csr-id-0d3f00fefd29350c51e4671b9de14d230b809931/> bump version to 1.1.0
 - <csr-id-639e712803a8d13d5f8c84728d0410a17b47561e/> bump all outdated crates to latest major versions
   - thiserror 1 → 2 (no API changes needed)
   - colored 2 → 3 (MSRV bump only, dropped lazy_static)
   - dirs 5 → 6 (API compatible)
   - toml 0.8 → 1.0 (API compatible)
   - handlebars 5 → 6 (RenderError refactored, no impact on our usage)
   - which 6 → 8 (API compatible)
   - nix 0.28 → 0.31 (no breaking changes for process feature)
   - flume 0.11 → 0.12 (API compatible)
 - <csr-id-f39d6b5fb56180c8cc9f40adf812138f8824b64d/> bump version to 1.0.0
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

<csr-id-d790a8101173e5797d7f331b56e0a0f5b06566a4/>
<csr-id-1a8faafa772e7c9014347f6802936d7d9a817bcb/>
<csr-id-73dc3fbbf0178af964a9f0481a5e85fc0e66cde1/>
<csr-id-13e44044ea08a91eb24e4b1b38c43c695a2fadc4/>
<csr-id-1bb57f9c0543cd7af986dd2303f34395980019f4/>
<csr-id-82c654ab1137ec963121638f6741617c59ee0c04/>
<csr-id-d878b9792a36f7c0d1157296401ca80af7f86f30/>
<csr-id-5b776f40ef697de1ecb06c16e97feb4102b23103/>
<csr-id-357f9a6dfb6ab792078fc900f9b1bb956b3a4e4a/>
<csr-id-a685a48ac6c1b1d693e440d4e565e0bbd3ea49c0/>
<csr-id-823eeb1a83ac668fe54b7dbb28a0d062c4f91e9a/>
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

 - <csr-id-780632c2fff774e3f968ee8254f5b57a46abaa55/> show draft vs review state, document PR nudge behavior
   Workers with draft PRs now show "draft" (blue) in the STATE column
   instead of "review" (cyan). This makes it visually clear which workers
   will receive PR nudges (draft) vs which are in human review (non-draft).
   
   Add PR Nudges section to daemon docs explaining the draft/non-draft
   nudge policy and what each health check means.
 - <csr-id-61339c359884180d22d04a206be57d7b28d6fa9a/> unify daemon/ps tick loops and add log toggle to watch mode
   Extract run_with() callback API from daemon so ps --watch shares the
   same setup code path instead of duplicating Daemon/Notifier/TmuxClient
   construction. The callback controls inter-tick delay and can signal
   stop, which enables keypress handling during the sleep window.
   
   Add log view toggle to watch mode: press 'l' to see timestamped daemon
   activity (nudges fired, PR check results, errors), 't' to switch back
   to the table, 'q' to quit cleanly. Uses crossterm raw mode with 100ms
   poll intervals for responsive input.
   
   Also allows spawned workers to transition to stalled (previously
   Spawned status was excluded from silence detection).
 - <csr-id-c34254a3c119de72e0c472c5bf814059547fdbd6/> surface PR health in ps --watch display
   Add a HEALTH column to the watch table showing per-worker PR check
   results (ci, conflicts, reviews, commits) so problems are visible at a
   glance without needing RUST_LOG=debug. Upgrade silent debug-level PR
   errors to info-level logging.
 - <csr-id-8c92e5a1faa6992a14fb494640fb263d6cbc7049/> add --base flag to spawn and create for custom branch base
   Allow overriding the default base branch (from jig.toml) per-command
   with --base/-b. Includes shell completions for branch names across
   bash, zsh, and fish. Also fixes spawn status message to show the
   actual base branch used instead of the current branch.
 - <csr-id-e33ab3dfa06347d2aee13dc6d53d422cc462117c/> wire issues into spawn pipeline with --issue flag
   Add `jig spawn --issue <id>` to resolve file-based issues and use their
   body as Claude context. Thread issue_ref through the full pipeline:
   spawn CLI → register() → Spawn event → WorkerState reducer → daemon
   workers.json → ps watch table.
   
   Also adds:
   - `jig issues` CLI command with --ids flag for scripting
- IssuesConfig in jig.toml for configurable issues directory
- ISSUE column in ps --watch table (shortened last path segment)
- Shell completions for --issue in bash, zsh, and fish
- issue_ref tests in reducer and daemon roundtrip
- TMUX column (●/○/✗) for session liveness
- STATE column from event-derived WorkerStatus
- NUDGES count and PR number from event log
- Configurable interval: `jig ps -w 5` for 5s refresh
- Discovers workers by scanning event log directories
- Replays events to derive current WorkerState per worker
- Compares old vs new state to dispatch actions
- Executes nudges via tmux and notifications via hooks
- Persists state to workers.json between ticks
- Hook wrapper templates that chain jig logic with user hooks
- Registry tracking installed hooks at jig-hooks.json
- Idempotent init with backup/restore of existing hooks
- Post-commit/merge handlers that emit events to worker logs
- Uninstall with rollback to original user hooks
- Event schema with typed EventType enum and flat JSONL serialization
- EventLog append-only reader/writer with per-worker JSONL files
- Claude Code hook templates (PostToolUse, Notification, Stop)
- `jig hooks install-claude` CLI command to install hooks to ~/.claude/hooks/
- Detect installation method (script, cargo, source, unknown)
- Check latest version from GitHub releases API
- Auto-update for script installations (~/.local/bin)
- Prompt dev builds to install release binaries
- Offer cleanup of old cargo bin after source build updates
- Add --force flag to skip version check
- Add Op trait in crates/jig-cli/src/op.rs
- Rewrite ps command with PsOutput, PsError, and Op impl
- Add comfy-table dependency for dynamic table rendering
- Update main.rs dispatch to use Op::execute()
- Add docs/ui/STDOUT-FORMATTING.md documenting the pattern
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

<csr-id-378031a0afe019f57edc9bae469bf8168e05de29/>
<csr-id-61dd7ff112e0cb63885649b399e764578f99e4b2/>
<csr-id-a41b92cb77141469539658c133da79f79f714452/>
<csr-id-bd9a6c99600670089a646b2e32cb6448d0b234bd/>
<csr-id-196774225c8eba52fdb9382f98418ecf82c48567/>

 - <csr-id-d720fcaa0d1f1e0a327ae5d3c90dfe49323b198a/> use if-let instead of unwrap to satisfy clippy
 - <csr-id-52c77af3da99153a3ff98e580f419a70f8500d93/> daemon PR discovery, tmux targeting, and nudge delivery
   - Add proactive PR discovery: daemon queries GitHub for open PRs on
   worker branches when pr_url is unknown, emits PrOpened events to
   make state durable across restarts
- Create per-repo GitHub clients via registry path lookup instead of
     ambient remote detection (fixes multi-repo daemon)
- Extract real branch name from spawn events for tmux window lookup
     (spawn creates windows with slashes, e.g. feature/foo, not dashes)
- Run all four PR checks (CI, conflicts, reviews, commits) on open PRs
- Nudge on every tick, not just state transitions, so polling daemon
     retries delivery until max_nudges
- Collapse multiline nudge templates to single line before tmux send
     to prevent premature submission in TUIs
- Fix tracing init: RUST_LOG now properly overrides default warn level
- Add stderr tick summary in continuous daemon mode for visibility
     without RUST_LOG
- Add debug logging for tmux window misses and notification pipeline

### Other

 - <csr-id-8abff4b7ca2031d3232127b93febb92eb07cd9c5/> fmt
 - <csr-id-f7c5d5451126c55a29a5742b0ac55e5d2357dc36/> fmt

### Refactor

 - <csr-id-12f9c10b9f61aa2054a2d5c2d559553d3af50069/> remove `jig status` command (redundant with `jig ps`)
 - <csr-id-f694a0ce3f1a96ad9fc8b38d1c947924e6acaeaf/> drop -g support from attach/merge/review, deduplicate ps
   attach, merge, and review don't make sense in global mode — worktree
   names can conflict across repos. Extract shared ps logic into
   execute_ps() helper to eliminate duplication between run/run_global.
 - <csr-id-a0c69ed63f57649a00d0484505bafc9c644ca7e9/> split Op trait into run/run_global for -g flag dispatch
   Replace OpContext (single struct with global bool + repos vec) with two
   distinct context types: RepoCtx for single-repo operations and GlobalCtx
   for cross-repo -g mode. The Op trait now has run() and run_global()
   methods, with the default run_global() rejecting unsupported commands.
   
   11 global commands (list, ps, kill, remove, review, merge, attach,
   status, nuke, issues, open) implement both methods. 14 non-global
   commands only implement run(). The command_enum! macro dispatches both,
   and main.rs branches on cli.global to build the right context.
 - <csr-id-78cff84a46db59e266f2fa4affdaafb3c5857708/> unify CLI rendering with shared ui module and daemon-backed ps
   Extract duplicated table rendering, color mappings, and truncation into
   a shared crates/jig-cli/src/ui.rs module. Non-watch `jig ps` now uses a
   single daemon tick (once:true) to get the same rich WorkerDisplayInfo as
   watch mode — same columns (WORKER/STATE/COMMITS/PR/HEALTH/ISSUE) for
   both paths. Merge tmux status indicator into the WORKER name cell
   (colored dot prefix) instead of a separate cryptic column.
   
   Also includes: actor-based daemon runtime, issue/github/sync actors,
   Linear integration, session management, and various daemon improvements
   that were pending on this branch.
 - <csr-id-80401de003d427eeb057c8f64805b91060278fe5/> extract daemon.rs into struct-based daemon/ submodule
   Split the 675-line daemon.rs into a daemon/ directory with three files:
   - mod.rs: Daemon struct with tick/process_worker/sync_repos methods
   - discovery.rs: worker discovery and directory name splitting
   - pr.rs: PrMonitor struct for PR lifecycle checks
   
   This eliminates #[allow(clippy::too_many_arguments)] by moving shared
   state into the Daemon struct. All 7 tests preserved, public API updated
   from daemon::tick() to Daemon::new().tick().
 - <csr-id-225e9a6d7b8837652cae0da672f7b4b6a0cd069b/> implement Op trait and command_enum! macro for CLI
   Introduce a trait-based pattern for CLI commands that provides:
   - Typed errors per command (vs anyhow::Result everywhere)
   - Typed output per command (Display impl for stdout)
   - Unified execution via command_enum! macro
   - Infallible commands use std::convert::Infallible
   
   The macro generates Command enum, OpOutput, OpError, and Op impl,
   reducing boilerplate in main.rs dispatch. Doc comments on Args structs
   are picked up by clap (no duplication needed in cli.rs).
   
   Adds thiserror dependency to jig-cli for per-command error enums.
   Updates docs/PATTERNS.md to document the new pattern.

### New Features (BREAKING)

 - <csr-id-0f3fd3073b7b06f30e4cb6c0ebe1320433a68dff/> restructure jig state directory from .worktrees/ to .jig/
   Move all jig-managed worktrees from <repo>/.worktrees/ to <repo>/.jig/
   and state files to <repo>/.jig/.state/state.json. This provides a
   cleaner directory layout with state files separated from worktrees.
   
   Key changes:
   - Worktrees now live under .jig/ instead of .worktrees/
- State file moved to .jig/.state/state.json
- Auto-migration from .worktrees/ layout on first load
- jig kill/unregister now removes workers from state entirely
     (instead of archiving them)
- jig ps auto-cleans stale workers whose tmux windows are gone
- Hidden directories (.state) are skipped when listing worktrees
- .jig/.state/ added to .gitignore, .jig/ added to git exclude

<csr-unknown>
 add watch mode to ps command for live dashboardjig ps --watch clears and refreshes the worker table every 2s.Shows enriched state from event logs alongside tmux status: add daemon loop to orchestrate event-driven pipelineThe missing conductor: jig daemon runs a periodic loop that:Supports –once for single-pass mode and –interval for tuning. add git hook management (install, uninstall, handlers)Implements the git-hooks epic (tickets 0-4): expand WorkerStatus with event-driven statesAdd Idle, WaitingInput, Stalled variants. Make all variants unit types(remove associated data from WaitingReview/Failed). Add needs_attention(),is_active(), is_terminal(), from_legacy() methods. Snake_case serialization. add event log format and Claude Code hooksImplement event-system tickets 1 and 2: add global state infrastructure for cross-repo aggregationIntroduces ~/.config/jig/ directory structure with structured TOML config,aggregated JSON worker state, and event log directories for the event-drivenpipeline. Ensures global dirs are created at CLI startup. introduce RepoContext and thread repo state through all operationsDerive repo_root, worktrees_dir, git_common_dir, base_branch, andsession_name once at startup via RepoContext::from_cwd(), eliminatingredundant git subprocess calls (e.g. spawn called get_base_repo() 8x).OpContext now holds Option<RepoContext>, and all jig-core functionsaccept &RepoContext instead of re-deriving from cwd. Also adds reporegistry for global mode auto-registration, removes dead spawn::kill(),and updates docs/patterns/issue status. implement smart jig update commandRewrite update command to: prettify jig ps with Op pattern and comfy-tableIntroduce the Op trait to separate command logic from presentation.Rewrite jig ps as the first adopter: ops return typed data, Displayimpls own all formatting via comfy-table with terminal-width-awarecolumn layout and color-coded status indicators. add worktree.copy for gitignored filesAdds worktree.copy config to copy gitignored files (like .env)to new worktrees:toml[worktree]
copy = [".env", ".env.local"]
Files are copied after worktree creation, before on_create hook runs. add worktree config to jig.tomljig.toml now supports worktree configuration: restructure issue tracking with categories and templates improve adapter architecture and audit templatesAdapter improvements:Template improvements: add agent-agnostic adapter architectureThis architecture allows future support for other agents (cursor, etc.)by adding new adapter constants. improve backup, audit prompt, and review skill upgrade jig init scaffolding to language-agnostic skeletons add Claude Code skills and simplify permissions use actual templates for jig init instead of bare-bones placeholdersThe init command now creates a complete scaffolding that matchesthe documentation, instead of empty placeholder comments. add –audit flag to init command that launches Claude interactivelyUses exec() on Unix to replace the current process with Claude Code,giving it full terminal control for interactive documentation audit. add shell-setup command and fix shell completionsRewrite shell completions with dynamic worktree completionUpdate docs/usage/shell-integration.md rewrite health check to validate repo setup and agent scaffoldingReplace terminal-detection-focused health check with structured validationof system deps (git, tmux, claude), repository config (jig.toml, basebranch, .worktrees), and agent scaffolding (CLAUDE.md, settings, skills).Remove unused jq/gh dependency checks and dead required field. Exitnon-zero when checks fail. add shell completions for bash, zsh, and fishShell completions are now emitted alongside the shell wrapper functionin jig shell-init. Completions cover all subcommands, aliases,per-command flags, nested config subcommands, and dynamic worktreename completion via command jig list. register Claude hooks in settings.json, add kill –all and nukeClaude Code hooks were installed as scripts but never registered in~/.claude/settings.json, so they never fired. Now jig init registersthem properly. Also fixes: hook templates read JSON from stdin (notenv vars), spawned workers no longer nudged as stalled, event logsreset on respawn, row ordering stabilized in ps –watch, kill/unregistercleans up event logs, and nuke command added for full repo cleanup. address review findings and wire up event pipeline end-to-endFix 6 issues from code review: UTF-8 safe truncate, stable statusserialization via as_str/from_legacy, stuck nudge sends message afterauto-approve, notification errors logged, branch names URL-encoded,tmux commands check exit status.Wire up missing pipeline links: jig spawn emits Spawn event, jig initauto-installs git+Claude hooks (idempotent on re-run), ps –watch runsdaemon tick on each refresh for integrated orchestration.Add docs/daemon.md with background service setup for launchd, systemd,OpenRC, and generic nohup. remove unnecessary return statement make –audit print command instead of trying to launch claudeSpawning claude programmatically was causing terminal issues and hangs.Now –audit just prints the command for the user to run manually. prevent shell-setup from corrupting shell config filesThe previous byte-slicing approach in find_path_line_end() calculatedoffsets incorrectly because lines() strips newlines but the code assumed+1 byte per line. This could corrupt or truncate config files.<csr-unknown/>

## v1.1.0 (2026-03-03)

<csr-id-0d3f00fefd29350c51e4671b9de14d230b809931/>
<csr-id-639e712803a8d13d5f8c84728d0410a17b47561e/>
<csr-id-f39d6b5fb56180c8cc9f40adf812138f8824b64d/>
<csr-id-72ff9fcf89d38f5e74d6d06c128226d2f094feb1/>
<csr-id-d38e493e16a264b81885608389452aa889ddfc6b/>
<csr-id-f7c5d5451126c55a29a5742b0ac55e5d2357dc36/>
<csr-id-78cff84a46db59e266f2fa4affdaafb3c5857708/>
<csr-id-80401de003d427eeb057c8f64805b91060278fe5/>
<csr-id-225e9a6d7b8837652cae0da672f7b4b6a0cd069b/>

### Chore

 - <csr-id-0d3f00fefd29350c51e4671b9de14d230b809931/> bump version to 1.1.0
 - <csr-id-639e712803a8d13d5f8c84728d0410a17b47561e/> bump all outdated crates to latest major versions
   - thiserror 1 → 2 (no API changes needed)
   - colored 2 → 3 (MSRV bump only, dropped lazy_static)
   - dirs 5 → 6 (API compatible)
   - toml 0.8 → 1.0 (API compatible)
   - handlebars 5 → 6 (RenderError refactored, no impact on our usage)
   - which 6 → 8 (API compatible)
   - nix 0.28 → 0.31 (no breaking changes for process feature)
   - flume 0.11 → 0.12 (API compatible)
 - <csr-id-f39d6b5fb56180c8cc9f40adf812138f8824b64d/> bump version to 1.0.0
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

<csr-id-d790a8101173e5797d7f331b56e0a0f5b06566a4/>
<csr-id-1a8faafa772e7c9014347f6802936d7d9a817bcb/>
<csr-id-73dc3fbbf0178af964a9f0481a5e85fc0e66cde1/>
<csr-id-13e44044ea08a91eb24e4b1b38c43c695a2fadc4/>
<csr-id-1bb57f9c0543cd7af986dd2303f34395980019f4/>
<csr-id-82c654ab1137ec963121638f6741617c59ee0c04/>
<csr-id-d878b9792a36f7c0d1157296401ca80af7f86f30/>
<csr-id-5b776f40ef697de1ecb06c16e97feb4102b23103/>
<csr-id-357f9a6dfb6ab792078fc900f9b1bb956b3a4e4a/>
<csr-id-a685a48ac6c1b1d693e440d4e565e0bbd3ea49c0/>
<csr-id-823eeb1a83ac668fe54b7dbb28a0d062c4f91e9a/>
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

 - <csr-id-780632c2fff774e3f968ee8254f5b57a46abaa55/> show draft vs review state, document PR nudge behavior
   Workers with draft PRs now show "draft" (blue) in the STATE column
   instead of "review" (cyan). This makes it visually clear which workers
   will receive PR nudges (draft) vs which are in human review (non-draft).
   
   Add PR Nudges section to daemon docs explaining the draft/non-draft
   nudge policy and what each health check means.
 - <csr-id-61339c359884180d22d04a206be57d7b28d6fa9a/> unify daemon/ps tick loops and add log toggle to watch mode
   Extract run_with() callback API from daemon so ps --watch shares the
   same setup code path instead of duplicating Daemon/Notifier/TmuxClient
   construction. The callback controls inter-tick delay and can signal
   stop, which enables keypress handling during the sleep window.
   
   Add log view toggle to watch mode: press 'l' to see timestamped daemon
   activity (nudges fired, PR check results, errors), 't' to switch back
   to the table, 'q' to quit cleanly. Uses crossterm raw mode with 100ms
   poll intervals for responsive input.
   
   Also allows spawned workers to transition to stalled (previously
   Spawned status was excluded from silence detection).
 - <csr-id-c34254a3c119de72e0c472c5bf814059547fdbd6/> surface PR health in ps --watch display
   Add a HEALTH column to the watch table showing per-worker PR check
   results (ci, conflicts, reviews, commits) so problems are visible at a
   glance without needing RUST_LOG=debug. Upgrade silent debug-level PR
   errors to info-level logging.
 - <csr-id-8c92e5a1faa6992a14fb494640fb263d6cbc7049/> add --base flag to spawn and create for custom branch base
   Allow overriding the default base branch (from jig.toml) per-command
   with --base/-b. Includes shell completions for branch names across
   bash, zsh, and fish. Also fixes spawn status message to show the
   actual base branch used instead of the current branch.
 - <csr-id-e33ab3dfa06347d2aee13dc6d53d422cc462117c/> wire issues into spawn pipeline with --issue flag
   Add `jig spawn --issue <id>` to resolve file-based issues and use their
   body as Claude context. Thread issue_ref through the full pipeline:
   spawn CLI → register() → Spawn event → WorkerState reducer → daemon
   workers.json → ps watch table.
   
   Also adds:
   - `jig issues` CLI command with --ids flag for scripting
   - Detects user's shell from $SHELL
   - Finds appropriate config file (~/.bashrc, ~/.zshrc, ~/.config/fish/config.fish)
   - Adds eval line with markers for easy identification
   - Places integration after PATH setup when possible
   - Supports --dry-run flag to preview changes

### Bug Fixes

<csr-id-378031a0afe019f57edc9bae469bf8168e05de29/>
<csr-id-61dd7ff112e0cb63885649b399e764578f99e4b2/>
<csr-id-a41b92cb77141469539658c133da79f79f714452/>
<csr-id-bd9a6c99600670089a646b2e32cb6448d0b234bd/>
<csr-id-196774225c8eba52fdb9382f98418ecf82c48567/>

 - <csr-id-52c77af3da99153a3ff98e580f419a70f8500d93/> daemon PR discovery, tmux targeting, and nudge delivery
   - Add proactive PR discovery: daemon queries GitHub for open PRs on
   worker branches when pr_url is unknown, emits PrOpened events to
   make state durable across restarts

### Other

 - <csr-id-f7c5d5451126c55a29a5742b0ac55e5d2357dc36/> fmt

### Refactor

 - <csr-id-78cff84a46db59e266f2fa4affdaafb3c5857708/> unify CLI rendering with shared ui module and daemon-backed ps
   Extract duplicated table rendering, color mappings, and truncation into
   a shared crates/jig-cli/src/ui.rs module. Non-watch `jig ps` now uses a
   single daemon tick (once:true) to get the same rich WorkerDisplayInfo as
   watch mode — same columns (WORKER/STATE/COMMITS/PR/HEALTH/ISSUE) for
   both paths. Merge tmux status indicator into the WORKER name cell
   (colored dot prefix) instead of a separate cryptic column.
   
   Also includes: actor-based daemon runtime, issue/github/sync actors,
   Linear integration, session management, and various daemon improvements
   that were pending on this branch.
 - <csr-id-80401de003d427eeb057c8f64805b91060278fe5/> extract daemon.rs into struct-based daemon/ submodule
   Split the 675-line daemon.rs into a daemon/ directory with three files:
   - mod.rs: Daemon struct with tick/process_worker/sync_repos methods
   - discovery.rs: worker discovery and directory name splitting
   - pr.rs: PrMonitor struct for PR lifecycle checks
   
   This eliminates #[allow(clippy::too_many_arguments)] by moving shared
   state into the Daemon struct. All 7 tests preserved, public API updated
   from daemon::tick() to Daemon::new().tick().
 - <csr-id-225e9a6d7b8837652cae0da672f7b4b6a0cd069b/> implement Op trait and command_enum! macro for CLI
   Introduce a trait-based pattern for CLI commands that provides:
   - Typed errors per command (vs anyhow::Result everywhere)
   - Typed output per command (Display impl for stdout)
   - Unified execution via command_enum! macro
   - Infallible commands use std::convert::Infallible
   
   The macro generates Command enum, OpOutput, OpError, and Op impl,
   reducing boilerplate in main.rs dispatch. Doc comments on Args structs
   are picked up by clap (no duplication needed in cli.rs).
   
   Adds thiserror dependency to jig-cli for per-command error enums.
   Updates docs/PATTERNS.md to document the new pattern.

### New Features (BREAKING)

 - <csr-id-0f3fd3073b7b06f30e4cb6c0ebe1320433a68dff/> restructure jig state directory from .worktrees/ to .jig/
   Move all jig-managed worktrees from <repo>/.worktrees/ to <repo>/.jig/
   and state files to <repo>/.jig/.state/state.json. This provides a
   cleaner directory layout with state files separated from worktrees.
   
   Key changes:
   - Worktrees now live under .jig/ instead of .worktrees/

<csr-unknown>
IssuesConfig in jig.toml for configurable issues directoryISSUE column in ps –watch table (shortened last path segment)Shell completions for –issue in bash, zsh, and fishissue_ref tests in reducer and daemon roundtripTMUX column (●/○/✗) for session livenessSTATE column from event-derived WorkerStatusNUDGES count and PR number from event logConfigurable interval: jig ps -w 5 for 5s refreshDiscovers workers by scanning event log directoriesReplays events to derive current WorkerState per workerCompares old vs new state to dispatch actionsExecutes nudges via tmux and notifications via hooksPersists state to workers.json between ticksHook wrapper templates that chain jig logic with user hooksRegistry tracking installed hooks at jig-hooks.jsonIdempotent init with backup/restore of existing hooksPost-commit/merge handlers that emit events to worker logsUninstall with rollback to original user hooksEvent schema with typed EventType enum and flat JSONL serializationEventLog append-only reader/writer with per-worker JSONL filesClaude Code hook templates (PostToolUse, Notification, Stop)jig hooks install-claude CLI command to install hooks to ~/.claude/hooks/Detect installation method (script, cargo, source, unknown)Check latest version from GitHub releases APIAuto-update for script installations (~/.local/bin)Prompt dev builds to install release binariesOffer cleanup of old cargo bin after source build updatesAdd –force flag to skip version checkAdd Op trait in crates/jig-cli/src/op.rsRewrite ps command with PsOutput, PsError, and Op implAdd comfy-table dependency for dynamic table renderingUpdate main.rs dispatch to use Op::execute()Add docs/ui/STDOUT-FORMATTING.md documenting the patternworktree.base — base branch for new worktrees (overrides global)worktree.on_create — command to run after worktree creationAdd directory-based issue organization (epics/, features/, bugs/, chores/)Add issue templates (_templates/): standalone.md, epic-index.md, ticket.mdCreate plan-and-execute epic for orchestration visionUpdate issues/README.md with comprehensive documentationUpdate /issues skill for new directory structureRemove old flat issue files and _template.mdAdd .backup/ to .gitignoreAdd AgentType enum for compile-time safe matchingRename template to PROJECT.md (agent-agnostic name)Dynamic audit prompt uses adapter.project_file and adapter.skills_dirValidate agent is installed before init (warns if not in PATH)Fix settings.json schema URLFix settings.json to use correct schemastore.org URLAdd WebFetch, WebSearch, mcp__, jig: to default permissionsUpdate review skill to check jig-specific docs and skillsUpdate issues skill to reference issues/README.mdAdd adapter module with AgentAdapter struct for pluggable agent supportjig init now requires agent argument: jig init claudejig.toml stores agent type in [agent] sectionspawn command uses adapter to build agent-specific commandsMove settings.json to templates/adapters/claude-code/Backup now copies files to .backup/ directory preserving path structureAudit prompt is detailed and opinionated about what to fill in each docReview skill now checks for documentation and skills updatesMove issue-tracking.md to issues/README.md, fix “wt” → “jig”Rename skills/jig → skills/spawn for consistencyRemove name: field from skill frontmatterAdd skeleton docs: PATTERNS.md, CONTRIBUTING.md, SUCCESS_CRITERIA.md, PROJECT_LAYOUT.mdExpand docs/index.md as documentation hubMake CLAUDE.md template a skeleton with guidance commentsUpgrade settings.json: add $schema, ask tier for destructive ops, better secret patternsAdd issues/_template.md ticket templateAdd skills for check, draft, issues, review, and spawn commandsSimplify .claude/settings.json using wildcard permissionsAdd jig.toml with spawn auto-configurationFix formatting in init.rsEmbed templates from templates/ directory using include_str!Add all 5 skills: check, draft, issues, review, spawnExpand permissions to cover tools used by skillsSet spawn.auto = true by defaultUse exec() on Unix for –audit flag (full terminal control)Add jig shell-setup command to automatically configure shell integrationFinds appropriate config file (~/.bashrc, ~/.zshrc, ~/.config/fish/config.fish)Adds eval line with markers for easy identificationPlaces integration after PATH setup when possibleSupports –dry-run flag to preview changesjig open/attach/review/merge/kill <TAB> shows actual worktreesContext-aware completions for all subcommandsSimplified zsh completion using _arguments -CAdd quick setup section for shell-setup commandAdd troubleshooting section for common issuesRemove stale sc alias references (legacy from “scribe” name)Create per-repo GitHub clients via registry path lookup instead ofambient remote detection (fixes multi-repo daemon)Extract real branch name from spawn events for tmux window lookup(spawn creates windows with slashes, e.g. feature/foo, not dashes)Run all four PR checks (CI, conflicts, reviews, commits) on open PRsNudge on every tick, not just state transitions, so polling daemonretries delivery until max_nudgesCollapse multiline nudge templates to single line before tmux sendto prevent premature submission in TUIsFix tracing init: RUST_LOG now properly overrides default warn levelAdd stderr tick summary in continuous daemon mode for visibilitywithout RUST_LOGAdd debug logging for tmux window misses and notification pipelineState file moved to .jig/.state/state.jsonAuto-migration from .worktrees/ layout on first loadjig kill/unregister now removes workers from state entirely(instead of archiving them)jig ps auto-cleans stale workers whose tmux windows are goneHidden directories (.state) are skipped when listing worktrees.jig/.state/ added to .gitignore, .jig/ added to git exclude<csr-unknown>
 add watch mode to ps command for live dashboardjig ps --watch clears and refreshes the worker table every 2s.Shows enriched state from event logs alongside tmux status: add daemon loop to orchestrate event-driven pipelineThe missing conductor: jig daemon runs a periodic loop that:Supports –once for single-pass mode and –interval for tuning. add git hook management (install, uninstall, handlers)Implements the git-hooks epic (tickets 0-4): expand WorkerStatus with event-driven statesAdd Idle, WaitingInput, Stalled variants. Make all variants unit types(remove associated data from WaitingReview/Failed). Add needs_attention(),is_active(), is_terminal(), from_legacy() methods. Snake_case serialization. add event log format and Claude Code hooksImplement event-system tickets 1 and 2: add global state infrastructure for cross-repo aggregationIntroduces ~/.config/jig/ directory structure with structured TOML config,aggregated JSON worker state, and event log directories for the event-drivenpipeline. Ensures global dirs are created at CLI startup. introduce RepoContext and thread repo state through all operationsDerive repo_root, worktrees_dir, git_common_dir, base_branch, andsession_name once at startup via RepoContext::from_cwd(), eliminatingredundant git subprocess calls (e.g. spawn called get_base_repo() 8x).OpContext now holds Option<RepoContext>, and all jig-core functionsaccept &RepoContext instead of re-deriving from cwd. Also adds reporegistry for global mode auto-registration, removes dead spawn::kill(),and updates docs/patterns/issue status. implement smart jig update commandRewrite update command to: prettify jig ps with Op pattern and comfy-tableIntroduce the Op trait to separate command logic from presentation.Rewrite jig ps as the first adopter: ops return typed data, Displayimpls own all formatting via comfy-table with terminal-width-awarecolumn layout and color-coded status indicators. add worktree.copy for gitignored filesAdds worktree.copy config to copy gitignored files (like .env)to new worktrees:toml[worktree]
copy = [".env", ".env.local"]
Files are copied after worktree creation, before on_create hook runs. add worktree config to jig.tomljig.toml now supports worktree configuration: restructure issue tracking with categories and templates improve adapter architecture and audit templatesAdapter improvements:Template improvements: add agent-agnostic adapter architectureThis architecture allows future support for other agents (cursor, etc.)by adding new adapter constants. improve backup, audit prompt, and review skill upgrade jig init scaffolding to language-agnostic skeletons add Claude Code skills and simplify permissions use actual templates for jig init instead of bare-bones placeholdersThe init command now creates a complete scaffolding that matchesthe documentation, instead of empty placeholder comments. add –audit flag to init command that launches Claude interactivelyUses exec() on Unix to replace the current process with Claude Code,giving it full terminal control for interactive documentation audit. add shell-setup command and fix shell completionsRewrite shell completions with dynamic worktree completionUpdate docs/usage/shell-integration.md rewrite health check to validate repo setup and agent scaffoldingReplace terminal-detection-focused health check with structured validationof system deps (git, tmux, claude), repository config (jig.toml, basebranch, .worktrees), and agent scaffolding (CLAUDE.md, settings, skills).Remove unused jq/gh dependency checks and dead required field. Exitnon-zero when checks fail. add shell completions for bash, zsh, and fishShell completions are now emitted alongside the shell wrapper functionin jig shell-init. Completions cover all subcommands, aliases,per-command flags, nested config subcommands, and dynamic worktreename completion via command jig list. register Claude hooks in settings.json, add kill –all and nukeClaude Code hooks were installed as scripts but never registered in~/.claude/settings.json, so they never fired. Now jig init registersthem properly. Also fixes: hook templates read JSON from stdin (notenv vars), spawned workers no longer nudged as stalled, event logsreset on respawn, row ordering stabilized in ps –watch, kill/unregistercleans up event logs, and nuke command added for full repo cleanup. address review findings and wire up event pipeline end-to-endFix 6 issues from code review: UTF-8 safe truncate, stable statusserialization via as_str/from_legacy, stuck nudge sends message afterauto-approve, notification errors logged, branch names URL-encoded,tmux commands check exit status.Wire up missing pipeline links: jig spawn emits Spawn event, jig initauto-installs git+Claude hooks (idempotent on re-run), ps –watch runsdaemon tick on each refresh for integrated orchestration.Add docs/daemon.md with background service setup for launchd, systemd,OpenRC, and generic nohup. remove unnecessary return statement make –audit print command instead of trying to launch claudeSpawning claude programmatically was causing terminal issues and hangs.Now –audit just prints the command for the user to run manually. prevent shell-setup from corrupting shell config filesThe previous byte-slicing approach in find_path_line_end() calculatedoffsets incorrectly because lines() strips newlines but the code assumed+1 byte per line. This could corrupt or truncate config files.<csr-unknown/>
<csr-unknown/>

## v1.0.0 (2026-02-20)

<csr-id-f39d6b5fb56180c8cc9f40adf812138f8824b64d/>
<csr-id-72ff9fcf89d38f5e74d6d06c128226d2f094feb1/>
<csr-id-d38e493e16a264b81885608389452aa889ddfc6b/>
<csr-id-225e9a6d7b8837652cae0da672f7b4b6a0cd069b/>

### Chore

 - <csr-id-f39d6b5fb56180c8cc9f40adf812138f8824b64d/> bump version to 1.0.0
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

<csr-id-357f9a6dfb6ab792078fc900f9b1bb956b3a4e4a/>
<csr-id-a685a48ac6c1b1d693e440d4e565e0bbd3ea49c0/>
<csr-id-823eeb1a83ac668fe54b7dbb28a0d062c4f91e9a/>
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

 - <csr-id-5b776f40ef697de1ecb06c16e97feb4102b23103/> implement smart jig update command
   Rewrite update command to:
   - Detect installation method (script, cargo, source, unknown)
- Detects user's shell from $SHELL
- Finds appropriate config file (~/.bashrc, ~/.zshrc, ~/.config/fish/config.fish)
- Adds eval line with markers for easy identification
- Places integration after PATH setup when possible
- Supports --dry-run flag to preview changes

### Bug Fixes

 - <csr-id-a41b92cb77141469539658c133da79f79f714452/> remove unnecessary return statement
 - <csr-id-bd9a6c99600670089a646b2e32cb6448d0b234bd/> make --audit print command instead of trying to launch claude
   Spawning claude programmatically was causing terminal issues and hangs.
   Now --audit just prints the command for the user to run manually.
 - <csr-id-196774225c8eba52fdb9382f98418ecf82c48567/> prevent shell-setup from corrupting shell config files
   The previous byte-slicing approach in find_path_line_end() calculated
   offsets incorrectly because lines() strips newlines but the code assumed
   +1 byte per line. This could corrupt or truncate config files.

### Refactor

 - <csr-id-225e9a6d7b8837652cae0da672f7b4b6a0cd069b/> implement Op trait and command_enum! macro for CLI
   Introduce a trait-based pattern for CLI commands that provides:
   - Typed errors per command (vs anyhow::Result everywhere)
   - Typed output per command (Display impl for stdout)
   - Unified execution via command_enum! macro
   - Infallible commands use std::convert::Infallible
   
   The macro generates Command enum, OpOutput, OpError, and Op impl,
   reducing boilerplate in main.rs dispatch. Doc comments on Args structs
   are picked up by clap (no duplication needed in cli.rs).
   
   Adds thiserror dependency to jig-cli for per-command error enums.
   Updates docs/PATTERNS.md to document the new pattern.

### New Features (BREAKING)

 - <csr-id-0f3fd3073b7b06f30e4cb6c0ebe1320433a68dff/> restructure jig state directory from .worktrees/ to .jig/
   Move all jig-managed worktrees from <repo>/.worktrees/ to <repo>/.jig/
   and state files to <repo>/.jig/.state/state.json. This provides a
   cleaner directory layout with state files separated from worktrees.
   
   Key changes:
   - Worktrees now live under .jig/ instead of .worktrees/

<csr-unknown>
<csr-unknown>
Check latest version from GitHub releases APIAuto-update for script installations (~/.local/bin)Prompt dev builds to install release binariesOffer cleanup of old cargo bin after source build updatesAdd –force flag to skip version checkAdd Op trait in crates/jig-cli/src/op.rsRewrite ps command with PsOutput, PsError, and Op implAdd comfy-table dependency for dynamic table renderingUpdate main.rs dispatch to use Op::execute()Add docs/ui/STDOUT-FORMATTING.md documenting the patternworktree.base — base branch for new worktrees (overrides global)worktree.on_create — command to run after worktree creationAdd directory-based issue organization (epics/, features/, bugs/, chores/)Add issue templates (_templates/): standalone.md, epic-index.md, ticket.mdCreate plan-and-execute epic for orchestration visionUpdate issues/README.md with comprehensive documentationUpdate /issues skill for new directory structureRemove old flat issue files and _template.mdAdd .backup/ to .gitignoreAdd AgentType enum for compile-time safe matchingRename template to PROJECT.md (agent-agnostic name)Dynamic audit prompt uses adapter.project_file and adapter.skills_dirValidate agent is installed before init (warns if not in PATH)Fix settings.json schema URLFix settings.json to use correct schemastore.org URLAdd WebFetch, WebSearch, mcp__, jig: to default permissionsUpdate review skill to check jig-specific docs and skillsUpdate issues skill to reference issues/README.mdAdd adapter module with AgentAdapter struct for pluggable agent supportjig init now requires agent argument: jig init claudejig.toml stores agent type in [agent] sectionspawn command uses adapter to build agent-specific commandsMove settings.json to templates/adapters/claude-code/Backup now copies files to .backup/ directory preserving path structureAudit prompt is detailed and opinionated about what to fill in each docReview skill now checks for documentation and skills updatesMove issue-tracking.md to issues/README.md, fix “wt” → “jig”Rename skills/jig → skills/spawn for consistencyRemove name: field from skill frontmatterAdd skeleton docs: PATTERNS.md, CONTRIBUTING.md, SUCCESS_CRITERIA.md, PROJECT_LAYOUT.mdExpand docs/index.md as documentation hubMake CLAUDE.md template a skeleton with guidance commentsUpgrade settings.json: add $schema, ask tier for destructive ops, better secret patternsAdd issues/_template.md ticket templateAdd skills for check, draft, issues, review, and spawn commandsSimplify .claude/settings.json using wildcard permissionsAdd jig.toml with spawn auto-configurationFix formatting in init.rsEmbed templates from templates/ directory using include_str!Add all 5 skills: check, draft, issues, review, spawnExpand permissions to cover tools used by skillsSet spawn.auto = true by defaultUse exec() on Unix for –audit flag (full terminal control)Add jig shell-setup command to automatically configure shell integrationFinds appropriate config file (~/.bashrc, ~/.zshrc, ~/.config/fish/config.fish)Adds eval line with markers for easy identificationPlaces integration after PATH setup when possibleSupports –dry-run flag to preview changesjig open/attach/review/merge/kill <TAB> shows actual worktreesContext-aware completions for all subcommandsSimplified zsh completion using _arguments -CAdd quick setup section for shell-setup commandAdd troubleshooting section for common issuesRemove stale sc alias references (legacy from “scribe” name)State file moved to .jig/.state/state.jsonAuto-migration from .worktrees/ layout on first loadjig kill/unregister now removes workers from state entirely(instead of archiving them)jig ps auto-cleans stale workers whose tmux windows are goneHidden directories (.state) are skipped when listing worktrees.jig/.state/ added to .gitignore, .jig/ added to git exclude<csr-unknown>
 prettify jig ps with Op pattern and comfy-tableIntroduce the Op trait to separate command logic from presentation.Rewrite jig ps as the first adopter: ops return typed data, Displayimpls own all formatting via comfy-table with terminal-width-awarecolumn layout and color-coded status indicators. add worktree.copy for gitignored filesAdds worktree.copy config to copy gitignored files (like .env)to new worktrees:toml[worktree]
copy = [".env", ".env.local"]
Files are copied after worktree creation, before on_create hook runs. add worktree config to jig.tomljig.toml now supports worktree configuration: restructure issue tracking with categories and templates improve adapter architecture and audit templatesAdapter improvements:Template improvements: add agent-agnostic adapter architectureThis architecture allows future support for other agents (cursor, etc.)by adding new adapter constants. improve backup, audit prompt, and review skill upgrade jig init scaffolding to language-agnostic skeletons add Claude Code skills and simplify permissions use actual templates for jig init instead of bare-bones placeholdersThe init command now creates a complete scaffolding that matchesthe documentation, instead of empty placeholder comments. add –audit flag to init command that launches Claude interactivelyUses exec() on Unix to replace the current process with Claude Code,giving it full terminal control for interactive documentation audit. add shell-setup command and fix shell completionsRewrite shell completions with dynamic worktree completionUpdate docs/usage/shell-integration.md rewrite health check to validate repo setup and agent scaffoldingReplace terminal-detection-focused health check with structured validationof system deps (git, tmux, claude), repository config (jig.toml, basebranch, .worktrees), and agent scaffolding (CLAUDE.md, settings, skills).Remove unused jq/gh dependency checks and dead required field. Exitnon-zero when checks fail. add shell completions for bash, zsh, and fishShell completions are now emitted alongside the shell wrapper functionin jig shell-init. Completions cover all subcommands, aliases,per-command flags, nested config subcommands, and dynamic worktreename completion via command jig list.<csr-unknown/>
<csr-unknown/>
<csr-unknown/>

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

### Bug Fixes

 - <csr-id-bd9a6c99600670089a646b2e32cb6448d0b234bd/> make --audit print command instead of trying to launch claude
   Spawning claude programmatically was causing terminal issues and hangs.
   Now --audit just prints the command for the user to run manually.
 - <csr-id-196774225c8eba52fdb9382f98418ecf82c48567/> prevent shell-setup from corrupting shell config files
   The previous byte-slicing approach in find_path_line_end() calculated
   offsets incorrectly because lines() strips newlines but the code assumed
   +1 byte per line. This could corrupt or truncate config files.

<csr-unknown>
Detects user’s shell from $SHELLFinds appropriate config file (~/.bashrc, ~/.zshrc, ~/.config/fish/config.fish)Adds eval line with markers for easy identificationPlaces integration after PATH setup when possibleSupports –dry-run flag to preview changes<csr-unknown>
<csr-unknown>
worktree.on_create — command to run after worktree creationAdd directory-based issue organization (epics/, features/, bugs/, chores/)Add issue templates (_templates/): standalone.md, epic-index.md, ticket.mdCreate plan-and-execute epic for orchestration visionUpdate issues/README.md with comprehensive documentationUpdate /issues skill for new directory structureRemove old flat issue files and _template.mdAdd .backup/ to .gitignoreAdd AgentType enum for compile-time safe matchingRename template to PROJECT.md (agent-agnostic name)Dynamic audit prompt uses adapter.project_file and adapter.skills_dirValidate agent is installed before init (warns if not in PATH)Fix settings.json schema URLFix settings.json to use correct schemastore.org URLAdd WebFetch, WebSearch, mcp__, jig: to default permissionsUpdate review skill to check jig-specific docs and skillsUpdate issues skill to reference issues/README.mdAdd adapter module with AgentAdapter struct for pluggable agent supportjig init now requires agent argument: jig init claudejig.toml stores agent type in [agent] sectionspawn command uses adapter to build agent-specific commandsMove settings.json to templates/adapters/claude-code/Backup now copies files to .backup/ directory preserving path structureAudit prompt is detailed and opinionated about what to fill in each docReview skill now checks for documentation and skills updatesMove issue-tracking.md to issues/README.md, fix “wt” → “jig”Rename skills/jig → skills/spawn for consistencyRemove name: field from skill frontmatterAdd skeleton docs: PATTERNS.md, CONTRIBUTING.md, SUCCESS_CRITERIA.md, PROJECT_LAYOUT.mdExpand docs/index.md as documentation hubMake CLAUDE.md template a skeleton with guidance commentsUpgrade settings.json: add $schema, ask tier for destructive ops, better secret patternsAdd issues/_template.md ticket templateAdd skills for check, draft, issues, review, and spawn commandsSimplify .claude/settings.json using wildcard permissionsAdd jig.toml with spawn auto-configurationFix formatting in init.rsEmbed templates from templates/ directory using include_str!Add all 5 skills: check, draft, issues, review, spawnExpand permissions to cover tools used by skillsSet spawn.auto = true by defaultUse exec() on Unix for –audit flag (full terminal control)Add jig shell-setup command to automatically configure shell integrationFinds appropriate config file (~/.bashrc, ~/.zshrc, ~/.config/fish/config.fish)Adds eval line with markers for easy identificationPlaces integration after PATH setup when possibleSupports –dry-run flag to preview changesjig open/attach/review/merge/kill/status <TAB> shows actual worktreesContext-aware completions for all subcommandsSimplified zsh completion using _arguments -CAdd quick setup section for shell-setup commandAdd troubleshooting section for common issuesRemove stale sc alias references (legacy from “scribe” name)<csr-unknown>
 restructure issue tracking with categories and templates improve adapter architecture and audit templatesAdapter improvements:Template improvements: add agent-agnostic adapter architectureThis architecture allows future support for other agents (cursor, etc.)by adding new adapter constants. improve backup, audit prompt, and review skill upgrade jig init scaffolding to language-agnostic skeletons add Claude Code skills and simplify permissions use actual templates for jig init instead of bare-bones placeholdersThe init command now creates a complete scaffolding that matchesthe documentation, instead of empty placeholder comments. add –audit flag to init command that launches Claude interactivelyUses exec() on Unix to replace the current process with Claude Code,giving it full terminal control for interactive documentation audit. add shell-setup command and fix shell completionsRewrite shell completions with dynamic worktree completionUpdate docs/usage/shell-integration.md rewrite health check to validate repo setup and agent scaffoldingReplace terminal-detection-focused health check with structured validationof system deps (git, tmux, claude), repository config (jig.toml, basebranch, .worktrees), and agent scaffolding (CLAUDE.md, settings, skills).Remove unused jq/gh dependency checks and dead required field. Exitnon-zero when checks fail. add shell completions for bash, zsh, and fishShell completions are now emitted alongside the shell wrapper functionin jig shell-init. Completions cover all subcommands, aliases,per-command flags, nested config subcommands, and dynamic worktreename completion via command jig list.<csr-unknown/>
<csr-unknown/>
<csr-unknown/>
<csr-unknown/>

## v0.4.0 (2026-02-12)

### New Features

<csr-id-0ab34082c061a8ffba63413c3a6b7e397d12de6f/>
<csr-id-5a59d80324580c092cdda14ce2e2faebf535b444/>

 - <csr-id-f05d75ea429a873ac6f749928f49cb9d850b22eb/> add shell-setup command and fix shell completions
   - Add `jig shell-setup` command to automatically configure shell integration

<csr-unknown>
<csr-unknown>
<csr-unknown>
Detects user’s shell from $SHELL<csr-unknown>
Finds appropriate config file (~/.bashrc, ~/.zshrc, ~/.config/fish/config.fish)Adds eval line with markers for easy identificationPlaces integration after PATH setup when possibleSupports –dry-run flag to preview changesjig open/attach/review/merge/kill/status <TAB> shows actual worktreesContext-aware completions for all subcommandsSimplified zsh completion using _arguments -CAdd quick setup section for shell-setup commandAdd troubleshooting section for common issuesRemove stale sc alias references (legacy from “scribe” name)<csr-unknown>
Rewrite shell completions with dynamic worktree completionUpdate docs/usage/shell-integration.md rewrite health check to validate repo setup and agent scaffoldingReplace terminal-detection-focused health check with structured validationof system deps (git, tmux, claude), repository config (jig.toml, basebranch, .worktrees), and agent scaffolding (CLAUDE.md, settings, skills).Remove unused jq/gh dependency checks and dead required field. Exitnon-zero when checks fail. add shell completions for bash, zsh, and fishShell completions are now emitted alongside the shell wrapper functionin jig shell-init. Completions cover all subcommands, aliases,per-command flags, nested config subcommands, and dynamic worktreename completion via command jig list.<csr-unknown/>
<csr-unknown/>
<csr-unknown/>
<csr-unknown/>
<csr-unknown/>

