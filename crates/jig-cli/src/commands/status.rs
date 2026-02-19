//! Show detailed worker status

use clap::Args;
use colored::Colorize;

use jig_core::{git, OrchestratorState, WorkerStatus};

use crate::op::{NoOutput, Op, OpContext};

/// Show detailed worker status
#[derive(Args, Debug, Clone)]
pub struct Status {
    /// Worker name
    pub name: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum StatusError {
    #[error(transparent)]
    Core(#[from] jig_core::Error),
    #[error("Worker '{0}' not found")]
    WorkerNotFound(String),
}

impl Op for Status {
    type Error = StatusError;
    type Output = NoOutput;

    fn execute(&self, _ctx: &OpContext) -> Result<Self::Output, Self::Error> {
        let repo_root = git::repo_root()?;
        let state = match OrchestratorState::load(&repo_root)? {
            Some(state) => state,
            None => {
                eprintln!(
                    "{}",
                    "No state file found. No workers have been spawned.".dimmed()
                );
                return Ok(NoOutput);
            }
        };

        match &self.name {
            Some(worker_name) => show_worker_status(&state, worker_name)?,
            None => show_all_workers(&state),
        }

        Ok(NoOutput)
    }
}

fn show_worker_status(state: &OrchestratorState, name: &str) -> Result<(), StatusError> {
    let worker = state
        .workers
        .values()
        .find(|w| w.name == name)
        .ok_or_else(|| StatusError::WorkerNotFound(name.to_string()))?;

    eprintln!("{}", format!("Worker: {}", worker.name).bold());
    eprintln!();
    eprintln!("  {} {}", "ID:".dimmed(), worker.id);
    eprintln!("  {} {}", "Branch:".dimmed(), worker.branch);
    eprintln!("  {} {}", "Base:".dimmed(), worker.base_branch);
    eprintln!("  {} {}", "Path:".dimmed(), worker.worktree_path.display());
    eprintln!("  {} {}", "Status:".dimmed(), format_status(&worker.status));
    eprintln!(
        "  {} {}:{}",
        "Session:".dimmed(),
        worker.tmux_session,
        worker.tmux_window.as_deref().unwrap_or("?")
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
    eprintln!(
        "  {} {}",
        "Created:".dimmed(),
        worker.created_at.format("%Y-%m-%d %H:%M:%S")
    );
    eprintln!(
        "  {} {}",
        "Updated:".dimmed(),
        worker.updated_at.format("%Y-%m-%d %H:%M:%S")
    );

    Ok(())
}

fn show_all_workers(state: &OrchestratorState) {
    if state.workers.is_empty() {
        eprintln!("{}", "No active workers".dimmed());
        return;
    }

    eprintln!("{}", "Workers".bold());
    eprintln!();

    for worker in state.workers.values() {
        let status_str = format_status(&worker.status);
        eprintln!("  {} {} {}", worker.name.cyan(), "→".dimmed(), status_str);
        if let Some(task) = &worker.task {
            eprintln!("    {}", task.description.dimmed());
        }
    }
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
