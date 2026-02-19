//! Init command - initialize repository for jig

use clap::Args;
use colored::Colorize;
use std::fs;
use std::path::Path;

use jig_core::{adapter, config, git, terminal, Error};

use crate::op::{NoOutput, Op, OpContext};

// Embed templates at compile time from the templates/ directory
const PROJECT_MD_TEMPLATE: &str = include_str!("../../../../templates/PROJECT.md");

// Docs templates
const DOCS_INDEX: &str = include_str!("../../../../templates/docs/index.md");
const DOCS_PATTERNS: &str = include_str!("../../../../templates/docs/PATTERNS.md");
const DOCS_CONTRIBUTING: &str = include_str!("../../../../templates/docs/CONTRIBUTING.md");
const DOCS_SUCCESS_CRITERIA: &str = include_str!("../../../../templates/docs/SUCCESS_CRITERIA.md");
const DOCS_PROJECT_LAYOUT: &str = include_str!("../../../../templates/docs/PROJECT_LAYOUT.md");

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
    pub agent: String,

    /// Reinitialize, overwriting existing files
    #[arg(long, short)]
    pub force: bool,

    /// Backup existing files before overwriting
    #[arg(long)]
    pub backup: bool,

    /// Print audit prompt after init
    #[arg(long)]
    pub audit: bool,
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

    fn execute(&self, _ctx: &OpContext) -> Result<Self::Output, Self::Error> {
        let repo = git::get_base_repo()?;

        // Validate agent argument
        let adapter = adapter::get_adapter(&self.agent).ok_or_else(|| {
            InitError::UnknownAgent(self.agent.clone(), adapter::supported_agents().join(", "))
        })?;

        // Check if agent is installed
        if !terminal::command_exists(adapter.command) {
            eprintln!(
                "{} '{}' not found in PATH. Install it before running agents.",
                "warning:".yellow().bold(),
                adapter.command
            );
        }

        // Check if already initialized
        if config::has_jig_toml()? && !self.force {
            return Err(Error::AlreadyInitialized.into());
        }

        eprintln!(
            "{} Initializing jig for {} in {}",
            "→".cyan(),
            adapter.name,
            repo.display()
        );

        // Create backup directory if backup is enabled
        let backup_dir = repo.join(".backup");
        if self.backup {
            fs::create_dir_all(&backup_dir)?;
            eprintln!("  {} Created .backup/", "✓".green());
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
            let path = repo.join(dir);
            if !path.exists() {
                fs::create_dir_all(&path)?;
                eprintln!("  {} Created {}/", "✓".green(), dir);
            }
        }

        // Create adapter-specific skill directories
        let skill_names = ["check", "draft", "issues", "review", "spawn"];
        for skill in skill_names {
            let dir = repo.join(adapter.skills_dir).join(skill);
            if !dir.exists() {
                fs::create_dir_all(&dir)?;
                eprintln!(
                    "  {} Created {}/{}/",
                    "✓".green(),
                    adapter.skills_dir,
                    skill
                );
            }
        }

        // Write jig.toml with agent type
        let jig_toml_content = format!(
            r#"# Worktree configuration
[worktree]
# base = "origin/main"       # Base branch for new worktrees
# on_create = "npm install"  # Command to run after worktree creation
# copy = [".env"]            # Gitignored files to copy to new worktrees

# Spawn configuration
[spawn]
auto = true

# Agent configuration
[agent]
type = "{}"
"#,
            self.agent
        );
        write_file(
            &repo,
            "jig.toml",
            &jig_toml_content,
            self.force,
            backup_dir_opt,
        )?;

        // Write generic docs files
        write_file(
            &repo,
            "docs/index.md",
            DOCS_INDEX,
            self.force,
            backup_dir_opt,
        )?;
        write_file(
            &repo,
            "docs/PATTERNS.md",
            DOCS_PATTERNS,
            self.force,
            backup_dir_opt,
        )?;
        write_file(
            &repo,
            "docs/CONTRIBUTING.md",
            DOCS_CONTRIBUTING,
            self.force,
            backup_dir_opt,
        )?;
        write_file(
            &repo,
            "docs/SUCCESS_CRITERIA.md",
            DOCS_SUCCESS_CRITERIA,
            self.force,
            backup_dir_opt,
        )?;
        write_file(
            &repo,
            "docs/PROJECT_LAYOUT.md",
            DOCS_PROJECT_LAYOUT,
            self.force,
            backup_dir_opt,
        )?;

        // Write issues files
        write_file(
            &repo,
            "issues/README.md",
            ISSUES_README,
            self.force,
            backup_dir_opt,
        )?;
        write_file(
            &repo,
            "issues/_templates/standalone.md",
            ISSUES_TEMPLATE_STANDALONE,
            self.force,
            backup_dir_opt,
        )?;
        write_file(
            &repo,
            "issues/_templates/epic-index.md",
            ISSUES_TEMPLATE_EPIC,
            self.force,
            backup_dir_opt,
        )?;
        write_file(
            &repo,
            "issues/_templates/ticket.md",
            ISSUES_TEMPLATE_TICKET,
            self.force,
            backup_dir_opt,
        )?;

        // Write adapter-specific project file (CLAUDE.md, .cursorrules, etc.)
        write_file(
            &repo,
            adapter.project_file,
            PROJECT_MD_TEMPLATE,
            self.force,
            backup_dir_opt,
        )?;

        // Write adapter-specific settings file if applicable
        if let Some(settings_path) = adapter.settings_file {
            let settings_content = get_settings_content(adapter);
            write_file(
                &repo,
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
            write_file(&repo, &path, content, self.force, backup_dir_opt)?;
        }

        eprintln!();
        eprintln!("{} Initialization complete", "✓".green().bold());

        if self.audit {
            print_audit_prompt(adapter);
        }

        Ok(NoOutput)
    }
}

/// Generate audit prompt with adapter-specific file paths
fn audit_prompt(adapter: &adapter::AgentAdapter) -> String {
    format!(
        r#"Audit this codebase and populate the skeleton documentation files with project-specific content.

## Files to populate

1. **{}** — Fill in:
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

5. **docs/PROJECT_LAYOUT.md** — Document:
   - Actual directory structure with descriptions
   - Key files and their purposes
   - Entry points

6. **docs/CONTRIBUTING.md** — Fill in:
   - Setup instructions
   - Commit message conventions used
   - Any project-specific contribution rules

7. **Skills** — Review each skill in {}/  and update if needed:
   - /check — Update with project-specific check commands
   - /review — Ensure review criteria match project conventions

Remove HTML comment placeholders as you fill in actual content. Commit when done."#,
        adapter.project_file, adapter.skills_dir
    )
}

/// Get settings file content for an adapter
fn get_settings_content(adapter: &adapter::AgentAdapter) -> &'static str {
    match adapter.agent_type {
        adapter::AgentType::Claude => CLAUDE_SETTINGS_JSON,
    }
}

/// Print the audit prompt for an adapter
fn print_audit_prompt(adapter: &adapter::AgentAdapter) {
    eprintln!();
    eprintln!(
        "{} Run this to audit and populate documentation:",
        "→".cyan()
    );
    eprintln!();
    eprintln!("  {} \"{}\"", adapter.command, audit_prompt(adapter));
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
            eprintln!("  {} Backed up {}", "→".dimmed(), relative_path);
        }

        if !force {
            eprintln!("  {} Skipped {} (exists)", "→".dimmed(), relative_path);
            return Ok(());
        }
    }

    fs::write(&path, content)?;
    eprintln!("  {} Created {}", "✓".green(), relative_path);
    Ok(())
}
