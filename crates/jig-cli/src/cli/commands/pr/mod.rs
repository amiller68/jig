//! Pr command — push, create, and inspect pull requests.

mod comments;
mod create;

use std::fmt;

use clap::{Args, Subcommand};

use crate::cli::op::Op;

pub use create::Create;

/// Push, create, and inspect pull requests
#[derive(Args, Debug, Clone)]
pub struct Pr {
    #[command(subcommand)]
    pub command: Option<PrCommand>,

    #[command(flatten)]
    pub create: Create,
}

#[derive(Subcommand, Debug, Clone)]
pub enum PrCommand {
    /// Push current branch and create a draft PR (default)
    Create(Create),
    /// Show review comments and feedback on the current PR
    Comments(comments::Comments),
}

#[derive(Debug, thiserror::Error)]
pub enum PrError {
    #[error(transparent)]
    Context(#[from] crate::context::ContextError),
    #[error(transparent)]
    Git(#[from] jig_core::GitError),
    #[error(transparent)]
    GitHub(#[from] jig_core::github::GitHubError),
    #[error(transparent)]
    Linear(#[from] jig_core::issues::providers::linear::client::LinearError),
    #[error("could not determine current branch")]
    NoBranch,
    #[error("no PR found for current branch")]
    NoPr,
}

#[derive(Debug)]
pub struct PrOutput(pub String);

impl fmt::Display for PrOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Op for Pr {
    type Error = PrError;
    type Output = PrOutput;

    fn run(&self) -> Result<Self::Output, Self::Error> {
        match &self.command {
            Some(PrCommand::Create(cmd)) => cmd.run(),
            Some(PrCommand::Comments(cmd)) => cmd.run(),
            None => self.create.run(),
        }
    }
}
