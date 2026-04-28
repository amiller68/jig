//! Attach command - attach to tmux session

use clap::Args;

use jig_core::host::tmux::TmuxSession;
use jig_core::worker::Worker;
use jig_core::{Config, RepoRegistry};

use crate::op::{NoOutput, Op, RepoCtx};

/// Attach to tmux session
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
    Tmux(#[from] jig_core::host::tmux::TmuxError),
}

impl Op for Attach {
    type Error = AttachError;
    type Output = NoOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        match ctx.config() {
            Ok(cfg) => {
                attach(cfg, self.name.as_deref())?;
                Ok(NoOutput)
            }
            Err(_) => {
                let name = self.name.as_deref().ok_or(jig_core::Error::NameRequired)?;
                let registry = RepoRegistry::load().unwrap_or_default();
                let configs: Vec<_> = registry
                    .repos()
                    .iter()
                    .filter(|e| e.path.exists())
                    .filter_map(|e| Config::from_path(&e.path).ok())
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

fn attach(cfg: &Config, name: Option<&str>) -> Result<(), AttachError> {
    match name {
        Some(worker_name) => {
            let workers = Worker::discover(&jig_core::git::Repo::open(&cfg.repo_root).unwrap());
            let worker = workers
                .iter()
                .find(|w| w.branch() == worker_name)
                .ok_or_else(|| {
                    jig_core::Error::Custom(format!("worker '{}' not found", worker_name))
                })?;
            worker.attach()?;
            Ok(())
        }
        None => {
            let session = TmuxSession::new(cfg.session_name());
            session.attach()?;
            Ok(())
        }
    }
}
