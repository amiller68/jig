//! Monitor actor — the daemon's main per-tick work loop.
//!
//! Each tick it discovers active workers, delegates state checking to
//! [`Worker::tick()`], runs nudge rules, sends notifications, and returns
//! [`PruneTarget`]s for the prune actor to clean up.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::context::{self, Config, JigToml, RepoConfig, RepoEntry};
use crate::notify::{NotificationEvent, NotificationQueue, Notifier};
use crate::worker::events::{self, Event, EventKind, TerminalKind, WorkerState};
use crate::worker::{MuxStatus, WorkerStatus};
use jig_core::git::Branch;
use jig_core::mux::{Mux, TmuxMux};
use jig_core::prompt::Prompt;

type Worker = crate::worker::Worker;

use super::prune::PruneTarget;
use super::{Actor, TickContext};

// ── Request / Actor ─────────────────────────────────────────────────

pub struct MonitorRequest {
    pub ctx: TickContext,
}

#[derive(Default)]
pub struct MonitorActor {
    github_last_polled: Mutex<HashMap<String, Instant>>,
    previous_states: Mutex<HashMap<String, WorkerState>>,
    workers: Mutex<Vec<WorkerState>>,
}

const GITHUB_POLL_INTERVAL: Duration = Duration::from_secs(60);

impl MonitorActor {
    pub fn workers(&self) -> Vec<WorkerState> {
        self.workers.lock().unwrap().clone()
    }

    fn should_poll_github(&self, key: &str) -> bool {
        match self.github_last_polled.lock().unwrap().get(key) {
            Some(t) => t.elapsed() >= GITHUB_POLL_INTERVAL,
            None => true,
        }
    }

    fn mark_github_polled(&self, key: &str) {
        self.github_last_polled
            .lock()
            .unwrap()
            .insert(key.to_string(), Instant::now());
    }
}

impl Actor for MonitorActor {
    type Request = MonitorRequest;
    type Response = Vec<PruneTarget>;

    const NAME: &'static str = "jig-monitor";
    const QUEUE_SIZE: usize = 1;

    fn handle(&self, req: MonitorRequest) -> Vec<PruneTarget> {
        let global_config = &req.ctx.config;

        let notifier = match build_notifier(global_config) {
            Some(n) => n,
            None => {
                tracing::warn!("monitor: failed to build notifier");
                return Vec::new();
            }
        };

        // Discover workers
        let workers: Vec<(&RepoEntry, Worker)> = {
            let mut out = Vec::new();
            for entry in req.ctx.repos.iter() {
                let repo = match jig_core::git::Repo::open(&entry.path) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                for worker in Worker::discover(&repo) {
                    out.push((entry, worker));
                }
            }
            out
        };

        let mut display_results = Vec::new();
        let mut prune_targets = Vec::new();
        let mut new_states = HashMap::new();

        for (entry, worker) in &workers {
            let key = worker.worker_key();
            let repo_name = worker.repo_name();
            let mux = TmuxMux::for_repo_with_prefix(&req.ctx.session_prefix, &repo_name);

            match self.process_worker(
                &mux,
                entry,
                worker,
                &key,
                global_config,
                &notifier,
            ) {
                Ok((state, targets)) => {
                    display_results.push(state.clone());
                    prune_targets.extend(targets);
                    new_states.insert(key, state);
                }
                Err(e) => {
                    tracing::warn!(worker = %key, "process_worker failed: {}", e);
                }
            }
        }

        // Filter terminal/dead workers from display
        display_results.retain(|w| {
            !w.status.is_terminal()
                && !matches!(w.mux_status, MuxStatus::NotFound)
        });
        display_results.sort_by(|a, b| a.name.cmp(&b.name));

        *self.workers.lock().unwrap() = display_results;
        *self.previous_states.lock().unwrap() = new_states;

        // Recovery prune: scan for merged/closed PRs with worktrees still on disk
        for (entry, worker) in &workers {
            let key = worker.worker_key();
            if let Some(prev) = self.previous_states.lock().unwrap().get(&key) {
                if (prev.status == WorkerStatus::Merged || prev.status == WorkerStatus::Failed)
                    && worker.path().exists()
                {
                    prune_targets.push(PruneTarget {
                        repo_path: entry.path.clone(),
                        repo_name: worker.repo_name(),
                        worker_name: worker.branch().to_string(),
                    });
                }
            }
        }

        prune_targets
    }
}

impl MonitorActor {
    fn process_worker(
        &self,
        mux: &dyn Mux,
        repo_entry: &RepoEntry,
        worker: &Worker,
        key: &str,
        global_config: &Config,
        notifier: &Notifier,
    ) -> jig_core::error::Result<(WorkerState, Vec<PruneTarget>)> {
        let worker_name = worker.branch().to_string();
        let repo_name = worker.repo_name();

        // Decide whether to poll GitHub this tick
        let gh_client = if self.should_poll_github(key) {
            worker.github_client()
        } else {
            None
        };
        let gh: Option<&dyn jig_core::github::GitHub> =
            gh_client.as_ref().map(|c| c as &dyn jig_core::github::GitHub);

        if gh.is_some() {
            self.mark_github_polled(key);
        }

        let state = worker.tick(mux, gh, global_config, repo_entry)?;

        if state.status == WorkerStatus::Created {
            return Ok((state, vec![]));
        }

        let old_state = self
            .previous_states
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .unwrap_or_default();

        // Dispatch rules-based actions
        let mut actions = dispatch_actions(
            &repo_name,
            &worker_name,
            &old_state,
            &state,
            global_config.max_nudges,
            global_config.silence_threshold_seconds,
        );

        // Resume dead mux windows
        if !state.status.is_terminal() && state.status != WorkerStatus::Initializing {
            if !mux.window_exists(&worker_name) {
                tracing::info!(
                    worker = key,
                    "active worker has no mux window, attempting resume"
                );
                actions.retain(|a| !matches!(a, DispatchAction::Nudge { .. }));
                match try_resume_worker(&repo_entry.path, &worker_name, mux) {
                    Ok(true) => tracing::info!(worker = key, "worker resumed"),
                    Ok(false) => {}
                    Err(e) => {
                        tracing::warn!(worker = key, error = %e, "failed to resume worker")
                    }
                }
            }
        }

        // PR-based actions
        if state.pr_health.has_pr {
            self.dispatch_pr_actions(
                key,
                &repo_name,
                &worker_name,
                &repo_entry.path,
                &old_state,
                &state,
                global_config,
                &mut actions,
            );
        }

        // Execute actions
        let branch: Branch = state
            .branch
            .as_deref()
            .unwrap_or(&worker_name)
            .into();
        let event_log = events::event_log_for_worker(&repo_name, &worker_name)?;
        let prune_targets = self.execute_actions(
            &actions,
            key,
            &repo_name,
            &worker_name,
            &branch,
            &repo_entry.path,
            mux,
            &event_log,
            &state,
            global_config,
            notifier,
        );

        Ok((state, prune_targets))
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch_pr_actions(
        &self,
        key: &str,
        repo_name: &str,
        worker_name: &str,
        repo_path: &std::path::Path,
        old_state: &WorkerState,
        state: &WorkerState,
        global_config: &Config,
        actions: &mut Vec<DispatchAction>,
    ) {
        // Check for merged/closed via PR health + worker status
        if state.status == WorkerStatus::Merged && global_config.auto_cleanup_merged {
            actions.push(DispatchAction::Cleanup);
            actions.push(DispatchAction::Notify {
                event: NotificationEvent::WorkCompleted {
                    repo: repo_name.to_string(),
                    worker: worker_name.to_string(),
                    pr_url: state.pr_url.clone(),
                },
            });

            let auto_complete = JigToml::load(repo_path)
                .ok()
                .flatten()
                .map(|t| t.issues.auto_complete_on_merge)
                .unwrap_or(false);
            if auto_complete {
                if let Some(issue_id) = state.issue_ref.as_ref() {
                    actions.push(DispatchAction::UpdateIssueStatus {
                        issue_id: issue_id.to_string(),
                    });
                }
            }
        }

        if state.status == WorkerStatus::Failed
            && state.pr_health.has_pr
            && old_state.status != WorkerStatus::Failed
        {
            actions.push(DispatchAction::Notify {
                event: NotificationEvent::NeedsIntervention {
                    repo: repo_name.to_string(),
                    worker: worker_name.to_string(),
                    reason: "PR closed without merge".to_string(),
                },
            });
            if global_config.auto_cleanup_closed {
                actions.push(DispatchAction::Cleanup);
            }
        }

        // Draft PR check nudges (CI, conflicts, reviews, commits)
        if state.is_draft && !state.pr_health.pr_checks.is_empty() {
            let previous_feedback = old_state.review_feedback_count;
            if state.review_feedback_count > previous_feedback {
                tracing::info!(
                    worker = key,
                    previous = previous_feedback,
                    current = state.review_feedback_count,
                    "new review feedback detected"
                );
                actions.push(DispatchAction::Notify {
                    event: NotificationEvent::FeedbackReceived {
                        repo: repo_name.to_string(),
                        worker: worker_name.to_string(),
                        pr_url: state.pr_url.clone().unwrap_or_default(),
                    },
                });
            }

            let base = JigToml::load(repo_path)
                .ok()
                .flatten()
                .and_then(|t| t.worktree.base)
                .unwrap_or_else(|| context::DEFAULT_BASE_BRANCH.to_string());

            for check_name in state.pr_health.pr_checks.problems() {
                let nkey = nudge_key_for_check(check_name);
                let count = state.nudge_counts.get(nkey).copied().unwrap_or(0);
                if count >= global_config.max_nudges {
                    continue;
                }
                if let Some(&last_ts) = state.last_nudge_at.get(nkey) {
                    let now = chrono::Utc::now().timestamp();
                    if now - last_ts < global_config.silence_threshold_seconds as i64 {
                        continue;
                    }
                }
                let mut prompt = Prompt::new(template_for_check(check_name))
                    .named(nkey)
                    .var_num("nudge_count", count + 1)
                    .var_num("max_nudges", global_config.max_nudges)
                    .var_bool("is_final_nudge", count + 1 >= global_config.max_nudges);

                match check_name {
                    "ci" => {
                        prompt = prompt.var_list("ci_failures", Vec::<String>::new());
                    }
                    "conflicts" => {
                        prompt = prompt.var("base_branch", &base);
                    }
                    "commits" => {
                        prompt = prompt
                            .var_list("bad_commits", Vec::<String>::new())
                            .var("base_branch", &base);
                    }
                    _ => {}
                }

                if let Ok(message) = prompt.render() {
                    actions.push(DispatchAction::Nudge {
                        message,
                        nudge_key: nkey.to_string(),
                        is_pr_nudge: true,
                    });
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_actions(
        &self,
        actions: &[DispatchAction],
        key: &str,
        repo_name: &str,
        worker_name: &str,
        branch: &Branch,
        repo_path: &std::path::Path,
        mux: &dyn Mux,
        event_log: &crate::worker::events::EventLog,
        _state: &WorkerState,
        global_config: &Config,
        notifier: &Notifier,
    ) -> Vec<PruneTarget> {
        let mut prune_targets = Vec::new();

        for action in actions {
            match action {
                DispatchAction::Nudge {
                    message,
                    nudge_key,
                    is_pr_nudge,
                } => {
                    let w = Worker::from_branch(repo_path, branch.clone());
                    if w.has_mux_window(mux) {
                        if !is_pr_nudge && !w.is_agent_running(mux) {
                            continue;
                        }
                        let prompt = Prompt::new(message).named(nudge_key);
                        match w.nudge(prompt, mux) {
                            Ok(()) => {
                                tracing::info!(worker = key, nudge_key = %nudge_key, "nudge delivered")
                            }
                            Err(e) => {
                                tracing::warn!(worker = key, nudge_key = %nudge_key, "nudge failed: {}", e)
                            }
                        }
                    }
                }
                DispatchAction::Notify { event } => {
                    if let Err(e) = notifier.emit(event.clone()) {
                        tracing::warn!(worker = key, "notification failed: {}", e);
                    }
                }
                DispatchAction::Cleanup => {
                    if mux.window_exists(&branch.to_string()) {
                        if let Err(e) = mux.kill_window(&branch.to_string()) {
                            tracing::warn!("failed to kill window for {}: {}", worker_name, e);
                        }
                    }
                    if let Err(e) = event_log.append(&Event::now(EventKind::Terminal {
                        terminal: TerminalKind::Archived,
                        reason: None,
                    })) {
                        tracing::warn!("failed to emit cleanup event for {}: {}", key, e);
                    }
                    prune_targets.push(PruneTarget {
                        repo_path: repo_path.to_path_buf(),
                        repo_name: repo_name.to_string(),
                        worker_name: worker_name.to_string(),
                    });
                }
                DispatchAction::Restart => {
                    match try_resume_worker(repo_path, worker_name, mux) {
                        Ok(true) => tracing::info!(worker = key, "worker resumed via restart"),
                        Ok(false) => {}
                        Err(e) => tracing::warn!(worker = key, "restart failed: {}", e),
                    }
                }
                DispatchAction::UpdateIssueStatus { issue_id } => {
                    if let Ok(ctx) = RepoConfig::from_path(repo_path) {
                        if let Ok(provider) = ctx.issue_provider(global_config) {
                            let should_update = match provider.get(issue_id) {
                                Ok(Some(issue)) => !matches!(
                                    issue.status(),
                                    jig_core::issues::issue::IssueStatus::Complete
                                ),
                                _ => false,
                            };
                            if should_update {
                                match provider.update_status(
                                    issue_id,
                                    &jig_core::issues::issue::IssueStatus::Complete,
                                ) {
                                    Ok(()) => {
                                        tracing::info!(worker = %worker_name, issue = %issue_id, "auto-completed issue")
                                    }
                                    Err(e) => {
                                        tracing::warn!(worker = %worker_name, issue = %issue_id, "auto-complete failed: {}", e)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        prune_targets
    }
}

// ── Dispatch actions ────────────────────────────────────────────────

#[allow(dead_code)]
enum DispatchAction {
    Nudge {
        message: String,
        nudge_key: String,
        is_pr_nudge: bool,
    },
    Notify {
        event: NotificationEvent,
    },
    Restart,
    Cleanup,
    UpdateIssueStatus {
        issue_id: String,
    },
}

// ── Dispatch rules ──────────────────────────────────────────────────

const TEMPLATE_IDLE: &str = r#"STATUS CHECK: You've been idle for a while (nudge {{nudge_count}}/{{max_nudges}}).

{{#if has_changes}}
You have uncommitted changes but no PR yet. What's blocking you?

1. If ready: commit (conventional format), push, create PR, update issue, call /review
2. If stuck: explain what you need help with
3. If complete but confused: finish the PR
{{else}}
No recent commits. What's the current state?

1. Still working? Give a brief status update and continue
2. Stuck on something? Explain what's blocking you
3. Done but forgot to create PR? Commit, push, create PR, call /review
{{/if}}

{{#if is_final_nudge}}
This is your final nudge. If you need human help, say so now.
{{/if}}
"#;

const TEMPLATE_STUCK: &str = r#"STUCK PROMPT DETECTED: You appear to be waiting at an interactive prompt.
Auto-approving... (nudge {{nudge_count}}/{{max_nudges}})
"#;

fn dispatch_actions(
    repo_name: &str,
    worker_name: &str,
    old_state: &WorkerState,
    new_state: &WorkerState,
    max_nudges: u32,
    cooldown_seconds: u64,
) -> Vec<DispatchAction> {
    let mut actions = vec![];
    let is_transition = old_state.status != new_state.status;

    if !new_state.status.is_terminal() && new_state.pr_url.is_none() {
        let (nudge_key, template, is_stuck) = match new_state.status {
            WorkerStatus::WaitingInput => ("stuck", TEMPLATE_STUCK, true),
            WorkerStatus::Stalled | WorkerStatus::Idle => ("idle", TEMPLATE_IDLE, false),
            _ => ("", "", false),
        };

        let _ = is_stuck;

        if !nudge_key.is_empty() {
            let count = new_state.nudge_counts.get(nudge_key).copied().unwrap_or(0);

            let cooldown_ok = match new_state.last_nudge_at.get(nudge_key) {
                Some(&last_ts) => {
                    let elapsed = chrono::Utc::now().timestamp() - last_ts;
                    elapsed >= cooldown_seconds as i64
                }
                None => true,
            };

            if count < max_nudges && cooldown_ok {
                let mut prompt = Prompt::new(template)
                    .named(nudge_key)
                    .var_num("nudge_count", count + 1)
                    .var_num("max_nudges", max_nudges)
                    .var_bool("is_final_nudge", count + 1 >= max_nudges);

                if nudge_key == "idle" {
                    prompt = prompt.var_bool("has_changes", new_state.commit_count > 0);
                }

                if let Ok(message) = prompt.render() {
                    actions.push(DispatchAction::Nudge {
                        message,
                        nudge_key: nudge_key.to_string(),
                        is_pr_nudge: false,
                    });
                }
            } else if is_transition {
                actions.push(DispatchAction::Notify {
                    event: NotificationEvent::NeedsIntervention {
                        repo: repo_name.to_string(),
                        worker: worker_name.to_string(),
                        reason: format!(
                            "Max nudges reached for {} worker, needs human attention",
                            match new_state.status {
                                WorkerStatus::WaitingInput => "stuck",
                                WorkerStatus::Stalled => "stalled",
                                WorkerStatus::Idle => "idle",
                                _ => "unknown",
                            }
                        ),
                    },
                });
            }
        }
    }

    tracing::debug!(
        worker = %format!("{}/{}", repo_name, worker_name),
        transition = is_transition,
        action_count = actions.len(),
        "dispatch_actions"
    );

    // PR opened
    if old_state.pr_url.is_none() && new_state.pr_url.is_some() {
        let pr_url = new_state.pr_url.clone().unwrap_or_default();
        actions.push(DispatchAction::Notify {
            event: NotificationEvent::PrOpened {
                repo: repo_name.to_string(),
                worker: worker_name.to_string(),
                pr_url,
            },
        });
    }

    // Transition to Failed
    if old_state.status != WorkerStatus::Failed && new_state.status == WorkerStatus::Failed {
        actions.push(DispatchAction::Notify {
            event: NotificationEvent::NeedsIntervention {
                repo: repo_name.to_string(),
                worker: worker_name.to_string(),
                reason: "Worker failed".to_string(),
            },
        });
    }

    actions
}

// ── Free functions ──────────────────────────────────────────────────

fn try_resume_worker(
    repo_root: &std::path::Path,
    worker_name: &str,
    mux: &dyn Mux,
) -> jig_core::error::Result<bool> {
    let worker = Worker::from_branch(repo_root, worker_name.into());
    if worker.has_mux_window(mux) {
        return Ok(false);
    }
    let wt = worker.worktree()?;
    let jig_config = JigToml::load(repo_root)?.unwrap_or_default();
    let agent = jig_core::agents::Agent::from_config(
        &jig_config.agent.agent_type,
        Some(&jig_config.agent.model),
        &jig_config.agent.disallowed_tools,
    )
    .unwrap_or_else(|| jig_core::agents::Agent::from_config("claude", None, &[]).unwrap());
    Worker::resume(&wt, &agent, "You were interrupted. Resume your previous task.", mux)?;
    Ok(true)
}

fn build_notifier(config: &Config) -> Option<Notifier> {
    let queue_path = crate::context::notifications_path().ok()?;
    let queue = NotificationQueue::new(queue_path);
    Some(Notifier::new(config.notify.clone(), queue))
}

// ── PR nudge templates ──────────────────────────────────────────────

const TEMPLATE_CI: &str = r#"CI is failing on your PR (nudge {{nudge_count}}/{{max_nudges}}).

Fix these issues:
{{#each ci_failures}}
  - {{this}}
{{/each}}

STEPS:
1. Fix the failing checks
2. Commit using conventional commits: fix(ci): fix linting errors
3. Push to your branch: git push
4. Verify CI passes
5. Call /review when green
"#;

const TEMPLATE_CONFLICT: &str = r#"Your PR has merge conflicts with {{base_branch}} (nudge {{nudge_count}}/{{max_nudges}}).

Resolve them:

1. git fetch origin
2. git rebase {{base_branch}}
3. Resolve conflicts, stage files, git rebase --continue
4. git push --force-with-lease
5. Call /review when conflicts are resolved
"#;

const TEMPLATE_REVIEW: &str = r#"Your PR has unresolved review comments (nudge {{nudge_count}}/{{max_nudges}}).

Address all feedback, commit, push, and call /review.
"#;

const TEMPLATE_BAD_COMMITS: &str = r#"Your PR has commits that don't follow conventional commit format (nudge {{nudge_count}}/{{max_nudges}}).

Bad commits:
{{#each bad_commits}}
  - {{this}}
{{/each}}

Fix with interactive rebase:

1. git rebase -i {{base_branch}}
2. Change 'pick' to 'reword' for each bad commit
3. Update message to: <type>(<scope>): <description>
   Types: feat|fix|docs|style|refactor|perf|test|chore|ci
4. git push --force-with-lease
5. Call /review
"#;

fn nudge_key_for_check(check_name: &str) -> &str {
    match check_name {
        "ci" => "ci",
        "conflicts" => "conflict",
        "reviews" => "review",
        "commits" => "bad_commits",
        _ => check_name,
    }
}

fn template_for_check(check_name: &str) -> &'static str {
    match check_name {
        "ci" => TEMPLATE_CI,
        "conflicts" => TEMPLATE_CONFLICT,
        "reviews" => TEMPLATE_REVIEW,
        "commits" => TEMPLATE_BAD_COMMITS,
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waiting_input_triggers_nudge() {
        let old = WorkerState {
            status: WorkerStatus::Running,
            ..Default::default()
        };
        let new = WorkerState {
            status: WorkerStatus::WaitingInput,
            ..Default::default()
        };

        let actions = dispatch_actions("repo", "test", &old, &new, 3, 300);
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(&actions[0], DispatchAction::Nudge { nudge_key, .. } if nudge_key == "stuck")
        );
    }

    #[test]
    fn max_nudges_triggers_notify() {
        let old = WorkerState {
            status: WorkerStatus::Running,
            ..Default::default()
        };
        let mut new = WorkerState {
            status: WorkerStatus::WaitingInput,
            ..Default::default()
        };
        new.nudge_counts.insert("stuck".to_string(), 3);

        let actions = dispatch_actions("repo", "test", &old, &new, 3, 300);
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(&actions[0], DispatchAction::Notify { event: NotificationEvent::NeedsIntervention { reason, .. } } if reason.contains("Max nudges"))
        );
    }

    #[test]
    fn stalled_triggers_nudge() {
        let old = WorkerState {
            status: WorkerStatus::Running,
            ..Default::default()
        };
        let new = WorkerState {
            status: WorkerStatus::Stalled,
            ..Default::default()
        };

        let actions = dispatch_actions("repo", "test", &old, &new, 3, 300);
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(&actions[0], DispatchAction::Nudge { nudge_key, .. } if nudge_key == "idle")
        );
    }

    #[test]
    fn pr_opened_triggers_notify() {
        let old = WorkerState::default();
        let new = WorkerState {
            pr_url: Some("https://github.com/pr/1".to_string()),
            ..Default::default()
        };

        let actions = dispatch_actions("repo", "test", &old, &new, 3, 300);
        assert_eq!(actions.len(), 1);
        assert!(matches!(
            &actions[0],
            DispatchAction::Notify {
                event: NotificationEvent::PrOpened { .. }
            }
        ));
    }

    #[test]
    fn no_change_no_actions() {
        let state = WorkerState {
            status: WorkerStatus::Running,
            ..Default::default()
        };

        let actions = dispatch_actions("repo", "test", &state, &state, 3, 300);
        assert!(actions.is_empty());
    }

    #[test]
    fn same_status_still_nudges() {
        let state = WorkerState {
            status: WorkerStatus::WaitingInput,
            ..Default::default()
        };

        let actions = dispatch_actions("repo", "test", &state, &state, 3, 300);
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(&actions[0], DispatchAction::Nudge { nudge_key, .. } if nudge_key == "stuck")
        );
    }

    #[test]
    fn failed_triggers_notify() {
        let old = WorkerState {
            status: WorkerStatus::Running,
            ..Default::default()
        };
        let new = WorkerState {
            status: WorkerStatus::Failed,
            ..Default::default()
        };

        let actions = dispatch_actions("repo", "test", &old, &new, 3, 300);
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(&actions[0], DispatchAction::Notify { event: NotificationEvent::NeedsIntervention { reason, .. } } if reason.contains("failed"))
        );
    }

    #[test]
    fn idle_triggers_nudge() {
        let old = WorkerState {
            status: WorkerStatus::Running,
            ..Default::default()
        };
        let new = WorkerState {
            status: WorkerStatus::Idle,
            ..Default::default()
        };

        let actions = dispatch_actions("repo", "test", &old, &new, 3, 300);
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(&actions[0], DispatchAction::Nudge { nudge_key, .. } if nudge_key == "idle")
        );
    }
}
