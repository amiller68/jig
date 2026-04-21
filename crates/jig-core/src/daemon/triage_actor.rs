//! Triage actor — runs ephemeral triage agents as direct subprocesses.
//!
//! Unlike normal workers (which run in tmux windows), triage agents are
//! one-shot read-only subprocesses that don't need interactive sessions.
//! This actor receives `TriageRequest`s, spawns each triage as a
//! `std::process::Command` subprocess, and returns `TriageComplete`s with
//! per-issue results.

use crate::spawn;

use super::messages::{TriageComplete, TriageIssue, TriageRequest, TriageResult};

/// Spawn the triage actor thread. Returns immediately.
///
/// The actor blocks on `rx.recv()` waiting for triage requests, runs each
/// triage as a subprocess via [`spawn::run_triage_subprocess`], and sends
/// `TriageComplete` back.
pub fn spawn(
    rx: flume::Receiver<TriageRequest>,
    tx: flume::Sender<TriageComplete>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("jig-triage".into())
        .spawn(move || {
            while let Ok(req) = rx.recv() {
                let mut results = Vec::new();

                for issue in &req.issues {
                    let result = run_single(issue);
                    results.push(result);
                }

                if tx.send(TriageComplete { results }).is_err() {
                    break;
                }
            }
        })
        .expect("failed to spawn triage actor thread")
}

/// Run a single triage subprocess and produce a result.
fn run_single(issue: &TriageIssue) -> TriageResult {
    let repo_name = issue
        .repo_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    tracing::info!(issue = %issue.issue.id, "running triage subprocess");

    match spawn::run_triage_subprocess(&issue.repo_root, &issue.issue) {
        Ok(()) => {
            tracing::info!(
                issue = %issue.issue.id,
                "triage subprocess completed successfully"
            );
            TriageResult {
                repo_name,
                issue_id: issue.issue.id.clone(),
                error: None,
            }
        }
        Err(msg) => {
            tracing::warn!(
                issue = %issue.issue.id,
                "triage subprocess failed: {}", msg
            );
            TriageResult {
                repo_name,
                issue_id: issue.issue.id.clone(),
                error: Some(msg),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::issues::{Issue, IssueStatus, ProviderKind};
    use std::path::PathBuf;

    fn make_triage_issue(id: &str) -> TriageIssue {
        TriageIssue {
            repo_root: PathBuf::from("/tmp/nonexistent-repo"),
            issue: Issue {
                id: id.to_string(),
                title: "Test issue".to_string(),
                body: "Test body".to_string(),
                status: IssueStatus::Triage,
                priority: None,
                category: None,
                depends_on: vec![],
                source: String::new(),
                children: vec![],
                labels: vec![],
                branch_name: None,
                parent: None,
            },
            provider_kind: ProviderKind::Linear,
        }
    }

    #[test]
    fn run_single_missing_repo() {
        let issue = make_triage_issue("JIG-99");
        let result = run_single(&issue);
        assert!(result.error.is_some());
        assert_eq!(result.issue_id, "JIG-99");
    }

    #[test]
    fn triage_result_success_fields() {
        let result = TriageResult {
            repo_name: "my-repo".to_string(),
            issue_id: "JIG-38".to_string(),
            error: None,
        };
        assert!(result.error.is_none());
        assert_eq!(result.issue_id, "JIG-38");
    }

    #[test]
    fn triage_result_error_fields() {
        let result = TriageResult {
            repo_name: "my-repo".to_string(),
            issue_id: "JIG-38".to_string(),
            error: Some("triage agent exited with code 1".to_string()),
        };
        assert!(result.error.is_some());
    }
}
