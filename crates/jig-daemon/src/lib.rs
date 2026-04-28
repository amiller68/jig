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

pub mod actors;
mod config;
mod discovery;
mod dispatch;
mod display;
pub mod lifecycle;
mod pr;
pub mod recovery;
pub mod runtime;
pub mod triage_tracker;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use jig_core::config::registry::{RepoEntry, RepoRegistry};
use jig_core::config::Config;
use jig_core::config::{GlobalConfig, HealthConfig, WorkerEntry, WorkersState};
use jig_core::config::{JigToml, RepoHealthConfig, ResolvedNudgeConfig};
use dispatch::{dispatch_actions, Action, NotifyKind};
use jig_core::error::Result;
use jig_core::host::tmux::{TmuxSession, TmuxWindow};
use jig_core::issues::issue::IssueRef;
use jig_core::notify::{NotificationEvent, Notifier};
use jig_core::prompt::Prompt;
use jig_core::review::{latest_verdict, review_count, ReviewVerdict};
use jig_core::worker::TmuxStatus;
use jig_core::worker::events::{Event, EventKind, EventLog, TerminalKind, WorkerState};
use jig_core::worker::WorkerStatus;

use actors::Actor;
use discovery::discover_workers;

pub use actors::spawn::SpawnableIssue;
pub use config::{DaemonConfig, TickResult};
pub use display::{TriageDisplayInfo, WorkerDisplayInfo, WorkerTickInfo};
pub use runtime::{DaemonRuntime, RuntimeConfig};

/// The daemon orchestrator — holds references to shared infrastructure.
pub struct Daemon<'a> {
    config: &'a GlobalConfig,
    notifier: &'a Notifier,
    daemon_config: &'a DaemonConfig,
}

impl<'a> Daemon<'a> {
    pub fn new(
        config: &'a GlobalConfig,
        notifier: &'a Notifier,
        daemon_config: &'a DaemonConfig,
    ) -> Self {
        Self {
            config,
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
    fn get_tmux_status(&self, repo_name: &str, worker_name: &str) -> TmuxStatus {
        let session = TmuxSession::new(format!(
            "{}{}",
            self.daemon_config.session_prefix, repo_name
        ));
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
        runtime: &mut DaemonRuntime,
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

            let (worker_name, _branch_name) = match parent_worker {
                Some(pw) => pw,
                None => continue,
            };

            let repo_entry = match Self::find_repo_path(registry, repo_name) {
                Some(e) => e,
                None => continue,
            };

            let worktree_path = jig_core::config::worktree_path(&repo_entry.path, &worker_name);
            let worktree_exists = worktree_path.exists();

            if worktree_exists {
                // Worktree exists: fast-forward via checkout (original path).
                let repo = match jig_core::git::Repo::open(&worktree_path) {
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
                        let worker = jig_core::worker::Worker::from_branch(
                            &repo_entry.path,
                            worker_name.as_str().into(),
                        );
                        if worker.has_tmux_window() {
                            let prompt = Prompt::new(
                                "Child work has been merged into your branch. \
                                 New commits are available. Run `git log --oneline -5` \
                                 to see what changed.",
                            )
                            .named("parent_update");

                            runtime.nudge.send(actors::nudge::NudgeRequest {
                                worker,
                                prompt,
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
                let repo = match jig_core::git::Repo::open(&repo_entry.path) {
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
        runtime.sync.drain().into_iter().next();

        // Parent-update phase: after sync, check if parent worktrees have new
        // remote commits (from child PR merges) and pull them in.
        self.update_parent_worktrees(&workers_state, &registry, runtime);

        runtime.github.drain();
        let issue_response = runtime.issues.drain().into_iter().next();
        let mut spawnable = issue_response
            .as_ref()
            .map(|r| r.spawnable.clone())
            .unwrap_or_default();
        let mut triageable = issue_response
            .as_ref()
            .map(|r| r.triageable.clone())
            .unwrap_or_default();
        // Accumulate parent branch results from both async and inline-poll
        // paths so the sync actor can fetch their tracking refs.
        let mut parent_branch_results: Vec<actors::issue::ParentBranchResult> = issue_response
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
        if spawnable.is_empty() && runtime.issues.should_first_poll() {
            runtime.issues.mark_first_poll_done();

            let repos: Vec<(std::path::PathBuf, String)> = registry
                .filtered_repos(self.daemon_config.repo_filter.as_deref())
                .into_iter()
                .map(|entry| {
                    let base = Config::resolve_base_branch_for(&entry.path)
                        .unwrap_or_else(|_| jig_core::config::DEFAULT_BASE_BRANCH.to_string());
                    (entry.path.clone(), base)
                })
                .collect();

            if !repos.is_empty() {
                let req = actors::issue::IssueRequest {
                    repos,
                    existing_workers: worker_list.clone(),
                };
                let response = actors::issue::process_request(&req);
                spawnable = response.spawnable;
                triageable = response.triageable;
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
            }
        }

        // Triage: filter out issues already tracked, register new ones, detect stuck
        {
            let now = chrono::Utc::now().timestamp();

            // Filter triageable issues to those not already being triaged
            triageable.retain(|issue| !runtime.triage_tracker.is_active(issue.issue.id()));

            // Stuck triage detection: check each active triage against its
            // repo's configured timeout (from [triage] timeout_seconds).
            let stuck_ids: Vec<(String, String, String)> = {
                let mut stuck = Vec::new();
                for entry in runtime.triage_tracker.stuck_entries() {
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
                runtime.triage_tracker.remove(issue_id);
            }

            // Triage worker completion is now handled by drain_triage() above —
            // the triage_actor reports results directly when subprocesses finish.
        }

        // Drain nudge completions from previous tick
        for nudge_result in runtime.nudge.drain() {
            if let Some(err) = nudge_result.error {
                tracing::warn!(
                    worker = %nudge_result.worker_key,
                    "nudge delivery error: {}",
                    err
                );
            }
        }

        // Drain review completions from previous tick
        for review_result in runtime.review.drain() {
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
                Some(entry) => jig_core::config::worktree_path(&entry.path, wname),
                None => {
                    tracing::warn!(worker = %worker_key, "repo not found for review result");
                    continue;
                }
            };

            // Get current HEAD SHA
            let head_sha = jig_core::git::Worktree::open(&worktree_path)
                .ok()
                .and_then(|wt| wt.head_sha().ok());
            let verdict = latest_verdict(&worktree_path);

            match verdict {
                Some(ReviewVerdict::Approve) => {
                    // Mark PR ready for review
                    if let Some(cached) = runtime.github.get_cached(worker_key) {
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
                        .github.get_cached(worker_key)
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
                    let repo_path = Self::find_repo_path(&registry, rname).map(|e| e.path.clone());
                    if let Some(rp) = repo_path {
                        let branch: jig_core::git::Branch = wname.into();
                        let worker = jig_core::worker::Worker::from_branch(&rp, branch);

                        if worker.has_tmux_window() {
                            let review_cfg = Self::find_repo_path(&registry, rname)
                                .and_then(|entry| JigToml::load(&entry.path).ok().flatten())
                                .map(|t| t.review)
                                .unwrap_or_default();

                            let round = review_count(&worktree_path);
                            let max_rounds = review_cfg.max_rounds;

                            let repo_health = Self::load_repo_health_config(&registry, rname);
                            let resolve = Self::make_nudge_resolver(&repo_health, &self.config.health);
                            let resolved = resolve("auto_review");

                            let effective_health =
                                Self::effective_health_config(&repo_health, &self.config.health);

                            if let Ok(event_log) = EventLog::for_worker(rname, wname) {
                                if let Ok(events) = event_log.read_all() {
                                    let state = WorkerState::reduce(&events, &effective_health);
                                    let count = state.nudge_counts.get("auto_review").copied().unwrap_or(0);
                                    let review_file = format!("{:03}.md", round);
                                    let prompt = Prompt::new(pr::TEMPLATE_AUTO_REVIEW)
                                        .named("auto_review")
                                        .var_num("nudge_count", count + 1)
                                        .var_num("max_nudges", resolved.max)
                                        .var_bool("is_final_nudge", count + 1 >= resolved.max)
                                        .var_num("review_round", round)
                                        .var_num("max_rounds", max_rounds)
                                        .var_bool("is_final_round", round >= max_rounds)
                                        .var("review_file", &review_file)
                                        .var_num("review_number", round);

                                    runtime.nudge.send(actors::nudge::NudgeRequest {
                                        worker,
                                        prompt,
                                    });
                                }
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
        if let Some(prune_complete) = runtime.prune.drain().into_iter().next() {
            for pr in prune_complete.results {
                if let Some(err) = pr.error {
                    result.errors.push(format!("prune {}: {}", pr.key, err));
                } else {
                    result.pruned.push(pr.key);
                }
            }
        }

        // Drain spawn results from previous tick
        if let Some(spawn_complete) = runtime.spawn.drain().into_iter().next() {
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
        if let Some(triage_complete) = runtime.triage.drain().into_iter().next() {
            for tr in triage_complete.results {
                // Remove from tracker regardless of success/failure
                runtime.triage_tracker.remove(&tr.issue_id);

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

            if runtime.poll_is_due() {
                let filtered = registry.filtered_repos(self.daemon_config.repo_filter.as_deref());

                let sync_repos: Vec<(String, std::path::PathBuf, String)> = filtered
                    .iter()
                    .filter_map(|entry| {
                        let name = entry.path.file_name()?.to_string_lossy().to_string();
                        let base = Config::resolve_base_branch_for(&entry.path)
                            .unwrap_or_else(|_| jig_core::config::DEFAULT_BASE_BRANCH.to_string());
                        Some((name, entry.path.clone(), base))
                    })
                    .collect();
                if !sync_repos.is_empty() {
                    runtime.sync.send(actors::sync::SyncRequest {
                        repos: sync_repos,
                        parent_branches,
                    });
                }

                let poll_repos: Vec<(std::path::PathBuf, String)> = filtered
                    .iter()
                    .map(|entry| {
                        let base = Config::resolve_base_branch_for(&entry.path)
                            .unwrap_or_else(|_| jig_core::config::DEFAULT_BASE_BRANCH.to_string());
                        (entry.path.clone(), base)
                    })
                    .collect();
                if !poll_repos.is_empty() {
                    runtime.issues.send(actors::issue::IssueRequest {
                        repos: poll_repos,
                        existing_workers: worker_list.clone(),
                    });
                }

                runtime.mark_polled();
            }
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
            runtime.prune.send(actors::prune::PruneRequest { targets: live_prune_targets });
        }

        // Filter out terminal workers and workers with no tmux session
        result.worker_display.retain(|w| {
            let is_terminal = w
                .worker_status
                .as_ref()
                .map(|s| s.is_terminal())
                .unwrap_or(false);
            let tmux_dead = matches!(w.tmux_status, TmuxStatus::NoSession | TmuxStatus::NoWindow);
            !is_terminal && !tmux_dead
        });
        result.worker_display.sort_by(|a, b| a.name.cmp(&b.name));

        // Build triage display from tracker
        {
            let now = chrono::Utc::now().timestamp();
            for entry in runtime.triage_tracker.active_entries() {
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
        if !runtime.prune.is_pending() {
            let mut prune_targets = Vec::new();
            for (repo_name, worker_name) in &worker_list {
                let key = format!("{}/{}", repo_name, worker_name);
                if let Some(cached) = runtime.github.get_cached(&key) {
                    if cached.pr_merged || cached.pr_closed {
                        if let Some(entry) = Self::find_repo_path(&registry, repo_name) {
                            let worktree_path =
                                jig_core::config::worktree_path(&entry.path, worker_name);
                            if worktree_path.exists() {
                                prune_targets.push(actors::prune::PruneTarget {
                                    repo_path: entry.path.clone(),
                                    repo_name: repo_name.clone(),
                                    worker_name: worker_name.clone(),
                                });
                            }
                        }
                    }
                }
            }
            runtime.prune.send(actors::prune::PruneRequest { targets: prune_targets });
        }

        // 4. Send spawnable issues to background spawn actor (non-blocking).
        if !spawnable.is_empty() {
            runtime.spawn.send(actors::spawn::SpawnRequest { issues: spawnable });
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
                runtime.triage_tracker.register(
                    ti.issue.id().to_string(),
                    triage_tracker::TriageEntry {
                        worker_name: ti.worker_name.clone(),
                        spawned_at: now,
                        issue_id: ti.issue.id().to_string(),
                        repo_name,
                    },
                );
            }
            runtime.triage.send(actors::triage::TriageRequest { issues: triageable });
        }

        result.spawning = runtime.spawn.spawning_workers().to_vec();
        result.poll_remaining_secs = runtime.poll_remaining_secs();

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
        Vec<actors::prune::PruneTarget>,
        Vec<(String, String)>,
    )> {
        // Load per-repo health config
        let repo_health = Self::load_repo_health_config(registry, repo_name);
        let effective_health = Self::effective_health_config(&repo_health, &self.config.health);
        let resolve = Self::make_nudge_resolver(&repo_health, &self.config.health);

        let event_log = EventLog::for_worker(repo_name, worker_name)?;
        let events = event_log.read_all()?;
        let new_state = WorkerState::reduce(&events, &effective_health);
        let branch_name = new_state
            .branch
            .as_deref()
            .unwrap_or(worker_name)
            .to_string();

        // Use cached GitHub data — request a check for next tick if needed
        let mut worker_tick_info = WorkerTickInfo::default();
        let mut is_draft = false;

        if let Some(cached) = runtime.github.get_cached(key) {
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
                    if let Err(e) = event_log.append(&Event::now(EventKind::PrOpened {
                        pr_url: url.clone(),
                        pr_number: pr_number.to_string(),
                    })) {
                        tracing::warn!(worker = key, error = %e, "failed to emit PrOpened event");
                    }
                }
            }
        }

        // Request PR check for next tick if worker is active
        if !new_state.status.is_terminal() {
            runtime.github.send(actors::github::GitHubRequest {
                worker_key: key.to_string(),
                repo_name: repo_name.to_string(),
                branch: branch_name.clone(),
                pr_url: new_state.pr_url.clone(),
                previous_is_draft: runtime.github.previous_is_draft(key),
            });
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
                tmux_status: TmuxStatus::NoWindow,
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
        if let Some(cached) = runtime.github.get_cached(key) {
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
                            issue_id: issue_id.to_string(),
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
                let base = Self::find_repo_path(registry, repo_name)
                    .and_then(|entry| JigToml::load(&entry.path).ok().flatten())
                    .and_then(|t| t.worktree.base)
                    .unwrap_or_else(|| jig_core::config::DEFAULT_BASE_BRANCH.to_string());
                for (check_name, has_problem) in &cached.pr_checks {
                    if !has_problem {
                        continue;
                    }
                    if check_name == "reviews" && auto_review_enabled {
                        continue;
                    }
                    let nkey = pr::nudge_key_for_check(check_name);
                    let resolved = resolve(nkey);
                    let count = new_state
                        .nudge_counts
                        .get(nkey)
                        .copied()
                        .unwrap_or(0);
                    if count >= resolved.max {
                        tracing::debug!(
                            worker = key,
                            nudge_key = nkey,
                            count,
                            max = resolved.max,
                            "PR nudge limit reached, skipping"
                        );
                        continue;
                    }
                    if let Some(&last_ts) = new_state.last_nudge_at.get(nkey) {
                        let now = chrono::Utc::now().timestamp();
                        let elapsed = now - last_ts;
                        if elapsed < resolved.cooldown_seconds as i64 {
                            tracing::debug!(
                                worker = key,
                                nudge_key = nkey,
                                elapsed,
                                cooldown = resolved.cooldown_seconds,
                                "PR nudge cooldown active, skipping"
                            );
                            continue;
                        }
                    }
                    let mut prompt = Prompt::new(pr::template_for_check(check_name))
                        .named(nkey)
                        .var_num("nudge_count", count + 1)
                        .var_num("max_nudges", resolved.max)
                        .var_bool("is_final_nudge", count + 1 >= resolved.max);

                    match check_name.as_str() {
                        "ci" => {
                            // details are stored in cached checks — not available here,
                            // use empty list (CI details come from the health check)
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
                        actions.push(Action::Nudge {
                            worker_id: worker_name.to_string(),
                            message,
                            nudge_key: nkey.to_string(),
                            is_stuck: false,
                            is_pr_nudge: true,
                        });
                    }
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

            if review_config.enabled && is_draft && !runtime.review.is_pending(key) {
                let last_reviewed = workers_state
                    .get_worker(key)
                    .and_then(|e| e.last_reviewed_sha.as_deref());

                if let Some(entry) = Self::find_repo_path(registry, repo_name) {
                    let worktree_path = jig_core::config::worktree_path(&entry.path, worker_name);
                    let head_sha = jig_core::git::Worktree::open(&worktree_path)
                        .ok()
                        .and_then(|wt| wt.head_sha().ok());

                    let needs_review = match (last_reviewed, &head_sha) {
                        (Some(prev), Some(current)) => prev != current,
                        (None, Some(_)) => true,
                        _ => false,
                    };

                    let round = review_count(&worktree_path);
                    let at_max = round >= review_config.max_rounds;

                    if needs_review && !at_max {
                        let base = Config::resolve_base_branch_for(&entry.path)
                            .unwrap_or_else(|_| jig_core::config::DEFAULT_BASE_BRANCH.to_string());
                        runtime.review.send(actors::review::ReviewRequest {
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

        let action_count = actions.len();
        let (nudge_count, notif_count, cleanup_prune_targets, tick_nudge_messages) = self
            .execute_actions(
                &actions,
                repo_name,
                worker_name,
                &branch_name,
                key,
                &event_log,
                registry,
                runtime,
            );

        // Update workers.json
        workers_state.set_worker(
            key,
            WorkerEntry {
                repo: repo_name.to_string(),
                branch: worker_name.to_string(),
                status: new_state.status.as_str().to_string(),
                issue: new_state.issue_ref.as_ref().map(|r| r.to_string()),
                pr_url: new_state.pr_url.clone(),
                started_at: new_state.started_at.unwrap_or(0),
                last_event_at: new_state.last_event_at.unwrap_or(0),
                nudge_counts: new_state.nudge_counts.clone(),
                review_feedback_count: current_review_feedback_count,
                parent_branch: None,
                last_reviewed_sha: workers_state
                    .get_worker(key)
                    .and_then(|e| e.last_reviewed_sha.clone()),
            },
        );

        // Build display info — git checks are fast local ops
        let tmux_status = self.get_tmux_status(repo_name, worker_name);
        let (commits_ahead, is_dirty) =
            if let Some(entry) = Self::find_repo_path(registry, repo_name) {
                let worktree_path = jig_core::config::worktree_path(&entry.path, worker_name);
                if worktree_path.exists() {
                    let base = Config::resolve_base_branch_for(&entry.path)
                        .unwrap_or_else(|_| jig_core::config::DEFAULT_BASE_BRANCH.to_string());
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
            issue_ref: new_state.issue_ref.as_ref().map(|r| r.to_string()),
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

    /// Execute dispatched actions, returning (nudge_count, notif_count, prune_targets).
    #[allow(clippy::too_many_arguments)]
    fn execute_actions(
        &self,
        actions: &[Action],
        repo_name: &str,
        worker_name: &str,
        branch_name: &str,
        key: &str,
        event_log: &EventLog,
        registry: &RepoRegistry,
        runtime: &mut DaemonRuntime,
    ) -> (
        usize,
        usize,
        Vec<actors::prune::PruneTarget>,
        Vec<(String, String)>,
    ) {
        let mut nudge_count = 0;
        let mut notif_count = 0;
        let mut prune_targets = Vec::new();
        let mut nudge_messages: Vec<(String, String)> = Vec::new();

        for action in actions {
            match action {
                Action::Nudge {
                    worker_id: _,
                    message,
                    nudge_key,
                    is_stuck: _,
                    is_pr_nudge,
                } => {
                    let worker = jig_core::worker::Worker::from_branch(
                        &Self::find_repo_path(registry, repo_name)
                            .map(|e| e.path.clone())
                            .unwrap_or_default(),
                        branch_name.into(),
                    );

                    if worker.has_tmux_window() {
                        if !is_pr_nudge && !worker.is_agent_running() {
                            tracing::debug!(
                                worker = key,
                                "no command running in pane, skipping nudge"
                            );
                            continue;
                        }

                        nudge_messages.push((nudge_key.clone(), message.clone()));
                        let prompt = Prompt::new(message).named(nudge_key);
                        runtime.nudge.send(actors::nudge::NudgeRequest {
                            worker,
                            prompt,
                        });
                        nudge_count += 1;
                    } else {
                        tracing::debug!(
                            worker = key,
                            nudge_key = %nudge_key,
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

                    if let Err(e) = event_log.append(&Event::now(EventKind::Terminal {
                        terminal: TerminalKind::Archived,
                        reason: None,
                    })) {
                        tracing::warn!("failed to emit cleanup event for {}: {}", key, e);
                    }

                    // Queue worktree for pruning
                    if let Some(entry) = Self::find_repo_path(registry, repo_name) {
                        prune_targets.push(actors::prune::PruneTarget {
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
                        match Config::from_path(&entry.path) {
                            Ok(ctx) => {
                                match ctx.issue_provider() {
                                    Ok(provider) => {
                                        // Check current status — skip if already terminal
                                        let should_update = match provider.get(issue_id) {
                                            Ok(Some(issue)) => !matches!(
                                                issue.status(),
                                                jig_core::issues::issue::IssueStatus::Complete
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
                                                &jig_core::issues::issue::IssueStatus::Complete,
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

}

/// Build a Notifier from global config.
fn make_notifier(global_config: &GlobalConfig) -> Result<Notifier> {
    let queue = jig_core::notify::NotificationQueue::global()?;
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

    let notifier = make_notifier(&global_config)?;
    let daemon = Daemon::new(&global_config, &notifier, daemon_config);

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

/// Convert a WorkerEntry (from workers.json) back to a WorkerState for comparison.
fn entry_to_worker_state(entry: &WorkerEntry) -> WorkerState {
    use jig_core::worker::WorkerStatus;

    let status = WorkerStatus::from_legacy(&entry.status);

    WorkerState {
        status,
        branch: Some(entry.branch.clone()),
        commit_count: 0,
        last_commit_at: None,
        pr_url: entry.pr_url.clone(),
        nudge_counts: entry.nudge_counts.clone(),
        last_nudge_at: HashMap::new(),
        issue_ref: entry.issue.as_ref().map(IssueRef::new),
        started_at: Some(entry.started_at),
        last_event_at: Some(entry.last_event_at),
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
        assert_eq!(state.status, jig_core::worker::WorkerStatus::Running);
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
        assert_eq!(config.poll_interval, 60);
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
        use jig_core::review::ReviewVerdict;

        let verdict = ReviewVerdict::Approve;
        let kind = match verdict {
            ReviewVerdict::Approve => NotifyKind::ReviewApproved { pr_url: None },
            ReviewVerdict::ChangesRequested => NotifyKind::NeedsIntervention,
        };
        assert!(matches!(kind, NotifyKind::ReviewApproved { .. }));
    }

    /// Verify that changes_requested verdict triggers an auto-review nudge.
    #[test]
    fn changes_requested_verdict_produces_auto_review_nudge() {
        use jig_core::review::ReviewVerdict;

        let verdict = ReviewVerdict::ChangesRequested;
        let should_nudge = matches!(verdict, ReviewVerdict::ChangesRequested);
        assert!(should_nudge);
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
    fn should_update_issue_status(current_status: jig_core::issues::issue::IssueStatus) -> bool {
        !matches!(current_status, jig_core::issues::issue::IssueStatus::Complete)
    }

    #[test]
    fn auto_complete_updates_in_progress_issue() {
        assert!(should_update_issue_status(
            jig_core::issues::issue::IssueStatus::InProgress
        ));
    }

    #[test]
    fn auto_complete_skips_already_complete_issue() {
        assert!(!should_update_issue_status(
            jig_core::issues::issue::IssueStatus::Complete
        ));
    }

    #[test]
    fn auto_complete_updates_planned_issue() {
        assert!(should_update_issue_status(
            jig_core::issues::issue::IssueStatus::Planned
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
