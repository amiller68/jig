//! Open worktree command

use anyhow::Result;
use colored::Colorize;

use wt_core::{git, terminal, Error};

pub fn run(name: Option<&str>, all: bool) -> Result<()> {
    let worktrees_dir = git::get_worktrees_dir()?;

    if all {
        // Open all worktrees in new tabs
        let worktrees = git::list_worktrees()?;

        if worktrees.is_empty() {
            eprintln!("No worktrees to open");
            return Ok(());
        }

        for wt_name in worktrees {
            let path = worktrees_dir.join(&wt_name);
            if path.exists() {
                let opened = terminal::open_tab(&path)?;
                if opened {
                    eprintln!("{} Opened '{}' in new tab", "✓".green(), wt_name.cyan());
                }
            }
        }

        // Don't output cd command - tabs are opened directly
        Ok(())
    } else {
        // Open specific worktree
        let name = name.ok_or(Error::NameRequired)?;
        let worktree_path = worktrees_dir.join(name);

        if !worktree_path.exists() {
            return Err(Error::WorktreeNotFound(name.to_string()).into());
        }

        // Output cd command for shell wrapper to eval
        let canonical = worktree_path.canonicalize()?;
        println!("cd '{}'", canonical.display());

        Ok(())
    }
}
