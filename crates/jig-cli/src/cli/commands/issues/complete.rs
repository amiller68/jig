use clap::Args;

use jig_core::issues::IssueStatus;

use crate::cli::op::Op;
use crate::context::{Context, RepoConfig};

use super::{IssuesError, IssuesOutput};

/// Mark an issue as complete
#[derive(Args, Debug, Clone)]
pub struct Complete {
    /// Issue ID (e.g. "features/my-feature")
    pub id: String,

    /// Delete the issue file after marking complete
    #[arg(long)]
    pub delete: bool,
}

fn run(
    repo: &RepoConfig,
    global: &crate::context::Config,
    cmd: &Complete,
) -> Result<IssuesOutput, IssuesError> {
    let linear_provider = repo.linear_provider(global)?;
    linear_provider.update_status(&cmd.id, &IssueStatus::Complete)?;

    Ok(IssuesOutput::Completed(cmd.id.clone(), cmd.delete))
}

impl Op for Complete {
    type Error = IssuesError;
    type Output = IssuesOutput;

    fn run(&self) -> Result<Self::Output, Self::Error> {
        let cfg = Context::from_cwd()?;
        let repo = cfg.repo()?;
        run(repo, &cfg.config, self)
    }
}
