//! PR lifecycle monitoring — checks merged/closed/open PRs and injects actions.

use crate::config::ResolvedNudgeConfig;
use crate::dispatch::{Action, NotifyKind};
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
    /// Total review feedback count (inline comments + ChangesRequested reviews).
    pub review_feedback_count: Option<u32>,
}

/// Monitors PR state and injects appropriate actions.
pub(crate) struct PrMonitor<'a, F> {
    client: &'a GitHubClient,
    config: &'a GlobalConfig,
    resolve: F,
}

impl<'a, F> PrMonitor<'a, F>
where
    F: Fn(&str) -> ResolvedNudgeConfig,
{
    pub(super) fn new(client: &'a GitHubClient, config: &'a GlobalConfig, resolve: F) -> Self {
        Self {
            client,
            config,
            resolve,
        }
    }

    /// Check PR lifecycle and inject cleanup/notify actions for merged/closed PRs.
    /// Returns a summary of check outcomes for display purposes.
    pub(super) fn check_lifecycle(
        &self,
        worker_name: &str,
        branch_name: &str,
        pr_url: &str,
        worker_state: &mut WorkerState,
        stored_review_feedback_count: Option<u32>,
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

        let pr_state_info = match self.client.get_pr_state(pr_number) {
            Ok(s) => s,
            Err(e) => {
                tracing::info!("failed to check PR state for {}: {}", worker_name, e);
                return result;
            }
        };

        tracing::info!(
            worker = worker_name,
            pr_number = pr_number,
            pr_state = ?pr_state_info.state,
            is_draft = pr_state_info.is_draft,
            "PR lifecycle check"
        );

        match pr_state_info.state {
            crate::github::PrState::Merged => {
                result.terminal = true;
                if self.config.github.auto_cleanup_merged {
                    actions.push(Action::Cleanup {
                        worker_id: worker_name.to_string(),
                    });
                    actions.push(Action::Notify {
                        worker_id: worker_name.to_string(),
                        message: format!("PR #{} merged, worker cleaned up", pr_number),
                        kind: NotifyKind::WorkCompleted {
                            pr_url: Some(pr_url.to_string()),
                        },
                    });
                }
            }
            crate::github::PrState::Closed => {
                result.terminal = true;
                actions.push(Action::Notify {
                    worker_id: worker_name.to_string(),
                    message: format!("PR #{} closed without merge", pr_number),
                    kind: NotifyKind::NeedsIntervention,
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
                            // Extract review feedback count
                            if check_type == "reviews" {
                                let comments = check.review_comment_count.unwrap_or(0);
                                let changes_req = check.changes_requested_count.unwrap_or(0);
                                let current = comments + changes_req;
                                result.review_feedback_count = Some(current);

                                // Reset review nudge count if new feedback arrived
                                if pr_state_info.is_draft {
                                    let previous = stored_review_feedback_count.unwrap_or(0);
                                    if current > previous {
                                        tracing::info!(
                                            previous,
                                            current,
                                            "new review feedback detected, resetting review nudge count"
                                        );
                                        worker_state.nudge_counts.remove("review");
                                        actions.push(Action::Notify {
                                            worker_id: worker_name.to_string(),
                                            message: format!(
                                                "New review feedback on PR ({}→{} items)",
                                                previous, current
                                            ),
                                            kind: NotifyKind::FeedbackReceived {
                                                pr_url: pr_url.to_string(),
                                            },
                                        });
                                    }
                                }
                            }
                            result.checks.push(PrCheckOutcome {
                                name: check_type,
                                has_problem,
                            });
                            // Only nudge draft PRs — non-draft PRs are in human review
                            if let Some(nudge_type) = check.nudge.filter(|_| pr_state_info.is_draft)
                            {
                                let resolved = (self.resolve)(nudge_type.count_key());
                                let count = worker_state
                                    .nudge_counts
                                    .get(nudge_type.count_key())
                                    .copied()
                                    .unwrap_or(0);
                                if count >= resolved.max {
                                    tracing::debug!(
                                        nudge_type = nudge_type.count_key(),
                                        count,
                                        max = resolved.max,
                                        "PR nudge limit reached, skipping"
                                    );
                                    continue;
                                }
                                // Cooldown: skip if last nudge of this type was too recent
                                if let Some(&last_ts) =
                                    worker_state.last_nudge_at.get(nudge_type.count_key())
                                {
                                    let now = chrono::Utc::now().timestamp();
                                    let elapsed = now - last_ts;
                                    if elapsed < resolved.cooldown_seconds as i64 {
                                        tracing::debug!(
                                            nudge_type = nudge_type.count_key(),
                                            elapsed,
                                            cooldown = resolved.cooldown_seconds,
                                            "PR nudge cooldown active, skipping"
                                        );
                                        continue;
                                    }
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
