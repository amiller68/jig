//! Daemon loop — the conductor that ties event derivation, dispatch, and execution together.
//!
//! Runs a periodic loop:
//! 1. Drain actor responses (non-blocking)
//! 2. Trigger background sync if interval elapsed
//! 3. For each worker: read events → derive state → compare → dispatch actions
//! 4. Execute actions (nudge via tmux, notify via hooks)
//! 5. Save updated state
//! 6. Trigger issue poll for auto-spawn
//! 7. Auto-spawn eligible workers

mod discovery;
pub mod github_actor;
pub mod issue_actor;
pub mod lifecycle;
pub mod messages;
pub mod nudge_actor;
mod pr;
pub mod prune_actor;
pub mod recovery;
pub mod review_actor;
pub mod runtime;
pub mod spawn_actor;
pub mod sync_actor;
pub mod triage_actor;
pub mod triage_tracker;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::config::{JigToml, RepoHealthConfig, ResolvedNudgeConfig};
use crate::context::RepoContext;
use crate::dispatch::{dispatch_actions, Action, NotifyKind};
use crate::error::Result;
use crate::events::{Event, EventLog, EventType, WorkerState};
use crate::global::{GlobalConfig, HealthConfig, WorkerEntry, WorkersState};
use crate::host::tmux::{TmuxSession, TmuxWindow};
use crate::notify::{NotificationEvent, Notifier};
use crate::nudge::{build_nudge_context, NudgeType};
use crate::registry::{RepoEntry, RepoRegistry};
use crate::review::{latest_verdict, review_count, ReviewVerdict};
use crate::spawn::TaskStatus;
use crate::templates::TemplateEngine;
use crate::worker::WorkerStatus;

use discovery::discover_workers;
use pr::{make_github_client, PrMonitor};

pub use messages::SpawnableIssue;
pub use runtime::{DaemonRuntime, RuntimeConfig, TimerInfo};

/// Get the HEAD SHA for a worktree path via the project's git module.
fn head_sha_for(worktree_path: &std::path::Path) -> Option<String> {
    crate::git::Repo::open(worktree_path)
        .and_then(|r| r.head_oid())
        .ok()
        .map(|oid| oid.to_string())
}

/// Extract the branch name from a worker's event log (looks for Spawn event),
/// falling back to worker_name if no Spawn event exists.
fn extract_branch_name(events: &[Event], worker_name: &str) -> String {
    events
        .iter()
        .find(|e| e.event_type == EventType::Spawn)
        .and_then(|e| e.data.get("branch").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .unwrap_or_else(|| worker_name.to_string())
}

/// Pre-computed display data for a worker, populated during tick so the render
/// callback can format output without any subprocess calls or file I/O.
#[derive(Debug, Clone)]
pub struct WorkerDisplayInfo {
    pub repo: String,
    pub name: String,
    pub branch: String,
    pub tmux_status: TaskStatus,
    pub worker_status: Option<WorkerStatus>,
    pub nudge_count: u32,
    pub max_nudges: u32,
    pub commits_ahead: usize,
    pub is_dirty: bool,
    pub pr_url: Option<String>,
    pub issue_ref: Option<String>,
    pub pr_health: WorkerTickInfo,
    /// Whether the worker's PR is a draft (affects display and nudge behavior).
    pub is_draft: bool,
    /// Seconds until the next nudge cooldown expires (min across all active types).
    pub nudge_cooldown_remaining: Option<u64>,
}

/// Pre-computed display data for an in-flight triage subprocess.
#[derive(Debug, Clone)]
pub struct TriageDisplayInfo {
    /// Linear issue identifier (e.g. "JIG-77").
    pub issue_id: String,
    /// Triage model name (e.g. "sonnet").
    pub model: String,
    /// Seconds elapsed since the triage was spawned.
    pub elapsed_secs: u64,
    /// Repo name this triage belongs to.
    pub repo_name: String,
}

/// Per-worker PR health info collected during a tick.
#[derive(Debug, Clone, Default)]
pub struct WorkerTickInfo {
    /// Per-check outcomes: (check_name, has_problem).
    pub pr_checks: Vec<(String, bool)>,
    /// Error message if the GitHub client failed entirely.
    pub pr_error: Option<String>,
    /// Whether the worker has a PR at all.
    pub has_pr: bool,
}

/// Configuration for the daemon loop.
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    /// How often to poll, in seconds.
    pub interval_seconds: u64,
    /// Whether to run once and exit (vs. looping).
    pub once: bool,
    /// Tmux session prefix (default: "jig-").
    pub session_prefix: String,
    /// Skip `git fetch` on each tick (unused with actors — kept for API compat).
    pub skip_sync: bool,
    /// If set, only process workers for this repo name.
    pub repo_filter: Option<String>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            interval_seconds: 30,
            once: false,
            session_prefix: "jig-".to_string(),
            skip_sync: false,
            repo_filter: None,
        }
    }
}

/// Result of a single daemon tick.
#[derive(Debug, Default)]
pub struct TickResult {
    pub workers_checked: usize,
    pub actions_dispatched: usize,
    pub nudges_sent: usize,
    pub notifications_sent: usize,
    pub errors: Vec<String>,
    /// Per-worker PR health info, keyed by "repo/worker".
    pub worker_info: HashMap<String, WorkerTickInfo>,
    /// Issues auto-spawned this tick (completed).
    pub auto_spawned: Vec<String>,
    /// Worker names currently being spawned in the background.
    pub spawning: Vec<String>,
    /// Workers pruned (worktree removed) this tick.
    pub pruned: Vec<String>,
    /// Pre-computed display data for the render callback (zero I/O).
    pub worker_display: Vec<WorkerDisplayInfo>,
    /// Pre-computed display data for in-flight triages.
    pub triage_display: Vec<TriageDisplayInfo>,
    /// Nudge messages delivered this tick: (worker_name, nudge_type, message_text).
    pub nudge_messages: Vec<(String, String, String)>,
    /// Timer info for the daemon's sync and poll intervals.
    pub timer_info: Option<TimerInfo>,
}

/// The daemon orchestrator — holds references to shared infrastructure.
pub struct Daemon<'a> {
    config: &'a GlobalConfig,
    engine: &'a TemplateEngine<'a>,
    notifier: &'a Notifier,
    daemon_config: &'a DaemonConfig,
}

impl<'a> Daemon<'a> {
    pub fn new(
        config: &'a GlobalConfig,
        engine: &'a TemplateEngine<'a>,
        notifier: &'a Notifier,
        daemon_config: &'a DaemonConfig,
    ) -> Self {
        Self {
            config,
            engine,
            notifier,
            daemon_config,
        }
    }

    /// Load per-repo health config from jig.toml, falling back to defaults.
    fn load_repo_health_config(registry: &RepoRegistry, repo_name: &str) -> RepoHealthConfig {
        Self::find_repo_path(registry, repo_name)
            .and_then(|entry| JigToml::load(&entry.path).ok().flatten())
            .map(|toml| toml.health)
            .unwrap_or_default()
    }

    /// Build a resolver closure for nudge type config resolution.
    fn make_nudge_resolver(
        repo_health: &RepoHealthConfig,
        global_health: &HealthConfig,
    ) -> impl Fn(&str) -> ResolvedNudgeConfig {
        let repo_health = repo_health.clone();
        let global_health = global_health.clone();
        move |key: &str| repo_health.resolve_for_nudge_type(key, &global_health)
    }

    /// Build a HealthConfig with per-repo silence threshold applied.
    fn effective_health_config(
        repo_health: &RepoHealthConfig,
        global_health: &HealthConfig,
    ) -> HealthConfig {
        HealthConfig {
            silence_threshold_seconds: repo_health.resolve_silence_threshold(global_health),
            max_nudges: repo_health.resolve_max_nudges(global_health),
        }
    }

    /// Look up the repo path from the registry by repo name.
    fn find_repo_path<'r>(registry: &'r RepoRegistry, repo_name: &str) -> Option<&'r RepoEntry> {
        registry.repos().iter().find(|e| {
            e.path
                .file_name()
                .map(|n| n.to_string_lossy() == repo_name)
                .unwrap_or(false)
        })
    }

    /// Get tmux status for a worker (session:window alive check).
    fn get_tmux_status(&self, repo_name: &str, worker_name: &str) -> TaskStatus {
        let session = TmuxSession::new(format!(
            "{}{}",
            self.daemon_config.session_prefix, repo_name
        ));
        if !session.exists() {
            return TaskStatus::NoSession;
        }
        let window = session.window(worker_name);
        if !window.exists() {
            return TaskStatus::NoWindow;
        }
        if window.is_running() {
            TaskStatus::Running
        } else {
            TaskStatus::Exited
        }
    }

    /// Returns `true` if a non-terminal worker already exists for the given issue ID.
    ///
    /// Used as a migration guard: when the daemon is about to spawn a parent
    /// worker for wrap-up (all children complete), this check prevents
    /// double-spawning if an old-model parent worker is still active.
    pub fn has_active_parent_worker(workers_state: &WorkersState, issue_id: &str) -> bool {
        workers_state.workers.values().any(|entry| {
            let is_terminal =
                entry.status == "merged" || entry.status == "archived" || entry.status == "failed";
            !is_terminal && entry.issue.as_deref() == Some(issue_id)
        })
    }

    /// Collect parent branches from active workers that need to be fetched during sync.
    ///
    /// Returns (repo_name, repo_path, branch) tuples for each unique parent branch.
    fn collect_parent_branches(
        &self,
        workers_state: &WorkersState,
        registry: &RepoRegistry,
    ) -> Vec<(String, std::path::PathBuf, String)> {
        let mut seen = HashSet::new();
        let mut result = Vec::new();

        for entry in workers_state.workers.values() {
            if let Some(ref parent_branch) = entry.parent_branch {
                let repo_key = format!("{}:{}", entry.repo, parent_branch);
                if seen.insert(repo_key) {
                    if let Some(repo_entry) = Self::find_repo_path(registry, &entry.repo) {
                        result.push((
                            entry.repo.clone(),
                            repo_entry.path.clone(),
                            parent_branch.clone(),
                        ));
                    }
                }
            }
        }

        result
    }

    /// After sync completes, fast-forward parent branches to match their remote.
    ///
    /// For each parent branch (a branch used as the base branch by child workers):
    /// - If a parent worktree exists → fast-forward via checkout (existing behavior).
    /// - If no parent worktree exists → fast-forward the local branch ref directly
    ///   via git2, then push to origin so other daemons/repos stay in sync.
    ///
    /// If new commits were pulled and a worktree exists, nudge the parent worker.
    fn update_parent_worktrees(
        &self,
        workers_state: &WorkersState,
        registry: &RepoRegistry,
        runtime: &DaemonRuntime,
    ) {
        // Build a set of parent branches that child workers depend on.
        let mut parent_branches: HashSet<(String, String)> = HashSet::new();
        for entry in workers_state.workers.values() {
            if entry.status == "merged" || entry.status == "archived" || entry.status == "failed" {
                continue;
            }
            if let Some(ref pb) = entry.parent_branch {
                parent_branches.insert((entry.repo.clone(), pb.clone()));
            }
        }

        if parent_branches.is_empty() {
            return;
        }

        // Find parent workers by matching branch name from workers_state
        // (avoids re-reading event logs).
        for (repo_name, parent_branch) in &parent_branches {
            let parent_worker = workers_state.workers.iter().find_map(|(key, entry)| {
                if &entry.repo == repo_name && entry.branch == *parent_branch {
                    // Extract worker name from the key (format: "repo/worker")
                    let worker_name = key.split('/').nth(1).unwrap_or(key);
                    Some((worker_name.to_string(), entry.branch.clone()))
                } else {
                    None
                }
            });

            let (worker_name, branch_name) = match parent_worker {
                Some(pw) => pw,
                None => continue,
            };

            let repo_entry = match Self::find_repo_path(registry, repo_name) {
                Some(e) => e,
                None => continue,
            };

            let worktree_path = crate::config::worktree_path(&repo_entry.path, &worker_name);
            let worktree_exists = worktree_path.exists();

            if worktree_exists {
                // Worktree exists: fast-forward via checkout (original path).
                let repo = match crate::git::Repo::open(&worktree_path) {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!(
                            worker = %worker_name,
                            repo = %repo_name,
                            "failed to open parent worktree repo: {}",
                            e
                        );
                        continue;
                    }
                };

                match repo.fast_forward_branch(&parent_branch.as_str().into(), true) {
                    Ok(true) => {
                        tracing::info!(
                            worker = %worker_name,
                            repo = %repo_name,
                            branch = %parent_branch,
                            "pulled new commits into parent worktree"
                        );

                        // Nudge the parent worker to let it know about new commits
                        let session_name =
                            format!("{}{}", self.daemon_config.session_prefix, repo_name);
                        let window = TmuxWindow::new(&session_name, &worker_name);

                        if window.exists() {
                            let key = format!("{}/{}", repo_name, worker_name);
                            let message = "Child work has been merged into your branch. \
                                           New commits are available. Run `git log --oneline -5` \
                                           to see what changed."
                                .to_string();

                            runtime.send_nudge(messages::NudgeRequest {
                                session: session_name,
                                window: branch_name,
                                message,
                                nudge_type_key: "parent_update".to_string(),
                                is_stuck: false,
                                repo_name: repo_name.to_string(),
                                worker_name: worker_name.to_string(),
                                worker_key: key,
                            });
                        }
                    }
                    Ok(false) => {
                        tracing::debug!(
                            worker = %worker_name,
                            repo = %repo_name,
                            "parent worktree already up to date"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            worker = %worker_name,
                            repo = %repo_name,
                            "fast-forward failed in parent worktree: {}",
                            e
                        );
                    }
                }
            } else {
                // No worktree: fast-forward the local branch ref directly from
                // the main repo, then push to origin.
                let repo = match crate::git::Repo::open(&repo_entry.path) {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!(
                            repo = %repo_name,
                            branch = %parent_branch,
                            "failed to open main repo for bare branch update: {}",
                            e
                        );
                        continue;
                    }
                };

                match repo.fast_forward_branch(&parent_branch.as_str().into(), false) {
                    Ok(true) => {
                        tracing::info!(
                            repo = %repo_name,
                            branch = %parent_branch,
                            "fast-forwarded parent branch ref (no worktree)"
                        );

                        // Push to origin so remote stays in sync
                        if let Err(e) = repo.push_branch(&parent_branch.as_str().into()) {
                            tracing::warn!(
                                repo = %repo_name,
                                branch = %parent_branch,
                                "push after bare fast-forward failed: {}",
                                e
                            );
                        }
                    }
                    Ok(false) => {
                        tracing::debug!(
                            repo = %repo_name,
                            branch = %parent_branch,
                            "parent branch ref already up to date (no worktree)"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            repo = %repo_name,
                            branch = %parent_branch,
                            "bare fast-forward failed for parent branch: {}",
                            e
                        );
                    }
                }
            }
        }
    }

    /// Execute a single tick of the daemon using actor-based runtime.
    /// If `quit` is set, the tick will bail early between workers.
    pub fn tick(&self, runtime: &mut DaemonRuntime, quit: &AtomicBool) -> Result<TickResult> {
        let mut result = TickResult::default();

        // Load current global state (previous worker states)
        let mut workers_state = WorkersState::load().unwrap_or_default();

        // Discover workers from repo registry (before draining actors so
        // existing_workers is available for the inline first poll)
        let registry = RepoRegistry::load().unwrap_or_default();

        let mut worker_list = discover_workers(&registry);

        // Filter to single repo if configured
        if let Some(ref filter) = self.daemon_config.repo_filter {
            worker_list.retain(|(repo_name, _)| repo_name == filter);
        }

        tracing::debug!(count = worker_list.len(), "discovered workers");

        // 1. Drain all pending actor responses (non-blocking)
        runtime.drain_sync();

        // Parent-update phase: after sync, check if parent worktrees have new
        // remote commits (from child PR merges) and pull them in.
        self.update_parent_worktrees(&workers_state, &registry, runtime);

        runtime.drain_github();
        let issue_response = runtime.drain_issues();
        let mut spawnable = issue_response
            .as_ref()
            .map(|r| r.spawnable.clone())
            .unwrap_or_default();
        let mut triageable = issue_response
            .as_ref()
            .map(|r| r.triageable.clone())
            .unwrap_or_default();
        let mut wrapup = issue_response
            .as_ref()
            .map(|r| r.wrapup.clone())
            .unwrap_or_default();

        // Accumulate parent branch results from both async and inline-poll
        // paths so the sync actor can fetch their tracking refs.
        let mut parent_branch_results: Vec<messages::ParentBranchResult> = issue_response
            .as_ref()
            .map(|r| r.parent_branches.clone())
            .unwrap_or_default();

        // Log parent integration branch results from the issue actor.
        for pb in &parent_branch_results {
            if let Some(ref err) = pb.error {
                tracing::warn!(
                    repo = %pb.repo_name,
                    issue = %pb.issue_id,
                    branch = %pb.branch_name,
                    "parent branch error: {}", err
                );
            }
        }

        // First-tick inline poll: run issue poll synchronously so that spawn
        // can happen in the same tick instead of waiting 3 ticks.
        //
        // Repo isolation: `filtered_repos` respects `repo_filter`, so when
        // `jig ps -w` runs within a single repo only that repo is polled.
        // Workers are never spawned for repos outside the filter scope.
        if spawnable.is_empty() && runtime.should_first_poll() {
            runtime.mark_first_poll_done();

            let repos: Vec<(std::path::PathBuf, String)> = registry
                .filtered_repos(self.daemon_config.repo_filter.as_deref())
                .into_iter()
                .map(|entry| {
                    let base = RepoContext::resolve_base_branch_for(&entry.path)
                        .unwrap_or_else(|_| crate::config::DEFAULT_BASE_BRANCH.to_string());
                    (entry.path.clone(), base)
                })
                .collect();

            if !repos.is_empty() {
                let req = messages::IssueRequest {
                    repos,
                    existing_workers: worker_list.clone(),
                };
                let response = issue_actor::process_request(&req);
                spawnable = response.spawnable;
                triageable = response.triageable;
                wrapup = response.wrapup;
                // Log parent branch results from inline poll
                for pb in &response.parent_branches {
                    if let Some(ref err) = pb.error {
                        tracing::warn!(
                            repo = %pb.repo_name,
                            issue = %pb.issue_id,
                            branch = %pb.branch_name,
                            "parent branch error: {}", err
                        );
                    }
                }
                parent_branch_results.extend(response.parent_branches);
                if !spawnable.is_empty() {
                    tracing::info!(
                        count = spawnable.len(),
                        "first-tick inline issue poll found spawnable issues"
                    );
                }
                if !triageable.is_empty() {
                    tracing::info!(
                        count = triageable.len(),
                        "first-tick inline issue poll found triageable issues"
                    );
                }
                if !wrapup.is_empty() {
                    tracing::info!(
                        count = wrapup.len(),
                        "first-tick inline issue poll found wrapup parents"
                    );
                }
            }
        }

        // Triage: filter out issues already tracked, register new ones, detect stuck
        {
            let now = chrono::Utc::now().timestamp();

            // Filter triageable issues to those not already being triaged
            triageable.retain(|issue| !runtime.triage_tracker().is_active(&issue.issue.id));

            // Stuck triage detection: check each active triage against its
            // repo's configured timeout (from [triage] timeout_seconds).
            let stuck_ids: Vec<(String, String, String)> = {
                let mut stuck = Vec::new();
                for entry in runtime.triage_tracker().stuck_entries() {
                    // Load per-repo triage timeout
                    let timeout = Self::find_repo_path(&registry, &entry.repo_name)
                        .and_then(|re| JigToml::load(&re.path).ok().flatten())
                        .map(|toml| toml.triage.timeout_seconds)
                        .unwrap_or(600);
                    if now - entry.spawned_at > timeout {
                        stuck.push((
                            entry.issue_id.clone(),
                            entry.worker_name.clone(),
                            entry.repo_name.clone(),
                        ));
                    }
                }
                stuck
            };

            for (issue_id, worker_name, repo_name) in &stuck_ids {
                tracing::warn!(
                    issue = %issue_id,
                    worker = %worker_name,
                    "triage timed out, emitting NeedsIntervention"
                );
                let event = NotificationEvent::NeedsIntervention {
                    repo: repo_name.clone(),
                    worker: worker_name.clone(),
                    reason: format!(
                        "Triage timed out for {} (worker: {})",
                        issue_id, worker_name
                    ),
                };
                if let Err(e) = self.notifier.emit(event) {
                    tracing::warn!(
                        issue = %issue_id,
                        "NeedsIntervention notification failed: {}", e
                    );
                }

                // Triage subprocesses have no tmux window to kill — the
                // tracker entry is cleared so a fresh triage can be dispatched
                // on a later tick. In-flight subprocess is owned by the
                // triage_actor and will be reported via drain_triage().
                runtime.triage_tracker_mut().remove(issue_id);
            }

            // Triage worker completion is now handled by drain_triage() above —
            // the triage_actor reports results directly when subprocesses finish.
        }

        // Drain nudge completions from previous tick
        for nudge_result in runtime.drain_nudges() {
            if let Some(err) = nudge_result.error {
                tracing::warn!(
                    worker = %nudge_result.worker_key,
                    nudge_type = %nudge_result.nudge_type_key,
                    "nudge delivery error: {}",
                    err
                );
            }
        }

        // Drain review completions from previous tick
        for review_result in runtime.drain_reviews() {
            if let Some(err) = review_result.error {
                tracing::warn!(
                    worker = %review_result.worker_key,
                    "review failed: {}", err
                );
                continue;
            }

            let worker_key = &review_result.worker_key;

            // Resolve worktree path from worker_key ("repo/worker")
            let (rname, wname) = match worker_key.split_once('/') {
                Some(pair) => pair,
                None => {
                    tracing::warn!(worker = %worker_key, "invalid worker key in review result");
                    continue;
                }
            };
            let worktree_path = match Self::find_repo_path(&registry, rname) {
                Some(entry) => crate::config::worktree_path(&entry.path, wname),
                None => {
                    tracing::warn!(worker = %worker_key, "repo not found for review result");
                    continue;
                }
            };

            // Get current HEAD SHA
            let head_sha = head_sha_for(&worktree_path);
            let verdict = latest_verdict(&worktree_path);

            match verdict {
                Some(ReviewVerdict::Approve) => {
                    // Mark PR ready for review
                    if let Some(cached) = runtime.get_cached_pr(worker_key) {
                        if let Some(ref pr_url) = cached.pr_url {
                            if let Some(pr_number) = pr_url.rsplit('/').next() {
                                let repo_path =
                                    Self::find_repo_path(&registry, rname).map(|e| e.path.clone());
                                if let Some(rp) = repo_path {
                                    let output = std::process::Command::new("gh")
                                        .args(["pr", "ready", pr_number])
                                        .current_dir(&rp)
                                        .stdin(std::process::Stdio::null())
                                        .output();
                                    match output {
                                        Ok(o) if o.status.success() => {
                                            tracing::info!(
                                                worker = %worker_key,
                                                pr = %pr_number,
                                                "marked PR ready for review"
                                            );
                                        }
                                        Ok(o) => {
                                            tracing::warn!(
                                                worker = %worker_key,
                                                "gh pr ready failed: {}",
                                                String::from_utf8_lossy(&o.stderr)
                                            );
                                        }
                                        Err(e) => {
                                            tracing::warn!(
                                                worker = %worker_key,
                                                "gh pr ready error: {}", e
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Update issue status to "In Review" if issue_ref is set
                    if let Some(entry) = workers_state.get_worker(worker_key) {
                        if let Some(ref issue_id) = entry.issue {
                            let output = std::process::Command::new("jig")
                                .args(["issues", "status", issue_id, "--status", "in-review"])
                                .stdin(std::process::Stdio::null())
                                .output();
                            match output {
                                Ok(o) if o.status.success() => {
                                    tracing::info!(
                                        worker = %worker_key,
                                        issue = %issue_id,
                                        "updated issue status to in-review"
                                    );
                                }
                                Ok(o) => {
                                    tracing::warn!(
                                        worker = %worker_key,
                                        "issue status update failed: {}",
                                        String::from_utf8_lossy(&o.stderr)
                                    );
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        worker = %worker_key,
                                        "issue status update error: {}", e
                                    );
                                }
                            }
                        }
                    }

                    // Emit notification
                    let pr_url = runtime
                        .get_cached_pr(worker_key)
                        .and_then(|c| c.pr_url.clone());
                    let event = NotificationEvent::ReviewApproved {
                        repo: rname.to_string(),
                        worker: wname.to_string(),
                        pr_url,
                    };
                    if let Err(e) = self.notifier.emit(event) {
                        tracing::warn!(
                            worker = %worker_key,
                            "ReviewApproved notification failed: {}", e
                        );
                    }
                }
                Some(ReviewVerdict::ChangesRequested) => {
                    // Dispatch AutoReview nudge to the implementation agent
                    let session_name = format!("{}{}", self.daemon_config.session_prefix, rname);
                    let branch = workers_state
                        .get_worker(worker_key)
                        .map(|e| e.branch.clone())
                        .unwrap_or_else(|| wname.to_string());
                    let window = TmuxWindow::new(&session_name, &branch);

                    if window.exists() {
                        // Load review config to get max_rounds
                        let review_cfg = Self::find_repo_path(&registry, rname)
                            .and_then(|entry| JigToml::load(&entry.path).ok().flatten())
                            .map(|t| t.review)
                            .unwrap_or_default();

                        let round = review_count(&worktree_path);
                        let max_rounds = review_cfg.max_rounds;

                        // Build nudge context with review-specific fields
                        let repo_health = Self::load_repo_health_config(&registry, rname);
                        let resolve = Self::make_nudge_resolver(&repo_health, &self.config.health);
                        let resolved = resolve(NudgeType::AutoReview.count_key());

                        let event_log_result = EventLog::for_worker(rname, wname);
                        let effective_health =
                            Self::effective_health_config(&repo_health, &self.config.health);

                        if let Ok(event_log) = event_log_result {
                            if let Ok(events) = event_log.read_all() {
                                let state = WorkerState::reduce(&events, &effective_health);
                                let mut ctx = build_nudge_context(
                                    NudgeType::AutoReview,
                                    &state,
                                    resolved,
                                    None,
                                );
                                ctx.set_num("review_round", round);
                                ctx.set_num("max_rounds", max_rounds);
                                ctx.set_bool("is_final_round", round >= max_rounds);

                                // Find the latest review file number
                                let review_file = format!("{:03}.md", round);
                                ctx.set("review_file", &review_file);
                                ctx.set_num("review_number", round);

                                let message = match self
                                    .engine
                                    .render(NudgeType::AutoReview.template_name(), &ctx)
                                {
                                    Ok(msg) => msg,
                                    Err(e) => {
                                        tracing::warn!(
                                            worker = %worker_key,
                                            "auto-review nudge template render failed: {}", e
                                        );
                                        // Update last_reviewed_sha even on template failure
                                        if let Some(sha) = &head_sha {
                                            if let Some(entry) =
                                                workers_state.workers.get_mut(worker_key)
                                            {
                                                entry.last_reviewed_sha = Some(sha.clone());
                                            }
                                        }
                                        continue;
                                    }
                                };

                                runtime.send_nudge(messages::NudgeRequest {
                                    session: session_name,
                                    window: branch,
                                    message,
                                    nudge_type_key: NudgeType::AutoReview.count_key().to_string(),
                                    is_stuck: false,
                                    repo_name: rname.to_string(),
                                    worker_name: wname.to_string(),
                                    worker_key: worker_key.to_string(),
                                });
                            }
                        }
                    }
                }
                None => {
                    tracing::warn!(
                        worker = %worker_key,
                        "review completed but no verdict file found"
                    );
                }
            }

            // Update last_reviewed_sha in WorkerEntry
            if let Some(sha) = head_sha {
                if let Some(entry) = workers_state.workers.get_mut(worker_key) {
                    entry.last_reviewed_sha = Some(sha);
                }
            }
        }

        // Drain prune results from previous tick
        if let Some(prune_complete) = runtime.drain_prune() {
            for pr in prune_complete.results {
                if let Some(err) = pr.error {
                    result.errors.push(format!("prune {}: {}", pr.key, err));
                } else {
                    result.pruned.push(pr.key);
                }
            }
        }

        // Drain spawn results from previous tick
        if let Some(spawn_complete) = runtime.drain_spawn() {
            for sr in spawn_complete.results {
                if let Some(err) = sr.error {
                    result
                        .errors
                        .push(format!("auto-spawn {}: {}", sr.worker_name, err));
                } else {
                    // Emit WorkStarted notification for successfully spawned workers
                    let event = NotificationEvent::WorkStarted {
                        repo: sr.repo_name.clone(),
                        worker: sr.worker_name.clone(),
                        issue: sr.issue_id.clone(),
                    };
                    if let Err(e) = self.notifier.emit(event) {
                        tracing::warn!(
                            worker = %sr.worker_name,
                            "WorkStarted notification failed: {}", e
                        );
                    }
                    result.auto_spawned.push(sr.worker_name);
                }
            }
        }

        // Drain triage results from previous tick
        if let Some(triage_complete) = runtime.drain_triage() {
            for tr in triage_complete.results {
                // Remove from tracker regardless of success/failure
                runtime.triage_tracker_mut().remove(&tr.issue_id);

                if let Some(err) = tr.error {
                    tracing::warn!(
                        worker = %tr.worker_name,
                        issue = %tr.issue_id,
                        "triage failed: {}", err
                    );
                    // Emit NeedsIntervention for failed triages
                    let event = NotificationEvent::NeedsIntervention {
                        repo: tr.repo_name.clone(),
                        worker: tr.worker_name.clone(),
                        reason: format!(
                            "Triage failed for {} (worker: {}): {}",
                            tr.issue_id, tr.worker_name, err
                        ),
                    };
                    if let Err(e) = self.notifier.emit(event) {
                        tracing::warn!(
                            worker = %tr.worker_name,
                            "NeedsIntervention notification failed: {}", e
                        );
                    }
                    result
                        .errors
                        .push(format!("triage {}: {}", tr.issue_id, err));
                } else {
                    tracing::info!(
                        worker = %tr.worker_name,
                        issue = %tr.issue_id,
                        "triage completed successfully"
                    );
                }
            }
        }

        // 2. Trigger background sync if interval elapsed
        if !self.daemon_config.skip_sync {
            let mut parent_branches = self.collect_parent_branches(&workers_state, &registry);

            // Also include parent branches from the issue actor response.
            // This closes the chicken-and-egg gap: ensure_parent_branches
            // creates/pushes parent branches before any child worker exists,
            // so collect_parent_branches (which only looks at active workers)
            // won't include them. Without this, the tracking ref
            // (refs/remotes/origin/{branch}) never gets populated by fetch,
            // and remote_branch_exists returns false indefinitely.
            {
                let mut seen: HashSet<String> = parent_branches
                    .iter()
                    .map(|(name, _, branch)| format!("{}:{}", name, branch))
                    .collect();
                for pb in &parent_branch_results {
                    if pb.error.is_none() {
                        let key = format!("{}:{}", pb.repo_name, pb.branch_name);
                        if seen.insert(key) {
                            parent_branches.push((
                                pb.repo_name.clone(),
                                pb.repo_root.clone(),
                                pb.branch_name.clone(),
                            ));
                        }
                    }
                }
            }

            runtime.maybe_trigger_sync(
                &registry,
                self.daemon_config.repo_filter.as_deref(),
                parent_branches,
            );
        }

        // 3. Process each worker
        let mut live_prune_targets = Vec::new();
        for (repo_name, worker_name) in &worker_list {
            if quit.load(Ordering::Relaxed) {
                break;
            }
            result.workers_checked += 1;
            let key = format!("{}/{}", repo_name, worker_name);

            match self.process_worker(
                repo_name,
                worker_name,
                &key,
                &mut workers_state,
                &registry,
                runtime,
            ) {
                Ok((
                    actions,
                    nudges,
                    notifs,
                    worker_tick_info,
                    display_info,
                    prune_targets,
                    nudge_msgs,
                )) => {
                    result.actions_dispatched += actions;
                    result.nudges_sent += nudges;
                    result.notifications_sent += notifs;
                    result.worker_info.insert(key.clone(), worker_tick_info);
                    result.worker_display.push(display_info);
                    live_prune_targets.extend(prune_targets);
                    for (ntype, msg) in nudge_msgs {
                        result
                            .nudge_messages
                            .push((worker_name.clone(), ntype, msg));
                    }
                }
                Err(e) => {
                    result.errors.push(format!("{}: {}", key, e));
                }
            }
        }

        // Live path: send prune targets from Cleanup actions
        if !live_prune_targets.is_empty() {
            runtime.send_prune(live_prune_targets);
        }

        // Filter out terminal workers and workers with no tmux session
        result.worker_display.retain(|w| {
            let is_terminal = w
                .worker_status
                .as_ref()
                .map(|s| s.is_terminal())
                .unwrap_or(false);
            let tmux_dead = matches!(w.tmux_status, TaskStatus::NoSession | TaskStatus::NoWindow);
            !is_terminal && !tmux_dead
        });
        result.worker_display.sort_by(|a, b| a.name.cmp(&b.name));

        // Build triage display from tracker
        {
            let now = chrono::Utc::now().timestamp();
            for entry in runtime.triage_tracker().active_entries() {
                let model = Self::find_repo_path(&registry, &entry.repo_name)
                    .and_then(|re| JigToml::load(&re.path).ok().flatten())
                    .map(|toml| toml.triage.model.clone())
                    .unwrap_or_else(|| "sonnet".to_string());
                let elapsed = (now - entry.spawned_at).max(0) as u64;
                result.triage_display.push(TriageDisplayInfo {
                    issue_id: entry.issue_id.clone(),
                    model,
                    elapsed_secs: elapsed,
                    repo_name: entry.repo_name.clone(),
                });
            }
            result
                .triage_display
                .sort_by(|a, b| a.issue_id.cmp(&b.issue_id));
        }

        // Save updated state
        workers_state.save().unwrap_or_else(|e| {
            tracing::warn!("failed to save workers state: {}", e);
        });

        // Recovery path: scan github cache for merged/closed PRs with worktrees still on disk.
        // This catches workers whose PRs were merged/closed while the daemon was off.
        if !runtime.prune_pending() {
            let mut prune_targets = Vec::new();
            for (repo_name, worker_name) in &worker_list {
                let key = format!("{}/{}", repo_name, worker_name);
                if let Some(cached) = runtime.get_cached_pr(&key) {
                    if cached.pr_merged || cached.pr_closed {
                        if let Some(entry) = Self::find_repo_path(&registry, repo_name) {
                            let worktree_path =
                                crate::config::worktree_path(&entry.path, worker_name);
                            if worktree_path.exists() {
                                prune_targets.push(messages::PruneTarget {
                                    repo_path: entry.path.clone(),
                                    repo_name: repo_name.clone(),
                                    worker_name: worker_name.clone(),
                                });
                            }
                        }
                    }
                }
            }
            runtime.send_prune(prune_targets);
        }

        // 4. Trigger issue poll if auto-spawn enabled (scoped to repo_filter)
        runtime.maybe_trigger_issue_poll(
            &registry,
            &worker_list,
            self.daemon_config.repo_filter.as_deref(),
        );

        // 5. Send spawnable issues to background spawn actor (non-blocking).
        //    Wrap-up parents are dispatched through the same actor. Skip any
        //    wrap-up whose issue already has an active worker (old-model
        //    migration guard — prevents double-spawning).
        if !wrapup.is_empty() {
            wrapup.retain(|si| {
                let active = Self::has_active_parent_worker(&workers_state, &si.issue.id);
                if active {
                    tracing::info!(
                        issue = %si.issue.id,
                        worker = %si.worker_name,
                        "skipping wrap-up spawn: active worker already exists for issue"
                    );
                }
                !active
            });
            if !wrapup.is_empty() {
                spawnable.extend(wrapup);
            }
        }
        if !spawnable.is_empty() {
            runtime.send_spawn(spawnable);
        }

        // 6. Send triageable issues to background triage actor (subprocess, non-blocking).
        //    The issue actor now emits `TriageIssue` directly, so no conversion
        //    from `SpawnableIssue` is needed — triageable flows through its own
        //    channel, never through `spawn_tx`. Duplicate prevention is handled
        //    by the `is_active` filter above plus `TriageTracker` registration
        //    here on the triage-routing path.
        if !triageable.is_empty() {
            let now = chrono::Utc::now().timestamp();
            for ti in &triageable {
                let repo_name = ti
                    .repo_root
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                runtime.triage_tracker_mut().register(
                    ti.issue.id.clone(),
                    triage_tracker::TriageEntry {
                        worker_name: ti.worker_name.clone(),
                        spawned_at: now,
                        issue_id: ti.issue.id.clone(),
                        repo_name,
                    },
                );
            }
            runtime.send_triage(triageable);
        }

        result.spawning = runtime.spawning_workers().to_vec();
        result.timer_info = Some(runtime.timer_info());

        Ok(result)
    }

    /// Execute a single tick without a runtime (legacy path for non-watch mode).
    pub fn tick_once(&self) -> Result<TickResult> {
        let mut result = TickResult::default();

        let mut workers_state = WorkersState::load().unwrap_or_default();
        let registry = RepoRegistry::load().unwrap_or_default();

        if !self.daemon_config.skip_sync {
            self.sync_repos(&registry);
        }

        let mut worker_list = discover_workers(&registry);

        if let Some(ref filter) = self.daemon_config.repo_filter {
            worker_list.retain(|(repo_name, _)| repo_name == filter);
        }

        tracing::debug!(count = worker_list.len(), "discovered workers");

        for (repo_name, worker_name) in &worker_list {
            result.workers_checked += 1;
            let key = format!("{}/{}", repo_name, worker_name);

            match self.process_worker_blocking(
                repo_name,
                worker_name,
                &key,
                &mut workers_state,
                &registry,
            ) {
                Ok((actions, nudges, notifs, worker_tick_info)) => {
                    result.actions_dispatched += actions;
                    result.nudges_sent += nudges;
                    result.notifications_sent += notifs;
                    result.worker_info.insert(key.clone(), worker_tick_info);
                }
                Err(e) => {
                    result.errors.push(format!("{}: {}", key, e));
                }
            }
        }

        workers_state.save().unwrap_or_else(|e| {
            tracing::warn!("failed to save workers state: {}", e);
        });

        // Auto-spawn: poll repos for spawnable issues (blocking).
        // Each repo's jig.toml controls auto_spawn and max_concurrent_workers.
        // When repo_filter is set, only poll that repo.
        {
            let repos: Vec<(std::path::PathBuf, String)> = registry
                .filtered_repos(self.daemon_config.repo_filter.as_deref())
                .into_iter()
                .map(|entry| {
                    let base = RepoContext::resolve_base_branch_for(&entry.path)
                        .unwrap_or_else(|_| crate::config::DEFAULT_BASE_BRANCH.to_string());
                    (entry.path.clone(), base)
                })
                .collect();

            if !repos.is_empty() {
                let req = messages::IssueRequest {
                    repos,
                    existing_workers: worker_list.clone(),
                };

                let response = issue_actor::process_request(&req);
                // Spawn normal issues
                for issue in response.spawnable {
                    match self.auto_spawn_worker(&issue) {
                        Ok(()) => {
                            tracing::info!(
                                worker = %issue.worker_name,
                                issue = %issue.issue.id,
                                "auto-spawned worker"
                            );
                            result.auto_spawned.push(issue.worker_name.clone());
                        }
                        Err(e) => {
                            result
                                .errors
                                .push(format!("auto-spawn {}: {}", issue.issue.id, e));
                        }
                    }
                }
                // Spawn wrap-up parents (same codepath as normal spawn, but
                // skipped if a worker already exists for the parent — old-model
                // migration guard).
                for issue in response.wrapup {
                    if Self::has_active_parent_worker(&workers_state, &issue.issue.id) {
                        tracing::info!(
                            issue = %issue.issue.id,
                            worker = %issue.worker_name,
                            "skipping wrap-up spawn: active worker already exists for issue"
                        );
                        continue;
                    }
                    match self.auto_spawn_worker(&issue) {
                        Ok(()) => {
                            tracing::info!(
                                worker = %issue.worker_name,
                                issue = %issue.issue.id,
                                "spawned wrap-up worker for parent"
                            );
                            result.auto_spawned.push(issue.worker_name.clone());
                        }
                        Err(e) => {
                            result
                                .errors
                                .push(format!("wrapup-spawn {}: {}", issue.issue.id, e));
                        }
                    }
                }
                // Run triage issues as direct subprocesses (blocking)
                for issue in response.triageable {
                    tracing::info!(
                        worker = %issue.worker_name,
                        issue = %issue.issue.id,
                        "running inline triage subprocess"
                    );
                    match crate::spawn::run_triage_subprocess(&issue.repo_root, &issue.issue) {
                        Ok(()) => {
                            tracing::info!(
                                worker = %issue.worker_name,
                                issue = %issue.issue.id,
                                "triage completed successfully"
                            );
                        }
                        Err(e) => {
                            result
                                .errors
                                .push(format!("triage {}: {}", issue.issue.id, e));
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    /// Process a single worker using cached PR data from the runtime.
    #[allow(clippy::type_complexity)]
    fn process_worker(
        &self,
        repo_name: &str,
        worker_name: &str,
        key: &str,
        workers_state: &mut WorkersState,
        registry: &RepoRegistry,
        runtime: &mut DaemonRuntime,
    ) -> Result<(
        usize,
        usize,
        usize,
        WorkerTickInfo,
        WorkerDisplayInfo,
        Vec<messages::PruneTarget>,
        Vec<(String, String)>,
    )> {
        // Load per-repo health config
        let repo_health = Self::load_repo_health_config(registry, repo_name);
        let effective_health = Self::effective_health_config(&repo_health, &self.config.health);
        let resolve = Self::make_nudge_resolver(&repo_health, &self.config.health);

        let event_log = EventLog::for_worker(repo_name, worker_name)?;
        let events = event_log.read_all()?;
        let new_state = WorkerState::reduce(&events, &effective_health);
        let branch_name = extract_branch_name(&events, worker_name);

        // Use cached GitHub data — request a check for next tick if needed
        let mut worker_tick_info = WorkerTickInfo::default();
        let mut is_draft = false;

        if let Some(cached) = runtime.get_cached_pr(key) {
            worker_tick_info.has_pr = cached.pr_url.is_some();
            if let Some(ref err) = cached.pr_error {
                worker_tick_info.pr_error = Some(err.clone());
            }
            worker_tick_info.pr_checks = cached.pr_checks.clone();
            is_draft = cached.is_draft;

            // If PR was discovered by the actor but we don't have it in events, emit PrOpened
            if cached.pr_url.is_some() && new_state.pr_url.is_none() {
                if let Some(ref url) = cached.pr_url {
                    let pr_number = url.rsplit('/').next().unwrap_or("0");
                    let event = Event::new(EventType::PrOpened)
                        .with_field("pr_url", url.as_str())
                        .with_field("pr_number", pr_number);
                    if let Err(e) = event_log.append(&event) {
                        tracing::warn!(worker = key, error = %e, "failed to emit PrOpened event");
                    }
                }
            }
        }

        // Request PR check for next tick if worker is active
        if !new_state.status.is_terminal() {
            runtime.request_pr_check(key, repo_name, &branch_name, new_state.pr_url.as_deref());
        }

        // Re-read state with potential PrOpened event
        let events = event_log.read_all()?;
        let mut new_state = WorkerState::reduce(&events, &effective_health);

        // Created workers (bare worktrees from `jig create`) are discovered for listing
        // but the daemon takes no actions on them.
        if new_state.status == WorkerStatus::Created {
            let display = WorkerDisplayInfo {
                repo: repo_name.to_string(),
                name: worker_name.to_string(),
                branch: branch_name,
                tmux_status: TaskStatus::NoWindow,
                worker_status: Some(new_state.status),
                nudge_count: 0,
                max_nudges: 0,
                commits_ahead: 0,
                is_dirty: false,
                pr_url: None,
                issue_ref: None,
                pr_health: WorkerTickInfo::default(),
                is_draft: false,
                nudge_cooldown_remaining: None,
            };
            return Ok((0, 0, 0, WorkerTickInfo::default(), display, vec![], vec![]));
        }

        let old_state = workers_state
            .get_worker(key)
            .map(entry_to_worker_state)
            .unwrap_or_default();

        tracing::debug!(
            worker = key,
            old_status = old_state.status.as_str(),
            new_status = new_state.status.as_str(),
            "worker state"
        );

        let mut actions = dispatch_actions(worker_name, &old_state, &new_state, &resolve);

        // Dead tmux detection: if worker is non-terminal but tmux window is gone,
        // resume instead of sending nudges to a dead window.
        // Skip Initializing workers — they're still running on-create hooks.
        if !new_state.status.is_terminal() && new_state.status != WorkerStatus::Initializing {
            let window = TmuxWindow::new(
                format!("{}{}", self.daemon_config.session_prefix, repo_name),
                worker_name,
            );
            if !window.exists() {
                tracing::info!(
                    worker = key,
                    status = new_state.status.as_str(),
                    "active worker has no tmux window, attempting resume"
                );
                // Replace nudge actions with resume attempt
                actions.retain(|a| !matches!(a, Action::Nudge { .. }));
                if let Some(entry) = Self::find_repo_path(registry, repo_name) {
                    match recovery::RecoveryScanner::try_resume_worker(
                        &entry.path,
                        repo_name,
                        worker_name,
                    ) {
                        Ok(true) => {
                            tracing::info!(worker = key, "worker resumed during steady-state tick");
                        }
                        Ok(false) => {}
                        Err(e) => {
                            tracing::warn!(
                                worker = key,
                                error = %e,
                                "failed to resume dead worker"
                            );
                        }
                    }
                }
            }
        }

        // Track review feedback count for nudge reset logic
        let mut current_review_feedback_count: Option<u32> = None;

        // Handle merged/closed PR from cached data
        if let Some(cached) = runtime.get_cached_pr(key) {
            current_review_feedback_count = cached.review_feedback_count;

            if cached.pr_merged && self.config.github.auto_cleanup_merged {
                actions.push(Action::Cleanup {
                    worker_id: worker_name.to_string(),
                });
                actions.push(Action::Notify {
                    worker_id: worker_name.to_string(),
                    message: "PR merged, worker cleaned up".to_string(),
                    kind: NotifyKind::WorkCompleted {
                        pr_url: cached.pr_url.clone(),
                    },
                });

                // Auto-complete linked issue if configured
                let auto_complete = Self::find_repo_path(registry, repo_name)
                    .and_then(|entry| JigToml::load(&entry.path).ok().flatten())
                    .map(|t| t.issues.auto_complete_on_merge)
                    .unwrap_or(false);
                if auto_complete {
                    if let Some(issue_id) = new_state.issue_ref.as_ref() {
                        actions.push(Action::UpdateIssueStatus {
                            worker_id: worker_name.to_string(),
                            issue_id: issue_id.clone(),
                        });
                    }
                }
            } else if cached.pr_closed {
                actions.push(Action::Notify {
                    worker_id: worker_name.to_string(),
                    message: "PR closed without merge".to_string(),
                    kind: NotifyKind::NeedsIntervention,
                });
                if self.config.github.auto_cleanup_closed {
                    actions.push(Action::Cleanup {
                        worker_id: worker_name.to_string(),
                    });
                }
            } else if cached.is_draft {
                // Reset review nudge count if new feedback arrived
                let stored_count = workers_state
                    .get_worker(key)
                    .and_then(|e| e.review_feedback_count);
                if let Some(current) = cached.review_feedback_count {
                    let previous = stored_count.unwrap_or(0);
                    if current > previous {
                        tracing::info!(
                            worker = key,
                            previous,
                            current,
                            "new review feedback detected, resetting review nudge count"
                        );
                        new_state.nudge_counts.remove("review");
                        if let Some(ref pr_url) = cached.pr_url {
                            actions.push(Action::Notify {
                                worker_id: worker_name.to_string(),
                                message: format!(
                                    "New review feedback on PR ({}→{} items)",
                                    previous, current
                                ),
                                kind: NotifyKind::FeedbackReceived {
                                    pr_url: pr_url.clone(),
                                },
                            });
                        }
                    }
                }

                // Draft PR — dispatch nudges from cached check results
                // Non-draft PRs are in human review, skip nudges.
                let auto_review_enabled = Self::find_repo_path(registry, repo_name)
                    .and_then(|entry| JigToml::load(&entry.path).ok().flatten())
                    .map(|t| t.review.enabled)
                    .unwrap_or(false);
                for (check_name, has_problem) in &cached.pr_checks {
                    if !has_problem {
                        continue;
                    }
                    let nudge_type = match check_name.as_str() {
                        "ci" => NudgeType::Ci,
                        "conflicts" => NudgeType::Conflict,
                        "reviews" => {
                            // When automated review is enabled and PR is draft,
                            // the review agent is the gatekeeper — suppress human
                            // comment nudges. Human feedback comes after PR exits draft.
                            if auto_review_enabled {
                                continue;
                            }
                            NudgeType::Review
                        }
                        "commits" => NudgeType::BadCommits,
                        _ => continue,
                    };
                    let resolved = resolve(nudge_type.count_key());
                    let count = new_state
                        .nudge_counts
                        .get(nudge_type.count_key())
                        .copied()
                        .unwrap_or(0);
                    if count >= resolved.max {
                        tracing::debug!(
                            worker = key,
                            nudge_type = nudge_type.count_key(),
                            count,
                            max = resolved.max,
                            "PR nudge limit reached, skipping"
                        );
                        continue;
                    }
                    // Cooldown: skip if last nudge of this type was too recent
                    if let Some(&last_ts) = new_state.last_nudge_at.get(nudge_type.count_key()) {
                        let now = chrono::Utc::now().timestamp();
                        let elapsed = now - last_ts;
                        if elapsed < resolved.cooldown_seconds as i64 {
                            tracing::debug!(
                                worker = key,
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
        }

        // Automated review trigger: fire when review is enabled, PR is draft,
        // HEAD has moved since last review, and no review is already in flight.
        {
            let review_config = Self::find_repo_path(registry, repo_name)
                .and_then(|entry| JigToml::load(&entry.path).ok().flatten())
                .map(|t| t.review)
                .unwrap_or_default();

            if review_config.enabled && is_draft && !runtime.review_pending(key) {
                let last_reviewed = workers_state
                    .get_worker(key)
                    .and_then(|e| e.last_reviewed_sha.as_deref());

                if let Some(entry) = Self::find_repo_path(registry, repo_name) {
                    let worktree_path = crate::config::worktree_path(&entry.path, worker_name);
                    let head_sha = head_sha_for(&worktree_path);

                    let needs_review = match (last_reviewed, &head_sha) {
                        (Some(prev), Some(current)) => prev != current,
                        (None, Some(_)) => true,
                        _ => false,
                    };

                    let round = review_count(&worktree_path);
                    let at_max = round >= review_config.max_rounds;

                    if needs_review && !at_max {
                        let base = RepoContext::resolve_base_branch_for(&entry.path)
                            .unwrap_or_else(|_| crate::config::DEFAULT_BASE_BRANCH.to_string());
                        runtime.send_review(messages::ReviewRequest {
                            worker_key: key.to_string(),
                            worktree_path,
                            base_branch: format!("origin/{}", base),
                        });
                    } else if needs_review && at_max {
                        actions.push(Action::Notify {
                            worker_id: worker_name.to_string(),
                            message: format!(
                                "Automated review reached max rounds ({}) without approval",
                                review_config.max_rounds
                            ),
                            kind: NotifyKind::NeedsIntervention,
                        });
                    }
                }
            }
        }

        // Resolve the repo's base branch for nudge templates
        let repo_base_branch = Self::find_repo_path(registry, repo_name)
            .and_then(|entry| RepoContext::resolve_base_branch_for(&entry.path).ok());

        let action_count = actions.len();
        let (nudge_count, notif_count, cleanup_prune_targets, tick_nudge_messages) = self
            .execute_actions(
                &actions,
                repo_name,
                worker_name,
                &branch_name,
                key,
                &new_state,
                &event_log,
                registry,
                &resolve,
                Some(runtime),
                repo_base_branch.as_deref(),
            );

        // Update workers.json
        workers_state.set_worker(
            key,
            WorkerEntry {
                repo: repo_name.to_string(),
                branch: worker_name.to_string(),
                status: new_state.status.as_str().to_string(),
                issue: new_state.issue_ref.clone(),
                pr_url: new_state.pr_url.clone(),
                started_at: new_state.started_at.unwrap_or(0),
                last_event_at: new_state.last_event_at.unwrap_or(0),
                nudge_counts: new_state.nudge_counts.clone(),
                review_feedback_count: current_review_feedback_count,
                parent_branch: new_state.parent_branch.clone(),
                last_reviewed_sha: workers_state
                    .get_worker(key)
                    .and_then(|e| e.last_reviewed_sha.clone()),
            },
        );

        // Build display info — git checks are fast local ops
        let tmux_status = self.get_tmux_status(repo_name, worker_name);
        let (commits_ahead, is_dirty) =
            if let Some(entry) = Self::find_repo_path(registry, repo_name) {
                let worktree_path = crate::config::worktree_path(&entry.path, worker_name);
                if worktree_path.exists() {
                    let base = RepoContext::resolve_base_branch_for(&entry.path)
                        .unwrap_or_else(|_| crate::config::DEFAULT_BASE_BRANCH.to_string());
                    let ahead = crate::git::Repo::open(&worktree_path)
                        .and_then(|r| r.commits_ahead(&crate::git::Branch::new(&base)))
                        .unwrap_or_default()
                        .len();
                    let dirty = crate::git::Repo::open(&worktree_path)
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

        // Compute minimum remaining cooldown across all active nudge types
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

        let display_info = WorkerDisplayInfo {
            repo: repo_name.to_string(),
            name: worker_name.to_string(),
            branch: branch_name.clone(),
            tmux_status,
            worker_status: Some(new_state.status),
            nudge_count: nudges_total,
            max_nudges: effective_health.max_nudges,
            commits_ahead,
            is_dirty,
            pr_url: new_state.pr_url.clone(),
            issue_ref: new_state.issue_ref.clone(),
            pr_health: worker_tick_info.clone(),
            is_draft,
            nudge_cooldown_remaining,
        };

        Ok((
            action_count,
            nudge_count,
            notif_count,
            worker_tick_info,
            display_info,
            cleanup_prune_targets,
            tick_nudge_messages,
        ))
    }

    /// Process a single worker with blocking I/O (legacy path for one-shot mode).
    fn process_worker_blocking(
        &self,
        repo_name: &str,
        worker_name: &str,
        key: &str,
        workers_state: &mut WorkersState,
        registry: &RepoRegistry,
    ) -> Result<(usize, usize, usize, WorkerTickInfo)> {
        // Load per-repo health config
        let repo_health = Self::load_repo_health_config(registry, repo_name);
        let effective_health = Self::effective_health_config(&repo_health, &self.config.health);
        let resolve = Self::make_nudge_resolver(&repo_health, &self.config.health);

        let event_log = EventLog::for_worker(repo_name, worker_name)?;
        let events = event_log.read_all()?;
        let mut new_state = WorkerState::reduce(&events, &effective_health);
        let branch_name = extract_branch_name(&events, worker_name);

        // Proactively discover PR if not already known
        if new_state.pr_url.is_none() && !new_state.status.is_terminal() {
            if let Some(client) = make_github_client(repo_name, registry) {
                match client.get_pr_for_branch(&branch_name) {
                    Ok(Some(pr_info)) => {
                        let event = Event::new(EventType::PrOpened)
                            .with_field("pr_url", pr_info.url.as_str())
                            .with_field("pr_number", pr_info.number.to_string());
                        if let Err(e) = event_log.append(&event) {
                            tracing::warn!(worker = key, error = %e, "failed to emit PrOpened event");
                        } else {
                            tracing::info!(worker = key, pr_url = %pr_info.url, "discovered PR for branch");
                            if let Ok(updated_events) = event_log.read_all() {
                                new_state = WorkerState::reduce(&updated_events, &effective_health);
                            }
                        }
                    }
                    Ok(None) => {
                        tracing::debug!(worker = key, branch = %branch_name, "no PR found for branch");
                    }
                    Err(e) => {
                        tracing::debug!(worker = key, error = %e, "PR discovery failed");
                    }
                }
            }
        }

        let old_state = workers_state
            .get_worker(key)
            .map(entry_to_worker_state)
            .unwrap_or_default();

        tracing::debug!(
            worker = key,
            old_status = old_state.status.as_str(),
            new_status = new_state.status.as_str(),
            "worker state"
        );

        let mut actions = dispatch_actions(worker_name, &old_state, &new_state, &resolve);

        // Check PR lifecycle
        let mut worker_tick_info = WorkerTickInfo::default();
        let mut current_review_feedback_count: Option<u32> = None;
        if !new_state.status.is_terminal() {
            if let Some(pr_url) = new_state.pr_url.clone() {
                worker_tick_info.has_pr = true;
                let stored_review_feedback_count = workers_state
                    .get_worker(key)
                    .and_then(|e| e.review_feedback_count);
                match make_github_client(repo_name, registry) {
                    Some(client) => {
                        let monitor = PrMonitor::new(&client, self.config, &resolve);
                        let pr_result = monitor.check_lifecycle(
                            worker_name,
                            &branch_name,
                            &pr_url,
                            &mut new_state,
                            stored_review_feedback_count,
                            &mut actions,
                        );
                        current_review_feedback_count = pr_result.review_feedback_count;
                        worker_tick_info.pr_checks = pr_result
                            .checks
                            .into_iter()
                            .map(|c| (c.name.to_string(), c.has_problem))
                            .collect();
                    }
                    None => {
                        worker_tick_info.pr_error = Some("GitHub client unavailable".to_string());
                    }
                }
            }
        }

        let repo_base_branch = Self::find_repo_path(registry, repo_name)
            .and_then(|entry| RepoContext::resolve_base_branch_for(&entry.path).ok());

        let action_count = actions.len();
        let (nudge_count, notif_count, _prune_targets, _nudge_messages) = self.execute_actions(
            &actions,
            repo_name,
            worker_name,
            &branch_name,
            key,
            &new_state,
            &event_log,
            registry,
            &resolve,
            None,
            repo_base_branch.as_deref(),
        );

        workers_state.set_worker(
            key,
            WorkerEntry {
                repo: repo_name.to_string(),
                branch: worker_name.to_string(),
                status: new_state.status.as_str().to_string(),
                issue: new_state.issue_ref.clone(),
                pr_url: new_state.pr_url.clone(),
                started_at: new_state.started_at.unwrap_or(0),
                last_event_at: new_state.last_event_at.unwrap_or(0),
                nudge_counts: new_state.nudge_counts.clone(),
                review_feedback_count: current_review_feedback_count,
                parent_branch: new_state.parent_branch.clone(),
                last_reviewed_sha: workers_state
                    .get_worker(key)
                    .and_then(|e| e.last_reviewed_sha.clone()),
            },
        );

        Ok((action_count, nudge_count, notif_count, worker_tick_info))
    }

    /// Execute dispatched actions, returning (nudge_count, notif_count, prune_targets).
    ///
    /// When `runtime` is `Some`, nudges are dispatched to the nudge actor
    /// (non-blocking). When `None` (legacy one-shot path), nudges are
    /// delivered synchronously via `execute_nudge`.
    #[allow(clippy::too_many_arguments)]
    fn execute_actions<F>(
        &self,
        actions: &[Action],
        repo_name: &str,
        worker_name: &str,
        branch_name: &str,
        key: &str,
        new_state: &WorkerState,
        event_log: &EventLog,
        registry: &RepoRegistry,
        resolve: &F,
        runtime: Option<&DaemonRuntime>,
        base_branch: Option<&str>,
    ) -> (
        usize,
        usize,
        Vec<messages::PruneTarget>,
        Vec<(String, String)>,
    )
    where
        F: Fn(&str) -> ResolvedNudgeConfig,
    {
        let mut nudge_count = 0;
        let mut notif_count = 0;
        let mut prune_targets = Vec::new();
        let mut nudge_messages: Vec<(String, String)> = Vec::new();

        for action in actions {
            match action {
                Action::Nudge {
                    worker_id: _,
                    nudge_type,
                } => {
                    let session_name =
                        format!("{}{}", self.daemon_config.session_prefix, repo_name);
                    let window = TmuxWindow::new(&session_name, branch_name.to_string());

                    if window.exists() {
                        let is_pr_nudge = matches!(
                            nudge_type,
                            NudgeType::Review
                                | NudgeType::Ci
                                | NudgeType::Conflict
                                | NudgeType::BadCommits
                        );
                        if !is_pr_nudge && !window.is_running() {
                            tracing::debug!(
                                worker = key,
                                "no command running in pane, skipping nudge"
                            );
                            continue;
                        }
                        let resolved = resolve(nudge_type.count_key());

                        // Render template on the tick thread (TemplateEngine has lifetime)
                        let ctx =
                            build_nudge_context(*nudge_type, new_state, resolved, base_branch);
                        let message = match self.engine.render(nudge_type.template_name(), &ctx) {
                            Ok(msg) => msg,
                            Err(e) => {
                                tracing::warn!("nudge template render failed for {}: {}", key, e);
                                continue;
                            }
                        };

                        if let Some(rt) = runtime {
                            // Async path: dispatch to nudge actor
                            nudge_messages
                                .push((nudge_type.count_key().to_string(), message.clone()));
                            rt.send_nudge(messages::NudgeRequest {
                                session: session_name,
                                window: branch_name.to_string(),
                                message,
                                nudge_type_key: nudge_type.count_key().to_string(),
                                is_stuck: *nudge_type == NudgeType::Stuck,
                                repo_name: repo_name.to_string(),
                                worker_name: worker_name.to_string(),
                                worker_key: key.to_string(),
                            });
                            nudge_count += 1;
                        } else {
                            let delivery = window.send_message(&message);

                            match delivery {
                                Ok(()) => {
                                    let event = Event::new(EventType::Nudge)
                                        .with_field("nudge_type", nudge_type.count_key())
                                        .with_field("message", message.as_str());
                                    if let Err(e) = event_log.append(&event) {
                                        tracing::warn!(
                                            "failed to append nudge event for {}: {}",
                                            key,
                                            e
                                        );
                                    }
                                    tracing::info!(
                                        worker = key,
                                        nudge_type = nudge_type.count_key(),
                                        "nudge delivered"
                                    );
                                    nudge_count += 1;
                                }
                                Err(e) => {
                                    tracing::warn!("nudge failed for {}: {}", key, e);
                                }
                            }
                        }
                    } else {
                        tracing::debug!(
                            worker = key,
                            nudge_type = nudge_type.count_key(),
                            session = %format!("{}{}", self.daemon_config.session_prefix, repo_name),
                            window = %branch_name,
                            "tmux window not found, skipping nudge"
                        );
                    }
                }
                Action::Notify {
                    worker_id,
                    message,
                    kind,
                } => {
                    tracing::info!(worker = key, message = %message, "notification sent");
                    let event = match kind {
                        NotifyKind::NeedsIntervention => NotificationEvent::NeedsIntervention {
                            repo: repo_name.to_string(),
                            worker: worker_id.clone(),
                            reason: message.clone(),
                        },
                        NotifyKind::PrOpened { pr_url } => NotificationEvent::PrOpened {
                            repo: repo_name.to_string(),
                            worker: worker_id.clone(),
                            pr_url: pr_url.clone(),
                        },
                        NotifyKind::WorkCompleted { pr_url } => NotificationEvent::WorkCompleted {
                            repo: repo_name.to_string(),
                            worker: worker_id.clone(),
                            pr_url: pr_url.clone(),
                        },
                        NotifyKind::FeedbackReceived { pr_url } => {
                            NotificationEvent::FeedbackReceived {
                                repo: repo_name.to_string(),
                                worker: worker_id.clone(),
                                pr_url: pr_url.clone(),
                            }
                        }
                        NotifyKind::ReviewApproved { pr_url } => {
                            NotificationEvent::ReviewApproved {
                                repo: repo_name.to_string(),
                                worker: worker_id.clone(),
                                pr_url: pr_url.clone(),
                            }
                        }
                    };
                    if let Err(e) = self.notifier.emit(event) {
                        tracing::warn!("notification failed for {}: {}", worker_id, e);
                    }
                    notif_count += 1;
                }
                Action::Restart { worker_id, reason } => {
                    tracing::info!(
                        worker = %worker_id,
                        reason = %reason,
                        "restart requested, attempting resume"
                    );
                    if let Some(entry) = Self::find_repo_path(registry, repo_name) {
                        match recovery::RecoveryScanner::try_resume_worker(
                            &entry.path,
                            repo_name,
                            worker_name,
                        ) {
                            Ok(true) => {
                                tracing::info!(worker = key, "worker resumed via restart action");
                            }
                            Ok(false) => {
                                tracing::debug!(
                                    worker = key,
                                    "worker still has tmux window, skip resume"
                                );
                            }
                            Err(e) => {
                                tracing::warn!(
                                    worker = key,
                                    error = %e,
                                    "failed to resume worker via restart action"
                                );
                            }
                        }
                    }
                }
                Action::Cleanup { worker_id } => {
                    let tmux_window = TmuxWindow::new(
                        format!("{}{}", self.daemon_config.session_prefix, repo_name),
                        branch_name.to_string(),
                    );

                    if tmux_window.exists() {
                        if let Err(e) = tmux_window.kill() {
                            tracing::warn!("failed to kill window for {}: {}", worker_id, e);
                        }
                    }

                    let event = Event::new(EventType::Terminal).with_field("terminal", "archived");
                    if let Err(e) = event_log.append(&event) {
                        tracing::warn!("failed to emit cleanup event for {}: {}", key, e);
                    }

                    // Queue worktree for pruning
                    if let Some(entry) = Self::find_repo_path(registry, repo_name) {
                        prune_targets.push(messages::PruneTarget {
                            repo_path: entry.path.clone(),
                            repo_name: repo_name.to_string(),
                            worker_name: worker_id.clone(),
                        });
                    }

                    tracing::info!("cleaned up worker {}", worker_id);
                }
                Action::UpdateIssueStatus {
                    worker_id,
                    issue_id,
                } => {
                    if let Some(entry) = Self::find_repo_path(registry, repo_name) {
                        match RepoContext::from_path(&entry.path) {
                            Ok(ctx) => {
                                match ctx.issue_provider() {
                                    Ok(provider) => {
                                        // Check current status — skip if already terminal
                                        let should_update = match provider.get(issue_id) {
                                            Ok(Some(issue)) => !matches!(
                                                issue.status,
                                                crate::issues::issue::IssueStatus::Complete
                                            ),
                                            Ok(None) => {
                                                tracing::warn!(
                                                    worker = %worker_id,
                                                    issue = %issue_id,
                                                    "linked issue not found, skipping auto-complete"
                                                );
                                                false
                                            }
                                            Err(e) => {
                                                tracing::warn!(
                                                    worker = %worker_id,
                                                    issue = %issue_id,
                                                    error = %e,
                                                    "failed to fetch issue status, skipping auto-complete"
                                                );
                                                false
                                            }
                                        };
                                        if should_update {
                                            match provider.update_status(
                                                issue_id,
                                                &crate::issues::issue::IssueStatus::Complete,
                                            ) {
                                                Ok(()) => {
                                                    tracing::info!(
                                                        worker = %worker_id,
                                                        issue = %issue_id,
                                                        "auto-completed linked issue after PR merge"
                                                    );
                                                }
                                                Err(e) => {
                                                    tracing::warn!(
                                                        worker = %worker_id,
                                                        issue = %issue_id,
                                                        error = %e,
                                                        "failed to auto-complete linked issue (non-fatal)"
                                                    );
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            worker = %worker_id,
                                            error = %e,
                                            "failed to create issue provider for auto-complete"
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(
                                    worker = %worker_id,
                                    error = %e,
                                    "failed to load repo context for auto-complete"
                                );
                            }
                        }
                    }
                }
            }
        }

        (nudge_count, notif_count, prune_targets, nudge_messages)
    }

    /// Auto-spawn a worker for an issue.
    ///
    /// Delegates to [`crate::spawn::spawn_worker_for_issue`] for the core spawn
    /// sequence, then emits the WorkStarted notification.
    fn auto_spawn_worker(&self, issue: &SpawnableIssue) -> Result<()> {
        use crate::spawn::{self, SpawnIssueInput};

        let input = SpawnIssueInput {
            repo_root: &issue.repo_root,
            issue: &issue.issue,
            worker_name: &issue.worker_name,
            provider_kind: issue.provider_kind,
            kind: issue.kind,
        };
        spawn::spawn_worker_for_issue(&input).map_err(crate::error::Error::Custom)?;

        // Emit WorkStarted notification
        let repo_name = issue
            .repo_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let event = NotificationEvent::WorkStarted {
            repo: repo_name,
            worker: issue.worker_name.clone(),
            issue: Some(issue.issue.id.clone()),
        };
        if let Err(e) = self.notifier.emit(event) {
            tracing::warn!(worker = %issue.worker_name, "WorkStarted notification failed: {}", e);
        }

        Ok(())
    }

    /// Fetch the configured base branch for each registered repo (blocking).
    fn sync_repos(&self, registry: &RepoRegistry) {
        for entry in registry.repos() {
            if !entry.path.exists() {
                continue;
            }
            let base = RepoContext::resolve_base_branch_for(&entry.path)
                .unwrap_or_else(|_| crate::config::DEFAULT_BASE_BRANCH.to_string());
            let (remote, branch) = base.split_once('/').unwrap_or(("origin", &base));

            match std::process::Command::new("git")
                .args(["fetch", remote, branch])
                .current_dir(&entry.path)
                .stdin(std::process::Stdio::null())
                .output()
            {
                Ok(o) if o.status.success() => {
                    tracing::debug!(repo = %entry.path.display(), "fetched {}", base);
                }
                Ok(o) => {
                    tracing::debug!(
                        repo = %entry.path.display(),
                        "fetch failed: {}",
                        String::from_utf8_lossy(&o.stderr).trim()
                    );
                }
                Err(e) => {
                    tracing::debug!(repo = %entry.path.display(), "fetch failed: {}", e);
                }
            }
        }
    }
}

/// Build a Notifier from global config.
fn make_notifier(global_config: &GlobalConfig) -> Result<Notifier> {
    let queue = crate::notify::NotificationQueue::global()?;
    Ok(Notifier::new(global_config.notify.clone(), queue))
}

/// Install SIGINT/SIGTERM handler that sets the quit flag for graceful shutdown.
fn install_signal_handler(quit: &Arc<AtomicBool>) {
    let quit_flag = Arc::clone(quit);
    if let Err(e) = ctrlc::set_handler(move || {
        tracing::info!("received shutdown signal, finishing current tick...");
        quit_flag.store(true, Ordering::Relaxed);
    }) {
        tracing::warn!("failed to install signal handler: {}", e);
    }
}

/// Run startup recovery: log lifecycle event, detect crash, resume orphans.
fn startup_recovery(global_config: &GlobalConfig) {
    let log = match lifecycle::DaemonLifecycleLog::global() {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!("failed to open daemon lifecycle log: {}", e);
            return;
        }
    };

    // Check for previous crash
    match log.previous_run_crashed() {
        Ok(true) => {
            tracing::warn!(
                "previous daemon run did not shut down cleanly — checking for orphaned workers"
            );
        }
        Ok(false) => {}
        Err(e) => {
            tracing::warn!("failed to check daemon lifecycle log: {}", e);
        }
    }

    // Log startup
    if let Err(e) = log.record_started() {
        tracing::warn!("failed to write daemon Started event: {}", e);
    }

    // Auto-recover orphaned workers if enabled
    if global_config.daemon.auto_recover {
        let registry = RepoRegistry::load().unwrap_or_default();
        let scanner = recovery::RecoveryScanner::new(&registry, &global_config.health);
        let recovered = scanner.recover_all();
        if !recovered.is_empty() {
            tracing::info!(
                count = recovered.len(),
                "recovered orphaned workers on startup"
            );
            for (repo, worker) in &recovered {
                tracing::info!(repo = %repo, worker = %worker, "recovered");
            }
        }
    }
}

/// Log a graceful shutdown event.
fn log_shutdown(reason: &str) {
    let log = match lifecycle::DaemonLifecycleLog::global() {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!("failed to open daemon lifecycle log: {}", e);
            return;
        }
    };
    if let Err(e) = log.record_stopped(reason) {
        tracing::warn!("failed to write daemon Stopped event: {}", e);
    }
}

/// Run the daemon loop with a per-tick callback and actor runtime.
///
/// The callback receives each `TickResult` and returns `true` to continue or `false` to stop.
/// The callback is responsible for any inter-tick delay (sleep, keypress polling, etc.).
///
/// A shared `quit` flag is provided so that external code (e.g. a key-polling thread)
/// can signal the tick to bail early between workers.
pub fn run_with<F>(
    daemon_config: &DaemonConfig,
    runtime_config: RuntimeConfig,
    mut on_tick: F,
) -> Result<Arc<AtomicBool>>
where
    F: FnMut(&TickResult, &Arc<AtomicBool>) -> bool,
{
    let global_config = GlobalConfig::load()?;

    // Startup: lifecycle logging + recovery
    startup_recovery(&global_config);

    let engine = TemplateEngine::new();
    let notifier = make_notifier(&global_config)?;
    let daemon = Daemon::new(&global_config, &engine, &notifier, daemon_config);

    let mut runtime = DaemonRuntime::new(runtime_config);
    let quit = Arc::new(AtomicBool::new(false));

    // Install signal handler for graceful shutdown
    install_signal_handler(&quit);

    let result = (|| -> Result<Arc<AtomicBool>> {
        loop {
            match daemon.tick(&mut runtime, &quit) {
                Ok(tick) => {
                    if tick.workers_checked > 0 || !tick.errors.is_empty() {
                        tracing::info!(
                            workers = tick.workers_checked,
                            actions = tick.actions_dispatched,
                            nudges = tick.nudges_sent,
                            notifications = tick.notifications_sent,
                            errors = tick.errors.len(),
                            "tick complete"
                        );
                    }
                    for err in &tick.errors {
                        tracing::warn!("worker error: {}", err);
                    }
                    if quit.load(Ordering::Relaxed) {
                        return Ok(quit.clone());
                    }
                    let keep_going = on_tick(&tick, &quit);
                    if daemon_config.once || !keep_going {
                        return Ok(quit.clone());
                    }
                }
                Err(e) => {
                    tracing::error!("tick failed: {}", e);
                    if daemon_config.once {
                        return Err(e);
                    }
                    if quit.load(Ordering::Relaxed) {
                        return Ok(quit.clone());
                    }
                    std::thread::sleep(Duration::from_secs(daemon_config.interval_seconds));
                }
            }
        }
    })();

    // Log shutdown with appropriate reason
    log_shutdown(if result.is_ok() { "normal" } else { "error" });
    result
}

/// Run the daemon loop (simple blocking mode). Returns after one pass if `config.once` is true.
pub fn run(daemon_config: &DaemonConfig) -> Result<()> {
    let global_config = GlobalConfig::load()?;

    // Startup: lifecycle logging + recovery
    startup_recovery(&global_config);

    let engine = TemplateEngine::new();
    let notifier = make_notifier(&global_config)?;
    let daemon = Daemon::new(&global_config, &engine, &notifier, daemon_config);

    let quit = Arc::new(AtomicBool::new(false));
    install_signal_handler(&quit);

    let result = (|| -> Result<()> {
        loop {
            if quit.load(Ordering::Relaxed) {
                return Ok(());
            }
            match daemon.tick_once() {
                Ok(tick) => {
                    if tick.workers_checked > 0 || !tick.errors.is_empty() {
                        eprintln!(
                            "[tick] {} workers, {} actions, {} nudges, {} notifications, {} errors",
                            tick.workers_checked,
                            tick.actions_dispatched,
                            tick.nudges_sent,
                            tick.notifications_sent,
                            tick.errors.len(),
                        );
                    }
                    if daemon_config.once {
                        return Ok(());
                    }
                    std::thread::sleep(Duration::from_secs(daemon_config.interval_seconds));
                }
                Err(e) => {
                    tracing::error!("tick failed: {}", e);
                    if daemon_config.once {
                        return Err(e);
                    }
                    std::thread::sleep(Duration::from_secs(daemon_config.interval_seconds));
                }
            }
        }
    })();

    log_shutdown(if result.is_ok() { "normal" } else { "error" });
    result
}

/// Convert a WorkerEntry (from workers.json) back to a WorkerState for comparison.
fn entry_to_worker_state(entry: &WorkerEntry) -> WorkerState {
    use crate::worker::WorkerStatus;

    let status = WorkerStatus::from_legacy(&entry.status);

    WorkerState {
        status,
        commit_count: 0,
        last_commit_at: None,
        pr_url: entry.pr_url.clone(),
        nudge_counts: entry.nudge_counts.clone(),
        last_nudge_at: HashMap::new(),
        issue_ref: entry.issue.clone(),
        started_at: Some(entry.started_at),
        last_event_at: Some(entry.last_event_at),
        parent_issue: None,
        parent_branch: entry.parent_branch.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn entry_to_state_roundtrip() {
        let entry = WorkerEntry {
            repo: "test".to_string(),
            branch: "main".to_string(),
            status: "running".to_string(),
            issue: Some("features/my-task".to_string()),
            pr_url: Some("https://github.com/pr/1".to_string()),
            started_at: 1000,
            last_event_at: 2000,
            nudge_counts: HashMap::new(),
            review_feedback_count: None,
            parent_branch: None,
            last_reviewed_sha: None,
        };
        let state = entry_to_worker_state(&entry);
        assert_eq!(state.status, crate::worker::WorkerStatus::Running);
        assert_eq!(state.pr_url.as_deref(), Some("https://github.com/pr/1"));
        assert_eq!(state.issue_ref.as_deref(), Some("features/my-task"));
    }

    #[test]
    fn daemon_config_defaults() {
        let config = DaemonConfig::default();
        assert_eq!(config.interval_seconds, 30);
        assert!(!config.once);
        assert_eq!(config.session_prefix, "jig-");
    }

    #[test]
    fn tick_result_defaults() {
        let result = TickResult::default();
        assert_eq!(result.workers_checked, 0);
        assert_eq!(result.actions_dispatched, 0);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn runtime_config_defaults() {
        let config = RuntimeConfig::default();
        assert_eq!(config.max_concurrent_workers, 3);
        assert_eq!(config.auto_spawn_interval, 120);
        assert_eq!(config.sync_interval, 60);
    }

    /// Helper: mirrors the review nudge reset logic from process_worker.
    fn maybe_reset_review_nudges(
        nudge_counts: &mut HashMap<String, u32>,
        stored: Option<u32>,
        current: u32,
    ) {
        let previous = stored.unwrap_or(0);
        if current > previous {
            nudge_counts.remove("review");
        }
    }

    #[test]
    fn review_nudge_count_resets_on_new_feedback() {
        let mut nudge_counts: HashMap<String, u32> = HashMap::new();
        nudge_counts.insert("review".to_string(), 3); // exhausted

        maybe_reset_review_nudges(&mut nudge_counts, Some(2), 5);

        assert_eq!(nudge_counts.get("review"), None);
    }

    #[test]
    fn review_nudge_count_unchanged_when_no_new_feedback() {
        let mut nudge_counts: HashMap<String, u32> = HashMap::new();
        nudge_counts.insert("review".to_string(), 2);

        maybe_reset_review_nudges(&mut nudge_counts, Some(3), 3);

        assert_eq!(nudge_counts.get("review"), Some(&2));
    }

    #[test]
    fn review_nudge_count_resets_from_none_stored() {
        let mut nudge_counts: HashMap<String, u32> = HashMap::new();
        nudge_counts.insert("review".to_string(), 1);

        maybe_reset_review_nudges(&mut nudge_counts, None, 2);

        assert_eq!(nudge_counts.get("review"), None);
    }

    // --- Review integration tests ---

    /// Helper: mirrors the review trigger decision logic from process_worker.
    /// Returns true if a review should be triggered.
    fn should_trigger_review(
        review_enabled: bool,
        is_draft: bool,
        is_reviewing: bool,
        last_reviewed_sha: Option<&str>,
        head_sha: Option<&str>,
        review_round: u32,
        max_rounds: u32,
    ) -> ReviewTriggerResult {
        if !review_enabled || !is_draft || is_reviewing {
            return ReviewTriggerResult::Skip;
        }

        let needs_review = match (last_reviewed_sha, head_sha) {
            (Some(prev), Some(current)) => prev != current,
            (None, Some(_)) => true,
            _ => false,
        };

        let at_max = review_round >= max_rounds;

        if needs_review && !at_max {
            ReviewTriggerResult::Trigger
        } else if needs_review && at_max {
            ReviewTriggerResult::Escalate
        } else {
            ReviewTriggerResult::Skip
        }
    }

    #[derive(Debug, PartialEq)]
    enum ReviewTriggerResult {
        Skip,
        Trigger,
        Escalate,
    }

    #[test]
    fn review_trigger_fires_on_new_commits() {
        assert_eq!(
            should_trigger_review(true, true, false, Some("aaa"), Some("bbb"), 0, 3),
            ReviewTriggerResult::Trigger,
        );
    }

    #[test]
    fn review_trigger_fires_on_first_review() {
        assert_eq!(
            should_trigger_review(true, true, false, None, Some("abc"), 0, 3),
            ReviewTriggerResult::Trigger,
        );
    }

    #[test]
    fn review_no_trigger_when_disabled() {
        assert_eq!(
            should_trigger_review(false, true, false, None, Some("abc"), 0, 3),
            ReviewTriggerResult::Skip,
        );
    }

    #[test]
    fn review_no_trigger_when_already_reviewing() {
        assert_eq!(
            should_trigger_review(true, true, true, None, Some("abc"), 0, 3),
            ReviewTriggerResult::Skip,
        );
    }

    #[test]
    fn review_no_trigger_when_head_matches_last_reviewed() {
        assert_eq!(
            should_trigger_review(true, true, false, Some("abc"), Some("abc"), 1, 3),
            ReviewTriggerResult::Skip,
        );
    }

    #[test]
    fn review_max_rounds_triggers_escalation() {
        assert_eq!(
            should_trigger_review(true, true, false, Some("aaa"), Some("bbb"), 3, 3),
            ReviewTriggerResult::Escalate,
        );
    }

    #[test]
    fn review_no_trigger_when_not_draft() {
        assert_eq!(
            should_trigger_review(true, false, false, None, Some("abc"), 0, 3),
            ReviewTriggerResult::Skip,
        );
    }

    /// Helper: mirrors the comment routing suppression logic.
    /// Returns true if the review nudge should be suppressed.
    fn should_suppress_review_nudge(review_enabled: bool, is_draft: bool) -> bool {
        review_enabled && is_draft
    }

    #[test]
    fn comment_routing_suppressed_when_review_enabled_and_draft() {
        assert!(should_suppress_review_nudge(true, true));
    }

    #[test]
    fn comment_routing_preserved_when_review_disabled() {
        assert!(!should_suppress_review_nudge(false, true));
    }

    #[test]
    fn comment_routing_preserved_when_not_draft() {
        assert!(!should_suppress_review_nudge(true, false));
    }

    #[test]
    fn last_reviewed_sha_updates_after_review() {
        let mut state = WorkersState::default();
        state.set_worker(
            "repo/worker",
            WorkerEntry {
                repo: "repo".to_string(),
                branch: "worker".to_string(),
                status: "running".to_string(),
                issue: None,
                pr_url: None,
                started_at: 1000,
                last_event_at: 2000,
                nudge_counts: HashMap::new(),
                review_feedback_count: None,
                parent_branch: None,
                last_reviewed_sha: None,
            },
        );

        // Simulate review completion: update last_reviewed_sha
        if let Some(entry) = state.workers.get_mut("repo/worker") {
            entry.last_reviewed_sha = Some("abc123".to_string());
        }

        let entry = state.get_worker("repo/worker").unwrap();
        assert_eq!(entry.last_reviewed_sha.as_deref(), Some("abc123"));
    }

    /// Verify that approve verdict maps to the correct action type.
    #[test]
    fn approve_verdict_produces_review_approved_notify() {
        use crate::review::ReviewVerdict;

        let verdict = ReviewVerdict::Approve;
        let kind = match verdict {
            ReviewVerdict::Approve => NotifyKind::ReviewApproved { pr_url: None },
            ReviewVerdict::ChangesRequested => NotifyKind::NeedsIntervention,
        };
        assert!(matches!(kind, NotifyKind::ReviewApproved { .. }));
    }

    /// Verify that changes_requested verdict maps to AutoReview nudge.
    #[test]
    fn changes_requested_verdict_produces_auto_review_nudge() {
        use crate::review::ReviewVerdict;

        let verdict = ReviewVerdict::ChangesRequested;
        let nudge_type = match verdict {
            ReviewVerdict::ChangesRequested => Some(NudgeType::AutoReview),
            ReviewVerdict::Approve => None,
        };
        assert_eq!(nudge_type, Some(NudgeType::AutoReview));
    }

    fn make_worker_entry(issue: Option<&str>, status: &str) -> WorkerEntry {
        WorkerEntry {
            repo: "test".to_string(),
            branch: "main".to_string(),
            status: status.to_string(),
            issue: issue.map(|s| s.to_string()),
            pr_url: None,
            started_at: 1000,
            last_event_at: 2000,
            nudge_counts: HashMap::new(),
            review_feedback_count: None,
            parent_branch: None,
            last_reviewed_sha: None,
        }
    }

    #[test]
    fn has_active_parent_worker_finds_running_worker() {
        let mut state = WorkersState::default();
        state.set_worker(
            "test/parent-worker",
            make_worker_entry(Some("ENG-100"), "running"),
        );

        assert!(Daemon::has_active_parent_worker(&state, "ENG-100"));
    }

    #[test]
    fn has_active_parent_worker_ignores_terminal_statuses() {
        for status in &["merged", "archived", "failed"] {
            let mut state = WorkersState::default();
            state.set_worker(
                "test/parent-worker",
                make_worker_entry(Some("ENG-100"), status),
            );

            assert!(
                !Daemon::has_active_parent_worker(&state, "ENG-100"),
                "status '{}' should be considered terminal",
                status
            );
        }
    }

    #[test]
    fn has_active_parent_worker_no_match() {
        let mut state = WorkersState::default();
        state.set_worker(
            "test/other-worker",
            make_worker_entry(Some("ENG-200"), "running"),
        );

        assert!(!Daemon::has_active_parent_worker(&state, "ENG-100"));
    }

    #[test]
    fn has_active_parent_worker_no_issue_field() {
        let mut state = WorkersState::default();
        state.set_worker("test/worker", make_worker_entry(None, "running"));

        assert!(!Daemon::has_active_parent_worker(&state, "ENG-100"));
    }

    // --- Auto-complete on merge tests ---

    /// Helper: mirrors the auto-complete decision logic from process_worker.
    /// Returns `Some(issue_id)` when an UpdateIssueStatus action should be pushed.
    fn should_auto_complete(
        auto_complete_on_merge: bool,
        issue_ref: Option<&str>,
    ) -> Option<String> {
        if auto_complete_on_merge {
            issue_ref.map(|id| id.to_string())
        } else {
            None
        }
    }

    #[test]
    fn auto_complete_pushes_when_enabled_and_has_issue() {
        let result = should_auto_complete(true, Some("ENG-42"));
        assert_eq!(result, Some("ENG-42".to_string()));
    }

    #[test]
    fn auto_complete_skips_when_disabled() {
        let result = should_auto_complete(false, Some("ENG-42"));
        assert_eq!(result, None);
    }

    #[test]
    fn auto_complete_skips_when_no_issue() {
        let result = should_auto_complete(true, None);
        assert_eq!(result, None);
    }

    /// Helper: mirrors the status check in the UpdateIssueStatus handler.
    /// Returns true if the issue should be updated to Complete.
    fn should_update_issue_status(current_status: crate::issues::issue::IssueStatus) -> bool {
        !matches!(current_status, crate::issues::issue::IssueStatus::Complete)
    }

    #[test]
    fn auto_complete_updates_in_progress_issue() {
        assert!(should_update_issue_status(
            crate::issues::issue::IssueStatus::InProgress
        ));
    }

    #[test]
    fn auto_complete_skips_already_complete_issue() {
        assert!(!should_update_issue_status(
            crate::issues::issue::IssueStatus::Complete
        ));
    }

    #[test]
    fn auto_complete_updates_planned_issue() {
        assert!(should_update_issue_status(
            crate::issues::issue::IssueStatus::Planned
        ));
    }

    #[test]
    fn auto_complete_action_variant_is_correct() {
        let action = Action::UpdateIssueStatus {
            worker_id: "test-worker".to_string(),
            issue_id: "ENG-42".to_string(),
        };
        assert!(matches!(
            action,
            Action::UpdateIssueStatus {
                worker_id,
                issue_id,
            } if worker_id == "test-worker" && issue_id == "ENG-42"
        ));
    }
}
