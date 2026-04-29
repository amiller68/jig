//! Dispatch actor — discovers workers, reads event logs, checks GitHub PR
//! status, sends nudges, emits notifications, saves workers state, and
//! caches display info for `ps`.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use url::Url;

use crate::config::registry::RepoRegistry;
use crate::config::{
    self, Config, GlobalConfig, HealthConfig, JigToml, RepoHealthConfig, ResolvedNudgeConfig,
    WorkerEntry, WorkersState,
};
use crate::notify::{NotificationEvent, NotificationQueue, Notifier};
use crate::worker::events::{Event, EventKind, EventLog, TerminalKind, WorkerState};
use crate::worker::{TmuxStatus, WorkerStatus};
use jig_core::github::{self, GitHubClient, PrState};
use jig_core::mux::tmux::{TmuxSession, TmuxWindow};
use jig_core::prompt::Prompt;

type Worker = crate::worker::Worker<jig_core::mux::tmux::TmuxWindow>;

use super::prune::PruneTarget;
use super::Actor;

/// Snapshot of a single worker's state, cached after each dispatch pass.
#[derive(Debug, Clone)]
pub struct WorkerSnapshot {
    pub repo: String,
    pub name: String,
    pub branch: String,
    pub tmux_status: TmuxStatus,
    pub worker_status: Option<WorkerStatus>,
    pub nudge_count: u32,
    pub max_nudges: u32,
    pub commits_ahead: usize,
    pub is_dirty: bool,
    pub pr_url: Option<String>,
    pub issue_ref: Option<String>,
    pub pr_health: PrHealth,
    pub is_draft: bool,
    pub nudge_cooldown_remaining: Option<u64>,
}

/// Per-worker PR health info collected during a dispatch pass.
#[derive(Debug, Clone, Default)]
pub struct PrHealth {
    pub pr_checks: PrChecks,
    pub pr_error: Option<String>,
    pub has_pr: bool,
}

// ── GitHub PR types ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PrInfo {
    pub status: PrStatus,
}

#[derive(Debug, Clone)]
pub enum PrStatus {
    NoPr,
    Error {
        pr_url: Option<Url>,
        error: String,
    },
    Merged {
        pr_url: Url,
    },
    Closed {
        pr_url: Url,
    },
    Open {
        pr_url: Url,
        is_draft: bool,
        checks: PrChecks,
        review_feedback_count: u32,
    },
}

#[derive(Debug, Clone, Default)]
pub struct PrChecks {
    pub ci: Option<bool>,
    pub conflicts: Option<bool>,
    pub reviews: Option<bool>,
    pub commits: Option<bool>,
}

impl PrChecks {
    pub fn problems(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.ci == Some(true) {
            out.push("ci");
        }
        if self.conflicts == Some(true) {
            out.push("conflicts");
        }
        if self.reviews == Some(true) {
            out.push("reviews");
        }
        if self.commits == Some(true) {
            out.push("commits");
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.ci.is_none()
            && self.conflicts.is_none()
            && self.reviews.is_none()
            && self.commits.is_none()
    }
}

// ── Request / Actor ─────────────────────────────────────────────────

pub struct DispatchRequest {
    pub session_prefix: String,
    pub repo_filter: Option<String>,
}

#[derive(Default)]
pub struct DispatchActor {
    github_cache: Mutex<HashMap<String, PrInfo>>,
    github_last_polled: Mutex<HashMap<String, Instant>>,
    workers: Mutex<Vec<WorkerSnapshot>>,
    prune_targets: Mutex<Vec<PruneTarget>>,
}

const GITHUB_POLL_INTERVAL: Duration = Duration::from_secs(60);

impl DispatchActor {
    pub fn workers(&self) -> Vec<WorkerSnapshot> {
        self.workers.lock().unwrap().clone()
    }

    pub fn take_prune_targets(&self) -> Vec<PruneTarget> {
        std::mem::take(&mut *self.prune_targets.lock().unwrap())
    }

    fn get_cached_pr(&self, key: &str) -> Option<PrInfo> {
        self.github_cache.lock().unwrap().get(key).cloned()
    }

    fn should_poll_github(&self, key: &str) -> bool {
        match self.github_last_polled.lock().unwrap().get(key) {
            Some(t) => t.elapsed() >= GITHUB_POLL_INTERVAL,
            None => true,
        }
    }

    fn poll_github(&self, worker: &Worker) {
        let key = worker.worker_key();
        self.github_last_polled
            .lock()
            .unwrap()
            .insert(key.clone(), Instant::now());

        let info = check_pr(worker);
        self.github_cache.lock().unwrap().insert(key, info);
    }
}

impl Actor for DispatchActor {
    type Request = DispatchRequest;
    type Response = ();

    const NAME: &'static str = "jig-dispatch";
    const QUEUE_SIZE: usize = 1;

    fn handle(&self, req: DispatchRequest) {
        let global_config = match GlobalConfig::load() {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("dispatch: failed to load global config: {}", e);
                return;
            }
        };
        let registry = RepoRegistry::load().unwrap_or_default();
        let mut workers_state = WorkersState::load().unwrap_or_default();

        let notifier = match build_notifier(&global_config) {
            Some(n) => n,
            None => {
                tracing::warn!("dispatch: failed to build notifier");
                return;
            }
        };

        // Discover workers
        let workers: Vec<(String, Worker)> = {
            let filtered = registry.filtered_repos(req.repo_filter.as_deref());
            let mut out = Vec::new();
            for entry in &filtered {
                let repo = match jig_core::git::Repo::open(&entry.path) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                for worker in Worker::discover(&repo) {
                    let repo_name = entry
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    out.push((repo_name, worker));
                }
            }
            out
        };

        let mut display_results = Vec::new();
        let mut prune_targets = Vec::new();

        for (repo_name, worker) in &workers {
            let worker_name = worker.branch().to_string();
            let key = format!("{}/{}", repo_name, worker_name);

            match self.process_worker(
                &req.session_prefix,
                repo_name,
                worker,
                &key,
                &mut workers_state,
                &global_config,
                &notifier,
                &registry,
            ) {
                Ok((display, targets)) => {
                    display_results.push(display);
                    prune_targets.extend(targets);
                }
                Err(e) => {
                    tracing::warn!(worker = %key, "process_worker failed: {}", e);
                }
            }
        }

        // Filter terminal/dead workers from display
        display_results.retain(|w| {
            let is_terminal = w
                .worker_status
                .as_ref()
                .map(|s| s.is_terminal())
                .unwrap_or(false);
            let tmux_dead = matches!(w.tmux_status, TmuxStatus::NoSession | TmuxStatus::NoWindow);
            !is_terminal && !tmux_dead
        });
        display_results.sort_by(|a, b| a.name.cmp(&b.name));

        *self.workers.lock().unwrap() = display_results;
        *self.prune_targets.lock().unwrap() = prune_targets;

        workers_state.save().unwrap_or_else(|e| {
            tracing::warn!("failed to save workers state: {}", e);
        });

        // Recovery prune: scan for merged/closed PRs with worktrees still on disk
        {
            let mut recovery = Vec::new();
            for (repo_name, worker) in &workers {
                let key = format!("{}/{}", repo_name, worker.branch());
                if let Some(cached) = self.get_cached_pr(&key) {
                    if matches!(
                        cached.status,
                        PrStatus::Merged { .. } | PrStatus::Closed { .. }
                    ) {
                        let repo_root = worker
                            .path()
                            .parent()
                            .and_then(|jig_dir| jig_dir.parent())
                            .unwrap_or(worker.path());
                        if worker.path().exists() {
                            recovery.push(PruneTarget {
                                repo_path: repo_root.to_path_buf(),
                                repo_name: repo_name.clone(),
                                worker_name: worker.branch().to_string(),
                            });
                        }
                    }
                }
            }
            if !recovery.is_empty() {
                self.prune_targets.lock().unwrap().extend(recovery);
            }
        }
    }
}

impl DispatchActor {
    #[allow(clippy::too_many_arguments)]
    fn process_worker(
        &self,
        session_prefix: &str,
        repo_name: &str,
        worker: &Worker,
        key: &str,
        workers_state: &mut WorkersState,
        global_config: &GlobalConfig,
        notifier: &Notifier,
        registry: &RepoRegistry,
    ) -> jig_core::error::Result<(WorkerSnapshot, Vec<PruneTarget>)> {
        let worker_name = worker.branch().to_string();
        let repo_path = find_repo_path(registry, repo_name).map(|e| e.path.clone());

        let repo_health = repo_path
            .as_ref()
            .and_then(|p| JigToml::load(p).ok().flatten())
            .map(|toml| toml.health)
            .unwrap_or_default();

        let effective_health = HealthConfig {
            silence_threshold_seconds: repo_health.resolve_silence_threshold(&global_config.health),
            max_nudges: repo_health.resolve_max_nudges(&global_config.health),
        };
        let resolve = make_nudge_resolver(&repo_health, &global_config.health);

        let event_log = EventLog::for_worker(repo_name, &worker_name)?;
        let events = event_log.read_all()?;
        let new_state = WorkerState::reduce(&events, &effective_health);
        let branch_name = new_state
            .branch
            .as_deref()
            .unwrap_or(&worker_name)
            .to_string();

        // Poll GitHub if needed (non-terminal workers only)
        if !new_state.status.is_terminal() && self.should_poll_github(key) {
            self.poll_github(worker);
        }

        let mut worker_tick_info = PrHealth::default();
        let mut is_draft = false;

        // Process cached PR data
        if let Some(cached) = self.get_cached_pr(key) {
            match &cached.status {
                PrStatus::NoPr => {}
                PrStatus::Error { error, .. } => {
                    worker_tick_info.pr_error = Some(error.clone());
                }
                PrStatus::Merged { pr_url } | PrStatus::Closed { pr_url } => {
                    worker_tick_info.has_pr = true;
                    if new_state.pr_url.is_none() {
                        let pr_number = pr_url
                            .path_segments()
                            .and_then(|mut s| s.next_back())
                            .unwrap_or("0");
                        let _ = event_log.append(&Event::now(EventKind::PrOpened {
                            pr_url: pr_url.to_string(),
                            pr_number: pr_number.to_string(),
                        }));
                    }
                }
                PrStatus::Open {
                    pr_url,
                    is_draft: draft,
                    checks,
                    ..
                } => {
                    worker_tick_info.has_pr = true;
                    worker_tick_info.pr_checks = checks.clone();
                    is_draft = *draft;
                    if new_state.pr_url.is_none() {
                        let pr_number = pr_url
                            .path_segments()
                            .and_then(|mut s| s.next_back())
                            .unwrap_or("0");
                        let _ = event_log.append(&Event::now(EventKind::PrOpened {
                            pr_url: pr_url.to_string(),
                            pr_number: pr_number.to_string(),
                        }));
                    }
                }
            }
        }

        // Re-read events after potential PrOpened append
        let events = event_log.read_all()?;
        let mut new_state = WorkerState::reduce(&events, &effective_health);

        if new_state.status == WorkerStatus::Created {
            let display = WorkerSnapshot {
                repo: repo_name.to_string(),
                name: worker_name.clone(),
                branch: branch_name,
                tmux_status: TmuxStatus::NoWindow,
                worker_status: Some(new_state.status),
                nudge_count: 0,
                max_nudges: 0,
                commits_ahead: 0,
                is_dirty: false,
                pr_url: None,
                issue_ref: None,
                pr_health: PrHealth::default(),
                is_draft: false,
                nudge_cooldown_remaining: None,
            };
            return Ok((display, vec![]));
        }

        let old_state = workers_state
            .get_worker(key)
            .map(entry_to_worker_state)
            .unwrap_or_default();

        // Dispatch rules-based actions (idle/stalled/stuck nudges, PR opened, failed)
        let mut actions =
            dispatch_actions(repo_name, &worker_name, &old_state, &new_state, &resolve);

        // Resume dead tmux windows
        if !new_state.status.is_terminal() && new_state.status != WorkerStatus::Initializing {
            let window = TmuxWindow::new(format!("{}{}", session_prefix, repo_name), &worker_name);
            if !window.exists() {
                tracing::info!(
                    worker = key,
                    "active worker has no tmux window, attempting resume"
                );
                actions.retain(|a| !matches!(a, DispatchAction::Nudge { .. }));
                if let Some(rp) = &repo_path {
                    match try_resume_worker(rp, &worker_name) {
                        Ok(true) => tracing::info!(worker = key, "worker resumed"),
                        Ok(false) => {}
                        Err(e) => {
                            tracing::warn!(worker = key, error = %e, "failed to resume worker")
                        }
                    }
                }
            }
        }

        // PR-based actions
        let mut current_review_feedback_count: Option<u32> = None;
        if let Some(cached) = self.get_cached_pr(key) {
            match &cached.status {
                PrStatus::Merged { pr_url } => {
                    if global_config.github.auto_cleanup_merged {
                        actions.push(DispatchAction::Cleanup);
                        actions.push(DispatchAction::Notify {
                            event: NotificationEvent::WorkCompleted {
                                repo: repo_name.to_string(),
                                worker: worker_name.clone(),
                                pr_url: Some(pr_url.to_string()),
                            },
                        });

                        let auto_complete = repo_path
                            .as_ref()
                            .and_then(|p| JigToml::load(p).ok().flatten())
                            .map(|t| t.issues.auto_complete_on_merge)
                            .unwrap_or(false);
                        if auto_complete {
                            if let Some(issue_id) = new_state.issue_ref.as_ref() {
                                actions.push(DispatchAction::UpdateIssueStatus {
                                    issue_id: issue_id.to_string(),
                                });
                            }
                        }
                    }
                }
                PrStatus::Closed { .. } => {
                    actions.push(DispatchAction::Notify {
                        event: NotificationEvent::NeedsIntervention {
                            repo: repo_name.to_string(),
                            worker: worker_name.clone(),
                            reason: "PR closed without merge".to_string(),
                        },
                    });
                    if global_config.github.auto_cleanup_closed {
                        actions.push(DispatchAction::Cleanup);
                    }
                }
                PrStatus::Open {
                    pr_url,
                    is_draft: true,
                    checks,
                    review_feedback_count,
                } => {
                    current_review_feedback_count = Some(*review_feedback_count);

                    let stored_count = workers_state
                        .get_worker(key)
                        .and_then(|e| e.review_feedback_count);
                    let previous = stored_count.unwrap_or(0);
                    if *review_feedback_count > previous {
                        tracing::info!(
                            worker = key,
                            previous,
                            current = review_feedback_count,
                            "new review feedback detected, resetting review nudge count"
                        );
                        new_state.nudge_counts.remove("review");
                        actions.push(DispatchAction::Notify {
                            event: NotificationEvent::FeedbackReceived {
                                repo: repo_name.to_string(),
                                worker: worker_name.clone(),
                                pr_url: pr_url.to_string(),
                            },
                        });
                    }

                    let base = repo_path
                        .as_ref()
                        .and_then(|p| JigToml::load(p).ok().flatten())
                        .and_then(|t| t.worktree.base)
                        .unwrap_or_else(|| config::DEFAULT_BASE_BRANCH.to_string());

                    for check_name in checks.problems() {
                        let nkey = nudge_key_for_check(check_name);
                        let resolved = resolve(nkey);
                        let count = new_state.nudge_counts.get(nkey).copied().unwrap_or(0);
                        if count >= resolved.max {
                            continue;
                        }
                        if let Some(&last_ts) = new_state.last_nudge_at.get(nkey) {
                            let now = chrono::Utc::now().timestamp();
                            if now - last_ts < resolved.cooldown_seconds as i64 {
                                continue;
                            }
                        }
                        let mut prompt = Prompt::new(template_for_check(check_name))
                            .named(nkey)
                            .var_num("nudge_count", count + 1)
                            .var_num("max_nudges", resolved.max)
                            .var_bool("is_final_nudge", count + 1 >= resolved.max);

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
                _ => {}
            }
        }

        // Execute actions
        let mut prune_targets = Vec::new();
        for action in &actions {
            match action {
                DispatchAction::Nudge {
                    message,
                    nudge_key,
                    is_pr_nudge,
                } => {
                    let w = Worker::from_branch(
                        &repo_path.clone().unwrap_or_default(),
                        branch_name.as_str().into(),
                    );
                    if w.has_mux_window() {
                        if !is_pr_nudge && !w.is_agent_running() {
                            continue;
                        }
                        let prompt = Prompt::new(message).named(nudge_key);
                        match w.nudge(prompt) {
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
                    let window = TmuxWindow::new(
                        format!("{}{}", session_prefix, repo_name),
                        branch_name.to_string(),
                    );
                    if window.exists() {
                        if let Err(e) = window.kill() {
                            tracing::warn!("failed to kill window for {}: {}", worker_name, e);
                        }
                    }
                    if let Err(e) = event_log.append(&Event::now(EventKind::Terminal {
                        terminal: TerminalKind::Archived,
                        reason: None,
                    })) {
                        tracing::warn!("failed to emit cleanup event for {}: {}", key, e);
                    }
                    if let Some(rp) = &repo_path {
                        prune_targets.push(PruneTarget {
                            repo_path: rp.clone(),
                            repo_name: repo_name.to_string(),
                            worker_name: worker_name.clone(),
                        });
                    }
                }
                DispatchAction::Restart => {
                    if let Some(rp) = &repo_path {
                        match try_resume_worker(rp, &worker_name) {
                            Ok(true) => tracing::info!(worker = key, "worker resumed via restart"),
                            Ok(false) => {}
                            Err(e) => tracing::warn!(worker = key, "restart failed: {}", e),
                        }
                    }
                }
                DispatchAction::UpdateIssueStatus { issue_id } => {
                    if let Some(rp) = &repo_path {
                        if let Ok(ctx) = Config::from_path(rp) {
                            if let Ok(provider) = ctx.issue_provider() {
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
        }

        // Update workers state
        workers_state.set_worker(
            key,
            WorkerEntry {
                repo: repo_name.to_string(),
                branch: worker_name.clone(),
                status: new_state.status.as_str().to_string(),
                issue: new_state.issue_ref.as_ref().map(|r| r.to_string()),
                pr_url: new_state.pr_url.clone(),
                started_at: new_state.started_at.unwrap_or(0),
                last_event_at: new_state.last_event_at.unwrap_or(0),
                nudge_counts: new_state.nudge_counts.clone(),
                review_feedback_count: current_review_feedback_count,
                parent_branch: None,
            },
        );

        // Build display info
        let tmux_status = get_tmux_status(session_prefix, repo_name, &worker_name);
        let (commits_ahead, is_dirty) = if let Some(rp) = &repo_path {
            let worktree_path = config::worktree_path(rp, &worker_name);
            if worktree_path.exists() {
                let base = Config::resolve_base_branch_for(rp)
                    .unwrap_or_else(|_| config::DEFAULT_BASE_BRANCH.to_string());
                let ahead = jig_core::git::Repo::open(&worktree_path)
                    .and_then(|r| r.commits_ahead(&jig_core::git::Branch::new(&base)))
                    .unwrap_or_default()
                    .len();
                let dirty = jig_core::git::Repo::open(&worktree_path)
                    .and_then(|r| r.has_uncommitted_changes())
                    .unwrap_or(false);
                (ahead, dirty)
            } else {
                (0, false)
            }
        } else {
            (0, false)
        };

        let nudges_total: u32 = new_state.nudge_counts.values().sum();
        let nudge_cooldown_remaining = {
            let now = chrono::Utc::now().timestamp();
            let mut min_remaining: Option<u64> = None;
            for (nudge_key, &last_ts) in &new_state.last_nudge_at {
                let resolved = resolve(nudge_key);
                let elapsed = now - last_ts;
                if elapsed < resolved.cooldown_seconds as i64 {
                    let remaining = (resolved.cooldown_seconds as i64 - elapsed) as u64;
                    min_remaining =
                        Some(min_remaining.map_or(remaining, |cur: u64| cur.min(remaining)));
                }
            }
            min_remaining
        };

        let display = WorkerSnapshot {
            repo: repo_name.to_string(),
            name: worker_name.clone(),
            branch: branch_name,
            tmux_status,
            worker_status: Some(new_state.status),
            nudge_count: nudges_total,
            max_nudges: effective_health.max_nudges,
            commits_ahead,
            is_dirty,
            pr_url: new_state.pr_url.clone(),
            issue_ref: new_state.issue_ref.as_ref().map(|r| r.to_string()),
            pr_health: worker_tick_info,
            is_draft,
            nudge_cooldown_remaining,
        };

        Ok((display, prune_targets))
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

fn dispatch_actions<F>(
    repo_name: &str,
    worker_name: &str,
    old_state: &WorkerState,
    new_state: &WorkerState,
    resolve: F,
) -> Vec<DispatchAction>
where
    F: Fn(&str) -> ResolvedNudgeConfig,
{
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
            let resolved = resolve(nudge_key);
            let count = new_state.nudge_counts.get(nudge_key).copied().unwrap_or(0);

            let cooldown_ok = match new_state.last_nudge_at.get(nudge_key) {
                Some(&last_ts) => {
                    let elapsed = chrono::Utc::now().timestamp() - last_ts;
                    elapsed >= resolved.cooldown_seconds as i64
                }
                None => true,
            };

            if count < resolved.max && cooldown_ok {
                let mut prompt = Prompt::new(template)
                    .named(nudge_key)
                    .var_num("nudge_count", count + 1)
                    .var_num("max_nudges", resolved.max)
                    .var_bool("is_final_nudge", count + 1 >= resolved.max);

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

fn check_pr(worker: &Worker) -> PrInfo {
    let worker_key = worker.worker_key();
    let branch = worker.branch().to_string();

    let client = match GitHubClient::from_repo_path(worker.path()) {
        Ok(c) => c,
        Err(_) => {
            return PrInfo {
                status: PrStatus::Error {
                    pr_url: None,
                    error: "GitHub client unavailable".to_string(),
                },
            };
        }
    };

    let pr_url = match client.get_pr_for_branch(&branch) {
        Ok(Some(pr_info)) => match Url::parse(&pr_info.url) {
            Ok(url) => url,
            Err(_) => {
                return PrInfo {
                    status: PrStatus::Error {
                        pr_url: None,
                        error: format!("invalid PR URL: {}", pr_info.url),
                    },
                };
            }
        },
        Ok(None) => {
            return PrInfo {
                status: PrStatus::NoPr,
            }
        }
        Err(e) => {
            tracing::debug!(worker = %worker_key, error = %e, "PR discovery failed");
            return PrInfo {
                status: PrStatus::Error {
                    pr_url: None,
                    error: e.to_string(),
                },
            };
        }
    };

    let pr_number = match pr_url
        .path_segments()
        .and_then(|mut s| s.next_back())
        .and_then(|s| s.parse::<u64>().ok())
    {
        Some(n) => n,
        None => {
            return PrInfo {
                status: PrStatus::Error {
                    pr_url: Some(pr_url),
                    error: "could not parse PR number from URL".to_string(),
                },
            };
        }
    };

    let pr_state_info = match client.get_pr_state(pr_number) {
        Ok(s) => s,
        Err(e) => {
            return PrInfo {
                status: PrStatus::Error {
                    pr_url: Some(pr_url),
                    error: e.to_string(),
                },
            };
        }
    };

    let status = match pr_state_info.state {
        PrState::Merged => PrStatus::Merged { pr_url },
        PrState::Closed => PrStatus::Closed { pr_url },
        PrState::Open => {
            let mut checks = PrChecks::default();
            let mut review_feedback_count: u32 = 0;

            match github::check_ci(&client, &branch) {
                Ok(c) => checks.ci = Some(c.has_problem),
                Err(e) => tracing::debug!(error = %e, "CI check failed"),
            }
            match github::check_conflicts(&client, pr_number) {
                Ok(c) => checks.conflicts = Some(c.has_problem),
                Err(e) => tracing::debug!(error = %e, "conflicts check failed"),
            }
            match github::check_reviews(&client, pr_number) {
                Ok(c) => {
                    review_feedback_count = c.review_comment_count.unwrap_or(0)
                        + c.changes_requested_count.unwrap_or(0);
                    checks.reviews = Some(c.has_problem);
                }
                Err(e) => tracing::debug!(error = %e, "reviews check failed"),
            }
            match github::check_commits(&client, pr_number) {
                Ok(c) => checks.commits = Some(c.has_problem),
                Err(e) => tracing::debug!(error = %e, "commits check failed"),
            }

            PrStatus::Open {
                pr_url,
                is_draft: pr_state_info.is_draft,
                checks,
                review_feedback_count,
            }
        }
    };

    PrInfo { status }
}

fn entry_to_worker_state(entry: &WorkerEntry) -> WorkerState {
    let status = WorkerStatus::from_legacy(entry.status.as_str());
    WorkerState {
        status,
        pr_url: entry.pr_url.clone(),
        issue_ref: entry.issue.as_ref().map(|s| s.as_str().into()),
        nudge_counts: entry.nudge_counts.clone(),
        started_at: Some(entry.started_at),
        last_event_at: Some(entry.last_event_at),
        ..Default::default()
    }
}

fn make_nudge_resolver(
    repo_health: &RepoHealthConfig,
    global_health: &HealthConfig,
) -> impl Fn(&str) -> ResolvedNudgeConfig {
    let repo_health = repo_health.clone();
    let global_health = global_health.clone();
    move |key: &str| repo_health.resolve_for_nudge_type(key, &global_health)
}

fn find_repo_path<'r>(
    registry: &'r RepoRegistry,
    repo_name: &str,
) -> Option<&'r crate::config::registry::RepoEntry> {
    registry.repos().iter().find(|e| {
        e.path
            .file_name()
            .map(|n| n.to_string_lossy() == repo_name)
            .unwrap_or(false)
    })
}

fn try_resume_worker(
    repo_root: &std::path::Path,
    worker_name: &str,
) -> jig_core::error::Result<bool> {
    let worker = Worker::from_branch(repo_root, worker_name.into());
    if worker.has_mux_window() {
        return Ok(false);
    }
    let wt = worker.worktree()?;
    let jig_config = JigToml::load(repo_root)?.unwrap_or_default();
    let agent = jig_core::agents::Agent::from_name(&jig_config.agent.agent_type)
        .unwrap_or_else(|| jig_core::agents::Agent::from_kind(jig_core::agents::AgentKind::Claude))
        .with_disallowed_tools(jig_config.agent.disallowed_tools.clone());
    let prompt = Prompt::new(crate::worker::SPAWN_PREAMBLE).var(
        "task_context",
        "You were interrupted. Resume your previous task.",
    );
    Worker::resume(&wt, &agent, prompt)?;
    Ok(true)
}

fn get_tmux_status(session_prefix: &str, repo_name: &str, worker_name: &str) -> TmuxStatus {
    let session = TmuxSession::new(format!("{}{}", session_prefix, repo_name));
    if !session.exists() {
        return TmuxStatus::NoSession;
    }
    let window = session.window(worker_name);
    if !window.exists() {
        return TmuxStatus::NoWindow;
    }
    if window.is_running() {
        TmuxStatus::Running
    } else {
        TmuxStatus::Exited
    }
}

fn build_notifier(config: &GlobalConfig) -> Option<Notifier> {
    let queue_path = config::paths::notifications_path().ok()?;
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

    fn default_resolve(_key: &str) -> ResolvedNudgeConfig {
        ResolvedNudgeConfig {
            max: 3,
            cooldown_seconds: 300,
        }
    }

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

        let actions = dispatch_actions("repo", "test", &old, &new, default_resolve);
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

        let actions = dispatch_actions("repo", "test", &old, &new, default_resolve);
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

        let actions = dispatch_actions("repo", "test", &old, &new, default_resolve);
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

        let actions = dispatch_actions("repo", "test", &old, &new, default_resolve);
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

        let actions = dispatch_actions("repo", "test", &state, &state, default_resolve);
        assert!(actions.is_empty());
    }

    #[test]
    fn same_status_still_nudges() {
        let state = WorkerState {
            status: WorkerStatus::WaitingInput,
            ..Default::default()
        };

        let actions = dispatch_actions("repo", "test", &state, &state, default_resolve);
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

        let actions = dispatch_actions("repo", "test", &old, &new, default_resolve);
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

        let actions = dispatch_actions("repo", "test", &old, &new, default_resolve);
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(&actions[0], DispatchAction::Nudge { nudge_key, .. } if nudge_key == "idle")
        );
    }
}
