//! List worktrees command

use anyhow::Result;
use colored::Colorize;

use wt_core::git;

pub fn run(all: bool) -> Result<()> {
    if all {
        // Show all git worktrees including base repo
        let worktrees = git::list_all_worktrees()?;

        for (path, branch) in worktrees {
            let branch_display = if branch.is_empty() {
                "(detached)".dimmed().to_string()
            } else {
                branch.cyan().to_string()
            };
            eprintln!("{} {}", path.display(), branch_display);
        }
    } else {
        // Show only .worktrees/
        let worktrees = git::list_worktrees()?;

        if worktrees.is_empty() {
            eprintln!("No worktrees found");
            return Ok(());
        }

        for name in worktrees {
            println!("{}", name);
        }
    }

    Ok(())
}
