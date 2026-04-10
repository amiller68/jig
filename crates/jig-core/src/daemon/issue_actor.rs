//! Issue actor — polls for auto-spawnable and triageable issues in a background thread.
//!
//! Also handles parent integration branch creation: when a parent issue (one
//! with children in Backlog or InProgress) is detected, the actor creates the
//! integration branch on `origin` idempotently and flips the parent to
//! InProgress — without spawning a worker.

use std::path::Path;
use std::process::Command;

use crate::context::RepoContext;
use crate::git::Repo;
use crate::issues::naming::derive_worker_name;
use crate::issues::provider::IssueProvider;
use crate::issues::types::{Issue, IssueFilter, IssueStatus};
use crate::issues::ProviderKind;
use crate::spawn::SpawnKind;

use super::messages::{IssueRequest, IssueResponse, ParentBranchResult, SpawnableIssue};

/// Spawn the issue actor thread. Returns immediately.
pub fn spawn(
    rx: flume::Receiver<IssueRequest>,
    tx: flume::Sender<IssueResponse>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("jig-issues".into())
        .spawn(move || {
            while let Ok(req) = rx.recv() {
                let result = process_request(&req);
                if tx.send(result).is_err() {
                    break;
                }
            }
        })
        .expect("failed to spawn issue actor thread")
}

/// Collect spawnable issues from a provider, respecting the budget and skipping
/// workers that already exist.
fn collect_spawnable(
    provider: &dyn IssueProvider,
    labels: &[String],
    repo_root: &Path,
    repo_name: &str,
    budget: usize,
    existing_workers: &[(String, String)],
) -> Vec<SpawnableIssue> {
    let issues = match provider.list_spawnable(labels) {
        Ok(issues) => issues,
        Err(e) => {
            tracing::debug!(repo = %repo_name, error = %e, "failed to list spawnable issues");
            return vec![];
        }
    };

    // Filter out child issues whose parent isn't ready
    let issues: Vec<_> = issues
        .into_iter()
        .filter(|issue| is_child_spawnable(issue, repo_root))
        .collect();

    let provider_kind = provider.kind();
    let mut result = Vec::new();
    let mut repo_spawned = 0;

    for issue in issues {
        if repo_spawned >= budget {
            break;
        }
        let worker_name = derive_worker_name(&issue.id, issue.branch_name.as_deref());
        if existing_workers.iter().any(|(_, wn)| wn == &worker_name) {
            continue;
        }
        result.push(SpawnableIssue {
            repo_root: repo_root.to_path_buf(),
            issue,
            worker_name,
            provider_kind,
            kind: SpawnKind::Normal,
        });
        repo_spawned += 1;
    }

    result
}

/// Collect triageable issues from a provider, skipping workers that already exist.
fn collect_triageable(
    provider: &dyn IssueProvider,
    provider_kind: ProviderKind,
    repo_root: &Path,
    repo_name: &str,
    budget: usize,
    existing_workers: &[(String, String)],
) -> Vec<SpawnableIssue> {
    let triageable = match provider.list_triageable() {
        Ok(issues) => issues,
        Err(e) => {
            tracing::debug!(repo = %repo_name, error = %e, "failed to list triageable issues");
            return vec![];
        }
    };

    let mut result = Vec::new();
    let mut count = 0;

    for issue in triageable {
        if count >= budget {
            break;
        }
        let worker_name = format!("triage-{}", issue.id.to_lowercase());
        if existing_workers.iter().any(|(_, wn)| wn == &worker_name) {
            continue;
        }
        result.push(SpawnableIssue {
            repo_root: repo_root.to_path_buf(),
            issue,
            worker_name,
            provider_kind,
            kind: SpawnKind::Triage,
        });
        count += 1;
    }

    result
}

/// Process an issue request synchronously. Used by both the actor thread and
/// the blocking `tick_once` path.
///
/// Each repo is checked independently: its own `jig.toml` controls whether
/// auto-spawn is enabled and the per-repo worker budget. Triage-eligible
/// issues (status=Triage, repo has `[triage] enabled = true`) are returned
/// separately from normal spawnable issues. Both triage and spawn share
/// the worker budget.
pub(crate) fn process_request(req: &IssueRequest) -> IssueResponse {
    let mut all_spawnable = Vec::new();
    let mut all_triageable = Vec::new();
    let mut all_parent_branches = Vec::new();

    for (repo_root, base_branch) in &req.repos {
        let ctx = match RepoContext::from_path(repo_root) {
            Ok(ctx) => ctx,
            Err(e) => {
                tracing::debug!(error = %e, "failed to load repo context for issue poll");
                continue;
            }
        };

        let repo_name = repo_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let provider = match ctx.issue_provider_with_ref(base_branch) {
            Ok(p) => p,
            Err(e) => {
                tracing::debug!(repo = %repo_name, error = %e, "failed to create issue provider");
                continue;
            }
        };

        // Parent integration branch path: detect parent issues and create
        // their integration branches on origin idempotently. This runs
        // before spawn collection so that children can see the parent
        // branch on the next tick.
        let parent_results =
            ensure_parent_branches(provider.as_ref(), repo_root, &repo_name, base_branch);
        all_parent_branches.extend(parent_results);

        // Count existing workers for this repo (shared budget for spawn + triage)
        let repo_worker_count = req
            .existing_workers
            .iter()
            .filter(|(rn, _)| rn == &repo_name)
            .count();
        let max_workers = ctx
            .jig_toml
            .spawn
            .resolve_max_concurrent_workers(&ctx.global_config.spawn);
        let budget = max_workers.saturating_sub(repo_worker_count);

        if budget == 0 {
            tracing::debug!(
                repo = %repo_name,
                active = repo_worker_count,
                max = max_workers,
                "repo at worker capacity, skipping"
            );
            continue;
        }

        let provider_kind = provider.kind();
        let mut remaining_budget = budget;

        // Triage path: collect triage-eligible issues first (they share the budget)
        if ctx.jig_toml.triage.enabled {
            let triage_issues = collect_triageable(
                provider.as_ref(),
                provider_kind,
                repo_root,
                &repo_name,
                remaining_budget,
                &req.existing_workers,
            );
            remaining_budget = remaining_budget.saturating_sub(triage_issues.len());
            all_triageable.extend(triage_issues);
        }

        // Auto-spawn path: collect spawnable issues with remaining budget.
        // Parent issues (those with active children) are excluded — they
        // should not get a worker until all children are complete (wrap-up).
        if let Some(labels) = &ctx.jig_toml.issues.auto_spawn_labels {
            let mut spawnable = collect_spawnable(
                provider.as_ref(),
                labels,
                repo_root,
                &repo_name,
                remaining_budget,
                &req.existing_workers,
            );
            spawnable.retain(|si| !is_active_parent(&si.issue, provider.as_ref()));
            all_spawnable.extend(spawnable);
        }
    }

    IssueResponse {
        spawnable: all_spawnable,
        triageable: all_triageable,
        parent_branches: all_parent_branches,
    }
}

/// Returns `true` if the issue is spawnable with respect to its parent.
///
/// Non-child issues always pass. Child issues require their parent to be
/// `InProgress` and to have pushed a branch to the remote.
fn is_child_spawnable(issue: &Issue, repo_root: &Path) -> bool {
    let Some(parent) = &issue.parent else {
        return true;
    };

    // Parent must be InProgress
    if parent.status.as_ref() != Some(&IssueStatus::InProgress) {
        tracing::debug!(
            issue = %issue.id,
            parent = %parent.id,
            parent_status = ?parent.status,
            "child not spawnable: parent not InProgress"
        );
        return false;
    }

    // Parent must have a branch that exists on the remote
    let Some(branch) = &parent.branch_name else {
        tracing::debug!(
            issue = %issue.id,
            parent = %parent.id,
            "child not spawnable: parent has no branch name"
        );
        return false;
    };

    if !remote_branch_exists(repo_root, branch) {
        tracing::debug!(
            issue = %issue.id,
            parent = %parent.id,
            branch = %branch,
            "child not spawnable: parent branch not on remote"
        );
        return false;
    }

    true
}

/// Checks whether a branch exists on the `origin` remote using git2.
fn remote_branch_exists(repo_root: &Path, branch: &str) -> bool {
    let Ok(repo) = Repo::open(repo_root) else {
        return false;
    };
    let result = repo
        .inner()
        .find_branch(&format!("origin/{branch}"), git2::BranchType::Remote);
    result.is_ok()
}

/// Returns `true` if the issue is an active parent — i.e. it has ≥1 child in
/// Backlog or InProgress status. Active parents are excluded from normal
/// auto-spawn because the daemon owns their integration branch.
fn is_active_parent(issue: &Issue, provider: &dyn IssueProvider) -> bool {
    if issue.children.is_empty() {
        return false;
    }
    issue.children.iter().any(|child_id| {
        matches!(
            provider.get(child_id),
            Ok(Some(child)) if matches!(child.status, IssueStatus::Backlog | IssueStatus::InProgress | IssueStatus::Planned)
        )
    })
}

/// Ensure parent integration branches exist on origin for all eligible parent
/// issues. A parent issue is one with ≥1 child in Backlog or InProgress, and
/// whose own status is Planned (Todo) or InProgress.
///
/// For each such parent:
/// 1. Derive the branch name from the issue.
/// 2. If the branch doesn't exist on `origin`, create it from `origin/{base_branch}` and push.
/// 3. Flip the parent issue to InProgress if not already.
/// 4. Do NOT spawn a worker.
fn ensure_parent_branches(
    provider: &dyn IssueProvider,
    repo_root: &Path,
    repo_name: &str,
    base_branch: &str,
) -> Vec<ParentBranchResult> {
    let mut results = Vec::new();

    // Collect candidate parent issues: Planned or InProgress with children.
    let candidates = collect_parent_candidates(provider);

    for issue in candidates {
        let branch = derive_worker_name(&issue.id, issue.branch_name.as_deref());
        if branch.is_empty() {
            continue;
        }

        let mut result = ParentBranchResult {
            repo_root: repo_root.to_path_buf(),
            repo_name: repo_name.to_string(),
            issue_id: issue.id.clone(),
            branch_name: branch.clone(),
            created: false,
            status_updated: false,
            error: None,
        };

        // Check if branch already exists on remote
        let branch_exists = remote_branch_exists(repo_root, &branch);

        if !branch_exists {
            // Create the branch from origin/{base_branch} and push it
            match create_and_push_branch(repo_root, &branch, base_branch) {
                Ok(()) => {
                    result.created = true;
                    tracing::info!(
                        repo = %repo_name,
                        issue = %issue.id,
                        branch = %branch,
                        "created parent integration branch"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        repo = %repo_name,
                        issue = %issue.id,
                        branch = %branch,
                        "failed to create parent integration branch: {}", e
                    );
                    result.error = Some(e);
                    results.push(result);
                    continue;
                }
            }
        } else {
            tracing::debug!(
                repo = %repo_name,
                issue = %issue.id,
                branch = %branch,
                "parent integration branch already exists"
            );
        }

        // Flip status to InProgress if currently Planned
        if issue.status == IssueStatus::Planned {
            match provider.update_status(&issue.id, &IssueStatus::InProgress) {
                Ok(()) => {
                    result.status_updated = true;
                    tracing::info!(
                        repo = %repo_name,
                        issue = %issue.id,
                        "flipped parent issue to InProgress"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        repo = %repo_name,
                        issue = %issue.id,
                        "failed to update parent status: {}", e
                    );
                    result.error = Some(format!("status update failed: {}", e));
                }
            }
        }

        results.push(result);
    }

    results
}

/// Collect parent issue candidates: issues in Planned or InProgress status that
/// have ≥1 child in Backlog or InProgress.
fn collect_parent_candidates(provider: &dyn IssueProvider) -> Vec<Issue> {
    let mut candidates = Vec::new();

    // Query Planned issues (Todo maps to Planned in jig's model)
    for status in [IssueStatus::Planned, IssueStatus::InProgress] {
        let issues = match provider.list(&IssueFilter {
            status: Some(status),
            ..Default::default()
        }) {
            Ok(issues) => issues,
            Err(e) => {
                tracing::debug!(error = %e, "failed to list issues for parent detection");
                continue;
            }
        };

        for issue in issues {
            if issue.children.is_empty() {
                continue;
            }
            // Check if ≥1 child is in Backlog or InProgress (active child)
            let has_active_child = issue.children.iter().any(|child_id| {
                matches!(
                    provider.get(child_id),
                    Ok(Some(child)) if matches!(
                        child.status,
                        IssueStatus::Backlog | IssueStatus::InProgress | IssueStatus::Planned
                    )
                )
            });
            if has_active_child {
                candidates.push(issue);
            }
        }
    }

    candidates
}

/// Create a local branch from `origin/{base_branch}` and push it to origin.
fn create_and_push_branch(repo_root: &Path, branch: &str, base_branch: &str) -> Result<(), String> {
    let repo = Repo::open(repo_root).map_err(|e| format!("failed to open repo: {}", e))?;
    let inner = repo.inner();

    // Resolve the start point: origin/{base_branch}
    let base_ref = base_branch.strip_prefix("origin/").unwrap_or(base_branch);
    let remote_ref = format!("origin/{}", base_ref);
    let reference = inner
        .find_branch(&remote_ref, git2::BranchType::Remote)
        .map_err(|e| format!("failed to find {}: {}", remote_ref, e))?;
    let commit = reference
        .get()
        .peel_to_commit()
        .map_err(|e| format!("failed to peel to commit: {}", e))?;

    // Create local branch
    inner
        .branch(branch, &commit, false)
        .map_err(|e| format!("failed to create branch '{}': {}", branch, e))?;

    // Push to origin via subprocess (git2 push requires credential setup)
    let output = Command::new("git")
        .args(["push", "origin", &format!("{branch}:{branch}")])
        .current_dir(repo_root)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("failed to run git push: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("git push failed: {}", stderr));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::issues::types::{Issue, IssueStatus, ParentIssue};

    /// A mock provider for testing parent detection logic.
    struct MockProvider {
        issues: Vec<Issue>,
    }

    impl MockProvider {
        fn new(issues: Vec<Issue>) -> Self {
            Self { issues }
        }
    }

    impl IssueProvider for MockProvider {
        fn name(&self) -> &str {
            "mock"
        }

        fn kind(&self) -> ProviderKind {
            ProviderKind::File
        }

        fn list(&self, filter: &IssueFilter) -> crate::error::Result<Vec<Issue>> {
            Ok(self
                .issues
                .iter()
                .filter(|i| i.matches(filter))
                .cloned()
                .collect())
        }

        fn get(&self, id: &str) -> crate::error::Result<Option<Issue>> {
            Ok(self.issues.iter().find(|i| i.id == id).cloned())
        }

        fn update_status(&self, _id: &str, _status: &IssueStatus) -> crate::error::Result<()> {
            Ok(())
        }
    }

    fn make_issue(id: &str, status: IssueStatus, children: Vec<String>) -> Issue {
        Issue {
            id: id.to_string(),
            title: format!("Issue {}", id),
            status,
            priority: None,
            category: None,
            depends_on: vec![],
            body: String::new(),
            source: String::new(),
            children,
            labels: vec![],
            branch_name: None,
            parent: None,
        }
    }

    fn make_child_issue(id: &str, status: IssueStatus, parent_id: &str) -> Issue {
        Issue {
            id: id.to_string(),
            title: format!("Child {}", id),
            status,
            priority: None,
            category: None,
            depends_on: vec![],
            body: String::new(),
            source: String::new(),
            children: vec![],
            labels: vec![],
            branch_name: None,
            parent: Some(ParentIssue {
                id: parent_id.to_string(),
                title: format!("Parent {}", parent_id),
                branch_name: None,
                status: Some(IssueStatus::InProgress),
                body: None,
            }),
        }
    }

    #[test]
    fn is_active_parent_with_backlog_child() {
        let parent = make_issue("ENG-100", IssueStatus::Planned, vec!["ENG-101".to_string()]);
        let child = make_issue("ENG-101", IssueStatus::Backlog, vec![]);
        let provider = MockProvider::new(vec![parent.clone(), child]);

        assert!(is_active_parent(&parent, &provider));
    }

    #[test]
    fn is_active_parent_with_in_progress_child() {
        let parent = make_issue("ENG-100", IssueStatus::Planned, vec!["ENG-101".to_string()]);
        let child = make_issue("ENG-101", IssueStatus::InProgress, vec![]);
        let provider = MockProvider::new(vec![parent.clone(), child]);

        assert!(is_active_parent(&parent, &provider));
    }

    #[test]
    fn is_not_active_parent_all_children_complete() {
        let parent = make_issue("ENG-100", IssueStatus::Planned, vec!["ENG-101".to_string()]);
        let child = make_issue("ENG-101", IssueStatus::Complete, vec![]);
        let provider = MockProvider::new(vec![parent.clone(), child]);

        assert!(!is_active_parent(&parent, &provider));
    }

    #[test]
    fn is_not_active_parent_no_children() {
        let issue = make_issue("ENG-100", IssueStatus::Planned, vec![]);
        let provider = MockProvider::new(vec![issue.clone()]);

        assert!(!is_active_parent(&issue, &provider));
    }

    #[test]
    fn collect_parent_candidates_finds_planned_parent() {
        let parent = make_issue("ENG-100", IssueStatus::Planned, vec!["ENG-101".to_string()]);
        let child = make_issue("ENG-101", IssueStatus::Backlog, vec![]);
        let provider = MockProvider::new(vec![parent, child]);

        let candidates = collect_parent_candidates(&provider);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].id, "ENG-100");
    }

    #[test]
    fn collect_parent_candidates_finds_in_progress_parent() {
        let parent = make_issue(
            "ENG-100",
            IssueStatus::InProgress,
            vec!["ENG-101".to_string()],
        );
        let child = make_issue("ENG-101", IssueStatus::InProgress, vec![]);
        let provider = MockProvider::new(vec![parent, child]);

        let candidates = collect_parent_candidates(&provider);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].id, "ENG-100");
    }

    #[test]
    fn collect_parent_candidates_skips_complete_parent() {
        let parent = make_issue(
            "ENG-100",
            IssueStatus::Complete,
            vec!["ENG-101".to_string()],
        );
        let child = make_issue("ENG-101", IssueStatus::Complete, vec![]);
        let provider = MockProvider::new(vec![parent, child]);

        let candidates = collect_parent_candidates(&provider);
        assert!(candidates.is_empty());
    }

    #[test]
    fn collect_parent_candidates_skips_no_active_children() {
        let parent = make_issue("ENG-100", IssueStatus::Planned, vec!["ENG-101".to_string()]);
        let child = make_issue("ENG-101", IssueStatus::Complete, vec![]);
        let provider = MockProvider::new(vec![parent, child]);

        let candidates = collect_parent_candidates(&provider);
        assert!(candidates.is_empty());
    }

    #[test]
    fn active_parent_excluded_from_spawnable() {
        // A parent with an active child should be detected as an active parent
        let parent = make_issue("ENG-100", IssueStatus::Planned, vec!["ENG-101".to_string()]);
        let child = make_issue("ENG-101", IssueStatus::Backlog, vec![]);
        let provider = MockProvider::new(vec![parent.clone(), child]);

        // The parent should be filtered out from auto-spawn
        assert!(is_active_parent(&parent, &provider));
    }

    #[test]
    fn child_issue_not_active_parent() {
        // A child issue (with a parent reference but no children) should not be
        // treated as an active parent.
        let child = make_child_issue("ENG-101", IssueStatus::Planned, "ENG-100");
        let provider = MockProvider::new(vec![child.clone()]);

        assert!(!is_active_parent(&child, &provider));
    }
}
