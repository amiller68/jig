//! PR lifecycle monitoring — checks merged/closed/open PRs and injects actions.

use crate::dispatch::Action;
use crate::events::WorkerState;
use crate::github::GitHubClient;
use crate::global::GlobalConfig;
use crate::registry::RepoRegistry;

/// Per-check result: check name and whether a problem was detected.
#[derive(Debug, Clone)]
pub(crate) struct PrCheckOutcome {
    pub name: &'static str,
    pub has_problem: bool,
}

/// Overall result of PR lifecycle checks for a single worker.
#[derive(Debug, Clone, Default)]
pub(crate) struct PrLifecycleResult {
    /// Per-check outcomes (only populated when PR is open).
    pub checks: Vec<PrCheckOutcome>,
    /// Whether the PR was merged/closed (terminal state handled separately).
    pub terminal: bool,
}

/// Monitors PR state and injects appropriate actions.
pub(crate) struct PrMonitor<'a> {
    client: &'a GitHubClient,
    config: &'a GlobalConfig,
}

impl<'a> PrMonitor<'a> {
    pub(super) fn new(client: &'a GitHubClient, config: &'a GlobalConfig) -> Self {
        Self { client, config }
    }

    /// Check PR lifecycle and inject cleanup/notify actions for merged/closed PRs.
    /// Returns a summary of check outcomes for display purposes.
    pub(super) fn check_lifecycle(
        &self,
        worker_name: &str,
        branch_name: &str,
        pr_url: &str,
        worker_state: &WorkerState,
        actions: &mut Vec<Action>,
    ) -> PrLifecycleResult {
        let mut result = PrLifecycleResult::default();

        // Extract PR number from URL (e.g., https://github.com/owner/repo/pull/123)
        let pr_number = match pr_url
            .rsplit('/')
            .next()
            .and_then(|s| s.parse::<u64>().ok())
        {
            Some(n) => n,
            None => return result,
        };

        let pr_state = match self.client.get_pr_state(pr_number) {
            Ok(s) => s,
            Err(e) => {
                tracing::info!("failed to check PR state for {}: {}", worker_name, e);
                return result;
            }
        };

        tracing::info!(
            worker = worker_name,
            pr_number = pr_number,
            pr_state = ?pr_state,
            "PR lifecycle check"
        );

        match pr_state {
            crate::github::PrState::Merged => {
                result.terminal = true;
                if self.config.github.auto_cleanup_merged {
                    actions.push(Action::Cleanup {
                        worker_id: worker_name.to_string(),
                    });
                    actions.push(Action::Notify {
                        worker_id: worker_name.to_string(),
                        message: format!("PR #{} merged, worker cleaned up", pr_number),
                    });
                }
            }
            crate::github::PrState::Closed => {
                result.terminal = true;
                actions.push(Action::Notify {
                    worker_id: worker_name.to_string(),
                    message: format!("PR #{} closed without merge", pr_number),
                });
                if self.config.github.auto_cleanup_closed {
                    actions.push(Action::Cleanup {
                        worker_id: worker_name.to_string(),
                    });
                }
            }
            crate::github::PrState::Open => {
                // Run all PR checks: CI, conflicts, reviews, commits
                let checks: Vec<(&str, std::result::Result<crate::github::PrCheck, _>)> = vec![
                    ("ci", crate::github::check_ci(self.client, branch_name)),
                    (
                        "conflicts",
                        crate::github::check_conflicts(self.client, pr_number),
                    ),
                    (
                        "reviews",
                        crate::github::check_reviews(self.client, pr_number),
                    ),
                    (
                        "commits",
                        crate::github::check_commits(self.client, pr_number),
                    ),
                ];

                for (check_type, check_result) in checks {
                    match check_result {
                        Ok(check) => {
                            let has_problem = check.nudge.is_some();
                            tracing::debug!(
                                check_type = check_type,
                                has_problem,
                                details = ?check.details,
                                "PR check result"
                            );
                            result.checks.push(PrCheckOutcome {
                                name: check_type,
                                has_problem,
                            });
                            if let Some(nudge_type) = check.nudge {
                                let count = worker_state
                                    .nudge_counts
                                    .get(nudge_type.count_key())
                                    .copied()
                                    .unwrap_or(0);
                                if count >= self.config.health.max_nudges {
                                    tracing::debug!(
                                        nudge_type = nudge_type.count_key(),
                                        count,
                                        "PR nudge limit reached, skipping"
                                    );
                                    continue;
                                }
                                actions.push(Action::Nudge {
                                    worker_id: worker_name.to_string(),
                                    nudge_type,
                                });
                            }
                        }
                        Err(e) => {
                            tracing::info!(
                                check_type = check_type,
                                error = %e,
                                "PR check failed"
                            );
                        }
                    }
                }
            }
        }

        result
    }
}

/// Create a GitHub client for a repo by looking up its path in the registry.
pub(super) fn make_github_client(repo_name: &str, registry: &RepoRegistry) -> Option<GitHubClient> {
    registry
        .repos()
        .iter()
        .find(|e| {
            e.path
                .file_name()
                .map(|n| n.to_string_lossy() == repo_name)
                .unwrap_or(false)
        })
        .and_then(|entry| {
            GitHubClient::from_repo_path(&entry.path)
                .map_err(|e| {
                    tracing::info!(repo = repo_name, error = %e, "GitHub client failed");
                    e
                })
                .ok()
        })
}
