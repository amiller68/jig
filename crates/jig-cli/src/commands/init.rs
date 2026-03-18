//! Init command - initialize repository for jig

use clap::Args;
use std::fs;
use std::path::Path;

use jig_core::git::Repo;
use jig_core::{adapter, session, terminal, Error, JigToml};

use crate::op::{NoOutput, Op, RepoCtx};
use crate::ui;

// Embed templates at compile time from the templates/ directory
const PROJECT_MD_TEMPLATE: &str = include_str!("../../../../templates/PROJECT.md");

// Docs templates
const DOCS_INDEX: &str = include_str!("../../../../templates/docs/index.md");
const DOCS_PATTERNS: &str = include_str!("../../../../templates/docs/PATTERNS.md");
const DOCS_CONTRIBUTING: &str = include_str!("../../../../templates/docs/CONTRIBUTING.md");
const DOCS_SUCCESS_CRITERIA: &str = include_str!("../../../../templates/docs/SUCCESS_CRITERIA.md");

// Issues templates
const ISSUES_README: &str = include_str!("../../../../templates/issues/README.md");
const ISSUES_TEMPLATE_STANDALONE: &str =
    include_str!("../../../../templates/issues/_templates/standalone.md");
const ISSUES_TEMPLATE_EPIC: &str =
    include_str!("../../../../templates/issues/_templates/epic-index.md");
const ISSUES_TEMPLATE_TICKET: &str =
    include_str!("../../../../templates/issues/_templates/ticket.md");

// Skills
const SKILL_CHECK: &str = include_str!("../../../../templates/skills/check/SKILL.md");
const SKILL_DRAFT: &str = include_str!("../../../../templates/skills/draft/SKILL.md");
const SKILL_ISSUES: &str = include_str!("../../../../templates/skills/issues/SKILL.md");
const SKILL_REVIEW: &str = include_str!("../../../../templates/skills/review/SKILL.md");
const SKILL_SPAWN: &str = include_str!("../../../../templates/skills/spawn/SKILL.md");

// Agent-specific templates
const CLAUDE_SETTINGS_JSON: &str =
    include_str!("../../../../templates/adapters/claude-code/settings.json");

/// Initialize repository for jig
#[derive(Args, Debug, Clone)]
pub struct Init {
    /// Agent framework to initialize (claude, cursor)
    #[arg(value_name = "AGENT")]
    pub agent: Option<String>,

    /// Reinitialize, overwriting existing files
    #[arg(long, short)]
    pub force: bool,

    /// Backup existing files before overwriting
    #[arg(long)]
    pub backup: bool,

    /// Launch agent to audit and populate docs. Optionally pass extra instructions.
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    pub audit: Option<String>,

    /// Initialize global config (~/.config/jig/config.toml) instead of a repo
    #[arg(long)]
    pub global: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum InitError {
    #[error(transparent)]
    Core(#[from] Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Unknown agent: '{0}'. Supported agents: {1}")]
    UnknownAgent(String, String),
}

impl Op for Init {
    type Error = InitError;
    type Output = NoOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        // Handle --global: scaffold ~/.config/jig/config.toml
        if self.global {
            return init_global(self.force);
        }

        let agent_name = self.agent.as_deref().ok_or_else(|| {
            InitError::UnknownAgent(
                String::new(),
                format!(
                    "agent argument required. Supported: {}",
                    adapter::supported_agents().join(", ")
                ),
            )
        })?;

        // Init needs get_base_repo() directly because RepoContext may not exist
        // (init is often the first jig command run in a repo)
        let repo_root = match ctx.repo() {
            Ok(repo) => repo.repo_root.clone(),
            Err(_) => {
                let git_repo = Repo::discover()?;
                git_repo.base_repo_dir()
            }
        };

        // Validate agent argument
        let adapter = adapter::get_adapter(agent_name).ok_or_else(|| {
            InitError::UnknownAgent(
                agent_name.to_string(),
                adapter::supported_agents().join(", "),
            )
        })?;

        // Check if agent is installed
        if !terminal::command_exists(adapter.command) {
            ui::warning(&format!(
                "'{}' not found in PATH. Install it before running agents.",
                adapter.command
            ));
        }

        // If already initialized, just ensure hooks are set up
        if JigToml::exists(&repo_root) && !self.force {
            ui::progress("Already initialized, ensuring hooks are set up...");
            install_hooks(&repo_root, adapter, false);
            eprintln!();
            ui::success("Hooks up to date");
            return Ok(NoOutput);
        }

        ui::progress(&format!(
            "Initializing jig for {} in {}",
            adapter.name,
            repo_root.display()
        ));

        // Create backup directory if backup is enabled
        let backup_dir = repo_root.join(".backup");
        if self.backup {
            fs::create_dir_all(&backup_dir)?;
            eprintln!("  {} Created .backup/", ui::SYM_OK);
        }

        let backup_dir_opt = if self.backup {
            Some(backup_dir.as_path())
        } else {
            None
        };

        // Create generic directories
        let generic_dirs = [
            "docs",
            "issues",
            "issues/_templates",
            "issues/epics",
            "issues/features",
            "issues/bugs",
            "issues/chores",
        ];
        for dir in generic_dirs {
            let path = repo_root.join(dir);
            if !path.exists() {
                fs::create_dir_all(&path)?;
                eprintln!("  {} Created {}/", ui::SYM_OK, dir);
            }
        }

        // Create adapter-specific skill directories
        let skill_names = ["check", "draft", "issues", "review", "spawn"];
        for skill in skill_names {
            let dir = repo_root.join(adapter.skills_dir).join(skill);
            if !dir.exists() {
                fs::create_dir_all(&dir)?;
                eprintln!("  {} Created {}/{}/", ui::SYM_OK, adapter.skills_dir, skill);
            }
        }

        // Write jig.toml with agent type
        let jig_toml_content = format!(
            r#"# Worktree configuration
[worktree]
# base = "origin/main"       # Base branch for new worktrees
# on_create = "npm install"  # Command to run after worktree creation
# copy = [".env"]            # Gitignored files to copy to new worktrees

# Agent configuration
[agent]
type = "{}"

# Issue configuration
# [issues]
# provider = "file"                    # or "linear"
# auto_spawn_labels = []               # [] = all issues, ["x"] = filtered, omit = disabled
"#,
            agent_name
        );
        write_file(
            &repo_root,
            "jig.toml",
            &jig_toml_content,
            self.force,
            backup_dir_opt,
        )?;

        // Write generic docs files
        write_file(
            &repo_root,
            "docs/index.md",
            DOCS_INDEX,
            self.force,
            backup_dir_opt,
        )?;
        write_file(
            &repo_root,
            "docs/PATTERNS.md",
            DOCS_PATTERNS,
            self.force,
            backup_dir_opt,
        )?;
        write_file(
            &repo_root,
            "docs/CONTRIBUTING.md",
            DOCS_CONTRIBUTING,
            self.force,
            backup_dir_opt,
        )?;
        write_file(
            &repo_root,
            "docs/SUCCESS_CRITERIA.md",
            DOCS_SUCCESS_CRITERIA,
            self.force,
            backup_dir_opt,
        )?;

        // Write issues files
        write_file(
            &repo_root,
            "issues/README.md",
            ISSUES_README,
            self.force,
            backup_dir_opt,
        )?;
        write_file(
            &repo_root,
            "issues/_templates/standalone.md",
            ISSUES_TEMPLATE_STANDALONE,
            self.force,
            backup_dir_opt,
        )?;
        write_file(
            &repo_root,
            "issues/_templates/epic-index.md",
            ISSUES_TEMPLATE_EPIC,
            self.force,
            backup_dir_opt,
        )?;
        write_file(
            &repo_root,
            "issues/_templates/ticket.md",
            ISSUES_TEMPLATE_TICKET,
            self.force,
            backup_dir_opt,
        )?;

        // Write adapter-specific project file (CLAUDE.md, .cursorrules, etc.)
        write_file(
            &repo_root,
            adapter.project_file,
            PROJECT_MD_TEMPLATE,
            self.force,
            backup_dir_opt,
        )?;

        // Write adapter-specific settings file if applicable
        if let Some(settings_path) = adapter.settings_file {
            let settings_content = get_settings_content(adapter);
            write_file(
                &repo_root,
                settings_path,
                settings_content,
                self.force,
                backup_dir_opt,
            )?;
        }

        // Write skills using adapter's skill file name
        let skills = [
            ("check", SKILL_CHECK),
            ("draft", SKILL_DRAFT),
            ("issues", SKILL_ISSUES),
            ("review", SKILL_REVIEW),
            ("spawn", SKILL_SPAWN),
        ];
        for (skill_name, content) in skills {
            let path = format!(
                "{}/{}/{}",
                adapter.skills_dir, skill_name, adapter.skill_file
            );
            write_file(&repo_root, &path, content, self.force, backup_dir_opt)?;
        }

        install_hooks(&repo_root, adapter, self.force);

        eprintln!();
        ui::success(&ui::bold("Initialization complete"));

        if let Some(ref extra) = self.audit {
            let extra = if extra.is_empty() {
                None
            } else {
                Some(extra.as_str())
            };
            launch_audit(&repo_root, adapter, self.backup, extra)?;
        }

        Ok(NoOutput)
    }
}

/// Generate audit prompt with adapter-specific file paths.
/// When `has_backup` is true, adds instructions to reference `.backup/` files.
/// When `extra` is provided, appends it as additional instructions.
fn audit_prompt(adapter: &adapter::AgentAdapter, has_backup: bool, extra: Option<&str>) -> String {
    let backup_section = if has_backup {
        "\n\n## Reference material\n\n\
         Existing files were backed up to `.backup/` before this initialization. \
         Use these as a jumping-off point — cannibalize content, conventions, and \
         project-specific details from the backup files to populate the new skeletons. \
         Don't copy blindly; adapt the content to fit the new structure."
    } else {
        ""
    };

    let extra_section = match extra {
        Some(text) => format!("\n\n## Additional instructions\n\n{text}"),
        None => String::new(),
    };

    format!(
        r#"Audit this codebase and populate the skeleton documentation files with project-specific content.{backup_section}

## Files to populate

1. **{project_file}** — Fill in:
   - One-line project description
   - Quick Reference commands (build, test, lint, run)
   - Project structure overview
   - Constraints specific to this project
   - Do Not rules specific to this project

2. **docs/index.md** — Fill in:
   - Quick Start section with actual commands
   - Any project-specific agent guidelines

3. **docs/PATTERNS.md** — Document:
   - Error handling patterns used in the codebase
   - Module/file organization conventions
   - Naming conventions
   - Output conventions (stderr/stdout usage)
   - Testing patterns

4. **docs/SUCCESS_CRITERIA.md** — Fill in:
   - Actual build command
   - Actual test command
   - Actual lint command
   - Actual format check command

5. **docs/CONTRIBUTING.md** — Fill in:
   - Setup instructions
   - Commit message conventions used
   - Any project-specific contribution rules

6. **Skills** — Review each skill in {skills_dir}/ and update if needed:
   - /check — Update with project-specific check commands
   - /review — Ensure review criteria match project conventions

Remove HTML comment placeholders as you fill in actual content. Commit when done.{extra_section}"#,
        project_file = adapter.project_file,
        skills_dir = adapter.skills_dir,
    )
}

/// Get settings file content for an adapter
fn get_settings_content(adapter: &adapter::AgentAdapter) -> &'static str {
    match adapter.agent_type {
        adapter::AgentType::Claude => CLAUDE_SETTINGS_JSON,
    }
}

/// Launch the agent with the audit prompt in a tmux session.
fn launch_audit(
    repo_root: &Path,
    adapter: &adapter::AgentAdapter,
    has_backup: bool,
    extra: Option<&str>,
) -> Result<(), InitError> {
    let prompt = audit_prompt(adapter, has_backup, extra);
    let cmd = adapter::build_spawn_command(adapter, Some(&prompt));

    let session_name = "jig-init";
    let window_name = repo_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("init");

    session::create_window(session_name, window_name, repo_root)?;
    session::send_keys(session_name, window_name, &cmd)?;

    eprintln!();
    ui::progress(&format!(
        "Audit launched in tmux session {}:{}",
        session_name, window_name
    ));
    eprintln!();
    eprintln!(
        "  Attach with: {} -t {}",
        ui::bold("tmux attach"),
        session_name
    );

    Ok(())
}

/// Initialize global config at ~/.config/jig/config.toml.
fn init_global(force: bool) -> Result<NoOutput, InitError> {
    let config_dir = jig_core::global::paths::global_config_dir()?;
    let config_path = config_dir.join("config.toml");

    if config_path.exists() && !force {
        ui::success(&format!(
            "Global config already exists: {}",
            config_path.display()
        ));
        eprintln!(
            "  Use {} to overwrite",
            ui::highlight("jig init --global --force")
        );
        return Ok(NoOutput);
    }

    fs::create_dir_all(&config_dir)?;

    let content = r#"# jig global configuration
# See: docs/cli/usage/configuration.md

[health]
silence_threshold_seconds = 300  # seconds of silence before worker is "stalled"
max_nudges = 3                   # nudges per type before escalating

[github]
auto_cleanup_merged = true       # clean up workers when PR merges
auto_cleanup_closed = false      # clean up workers when PR closed without merge

[spawn]
max_concurrent_workers = 3       # max auto-spawned workers per repo
auto_spawn_interval = 120        # seconds between issue polls

# [notify]
# exec = "~/.config/jig/hooks/notify.sh"
# events = ["needs_intervention", "worker_failed"]

# [linear.profiles.work]
# api_key = "lin_api_xxxxxxxxxxxx"
# team = "ENG"
"#;

    fs::write(&config_path, content)?;

    ui::success(&format!("Created {}", config_path.display()));

    Ok(NoOutput)
}

/// Install git hooks and agent-specific hooks (idempotent).
fn install_hooks(repo_root: &Path, adapter: &adapter::AgentAdapter, force: bool) {
    eprintln!();
    ui::progress("Installing git hooks...");
    match jig_core::hooks::init_hooks(repo_root, force) {
        Ok(result) => {
            for r in &result.results {
                match r {
                    jig_core::hooks::install::HookResult::Installed(name) => {
                        eprintln!("  {} {}: installed", ui::SYM_OK, name);
                    }
                    jig_core::hooks::install::HookResult::AlreadyInstalled(name) => {
                        eprintln!("  {} {}: already installed", ui::SYM_OK, name);
                    }
                    jig_core::hooks::install::HookResult::BackedUpAndInstalled {
                        hook,
                        backup: _,
                    } => {
                        eprintln!("  {} {}: installed (backed up existing)", ui::SYM_OK, hook);
                    }
                }
            }
        }
        Err(e) => {
            ui::warning(&format!("Git hooks: {}", e));
        }
    }

    if matches!(adapter.agent_type, jig_core::adapter::AgentType::Claude) {
        ui::progress("Installing Claude Code hooks...");
        match jig_core::hooks::install_claude_hooks() {
            Ok(result) => {
                for name in &result.installed {
                    eprintln!("  {} {}: installed", ui::SYM_OK, name);
                }
                for name in &result.skipped {
                    eprintln!("  {} {}: already exists", ui::SYM_OK, name);
                }
            }
            Err(e) => {
                ui::warning(&format!("Claude hooks: {}", e));
            }
        }
    }
}

fn write_file(
    repo: &Path,
    relative_path: &str,
    content: &str,
    force: bool,
    backup_dir: Option<&Path>,
) -> Result<(), InitError> {
    let path = repo.join(relative_path);

    if path.exists() {
        if let Some(backup_dir) = backup_dir {
            let backup_path = backup_dir.join(relative_path);
            if let Some(parent) = backup_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&path, &backup_path)?;
            eprintln!("  {} Backed up {}", ui::dim(ui::SYM_ARROW), relative_path);
        }

        if !force {
            eprintln!(
                "  {} Skipped {} (exists)",
                ui::dim(ui::SYM_ARROW),
                relative_path
            );
            return Ok(());
        }
    }

    fs::write(&path, content)?;
    eprintln!("  {} Created {}", ui::SYM_OK, relative_path);
    Ok(())
}
