//! Ps command - show status of spawned sessions

use anyhow::Result;
use colored::Colorize;

use wt_core::spawn::{self, TaskStatus};

pub fn run() -> Result<()> {
    let tasks = spawn::list_tasks()?;

    if tasks.is_empty() {
        eprintln!("No spawned sessions");
        return Ok(());
    }

    // Print header
    eprintln!(
        "{:16} {:12} {:20} {:8} {}",
        "NAME".bold(),
        "STATUS".bold(),
        "BRANCH".bold(),
        "COMMITS".bold(),
        "DIRTY".bold()
    );

    for task in tasks {
        let status_color = match task.status {
            TaskStatus::Running => task.status.as_str().green(),
            TaskStatus::Exited => task.status.as_str().yellow(),
            TaskStatus::NoSession | TaskStatus::NoWindow => {
                task.status.as_str().red()
            }
        };

        let dirty_indicator = if task.is_dirty {
            "●".yellow().to_string()
        } else {
            "-".dimmed().to_string()
        };

        eprintln!(
            "{:16} {:12} {:20} {:8} {}",
            task.name.cyan(),
            status_color,
            task.branch,
            task.commits_ahead,
            dirty_indicator
        );
    }

    Ok(())
}
