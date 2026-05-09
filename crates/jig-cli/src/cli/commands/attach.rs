//! Attach command - attach to mux session

use clap::Args;

use crate::context::RepoConfig;
use crate::context::RepoRegistry;
use crate::worker::Worker;
use jig_core::git::Branch;
use jig_core::mux::{Mux, TmuxMux};

use crate::cli::op::{NoOutput, Op};

/// Attach to mux session
#[derive(Args, Debug, Clone)]
pub struct Attach {
    /// Branch name to switch to
    pub branch: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum AttachError {
    #[error(transparent)]
    Context(#[from] crate::context::ContextError),
    #[error(transparent)]
    Worker(#[from] crate::worker::WorkerError),
    #[error(transparent)]
    Mux(#[from] jig_core::mux::MuxError),
    #[error("{0}")]
    Usage(String),
}

impl Op for Attach {
    type Error = AttachError;
    type Output = NoOutput;

    fn run(&self) -> Result<Self::Output, Self::Error> {
        match RepoConfig::from_cwd() {
            Ok(cfg) => {
                attach(&cfg, self.branch.as_deref().map(Branch::new).as_ref())?;
                Ok(NoOutput)
            }
            Err(_) => {
                let branch = self.branch.as_deref().ok_or(AttachError::Usage("branch is required".into()))?;
                let registry = RepoRegistry::load().unwrap_or_default();
                let configs: Vec<_> = registry
                    .repos()
                    .iter()
                    .filter(|e| e.path.exists())
                    .filter_map(|e| RepoConfig::from_path(&e.path).ok())
                    .collect();
                let cfg = configs
                    .iter()
                    .find(|c| c.worktrees_path.join(branch).exists())
                    .ok_or(AttachError::Worker(crate::worker::WorkerError::NotFound(branch.to_string())))?;
                attach(cfg, Some(&Branch::new(branch)))?;
                Ok(NoOutput)
            }
        }
    }
}

fn attach(cfg: &RepoConfig, branch: Option<&Branch>) -> Result<(), AttachError> {
    let mux = TmuxMux::new(cfg.session_name());
    match branch {
        Some(branch) => {
            let workers = Worker::discover(&jig_core::git::Repo::open(&cfg.repo_root).unwrap());
            let worker = workers
                .iter()
                .find(|w| w.branch() == branch)
                .ok_or_else(|| {
                    AttachError::Worker(crate::worker::WorkerError::NotFound(branch.to_string()))
                })?;
            worker.attach(&mux)?;
            Ok(())
        }
        None => {
            mux.attach()?;
            Ok(())
        }
    }
}
