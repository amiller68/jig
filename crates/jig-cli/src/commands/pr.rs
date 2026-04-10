//! Pr command — push current branch and create a draft PR with automatic base resolution

use std::process::Command;

use clap::Args;

use jig_core::git;
use jig_core::state::OrchestratorState;
use jig_core::Error;

use crate::op::{Op, RepoCtx};
use crate::ui;

/// Push current branch and create a draft PR
#[derive(Args, Debug, Clone)]
pub struct Pr {
    /// PR title (defaults to --fill behavior)
    #[arg(short, long)]
    pub title: Option<String>,

    /// PR body/description
    #[arg(short, long)]
    pub body: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum PrError {
    #[error(transparent)]
    Core(#[from] Error),
    #[error("git push failed: {0}")]
    PushFailed(String),
    #[error("gh pr create failed: {0}")]
    GhFailed(String),
    #[error("could not determine current branch")]
    NoBranch,
}

#[derive(Debug)]
pub struct PrOutput(String);

impl std::fmt::Display for PrOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Op for Pr {
    type Error = PrError;
    type Output = PrOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        let repo = ctx.repo()?;

        // 1. Get current branch
        let git_repo = jig_core::git::Repo::discover()?;
        let branch = git_repo.current_branch().map_err(|_| PrError::NoBranch)?;

        // 2. Resolve base branch
        let base = resolve_base(&repo.worktrees_dir, &repo.repo_root, repo)?;
        let base_for_gh = base.strip_prefix("origin/").unwrap_or(&base);

        ui::detail(&format!(
            "Base: {} → {}",
            ui::highlight(&branch),
            ui::highlight(base_for_gh)
        ));

        // 3. Push
        ui::detail("Pushing...");
        let push = Command::new("git")
            .args(["push", "-u", "origin", &branch])
            .output()
            .map_err(|e| PrError::PushFailed(e.to_string()))?;

        if !push.status.success() {
            let stderr = String::from_utf8_lossy(&push.stderr);
            return Err(PrError::PushFailed(stderr.to_string()));
        }

        // 4. Create PR
        let mut gh_args = vec![
            "pr".to_string(),
            "create".to_string(),
            "--draft".to_string(),
            "--base".to_string(),
            base_for_gh.to_string(),
        ];

        if let Some(ref title) = self.title {
            gh_args.push("--title".to_string());
            gh_args.push(title.clone());
        }

        if let Some(ref body) = self.body {
            gh_args.push("--body".to_string());
            gh_args.push(body.clone());
        }

        // Use --fill for any fields not explicitly provided
        if self.title.is_none() {
            gh_args.push("--fill".to_string());
        }

        let gh = Command::new("gh")
            .args(&gh_args)
            .output()
            .map_err(|e| PrError::GhFailed(e.to_string()))?;

        if !gh.status.success() {
            let stderr = String::from_utf8_lossy(&gh.stderr);
            return Err(PrError::GhFailed(stderr.to_string()));
        }

        let url = String::from_utf8_lossy(&gh.stdout).trim().to_string();
        ui::success(&format!("Draft PR created: {}", ui::highlight(&url)));

        Ok(PrOutput(url))
    }
}

/// Resolve the PR base branch.
///
/// If running inside a jig worktree with an associated issue that has a parent,
/// use the parent issue's branch name. Otherwise fall back to the repo base branch.
fn resolve_base(
    worktrees_dir: &std::path::Path,
    repo_root: &std::path::Path,
    repo: &jig_core::RepoContext,
) -> Result<String, PrError> {
    // Try to detect if we're in a jig worktree
    let worktree_name = match git::get_current_worktree_name(worktrees_dir)? {
        Some(name) => name,
        None => return Ok(repo.base_branch.clone()),
    };

    // Load orchestrator state and find our worker
    let state = match OrchestratorState::load(repo_root)? {
        Some(s) => s,
        None => return Ok(repo.base_branch.clone()),
    };

    let worker = match state.get_worker_by_name(&worktree_name) {
        Some(w) => w,
        None => return Ok(repo.base_branch.clone()),
    };

    // Get issue ref from the worker's task
    let issue_ref = match worker.task.as_ref().and_then(|t| t.issue_ref.as_ref()) {
        Some(r) => r.clone(),
        None => return Ok(repo.base_branch.clone()),
    };

    // Fetch the issue and check for a parent
    let provider = repo.issue_provider()?;
    let issue = match provider.get(&issue_ref)? {
        Some(i) => i,
        None => return Ok(repo.base_branch.clone()),
    };

    // If the issue has a parent, fetch the parent to get its branch name
    if let Some(parent) = &issue.parent {
        if let Ok(Some(parent_issue)) = provider.get(&parent.id) {
            if let Some(parent_branch) = &parent_issue.branch_name {
                return Ok(parent_branch.clone());
            }
        }
    }

    Ok(repo.base_branch.clone())
}
