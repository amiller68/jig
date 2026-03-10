//! Kill command - kill a running tmux window

use clap::Args;

use jig_core::spawn;

use crate::op::{GlobalCtx, NoOutput, Op, RepoCtx};
use crate::ui;

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

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        let repo = ctx.repo()?;

        if self.all {
            let tasks = spawn::list_tasks(repo)?;
            if tasks.is_empty() {
                eprintln!("{}", ui::dim("No workers to kill."));
            }
            for task in &tasks {
                let _ = spawn::kill_window(repo, &task.name);
                spawn::unregister(repo, &task.name)?;
                ui::success(&format!("Killed '{}'", ui::highlight(&task.name)));
            }
            return Ok(NoOutput);
        }

        let name = self.name.as_deref().ok_or(KillError::NoTarget)?;
        spawn::kill_window(repo, name)?;
        spawn::unregister(repo, name)?;
        ui::success(&format!("Killed '{}'", ui::highlight(name)));
        Ok(NoOutput)
    }

    fn run_global(&self, ctx: &GlobalCtx) -> Result<Self::Output, Self::Error> {
        if self.all {
            let mut killed = 0;
            for repo in &ctx.repos {
                let tasks = spawn::list_tasks(repo)?;
                for task in &tasks {
                    let _ = spawn::kill_window(repo, &task.name);
                    spawn::unregister(repo, &task.name)?;
                    ui::success(&format!("Killed '{}'", ui::highlight(&task.name)));
                    killed += 1;
                }
            }
            if killed == 0 {
                eprintln!("{}", ui::dim("No workers to kill."));
            }
            return Ok(NoOutput);
        }

        let name = self.name.as_deref().ok_or(KillError::NoTarget)?;
        let repo = ctx.repo_for_worktree(name)?;
        spawn::kill_window(repo, name)?;
        spawn::unregister(repo, name)?;
        ui::success(&format!("Killed '{}'", ui::highlight(name)));
        Ok(NoOutput)
    }
}
