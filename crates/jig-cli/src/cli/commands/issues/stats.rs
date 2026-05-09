use clap::Args;

use jig_core::issues::{Issue as CoreIssue, IssueFilter, IssuePriority, IssueStatus};

use crate::cli::op::Op;
use crate::context::{Context, RepoConfig};

use super::{IssuesError, IssuesOutput, StatsData};

/// Show issue statistics
#[derive(Args, Debug, Clone)]
pub struct Stats {
    /// Operate on all tracked repos
    #[arg(short = 'g', long)]
    pub global: bool,
}

fn run_for_repos(
    repos: &[RepoConfig],
    global: &crate::context::Config,
) -> Result<IssuesOutput, IssuesError> {
    let mut all_issues = Vec::new();
    for repo in repos {
        let provider = repo.issue_provider(global)?;
        all_issues.extend(provider.list(&IssueFilter::default())?);
    }
    Ok(IssuesOutput::Stats(compute_stats(&all_issues)))
}

fn compute_stats(issues: &[CoreIssue]) -> StatsData {
    let mut triage = 0usize;
    let mut backlog = 0usize;
    let mut planned = 0usize;
    let mut in_progress = 0usize;
    let mut complete = 0usize;
    let mut blocked = 0usize;

    let mut urgent = 0usize;
    let mut high = 0usize;
    let mut medium = 0usize;
    let mut low = 0usize;

    for issue in issues {
        match issue.status() {
            IssueStatus::Triage => triage += 1,
            IssueStatus::Backlog => backlog += 1,
            IssueStatus::Planned => planned += 1,
            IssueStatus::InProgress => in_progress += 1,
            IssueStatus::Complete => complete += 1,
            IssueStatus::Blocked => blocked += 1,
        }
        match &issue.priority() {
            IssuePriority::Urgent => urgent += 1,
            IssuePriority::High => high += 1,
            IssuePriority::Medium => medium += 1,
            IssuePriority::Low => low += 1,
        }
    }

    let mut by_status = vec![
        ("Triage".to_string(), triage),
        ("Backlog".to_string(), backlog),
        ("Planned".to_string(), planned),
        ("In Progress".to_string(), in_progress),
        ("Complete".to_string(), complete),
        ("Blocked".to_string(), blocked),
    ];
    by_status.retain(|(_, count)| *count > 0);

    let mut by_priority = vec![
        ("Urgent".to_string(), urgent),
        ("High".to_string(), high),
        ("Medium".to_string(), medium),
        ("Low".to_string(), low),
    ];
    by_priority.retain(|(_, count)| *count > 0);

    StatsData {
        by_status,
        by_priority,
    }
}

impl Op for Stats {
    type Error = IssuesError;
    type Output = IssuesOutput;

    fn run(&self) -> Result<Self::Output, Self::Error> {
        if self.global {
            let cfg = Context::from_global()?;
            run_for_repos(&cfg.repos, &cfg.config)
        } else {
            let cfg = Context::from_cwd()?;
            run_for_repos(&cfg.repos, &cfg.config)
        }
    }
}
