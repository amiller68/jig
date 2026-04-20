//! Resume command — relaunch a dead worker's agent session

use clap::Args;

use jig_core::Error;
use jig_core::Worker;

use crate::op::{NoOutput, Op, RepoCtx};
use crate::ui;

/// Resume a dead worker by relaunching its agent session
#[derive(Args, Debug, Clone)]
pub struct Resume {
    /// Worker name to resume
    pub name: String,

    /// Override the task context for the resumed session
    #[arg(long, short)]
    pub context: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ResumeError {
    #[error(transparent)]
    Core(#[from] Error),
}

impl Op for Resume {
    type Error = ResumeError;
    type Output = NoOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        let repo = ctx.repo()?;

        // Open existing worktree
        let wt = Worker::open(&repo.repo_root, &repo.worktrees_path, &self.name)?;

        // Error if tmux window already exists
        if wt.has_tmux_window() {
            ui::failure(&format!(
                "Worker '{}' already has a tmux window. Use '{}' to attach.",
                ui::highlight(&self.name),
                ui::highlight(&format!("jig attach {}", self.name))
            ));
            return Err(Error::Custom(format!(
                "Worker '{}' already running — use `jig attach` instead",
                self.name
            ))
            .into());
        }

        // Read original context from spawn event if no override provided
        let effective_context = if let Some(ref ctx_override) = self.context {
            Some(ctx_override.clone())
        } else {
            let repo_name = repo
                .repo_root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            jig_core::daemon::recovery::RecoveryScanner::read_spawn_context(&repo_name, &self.name)
        };

        // Resume the worker
        wt.resume(effective_context.as_deref())?;

        ui::success(&format!(
            "Resumed worker '{}' in tmux",
            ui::highlight(&self.name)
        ));

        eprintln!();
        eprintln!(
            "  Use '{}' to attach",
            ui::highlight(&format!("jig attach {}", self.name))
        );
        eprintln!("  Use '{}' to check status", ui::highlight("jig ps"));

        Ok(NoOutput)
    }
}
