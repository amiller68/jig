//! Home command - navigate to base repo root

use clap::Args;
use std::path::PathBuf;

use crate::op::{Op, RepoCtx};

/// Go to base repository root
#[derive(Args, Debug, Clone)]
pub struct Home;

#[derive(Debug)]
pub struct HomeOutput(PathBuf);

impl std::fmt::Display for HomeOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cd '{}'", self.0.display())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HomeError {
    #[error(transparent)]
    Core(#[from] jig_core::Error),
}

impl Op for Home {
    type Error = HomeError;
    type Output = HomeOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        let cfg = ctx.config()?;
        Ok(HomeOutput(cfg.repo_root.clone()))
    }
}
