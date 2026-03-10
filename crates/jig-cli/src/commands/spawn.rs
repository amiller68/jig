//! Spawn command - create worktree and launch Claude in tmux

use clap::Args;

use jig_core::git::Repo;
use jig_core::global::GlobalConfig;
use jig_core::issues;
use jig_core::{git, spawn, terminal, Error, JigToml};

use crate::op::{NoOutput, Op, RepoCtx};
use crate::ui;

/// Create worktree and launch Claude in tmux
#[derive(Args, Debug, Clone)]
pub struct Spawn {
    /// Worktree name
    pub name: String,

    /// Task context/description
    #[arg(long, short)]
    pub context: Option<String>,

    /// Issue ID to work on (e.g. "features/smart-context-injection")
    #[arg(long, short = 'I')]
    pub issue: Option<String>,

    /// Base branch to create worktree from (overrides jig.toml default)
    #[arg(long, short = 'b')]
    pub base: Option<String>,

    /// Auto-start Claude with full prompt
    #[arg(long)]
    pub auto: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SpawnError {
    #[error(transparent)]
    Core(#[from] Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Op for Spawn {
    type Error = SpawnError;
    type Output = NoOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        let repo = ctx.repo()?;

        // Check for tmux
        if !terminal::command_exists("tmux") {
            return Err(Error::MissingDependency("tmux".to_string()).into());
        }

        // Check for claude
        if !terminal::command_exists("claude") {
            return Err(Error::MissingDependency("claude".to_string()).into());
        }

        let worktree_path = repo.worktrees_dir.join(&self.name);

        // Check if worktree already exists
        let needs_create = !worktree_path.exists();

        if needs_create {
            git::ensure_worktrees_excluded(&repo.git_common_dir)?;

            // Create parent directories for nested paths
            if let Some(parent) = worktree_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // Create new branch from base
            let base = self.base.as_deref().unwrap_or(&repo.base_branch);
            let git_repo = Repo::discover()?;
            git_repo.create_worktree(&worktree_path, &self.name, base)?;

            ui::success(&format!(
                "Created worktree '{}' from '{}'",
                ui::highlight(&self.name),
                ui::highlight(base)
            ));
        }

        // Resolve issue if provided
        let jig_toml = JigToml::load(&repo.repo_root)?.unwrap_or_default();
        let issue_ref = self.issue.as_deref();
        let issue_context = if let Some(id) = issue_ref {
            let global_config = GlobalConfig::load().unwrap_or_default();
            let provider = issues::make_provider(&repo.repo_root, &jig_toml, &global_config)?;
            let issue = provider
                .get(id)?
                .ok_or_else(|| Error::Custom(format!("issue not found: {}", id)))?;
            Some(issue.body)
        } else {
            None
        };

        // Build effective context: --context takes precedence, issue body as fallback
        let effective_context = match (&self.context, &issue_context) {
            (Some(ctx), _) => Some(ctx.clone()),
            (None, Some(body)) => Some(body.clone()),
            (None, None) => None,
        };

        // Determine if auto mode should be used
        let use_auto = if self.auto { true } else { jig_toml.spawn.auto };

        // Register in spawn state
        let branch = Repo::worktree_branch(&worktree_path)?;
        spawn::register(
            repo,
            &self.name,
            &branch,
            effective_context.as_deref(),
            issue_ref,
        )?;

        // Launch in tmux
        spawn::launch_tmux_window(
            repo,
            &self.name,
            &worktree_path,
            use_auto,
            effective_context.as_deref(),
        )?;

        ui::success(&format!(
            "Launched Claude in tmux window '{}'",
            ui::highlight(&self.name)
        ));

        if use_auto {
            ui::detail("Auto mode enabled");
        }

        eprintln!();
        eprintln!(
            "  Use '{}' to attach",
            ui::highlight(&format!("jig attach {}", self.name))
        );
        eprintln!("  Use '{}' to check status", ui::highlight("jig ps"));

        Ok(NoOutput)
    }
}
