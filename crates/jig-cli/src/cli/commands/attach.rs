//! Attach command - attach to mux session

use clap::Args;

use crate::context::RepoConfig;
use crate::context::RepoRegistry;
use crate::worker::Worker;
use jig_core::mux::{Mux, TmuxMux};

use crate::cli::op::{NoOutput, Op};

/// Attach to mux session
#[derive(Args, Debug, Clone)]
pub struct Attach {
    /// Window name to switch to
    pub name: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum AttachError {
    #[error(transparent)]
    Core(#[from] jig_core::Error),
    #[error(transparent)]
    Mux(#[from] jig_core::MuxError),
}

impl Op for Attach {
    type Error = AttachError;
    type Output = NoOutput;

    fn run(&self) -> Result<Self::Output, Self::Error> {
        match RepoConfig::from_cwd() {
            Ok(cfg) => {
                attach(&cfg, self.name.as_deref())?;
                Ok(NoOutput)
            }
            Err(_) => {
                let name = self.name.as_deref().ok_or(jig_core::Error::NameRequired)?;
                let registry = RepoRegistry::load().unwrap_or_default();
                let configs: Vec<_> = registry
                    .repos()
                    .iter()
                    .filter(|e| e.path.exists())
                    .filter_map(|e| RepoConfig::from_path(&e.path).ok())
                    .collect();
                let cfg = configs
                    .iter()
                    .find(|c| c.worktrees_path.join(name).exists())
                    .ok_or(jig_core::Error::WorktreeNotFound(name.to_string()))?;
                attach(cfg, Some(name))?;
                Ok(NoOutput)
            }
        }
    }
}

fn attach(cfg: &RepoConfig, name: Option<&str>) -> Result<(), AttachError> {
    let mux = TmuxMux::new(cfg.session_name());
    match name {
        Some(worker_name) => {
            let workers = Worker::discover(&jig_core::git::Repo::open(&cfg.repo_root).unwrap());
            let worker = workers
                .iter()
                .find(|w| w.branch() == worker_name)
                .ok_or_else(|| {
                    jig_core::Error::Custom(format!("worker '{}' not found", worker_name))
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
