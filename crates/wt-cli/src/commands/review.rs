//! Review command - show diff for parent review

use anyhow::Result;
use colored::Colorize;

use wt_core::{config, git, Error};

pub fn run(name: &str, full: bool) -> Result<()> {
    let worktrees_dir = git::get_worktrees_dir()?;
    let worktree_path = worktrees_dir.join(name);

    if !worktree_path.exists() {
        return Err(Error::WorktreeNotFound(name.to_string()).into());
    }

    let base_branch = config::get_base_branch()?;
    let branch = git::get_worktree_branch(&worktree_path)?;
    let commits = git::get_commits_ahead(&worktree_path, &base_branch)?;
    let is_dirty = git::has_uncommitted_changes(&worktree_path)?;

    // Header
    eprintln!("{}", format!("Review: {}", name).bold());
    eprintln!();
    eprintln!("  {} {}", "Branch:".dimmed(), branch.cyan());
    eprintln!(
        "  {} {} ahead of {}",
        "Commits:".dimmed(),
        commits.len().to_string().cyan(),
        base_branch.dimmed()
    );

    if is_dirty {
        eprintln!();
        eprintln!(
            "  {} {}",
            "Warning:".yellow().bold(),
            "Worktree has uncommitted changes".yellow()
        );
    }

    // Commit history
    if !commits.is_empty() {
        eprintln!();
        eprintln!("{}", "Commits:".bold());
        for commit in &commits {
            eprintln!("  {}", commit);
        }
    }

    // Diff
    eprintln!();
    if full {
        eprintln!("{}", "Full diff:".bold());
        let diff = git::get_diff(&worktree_path, &base_branch)?;
        if diff.is_empty() {
            eprintln!("  No changes");
        } else {
            println!("{}", diff);
        }
    } else {
        eprintln!("{}", "Changed files:".bold());
        let stat = git::get_diff_stat(&worktree_path, &base_branch)?;
        if stat.is_empty() {
            eprintln!("  No changes");
        } else {
            println!("{}", stat);
        }
    }

    Ok(())
}
