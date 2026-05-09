use clap::Args;

use jig_core::issues::IssueStatus;

use crate::cli::op::Op;
use crate::context::{Context, RepoConfig};

use super::{IssuesError, IssuesOutput};

/// Update issue status
#[derive(Args, Debug, Clone)]
pub struct Status {
    /// Issue ID (e.g. "features/my-feature")
    pub id: String,

    /// New status (triage, backlog, planned, in-progress, complete, blocked)
    #[arg(short, long)]
    pub status: String,
}

fn run(
    repo: &RepoConfig,
    global: &crate::context::Config,
    cmd: &Status,
) -> Result<IssuesOutput, IssuesError> {
    let status: IssueStatus = cmd
        .status
        .parse()
        .map_err(|_| IssuesError::Usage(format!("unknown status: {}", cmd.status)))?;

    let linear_provider = repo.linear_provider(global)?;
    linear_provider.update_status(&cmd.id, &status)?;

    Ok(IssuesOutput::StatusUpdated(
        cmd.id.clone(),
        status.to_string(),
    ))
}

impl Op for Status {
    type Error = IssuesError;
    type Output = IssuesOutput;

    fn run(&self) -> Result<Self::Output, Self::Error> {
        let cfg = Context::from_cwd()?;
        let repo = cfg.repo()?;
        run(repo, &cfg.config, self)
    }
}
