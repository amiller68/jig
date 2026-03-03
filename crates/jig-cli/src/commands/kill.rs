//! Kill command - kill a running tmux window

use clap::Args;
use colored::Colorize;

use jig_core::spawn;

use crate::op::{NoOutput, Op, OpContext};

/// Kill a running tmux window
#[derive(Args, Debug, Clone)]
pub struct Kill {
    /// Worktree name
    pub name: Option<String>,

    /// Kill all workers
    #[arg(long, short)]
    pub all: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum KillError {
    #[error(transparent)]
    Core(#[from] jig_core::Error),

    #[error("specify a worker name or --all")]
    NoTarget,
}

impl Op for Kill {
    type Error = KillError;
    type Output = NoOutput;

    fn execute(&self, ctx: &OpContext) -> Result<Self::Output, Self::Error> {
        let repo = ctx.repo()?;

        if self.all {
            let tasks = spawn::list_tasks(repo)?;
            if tasks.is_empty() {
                eprintln!("{}", "No workers to kill.".dimmed());
                return Ok(NoOutput);
            }
            for task in &tasks {
                let _ = spawn::kill_window(repo, &task.name);
                spawn::unregister(repo, &task.name)?;
                eprintln!("{} Killed '{}'", "✓".green(), task.name.cyan());
            }
            return Ok(NoOutput);
        }

        let name = self.name.as_deref().ok_or(KillError::NoTarget)?;

        spawn::kill_window(repo, name)?;
        spawn::unregister(repo, name)?;
        eprintln!("{} Killed '{}'", "✓".green(), name.cyan());

        Ok(NoOutput)
    }
}
