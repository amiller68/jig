//! Issue actor — polls for auto-spawnable and triageable issues in a background thread.

use std::path::Path;

use crate::context::RepoContext;
use crate::git::Repo;
use crate::issues::naming::derive_worker_name;
use crate::issues::provider::IssueProvider;
use crate::issues::types::{Issue, IssueStatus};
use crate::issues::ProviderKind;
use crate::spawn::SpawnKind;

use super::messages::{IssueRequest, IssueResponse, SpawnableIssue};

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
    let mut all_wrapup = Vec::new();

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

        let provider = match ctx.issue_provider_with_ref(base_branch) {
            Ok(p) => p,
            Err(e) => {
                tracing::debug!(repo = %repo_name, error = %e, "failed to create issue provider");
                continue;
            }
        };

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

        // Auto-spawn path: collect spawnable issues with remaining budget
        if let Some(labels) = &ctx.jig_toml.issues.auto_spawn_labels {
            let spawnable = collect_spawnable(
                provider.as_ref(),
                labels,
                repo_root,
                &repo_name,
                remaining_budget,
                &req.existing_workers,
            );
            remaining_budget = remaining_budget.saturating_sub(spawnable.len());
            all_spawnable.extend(spawnable);
        }

        // Wrap-up path: check for parent issues ready for wrap-up
        let wrapup = collect_wrapup_parents(
            provider.as_ref(),
            provider_kind,
            repo_root,
            &repo_name,
            remaining_budget,
            &req.existing_workers,
        );
        all_wrapup.extend(wrapup);
    }

    IssueResponse {
        spawnable: all_spawnable,
        triageable: all_triageable,
        wrapup: all_wrapup,
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

/// Collect parent issues that are ready for wrap-up spawning.
///
/// A parent is ready when all its children are Complete and all child branches
/// are merged (reachable from) the parent branch.
fn collect_wrapup_parents(
    provider: &dyn IssueProvider,
    provider_kind: ProviderKind,
    repo_root: &Path,
    repo_name: &str,
    budget: usize,
    existing_workers: &[(String, String)],
) -> Vec<SpawnableIssue> {
    let issues = match provider.list(&crate::issues::types::IssueFilter {
        status: Some(IssueStatus::InProgress),
        ..Default::default()
    }) {
        Ok(issues) => issues,
        Err(e) => {
            tracing::debug!(repo = %repo_name, error = %e, "failed to list issues for wrapup check");
            return vec![];
        }
    };

    let mut result = Vec::new();
    let mut count = 0;

    for issue in issues {
        if count >= budget {
            break;
        }
        // Only consider issues with children (parent issues)
        if issue.children.is_empty() {
            continue;
        }
        // Skip if a worker already exists for this parent
        let worker_name = derive_worker_name(&issue.id, issue.branch_name.as_deref());
        if existing_workers.iter().any(|(_, wn)| wn == &worker_name) {
            continue;
        }
        // Check readiness
        if is_parent_ready_for_wrapup(&issue, provider, repo_root) {
            tracing::info!(
                repo = %repo_name,
                issue = %issue.id,
                "parent issue ready for wrap-up spawn"
            );
            result.push(SpawnableIssue {
                repo_root: repo_root.to_path_buf(),
                issue,
                worker_name,
                provider_kind,
                kind: SpawnKind::Wrapup,
            });
            count += 1;
        }
    }

    result
}

/// Check if a parent issue is ready for wrap-up spawning.
///
/// Returns true iff:
/// 1. All children are in status Complete (fetched from the provider).
/// 2. Every child branch tip is reachable from the parent branch tip
///    (i.e., child work is merged into the parent integration branch).
pub fn is_parent_ready_for_wrapup(
    parent: &Issue,
    provider: &dyn IssueProvider,
    repo_root: &Path,
) -> bool {
    // Must have a branch name
    let Some(parent_branch) = &parent.branch_name else {
        tracing::debug!(
            issue = %parent.id,
            "parent not ready for wrapup: no branch name"
        );
        return false;
    };

    // Fetch each child issue and check status + collect branch names
    let mut child_branches: Vec<(String, String)> = Vec::new();

    for child_id in &parent.children {
        let child = match provider.get(child_id) {
            Ok(Some(child)) => child,
            Ok(None) => {
                tracing::debug!(
                    parent = %parent.id,
                    child = %child_id,
                    "parent not ready for wrapup: child not found"
                );
                return false;
            }
            Err(e) => {
                tracing::debug!(
                    parent = %parent.id,
                    child = %child_id,
                    error = %e,
                    "parent not ready for wrapup: failed to fetch child"
                );
                return false;
            }
        };

        // Check 1: child must be Complete
        if child.status != IssueStatus::Complete {
            tracing::debug!(
                parent = %parent.id,
                child = %child_id,
                status = ?child.status,
                "parent not ready for wrapup: child not Complete"
            );
            return false;
        }

        // Collect branch name for git check
        let branch = child
            .branch_name
            .clone()
            .unwrap_or_else(|| derive_worker_name(child_id, None));
        child_branches.push((child_id.clone(), branch));
    }

    // Check 2: all child branches must be merged into the parent branch
    let Ok(repo) = Repo::open(repo_root) else {
        tracing::debug!(
            issue = %parent.id,
            "parent not ready for wrapup: failed to open repo"
        );
        return false;
    };

    // Resolve parent branch tip (use remote ref since integration is bare)
    let parent_ref = format!("origin/{}", parent_branch);
    let parent_oid = match repo
        .inner()
        .find_branch(&parent_ref, git2::BranchType::Remote)
    {
        Ok(branch) => match branch.get().target() {
            Some(oid) => oid,
            None => {
                tracing::debug!(
                    issue = %parent.id,
                    branch = %parent_ref,
                    "parent not ready for wrapup: parent branch has no target"
                );
                return false;
            }
        },
        Err(_) => {
            tracing::debug!(
                issue = %parent.id,
                branch = %parent_ref,
                "parent not ready for wrapup: parent branch not found on remote"
            );
            return false;
        }
    };

    for (child_id, child_branch) in &child_branches {
        if !is_branch_merged_into(repo.inner(), child_branch, parent_oid) {
            tracing::debug!(
                parent = %parent.id,
                child = %child_id,
                branch = %child_branch,
                "parent not ready for wrapup: child branch not merged"
            );
            return false;
        }
    }

    true
}

/// Check if a branch's tip is reachable from (merged into) a target commit.
///
/// Tries both `origin/{branch}` (remote tracking) and `{branch}` (local).
fn is_branch_merged_into(repo: &git2::Repository, branch: &str, target_oid: git2::Oid) -> bool {
    // Try remote ref first
    let remote_ref = format!("origin/{}", branch);
    if let Ok(remote_branch) = repo.find_branch(&remote_ref, git2::BranchType::Remote) {
        if let Some(child_oid) = remote_branch.get().target() {
            return repo
                .graph_descendant_of(target_oid, child_oid)
                .unwrap_or(false);
        }
    }

    // Try local ref
    if let Ok(local_branch) = repo.find_branch(branch, git2::BranchType::Local) {
        if let Some(child_oid) = local_branch.get().target() {
            return repo
                .graph_descendant_of(target_oid, child_oid)
                .unwrap_or(false);
        }
    }

    false
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Result;
    use crate::issues::types::IssueFilter;
    use std::process::Command;
    use tempfile::TempDir;

    /// A mock issue provider backed by an in-memory Vec.
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

        fn list(&self, filter: &IssueFilter) -> Result<Vec<Issue>> {
            Ok(self
                .issues
                .iter()
                .filter(|i| i.matches(filter))
                .cloned()
                .collect())
        }

        fn get(&self, id: &str) -> Result<Option<Issue>> {
            Ok(self.issues.iter().find(|i| i.id == id).cloned())
        }

        fn update_status(&self, _id: &str, _status: &IssueStatus) -> Result<()> {
            Ok(())
        }
    }

    fn make_issue(id: &str, status: IssueStatus, branch: Option<&str>) -> Issue {
        Issue {
            id: id.into(),
            title: format!("Issue {}", id),
            status,
            priority: None,
            category: None,
            depends_on: vec![],
            body: String::new(),
            source: String::new(),
            children: vec![],
            labels: vec![],
            branch_name: branch.map(String::from),
            parent: None,
        }
    }

    /// Set up a bare "remote" repo and a working clone with an initial commit.
    /// Returns (working_dir, _remote_dir) — keep _remote_dir alive.
    fn setup_git_repo() -> (TempDir, TempDir) {
        let remote_dir = TempDir::new().unwrap();
        let work_dir = TempDir::new().unwrap();

        // Create bare remote
        Command::new("git")
            .args(["init", "--bare", "-q"])
            .current_dir(remote_dir.path())
            .output()
            .unwrap();

        // Clone it
        Command::new("git")
            .args([
                "clone",
                "-q",
                &remote_dir.path().to_string_lossy(),
                &work_dir.path().to_string_lossy(),
            ])
            .output()
            .unwrap();

        // Configure user
        for (key, val) in [
            ("user.email", "test@test.com"),
            ("user.name", "Test"),
            ("commit.gpgsign", "false"),
        ] {
            Command::new("git")
                .args(["config", key, val])
                .current_dir(work_dir.path())
                .output()
                .unwrap();
        }

        // Initial commit on main
        Command::new("git")
            .args(["commit", "--allow-empty", "-m", "init", "-q"])
            .current_dir(work_dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["push", "-q", "origin", "main"])
            .current_dir(work_dir.path())
            .output()
            .unwrap();

        (work_dir, remote_dir)
    }

    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn commit_file(dir: &Path, filename: &str, content: &str, msg: &str) {
        std::fs::write(dir.join(filename), content).unwrap();
        git(dir, &["add", filename]);
        git(dir, &["commit", "-q", "-m", msg]);
    }

    #[test]
    fn wrapup_ready_all_children_complete_and_merged() {
        let (work_dir, _remote) = setup_git_repo();
        let repo_root = work_dir.path();

        // Create parent integration branch
        git(repo_root, &["checkout", "-b", "parent-branch"]);
        commit_file(repo_root, "parent.txt", "parent", "parent init");
        git(repo_root, &["push", "-q", "origin", "parent-branch"]);

        // Create and merge 3 child branches into parent
        for i in 1..=3 {
            let child_branch = format!("child-{}", i);
            git(repo_root, &["checkout", "parent-branch"]);
            git(repo_root, &["checkout", "-b", &child_branch]);
            let filename = format!("child{}.txt", i);
            let msg = format!("child {} work", i);
            commit_file(repo_root, &filename, &format!("child {}", i), &msg);
            git(repo_root, &["push", "-q", "origin", &child_branch]);

            // Merge child into parent
            git(repo_root, &["checkout", "parent-branch"]);
            git(
                repo_root,
                &[
                    "merge",
                    "--no-ff",
                    "-q",
                    "-m",
                    &format!("merge child-{}", i),
                    &child_branch,
                ],
            );
        }
        git(repo_root, &["push", "-q", "origin", "parent-branch"]);

        // Fetch so remote refs are up to date
        git(repo_root, &["fetch", "-q", "origin"]);

        // Set up parent issue with 3 children
        let parent = Issue {
            id: "EPIC-1".into(),
            title: "Epic".into(),
            status: IssueStatus::InProgress,
            priority: None,
            category: None,
            depends_on: vec![],
            body: String::new(),
            source: String::new(),
            children: vec!["CHILD-1".into(), "CHILD-2".into(), "CHILD-3".into()],
            labels: vec![],
            branch_name: Some("parent-branch".into()),
            parent: None,
        };

        let provider = MockProvider::new(vec![
            parent.clone(),
            make_issue("CHILD-1", IssueStatus::Complete, Some("child-1")),
            make_issue("CHILD-2", IssueStatus::Complete, Some("child-2")),
            make_issue("CHILD-3", IssueStatus::Complete, Some("child-3")),
        ]);

        assert!(is_parent_ready_for_wrapup(&parent, &provider, repo_root));
    }

    #[test]
    fn wrapup_not_ready_child_still_backlog() {
        let (work_dir, _remote) = setup_git_repo();
        let repo_root = work_dir.path();

        // Create parent branch
        git(repo_root, &["checkout", "-b", "parent-branch"]);
        commit_file(repo_root, "parent.txt", "parent", "parent init");
        git(repo_root, &["push", "-q", "origin", "parent-branch"]);
        git(repo_root, &["fetch", "-q", "origin"]);

        let parent = Issue {
            id: "EPIC-1".into(),
            title: "Epic".into(),
            status: IssueStatus::InProgress,
            priority: None,
            category: None,
            depends_on: vec![],
            body: String::new(),
            source: String::new(),
            children: vec!["CHILD-1".into(), "CHILD-2".into()],
            labels: vec![],
            branch_name: Some("parent-branch".into()),
            parent: None,
        };

        let provider = MockProvider::new(vec![
            parent.clone(),
            make_issue("CHILD-1", IssueStatus::Complete, Some("child-1")),
            make_issue("CHILD-2", IssueStatus::Backlog, Some("child-2")),
        ]);

        assert!(!is_parent_ready_for_wrapup(&parent, &provider, repo_root));
    }

    #[test]
    fn wrapup_not_ready_child_branch_not_merged() {
        let (work_dir, _remote) = setup_git_repo();
        let repo_root = work_dir.path();

        // Create parent branch
        git(repo_root, &["checkout", "-b", "parent-branch"]);
        commit_file(repo_root, "parent.txt", "parent", "parent init");
        git(repo_root, &["push", "-q", "origin", "parent-branch"]);

        // Create child-1, merge into parent
        git(repo_root, &["checkout", "parent-branch"]);
        git(repo_root, &["checkout", "-b", "child-1"]);
        commit_file(repo_root, "c1.txt", "c1", "child 1");
        git(repo_root, &["push", "-q", "origin", "child-1"]);
        git(repo_root, &["checkout", "parent-branch"]);
        git(
            repo_root,
            &["merge", "--no-ff", "-q", "-m", "merge child-1", "child-1"],
        );
        git(repo_root, &["push", "-q", "origin", "parent-branch"]);

        // Create child-2, push but DON'T merge
        git(repo_root, &["checkout", "parent-branch"]);
        git(repo_root, &["checkout", "-b", "child-2"]);
        commit_file(repo_root, "c2.txt", "c2", "child 2");
        git(repo_root, &["push", "-q", "origin", "child-2"]);

        git(repo_root, &["checkout", "parent-branch"]);
        git(repo_root, &["fetch", "-q", "origin"]);

        let parent = Issue {
            id: "EPIC-1".into(),
            title: "Epic".into(),
            status: IssueStatus::InProgress,
            priority: None,
            category: None,
            depends_on: vec![],
            body: String::new(),
            source: String::new(),
            children: vec!["CHILD-1".into(), "CHILD-2".into()],
            labels: vec![],
            branch_name: Some("parent-branch".into()),
            parent: None,
        };

        let provider = MockProvider::new(vec![
            parent.clone(),
            make_issue("CHILD-1", IssueStatus::Complete, Some("child-1")),
            make_issue("CHILD-2", IssueStatus::Complete, Some("child-2")),
        ]);

        // Child-2 is Complete in status but NOT merged into parent branch
        assert!(!is_parent_ready_for_wrapup(&parent, &provider, repo_root));
    }

    #[test]
    fn wrapup_not_ready_no_parent_branch() {
        let (work_dir, _remote) = setup_git_repo();
        let repo_root = work_dir.path();

        let parent = Issue {
            id: "EPIC-1".into(),
            title: "Epic".into(),
            status: IssueStatus::InProgress,
            priority: None,
            category: None,
            depends_on: vec![],
            body: String::new(),
            source: String::new(),
            children: vec!["CHILD-1".into()],
            labels: vec![],
            branch_name: None, // No branch name
            parent: None,
        };

        let provider = MockProvider::new(vec![
            parent.clone(),
            make_issue("CHILD-1", IssueStatus::Complete, Some("child-1")),
        ]);

        assert!(!is_parent_ready_for_wrapup(&parent, &provider, repo_root));
    }

    #[test]
    fn wrapup_idempotent_existing_worker_skipped() {
        let (work_dir, _remote) = setup_git_repo();
        let repo_root = work_dir.path();

        // Create parent branch with merged child
        git(repo_root, &["checkout", "-b", "parent-branch"]);
        commit_file(repo_root, "parent.txt", "parent", "parent init");
        git(repo_root, &["checkout", "-b", "child-1"]);
        commit_file(repo_root, "c1.txt", "c1", "child 1");
        git(repo_root, &["push", "-q", "origin", "child-1"]);
        git(repo_root, &["checkout", "parent-branch"]);
        git(
            repo_root,
            &["merge", "--no-ff", "-q", "-m", "merge child-1", "child-1"],
        );
        git(repo_root, &["push", "-q", "origin", "parent-branch"]);
        git(repo_root, &["fetch", "-q", "origin"]);

        let parent = make_issue("EPIC-1", IssueStatus::InProgress, Some("parent-branch"));
        let parent = Issue {
            children: vec!["CHILD-1".into()],
            ..parent
        };

        let provider = MockProvider::new(vec![
            parent,
            make_issue("CHILD-1", IssueStatus::Complete, Some("child-1")),
        ]);

        // First call: should find the wrapup candidate
        let wrapup1 = collect_wrapup_parents(
            &provider,
            ProviderKind::File,
            repo_root,
            "test-repo",
            10,
            &[],
        );
        assert_eq!(wrapup1.len(), 1);
        assert_eq!(wrapup1[0].kind, SpawnKind::Wrapup);

        // Second call with existing worker: should skip
        let existing = vec![("test-repo".to_string(), "parent-branch".to_string())];
        let wrapup2 = collect_wrapup_parents(
            &provider,
            ProviderKind::File,
            repo_root,
            "test-repo",
            10,
            &existing,
        );
        assert!(
            wrapup2.is_empty(),
            "should not spawn duplicate wrapup worker"
        );
    }
}
