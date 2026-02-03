//! Spawn command - create worktree and launch Claude in tmux

use anyhow::Result;
use colored::Colorize;

use wt_core::{config, git, spawn, terminal, Error};

pub fn run(name: &str, context: Option<&str>, auto: bool) -> Result<()> {
    // Check for tmux
    if !terminal::command_exists("tmux") {
        return Err(Error::MissingDependency("tmux".to_string()).into());
    }

    // Check for claude
    if !terminal::command_exists("claude") {
        return Err(Error::MissingDependency("claude".to_string()).into());
    }

    let worktrees_dir = git::get_worktrees_dir()?;
    let worktree_path = worktrees_dir.join(name);

    // Check if worktree already exists
    let needs_create = !worktree_path.exists();

    if needs_create {
        // Create worktree from current branch
        let current_branch = git::get_current_branch()?;
        let base_branch = config::get_base_branch()?;

        git::ensure_worktrees_excluded_auto()?;

        // Create parent directories for nested paths
        if let Some(parent) = worktree_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Create new branch from current position
        git::create_worktree(&worktree_path, name, &base_branch)?;

        eprintln!(
            "{} Created worktree '{}' from '{}'",
            "✓".green(),
            name.cyan(),
            current_branch.cyan()
        );
    }

    // Determine if auto mode should be used
    let use_auto = if auto {
        true
    } else {
        // Check jig.toml for spawn.auto setting
        config::read_jig_toml()?
            .map(|c| c.spawn.auto)
            .unwrap_or(false)
    };

    // Register in spawn state
    let branch = git::get_worktree_branch(&worktree_path)?;
    spawn::register(name, &branch, context)?;

    // Launch in tmux
    spawn::launch_tmux_window(name, &worktree_path, use_auto, context)?;

    eprintln!(
        "{} Launched Claude in tmux window '{}'",
        "✓".green(),
        name.cyan()
    );

    if use_auto {
        eprintln!("  {} Auto mode enabled", "→".dimmed());
    }

    eprintln!();
    eprintln!("  Use '{}' to attach", format!("jig attach {}", name).cyan());
    eprintln!("  Use '{}' to check status", "jig ps".cyan());

    Ok(())
}
