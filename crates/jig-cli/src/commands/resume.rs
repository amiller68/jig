//! Resume command — relaunch a dead worker's agent session

use clap::Args;

use jig_core::worktree::Worktree;
use jig_core::{terminal, Error};

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

    /// Auto-start Claude with full prompt
    #[arg(long)]
    pub auto: bool,
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

        // Check for tmux
        if !terminal::command_exists("tmux") {
            return Err(Error::MissingDependency("tmux".to_string()).into());
        }

        // Check for claude
        if !terminal::command_exists("claude") {
            return Err(Error::MissingDependency("claude".to_string()).into());
        }

        // Open existing worktree
        let wt = Worktree::open(&repo.repo_root, &repo.worktrees_dir, &self.name)?;

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
            read_spawn_context(&repo.repo_root, &self.name)
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

/// Read the original spawn context from the worker's event log.
fn read_spawn_context(repo_root: &std::path::Path, worker_name: &str) -> Option<String> {
    let repo_name = repo_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let event_log = jig_core::EventLog::for_worker(&repo_name, worker_name).ok()?;
    let events = event_log.read_all().ok()?;
    events
        .iter()
        .find(|e| e.event_type == jig_core::EventType::Spawn)
        .and_then(|e| e.data.get("context").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
}
