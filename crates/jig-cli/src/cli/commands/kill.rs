//! Kill command - kill a running tmux window

use clap::Args;

use crate::worker::TmuxWorker as Worker;

use crate::cli::op::{GlobalCtx, NoOutput, Op, RepoCtx};
use crate::cli::ui;

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
        let cfg = ctx.config()?;

        if self.all {
            let workers = Worker::discover(&jig_core::git::Repo::open(&cfg.repo_root).unwrap());
            if workers.is_empty() {
                eprintln!("{}", ui::dim("No workers to kill."));
            }
            for worker in &workers {
                let _ = worker.kill();
                worker.unregister()?;
                ui::success(&format!("Killed '{}'", ui::highlight(worker.branch())));
            }
            return Ok(NoOutput);
        }

        let name = self.name.as_deref().ok_or(KillError::NoTarget)?;
        let workers = Worker::discover(&jig_core::git::Repo::open(&cfg.repo_root).unwrap());
        let worker = workers
            .iter()
            .find(|w| w.branch() == name)
            .ok_or_else(|| jig_core::Error::Custom(format!("worker '{}' not found", name)))?;
        let _ = worker.kill();
        worker.unregister()?;
        ui::success(&format!("Killed '{}'", ui::highlight(name)));
        Ok(NoOutput)
    }

    fn run_global(&self, ctx: &GlobalCtx) -> Result<Self::Output, Self::Error> {
        if self.all {
            let mut killed = 0;
            for cfg in &ctx.configs {
                let workers = Worker::discover(&jig_core::git::Repo::open(&cfg.repo_root).unwrap());
                for worker in &workers {
                    let _ = worker.kill();
                    worker.unregister()?;
                    ui::success(&format!("Killed '{}'", ui::highlight(worker.branch())));
                    killed += 1;
                }
            }
            if killed == 0 {
                eprintln!("{}", ui::dim("No workers to kill."));
            }
            return Ok(NoOutput);
        }

        let name = self.name.as_deref().ok_or(KillError::NoTarget)?;
        let cfg = ctx.config_for_worktree(name)?;
        let workers = Worker::discover(&jig_core::git::Repo::open(&cfg.repo_root).unwrap());
        let worker = workers
            .iter()
            .find(|w| w.branch() == name)
            .ok_or_else(|| jig_core::Error::Custom(format!("worker '{}' not found", name)))?;
        let _ = worker.kill();
        worker.unregister()?;
        ui::success(&format!("Killed '{}'", ui::highlight(name)));
        Ok(NoOutput)
    }
}
