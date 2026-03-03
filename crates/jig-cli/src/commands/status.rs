//! Show detailed worker status

use clap::Args;
use colored::Colorize;

use jig_core::OrchestratorState;

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

    fn execute(&self, ctx: &OpContext) -> Result<Self::Output, Self::Error> {
        let repo = ctx.repo()?;
        let state = match OrchestratorState::load(&repo.repo_root)? {
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
    eprintln!(
        "  {} {}",
        "Status:".dimmed(),
        crate::ui::format_worker_status_colored(&worker.status)
    );
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

    if worker.status.is_waiting_review() {
        eprintln!();
        eprintln!(
            "  {} run '{}' to see changes",
            "Review:".bold(),
            format!("jig review {}", worker.name).cyan()
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
        let status_str = crate::ui::format_worker_status_colored(&worker.status);
        eprintln!("  {} {} {}", worker.name.cyan(), "→".dimmed(), status_str);
        if let Some(task) = &worker.task {
            eprintln!("    {}", task.description.dimmed());
        }
    }
}
