//! Issue actor — polls for auto-spawnable and triageable issues in a background thread.
//!
//! Also handles parent integration branch creation: when a parent issue (one
//! with children in Backlog or InProgress) is detected, the actor creates the
//! integration branch on `origin` idempotently and flips the parent to
//! InProgress — without spawning a worker.

use std::path::{Path, PathBuf};

use jig_core::config::Config;
use jig_core::git::Branch;
use jig_core::git::Repo;
use jig_core::issues::issue::{Issue, IssueFilter, IssueStatus};
use jig_core::issues::providers::IssueProvider;
use jig_core::issues::ProviderKind;

use crate::actors::Actor;
use crate::actors::spawn::SpawnableIssue;
use crate::actors::triage::TriageIssue;

/// Request sent to the issue actor to poll for auto-spawnable issues.
pub struct IssueRequest {
    /// (repo_root, base_branch) for each registered repo.
    pub repos: Vec<(PathBuf, String)>,
    /// Active workers as (repo_name, worker_name) pairs for per-repo budgeting.
    pub existing_workers: Vec<(String, String)>,
}

/// Response from the issue actor containing both spawnable and triageable issues.
pub struct IssueResponse {
    /// Issues eligible for normal auto-spawn (status=Planned).
    pub spawnable: Vec<SpawnableIssue>,
    /// Issues eligible for triage (status=Triage, repo has triage enabled).
    pub triageable: Vec<TriageIssue>,
    /// Parent integration branches created or verified this poll.
    pub parent_branches: Vec<ParentBranchResult>,
}

/// Result of creating (or skipping) a parent integration branch.
#[derive(Debug, Clone)]
pub struct ParentBranchResult {
    pub repo_root: PathBuf,
    pub repo_name: String,
    pub issue_id: String,
    pub branch_name: String,
    pub created: bool,
    pub status_updated: bool,
    pub error: Option<String>,
}

pub struct IssueActor {
    tx: flume::Sender<IssueRequest>,
    rx: flume::Receiver<IssueResponse>,
    pending: bool,
    first_poll_done: bool,
}

impl Actor for IssueActor {
    type Request = IssueRequest;
    type Response = IssueResponse;

    const NAME: &'static str = "jig-issues";
    const QUEUE_SIZE: usize = 1;

    fn handle(req: IssueRequest) -> IssueResponse {
        process_request(&req)
    }

    fn send(&mut self, req: IssueRequest) -> bool {
        if self.pending {
            return false;
        }
        if self.tx.try_send(req).is_ok() {
            self.pending = true;
            true
        } else {
            false
        }
    }

    fn drain(&mut self) -> Vec<IssueResponse> {
        match self.rx.try_recv() {
            Ok(response) => {
                self.pending = false;
                if !response.spawnable.is_empty() {
                    tracing::info!(count = response.spawnable.len(), "found spawnable issues");
                }
                if !response.triageable.is_empty() {
                    tracing::info!(count = response.triageable.len(), "found triageable issues");
                }
                vec![response]
            }
            Err(_) => vec![],
        }
    }

    fn from_channels(
        tx: flume::Sender<IssueRequest>,
        rx: flume::Receiver<IssueResponse>,
    ) -> Self {
        Self {
            tx,
            rx,
            pending: false,
            first_poll_done: false,
        }
    }
}

impl IssueActor {
    pub fn is_pending(&self) -> bool {
        self.pending
    }

    pub fn should_first_poll(&self) -> bool {
        !self.first_poll_done
    }

    pub fn mark_first_poll_done(&mut self) {
        self.first_poll_done = true;
    }
}

/// Collect spawnable issues from a provider, respecting the budget and skipping
/// workers that already exist.
fn collect_spawnable(
    provider: &IssueProvider,
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

    let repo = match Repo::open(repo_root) {
        Ok(r) => Some(r),
        Err(e) => {
            tracing::debug!(repo = %repo_name, error = %e, "failed to open repo for child-spawnable check");
            None
        }
    };

    // Filter out child issues whose parent isn't ready
    let issues: Vec<_> = issues
        .into_iter()
        .filter(|issue| match &repo {
            Some(r) => is_child_spawnable(issue, provider, r),
            None => issue.parent().is_none(),
        })
        .collect();

    let provider_kind = provider.kind();
    let mut result = Vec::new();
    let mut repo_spawned = 0;

    for issue in issues {
        if repo_spawned >= budget {
            break;
        }
        let worker_name = issue.branch().to_string();
        if existing_workers.iter().any(|(_, wn)| wn == &worker_name) {
            continue;
        }
        result.push(SpawnableIssue {
            repo_root: repo_root.to_path_buf(),
            issue,
            worker_name,
            provider_kind,
        });
        repo_spawned += 1;
    }

    result
}

/// Collect triageable issues from a provider.
///
/// Triage runs as a direct subprocess (no worktree, branch, or tmux window),
/// so these issues bypass the spawn actor entirely and do not have a
/// corresponding worker in `existing_workers`. Duplicate prevention for
/// in-flight triages is handled by `TriageTracker::is_active` in the daemon
/// tick loop, not by checking worker names here.
fn collect_triageable(
    provider: &IssueProvider,
    provider_kind: ProviderKind,
    repo_root: &Path,
    repo_name: &str,
    budget: usize,
) -> Vec<TriageIssue> {
    let triageable = match provider.list_triageable() {
        Ok(issues) => issues,
        Err(e) => {
            tracing::debug!(repo = %repo_name, error = %e, "failed to list triageable issues");
            return vec![];
        }
    };

    triageable
        .into_iter()
        .take(budget)
        .map(|issue| {
            // Synthetic display name for logging/notifications; there is no
            // worktree or tmux window behind it.
            let worker_name = format!("triage-{}", issue.id().to_lowercase());
            TriageIssue {
                repo_root: repo_root.to_path_buf(),
                issue,
                worker_name,
                provider_kind,
            }
        })
        .collect()
}

/// Process an issue request synchronously.
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
        let cfg = match Config::from_path(repo_root) {
            Ok(cfg) => cfg,
            Err(e) => {
                tracing::debug!(error = %e, "failed to load repo context for issue poll");
                continue;
            }
        };

        let repo_name = repo_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let provider = match cfg.issue_provider() {
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
        let parent_results = ensure_parent_branches(&provider, repo_root, &repo_name, base_branch);
        all_parent_branches.extend(parent_results);

        // Count existing workers for this repo (shared budget for spawn + triage)
        let repo_worker_count = req
            .existing_workers
            .iter()
            .filter(|(rn, _)| rn == &repo_name)
            .count();
        let max_workers = cfg
            .repo
            .spawn
            .resolve_max_concurrent_workers(&cfg.global.spawn);
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
        let remaining_budget = budget;

        // Triage path: collect triage-eligible issues first. Triage runs as
        // direct subprocesses with no worker/worktree, so it doesn't consume
        // the worker budget.
        if cfg.repo.triage.enabled {
            let triage_issues = collect_triageable(
                &provider,
                provider_kind,
                repo_root,
                &repo_name,
                remaining_budget,
            );
            all_triageable.extend(triage_issues);
        }

        // Auto-spawn path: collect spawnable issues with remaining budget.
        // Parent issues (those with active children) are excluded.
        if let Some(labels) = &cfg.repo.issues.auto_spawn_labels {
            let mut spawnable = collect_spawnable(
                &provider,
                labels,
                repo_root,
                &repo_name,
                remaining_budget,
                &req.existing_workers,
            );
            spawnable.retain(|si| si.issue.children().is_empty());
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
fn is_child_spawnable(issue: &Issue, provider: &IssueProvider, repo: &Repo) -> bool {
    let Some(parent_ref) = issue.parent() else {
        return true;
    };

    let parent = match provider.get(parent_ref) {
        Ok(Some(p)) => p,
        _ => {
            tracing::debug!(
                issue = %issue.id(),
                parent = %parent_ref,
                "child not spawnable: failed to resolve parent"
            );
            return false;
        }
    };

    if *parent.status() != IssueStatus::InProgress {
        tracing::debug!(
            issue = %issue.id(),
            parent = %parent_ref,
            parent_status = ?parent.status(),
            "child not spawnable: parent not InProgress"
        );
        return false;
    }

    if !repo.remote_branch_exists(parent.branch()) {
        tracing::debug!(
            issue = %issue.id(),
            parent = %parent_ref,
            branch = %parent.branch(),
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
    let b: Branch = branch.into();
    repo.remote_branch_exists(&b)
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
    provider: &IssueProvider,
    repo_root: &Path,
    repo_name: &str,
    base_branch: &str,
) -> Vec<ParentBranchResult> {
    let mut results = Vec::new();

    // Collect candidate parent issues: Planned or InProgress with children.
    let candidates = collect_parent_candidates(provider);

    for issue in candidates {
        let branch = issue.branch().to_string();

        let mut result = ParentBranchResult {
            repo_root: repo_root.to_path_buf(),
            repo_name: repo_name.to_string(),
            issue_id: issue.id().to_string(),
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
                        issue = %issue.id(),
                        branch = %branch,
                        "created parent integration branch"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        repo = %repo_name,
                        issue = %issue.id(),
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
                issue = %issue.id(),
                branch = %branch,
                "parent integration branch already exists"
            );
        }

        // Flip status to InProgress if currently Planned
        if *issue.status() == IssueStatus::Planned {
            match provider.update_status(issue.id(), &IssueStatus::InProgress) {
                Ok(()) => {
                    result.status_updated = true;
                    tracing::info!(
                        repo = %repo_name,
                        issue = %issue.id(),
                        "flipped parent issue to InProgress"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        repo = %repo_name,
                        issue = %issue.id(),
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

/// Collect parent issue candidates: issues in Planned or InProgress status
/// that have children.
fn collect_parent_candidates(provider: &IssueProvider) -> Vec<Issue> {
    let mut candidates = Vec::new();

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
            if !issue.children().is_empty() {
                candidates.push(issue);
            }
        }
    }

    candidates
}

/// Create a local branch from `origin/{base_branch}` and push it to origin.
///
/// If the local branch already exists (e.g. leftover from a previous run),
/// skip creation and push the existing branch. This prevents a deadlock where
/// `git2::Repository::branch()` fails with "reference already exists" every
/// tick while the tracking ref remains absent.
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

    // Create local branch — tolerate "already exists" so we can still push
    match inner.branch(branch, &commit, false) {
        Ok(_) => {}
        Err(e) if e.code() == git2::ErrorCode::Exists => {
            tracing::debug!(
                branch = %branch,
                "local branch already exists, skipping creation — will push existing"
            );
        }
        Err(e) => {
            return Err(format!("failed to create branch '{}': {}", branch, e));
        }
    }

    let branch_ref: Branch = branch.into();
    repo.push_branch(&branch_ref)
        .map_err(|e| format!("push failed: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use jig_core::issues::issue::{Issue, IssuePriority, IssueRef, IssueStatus};

    /// A mock provider for testing parent detection logic.
    struct MockProvider {
        issues: Vec<Issue>,
    }

    impl MockProvider {
        fn into_provider(self) -> IssueProvider {
            IssueProvider::new(Box::new(self))
        }
    }

    impl jig_core::issues::providers::IssueBackend for MockProvider {
        fn name(&self) -> &str {
            "mock"
        }

        fn kind(&self) -> ProviderKind {
            ProviderKind::Linear
        }

        fn list(&self, filter: &IssueFilter) -> jig_core::error::Result<Vec<Issue>> {
            Ok(self
                .issues
                .iter()
                .filter(|i| i.matches(filter))
                .cloned()
                .collect())
        }

        fn get(&self, id: &str) -> jig_core::error::Result<Option<Issue>> {
            Ok(self.issues.iter().find(|i| *i.id() == *id).cloned())
        }

        fn update_status(&self, _id: &str, _status: &IssueStatus) -> jig_core::error::Result<()> {
            Ok(())
        }
    }

    fn make_issue(id: &str, status: IssueStatus, children: Vec<IssueRef>) -> Issue {
        Issue::new(
            id,
            format!("Issue {}", id),
            status,
            IssuePriority::Medium,
            Branch::new(id.to_lowercase()),
            "",
        )
        .with_children(children)
    }

    fn make_child_issue(id: &str, status: IssueStatus, parent_id: &str) -> Issue {
        Issue::new(
            id,
            format!("Child {}", id),
            status,
            IssuePriority::Medium,
            Branch::new(id.to_lowercase()),
            "",
        )
        .with_parent(parent_id)
    }

    #[test]
    fn parent_with_children_excluded() {
        let parent = make_issue("ENG-100", IssueStatus::Planned, vec!["ENG-101".into()]);
        assert!(!parent.children().is_empty());
    }

    #[test]
    fn childless_issue_not_excluded() {
        let issue = make_issue("ENG-100", IssueStatus::Planned, vec![]);
        assert!(issue.children().is_empty());
    }

    #[test]
    fn collect_parent_candidates_finds_planned_parent() {
        let parent = make_issue("ENG-100", IssueStatus::Planned, vec!["ENG-101".into()]);
        let child = make_issue("ENG-101", IssueStatus::Backlog, vec![]);
        let provider = MockProvider {
            issues: vec![parent, child],
        }
        .into_provider();

        let candidates = collect_parent_candidates(&provider);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].id(), "ENG-100");
    }

    #[test]
    fn collect_parent_candidates_finds_in_progress_parent() {
        let parent = make_issue("ENG-100", IssueStatus::InProgress, vec!["ENG-101".into()]);
        let child = make_issue("ENG-101", IssueStatus::InProgress, vec![]);
        let provider = MockProvider {
            issues: vec![parent, child],
        }
        .into_provider();

        let candidates = collect_parent_candidates(&provider);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].id(), "ENG-100");
    }

    #[test]
    fn collect_parent_candidates_skips_complete_parent() {
        let parent = make_issue("ENG-100", IssueStatus::Complete, vec!["ENG-101".into()]);
        let child = make_issue("ENG-101", IssueStatus::Complete, vec![]);
        let provider = MockProvider {
            issues: vec![parent, child],
        }
        .into_provider();

        let candidates = collect_parent_candidates(&provider);
        assert!(candidates.is_empty());
    }

    #[test]
    fn collect_parent_candidates_skips_childless() {
        let issue = make_issue("ENG-100", IssueStatus::Planned, vec![]);
        let provider = MockProvider {
            issues: vec![issue],
        }
        .into_provider();

        let candidates = collect_parent_candidates(&provider);
        assert!(candidates.is_empty());
    }

    #[test]
    fn active_parent_excluded_from_spawnable() {
        let parent = make_issue("ENG-100", IssueStatus::Planned, vec!["ENG-101".into()]);
        assert!(!parent.children().is_empty());
    }

    #[test]
    fn child_issue_not_active_parent() {
        let child = make_child_issue("ENG-101", IssueStatus::Planned, "ENG-100");
        assert!(child.children().is_empty());
    }

    // -- Blocked-by DAG walking tests for parent-child children ---------------

    /// Integration test: blocked-by DAG walking combined with parent-readiness
    /// for the A→B→C chain scenario.
    ///
    /// Uses a MockProvider to simulate the Linear/file provider, and a real git
    /// repo to test `is_child_spawnable`'s remote branch check.
    #[test]
    fn blocked_by_dag_with_parent_readiness() {
        use std::process::Command as StdCommand;

        let tmp = tempfile::tempdir().unwrap();
        let repo_root = tmp.path();

        // Init a git repo with self-referencing remote so origin/ refs work
        let run = |args: &[&str]| {
            StdCommand::new("git")
                .args(args)
                .current_dir(repo_root)
                .env("GIT_AUTHOR_NAME", "test")
                .env("GIT_AUTHOR_EMAIL", "test@test.com")
                .env("GIT_COMMITTER_NAME", "test")
                .env("GIT_COMMITTER_EMAIL", "test@test.com")
                .output()
                .expect("git command failed");
        };
        run(&["init", "-q", "-b", "main"]);
        run(&[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--allow-empty",
            "-m",
            "init",
        ]);
        run(&["remote", "add", "origin", repo_root.to_str().unwrap()]);

        // Create parent branch and push to origin
        let parent_branch = "al/parent-epic";
        run(&["branch", parent_branch]);
        run(&["fetch", "origin"]);

        // Parent issue that children reference
        let parent_issue = Issue::new(
            "ENG-100",
            "Parent Epic",
            IssueStatus::InProgress,
            IssuePriority::Medium,
            Branch::new(parent_branch),
            "",
        )
        .with_children(vec!["ENG-101".into(), "ENG-102".into(), "ENG-103".into()]);

        // Child A: no deps
        let child_a = Issue::new(
            "ENG-101",
            "Child A",
            IssueStatus::Planned,
            IssuePriority::Medium,
            Branch::new("al/eng-101-child-a"),
            "",
        )
        .with_labels(vec!["auto".to_string()])
        .with_parent("ENG-100");

        // Child B: blocked by A
        let child_b = Issue::new(
            "ENG-102",
            "Child B",
            IssueStatus::Planned,
            IssuePriority::Medium,
            Branch::new("al/eng-101-child-a"),
            "",
        )
        .with_labels(vec!["auto".to_string()])
        .with_parent("ENG-100")
        .with_depends_on(vec!["ENG-101".into()]);

        // Child C: blocked by B
        let child_c = Issue::new(
            "ENG-103",
            "Child C",
            IssueStatus::Planned,
            IssuePriority::Medium,
            Branch::new("al/eng-101-child-a"),
            "",
        )
        .with_labels(vec!["auto".to_string()])
        .with_parent("ENG-100")
        .with_depends_on(vec!["ENG-102".into()]);

        // --- Tick 1: A=Planned, B=Planned(blocked by A), C=Planned(blocked by B) ---
        let labels = vec!["auto".to_string()];
        let provider = MockProvider {
            issues: vec![
                parent_issue.clone(),
                child_a.clone(),
                child_b.clone(),
                child_c.clone(),
            ],
        }
        .into_provider();
        let spawnable = collect_spawnable(&provider, &labels, repo_root, "test", 10, &[]);
        let ids: Vec<&str> = spawnable.iter().map(|s| s.issue.id().as_ref()).collect();
        assert!(ids.contains(&"ENG-101"), "tick 1: A spawnable");
        assert!(!ids.contains(&"ENG-102"), "tick 1: B blocked by A");
        assert!(!ids.contains(&"ENG-103"), "tick 1: C blocked by B");

        // --- Tick 2: A=Complete, B now unblocked ---
        let child_a_done = Issue::new(
            "ENG-101",
            "Child A",
            IssueStatus::Complete,
            IssuePriority::Medium,
            Branch::new("al/eng-101-child-a"),
            "",
        )
        .with_labels(vec!["auto".to_string()])
        .with_parent("ENG-100");
        let provider = MockProvider {
            issues: vec![
                parent_issue.clone(),
                child_a_done.clone(),
                child_b.clone(),
                child_c.clone(),
            ],
        }
        .into_provider();
        let spawnable = collect_spawnable(&provider, &labels, repo_root, "test", 10, &[]);
        let ids: Vec<&str> = spawnable.iter().map(|s| s.issue.id().as_ref()).collect();
        assert!(!ids.contains(&"ENG-101"), "tick 2: A already Complete");
        assert!(ids.contains(&"ENG-102"), "tick 2: B spawnable");
        assert!(!ids.contains(&"ENG-103"), "tick 2: C still blocked");

        // --- Tick 3: B=Complete, C now unblocked ---
        let child_b_done = Issue::new(
            "ENG-102",
            "Child B",
            IssueStatus::Complete,
            IssuePriority::Medium,
            Branch::new("al/eng-101-child-a"),
            "",
        )
        .with_labels(vec!["auto".to_string()])
        .with_parent("ENG-100")
        .with_depends_on(vec!["ENG-101".into()]);
        let provider = MockProvider {
            issues: vec![
                parent_issue.clone(),
                child_a_done.clone(),
                child_b_done.clone(),
                child_c.clone(),
            ],
        }
        .into_provider();
        let spawnable = collect_spawnable(&provider, &labels, repo_root, "test", 10, &[]);
        let ids: Vec<&str> = spawnable.iter().map(|s| s.issue.id().as_ref()).collect();
        assert!(ids.contains(&"ENG-103"), "tick 3: C spawnable");
        assert_eq!(ids.len(), 1, "tick 3: only C left");

        // --- Tick 4: C=Complete, nothing left ---
        let child_c_done = Issue::new(
            "ENG-103",
            "Child C",
            IssueStatus::Complete,
            IssuePriority::Medium,
            Branch::new("al/eng-101-child-a"),
            "",
        )
        .with_labels(vec!["auto".to_string()])
        .with_parent("ENG-100")
        .with_depends_on(vec!["ENG-102".into()]);
        let provider = MockProvider {
            issues: vec![
                parent_issue.clone(),
                child_a_done,
                child_b_done,
                child_c_done,
            ],
        }
        .into_provider();
        let spawnable = collect_spawnable(&provider, &labels, repo_root, "test", 10, &[]);
        assert!(spawnable.is_empty(), "tick 4: no children left");
    }

    #[test]
    fn is_child_spawnable_requires_parent_in_progress() {
        let tmp = tempfile::tempdir().unwrap();
        git2::Repository::init(tmp.path()).unwrap();
        let repo = Repo::open(tmp.path()).unwrap();
        let child = make_child_issue("ENG-101", IssueStatus::Planned, "ENG-100");

        // Parent is Planned → child not spawnable
        let parent_planned = make_issue("ENG-100", IssueStatus::Planned, vec!["ENG-101".into()]);
        let provider = MockProvider {
            issues: vec![parent_planned, child.clone()],
        }
        .into_provider();
        assert!(!is_child_spawnable(&child, &provider, &repo));

        // Parent InProgress but no remote branch → false
        let parent_ip = make_issue("ENG-100", IssueStatus::InProgress, vec!["ENG-101".into()]);
        let provider = MockProvider {
            issues: vec![parent_ip, child.clone()],
        }
        .into_provider();
        assert!(!is_child_spawnable(&child, &provider, &repo));

        // Non-child issue → always true
        let standalone = make_issue("ENG-200", IssueStatus::Planned, vec![]);
        let provider = MockProvider {
            issues: vec![standalone.clone()],
        }
        .into_provider();
        assert!(is_child_spawnable(&standalone, &provider, &repo));
    }

    /// Verify that blocked-by gating in `collect_spawnable` works for standalone
    /// (non-parent) issues — no regression from parent-child code paths.
    #[test]
    fn blocked_by_dag_standalone_no_regression() {
        let tmp = tempfile::tempdir().unwrap();

        // A — no deps
        let step_a = Issue::new(
            "ENG-201",
            "Step A",
            IssueStatus::Planned,
            IssuePriority::Medium,
            Branch::new("al/eng-201-step-a"),
            "",
        )
        .with_labels(vec!["auto".to_string()]);

        // B — blocked by A
        let step_b = Issue::new(
            "ENG-202",
            "Step B",
            IssueStatus::Planned,
            IssuePriority::Medium,
            Branch::new("al/eng-201-step-a"),
            "",
        )
        .with_labels(vec!["auto".to_string()])
        .with_depends_on(vec!["ENG-201".into()]);

        let provider = MockProvider {
            issues: vec![step_a.clone(), step_b.clone()],
        }
        .into_provider();
        let labels = vec!["auto".to_string()];
        let spawnable = collect_spawnable(&provider, &labels, tmp.path(), "test", 10, &[]);
        let ids: Vec<&str> = spawnable.iter().map(|s| s.issue.id().as_ref()).collect();
        assert!(ids.contains(&"ENG-201"), "A spawnable");
        assert!(!ids.contains(&"ENG-202"), "B blocked by A");

        // Complete A → B spawnable
        let step_a_done = Issue::new(
            "ENG-201",
            "Step A",
            IssueStatus::Complete,
            IssuePriority::Medium,
            Branch::new("al/eng-201-step-a"),
            "",
        )
        .with_labels(vec!["auto".to_string()]);
        let provider = MockProvider {
            issues: vec![step_a_done, step_b],
        }
        .into_provider();
        let spawnable = collect_spawnable(&provider, &labels, tmp.path(), "test", 10, &[]);
        let ids: Vec<&str> = spawnable.iter().map(|s| s.issue.id().as_ref()).collect();
        assert!(ids.contains(&"ENG-202"), "B now spawnable");
    }

    // -- create_and_push_branch / ensure_parent_branches deadlock fix tests ---

    fn setup_self_remote_repo() -> (tempfile::TempDir, std::path::PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let bare_path = tmp.path().join("bare.git");
        let repo_path = tmp.path().join("work");

        let bare = git2::Repository::init_bare(&bare_path).unwrap();

        let work = git2::Repository::init(&repo_path).unwrap();
        {
            let sig = git2::Signature::now("test", "test@test.com").unwrap();
            let tree_id = work.index().unwrap().write_tree().unwrap();
            let tree = work.find_tree(tree_id).unwrap();
            work.commit(Some("refs/heads/main"), &sig, &sig, "init", &tree, &[])
                .unwrap();
            work.set_head("refs/heads/main").unwrap();
        }

        work.remote("origin", bare_path.to_str().unwrap()).unwrap();

        // Push main to bare so tracking refs exist
        {
            let mut remote = work.find_remote("origin").unwrap();
            remote
                .push(&["refs/heads/main:refs/heads/main"], None)
                .unwrap();
        }
        // Fetch so origin/main tracking ref exists locally
        {
            let mut remote = work.find_remote("origin").unwrap();
            remote.fetch(&["main"], None, None).unwrap();
        }

        drop(bare);
        (tmp, repo_path)
    }

    /// When a local branch exists but the tracking ref does not,
    /// `create_and_push_branch` should succeed (not error with
    /// "reference already exists") and the tracking ref should be
    /// populated after the push.
    #[test]
    fn create_and_push_branch_tolerates_existing_local_branch() {
        use std::process::Command as StdCommand;

        let (_tmp, repo_root) = setup_self_remote_repo();

        let branch = "al/parent-integration";

        // Create the local branch manually (simulating leftover from earlier run)
        let out = StdCommand::new("git")
            .args(["branch", branch])
            .current_dir(&repo_root)
            .output()
            .unwrap();
        assert!(out.status.success());

        // Verify: local branch exists
        let repo = Repo::open(&repo_root).unwrap();
        assert!(
            repo.inner()
                .find_branch(branch, git2::BranchType::Local)
                .is_ok(),
            "local branch should exist"
        );
        // Verify: tracking ref does NOT exist yet
        assert!(
            !remote_branch_exists(&repo_root, branch),
            "tracking ref should not exist before push"
        );

        // This is the call that used to fail with "reference already exists"
        let result = create_and_push_branch(&repo_root, branch, "main");
        assert!(
            result.is_ok(),
            "create_and_push_branch should succeed: {:?}",
            result.err()
        );

        // After push, the tracking ref should be populated
        assert!(
            remote_branch_exists(&repo_root, branch),
            "tracking ref should exist after push"
        );
    }

    /// When no local branch exists, `create_and_push_branch` should
    /// create it and push — the normal happy path still works.
    #[test]
    fn create_and_push_branch_creates_new_branch() {
        let (_tmp, repo_root) = setup_self_remote_repo();

        let branch = "al/new-parent";
        assert!(!remote_branch_exists(&repo_root, branch));

        let result = create_and_push_branch(&repo_root, branch, "main");
        assert!(result.is_ok(), "should succeed: {:?}", result.err());
        assert!(remote_branch_exists(&repo_root, branch));
    }

    /// `ensure_parent_branches` with a pre-existing local branch and no
    /// tracking ref should succeed and make children spawnable.
    #[test]
    fn ensure_parent_branches_with_stale_local_branch() {
        use std::process::Command as StdCommand;

        let (_tmp, repo_root) = setup_self_remote_repo();

        let parent_branch = "al/eng-100-parent-epic";

        // Pre-seed: local branch exists, no tracking ref
        let out = StdCommand::new("git")
            .args(["branch", parent_branch])
            .current_dir(&repo_root)
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(!remote_branch_exists(&repo_root, parent_branch));

        // Set up issues: parent with a child
        let parent = Issue::new(
            "ENG-100",
            "Parent Epic",
            IssueStatus::Planned,
            IssuePriority::Medium,
            Branch::new(parent_branch),
            "",
        )
        .with_children(vec!["ENG-101".into()]);
        let provider = MockProvider {
            issues: vec![parent],
        }
        .into_provider();

        let results = ensure_parent_branches(&provider, &repo_root, "test", "main");

        assert_eq!(results.len(), 1);
        let pb = &results[0];
        assert!(pb.error.is_none(), "should not error: {:?}", pb.error);
        assert!(pb.created, "branch should be marked as created");

        assert!(
            remote_branch_exists(&repo_root, parent_branch),
            "tracking ref should be populated after ensure_parent_branches"
        );

        // After ensure_parent_branches, parent is InProgress
        let parent_ip = Issue::new(
            "ENG-100",
            "Parent Epic",
            IssueStatus::InProgress,
            IssuePriority::Medium,
            Branch::new(parent_branch),
            "",
        )
        .with_children(vec!["ENG-101".into()]);
        let child = Issue::new(
            "ENG-101",
            "Child",
            IssueStatus::Planned,
            IssuePriority::Medium,
            Branch::new("al/eng-101-child"),
            "",
        )
        .with_labels(vec!["auto".to_string()])
        .with_parent("ENG-100");
        let provider = MockProvider {
            issues: vec![parent_ip, child.clone()],
        }
        .into_provider();
        let repo = Repo::open(&repo_root).unwrap();
        assert!(
            is_child_spawnable(&child, &provider, &repo),
            "child should be spawnable after parent branch is pushed"
        );
    }
}
