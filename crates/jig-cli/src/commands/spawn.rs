//! Spawn command - create worktree and launch Claude in tmux

use clap::Args;
use colored::Colorize;

use jig_core::{git, spawn, terminal, Error, JigToml};

use crate::op::{NoOutput, Op, OpContext};

/// Create worktree and launch Claude in tmux
#[derive(Args, Debug, Clone)]
pub struct Spawn {
    /// Worktree name
    pub name: String,

    /// Task context/description
    #[arg(long, short)]
    pub context: Option<String>,

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

    fn execute(&self, ctx: &OpContext) -> Result<Self::Output, Self::Error> {
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
            // Create worktree from current branch
            let current_branch = git::get_current_branch()?;

            git::ensure_worktrees_excluded(&repo.git_common_dir)?;

            // Create parent directories for nested paths
            if let Some(parent) = worktree_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // Create new branch from current position
            git::create_worktree(&worktree_path, &self.name, &repo.base_branch)?;

            eprintln!(
                "{} Created worktree '{}' from '{}'",
                "✓".green(),
                self.name.cyan(),
                current_branch.cyan()
            );
        }

        // Determine if auto mode should be used
        let use_auto = if self.auto {
            true
        } else {
            // Check jig.toml for spawn.auto setting
            JigToml::load(&repo.repo_root)?
                .map(|c| c.spawn.auto)
                .unwrap_or(false)
        };

        // Register in spawn state
        let branch = git::get_worktree_branch(&worktree_path)?;
        spawn::register(repo, &self.name, &branch, self.context.as_deref())?;

        // Launch in tmux
        spawn::launch_tmux_window(
            repo,
            &self.name,
            &worktree_path,
            use_auto,
            self.context.as_deref(),
        )?;

        eprintln!(
            "{} Launched Claude in tmux window '{}'",
            "✓".green(),
            self.name.cyan()
        );

        if use_auto {
            eprintln!("  {} Auto mode enabled", "→".dimmed());
        }

        eprintln!();
        eprintln!(
            "  Use '{}' to attach",
            format!("jig attach {}", self.name).cyan()
        );
        eprintln!("  Use '{}' to check status", "jig ps".cyan());

        Ok(NoOutput)
    }
}
