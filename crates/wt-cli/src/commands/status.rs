//! Show detailed worker status

use anyhow::Result;
use colored::Colorize;
use wt_core::{git, OrchestratorState, WorkerStatus};

pub fn run(name: Option<&str>) -> Result<()> {
    let repo_root = git::repo_root()?;
    let state = match OrchestratorState::load(&repo_root)? {
        Some(state) => state,
        None => {
            eprintln!("{}", "No state file found. No workers have been spawned.".dimmed());
            return Ok(());
        }
    };

    match name {
        Some(worker_name) => show_worker_status(&state, worker_name),
        None => show_all_workers(&state),
    }
}

fn show_worker_status(state: &OrchestratorState, name: &str) -> Result<()> {
    let worker = state
        .workers
        .values()
        .find(|w| w.name == name)
        .ok_or_else(|| anyhow::anyhow!("Worker '{}' not found", name))?;

    eprintln!("{}", format!("Worker: {}", worker.name).bold());
    eprintln!();
    eprintln!("  {} {}", "ID:".dimmed(), worker.id);
    eprintln!("  {} {}", "Branch:".dimmed(), worker.branch);
    eprintln!("  {} {}", "Base:".dimmed(), worker.base_branch);
    eprintln!("  {} {}", "Path:".dimmed(), worker.worktree_path.display());
    eprintln!("  {} {}", "Status:".dimmed(), format_status(&worker.status));
    eprintln!(
        "  {} {}",
        "Session:".dimmed(),
        format!("{}:{}", worker.tmux_session, worker.tmux_window.as_deref().unwrap_or("?"))
    );

    if let Some(task) = &worker.task {
        eprintln!();
        eprintln!("  {}", "Task:".bold());
        eprintln!("    {}", task.description);
        if let Some(issue) = &task.issue_ref {
            eprintln!("    {} {}", "Issue:".dimmed(), issue);
        }
        if !task.files_hint.is_empty() {
            eprintln!("    {} {}", "Files:".dimmed(), task.files_hint.join(", "));
        }
    }

    if let WorkerStatus::WaitingReview { diff_stats } = &worker.status {
        eprintln!();
        eprintln!("  {}", "Changes:".bold());
        eprintln!(
            "    {} files, {} insertions(+), {} deletions(-)",
            diff_stats.files_changed,
            diff_stats.insertions.to_string().green(),
            diff_stats.deletions.to_string().red()
        );
    }

    eprintln!();
    eprintln!("  {} {}", "Created:".dimmed(), worker.created_at.format("%Y-%m-%d %H:%M:%S"));
    eprintln!("  {} {}", "Updated:".dimmed(), worker.updated_at.format("%Y-%m-%d %H:%M:%S"));

    Ok(())
}

fn show_all_workers(state: &OrchestratorState) -> Result<()> {
    if state.workers.is_empty() {
        eprintln!("{}", "No active workers".dimmed());
        return Ok(());
    }

    eprintln!("{}", "Workers".bold());
    eprintln!();

    for worker in state.workers.values() {
        let status_str = format_status(&worker.status);
        eprintln!(
            "  {} {} {}",
            worker.name.cyan(),
            "→".dimmed(),
            status_str
        );
        if let Some(task) = &worker.task {
            eprintln!("    {}", task.description.dimmed());
        }
    }

    Ok(())
}

fn format_status(status: &WorkerStatus) -> String {
    match status {
        WorkerStatus::Spawned => "spawned".yellow().to_string(),
        WorkerStatus::Running => "running".blue().to_string(),
        WorkerStatus::WaitingReview { diff_stats } => {
            format!(
                "{} ({} files)",
                "waiting review".magenta(),
                diff_stats.files_changed
            )
        }
        WorkerStatus::Approved => "approved".green().to_string(),
        WorkerStatus::Merged => "merged".green().bold().to_string(),
        WorkerStatus::Failed { reason } => format!("{}: {}", "failed".red(), reason),
        WorkerStatus::Archived => "archived".dimmed().to_string(),
    }
}
