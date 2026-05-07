//! Kill command - kill a running worker window

use clap::Args;

use crate::context::{Context, RepoConfig};
use crate::worker::Worker;
use jig_core::mux::TmuxMux;

use crate::cli::op::{NoOutput, Op};
use crate::cli::ui;

/// Kill a running worker window
#[derive(Args, Debug, Clone)]
pub struct Kill {
    /// Worktree name
    pub name: Option<String>,

    /// Kill all workers
    #[arg(long, short)]
    pub all: bool,

    /// Operate on all tracked repos
    #[arg(short = 'g', long)]
    global: bool,
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

    fn run(&self) -> Result<Self::Output, Self::Error> {
        if self.global {
            let cfg = Context::from_global()?;

            if self.all {
                let mut killed = 0;
                for repo in &cfg.repos {
                    killed += kill_all_in_repo(repo)?;
                }
                if killed == 0 {
                    eprintln!("{}", ui::dim("No workers to kill."));
                }
                return Ok(NoOutput);
            }

            let name = self.name.as_deref().ok_or(KillError::NoTarget)?;
            for repo in &cfg.repos {
                let git_repo = jig_core::git::Repo::open(&repo.repo_root).unwrap();
                let repo_name = repo.repo_root.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                let mux = TmuxMux::for_repo(&repo_name);
                let workers = Worker::discover(&git_repo);
                if let Some(worker) = workers.iter().find(|w| w.branch() == name) {
                    let _ = worker.kill(&mux);
                    worker.unregister()?;
                    ui::success(&format!("Killed '{}'", ui::highlight(name)));
                    return Ok(NoOutput);
                }
            }
            return Err(jig_core::Error::Custom(format!("worker '{}' not found", name)).into());
        }

        let cfg = Context::from_cwd()?;
        let repo = cfg.repo()?;
        let repo_name = repo.repo_root.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let mux = TmuxMux::for_repo(&repo_name);

        if self.all {
            let killed = kill_all_in_repo(repo)?;
            if killed == 0 {
                eprintln!("{}", ui::dim("No workers to kill."));
            }
            return Ok(NoOutput);
        }

        let name = self.name.as_deref().ok_or(KillError::NoTarget)?;
        let workers = Worker::discover(&jig_core::git::Repo::open(&repo.repo_root).unwrap());
        let worker = workers
            .iter()
            .find(|w| w.branch() == name)
            .ok_or_else(|| jig_core::Error::Custom(format!("worker '{}' not found", name)))?;
        let _ = worker.kill(&mux);
        worker.unregister()?;
        ui::success(&format!("Killed '{}'", ui::highlight(name)));
        Ok(NoOutput)
    }
}

fn kill_all_in_repo(repo: &RepoConfig) -> Result<usize, KillError> {
    let repo_name = repo.repo_root.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let mux = TmuxMux::for_repo(&repo_name);
    let workers = Worker::discover(&jig_core::git::Repo::open(&repo.repo_root).unwrap());
    for worker in &workers {
        let _ = worker.kill(&mux);
        worker.unregister()?;
        ui::success(&format!("Killed '{}'", ui::highlight(worker.branch())));
    }
    Ok(workers.len())
}
